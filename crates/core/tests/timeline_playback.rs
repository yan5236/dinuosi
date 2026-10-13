//! 倒放、定格的帧映射、编辑连续性、锁定边界、历史及旧项目兼容回归。
use dinuosi_core::{
    ProjectStore,
    media::MediaKind,
    timeline::{History, Timeline, TrackKind},
};

/// 构造带源裁剪和时间线偏移的视频，避免只用零入点掩盖倒放边界错误。
fn video() -> (Timeline, String) {
    let mut t = Timeline::default();
    let id = t
        .insert(
            ("video.mp4".into(), MediaKind::Video),
            "视频".into(),
            0,
            100,
            20..80,
            120,
        )
        .unwrap();
    (t, id)
}

/// 验证当前选区首尾逆序、定格取倒放画面、重复点击恢复、类型及锁定边界。
#[test]
fn playback_mapping_and_editability() {
    let (mut t, id) = video();
    let ids = [id.clone()].into();
    let original = t.clone();
    assert_eq!(t.clips[0].source_frame(100), 20);
    assert_eq!(t.clips[0].source_frame(159), 79);
    t.reverse_clips(&ids);
    assert_eq!(t.clips[0].source_frame(100), 79);
    assert_eq!(t.clips[0].source_frame(159), 20);
    t.freeze_clips(&ids, 115);
    assert_eq!(t.clips[0].freeze_frame, Some(64));
    for frame in [0, 100, 115, 159, 200] {
        assert_eq!(t.clips[0].source_frame(frame), 64);
    }
    t.freeze_clips(&ids, 140);
    t.reverse_clips(&ids);
    assert_eq!(t, original);
    t.freeze_clips(&ids, 0);
    assert_eq!(t.clips[0].freeze_frame, Some(20));
    t.freeze_clips(&ids, 0);
    t.freeze_clips(&ids, 200);
    assert_eq!(t.clips[0].freeze_frame, Some(79));
    t.clips[0].locked = true;
    let locked = t.clone();
    t.reverse_clips(&ids);
    t.freeze_clips(&ids, 110);
    assert_eq!(t, locked);
    t.clips[0].locked = false;
    t.tracks[0].locked = true;
    let locked = t.clone();
    t.reverse_clips(&ids);
    t.freeze_clips(&ids, 110);
    assert_eq!(t, locked);
    let mut image = original.clone();
    image.clips[0].kind = MediaKind::Image;
    let before = image.clone();
    image.reverse_clips(&ids);
    image.freeze_clips(&ids, 110);
    assert_eq!(image, before);
    let mut audio = original;
    audio.clips[0].kind = MediaKind::Audio;
    audio.tracks[0].kind = TrackKind::Audio;
    audio.reverse_clips(&ids);
    assert!(audio.clips[0].reversed);
    audio.freeze_clips(&ids, 110);
    assert_eq!(audio.clips[0].freeze_frame, None);
}

/// 比较保留帧逐帧映射，验证倒放左右裁边、延长边界、切割和定格切割不跳源画面。
#[test]
fn trimming_and_splitting_preserve_source_frames() {
    let (mut t, id) = video();
    let ids = [id.clone()].into();
    t.reverse_clips(&ids);
    let original = t.clone();
    t.trim(&id, true, 10).unwrap();
    assert_eq!((t.clips[0].source_in, t.clips[0].duration), (20, 50));
    for frame in 110..160 {
        assert_eq!(
            t.clips[0].source_frame(frame),
            original.clips[0].source_frame(frame)
        );
    }
    t.trim(&id, false, -10).unwrap();
    assert_eq!((t.clips[0].source_in, t.clips[0].duration), (30, 40));
    for frame in 110..150 {
        assert_eq!(
            t.clips[0].source_frame(frame),
            original.clips[0].source_frame(frame)
        );
    }
    t.trim(&id, false, 1000).unwrap();
    assert_eq!(t.clips[0].source_in, 0);
    t.trim(&id, true, -1000).unwrap();
    assert_eq!(t.clips[0].source_in + t.clips[0].duration, 120);
    t.validate().unwrap();
    let mut split = original.clone();
    let right = split.split(&ids, 125).unwrap();
    assert_eq!(right.len(), 1);
    for frame in 100..160 {
        let clip = split
            .clips
            .iter()
            .find(|c| frame >= c.start && frame < c.start + c.duration)
            .unwrap();
        assert_eq!(
            clip.source_frame(frame),
            original.clips[0].source_frame(frame)
        );
    }
    let mut frozen = original;
    frozen.freeze_clips(&ids, 115);
    frozen.split(&ids, 125).unwrap();
    assert!(frozen.clips.iter().all(|c| c.freeze_frame == Some(64)));
    frozen.configure(1920, 1080, 60, 12000).unwrap();
    assert!(frozen.clips.iter().all(|c| c.freeze_frame == Some(128)));
}

/// 保存重开、历史分叉和旧文件缺省必须保留明确语义；非法固定源帧或类型拒绝载入。
#[test]
fn playback_storage_history_and_legacy_defaults() {
    let root = std::env::temp_dir().join(format!("dinuosi-playback-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&root).unwrap();
    let store = ProjectStore::new(root.join("recent.json"));
    let entry = store.create("倒放定格", &root).unwrap();
    let (mut t, id) = video();
    let ids = [id].into();
    let original = t.clone();
    let mut history = History::default();
    assert!(!history.can_step(false) && !history.can_step(true));
    t.reverse_clips(&ids);
    t.freeze_clips(&ids, 115);
    history.record(original.clone(), &t);
    assert!(history.can_step(false) && !history.can_step(true));
    let edited = t.clone();
    history.step(&mut t, false);
    assert_eq!(t, original);
    assert!(history.can_step(true));
    history.step(&mut t, true);
    assert_eq!(t, edited);
    store.save_timeline(&entry, &t).unwrap();
    assert_eq!(store.load_timeline(&entry).unwrap(), edited);
    history.step(&mut t, false);
    let before = t.clone();
    t.freeze_clips(&ids, 110);
    history.record(before, &t);
    assert!(!history.can_step(true));
    let mut json: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&entry.path).unwrap()).unwrap();
    for clip in json["timeline"]["clips"].as_array_mut().unwrap() {
        clip.as_object_mut().unwrap().remove("reversed");
        clip.as_object_mut().unwrap().remove("freeze_frame");
    }
    std::fs::write(&entry.path, serde_json::to_vec(&json).unwrap()).unwrap();
    assert_eq!(store.load_timeline(&entry).unwrap(), original);
    for value in [
        serde_json::json!(-1),
        serde_json::json!(120),
        serde_json::json!("invalid"),
    ] {
        json["timeline"]["clips"][0]["freeze_frame"] = value;
        std::fs::write(&entry.path, serde_json::to_vec(&json).unwrap()).unwrap();
        assert!(store.load_timeline(&entry).is_err());
    }
    std::fs::remove_dir_all(root).unwrap();
}
