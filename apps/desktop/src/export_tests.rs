//! 导出入口及参数交互回归，保持项目配置与文件系统保护的边界。
use crate::{export_dialog::export_path, view::Desktop};
use dinuosi_core::{
    ProjectStore,
    export::{Container, Encoder},
};
use gpui::{Modifiers, TestAppContext, px, size};

/// 输出文件名仅为一个组件，中文可用，Windows 保留名、目录穿越及缺少文件夹均拒绝。
#[test]
fn output_name_stays_inside_selected_folder() {
    let root = std::env::temp_dir();
    assert_eq!(
        export_path("视频完成", Some(&root), Container::Mp4).unwrap(),
        root.join("视频完成.mp4")
    );
    for name in [
        "",
        "../escape",
        "a\\b",
        "NUL",
        "com1.txt",
        "CON",
        "x.",
        "x ",
        "a:b",
        "a\n",
    ] {
        assert!(
            export_path(name, Some(&root), Container::Matroska).is_err(),
            "{name}"
        );
    }
    assert!(export_path("valid", None, Container::Mp4).is_err());
}

/// 使用 GPUI 鼠标打开导出、选择参数，验证缺少目录报错、运行中 Tab/Enter 只请求取消及 Esc 关闭。
/// 项目配置不提前改变；运行状态的键盘回归不启动编码，真实取消与文件清理由核心集成测试覆盖。
#[gpui::test]
fn export_modal_parameters_do_not_edit_project(cx: &mut TestAppContext) {
    let root = std::env::temp_dir().join(format!("dinuosi-export-ui-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let store = ProjectStore::new(root.join("recent.json"));
    let entry = store.create("导出验证", &root).unwrap();
    let (desktop, visual) = cx
        .add_window_view(|window, cx| Desktop::new(store.clone(), Some(entry.clone()), window, cx));
    visual.simulate_resize(size(px(1440.0), px(900.0)));
    visual.run_until_parked();
    let button = visual.debug_bounds("export").unwrap().center();
    visual.simulate_click(button, Modifiers::none());
    visual.run_until_parked();
    assert!(visual.debug_bounds("export-overlay").is_some());
    desktop.update_in(visual, |v, _, cx| {
        let d = v.export_dialog.as_mut().unwrap();
        d.probing = false;
        d.encoders = vec![Encoder::Mpeg4];
        d.settings.encoder = Encoder::Mpeg4;
        cx.notify();
    });
    visual.run_until_parked();
    let button = visual.debug_bounds("export-fps").unwrap().center();
    visual.simulate_click(button, Modifiers::none());
    visual.run_until_parked();
    let button = visual.debug_bounds("export-fps-0").unwrap().center();
    visual.simulate_click(button, Modifiers::none());
    visual.run_until_parked();
    assert_eq!(
        desktop.read_with(visual, |v, _| v
            .export_dialog
            .as_ref()
            .unwrap()
            .settings
            .fps),
        24
    );
    assert_eq!(store.load_timeline(&entry).unwrap().fps, 30);
    let button = visual.debug_bounds("export-submit").unwrap().center();
    visual.simulate_click(button, Modifiers::none());
    assert!(desktop.read_with(visual, |v, _| {
        v.export_dialog
            .as_ref()
            .unwrap()
            .error
            .as_ref()
            .is_some_and(|e| e.contains("输出文件夹"))
    }));
    desktop.update_in(visual, |v, _, cx| {
        v.export_dialog.as_mut().unwrap().running = true;
        cx.notify();
    });
    visual.run_until_parked();
    visual.simulate_keystrokes("tab enter");
    assert!(desktop.read_with(visual, |v, _| {
        let d = v.export_dialog.as_ref().unwrap();
        d.running && d.cancel.load(std::sync::atomic::Ordering::Relaxed)
    }));
    desktop.update_in(visual, |v, _, cx| {
        v.export_dialog.as_mut().unwrap().running = false;
        cx.notify();
    });
    visual.simulate_keystrokes("escape");
    assert!(desktop.read_with(visual, |v, _| v.export_dialog.is_none()));
    std::fs::remove_dir_all(root).unwrap();
}
