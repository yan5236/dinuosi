use crate::{
    preferences::UserPreferences,
    settings::{Settings, Tab},
    shortcuts,
    view::Desktop,
};
use dinuosi_core::{Preferences, ProjectStore};
use gpui::{Keystroke, Modifiers, TestAppContext, VisualTestContext, px, size};

/// 按实际布局坐标点击控件，缺少目标立即失败，避免测试绕过真实事件路由。
fn click(cx: &mut VisualTestContext, id: &'static str) {
    let bounds = cx
        .debug_bounds(id)
        .unwrap_or_else(|| panic!("缺少控件 {id}"));
    cx.simulate_click(bounds.center(), Modifiers::none());
    cx.run_until_parked();
}

/// 校验默认、用户重绑定、禁用、组合键精确匹配与冲突，保证恢复默认有统一来源。
#[test]
fn shortcut_registry_respects_overrides_and_conflicts() {
    let mut data = Preferences::default();
    shortcuts::validate(&data).unwrap();
    assert_eq!(
        shortcuts::resolve(&Keystroke::parse("ctrl-z").unwrap(), &data),
        Some(shortcuts::Command::Undo)
    );
    assert_eq!(
        shortcuts::resolve(&Keystroke::parse("ctrl-alt-z").unwrap(), &data),
        None
    );
    data.shortcuts.insert("new".into(), vec!["ctrl-o".into()]);
    assert!(shortcuts::validate(&data).is_err());
    data.shortcuts
        .insert("new".into(), vec!["ctrl-shift-n".into()]);
    data.shortcuts.insert("delete".into(), Vec::new());
    shortcuts::validate(&data).unwrap();
    assert_eq!(
        shortcuts::resolve(&Keystroke::parse("ctrl-n").unwrap(), &data),
        None
    );
    assert_eq!(
        shortcuts::resolve(&Keystroke::parse("delete").unwrap(), &data),
        None
    );
    assert!(shortcuts::normalize("alt-f4").is_err());
    assert!(shortcuts::normalize("tab").is_err());
}

/// 使用独立 GPUI 窗口验证录制不触发编辑、冲突提示、应用持久化、搜索与修改项目后的撤销。
#[gpui::test]
fn settings_window_isolates_input_and_applies_to_live_project(cx: &mut TestAppContext) {
    let root = std::env::temp_dir().join(format!("dinuosi-settings-ui-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let store = ProjectStore::new(root.join("recent.json"));
    let entry = store.create("设置验证", &root).unwrap();
    let (desktop, editor) = cx
        .add_window_view(|window, cx| Desktop::new(store.clone(), Some(entry.clone()), window, cx));
    editor.run_until_parked();
    let owner = desktop.downgrade();
    let (settings, visual) = cx.add_window_view(|window, cx| {
        Settings::new(
            owner,
            store.clone(),
            Some(Default::default()),
            Tab::Shortcuts,
            window,
            cx,
        )
    });
    visual.simulate_resize(size(px(860.0), px(600.0)));
    visual.run_until_parked();
    click(visual, "binding-0");
    visual.simulate_keystrokes("ctrl-o");
    assert!(settings.read_with(visual, |s, _| s.error.as_ref().unwrap().contains("冲突")));
    visual.simulate_keystrokes("ctrl-shift-n");
    assert!(desktop.read_with(visual, |v, _| v.dialog.is_none()));
    click(visual, "settings-apply");
    assert_eq!(
        store.load_preferences().unwrap().shortcuts["new"],
        ["ctrl-shift-n"]
    );
    assert!(visual.read(|cx| {
        cx.global::<UserPreferences>()
            .0
            .shortcuts
            .contains_key("new")
    }));
    click(visual, "settings-search");
    visual.simulate_input("帧率");
    visual.run_until_parked();
    assert!(
        visual.debug_bounds("project-fps").is_some(),
        "query={:?}, project_match={}",
        settings.read_with(visual, |s, cx| s.search.read(cx).text.clone()),
        settings.read_with(visual, |s, cx| s.matches(Tab::Project, cx))
    );
    click(visual, "project-width");
    visual.simulate_keystrokes("ctrl-a");
    visual.simulate_input("1080");
    click(visual, "project-height");
    visual.simulate_keystrokes("ctrl-a");
    visual.simulate_input("1920");
    click(visual, "project-fps");
    visual.simulate_keystrokes("ctrl-a");
    visual.simulate_input("60");
    click(visual, "settings-apply");
    assert!(settings.read_with(visual, |s, _| s.error.is_none()));
    let model = store.load_timeline(&entry).unwrap();
    assert_eq!((model.width, model.height, model.fps), (1080, 1920, 60));
    desktop.update(visual, |v, cx| v.timeline_history(false, cx));
    assert_eq!(store.load_timeline(&entry).unwrap().width, 1920);
    std::fs::remove_dir_all(root).unwrap();
}
