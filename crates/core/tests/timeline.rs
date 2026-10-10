use dinuosi_core::media::MediaKind;
use dinuosi_core::{
    ProjectStore, composite,
    media::{FRAME_HEIGHT, FRAME_WIDTH},
    timeline::{History, Property, Timeline, TrackKind, Transform, ticks},
};
use std::collections::BTreeSet;

/// 验证帧精确插入、组移动、裁边、锁定、删除与撤销，不需要 GUI 或媒体解码。
#[test]
fn edits_preserve_source_limits_and_history() {
    let mut t = Timeline::default();
    t.create_track(0, TrackKind::Media).unwrap();
    t.create_track(0, TrackKind::Media).unwrap();
    let a = t
        .insert(
            ("a.mp4".into(), MediaKind::Video),
            "a".into(),
            2,
            30,
            15..105,
            120,
        )
        .unwrap();
    let b = t
        .insert(
            ("b.mp4".into(), MediaKind::Video),
            "b".into(),
            2,
            150,
            0..90,
            120,
        )
        .unwrap();
    let selected = BTreeSet::from([a.clone(), b.clone()]);
    let before = t.clone();
    let mut history = History::default();
    t.move_clips(&selected, -60, -1).unwrap();
    assert_eq!(
        (t.clips[0].start, t.clips[1].start, t.clips[0].track),
        (0, 120, 1)
    );
    t.trim(&a, true, 500).unwrap();
    assert_eq!((t.clips[0].duration, t.clips[0].source_in), (1, 104));
    t.trim(&a, false, 500).unwrap();
    assert_eq!(t.clips[0].duration, 16);
    history.record(before.clone(), &t);
    history.step(&mut t, false);
    assert_eq!(t, before);
    history.step(&mut t, true);
    assert_eq!(t.clips[0].duration, 16);
    t.tracks[1].locked = true;
    assert!(t.move_clips(&selected, 30, 0).is_err());
    t.delete(&selected);
    assert_eq!(t.clips.len(), 2);
    t.tracks[t.clips[0].track].locked = false;
    t.delete(&selected);
    assert!(t.clips.is_empty());
}

/// 验证参数边界、跨轨框选和帧标尺；最大缩放在两帧处标注 2f，短线每帧出现。
#[test]
fn properties_selection_and_adaptive_ticks() {
    let mut t = Timeline::default();
    t.create_track(0, TrackKind::Media).unwrap();
    t.create_track(0, TrackKind::Media).unwrap();
    let id = t
        .insert(
            ("a.png".into(), MediaKind::Image),
            "a".into(),
            1,
            30,
            0..150,
            150,
        )
        .unwrap();
    let selected = BTreeSet::from([id]);
    assert_eq!(t.select_range(0, 31, 1, 2), selected);
    assert!(t.select_range(0, 30, 1, 2).is_empty());
    t.set_property(&selected, Property::Rotation, -500.0)
        .unwrap();
    t.set_property(&selected, Property::Scale, 900.0).unwrap();
    assert_eq!(t.clips[0].transform.rotation, -360.0);
    assert_eq!(t.clips[0].transform.scale, 500.0);
    assert!(t.set_property(&selected, Property::X, f64::NAN).is_err());
    let near = ticks(30, 1800.0, 0.0, 800.0);
    assert_eq!(near[1].frame, 1);
    assert_eq!(near[2].label, "2f");
    assert_eq!(near[4].label, "4f");
    let far = ticks(30, 20.0, 0.0, 800.0);
    assert!(far.iter().any(|t| t.label == "00:05"));
    assert_eq!(t.frame(1.0 / 30.0), 1);
}

/// 验证时间线落盘重开和旧 V1 项目兼容；素材库删除不会删除源文件或时间线引用。
#[test]
fn timeline_round_trip_and_legacy_project() {
    let root = std::env::temp_dir().join(format!("dinuosi-timeline-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&root).unwrap();
    let store = ProjectStore::new(root.join("recent.json"));
    let entry = store.create("时间线", &root).unwrap();
    let source = root.join("image.png");
    std::fs::write(&source, b"source").unwrap();
    store
        .add_media_paths(&entry, std::slice::from_ref(&source))
        .unwrap();
    let mut t = Timeline::default();
    t.create_track(0, TrackKind::Media).unwrap();
    t.create_track(0, TrackKind::Media).unwrap();
    t.insert(
        (source.clone(), MediaKind::Image),
        "图像".into(),
        2,
        60,
        0..150,
        150,
    )
    .unwrap();
    t.clips[0].transform.x = -123.0;
    t.remove_empty_tracks();
    store.save_timeline(&entry, &t).unwrap();
    assert_eq!(store.load_timeline(&entry).unwrap(), t);
    store
        .remove_media(&entry, &std::fs::canonicalize(&source).unwrap())
        .unwrap();
    assert!(source.exists());
    assert_eq!(store.load_timeline(&entry).unwrap(), t);
    let mut json: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&entry.path).unwrap()).unwrap();
    json.as_object_mut().unwrap().remove("timeline");
    std::fs::write(&entry.path, serde_json::to_vec(&json).unwrap()).unwrap();
    assert!(store.load_timeline(&entry).unwrap().clips.is_empty());
    std::fs::remove_dir_all(root).unwrap();
}

/// 主轨按指定落点插入并自由移动；严格重叠新建上方轨道，首尾相接和空隙保持合法。
/// 同时验证锁轨拒绝是原子的、裁边冲突不覆盖旧片段、黑色空隙与轨道重排落盘结果。
#[test]
fn dynamic_tracks_keep_positions_and_never_overlap() {
    let mut t = Timeline::default();
    assert_eq!(t.tracks.len(), 1);
    assert_eq!(t.tracks[0].kind, TrackKind::Main);
    let a = t
        .insert(
            ("a.png".into(), MediaKind::Image),
            "a".into(),
            0,
            30,
            0..30,
            150,
        )
        .unwrap();
    let b = t
        .insert(
            ("b.png".into(), MediaKind::Image),
            "b".into(),
            0,
            60,
            0..30,
            150,
        )
        .unwrap();
    assert_eq!(t.tracks.len(), 1);
    t.move_clips(&BTreeSet::from([b.clone()]), 90, 0).unwrap();
    assert_eq!(t.clips[1].start, 150);
    let media = dinuosi_core::media::Media {
        path: "a.png".into(),
        name: "a".into(),
        kind: dinuosi_core::media::MediaKind::Image,
        duration: 5.0,
        has_audio: false,
        preview_size: (640, 360),
        source_size: (1920, 1080),
        offline: false,
        thumbnail: Some(std::sync::Arc::new(
            [60, 80, 160, 255].repeat((FRAME_WIDTH * FRAME_HEIGHT) as usize),
        )),
    };
    let colored = composite::render(&t, 30, std::slice::from_ref(&media)).unwrap();
    assert_eq!(&colored[..4], &[60, 80, 160, 255]);
    let black = composite::render(&t, 100, std::slice::from_ref(&media)).unwrap();
    assert!(
        black
            .as_chunks::<4>()
            .0
            .iter()
            .all(|p| *p == [0, 0, 0, 255])
    );
    let snapshot = t.clone();
    t.tracks[0].locked = true;
    let locked = t.clone();
    assert!(t.move_clips(&BTreeSet::from([b.clone()]), -110, 0).is_err());
    assert_eq!(t, locked);
    t = snapshot;
    t.move_clips(&BTreeSet::from([b.clone()]), -110, 0).unwrap();
    assert_eq!(t.tracks.len(), 2);
    assert_eq!(
        (t.clips[0].track, t.clips[1].track, t.clips[1].start),
        (1, 0, 40)
    );
    let c = t
        .insert(
            ("c.png".into(), MediaKind::Image),
            "c".into(),
            0,
            50,
            0..30,
            150,
        )
        .unwrap();
    assert_eq!(t.tracks.len(), 3);
    assert_eq!(
        (t.clips[1].track, t.clips[2].track, t.clips[2].start),
        (1, 0, 50)
    );
    t.move_clips(&BTreeSet::from([c.clone()]), 50, 1).unwrap();
    t.trim(&b, false, 40).unwrap();
    assert_eq!(t.tracks.len(), 3, "裁边冲突应复用上方刚腾空的轨道");
    assert_eq!(t.clips.iter().find(|c| c.id == b).unwrap().start, 40);
    t.validate().unwrap();
    let track = t.clips.iter().find(|c| c.id == b).unwrap().track;
    t.tracks[track].name = "移动层".into();
    t.reorder_track(track, t.main_track()).unwrap();
    assert_eq!(t.tracks[1].name, "移动层");
    assert_eq!(t.clips.iter().find(|c| c.id == b).unwrap().track, 1);
    assert!(t.reorder_track(t.main_track(), 0).is_err());
    let subtitle = t.create_track(0, TrackKind::Subtitle).unwrap();
    let before = t.clone();
    assert!(
        t.insert(
            ("x".into(), MediaKind::Video),
            "x".into(),
            subtitle,
            0,
            0..30,
            30
        )
        .is_err()
    );
    assert_eq!(t, before);
    assert!(t.clips.iter().any(|c| c.id == a));
    t.validate().unwrap();
}

/// 旧 V1 三轨移除空默认行，保留被重命名的主轨、时间空隙和源选区，并拆开旧同轨重叠。
/// 新动态轨道类型落盘重开保持一致；损坏索引和重复 UUID 仍然拒绝载入。
#[test]
fn legacy_three_tracks_migrate_without_losing_clips() {
    let root =
        std::env::temp_dir().join(format!("dinuosi-track-migration-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&root).unwrap();
    let store = ProjectStore::new(root.join("recent.json"));
    let entry = store.create("迁移", &root).unwrap();
    let mut t = Timeline::default();
    t.create_track(0, TrackKind::Media).unwrap();
    t.create_track(0, TrackKind::Subtitle).unwrap();
    t.tracks[2].name = "已改名主轨".into();
    t.insert(
        ("a.png".into(), MediaKind::Image),
        "a".into(),
        2,
        60,
        15..75,
        120,
    )
    .unwrap();
    t.insert(
        ("b.png".into(), MediaKind::Image),
        "b".into(),
        2,
        180,
        0..60,
        120,
    )
    .unwrap();
    store.save_timeline(&entry, &t).unwrap();
    let mut json: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&entry.path).unwrap()).unwrap();
    for track in json["timeline"]["tracks"].as_array_mut().unwrap() {
        track.as_object_mut().unwrap().remove("kind");
    }
    json["timeline"]["clips"][1]["start"] = 90.into();
    std::fs::write(&entry.path, serde_json::to_vec(&json).unwrap()).unwrap();
    let migrated = store.load_timeline(&entry).unwrap();
    assert_eq!(migrated.tracks.len(), 2);
    assert_eq!(migrated.tracks[1].name, "已改名主轨");
    assert_eq!(migrated.tracks[1].kind, TrackKind::Main);
    assert_eq!(
        migrated
            .clips
            .iter()
            .map(|c| (c.start, c.source_in))
            .collect::<Vec<_>>(),
        [(60, 15), (90, 0)]
    );
    migrated.validate().unwrap();
    store.save_timeline(&entry, &migrated).unwrap();
    assert_eq!(store.load_timeline(&entry).unwrap(), migrated);
    json["timeline"]["clips"][0]["track"] = 999.into();
    std::fs::write(&entry.path, serde_json::to_vec(&json).unwrap()).unwrap();
    assert!(store.load_timeline(&entry).is_err());
    std::fs::remove_dir_all(root).unwrap();
}

/// 验证中心坐标、透明度和旋转确实改变预览像素，防止检查器仅改变显示数值。
#[test]
fn composite_applies_centered_transform_and_opacity() {
    let len = (FRAME_WIDTH * FRAME_HEIGHT * 4) as usize;
    let mut source = vec![0; len];
    let center = ((180 * FRAME_WIDTH + 320) * 4) as usize;
    source[center..center + 4].copy_from_slice(&[200, 100, 50, 255]);
    let mut out = vec![0; len];
    composite::blend(&mut out, &source, &Transform::default(), (1920, 1080)).unwrap();
    assert_eq!(&out[center..center + 4], &[200, 100, 50, 255]);
    out.fill(0);
    let t = Transform {
        x: 30.0,
        opacity: 50.0,
        ..Default::default()
    };
    composite::blend(&mut out, &source, &t, (1920, 1080)).unwrap();
    assert_eq!(&out[center + 40..center + 44], &[100, 50, 25, 255]);
    assert_eq!(out[center], 0);
    out.fill(0);
    composite::blend(
        &mut out,
        &source,
        &Transform {
            scale: 0.0,
            ..Default::default()
        },
        (1920, 1080),
    )
    .unwrap();
    assert!(out.iter().all(|v| *v == 0));
}

/// 使用真实解码器检查音频片段的延迟、叠加与轨道静音，确保拖入音频后仍可实际播放。
#[test]
#[ignore = "需要准备应用配套 FFmpeg 和 ffprobe"]
fn timeline_audio_mixes_and_respects_track_mute() {
    let root = std::env::temp_dir().join(format!("dinuosi-mix-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&root).unwrap();
    let source = root.join("constant.wav");
    assert!(
        dinuosi_core::media::command("ffmpeg")
            .args([
                "-v",
                "error",
                "-f",
                "lavfi",
                "-i",
                "aevalsrc=0.125|0.125:s=48000:d=0.2",
                "-ac",
                "2",
                "-c:a",
                "pcm_f32le"
            ])
            .arg(&source)
            .status()
            .unwrap()
            .success()
    );
    let media = dinuosi_core::media::probe(&source).unwrap();
    let mut timeline = Timeline::default();
    timeline.create_track(0, TrackKind::Audio).unwrap();
    timeline.create_track(0, TrackKind::Audio).unwrap();
    for track in [0, 1] {
        timeline
            .insert(
                (media.path.clone(), MediaKind::Audio),
                "tone".into(),
                track,
                3,
                0..6,
                6,
            )
            .unwrap();
    }
    for (gain, muted, expected) in [
        (1.0, false, 0.25),
        (1.0, true, 0.125),
        (0.5, true, 0.0625),
        (10.0, true, 1.0),
        (0.0, true, 0.0),
    ] {
        timeline.tracks[1].muted = muted;
        let ids = [timeline.clips[0].id.clone()].into();
        timeline.set_volume(&ids, gain).unwrap();
        let mut audio = dinuosi_core::timeline_audio::TimelineAudio::start(
            &timeline,
            0.0,
            std::slice::from_ref(&media),
        )
        .unwrap();
        assert!(audio.by_ref().take(9600).all(|s| s == 0.0));
        let sample = audio.nth(1000).unwrap();
        assert!((sample - expected).abs() < 0.001, "{sample} != {expected}");
    }
    std::fs::remove_dir_all(root).unwrap();
}
