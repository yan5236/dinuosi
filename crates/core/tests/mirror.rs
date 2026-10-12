//! 镜像方向、组合、锁定边界与旧项目兼容性回归。
use dinuosi_core::{
    ProjectStore,
    composite::blend_pixels,
    media::MediaKind,
    timeline::{History, Property, Timeline, TrackKind, Transform},
};

/// 用四个不同色值证明镜像轴和双轴组合，断言预览与导出共用采样入口的完整像素结果。
#[test]
fn mirror_pixels_follow_each_axis() {
    let source: Vec<u8> = [1, 2, 3, 4]
        .into_iter()
        .flat_map(|v| [v, 0, 0, 255])
        .collect();
    for (horizontal, vertical, rotation, expected) in [
        (false, false, 0.0, [1, 2, 3, 4]),
        (true, false, 0.0, [2, 1, 4, 3]),
        (false, true, 0.0, [3, 4, 1, 2]),
        (true, true, 0.0, [4, 3, 2, 1]),
        (true, false, 90.0, [4, 2, 3, 1]),
        (false, true, 90.0, [1, 3, 2, 4]),
    ] {
        let mut out = vec![0; 16];
        let transform = Transform {
            mirror_horizontal: horizontal,
            mirror_vertical: vertical,
            rotation,
            ..Default::default()
        };
        blend_pixels(
            &mut out,
            (2, 2),
            &source,
            (2, 2),
            &transform,
            (2, 2),
            (2.0, 2.0),
        )
        .unwrap();
        assert_eq!(
            out.as_chunks::<4>()
                .0
                .iter()
                .map(|p| p[0])
                .collect::<Vec<_>>(),
            expected
        );
    }
}

/// 验证连续点击恢复、轴状态独立、逐帧变换保留、撤销重做、锁片/锁轨/音频隔离及保存重开。
/// 删除镜像字段模拟旧格式必须默认不翻转；非法布尔字段必须拒绝载入。
#[test]
fn mirror_selection_history_and_storage() {
    let root = std::env::temp_dir().join(format!("dinuosi-mirror-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&root).unwrap();
    let store = ProjectStore::new(root.join("recent.json"));
    let entry = store.create("镜像回归", &root).unwrap();
    let mut t = Timeline::default();
    let id = t
        .insert(
            ("mirror.png".into(), MediaKind::Image),
            "图片".into(),
            0,
            0,
            0..30,
            30,
        )
        .unwrap();
    let ids = [id].into();
    t.toggle_keyframe(&ids, dinuosi_core::keyframes::KeyframeKind::Position, 0);
    t.set_property_at(&ids, Property::X, 100.0, 20, true)
        .unwrap();
    let original = t.clone();
    t.mirror_clips(&ids, true);
    assert!(t.clips[0].transform_at(15.0).mirror_horizontal);
    assert!(!t.clips[0].transform_at(15.0).mirror_vertical);
    assert_eq!(t.clips[0].transform_at(10.0).x, 50.0);
    assert_eq!(t.clips[0].keyframes, original.clips[0].keyframes);
    t.mirror_clips(&ids, true);
    assert_eq!(t, original);
    t.mirror_clips(&ids, true);
    t.mirror_clips(&ids, false);
    let mirrored = t.clone();
    let mut history = History::default();
    history.record(original.clone(), &t);
    history.step(&mut t, false);
    assert_eq!(t, original);
    history.step(&mut t, true);
    assert_eq!(t, mirrored);
    t.clips[0].locked = true;
    let locked = t.clone();
    t.mirror_clips(&ids, true);
    assert_eq!(t, locked);
    t.clips[0].locked = false;
    t.tracks[0].locked = true;
    let locked = t.clone();
    t.mirror_clips(&ids, false);
    assert_eq!(t, locked);
    t.tracks[0].locked = false;
    let audio_track = t.create_track(0, TrackKind::Audio).unwrap();
    let audio = t
        .insert(
            ("audio.wav".into(), MediaKind::Audio),
            "音频".into(),
            audio_track,
            0,
            0..30,
            30,
        )
        .unwrap();
    let before = t.clone();
    t.mirror_clips(&[audio, "unknown".into()].into(), true);
    assert_eq!(t, before);
    store.save_timeline(&entry, &t).unwrap();
    assert_eq!(store.load_timeline(&entry).unwrap(), t);
    let mut json: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&entry.path).unwrap()).unwrap();
    for clip in json["timeline"]["clips"].as_array_mut().unwrap() {
        clip.as_object_mut().unwrap().remove("mirror_horizontal");
        clip.as_object_mut().unwrap().remove("mirror_vertical");
    }
    std::fs::write(&entry.path, serde_json::to_vec(&json).unwrap()).unwrap();
    assert!(
        store
            .load_timeline(&entry)
            .unwrap()
            .clips
            .iter()
            .all(|c| !c.transform.mirror_horizontal && !c.transform.mirror_vertical)
    );
    json["timeline"]["clips"][0]["mirror_horizontal"] = "invalid".into();
    std::fs::write(&entry.path, serde_json::to_vec(&json).unwrap()).unwrap();
    assert!(store.load_timeline(&entry).is_err());
    std::fs::remove_dir_all(root).unwrap();
}
