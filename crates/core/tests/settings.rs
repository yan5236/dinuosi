use dinuosi_core::{
    Preferences, ProjectStore, composite,
    media::{Media, MediaKind},
    timeline::Timeline,
};
use std::sync::Arc;

/// 验证项目参数换帧率不改变秒数、共享边界或源入点，失败原子回滚，旧文件码率兼容。
#[test]
fn project_settings_round_trip_and_frame_conversion() {
    let root = std::env::temp_dir().join(format!("dinuosi-settings-{}", uuid::Uuid::new_v4()));
    let store = ProjectStore::new(root.join("recent.json"));
    std::fs::create_dir_all(&root).unwrap();
    let entry = store.create("设置验证", &root).unwrap();
    let mut model = Timeline::default();
    for start in [30, 90] {
        model
            .insert(
                ("source.png".into(), MediaKind::Image),
                "源".into(),
                0,
                start,
                15..75,
                150,
            )
            .unwrap();
    }
    model.configure(1080, 1920, 60, 24000).unwrap();
    assert_eq!(
        (
            model.clips[0].start,
            model.clips[0].duration,
            model.clips[0].source_in
        ),
        (60, 120, 30)
    );
    assert_eq!(
        model.clips[1].start,
        model.clips[0].start + model.clips[0].duration
    );
    store.save_timeline(&entry, &model).unwrap();
    assert_eq!(store.load_timeline(&entry).unwrap(), model);
    let before = model.clone();
    assert!(model.configure(0, 1920, 60, 24000).is_err());
    assert!(model.configure(1080, 1920, 0, 24000).is_err());
    assert!(model.configure(1080, 1920, 60, 0).is_err());
    assert_eq!(model, before);
    let mut json: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&entry.path).unwrap()).unwrap();
    json["timeline"]
        .as_object_mut()
        .unwrap()
        .remove("bitrate_kbps");
    std::fs::write(&entry.path, serde_json::to_vec(&json).unwrap()).unwrap();
    assert_eq!(store.load_timeline(&entry).unwrap().bitrate_kbps, 12000);
    let mut short = Timeline::default();
    short
        .insert(
            ("s.png".into(), MediaKind::Image),
            "短片段".into(),
            0,
            0,
            0..1,
            30,
        )
        .unwrap();
    let before = short.clone();
    assert!(short.configure(1920, 1080, 1, 12000).is_err());
    assert_eq!(short, before);
    std::fs::remove_dir_all(root).unwrap();
}

/// 验证偏好与项目独立保存、覆盖值和禁用快捷键重开保留，损坏及未来版本不会静默重置。
#[test]
fn preferences_persist_and_reject_corrupt_data() {
    let root = std::env::temp_dir().join(format!("dinuosi-preferences-{}", uuid::Uuid::new_v4()));
    let store = ProjectStore::new(root.join("recent.json"));
    assert_eq!(store.load_preferences().unwrap(), Preferences::default());
    let mut data = Preferences {
        live_scrub: false,
        language: "zh-CN".into(),
        ..Default::default()
    };
    data.shortcuts
        .insert("new".into(), vec!["ctrl-shift-n".into()]);
    data.shortcuts.insert("delete".into(), Vec::new());
    store.save_preferences(&data).unwrap();
    assert_eq!(store.load_preferences().unwrap(), data);
    let path = root.join("preferences.json");
    let previous = std::fs::read(&path).unwrap();
    data.version = 2;
    assert!(store.save_preferences(&data).is_err());
    assert_eq!(std::fs::read(&path).unwrap(), previous);
    std::fs::write(&path, b"broken").unwrap();
    assert!(store.load_preferences().is_err());
    assert_eq!(std::fs::read(&path).unwrap(), b"broken");
    std::fs::remove_dir_all(root).unwrap();
}

/// 验证小素材不放大且四边黑色、竖屏项目画布比例与中心位置正确，大素材等比适配。
#[test]
fn project_canvas_letterboxes_small_and_portrait_media() {
    let mut model = Timeline::default();
    model
        .insert(
            ("s.png".into(), MediaKind::Image),
            "小素材".into(),
            0,
            0,
            0..30,
            150,
        )
        .unwrap();
    let media = Media {
        path: "s.png".into(),
        name: "小素材".into(),
        kind: MediaKind::Image,
        duration: 5.0,
        has_audio: false,
        preview_size: (640, 360),
        source_size: (320, 180),
        thumbnail: Some(Arc::new([20, 80, 200, 255].repeat(640 * 360))),
        offline: false,
    };
    for (width, height) in [(1920, 1080), (1080, 1920), (1080, 1080)] {
        model.configure(width, height, 30, 12000).unwrap();
        let pixels = composite::render(&model, 0, std::slice::from_ref(&media)).unwrap();
        let center = (180 * 640 + 320) * 4;
        assert_eq!(&pixels[center..center + 4], &[20, 80, 200, 255]);
        assert_eq!(&pixels[..4], &[0, 0, 0, 255]);
        let colored = pixels
            .as_chunks::<4>()
            .0
            .iter()
            .filter(|p| p[0] == 20)
            .count();
        let size = dinuosi_core::media::preview_dimensions((width, height));
        let expected = 320.0 / width as f64 * size.0 as f64 * 180.0 / height as f64 * size.1 as f64;
        assert!((colored as f64 - expected).abs() < 150.0);
    }
    assert_eq!(
        composite::fitted_size((3840, 2160), (1920, 1080)),
        (1920.0, 1080.0)
    );
}
