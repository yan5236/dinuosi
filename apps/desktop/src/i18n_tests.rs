use crate::{
    i18n, preferences,
    settings::{Settings, Tab},
    view::Desktop,
};
use dinuosi_core::{Preferences, ProjectStore};
use gpui::{Modifiers, TestAppContext, VisualTestContext, px, size};
use std::collections::{BTreeMap, BTreeSet};

/// 提取翻译模板参数名，供资源检查确认两种语言替换相同数据；空参数按出现顺序编号。
fn parameters(text: &str) -> BTreeSet<String> {
    let mut rest = text;
    let mut result = BTreeSet::new();
    let mut position = 0;
    while let Some(start) = rest.find('{') {
        let end = rest[start..].find('}').unwrap() + start;
        let key = &rest[start + 1..end];
        result.insert(if key.is_empty() {
            let key = position.to_string();
            position += 1;
            key
        } else {
            key.to_owned()
        });
        rest = &rest[end + 1..];
    }
    result
}

/// 检查全部模块 JSON 的非空值、键集、占位符与英文内容，防止只有键对齐却没有实际翻译。
#[test]
fn modular_catalogs_are_complete_and_use_matching_parameters() {
    for (module, en, zh) in i18n::SOURCES {
        let en: BTreeMap<String, String> = serde_json::from_str(en).unwrap();
        let zh: BTreeMap<String, String> = serde_json::from_str(zh).unwrap();
        assert!(!en.is_empty(), "{module}");
        assert_eq!(
            en.keys().collect::<Vec<_>>(),
            zh.keys().collect::<Vec<_>>(),
            "{module}"
        );
        for (key, text) in &en {
            assert!(!text.is_empty() && !zh[key].is_empty(), "{module}/{key}");
            assert!(
                !text.chars().any(|c| ('\u{4e00}'..='\u{9fff}').contains(&c)),
                "{module}/{key}"
            );
            assert_eq!(parameters(text), parameters(&zh[key]), "{module}/{key}");
            if module.starts_with("errors-") {
                assert_eq!(parameters(key), parameters(text), "{module}/{key}");
            }
        }
        if *module == "icons" {
            for icon in dinuosi_icons::Icon::ALL {
                assert!(en.contains_key(icon.name()));
            }
        }
        if *module == "settings" {
            for shortcut in crate::shortcuts::SHORTCUTS {
                assert!(en.contains_key(shortcut.label));
            }
            for tab in crate::settings::TABS {
                assert!(en.contains_key(tab.label()));
            }
            for language in dinuosi_core::language::LANGUAGES {
                assert!(en.contains_key(language.id));
            }
        }
        if *module == "library" {
            for (_, key, _) in crate::library::Filter::items() {
                assert!(en.contains_key(key));
            }
        }
    }
    assert_eq!(i18n::lookup("fr", "manager", "New project"), "New project");
    assert_eq!(i18n::lookup("zh-CN", "manager", "New project"), "新建项目");
    assert_eq!(
        i18n::lookup("zh-CN", "manager", "Unknown English message"),
        "Unknown English message"
    );
}

/// 点击本次测试窗口内的真实控件并等待刷新；不存在的目标直接失败。
fn click(cx: &mut VisualTestContext, selector: &'static str) {
    let bounds = cx
        .debug_bounds(selector)
        .unwrap_or_else(|| panic!("missing {selector}"));
    cx.simulate_click(bounds.center(), Modifiers::none());
    cx.run_until_parked();
}

/// 验证英文/中文及最小/默认窗口宽度下，六个分类 SVG 的实际布局都保持 18×18px。
/// 设置通过真实偏好保存入口发布，确保检测切换后的布局而非构造时的静态尺寸。
#[gpui::test]
fn library_filter_icons_keep_their_size_in_both_languages(cx: &mut TestAppContext) {
    let root = std::env::temp_dir().join(format!("dinuosi-filter-i18n-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let store = ProjectStore::new(root.join("recent.json"));
    let entry = store.create("图标尺寸验证", &root).unwrap();
    let (desktop, visual) =
        cx.add_window_view(|window, cx| Desktop::new(store.clone(), Some(entry), window, cx));
    for width in [1040.0, 1440.0] {
        visual.simulate_resize(size(px(width), px(900.0)));
        for (language, sidebar_width) in [("en", 96.0), ("zh-CN", 84.0)] {
            desktop.update(visual, |_, cx| {
                preferences::save(
                    &store,
                    Preferences {
                        language: language.into(),
                        ..Default::default()
                    },
                    cx,
                )
                .unwrap()
            });
            visual.run_until_parked();
            assert_eq!(
                visual.debug_bounds("library-filters").unwrap().size.width,
                px(sidebar_width)
            );
            for selector in [
                "filter-icon-all-media",
                "filter-icon-video",
                "filter-icon-audio",
                "filter-icon-image",
                "filter-icon-motion",
                "filter-icon-effects",
            ] {
                assert_eq!(
                    visual.debug_bounds(selector).unwrap().size,
                    size(px(18.0), px(18.0)),
                    "{language}/{width}/{selector}"
                );
            }
        }
    }
    std::fs::remove_dir_all(root).unwrap();
}

/// 真实事件验证草稿隔离、跨窗口刷新、英文与中文切换、自动选择、重载持久化和保存失败回滚。
/// 同时检查动态参数不会再被当作模板，以及嵌套错误保留用户路径与系统诊断。
#[gpui::test]
fn language_apply_refreshes_open_windows_and_persists(cx: &mut TestAppContext) {
    let root = std::env::temp_dir().join(format!("dinuosi-i18n-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let store = ProjectStore::new(root.join("recent.json"));
    store
        .save_preferences(&Preferences {
            language: "zh-CN".into(),
            ..Default::default()
        })
        .unwrap();
    let (desktop, manager) =
        cx.add_window_view(|window, cx| Desktop::new(store.clone(), None, window, cx));
    manager.run_until_parked();
    assert!(manager.debug_bounds("text-new-project-新建项目").is_some());
    let mut manager = manager.clone();
    let (settings, visual) = cx.add_window_view(|window, cx| {
        Settings::new(
            desktop.downgrade(),
            store.clone(),
            None,
            Tab::Language,
            window,
            cx,
        )
    });
    visual.simulate_resize(size(px(860.0), px(600.0)));
    visual.run_until_parked();
    for (id, selector, locale, manager_text, apply_text) in [
        (
            "en",
            "language-en",
            "en",
            "text-new-project-New project",
            "text-settings-apply-Apply",
        ),
        (
            "zh-CN",
            "language-zh-CN",
            "zh-CN",
            "text-new-project-新建项目",
            "text-settings-apply-应用",
        ),
    ] {
        let previous = store.load_preferences().unwrap().language;
        click(visual, selector);
        assert_eq!(store.load_preferences().unwrap().language, previous);
        click(visual, "settings-apply");
        manager.run_until_parked();
        assert_eq!(visual.read(i18n::locale), locale);
        assert!(
            manager.debug_bounds(manager_text).is_some(),
            "{manager_text}"
        );
        assert!(visual.debug_bounds(apply_text).is_some(), "{apply_text}");
        assert_eq!(store.load_preferences().unwrap().language, id);
    }
    click(visual, "language-auto");
    click(visual, "settings-apply");
    assert_eq!(store.load_preferences().unwrap().language, "auto");
    assert_eq!(
        visual.read(i18n::locale),
        dinuosi_core::language::resolve("auto", sys_locale::get_locale().as_deref())
    );
    // 清除旧全局偏好后走启动载入路径，验证从磁盘恢复语言。
    let saved = store.load_preferences().unwrap();
    manager.update(|_, cx| {
        cx.remove_global::<preferences::UserPreferences>();
        preferences::initialize(&store, cx).unwrap();
    });
    assert_eq!(
        manager.read(i18n::locale),
        dinuosi_core::language::resolve(&saved.language, sys_locale::get_locale().as_deref())
    );

    visual.update(|_, cx| {
        i18n::apply("en", cx);
        assert_eq!(
            i18n::f(
                "dialogs",
                "Delete “{name}”?",
                &[("name", "中文 {name}".into())],
                cx
            ),
            "Delete “中文 {name}”?"
        );
        assert_eq!(
            i18n::error("无法打开项目 中文.dinuosi：项目名称不能为空", cx),
            "Unable to open project 中文.dinuosi: Project name cannot be empty"
        );
        assert_eq!(
            i18n::error("中文.png：无法解码画面：decoder failed", cx),
            "中文.png: Unable to decode frame: decoder failed"
        );
        assert_eq!(
            i18n::error("“New project”与“Open project”的快捷键冲突", cx),
            "Shortcut conflict between “New project” and “Open project”"
        );
        i18n::apply("zh-CN", cx);
        assert_eq!(
            i18n::error("无法打开项目 中文.dinuosi：项目名称不能为空", cx),
            "无法打开项目 中文.dinuosi：项目名称不能为空"
        );
        assert_eq!(
            i18n::error("“New project”与“Open project”的快捷键冲突", cx),
            "“新建项目”与“打开项目”的快捷键冲突"
        );
        let mut track = dinuosi_core::timeline::Timeline::default().tracks.remove(0);
        assert_eq!(crate::timeline_tracks::track_name(&track, cx), "主轨");
        i18n::apply("en", cx);
        assert_eq!(crate::timeline_tracks::track_name(&track, cx), "Main");
        track.custom_name = true;
        i18n::apply("en", cx);
        assert_eq!(crate::timeline_tracks::track_name(&track, cx), "主轨");
        let invalid = ProjectStore::new(root.join("blocker/recent.json"));
        std::fs::write(root.join("blocker"), b"file").unwrap();
        assert!(
            preferences::save(
                &invalid,
                Preferences {
                    language: "zh-CN".into(),
                    ..Default::default()
                },
                cx
            )
            .is_err()
        );
        assert_eq!(i18n::locale(cx), "en");
    });
    assert!(settings.read_with(visual, |s, _| s.error.is_none()));
    std::fs::remove_dir_all(root).unwrap();
}
