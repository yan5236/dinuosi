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
    // 明确重命名成默认中文名也必须原样保存；旧文件缺失标记按旧默认名迁移。
    model.tracks[0].custom_name = true;
    store.save_timeline(&entry, &model).unwrap();
    assert!(store.load_timeline(&entry).unwrap().tracks[0].custom_name);
    let mut json: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&entry.path).unwrap()).unwrap();
    json["timeline"]["tracks"][0]
        .as_object_mut()
        .unwrap()
        .remove("custom_name");
    std::fs::write(&entry.path, serde_json::to_vec(&json).unwrap()).unwrap();
    assert!(!store.load_timeline(&entry).unwrap().tracks[0].custom_name);
    json["timeline"]["tracks"][0]["name"] = "用户中文名称".into();
    std::fs::write(&entry.path, serde_json::to_vec(&json).unwrap()).unwrap();
    assert!(store.load_timeline(&entry).unwrap().tracks[0].custom_name);
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

/// 验证偏好独立保存、拖动显示模式、三种有效语言与快捷键覆盖重开保留，未知语言、损坏及未来版本不会覆盖旧值。
/// 自动关键帧偏好可关闭并保存，旧文件缺字段默认开启；标记竖线可开启并保存，旧文件缺字段默认关闭。
/// 旧文件缺少拖动显示字段时默认跟随鼠标，不改写磁盘中的已有设置。
#[test]
fn preferences_persist_and_reject_corrupt_data() {
    let root = std::env::temp_dir().join(format!("dinuosi-preferences-{}", uuid::Uuid::new_v4()));
    let store = ProjectStore::new(root.join("recent.json"));
    assert_eq!(store.load_preferences().unwrap(), Preferences::default());
    let mut data = Preferences {
        live_scrub: false,
        keep_dragged_clip: true,
        auto_keyframe: false,
        show_marker_lines: true,
        language: "zh-CN".into(),
        ..Default::default()
    };
    data.shortcuts
        .insert("new".into(), vec!["ctrl-shift-n".into()]);
    data.shortcuts.insert("delete".into(), Vec::new());
    store.save_preferences(&data).unwrap();
    assert_eq!(store.load_preferences().unwrap(), data);
    for language in dinuosi_core::language::LANGUAGES {
        data.language = language.id.into();
        store.save_preferences(&data).unwrap();
        assert_eq!(store.load_preferences().unwrap(), data);
    }
    let path = root.join("preferences.json");
    let mut old = serde_json::to_value(&data).unwrap();
    old.as_object_mut().unwrap().remove("keep_dragged_clip");
    old.as_object_mut().unwrap().remove("auto_keyframe");
    old.as_object_mut().unwrap().remove("show_marker_lines");
    std::fs::write(&path, serde_json::to_vec(&old).unwrap()).unwrap();
    assert!(!store.load_preferences().unwrap().keep_dragged_clip);
    assert!(store.load_preferences().unwrap().auto_keyframe);
    assert!(!store.load_preferences().unwrap().show_marker_lines);
    assert!(
        serde_json::from_slice::<serde_json::Value>(&std::fs::read(&path).unwrap())
            .unwrap()
            .get("keep_dragged_clip")
            .is_none()
    );
    store.save_preferences(&data).unwrap();
    for legacy in ["zh-TW", "es", "ru"] {
        let mut old = serde_json::to_value(&data).unwrap();
        old["language"] = legacy.into();
        std::fs::write(&path, serde_json::to_vec(&old).unwrap()).unwrap();
        let migrated = store.load_preferences().unwrap();
        assert_eq!(migrated.language, "en");
        assert_eq!(migrated.shortcuts, data.shortcuts);
        assert_eq!(migrated.live_scrub, data.live_scrub);
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(&std::fs::read(&path).unwrap()).unwrap()["language"],
            legacy
        );
    }
    store.save_preferences(&data).unwrap();
    let previous = std::fs::read(&path).unwrap();
    data.language = "fr".into();
    assert!(store.save_preferences(&data).is_err());
    assert_eq!(std::fs::read(&path).unwrap(), previous);
    data.language = "auto".into();
    data.version = 2;
    assert!(store.save_preferences(&data).is_err());
    assert_eq!(std::fs::read(&path).unwrap(), previous);
    std::fs::write(&path, b"broken").unwrap();
    assert!(store.load_preferences().is_err());
    assert_eq!(std::fs::read(&path).unwrap(), b"broken");
    std::fs::remove_dir_all(root).unwrap();
}

/// 校验有效语言索引与自动解析，覆盖英文回退、显式选择优先级、中文脚本和系统标签差异。
#[test]
fn language_index_names_and_system_resolution() {
    use dinuosi_core::language::{LANGUAGES, resolve};
    assert_eq!(LANGUAGES.each_ref().map(|l| l.id), ["auto", "zh-CN", "en"]);
    assert_eq!(
        LANGUAGES.each_ref().map(|l| l.native_name),
        ["Automatic", "简体中文", "English"]
    );
    for (tag, expected) in [
        ("en-US", "en"),
        ("ES_mx", "en"),
        ("ru-RU", "en"),
        ("zh-HK", "en"),
        ("zh-MO", "en"),
        ("zh-TW", "en"),
        ("zh-Hant-CN", "en"),
        ("zh-Hans-TW", "zh-CN"),
        ("ZH_cn", "zh-CN"),
        ("zh-SG", "zh-CN"),
        ("zh", "zh-CN"),
        ("fr-FR", "en"),
        ("", "en"),
    ] {
        assert_eq!(resolve("auto", Some(tag)), expected);
    }
    assert_eq!(resolve("auto", None), "en");
    assert_eq!(resolve("zh-CN", Some("en-US")), "zh-CN");
    assert_eq!(resolve("en", Some("zh-CN")), "en");
    assert_eq!(resolve("ru", Some("zh-CN")), "en");
}

/// 验证小素材不放大且四边黑色、竖屏项目画布比例与中心位置正确，大素材等比适配。
#[test]
fn project_canvas_letterboxes_small_and_portrait_media() {
    let mut model = Timeline {
        media_fit: dinuosi_core::video_settings::MediaFit::Original,
        ..Default::default()
    };
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
