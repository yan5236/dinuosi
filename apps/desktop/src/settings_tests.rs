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

/// 通过剪辑标题栏的真实点击打开通用设置，验证菜单收起、项目参数可用及重复打开复用窗口。
/// 所有项目文件使用测试独占临时目录，不读取或修改用户项目。
#[gpui::test]
fn editor_file_menu_opens_and_reuses_settings(cx: &mut TestAppContext) {
    let root = std::env::temp_dir().join(format!("dinuosi-settings-menu-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let store = ProjectStore::new(root.join("recent.json"));
    let entry = store.create("设置菜单验证", &root).unwrap();
    let (desktop, editor) =
        cx.add_window_view(|window, cx| Desktop::new(store, Some(entry.clone()), window, cx));
    editor.simulate_resize(size(px(1040.0), px(640.0)));
    editor.run_until_parked();

    click(editor, "file-menu");
    click(editor, "menu-settings");
    let handle = desktop.read_with(editor, |v, _| {
        assert!(!v.menu_open);
        assert!(v.dialog.is_none());
        assert_eq!(v.current.as_ref().unwrap().id, entry.id);
        v.settings_window.expect("文件菜单未打开设置窗口")
    });
    handle
        .update(editor, |settings, _, _| {
            assert!(settings.tab == Tab::General);
            assert!(settings.project);
            settings.tab = Tab::Shortcuts;
        })
        .unwrap();

    click(editor, "file-menu");
    click(editor, "menu-settings");
    assert_eq!(editor.read(|app| app.windows().len()), 2);
    assert_eq!(
        desktop.read_with(editor, |v, _| v.settings_window.unwrap().window_id()),
        handle.window_id()
    );
    handle
        .update(editor, |settings, _, _| {
            assert!(settings.tab == Tab::General)
        })
        .unwrap();
    std::fs::remove_dir_all(root).unwrap();
}

/// 通过真实开关点击和焦点键盘事件验证拨片左右位置、草稿隔离与应用持久化。
/// Enter 在开关上只切换，不冒泡触发设置应用；Shift+Tab/Tab 可离开并返回开关。
#[gpui::test]
fn settings_switch_toggles_draft_without_applying(cx: &mut TestAppContext) {
    let root = std::env::temp_dir().join(format!("dinuosi-switch-ui-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let store = ProjectStore::new(root.join("recent.json"));
    let (desktop, editor) =
        cx.add_window_view(|window, cx| Desktop::new(store.clone(), None, window, cx));
    editor.run_until_parked();
    let (settings, visual) = cx.add_window_view(|window, cx| {
        Settings::new(
            desktop.downgrade(),
            store.clone(),
            None,
            Tab::General,
            window,
            cx,
        )
    });
    visual.simulate_resize(size(px(860.0), px(600.0)));
    visual.run_until_parked();
    let bounds = visual.debug_bounds("live-scrub").unwrap();
    let on = visual.debug_bounds("live-scrub-thumb").unwrap();
    assert!(on.center().x > bounds.center().x);
    click(visual, "live-scrub");
    let off = visual.debug_bounds("live-scrub-thumb").unwrap();
    assert!(off.center().x < bounds.center().x);
    assert_eq!(off.size, size(px(18.0), px(18.0)));
    assert!(!settings.read_with(visual, |s, _| s.draft.live_scrub));
    assert!(store.load_preferences().unwrap().live_scrub);
    assert!(visual.read(|cx| cx.global::<UserPreferences>().0.live_scrub));

    visual.simulate_keystrokes("space");
    visual.run_until_parked();
    assert!(settings.read_with(visual, |s, _| s.draft.live_scrub));
    visual.simulate_keystrokes("enter");
    visual.run_until_parked();
    assert!(!settings.read_with(visual, |s, _| s.draft.live_scrub));
    assert!(store.load_preferences().unwrap().live_scrub);
    visual.simulate_keystrokes("ctrl-space");
    visual.simulate_keystrokes("shift-tab tab space");
    visual.run_until_parked();
    assert!(settings.read_with(visual, |s, _| s.draft.live_scrub));
    visual.simulate_keystrokes("space");
    click(visual, "settings-apply");
    assert!(settings.read_with(visual, |s, _| s.error.is_none()));
    assert!(!store.load_preferences().unwrap().live_scrub);
    assert!(!visual.read(|cx| cx.global::<UserPreferences>().0.live_scrub));
    std::fs::remove_dir_all(root).unwrap();
}

/// 点击全部语言行验证三列对齐、单选草稿隔离及应用/重开持久化；键盘选择不冒泡应用。
/// 系统语言固定为中文以消除测试机器差异，所有磁盘写入限制在独占临时目录。
#[gpui::test]
fn language_rows_select_and_persist(cx: &mut TestAppContext) {
    let root = std::env::temp_dir().join(format!("dinuosi-language-ui-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let store = ProjectStore::new(root.join("recent.json"));
    let (desktop, editor) =
        cx.add_window_view(|window, cx| Desktop::new(store.clone(), None, window, cx));
    editor.run_until_parked();
    let (settings, visual) = cx.add_window_view(|window, cx| {
        let mut view = Settings::new(
            desktop.downgrade(),
            store.clone(),
            None,
            Tab::Language,
            window,
            cx,
        );
        view.system_locale = Some("zh-CN".into());
        view
    });
    visual.simulate_resize(size(px(860.0), px(600.0)));
    visual.run_until_parked();
    let rows = [
        (
            "auto",
            "language-auto",
            "language-native-auto",
            "language-name-auto",
            "language-check-auto",
        ),
        (
            "zh-CN",
            "language-zh-CN",
            "language-native-zh-CN",
            "language-name-zh-CN",
            "language-check-zh-CN",
        ),
        (
            "zh-TW",
            "language-zh-TW",
            "language-native-zh-TW",
            "language-name-zh-TW",
            "language-check-zh-TW",
        ),
        (
            "en",
            "language-en",
            "language-native-en",
            "language-name-en",
            "language-check-en",
        ),
        (
            "es",
            "language-es",
            "language-native-es",
            "language-name-es",
            "language-check-es",
        ),
        (
            "ru",
            "language-ru",
            "language-native-ru",
            "language-name-ru",
            "language-check-ru",
        ),
    ];
    let native_x = visual.debug_bounds("language-native-en").unwrap().origin.x;
    let translated_x = visual.debug_bounds("language-name-en").unwrap().origin.x;
    let mut previous_y = px(0.0);
    for (id, selector, native_selector, translated_selector, check_selector) in rows {
        let row = visual.debug_bounds(selector).unwrap();
        let native = visual.debug_bounds(native_selector).unwrap();
        let translated = visual.debug_bounds(translated_selector).unwrap();
        assert_eq!(native.origin.x, native_x);
        assert_eq!(translated.origin.x, translated_x);
        assert!(row.origin.y > previous_y);
        previous_y = row.origin.y;
        let saved = store.load_preferences().unwrap().language;
        click(visual, selector);
        assert_eq!(
            settings.read_with(visual, |s, _| s.draft.language.clone()),
            id
        );
        assert_eq!(store.load_preferences().unwrap().language, saved);
        for (other, _, _, _, check_selector) in rows {
            assert_eq!(
                visual.debug_bounds(check_selector).unwrap().size.width > px(0.0),
                other == id
            );
        }
        let check = visual.debug_bounds(check_selector).unwrap();
        assert!(check.origin.x < native_x && native_x < translated_x);
        click(visual, "settings-apply");
        assert!(settings.read_with(visual, |s, _| s.error.is_none()));
        assert_eq!(store.load_preferences().unwrap().language, id);
        assert_eq!(
            visual.read(|cx| cx.global::<UserPreferences>().0.language.clone()),
            id
        );
    }
    click(visual, "language-en");
    visual.simulate_keystrokes("enter");
    visual.run_until_parked();
    assert_eq!(store.load_preferences().unwrap().language, "ru");
    visual.simulate_keystrokes("tab space");
    visual.run_until_parked();
    assert_eq!(
        settings.read_with(visual, |s, _| s.draft.language.clone()),
        "es"
    );
    assert_eq!(store.load_preferences().unwrap().language, "ru");
    click(visual, "settings-search");
    visual.simulate_input("English");
    visual.run_until_parked();
    assert!(visual.debug_bounds("language-en").is_some());
    let owner = desktop.downgrade();
    let (reopened, other) = cx.add_window_view(|window, cx| {
        Settings::new(owner, store.clone(), None, Tab::Language, window, cx)
    });
    assert_eq!(
        reopened.read_with(other, |s, _| s.draft.language.clone()),
        "ru"
    );
    std::fs::remove_dir_all(root).unwrap();
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
