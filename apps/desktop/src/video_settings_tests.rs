//! 项目与导出预设的真实 GPUI 事件回归；不依赖坐标常量或系统编码器探测成功。
use crate::{
    settings::{Settings, Tab},
    view::Desktop,
};
use dinuosi_core::{
    ProjectStore,
    export::Encoder,
    video_settings::{BitrateMode, MediaFit},
};
use gpui::{Modifiers, TestAppContext, VisualTestContext, px, size};

/// 按渲染后的控件边界发送鼠标点击并等待布局，缺少控件直接失败。
fn click(cx: &mut VisualTestContext, selector: &'static str) {
    let bounds = cx
        .debug_bounds(selector)
        .unwrap_or_else(|| panic!("Missing {selector}"));
    cx.simulate_click(bounds.center(), Modifiers::none());
    cx.run_until_parked();
}

/// 通过预设完成竖屏、720p、素材裁切和质量设置，验证草稿隔离、应用重开及单次撤销。
#[gpui::test]
fn project_presets_apply_atomically_and_keep_custom_values(cx: &mut TestAppContext) {
    let root = std::env::temp_dir().join(format!("dinuosi-video-ui-{}", uuid_for_test()));
    std::fs::create_dir_all(&root).unwrap();
    let store = ProjectStore::new(root.join("recent.json"));
    let entry = store.create("竖屏", &root).unwrap();
    let (desktop, editor) = cx
        .add_window_view(|window, cx| Desktop::new(store.clone(), Some(entry.clone()), window, cx));
    editor.run_until_parked();
    let (settings, visual) = cx.add_window_view(|window, cx| {
        Settings::new(
            desktop.downgrade(),
            store.clone(),
            Some(Default::default()),
            Tab::Project,
            window,
            cx,
        )
    });
    visual.simulate_resize(size(px(860.0), px(600.0)));
    visual.run_until_parked();
    click(visual, "project-fps");
    let last = visual.debug_bounds("project-fps-6").unwrap();
    assert!(last.origin.y >= px(0.0) && last.bottom() <= px(600.0));
    click(visual, "project-fps-2");
    click(visual, "project-aspect");
    let trigger = visual.debug_bounds("project-aspect").unwrap();
    let first = visual.debug_bounds("project-aspect-0").unwrap();
    assert!(first.origin.y <= trigger.bottom() + px(10.0));
    click(visual, "project-aspect-1");
    click(visual, "project-resolution");
    click(visual, "project-resolution-0");
    click(visual, "project-fit");
    click(visual, "project-fit-1");
    click(visual, "project-quality");
    click(visual, "project-quality-1");
    assert_eq!(store.load_timeline(&entry).unwrap().width, 1920);
    assert!(visual.debug_bounds("project-width").is_none());
    assert!(visual.debug_bounds("project-bitrate").is_none());
    click(visual, "settings-apply");
    assert!(settings.read_with(visual, |s, _| s.error.is_none()));
    let saved = store.load_timeline(&entry).unwrap();
    assert_eq!((saved.width, saved.height, saved.fps), (720, 1280, 30));
    assert_eq!(saved.media_fit, MediaFit::Fill);
    assert_eq!(saved.bitrate_mode, BitrateMode::High);
    assert_eq!(saved.bitrate_kbps, 9000);
    desktop.update(visual, |v, cx| v.timeline_history(false, cx));
    assert_eq!(
        store.load_timeline(&entry).unwrap().media_fit,
        MediaFit::Fit
    );
    click(visual, "project-resolution");
    click(visual, "project-resolution-3");
    click(visual, "project-width");
    visual.simulate_keystrokes("ctrl-a");
    visual.simulate_input("0");
    click(visual, "settings-apply");
    assert!(settings.read_with(visual, |s, _| s.error.is_some()));
    assert_eq!(store.load_timeline(&entry).unwrap().width, 1920);
    std::fs::remove_dir_all(root).unwrap();
}

/// 创建测试独占后缀，使用现有标准库，不为测试引入额外依赖。
fn uuid_for_test() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos()
}

/// 导出横竖屏及分辨率可选择，码率仅自定义时出现；非法数值拒绝且不改项目，Esc 关闭菜单后保留弹窗。
#[gpui::test]
fn export_presets_and_custom_validation_keep_project_unchanged(cx: &mut TestAppContext) {
    let root = std::env::temp_dir().join(format!("dinuosi-export-presets-{}", uuid_for_test()));
    std::fs::create_dir_all(&root).unwrap();
    let store = ProjectStore::new(root.join("recent.json"));
    let entry = store.create("导出", &root).unwrap();
    let (desktop, visual) = cx
        .add_window_view(|window, cx| Desktop::new(store.clone(), Some(entry.clone()), window, cx));
    visual.simulate_resize(size(px(1440.0), px(1000.0)));
    visual.run_until_parked();
    click(visual, "export");
    desktop.update_in(visual, |v, _, cx| {
        let d = v.export_dialog.as_mut().unwrap();
        d.probing = false;
        d.encoders = vec![Encoder::Mpeg4];
        d.directory = Some(
            root.join("a_very_long_output_folder_name_that_must_not_resize_the_export_preview"),
        );
        cx.notify();
    });
    visual.run_until_parked();
    assert_eq!(
        visual.debug_bounds("export-preview").unwrap().size.width,
        px(476.0)
    );
    click(visual, "export-aspect");
    click(visual, "export-aspect-1");
    click(visual, "export-resolution");
    click(visual, "export-resolution-1");
    desktop.read_with(visual, |v, _| {
        let d = v.export_dialog.as_ref().unwrap();
        assert_eq!((d.settings.width, d.settings.height), (720, 1280));
        assert!(!d.follow_resolution);
    });
    click(visual, "export-bitrate");
    click(visual, "export-bitrate-2");
    assert!(visual.debug_bounds("export-custom-bitrate").is_some());
    let custom_container_y = visual.debug_bounds("export-container").unwrap().origin.y;
    click(visual, "export-custom-bitrate");
    visual.simulate_keystrokes("ctrl-a");
    visual.simulate_input("99");
    click(visual, "export-submit");
    desktop.read_with(visual, |v, _| {
        let d = v.export_dialog.as_ref().unwrap();
        assert!(!d.running);
        assert!(d.error.as_ref().unwrap().contains("码率"));
    });
    click(visual, "export-bitrate");
    visual.simulate_keystrokes("escape");
    visual.run_until_parked();
    assert!(desktop.read_with(visual, |v, _| {
        v.export_dialog.as_ref().unwrap().menu.is_none()
    }));
    click(visual, "export-bitrate");
    click(visual, "export-bitrate-0");
    assert_eq!(
        desktop.read_with(visual, |v, _| v
            .export_dialog
            .as_ref()
            .unwrap()
            .bitrate_mode),
        BitrateMode::Standard
    );
    // GPUI 0.2.2 的 debug_bounds 保留旧帧标识；通过后续控件上移确认自定义输入行已撤下。
    assert!(visual.debug_bounds("export-container").unwrap().origin.y < custom_container_y);
    click(visual, "export-resolution");
    click(visual, "export-resolution-0");
    desktop.read_with(visual, |v, _| {
        let d = v.export_dialog.as_ref().unwrap();
        assert_eq!((d.settings.width, d.settings.height), (1920, 1080));
        assert!(d.follow_resolution);
    });
    assert_eq!(store.load_timeline(&entry).unwrap().width, 1920);
    std::fs::remove_dir_all(root).unwrap();
}
