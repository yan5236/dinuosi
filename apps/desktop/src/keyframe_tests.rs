//! GPUI 指针回归：关键帧增删、类型导航、自动编辑、保存与设置草稿隔离。
use crate::{
    preferences::UserPreferences,
    settings::{Settings, Tab},
    view::Desktop,
};
use dinuosi_core::{
    ProjectStore, keyframes::KeyframeKind as K, media::MediaKind, timeline::Property,
};
use gpui::{Modifiers, TestAppContext, px, size};
/// 单次点击使用最新布局，运行至空闲后检查业务状态，避免凭旧坐标触发不同控件。
fn click(cx: &mut gpui::VisualTestContext, selector: &'static str) {
    let p = cx.debug_bounds(selector).expect(selector).center();
    cx.simulate_click(p, Modifiers::none());
    cx.run_until_parked();
}
/// 验证检查器空实心切换、下一帧自动坐标、同帧缩放、同类型导航以及撤销/重做和磁盘一致。
#[gpui::test]
fn keyframe_inspector_and_navigation(cx: &mut TestAppContext) {
    let root = std::env::temp_dir().join(format!("dinuosi-keyframe-ui-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let store = ProjectStore::new(root.join("recent.json"));
    let entry = store.create("关键帧", &root).unwrap();
    let (desktop, cx) = cx
        .add_window_view(|window, cx| Desktop::new(store.clone(), Some(entry.clone()), window, cx));
    cx.simulate_resize(size(px(1440.0), px(900.0)));
    desktop.update_in(cx, |v, _, cx| {
        let id = v
            .timeline
            .model
            .insert(
                ("keys.png".into(), MediaKind::Image),
                "图像".into(),
                0,
                30,
                0..150,
                150,
            )
            .unwrap();
        v.timeline.selected = [id].into();
        v.session.timeline_position = 1.0;
        v.commit_timeline(Default::default(), cx);
    });
    cx.run_until_parked();
    click(cx, "keyframe-Position-0-empty");
    assert_eq!(
        store.load_timeline(&entry).unwrap().clips[0]
            .keyframes
            .len(),
        1
    );
    desktop.update_in(cx, |v, _, cx| v.seek_timeline(31.0 / 30.0, true, cx));
    cx.run_until_parked();
    click(cx, "inspector-step-X-1");
    assert_eq!(
        store.load_timeline(&entry).unwrap().clips[0]
            .keyframes
            .len(),
        2
    );
    click(cx, "keyframe-Scale-0-empty");
    let saved = store.load_timeline(&entry).unwrap();
    assert_eq!(saved.clips[0].keyframes.len(), 3);
    assert_eq!(saved.clips[0].keyframe_frames(None), [30, 31].into());
    click(cx, "keyframe-Position--1-empty");
    assert_eq!(desktop.read_with(cx, |v, _| v.keyframe_frame()), 30);
    click(cx, "keyframe-Position-1-empty");
    assert_eq!(desktop.read_with(cx, |v, _| v.keyframe_frame()), 31);
    click(cx, "keyframe-Position-0-filled");
    assert_eq!(
        store.load_timeline(&entry).unwrap().clips[0].keyframe_frames(Some(K::Position)),
        [30].into()
    );
    desktop.update_in(cx, |v, _, cx| v.timeline_history(false, cx));
    cx.run_until_parked();
    assert_eq!(store.load_timeline(&entry).unwrap(), saved);
    desktop.update_in(cx, |v, _, cx| v.timeline_history(true, cx));
    cx.run_until_parked();
    assert_eq!(
        store.load_timeline(&entry).unwrap().clips[0]
            .keyframes
            .len(),
        2
    );
    for language in ["zh-CN", "en"] {
        desktop.update_in(cx, |_, _, cx| {
            crate::preferences::save(
                &store,
                dinuosi_core::Preferences {
                    language: language.into(),
                    ..Default::default()
                },
                cx,
            )
            .unwrap();
        });
        for dimensions in [(1040.0, 640.0), (1440.0, 900.0)] {
            cx.simulate_resize(size(px(dimensions.0), px(dimensions.1)));
            cx.run_until_parked();
            let button = cx.debug_bounds("keyframe-Scale-0-filled").unwrap();
            let value = cx.debug_bounds("inspector-value-Scale").unwrap();
            assert!(button.left() >= value.right());
            assert!(button.right() <= px(dimensions.0));
            let x = cx.debug_bounds("inspector-value-X").unwrap();
            let y = cx.debug_bounds("inspector-value-Y").unwrap();
            let key = cx.debug_bounds("keyframe-Position-0-empty").unwrap();
            if dimensions.0 * 0.259 < 360.0 {
                assert!(x.center().y < y.center().y);
            } else {
                assert_eq!(x.center().y, y.center().y);
            }
            assert_eq!(x.size.width, px(59.0));
            assert_eq!(y.size.width, px(59.0));
            let label = cx.debug_bounds("inspector-position-label").unwrap();
            let axis = cx.debug_bounds("inspector-coordinate-X-label").unwrap();
            assert!(axis.left() - label.right() >= px(6.0));
            assert!(x.left() - axis.right() >= px(6.0));
            assert!(key.left() >= y.right(), "{language}: key={key:?}, y={y:?}");
            assert!(key.right() <= px(dimensions.0), "{language}: key={key:?}");
        }
    }
    std::fs::remove_dir_all(root).unwrap();
}
/// 点击设置开关只更新草稿；应用后持久化全局偏好并改变帧间参数编辑规则。
#[gpui::test]
fn automatic_keyframes_setting_applies_after_save(cx: &mut TestAppContext) {
    let root =
        std::env::temp_dir().join(format!("dinuosi-keyframe-settings-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let store = ProjectStore::new(root.join("recent.json"));
    let entry = store.create("设置", &root).unwrap();
    let mut context = cx.clone();
    let (desktop, editor) =
        cx.add_window_view(|w, cx| Desktop::new(store.clone(), Some(entry.clone()), w, cx));
    editor.simulate_resize(size(px(1440.0), px(900.0)));
    desktop.update_in(editor, |v, _, cx| {
        let id = v
            .timeline
            .model
            .insert(
                ("keys.png".into(), MediaKind::Image),
                "图片".into(),
                0,
                0,
                0..150,
                150,
            )
            .unwrap();
        v.timeline.selected = [id].into();
        v.timeline
            .model
            .toggle_keyframe(&v.timeline.selected, K::Position, 0);
        v.timeline
            .model
            .set_property_at(&v.timeline.selected, Property::X, 100.0, 30, true)
            .unwrap();
        v.commit_timeline(Default::default(), cx);
    });
    editor.run_until_parked();
    let (settings, visual) = context.add_window_view(|w, cx| {
        Settings::new(
            desktop.downgrade(),
            store.clone(),
            None,
            Tab::General,
            w,
            cx,
        )
    });
    visual.simulate_resize(size(px(860.0), px(600.0)));
    visual.run_until_parked();
    click(visual, "auto-keyframe");
    assert!(!settings.read_with(visual, |v, _| v.draft.auto_keyframe));
    assert!(visual.read(|cx| cx.global::<UserPreferences>().0.auto_keyframe));
    assert!(store.load_preferences().unwrap().auto_keyframe);
    click(visual, "settings-apply");
    assert!(!store.load_preferences().unwrap().auto_keyframe);
    assert!(!visual.read(|cx| cx.global::<UserPreferences>().0.auto_keyframe));
    desktop.update_in(editor, |v, _, cx| {
        v.seek_timeline(0.5, true, cx);
        v.change_property(Property::X, 60.0, cx);
    });
    editor.run_until_parked();
    let saved = store.load_timeline(&entry).unwrap();
    assert_eq!(saved.clips[0].keyframes.len(), 2);
    assert_eq!(saved.clips[0].transform_at(0.0).x, 10.0);
    assert_eq!(saved.clips[0].transform_at(30.0).x, 110.0);
    std::fs::remove_dir_all(root).unwrap();
}
/// 通过实际 GPUI 鼠标命中验证首帧中心对齐、左右键、菜单隔离、整组删除与单次撤销/重做。
/// 横向滚动后命中仍映射到原项目帧；键盘 Enter 删除最后一项时保留静态参数且不删除素材。
#[gpui::test]
fn timeline_keyframe_marker_seek_menu_and_history(cx: &mut TestAppContext) {
    use gpui::MouseButton;
    let root = std::env::temp_dir().join(format!("dinuosi-marker-ui-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let store = ProjectStore::new(root.join("recent.json"));
    let entry = store.create("标记", &root).unwrap();
    let (desktop, cx) =
        cx.add_window_view(|w, cx| Desktop::new(store.clone(), Some(entry.clone()), w, cx));
    cx.simulate_resize(size(px(1440.0), px(900.0)));
    let id = desktop.update_in(cx, |v, _, cx| {
        let id = v
            .timeline
            .model
            .insert(
                ("markers.png".into(), MediaKind::Image),
                "图片".into(),
                0,
                30,
                0..150,
                150,
            )
            .unwrap();
        v.timeline.selected = [id.clone()].into();
        v.timeline
            .model
            .toggle_keyframe(&v.timeline.selected, K::Position, 30);
        v.timeline
            .model
            .toggle_keyframe(&v.timeline.selected, K::Scale, 30);
        v.timeline
            .model
            .set_property_at(&v.timeline.selected, Property::X, 120.0, 90, true)
            .unwrap();
        v.zoom = 0.5;
        v.session.timeline_position = 1.5;
        v.commit_timeline(Default::default(), cx);
        id
    });
    cx.run_until_parked();
    let start: &'static str = Box::leak(format!("timeline-keyframe-{id}-30").into_boxed_str());
    let end: &'static str = Box::leak(format!("timeline-keyframe-{id}-90").into_boxed_str());
    let saved = store.load_timeline(&entry).unwrap();
    for (selector, frame) in [(start, 30), (end, 90)] {
        let mark = cx.debug_bounds(selector).unwrap();
        let expected = desktop.read_with(cx, |v, _| {
            v.timeline.bounds.left()
                + px(v.timeline.header
                    + frame as f32 / v.timeline.model.fps as f32 * v.pixels_per_second()
                    - v.timeline.scroll_x)
        });
        assert!(
            (f32::from(mark.center().x - expected)).abs() <= 0.5,
            "frame={frame}, marker={mark:?}, expected={expected:?}"
        );
        click(cx, selector);
        assert_eq!(desktop.read_with(cx, |v, _| v.keyframe_frame()), frame);
        assert!(desktop.read_with(cx, |v, _| v.timeline.gesture.is_none()));
        assert_eq!(store.load_timeline(&entry).unwrap(), saved);
    }
    desktop.update_in(cx, |v, _, cx| {
        v.timeline.scroll_x = v.pixels_per_second() * 1.4;
        cx.notify();
    });
    cx.run_until_parked();
    click(cx, end);
    assert_eq!(desktop.read_with(cx, |v, _| v.keyframe_frame()), 90);
    let point = cx.debug_bounds(end).unwrap().center();
    cx.simulate_mouse_down(point, MouseButton::Right, Modifiers::none());
    cx.simulate_mouse_up(point, MouseButton::Right, Modifiers::none());
    cx.run_until_parked();
    assert_eq!(
        desktop.read_with(cx, |v, _| v.timeline.keyframe_menu.as_ref().unwrap().frame),
        90
    );
    desktop.update_in(cx, |v, _, cx| {
        v.timeline.keyframe_menu.as_mut().unwrap().position = gpui::point(px(1439.0), px(899.0));
        cx.notify();
    });
    cx.run_until_parked();
    let menu = cx.debug_bounds("timeline-keyframe-menu").unwrap();
    assert!(menu.right() <= px(1440.0) && menu.bottom() <= px(900.0));
    cx.simulate_keystrokes("delete");
    cx.run_until_parked();
    assert_eq!(store.load_timeline(&entry).unwrap(), saved);
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    assert!(desktop.read_with(cx, |v, _| v.timeline.keyframe_menu.is_none()));
    desktop.update_in(cx, |v, _, cx| {
        v.timeline.scroll_x = 0.0;
        cx.notify();
    });
    cx.run_until_parked();
    let point = cx.debug_bounds(start).unwrap().center();
    cx.simulate_mouse_down(point, MouseButton::Right, Modifiers::none());
    cx.simulate_mouse_up(point, MouseButton::Right, Modifiers::none());
    cx.run_until_parked();
    click(cx, "timeline-keyframe-delete");
    let deleted = store.load_timeline(&entry).unwrap();
    assert_eq!(deleted.clips[0].keyframes.len(), 1);
    assert_eq!(deleted.clips[0].keyframes[0].frame, 60);
    assert_eq!(deleted.clips.len(), 1);
    assert_eq!(desktop.read_with(cx, |v, _| v.keyframe_frame()), 90);
    cx.simulate_keystrokes("ctrl-z");
    cx.run_until_parked();
    assert_eq!(store.load_timeline(&entry).unwrap(), saved);
    cx.simulate_keystrokes("ctrl-shift-z");
    cx.run_until_parked();
    assert_eq!(store.load_timeline(&entry).unwrap(), deleted);
    let point = cx.debug_bounds(end).unwrap().center();
    cx.simulate_mouse_down(point, MouseButton::Right, Modifiers::none());
    cx.simulate_mouse_up(point, MouseButton::Right, Modifiers::none());
    cx.run_until_parked();
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    let last = store.load_timeline(&entry).unwrap();
    assert_eq!(last.clips.len(), 1);
    assert!(last.clips[0].keyframes.is_empty());
    assert_eq!(last.clips[0].transform.x, 120.0);
    std::fs::remove_dir_all(root).unwrap();
}
