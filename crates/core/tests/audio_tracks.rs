use dinuosi_core::{
    ProjectStore,
    media::MediaKind,
    timeline::{History, Timeline, TrackKind, volume_gain, volume_ratio},
};

/// 验证音频落在视觉轨道自动建独立轨道，冲突仍建音频轨；错误跨轨和锁轨保持模型不变。
#[test]
fn audio_placement_enforces_track_types_atomically() {
    let mut t = Timeline::default();
    let image = t
        .insert(
            ("image.png".into(), MediaKind::Image),
            "图片".into(),
            0,
            0,
            0..30,
            90,
        )
        .unwrap();
    let audio = t
        .insert(
            ("tone.wav".into(), MediaKind::Audio),
            "声音".into(),
            0,
            60,
            0..30,
            90,
        )
        .unwrap();
    assert_eq!(t.tracks[0].kind, TrackKind::Audio);
    assert_eq!(t.tracks[0].name, "音频轨道");
    assert_eq!(t.clips[1].start, 60);
    assert_eq!(t.clips[1].volume_db(), 0.0);
    assert_eq!(t.clips[0].track, 1);
    let before = t.clone();
    assert!(
        t.insert(
            ("video.mp4".into(), MediaKind::Video),
            "视频".into(),
            0,
            0,
            0..30,
            90
        )
        .is_err()
    );
    assert!(t.move_clips(&[image.clone()].into(), 0, -1).is_err());
    assert!(t.move_clips(&[audio.clone()].into(), 0, 1).is_err());
    assert!(t.move_clip_ripple(&audio, 1, 30).is_err());
    assert!(
        t.insert_ripple(
            ("bad.wav".into(), MediaKind::Audio),
            "声音".into(),
            1,
            30,
            0..30,
            90
        )
        .is_err()
    );
    assert_eq!(t, before);
    t.insert(
        ("second.wav".into(), MediaKind::Audio),
        "第二段".into(),
        0,
        60,
        0..30,
        90,
    )
    .unwrap();
    assert_eq!(
        t.tracks
            .iter()
            .filter(|t| t.kind == TrackKind::Audio)
            .count(),
        2
    );
    t.validate().unwrap();
    t.tracks[2].locked = true;
    let before = t.clone();
    assert!(
        t.insert(
            ("locked.wav".into(), MediaKind::Audio),
            "声音".into(),
            2,
            0,
            0..30,
            90
        )
        .is_err()
    );
    assert_eq!(t, before);
    t.clips[0].track = 0;
    assert!(t.validate().is_err());
}

/// 检查默认/端点增益、非法参数、锁轨及单次撤销，保存重开后仍保留轨道类型和精确音量。
/// 移除新字段模拟旧项目，验证旧媒体轨中的音频自动迁移且默认音量不改变源文件。
#[test]
fn audio_gain_round_trip_history_and_old_project_migration() {
    let root = std::env::temp_dir().join(format!("dinuosi-audio-model-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let store = ProjectStore::new(root.join("recent.json"));
    let entry = store.create("音频契约", &root).unwrap();
    let mut t = Timeline::default();
    let audio = t
        .insert(
            ("tone.wav".into(), MediaKind::Audio),
            "音频".into(),
            0,
            30,
            0..30,
            90,
        )
        .unwrap();
    let selected = [audio].into();
    let before = t.clone();
    assert_eq!(volume_gain(0.0), 0.0);
    assert_eq!(volume_gain(0.75), 1.0);
    assert_eq!(volume_gain(1.0), 10.0);
    assert_eq!(volume_ratio(1.0), 0.75);
    t.set_volume(&selected, 0.0).unwrap();
    assert_eq!(t.clips[0].volume_db(), f64::NEG_INFINITY);
    for bad in [f64::NAN, f64::INFINITY, -0.01, 10.01] {
        assert!(t.set_volume(&selected, bad).is_err());
        assert_eq!(t.clips[0].gain, 0.0);
    }
    t.tracks[0].locked = true;
    t.set_volume(&selected, 10.0).unwrap();
    assert_eq!(t.clips[0].gain, 0.0);
    t.tracks[0].locked = false;
    let mut history = History::default();
    history.record(before.clone(), &t);
    history.step(&mut t, false);
    assert_eq!(t, before);
    history.step(&mut t, true);
    store.save_timeline(&entry, &t).unwrap();
    assert_eq!(store.load_timeline(&entry).unwrap(), t);
    t.set_volume(&selected, 10.0).unwrap();
    store.save_timeline(&entry, &t).unwrap();
    assert_eq!(
        store.load_timeline(&entry).unwrap().clips[0].volume_db(),
        20.0
    );
    let mut json: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&entry.path).unwrap()).unwrap();
    json["timeline"]["tracks"][0]["kind"] = "media".into();
    json["timeline"]["tracks"][0]["name"] = "媒体轨道".into();
    let clip = json["timeline"]["clips"][0].as_object_mut().unwrap();
    clip.remove("kind");
    clip.remove("gain");
    std::fs::write(&entry.path, serde_json::to_vec(&json).unwrap()).unwrap();
    let migrated = store.load_timeline(&entry).unwrap();
    assert_eq!(migrated.tracks[0].kind, TrackKind::Audio);
    assert_eq!(migrated.clips[0].gain, 1.0);
    assert_eq!(migrated.clips[0].start, 30);
    // 旧主轨混放音频与图片时拆出音频轨，保留视觉主轨以及锁定、静音状态。
    json["timeline"]["tracks"].as_array_mut().unwrap().pop();
    json["timeline"]["tracks"][0]["kind"] = "main".into();
    json["timeline"]["tracks"][0]["locked"] = true.into();
    json["timeline"]["tracks"][0]["muted"] = true.into();
    let mut image = json["timeline"]["clips"][0].clone();
    image["id"] = uuid::Uuid::new_v4().to_string().into();
    image["source"] = "old.png".into();
    image["start"] = 90.into();
    json["timeline"]["clips"]
        .as_array_mut()
        .unwrap()
        .push(image);
    std::fs::write(&entry.path, serde_json::to_vec(&json).unwrap()).unwrap();
    let mixed = store.load_timeline(&entry).unwrap();
    assert_eq!(mixed.tracks[0].kind, TrackKind::Audio);
    assert_eq!(mixed.tracks[1].kind, TrackKind::Main);
    assert!(mixed.tracks[0].locked && mixed.tracks[0].muted);
    assert_eq!(mixed.clips[0].track, 0);
    assert_eq!(mixed.clips[1].track, 1);
    assert_eq!(mixed.clips[1].start, 90);
    // 恶意大索引必须在迁移前拒绝，不能让索引重映射发生溢出。
    json["timeline"]["clips"][0]["track"] = usize::MAX.into();
    std::fs::write(&entry.path, serde_json::to_vec(&json).unwrap()).unwrap();
    assert!(store.load_timeline(&entry).is_err());
    std::fs::remove_dir_all(root).unwrap();
}
