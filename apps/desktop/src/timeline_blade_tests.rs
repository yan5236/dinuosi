//! 通过真实 GPUI 点击、悬停和快捷键验证刀片工具，不绕过工作区事件路由。
use crate::{
    preferences::{self, UserPreferences},
    view::Desktop,
};
use dinuosi_core::{Preferences, ProjectStore, media::MediaKind};
use gpui::{Modifiers, TestAppContext, point, px, size};

/// 在真实设置视图点击下拉选项并应用，验证草稿不提前生效、两项可切换、Esc 收起和重开保存值。
#[gpui::test]
fn blade_mode_dropdown_applies_only_after_confirmation(cx: &mut TestAppContext) {
    let root = std::env::temp_dir().join(format!("dinuosi-blade-settings-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let store = ProjectStore::new(root.join("recent.json"));
    let (desktop, editor) =
        cx.add_window_view(|window, cx| Desktop::new(store.clone(), None, window, cx));
    editor.run_until_parked();
    let (settings, visual) = cx.add_window_view(|window, cx| {
        crate::settings::Settings::new(
            desktop.downgrade(),
            store.clone(),
            None,
            crate::settings::Tab::General,
            window,
            cx,
        )
    });
    visual.simulate_resize(size(px(860.0), px(600.0)));
    visual.run_until_parked();
    let trigger = visual.debug_bounds("blade-mode").unwrap().center();
    visual.simulate_click(trigger, Modifiers::none());
    visual.run_until_parked();
    let option = visual.debug_bounds("blade-mode-selected").unwrap().center();
    assert!(option.y > trigger.y);
    visual.simulate_click(option, Modifiers::none());
    visual.run_until_parked();
    assert!(settings.read_with(visual, |v, _| v.draft.blade_requires_selection));
    assert!(!visual.read(|cx| cx.global::<UserPreferences>().0.blade_requires_selection));
    assert!(!store.load_preferences().unwrap().blade_requires_selection);
    settings.update(visual, |v, cx| v.apply(cx));
    visual.run_until_parked();
    assert!(store.load_preferences().unwrap().blade_requires_selection);
    assert!(visual.read(|cx| cx.global::<UserPreferences>().0.blade_requires_selection));
    visual.simulate_click(trigger, Modifiers::none());
    visual.simulate_keystrokes("escape");
    assert!(!settings.read_with(visual, |v, _| v.blade_menu_open));
    visual.simulate_click(trigger, Modifiers::none());
    let option = visual.debug_bounds("blade-mode-direct").unwrap().center();
    visual.simulate_click(option, Modifiers::none());
    visual.run_until_parked();
    assert!(!settings.read_with(visual, |v, _| v.draft.blade_requires_selection));
    std::fs::remove_dir_all(root).unwrap();
}

/// 验证无需选择及先选择两次点击、虚线/光标、锁轨、工具切换、播放头切割和持久化撤销。
#[gpui::test]
fn blade_and_playhead_split_follow_real_pointer_and_keyboard_events(cx: &mut TestAppContext) {
    let root = std::env::temp_dir().join(format!("dinuosi-blade-ui-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let store = ProjectStore::new(root.join("recent.json"));
    let entry = store.create("刀片验证", &root).unwrap();
    let mut model = dinuosi_core::timeline::Timeline::default();
    model
        .insert(
            ("test.png".into(), MediaKind::Image),
            "图片".into(),
            0,
            0,
            0..150,
            150,
        )
        .unwrap();
    store.save_timeline(&entry, &model).unwrap();
    let (desktop, cx) = cx
        .add_window_view(|window, cx| Desktop::new(store.clone(), Some(entry.clone()), window, cx));
    cx.simulate_resize(size(px(1440.0), px(900.0)));
    cx.run_until_parked();
    desktop.update_in(cx, |v, _, cx| {
        v.zoom = 0.5;
        cx.notify();
    });
    cx.run_until_parked();
    let position = desktop.read_with(cx, |v, _| {
        let (top, h) = v.track_geometry(0);
        point(
            v.timeline.bounds.left() + px(v.timeline.header + 2.0 * v.pixels_per_second()),
            v.timeline.bounds.top() + px(top + h / 2.0),
        )
    });
    // 工具栏真实点击启用刀片，无选择也显示预览并直接切割。
    let button = cx.debug_bounds("timeline-razor").unwrap().center();
    cx.simulate_click(button, Modifiers::none());
    cx.simulate_mouse_move(position, None, Modifiers::none());
    assert!(cx.debug_bounds("blade-cursor").is_some());
    assert!(cx.debug_bounds("blade-cut-line").is_some());
    cx.simulate_click(position, Modifiers::none());
    cx.run_until_parked();
    assert_eq!(store.load_timeline(&entry).unwrap().clips.len(), 2);
    assert!(desktop.read_with(cx, |v, _| v.timeline.selected.is_empty()));
    cx.simulate_keystrokes("ctrl-z");
    cx.run_until_parked();
    assert_eq!(store.load_timeline(&entry).unwrap(), model);
    desktop.update(cx, |_, cx| {
        preferences::save(
            &store,
            Preferences {
                blade_requires_selection: true,
                ..Default::default()
            },
            cx,
        )
        .unwrap()
    });
    cx.run_until_parked();
    cx.simulate_mouse_move(position, None, Modifiers::none());
    desktop.read_with(cx, |v, cx| {
        assert!(cx.global::<UserPreferences>().0.blade_requires_selection);
        assert!(v.timeline.selected.is_empty());
        assert!(v.blade_cut_line(cx).is_none());
    });
    assert!(desktop.read_with(cx, |v, cx| v.blade_cut_line(cx).is_none()));
    cx.simulate_click(position, Modifiers::none());
    cx.run_until_parked();
    assert_eq!(store.load_timeline(&entry).unwrap().clips.len(), 1);
    assert!(cx.debug_bounds("blade-cut-line").is_some());
    cx.simulate_click(position, Modifiers::none());
    cx.run_until_parked();
    assert_eq!(store.load_timeline(&entry).unwrap().clips.len(), 2);
    cx.simulate_keystrokes("ctrl-z");
    cx.run_until_parked();
    let button = cx.debug_bounds("track-lock-0").unwrap().center();
    cx.simulate_click(button, Modifiers::none());
    cx.simulate_mouse_move(position, None, Modifiers::none());
    assert!(desktop.read_with(cx, |v, cx| v.blade_cut_line(cx).is_none()));
    let locked = store.load_timeline(&entry).unwrap();
    cx.simulate_click(position, Modifiers::none());
    cx.simulate_keystrokes("ctrl-b");
    cx.run_until_parked();
    assert_eq!(store.load_timeline(&entry).unwrap(), locked);
    let button = cx.debug_bounds("track-lock-0").unwrap().center();
    cx.simulate_click(button, Modifiers::none());
    cx.simulate_keystrokes("v");
    assert!(!desktop.read_with(cx, |v, _| v.timeline.blade));
    assert!(desktop.read_with(cx, |v, _| v.blade_cursor_overlay().is_none()));
    cx.simulate_keystrokes("c");
    assert!(desktop.read_with(cx, |v, _| v.timeline.blade));
    cx.simulate_click(position, Modifiers::none());
    cx.run_until_parked();
    // 标尺设置播放头，Ctrl+B 只切割选中素材并保存；一条历史恢复整次切割。
    let ruler = cx.debug_bounds("timeline-seek").unwrap();
    cx.simulate_click(point(position.x, ruler.center().y), Modifiers::none());
    desktop.read_with(cx, |v, _| {
        assert_eq!(v.timeline.model.frame(v.session.timeline_position), 60);
        assert!(v.session.drag.is_none());
        assert!(!v.timeline.model.tracks[0].locked);
        assert!(v.timeline.selected.contains(&model.clips[0].id));
    });
    cx.simulate_keystrokes("ctrl-b");
    cx.run_until_parked();
    assert_eq!(store.load_timeline(&entry).unwrap().clips.len(), 2);
    assert_eq!(desktop.read_with(cx, |v, _| v.timeline.selected.len()), 2);
    cx.simulate_keystrokes("ctrl-z");
    cx.run_until_parked();
    assert_eq!(store.load_timeline(&entry).unwrap(), model);
    cx.simulate_keystrokes("ctrl-y");
    cx.run_until_parked();
    assert_eq!(store.load_timeline(&entry).unwrap().clips.len(), 2);
    assert!(cx.read(|cx| cx.global::<UserPreferences>().0.blade_requires_selection));
    assert!(store.load_preferences().unwrap().blade_requires_selection);
    std::fs::remove_dir_all(root).unwrap();
}
