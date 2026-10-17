//! 选区复制及原子粘贴覆盖关联、轨道、完整参数和时间边界。
use dinuosi_core::{
    media::MediaKind,
    text::Text,
    timeline::{Property, Timeline},
};
use std::collections::BTreeSet;

/// 创建有音画关联、裁剪源入点和动画的选区，重复粘贴不能连接原组或丢失参数。
#[test]
fn clipboard_preserves_parameters_links_offsets_and_collision_rows() {
    let mut t = Timeline::default();
    let a = t
        .insert(
            ("video.mp4".into(), MediaKind::Video),
            "画面".into(),
            0,
            60,
            15..105,
            300,
        )
        .unwrap();
    let b = t
        .insert(
            ("audio.wav".into(), MediaKind::Audio),
            "声音".into(),
            0,
            75,
            30..120,
            300,
        )
        .unwrap();
    let ids = [a.clone(), b.clone()].into();
    t.link_clips(&ids, false);
    t.set_property(&ids, Property::Scale, 150.0).unwrap();
    t.toggle_keyframe(&ids, dinuosi_core::keyframes::KeyframeKind::Scale, 60);
    let original = t.clone();
    let copy = t.copy_clips(&ids).unwrap();
    for _ in 0..2 {
        let pasted = t.paste_clips(&copy, 60).unwrap();
        assert_eq!(pasted.len(), 2);
        let clips: Vec<_> = t.clips.iter().filter(|c| pasted.contains(&c.id)).collect();
        assert_eq!(clips[1].start - clips[0].start, 15);
        assert_eq!(clips[0].link, clips[1].link);
        assert_ne!(clips[0].link, original.clips[0].link);
        assert_eq!(clips[0].source, original.clips[0].source);
        assert_eq!(clips[0].source_in, 15);
        assert_eq!(clips[0].transform.scale, 150.0);
        assert_eq!(clips[0].duration, 90);
        assert_eq!(clips[0].keyframes, original.clips[0].keyframes);
        assert_ne!(clips[0].track, clips[1].track);
        t.validate().unwrap();
    }
    for c in &original.clips {
        let saved = t.clips.iter().find(|v| v.id == c.id).unwrap();
        assert_eq!(saved.start, c.start);
        assert_eq!(saved.source_in, c.source_in);
    }
    let single = original.copy_clips(&[a].into()).unwrap();
    assert!(single.clips[0].link.is_none());
    assert!(original.copy_clips(&BTreeSet::new()).is_none());
}

/// 两个同类源轨道在整组冲突时仍各占独立行，组内同轨片段保留原有间隙。
#[test]
fn clipboard_keeps_multirow_layout_and_same_row_gaps() {
    let mut t = Timeline::default();
    let a = t
        .insert(
            ("a.png".into(), MediaKind::Image),
            "甲".into(),
            0,
            0,
            0..30,
            30,
        )
        .unwrap();
    let b = t
        .insert(
            ("b.png".into(), MediaKind::Image),
            "乙".into(),
            0,
            60,
            0..30,
            30,
        )
        .unwrap();
    let c = t
        .insert(
            ("c.png".into(), MediaKind::Image),
            "丙".into(),
            0,
            0,
            0..30,
            30,
        )
        .unwrap();
    let copy = t.copy_clips(&[a, b, c].into()).unwrap();
    let ids = t.paste_clips(&copy, 0).unwrap();
    let clips: Vec<_> = t.clips.iter().filter(|c| ids.contains(&c.id)).collect();
    assert_eq!(clips[0].track, clips[1].track);
    assert_ne!(clips[0].track, clips[2].track);
    assert!(clips[2].track < clips[0].track);
    assert_eq!(clips[1].start - clips[0].start, 60);
    let mut empty = Timeline::default();
    let pasted = empty.paste_clips(&copy, 0).unwrap();
    let rows: Vec<_> = empty
        .clips
        .iter()
        .filter(|c| pasted.contains(&c.id))
        .collect();
    assert_eq!(rows[0].track, rows[1].track);
    assert!(rows[2].track < rows[0].track);
}

/// 失败保持目标完全不变；删除空轨后重建职责，跨帧率保留秒数，字幕粘贴建立独立样式组。
#[test]
fn clipboard_handles_locks_day_limit_missing_tracks_and_subtitles() {
    let mut source = Timeline::default();
    let id = source
        .add_text(30, 90, Text::new("字幕".into(), (1920, 1080), true))
        .unwrap();
    let copy = source.copy_clips(&[id].into()).unwrap();
    let mut target = Timeline {
        fps: 60,
        ..Default::default()
    };
    let ids = target.paste_clips(&copy, 120).unwrap();
    assert_eq!(ids.len(), 1);
    assert_eq!(target.clips[0].start, 120);
    assert_eq!(target.clips[0].duration, 180);
    assert_ne!(
        target.clips[0].text.as_ref().unwrap().group,
        copy.clips[0].text.as_ref().unwrap().group
    );
    let before = target.clone();
    for frame in [-1, 86400 * 60, i64::MAX] {
        assert!(target.paste_clips(&copy, frame).is_err());
        assert_eq!(target, before);
    }
    target.tracks[0].locked = true;
    let before = target.clone();
    assert!(target.paste_clips(&copy, 300).is_err());
    assert_eq!(target, before);
}
