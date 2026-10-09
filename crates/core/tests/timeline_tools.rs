//! 磁吸、标记与片段锁定的核心回归；断言最终时间和参数，并检查保存兼容性。
use dinuosi_core::{
    ProjectStore,
    keyframes::KeyframeKind,
    media::MediaKind,
    timeline::{History, Property, Timeline, TrackKind},
};
use std::collections::BTreeSet;
/// 添加可编辑图片片段，返回稳定身份，测试无需媒体探测或解码。
fn image(t: &mut Timeline, track: usize, start: i64, duration: i64) -> String {
    t.insert(
        ("tools.png".into(), MediaKind::Image),
        "图片".into(),
        track,
        start,
        0..duration,
        duration,
    )
    .unwrap()
}
/// 同轨首尾、跨轨首尾、播放头、标记点均使用最近目标，排除整组自身，开关关闭后不修正。
#[test]
fn snapping_targets_and_group_boundaries() {
    let mut t = Timeline::default();
    let a = image(&mut t, 0, 30, 60);
    t.create_track(0, TrackKind::Media).unwrap();
    image(&mut t, 0, 180, 30);
    t.add_marker(300, "标记".into()).unwrap();
    let exclude = BTreeSet::from([a]);
    assert_eq!(t.snap_offset(&[92, 152], &BTreeSet::new(), 600, 4), -2);
    assert_eq!(t.snap_offset(&[177, 237], &exclude, 600, 4), 3);
    assert_eq!(t.snap_offset(&[242, 302], &exclude, 600, 4), -2);
    assert_eq!(t.snap_offset(&[399, 459], &exclude, 400, 4), 1);
    assert_eq!(t.snap_offset(&[30, 90], &exclude, 600, 4), 0);
    assert_eq!(t.snap_offset(&[10, 70], &exclude, 600, 4), 0);
    t.snapping = false;
    assert_eq!(t.snap_offset(&[177, 237], &exclude, 600, 4), 0);
}
/// 片段锁定保持可选，所有参数、动画、时间和破坏性编辑保持原值；解锁后可继续修改。
#[test]
fn locked_clips_are_selectable_and_read_only() {
    let mut t = Timeline::default();
    let id = image(&mut t, 0, 0, 90);
    let ids = BTreeSet::from([id.clone()]);
    t.toggle_keyframe(&ids, KeyframeKind::Position, 0);
    t.lock_clips(&ids, true);
    let before = t.clone();
    assert!(t.select_range(0, 30, 0, 0).contains(&id));
    t.set_property(&ids, Property::X, 120.0).unwrap();
    t.set_property_at(&ids, Property::Scale, 200.0, 30, true)
        .unwrap();
    t.toggle_keyframe(&ids, KeyframeKind::Position, 30);
    t.remove_keyframes(&id, 0);
    assert!(t.move_clips(&ids, 30, 0).is_err());
    assert!(t.move_clip_ripple(&id, 0, 120).is_err());
    assert!(t.trim(&id, true, 10).is_err());
    assert!(t.trim(&id, false, 10).is_err());
    t.split(&ids, 45).unwrap();
    t.delete(&ids);
    assert_eq!(t, before);
    t.lock_clips(&ids, false);
    t.move_clips(&ids, 30, 0).unwrap();
    t.trim(&id, false, 10).unwrap();
    t.set_property_at(&ids, Property::X, 120.0, 30, true)
        .unwrap();
    assert_eq!((t.clips[0].start, t.clips[0].duration), (30, 100));
    assert_eq!(t.clips[0].transform_at(30.0).x, 120.0);
}
/// 锁定音频禁止基础和动画音量修改，后续锁定素材使插入后移原子拒绝，不产生新轨道或部分移动。
#[test]
fn locks_protect_audio_and_indirect_ripple_moves() {
    let mut t = Timeline::default();
    let audio = t
        .insert(
            ("tools.wav".into(), MediaKind::Audio),
            "音频".into(),
            0,
            0,
            0..90,
            90,
        )
        .unwrap();
    let ids = BTreeSet::from([audio]);
    t.lock_clips(&ids, true);
    let before = t.clone();
    t.set_volume(&ids, 0.0).unwrap();
    t.set_volume_at(&ids, 0.1, 30, true).unwrap();
    t.toggle_keyframe(&ids, KeyframeKind::Volume, 30);
    assert_eq!(t, before);
    let main = t.main_track();
    image(&mut t, main, 0, 30);
    let b = image(&mut t, main, 30, 30);
    t.lock_clips(&[b].into(), true);
    let before = t.clone();
    assert!(
        t.insert_ripple(
            ("tools.png".into(), MediaKind::Image),
            "新增".into(),
            main,
            30,
            0..60,
            60
        )
        .is_err()
    );
    assert_eq!(t, before);
}
/// 标记编辑可撤销，非法输入无副作用；换帧率保持绝对时间，保存重开保留名称颜色、磁吸和片段锁定。
#[test]
fn markers_history_and_persistence_are_compatible() {
    let root = std::env::temp_dir().join(format!("dinuosi-tools-core-{}", uuid::Uuid::new_v4()));
    let store = ProjectStore::new(root.join("recent.json"));
    std::fs::create_dir_all(&root).unwrap();
    let entry = store.create("工具验证", &root).unwrap();
    let mut t = Timeline::default();
    let clip = image(&mut t, 0, 0, 90);
    let id = t.add_marker(45, "中点".into()).unwrap();
    assert_eq!(t.add_marker(45, "重复".into()).unwrap(), id);
    let before = t.clone();
    assert!(t.add_marker(-1, "负帧".into()).is_err());
    assert!(t.edit_marker(&id, Some(" ".into()), None).is_err());
    assert!(t.edit_marker(&id, None, Some(0x1000000)).is_err());
    assert_eq!(t, before);
    t.edit_marker(&id, Some("改名".into()), Some(0x66c9cf))
        .unwrap();
    let mut history = History::default();
    history.record(before.clone(), &t);
    history.step(&mut t, false);
    assert_eq!(t, before);
    history.step(&mut t, true);
    t.configure(1920, 1080, 60, 12000).unwrap();
    assert_eq!(t.markers[0].frame, 90);
    t.lock_clips(&[clip].into(), true);
    t.snapping = false;
    store.save_timeline(&entry, &t).unwrap();
    assert_eq!(store.load_timeline(&entry).unwrap(), t);
    let mut json: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&entry.path).unwrap()).unwrap();
    json["timeline"].as_object_mut().unwrap().remove("markers");
    json["timeline"].as_object_mut().unwrap().remove("snapping");
    for clip in json["timeline"]["clips"].as_array_mut().unwrap() {
        clip.as_object_mut().unwrap().remove("locked");
    }
    std::fs::write(&entry.path, serde_json::to_vec(&json).unwrap()).unwrap();
    let legacy = store.load_timeline(&entry).unwrap();
    assert!(legacy.snapping && legacy.markers.is_empty() && !legacy.clips[0].locked);
    t.delete_marker(&id);
    assert!(t.markers.is_empty());
    std::fs::remove_dir_all(root).unwrap();
}
