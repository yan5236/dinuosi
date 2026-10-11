use super::*;
use gpui::{
    EntityInputHandler, Modifiers, MouseButton, TestAppContext, VisualTestContext, point, size,
};

/// 验证窗口控件布局及鼠标默认行为；Linux 测试窗口未实现系统拖动，该项由原生 CI 检查。
fn assert_native_mouse_default(cx: &mut VisualTestContext, selectors: &[&'static str]) {
    for selector in selectors {
        let position = cx.debug_bounds(selector).expect("缺少原生命中区").center();
        if cfg!(target_os = "linux") && *selector == "title-drag" {
            continue;
        }
        cx.simulate_mouse_down(position, MouseButton::Left, Modifiers::none());
        assert!(
            !cx.update(|window, _| window.default_prevented()),
            "{selector} 阻止了系统默认行为"
        );
        cx.simulate_mouse_up(
            point(px(-1.0), px(-1.0)),
            MouseButton::Left,
            Modifiers::none(),
        );
    }
}

/// 在 GPUI 原生事件与布局中验证弹窗不会直接创建项目，校验与取消有效，且管理窗口禁止系统缩放。
#[gpui::test]
fn manager_modal_validation_and_native_controls(cx: &mut TestAppContext) {
    let store = ProjectStore::new(
        std::env::temp_dir().join(format!("dinuosi-gpui-{}.json", std::process::id())),
    );
    let (desktop, cx) = cx.add_window_view(|window, cx| Desktop::new(store, None, window, cx));
    cx.simulate_resize(size(px(900.0), px(620.0)));
    cx.run_until_parked();
    assert_native_mouse_default(cx, &["title-drag", "window-minimize", "window-close"]);
    assert!(cx.debug_bounds("window-maximize").is_none());
    assert!(!cx.read(|app| crate::windows::options(false, app).is_resizable));
    let create = cx.debug_bounds("new-project").unwrap().center();
    cx.simulate_click(create, Modifiers::none());
    assert!(cx.read(|app| desktop.read(app).dialog.is_some()));
    assert!(cx.read(|app| desktop.read(app).projects.is_empty()));
    let name = cx.debug_bounds("project-name").unwrap();
    assert!(name.left() >= px(200.0) && name.right() <= px(700.0));
    cx.simulate_input("中文😀项目");
    let submit = cx.debug_bounds("dialog-submit").unwrap().center();
    cx.simulate_click(submit, Modifiers::none());
    assert!(cx.read(|app| desktop.read(app).dialog.as_ref().unwrap().error.is_some()));
    cx.simulate_keystrokes("escape");
    assert!(cx.read(|app| desktop.read(app).dialog.is_none()));
}

/// 验证独立剪辑窗口在原有尺寸下保留三栏与缩放交互；空项目没有媒体伪数据。
#[gpui::test]
fn editor_layout_and_zoom_remain_consistent(cx: &mut TestAppContext) {
    let entry = ProjectEntry {
        id: "test".into(),
        name: "测试项目".into(),
        path: "unused".into(),
        created_at: 0,
        modified_at: 0,
    };
    let store = ProjectStore::new(std::env::temp_dir().join("dinuosi-layout-unused.json"));
    let (desktop, cx) =
        cx.add_window_view(|window, cx| Desktop::new(store, Some(entry), window, cx));
    for (width, height) in [(1440.0, 900.0), (1040.0, 640.0)] {
        cx.simulate_resize(size(px(width), px(height)));
        cx.run_until_parked();
        assert_native_mouse_default(
            cx,
            &[
                "title-drag",
                "window-minimize",
                "window-maximize",
                "window-close",
            ],
        );
        assert!(
            (f32::from(cx.debug_bounds("素材库").unwrap().right()) - width * 0.302).abs() < 1.0
        );
        assert!(
            (f32::from(cx.debug_bounds("preview-panel").unwrap().right()) - width * 0.741).abs()
                < 1.0
        );
        let zoom = desktop.read_with(cx, |view, _| view.zoom);
        let zoom_in = cx.debug_bounds("zoom-in").unwrap().center();
        cx.simulate_click(zoom_in, Modifiers::none());
        assert!(desktop.read_with(cx, |view, _| view.zoom) > zoom);
    }
}

/// 验证中文与代理对的 UTF-16 组合范围、替换、选择和确认；防止输入法把名称或光标切到非法边界。
#[gpui::test]
fn project_name_supports_ime_and_unicode(cx: &mut TestAppContext) {
    let (input, cx) = cx.add_window_view(|_, cx| crate::input::TextInput::new("原名".into(), cx));
    input.update_in(cx, |input, window, cx| {
        input.focus.focus(window);
        input.replace_and_mark_text_in_range(None, "中😀", Some(3..3), window, cx);
        assert_eq!(input.marked_text_range(window, cx), Some(0..3));
        assert_eq!(
            input.selected_text_range(false, window, cx).unwrap().range,
            3..3
        );
        input.replace_and_mark_text_in_range(None, "中文😀", Some(4..4), window, cx);
        input.replace_text_in_range(None, "中文😀项目", window, cx);
        assert_eq!(input.text, "中文😀项目");
        assert!(!input.is_composing());
    });
    cx.simulate_keystrokes("backspace");
    assert_eq!(
        input.read_with(cx, |input, _| input.text.clone()),
        "中文😀项"
    );
    cx.simulate_keystrokes("ctrl-a backspace");
    assert_eq!(input.read_with(cx, |input, _| input.text.clone()), "");
}

/// 在真实 GPUI 窗口集合中执行创建、管理窗口销毁、剪辑关闭恢复及持久化重新打开。
/// 检查不同 WindowId，避免单窗口替换视图冒充窗口生命周期切换；所有数据位于临时独占目录。
#[gpui::test]
fn persisted_projects_replace_windows_and_return_on_editor_close(cx: &mut TestAppContext) {
    let directory = std::env::temp_dir().join(format!(
        "dinuosi-windows-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir(&directory).unwrap();
    let store = ProjectStore::new(directory.join("data/recent.json"));
    cx.update(|app| crate::windows::start(store.clone(), app).unwrap());
    cx.background_executor.run_until_parked();
    let manager = cx.read(|app| app.windows()[0]);
    let mut visual = VisualTestContext::from_window(manager, cx);
    visual.simulate_keystrokes("ctrl-n");
    manager
        .update(cx, |root, window, app| {
            let desktop = root.downcast::<Desktop>().unwrap();
            desktop.update(app, |view, cx| {
                view.dialog.as_mut().unwrap().directory = Some(directory.clone());
                view.submit_dialog(window, cx);
            });
        })
        .unwrap();
    visual.run_until_parked();
    let editor = cx.read(|app| {
        assert_eq!(app.windows().len(), 1);
        app.windows()[0]
    });
    assert_ne!(manager.window_id(), editor.window_id());
    editor
        .update(cx, |root, window, app| {
            assert!(
                root.downcast::<Desktop>()
                    .unwrap()
                    .read(app)
                    .current
                    .is_some()
            );
            window.remove_window();
        })
        .unwrap();
    cx.background_executor.run_until_parked();
    let reopened = cx.read(|app| {
        assert_eq!(app.windows().len(), 1);
        app.windows()[0]
    });
    assert_ne!(reopened.window_id(), editor.window_id());
    reopened
        .update(cx, |root, _, app| {
            let view = root.downcast::<Desktop>().unwrap();
            assert!(view.read(app).current.is_none());
            assert_eq!(view.read(app).projects.len(), 1);
        })
        .unwrap();
    let mut visual = VisualTestContext::from_window(reopened, cx);
    let more = visual.debug_bounds("project-more-0").unwrap().center();
    visual.simulate_click(more, Modifiers::none());
    let rename = visual.debug_bounds("project-rename").unwrap().center();
    visual.simulate_click(rename, Modifiers::none());
    visual.run_until_parked();
    assert_eq!(
        reopened.window_id(),
        cx.read(|app| app.windows()[0].window_id()),
        "浮层点击穿透项目卡片"
    );
    visual.simulate_input("改名后的项目");
    let submit = visual.debug_bounds("dialog-submit").unwrap().center();
    visual.simulate_click(submit, Modifiers::none());
    visual.run_until_parked();
    assert_eq!(store.list().unwrap()[0].name, "改名后的项目");
    let more = visual.debug_bounds("project-more-0").unwrap().center();
    visual.simulate_click(more, Modifiers::none());
    let delete = visual.debug_bounds("project-delete").unwrap().center();
    visual.simulate_click(delete, Modifiers::none());
    let checkbox = visual.debug_bounds("delete-folder").unwrap().center();
    visual.simulate_click(checkbox, Modifiers::none());
    assert_eq!(
        reopened.window_id(),
        cx.read(|app| app.windows()[0].window_id()),
        "删除弹窗点击穿透项目卡片"
    );
    visual.simulate_keystrokes("escape");
    let card = visual.debug_bounds("project-0").unwrap().center();
    visual.simulate_click(card, Modifiers::none());
    visual.run_until_parked();
    let editor = cx.read(|app| app.windows()[0]);
    assert_ne!(reopened.window_id(), editor.window_id());
    let mut visual = VisualTestContext::from_window(editor, cx);
    visual.simulate_keystrokes("ctrl-h");
    visual.run_until_parked();
    assert_ne!(
        editor.window_id(),
        cx.read(|app| app.windows()[0].window_id())
    );
    assert_eq!(store.list().unwrap().len(), 1);
    std::fs::remove_dir_all(directory).unwrap();
}
