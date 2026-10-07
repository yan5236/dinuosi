//! 素材拖动显示模式：下拉草稿与应用隔离，真实指针拖动验证原块、浮块及取消恢复。
use crate::{
    preferences::UserPreferences,
    settings::{Settings, Tab},
    view::Desktop,
};
use dinuosi_core::{ProjectStore, media::MediaKind};
use gpui::{Modifiers, MouseButton, TestAppContext, point, px, size};

/// 点击两项下拉与应用按钮，验证默认跟随、未应用隔离、保留模式及切回跟随模式的持久化。
/// 用真实多选拖动检查原块/把手的显示、根层浮块及取消后的恢复，拖动中不得修改模型或磁盘。
#[gpui::test]
fn clip_drag_mode_applies_and_controls_original_blocks(cx: &mut TestAppContext) {
    let root = std::env::temp_dir().join(format!("dinuosi-drag-mode-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let store = ProjectStore::new(root.join("recent.json"));
    let entry = store.create("拖动模式", &root).unwrap();
    let mut settings_context = cx.clone();
    let (desktop, editor) = cx
        .add_window_view(|window, cx| Desktop::new(store.clone(), Some(entry.clone()), window, cx));
    editor.simulate_resize(size(px(1440.0), px(900.0)));
    editor.run_until_parked();
    let ids = desktop.update_in(editor, |v, _, cx| {
        v.zoom = 0.5;
        let ids: Vec<_> = [60, 240]
            .into_iter()
            .map(|start| {
                v.timeline
                    .model
                    .insert(
                        ("drag.png".into(), MediaKind::Image),
                        "图片".into(),
                        0,
                        start,
                        0..60,
                        60,
                    )
                    .unwrap()
            })
            .collect();
        v.commit_timeline(Default::default(), cx);
        ids
    });
    editor.run_until_parked();
    let (settings, visual) = settings_context.add_window_view(|window, cx| {
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
    assert!(!settings.read_with(visual, |v, _| v.draft.keep_dragged_clip));
    let trigger = visual.debug_bounds("clip-drag-mode").unwrap().center();
    visual.simulate_click(trigger, Modifiers::none());
    visual.run_until_parked();
    assert!(visual.debug_bounds("clip-drag-follow").is_some());
    let keep = visual.debug_bounds("clip-drag-keep").unwrap().center();
    assert!(keep.y > trigger.y);
    visual.simulate_click(keep, Modifiers::none());
    visual.run_until_parked();
    assert!(settings.read_with(visual, |v, _| v.draft.keep_dragged_clip));
    assert!(!store.load_preferences().unwrap().keep_dragged_clip);
    assert!(!visual.read(|cx| cx.global::<UserPreferences>().0.keep_dragged_clip));

    let selectors: Vec<&'static str> = ids
        .iter()
        .map(|id| &*Box::leak(format!("clip-{id}").into_boxed_str()))
        .collect();
    let first = editor.debug_bounds(selectors[0]).unwrap().center();
    let second = editor.debug_bounds(selectors[1]).unwrap().center();
    editor.simulate_click(first, Modifiers::none());
    editor.simulate_click(second, Modifiers::control());
    editor.run_until_parked();
    assert_eq!(
        desktop.read_with(editor, |v, _| v.timeline.selected.len()),
        2
    );
    let before = store.load_timeline(&entry).unwrap();
    for keep in [false, true] {
        if keep {
            let apply = visual.debug_bounds("settings-apply").unwrap().center();
            visual.simulate_click(apply, Modifiers::none());
            visual.run_until_parked();
            editor.run_until_parked();
            assert!(store.load_preferences().unwrap().keep_dragged_clip);
        }
        editor.simulate_mouse_down(first, MouseButton::Left, Modifiers::none());
        editor.run_until_parked();
        assert!(
            editor.debug_bounds(selectors[0]).is_some(),
            "按下未拖动不能隐藏原块"
        );
        editor.simulate_mouse_move(
            first + point(px(45.0), px(-10.0)),
            MouseButton::Left,
            Modifiers::none(),
        );
        editor.run_until_parked();
        // GPUI 会缓存已移除元素的 debug_bounds；使用首次在实际拖动期间出现的原块标记。
        for id in &ids {
            let selector = Box::leak(format!("drag-original-{id}").into_boxed_str());
            assert_eq!(editor.debug_bounds(selector).is_some(), keep);
        }
        assert!(editor.debug_bounds("timeline-drag-ghost").is_some());
        assert_eq!(
            desktop.read_with(editor, |v, _| v.timeline.model.clone()),
            before
        );
        assert_eq!(store.load_timeline(&entry).unwrap(), before);
        let cancel = desktop.read_with(editor, |v, _| {
            point(
                v.timeline.bounds.left() + px(10.0),
                v.timeline.bounds.top() + px(60.0),
            )
        });
        editor.simulate_mouse_move(cancel, MouseButton::Left, Modifiers::none());
        editor.simulate_mouse_up(cancel, MouseButton::Left, Modifiers::none());
        editor.run_until_parked();
        for selector in &selectors {
            assert!(editor.debug_bounds(selector).is_some());
        }
        assert!(desktop.read_with(editor, |v, _| v.timeline.gesture.is_none()));
        assert_eq!(store.load_timeline(&entry).unwrap(), before);
    }
    visual.simulate_click(trigger, Modifiers::none());
    visual.simulate_keystrokes("escape");
    visual.run_until_parked();
    assert!(settings.read_with(visual, |v, _| v.general_menu.is_none()));
    visual.simulate_click(trigger, Modifiers::none());
    visual.run_until_parked();
    let follow = visual.debug_bounds("clip-drag-follow").unwrap().center();
    visual.simulate_click(follow, Modifiers::none());
    visual.run_until_parked();
    let apply = visual.debug_bounds("settings-apply").unwrap().center();
    visual.simulate_click(apply, Modifiers::none());
    visual.run_until_parked();
    assert!(!store.load_preferences().unwrap().keep_dragged_clip);
    assert!(!visual.read(|cx| cx.global::<UserPreferences>().0.keep_dragged_clip));
    std::fs::remove_dir_all(root).unwrap();
}
