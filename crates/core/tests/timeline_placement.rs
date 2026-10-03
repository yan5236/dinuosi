//! 放置规则回归：空轨道清理、冲突复用及片段间插入，不需要图形窗口或解码器。
use dinuosi_core::media::MediaKind;
use dinuosi_core::timeline::{History, Timeline, TrackKind};
use std::collections::BTreeSet;

/// 插入单段一秒素材供业务断言使用；返回稳定片段身份，不访问源文件。
fn clip(t: &mut Timeline, track: usize, start: i64) -> String {
    t.insert(
        ("fixture.png".into(), MediaKind::Image),
        "片段".into(),
        track,
        start,
        0..30,
        60,
    )
    .unwrap()
}

/// 清空非主轨时重映射剩余索引，主轨即使为空或已锁定也保留；撤销恢复片段和轨道一起。
#[test]
fn empty_tracks_disappear_but_main_survives_and_undo_restores() {
    let mut t = Timeline::default();
    let base = clip(&mut t, 0, 90);
    t.create_track(0, TrackKind::Media).unwrap();
    let upper = clip(&mut t, 0, 30);
    t.create_track(0, TrackKind::Subtitle).unwrap();
    t.remove_empty_tracks();
    assert_eq!(t.tracks.len(), 2);
    assert_eq!(t.clips.iter().find(|c| c.id == base).unwrap().track, 1);
    let before = t.clone();
    let mut history = History::default();
    t.delete(&BTreeSet::from([upper]));
    assert_eq!(t.tracks.len(), 1);
    assert_eq!(t.clips[0].track, 0);
    history.record(before.clone(), &t);
    history.step(&mut t, false);
    assert_eq!(t, before);
    history.step(&mut t, true);
    t.delete(&BTreeSet::from([base]));
    assert!(t.clips.is_empty());
    assert_eq!(t.tracks.len(), 1);
    assert_eq!(t.tracks[0].kind, TrackKind::Main);
    t.tracks[0].locked = true;
    t.remove_empty_tracks();
    assert_eq!(t.tracks.len(), 1);
}

/// 同轨覆盖先复用紧邻上方空媒体轨道；刚腾空的源轨道也能复用，锁定或已有内容则新建。
#[test]
fn collisions_reuse_empty_upper_track_before_creating_another() {
    let mut t = Timeline::default();
    let base = clip(&mut t, 0, 30);
    t.create_track(0, TrackKind::Media).unwrap();
    let moved = clip(&mut t, 0, 90);
    t.move_clips(&BTreeSet::from([moved.clone()]), -50, 1)
        .unwrap();
    assert_eq!(t.tracks.len(), 2, "腾空的上轨应该直接复用");
    assert_eq!(t.clips.iter().find(|c| c.id == moved).unwrap().track, 0);
    assert_eq!(t.clips.iter().find(|c| c.id == base).unwrap().track, 1);
    t.validate().unwrap();
    t.delete(&BTreeSet::from([moved]));
    t.create_track(0, TrackKind::Media).unwrap();
    let inserted = clip(&mut t, 1, 40);
    assert_eq!(t.tracks.len(), 2);
    assert_eq!(t.clips.iter().find(|c| c.id == inserted).unwrap().track, 0);
    t.create_track(0, TrackKind::Media).unwrap();
    t.tracks[0].locked = true;
    clip(&mut t, 1, 45);
    assert_eq!(t.tracks.len(), 4, "锁定空轨道不能被自动复用");
    t.remove_empty_tracks();
    t.validate().unwrap();
}

/// 插入边界只后移同轨后续素材，保留其他轨道、参数和源选区；非法选区/上限失败完全回滚。
/// 移动已有片段到片段间也使用同一后移规则，删除原空轨道后内容不会丢失。
#[test]
fn insertion_between_clips_pushes_following_content_atomically() {
    let mut t = Timeline::default();
    let first = clip(&mut t, 0, 0);
    let second = clip(&mut t, 0, 30);
    let third = clip(&mut t, 0, 90);
    assert_eq!(t.insertion_frame(0, 32, 4, &BTreeSet::new()), Some(30));
    assert_eq!(t.insertion_frame(0, 15, 4, &BTreeSet::new()), None);
    let inserted = t
        .insert_ripple(
            ("insert.png".into(), MediaKind::Image),
            "插入".into(),
            0,
            30,
            15..60,
            90,
        )
        .unwrap();
    assert_eq!(t.tracks.len(), 1);
    assert_eq!(t.clips.iter().find(|c| c.id == first).unwrap().start, 0);
    assert_eq!(t.clips.iter().find(|c| c.id == second).unwrap().start, 75);
    assert_eq!(t.clips.iter().find(|c| c.id == third).unwrap().start, 135);
    assert_eq!(
        t.clips.iter().find(|c| c.id == inserted).unwrap().source_in,
        15
    );
    let before = t.clone();
    assert!(
        t.insert_ripple(
            ("bad".into(), MediaKind::Video),
            "bad".into(),
            0,
            75,
            0..200,
            30
        )
        .is_err()
    );
    assert_eq!(t, before);
    assert!(
        t.insert_ripple(
            ("bad".into(), MediaKind::Video),
            "bad".into(),
            0,
            20,
            0..30,
            30
        )
        .is_err()
    );
    assert_eq!(t, before);
    t.create_track(0, TrackKind::Media).unwrap();
    let moved = clip(&mut t, 0, 200);
    t.move_clip_ripple(&moved, 1, 75).unwrap();
    t.remove_empty_tracks();
    assert_eq!(t.tracks.len(), 1);
    assert_eq!(t.clips.iter().find(|c| c.id == moved).unwrap().start, 75);
    assert_eq!(t.clips.iter().find(|c| c.id == second).unwrap().start, 105);
    assert_eq!(t.clips.iter().find(|c| c.id == third).unwrap().start, 165);
    t.validate().unwrap();
    t.tracks[0].locked = true;
    let before = t.clone();
    assert!(t.move_clip_ripple(&first, 0, 105).is_err());
    assert_eq!(t, before);
    let mut limit = Timeline::default();
    clip(&mut limit, 0, 0);
    let last_frame = 86400 * limit.fps as i64;
    clip(&mut limit, 0, last_frame);
    let before = limit.clone();
    assert!(
        limit
            .insert_ripple(
                ("limit.png".into(), MediaKind::Image),
                "上限".into(),
                0,
                30,
                0..30,
                60
            )
            .is_err()
    );
    assert_eq!(limit, before, "后移越界不能留下部分修改");
}
