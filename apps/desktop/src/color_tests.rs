//! 调色检查器实际事件、持久化与历史回归。
use crate::view::Desktop;
use dinuosi_core::{ProjectStore, media::MediaKind};
use gpui::{Modifiers, MouseButton, TestAppContext, VisualTestContext, point, px, size};

/// 点击最新布局中的控件并等待绘制；目标缺失使测试失败，不直接调用编辑接口。
fn click(cx: &mut VisualTestContext, id: &'static str) {
    let p = cx.debug_bounds(id).expect(id).center();
    cx.simulate_click(p, Modifiers::none());
    cx.run_until_parked();
}
/// 用鼠标打开调色、拖动、输入、旁路和步骤，再检查磁盘结果与单次撤销；覆盖小窗口与音频入口。
#[gpui::test]
fn color_controls_drag_input_steps_and_undo(cx: &mut TestAppContext) {
    let root = std::env::temp_dir().join(format!("dinuosi-color-ui-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let store = ProjectStore::new(root.join("recent.json"));
    let entry = store.create("调色", &root).unwrap();
    let (desktop, cx) =
        cx.add_window_view(|w, cx| Desktop::new(store.clone(), Some(entry.clone()), w, cx));
    cx.simulate_resize(size(px(1440.0), px(900.0)));
    cx.run_until_parked();
    desktop.update_in(cx, |v, _, cx| {
        let before = v.timeline.model.clone();
        let id = v
            .timeline
            .model
            .insert(
                ("color.png".into(), MediaKind::Image),
                "图".into(),
                0,
                0,
                0..150,
                150,
            )
            .unwrap();
        v.timeline.selected = [id].into();
        v.commit_timeline(before, cx);
    });
    cx.run_until_parked();
    click(cx, "inspector-color-tab");
    assert!(cx.debug_bounds("color-stage-add").is_none());
    let original = store.load_timeline(&entry).unwrap();
    let b = cx.debug_bounds("color-slider-basic-0").unwrap();
    let from = b.center();
    let to = point(b.right() - px(5.0), from.y);
    cx.simulate_mouse_down(from, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_move(to, MouseButton::Left, Modifiers::none());
    cx.run_until_parked();
    assert_eq!(store.load_timeline(&entry).unwrap(), original);
    cx.simulate_mouse_up(to, MouseButton::Left, Modifiers::none());
    cx.run_until_parked();
    let changed = store.load_timeline(&entry).unwrap();
    assert!(changed.clips[0].color.stages[0].basic[0] > 3.0);
    cx.simulate_keystrokes("ctrl-z");
    cx.run_until_parked();
    assert_eq!(store.load_timeline(&entry).unwrap(), original);
    click(cx, "color-value-basic-0");
    cx.simulate_keystrokes("ctrl-a");
    cx.simulate_input("1.3");
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    assert_eq!(
        store.load_timeline(&entry).unwrap().clips[0].color.stages[0].basic[0],
        1.3
    );
    click(cx, "color-enabled");
    assert!(!store.load_timeline(&entry).unwrap().clips[0].color.enabled);
    click(cx, "color-enabled");
    click(cx, "color-steps");
    click(cx, "color-stage-add");
    assert_eq!(
        store.load_timeline(&entry).unwrap().clips[0]
            .color
            .stages
            .len(),
        2
    );
    click(cx, "color-stage-0");
    click(cx, "color-steps");
    click(cx, "color-tool-2");
    let b = cx.debug_bounds("color-curve-graph").unwrap();
    cx.simulate_click(
        point(
            b.left() + b.size.width * 0.5,
            b.top() + b.size.height * 0.25,
        ),
        Modifiers::none(),
    );
    cx.run_until_parked();
    assert_eq!(
        store.load_timeline(&entry).unwrap().clips[0].color.stages[0].curves[0].len(),
        3
    );
    for language in ["en", "zh-CN"] {
        desktop.update_in(cx, |_, _, cx| crate::i18n::apply(language, cx));
        cx.simulate_resize(size(px(1040.0), px(640.0)));
        cx.run_until_parked();
        assert!(cx.debug_bounds("color-parameters").unwrap().size.height > px(60.0));
        let graph = cx.debug_bounds("color-curve-graph").unwrap();
        assert!(graph.right() <= px(1040.0));
    }
    click(cx, "color-tool-3");
    for index in ["color-wheel-0", "color-wheel-1", "color-wheel-2"] {
        let bounds = cx.debug_bounds(index).unwrap();
        assert!(bounds.right() <= px(1040.0));
        assert!(bounds.bottom() <= cx.debug_bounds("color-parameters").unwrap().bottom());
    }
    let slider = cx.debug_bounds("color-slider-wheel-0-2").unwrap();
    let number = cx.debug_bounds("color-value-wheel-0-2").unwrap();
    assert!(number.top() >= slider.bottom());
    assert!(number.bottom() <= cx.debug_bounds("color-parameters").unwrap().bottom());
    let bounds = cx.debug_bounds("color-wheel-1").unwrap();
    let from = bounds.center();
    let to = point(from.x + bounds.size.width * 0.25, from.y);
    cx.simulate_mouse_down(from, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_move(to, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_up(to, MouseButton::Left, Modifiers::none());
    cx.run_until_parked();
    assert!(store.load_timeline(&entry).unwrap().clips[0].color.stages[0].wheels[1][0] > 40.0);
    click(cx, "color-tool-4");
    click(cx, "color-mask-enabled");
    assert!(
        store.load_timeline(&entry).unwrap().clips[0].color.stages[0]
            .mask
            .enabled
    );
    desktop.update_in(cx, |v, _, cx| {
        let id = v
            .timeline
            .model
            .insert(
                ("color.wav".into(), MediaKind::Audio),
                "声音".into(),
                0,
                0,
                0..150,
                150,
            )
            .unwrap();
        v.timeline.selected = [id].into();
        cx.notify();
    });
    cx.run_until_parked();
    assert!(desktop.read_with(cx, |v, _| v.color_clip().is_none()));
    // GPUI 0.2.2 的 debug_bounds 保留已移除控件，改为检查当前主对象与新绘制的音频面板。
    assert!(cx.debug_bounds("audio-inspector").is_some());
    std::fs::remove_dir_all(root).unwrap();
}

/// 鼠标验证调色关键帧、自动数值编辑、导航、撤销、关联主对象与最小窗口；磁盘值必须一致。
#[gpui::test]
fn color_keyframe_controls_navigation_and_primary_clip(cx: &mut TestAppContext) {
    use dinuosi_core::{color::Control as C, keyframes::KeyframeKind as K};
    let root = std::env::temp_dir().join(format!("dinuosi-color-keys-ui-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let store = ProjectStore::new(root.join("recent.json"));
    let entry = store.create("调色动画", &root).unwrap();
    let (desktop, cx) =
        cx.add_window_view(|w, cx| Desktop::new(store.clone(), Some(entry.clone()), w, cx));
    cx.simulate_resize(size(px(1440.0), px(900.0)));
    desktop.update_in(cx, |v, _, cx| {
        let id = v
            .timeline
            .model
            .insert(
                ("color.png".into(), MediaKind::Image),
                "图".into(),
                0,
                0,
                0..150,
                150,
            )
            .unwrap();
        let audio = v
            .timeline
            .model
            .insert(
                ("sound.wav".into(), MediaKind::Audio),
                "声音".into(),
                0,
                0,
                0..150,
                150,
            )
            .unwrap();
        v.timeline.primary_clip = Some(id.clone());
        v.timeline.selected = [id, audio].into();
        cx.global_mut::<crate::preferences::UserPreferences>()
            .0
            .auto_keyframe = true;
        cx.notify();
    });
    cx.run_until_parked();
    click(cx, "inspector-color-tab");
    click(cx, "keyframe-Color(0, Basic(0))-0-empty");
    assert_eq!(
        store
            .load_timeline(&entry)
            .unwrap()
            .clips
            .iter()
            .map(|c| c.keyframes.len())
            .sum::<usize>(),
        1
    );
    desktop.update_in(cx, |v, _, cx| v.seek_timeline(1.0, true, cx));
    cx.run_until_parked();
    click(cx, "color-value-basic-0");
    cx.simulate_keystrokes("ctrl-a");
    cx.simulate_input("2.0");
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    let t = store.load_timeline(&entry).unwrap();
    let image = t.clips.iter().find(|c| c.kind == MediaKind::Image).unwrap();
    assert_eq!(
        image.keyframe_frames(Some(K::Color(0, C::Basic(0)))),
        [0, 30].into()
    );
    assert_eq!(image.color_at(15.0).stages[0].basic[0], 1.0);
    assert!(
        t.clips
            .iter()
            .find(|c| c.kind == MediaKind::Audio)
            .unwrap()
            .keyframes
            .is_empty()
    );
    click(cx, "keyframe-Color(0, Basic(0))--1-empty");
    assert_eq!(desktop.read_with(cx, |v, _| v.keyframe_frame()), 0);
    click(cx, "keyframe-Color(0, Basic(0))-1-empty");
    assert_eq!(desktop.read_with(cx, |v, _| v.keyframe_frame()), 30);
    cx.simulate_keystrokes("ctrl-z");
    cx.run_until_parked();
    assert_eq!(
        store
            .load_timeline(&entry)
            .unwrap()
            .clips
            .iter()
            .map(|c| c.keyframes.len())
            .sum::<usize>(),
        1
    );
    cx.simulate_keystrokes("ctrl-shift-z");
    cx.run_until_parked();
    assert_eq!(store.load_timeline(&entry).unwrap(), t);
    click(cx, "keyframe-curves");
    assert_eq!(desktop.read_with(cx, |v, _| v.curve_channel()), 6);
    click(cx, "curve-node-0");
    click(cx, "curve-easing-1");
    let eased = store.load_timeline(&entry).unwrap();
    let image = eased
        .clips
        .iter()
        .find(|c| c.kind == MediaKind::Image)
        .unwrap();
    assert!(image.color_at(15.0).stages[0].basic[0] < 1.0);
    cx.simulate_keystrokes("ctrl-z");
    cx.run_until_parked();
    click(cx, "keyframe-curves");
    click(cx, "color-tone-details");
    assert!(desktop.read_with(cx, |v, _| v.timeline.color.tones_open));
    click(cx, "color-steps");
    click(cx, "color-stage-add");
    assert_eq!(
        store
            .load_timeline(&entry)
            .unwrap()
            .clips
            .iter()
            .map(|c| c.keyframes.len())
            .sum::<usize>(),
        2
    );
    click(cx, "color-stage-0");
    click(cx, "color-stage-remove");
    assert!(
        store
            .load_timeline(&entry)
            .unwrap()
            .clips
            .iter()
            .all(|c| c.keyframes.is_empty())
    );
    cx.simulate_resize(size(px(1024.0), px(680.0)));
    cx.run_until_parked();
    let slider = cx.debug_bounds("color-slider-basic-0").unwrap();
    let keys = cx
        .debug_bounds("keyframe-Color(0, Basic(0))-0-empty")
        .unwrap();
    assert!(slider.size.width > px(20.0));
    assert!(keys.right() <= px(1024.0));
    std::fs::remove_dir_all(root).unwrap();
}

/// 关闭自动加帧，在真实滑块上跨越曝光范围再返回中点，断言整条动画、磁盘和撤销历史未改变。
/// 同一拖动最终停在新值时只保存一次，撤销必须恢复按下前的完整关键帧。
#[gpui::test]
fn color_drag_roundtrip_preserves_animation_snapshot(cx: &mut TestAppContext) {
    use dinuosi_core::{color::Control as C, keyframes::KeyframeKind as K};
    let root = std::env::temp_dir().join(format!("dinuosi-color-drag-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let store = ProjectStore::new(root.join("recent.json"));
    let entry = store.create("调色拖动", &root).unwrap();
    let (desktop, cx) =
        cx.add_window_view(|w, cx| Desktop::new(store.clone(), Some(entry.clone()), w, cx));
    cx.simulate_resize(size(px(1440.0), px(900.0)));
    desktop.update_in(cx, |v, _, cx| {
        let before = v.timeline.model.clone();
        let id = v
            .timeline
            .model
            .insert(
                ("color.png".into(), MediaKind::Image),
                "图".into(),
                0,
                0,
                0..90,
                90,
            )
            .unwrap();
        let ids = [id.clone()].into();
        v.timeline.model.clips[0].color.stages[0].basic[0] = -4.0;
        v.timeline
            .model
            .toggle_keyframe(&ids, K::Color(0, C::Basic(0)), 0);
        let mut grade = v.timeline.model.clips[0].color_at(60.0).into_owned();
        grade.stages[0].basic[0] = 4.0;
        v.timeline
            .model
            .set_color_at(&ids, &grade, 60, true)
            .unwrap();
        v.timeline.selected = ids;
        v.timeline.primary_clip = Some(id);
        v.session.timeline_position = 1.0;
        cx.global_mut::<crate::preferences::UserPreferences>()
            .0
            .auto_keyframe = false;
        v.commit_timeline(before, cx);
    });
    cx.run_until_parked();
    click(cx, "inspector-color-tab");
    let original = store.load_timeline(&entry).unwrap();
    let b = cx.debug_bounds("color-slider-basic-0").unwrap();
    let midpoint = b.center();
    let edge = point(b.right(), midpoint.y);
    cx.simulate_mouse_down(midpoint, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_move(edge, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_move(midpoint, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_up(midpoint, MouseButton::Left, Modifiers::none());
    cx.run_until_parked();
    assert_eq!(
        desktop.read_with(cx, |v, _| v.timeline.model.clone()),
        original
    );
    assert_eq!(store.load_timeline(&entry).unwrap(), original);
    let target = point(b.left() + b.size.width * 0.6, midpoint.y);
    cx.simulate_mouse_down(midpoint, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_move(edge, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_move(target, MouseButton::Left, Modifiers::none());
    assert_eq!(store.load_timeline(&entry).unwrap(), original);
    cx.simulate_mouse_up(target, MouseButton::Left, Modifiers::none());
    cx.run_until_parked();
    let edited = store.load_timeline(&entry).unwrap();
    assert!((edited.clips[0].keyframes[0].values[0] + 3.0).abs() < 1e-5);
    assert_eq!(edited.clips[0].keyframes[1].values[0], 5.0);
    cx.simulate_keystrokes("ctrl-z");
    cx.run_until_parked();
    assert_eq!(store.load_timeline(&entry).unwrap(), original);
    std::fs::remove_dir_all(root).unwrap();
}
