//! 曲线面板真实 GPUI 指针回归：选择、精确输入、手柄、保存、历史与布局。
use crate::view::Desktop;
use dinuosi_core::{
    ProjectStore, keyframes::KeyframeKind as K, media::MediaKind, timeline::Property,
};
use gpui::{Modifiers, MouseButton, TestAppContext, VisualTestContext, point, px, size};

/// 从最新布局点击控件并等到空闲，节点不存在时明确失败，不直接调用编辑业务接口。
fn click(cx: &mut VisualTestContext, selector: &'static str) {
    let position = cx.debug_bounds(selector).expect(selector).center();
    cx.simulate_click(position, Modifiers::none());
    cx.run_until_parked();
}

/// 验证预设求值、选择不寻址、精确节点输入、自由手柄、连续拖动只保存一次和单次历史。
#[gpui::test]
fn curve_controls_drag_inputs_and_history(cx: &mut TestAppContext) {
    let root = std::env::temp_dir().join(format!("dinuosi-curve-ui-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let store = ProjectStore::new(root.join("recent.json"));
    let entry = store.create("曲线", &root).unwrap();
    let (desktop, cx) =
        cx.add_window_view(|w, cx| Desktop::new(store.clone(), Some(entry.clone()), w, cx));
    cx.simulate_resize(size(px(1440.0), px(900.0)));
    cx.run_until_parked();
    assert!(cx.debug_bounds("keyframe-curve-panel").is_none());
    desktop.update_in(cx, |v, _, cx| {
        let before = v.timeline.model.clone();
        let id = v
            .timeline
            .model
            .insert(
                ("curve.png".into(), MediaKind::Image),
                "图像".into(),
                0,
                0,
                0..150,
                150,
            )
            .unwrap();
        v.timeline.selected = [id].into();
        v.timeline
            .model
            .toggle_keyframe(&v.timeline.selected, K::Scale, 0);
        v.timeline
            .model
            .set_property_at(&v.timeline.selected, Property::Scale, 200.0, 60, true)
            .unwrap();
        v.timeline
            .model
            .set_property_at(&v.timeline.selected, Property::Scale, 120.0, 120, true)
            .unwrap();
        v.zoom = 0.5;
        v.session.timeline_position = 1.0;
        v.commit_timeline(before, cx);
    });
    cx.run_until_parked();
    click(cx, "keyframe-curves");
    click(cx, "curve-node-0");
    assert_eq!(desktop.read_with(cx, |v, _| v.keyframe_frame()), 30);
    click(cx, "curve-easing-3");
    let eased = store.load_timeline(&entry).unwrap();
    assert!((eased.clips[0].transform_at(15.0).scale - 112.916193).abs() < 0.0001);
    click(cx, "curve-node-60");
    click(cx, "curve-frame");
    cx.simulate_keystrokes("ctrl-a");
    cx.simulate_input("65");
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    assert!(
        store.load_timeline(&entry).unwrap().clips[0]
            .keyframes
            .iter()
            .any(|k| k.frame == 65)
    );
    click(cx, "curve-value");
    cx.simulate_keystrokes("ctrl-a");
    cx.simulate_input("180.5");
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    let numbered = store.load_timeline(&entry).unwrap();
    assert_eq!(numbered.clips[0].transform_at(65.0).scale, 180.5);
    click(cx, "curve-frame");
    cx.simulate_keystrokes("ctrl-a");
    cx.simulate_input("120");
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    assert_eq!(store.load_timeline(&entry).unwrap(), numbered);
    click(cx, "curve-linked-false");
    let before = store.load_timeline(&entry).unwrap();
    let from = cx.debug_bounds("curve-out-65").unwrap().center();
    let to = point(from.x + px(20.0), from.y - px(18.0));
    cx.simulate_mouse_down(from, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_move(to, MouseButton::Left, Modifiers::none());
    cx.run_until_parked();
    assert_eq!(store.load_timeline(&entry).unwrap(), before);
    assert_ne!(
        desktop.read_with(cx, |v, _| v.timeline.model.clone()),
        before
    );
    cx.simulate_mouse_up(to, MouseButton::Left, Modifiers::none());
    cx.run_until_parked();
    let changed = store.load_timeline(&entry).unwrap();
    assert_ne!(changed, before);
    assert_eq!(
        changed.clips[0].curve_handle(K::Scale, 0, 65, true),
        before.clips[0].curve_handle(K::Scale, 0, 65, true)
    );
    assert_eq!(desktop.read_with(cx, |v, _| v.keyframe_frame()), 30);
    cx.simulate_keystrokes("ctrl-z");
    cx.run_until_parked();
    assert_eq!(store.load_timeline(&entry).unwrap(), before);
    cx.simulate_keystrokes("ctrl-shift-z");
    cx.run_until_parked();
    assert_eq!(store.load_timeline(&entry).unwrap(), changed);
    let from = cx.debug_bounds("curve-node-65").unwrap().center();
    let to = point(from.x + px(16.0), from.y + px(10.0));
    cx.simulate_mouse_down(from, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_move(to, MouseButton::Left, Modifiers::none());
    cx.run_until_parked();
    assert_ne!(
        desktop.read_with(cx, |v, _| v.timeline.model.clone()),
        changed
    );
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    cx.simulate_mouse_up(to, MouseButton::Left, Modifiers::none());
    cx.run_until_parked();
    assert_eq!(store.load_timeline(&entry).unwrap(), changed);
    assert_eq!(
        desktop.read_with(cx, |v, _| v.timeline.model.clone()),
        changed
    );
    click(cx, "curve-node-65");
    cx.simulate_keystrokes("delete");
    cx.run_until_parked();
    let removed = store.load_timeline(&entry).unwrap();
    assert_eq!(removed.clips.len(), 1);
    assert_eq!(removed.clips[0].keyframes.len(), 2);
    cx.simulate_keystrokes("ctrl-z");
    cx.run_until_parked();
    assert_eq!(store.load_timeline(&entry).unwrap(), changed);
    for language in ["en", "zh-CN"] {
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
        for (w, h) in [(1040.0, 640.0), (1440.0, 900.0)] {
            cx.simulate_resize(size(px(w), px(h)));
            cx.run_until_parked();
            click(cx, "curve-node-65");
            let panel = cx.debug_bounds("keyframe-curve-panel").unwrap();
            let graph = cx.debug_bounds("curve-graph").unwrap();
            let close = cx.debug_bounds("curve-close").unwrap();
            let timeline = cx.debug_bounds("timeline-panel").unwrap();
            assert!(panel.bottom() <= timeline.top());
            assert!(timeline.size.height >= px(100.0));
            assert!(close.right() <= px(w) && close.bottom() <= graph.top());
            assert!(cx.debug_bounds("curve-value").unwrap().right() <= close.left());
            let playhead =
                desktop.read_with(cx, |v, _| v.timeline.bounds.left() + px(v.timeline.header));
            assert!(f32::from(graph.left() - playhead).abs() < 0.01);
        }
    }
    let open_height = desktop.read_with(cx, |v, _| v.timeline.bounds.size.height);
    click(cx, "curve-close");
    assert!(
        !desktop.read_with(cx, |v, _| v.timeline.curves.open),
        "关闭必须更新会话状态"
    );
    // GPUI 0.2.2 的 debug_bounds 缓存不清理移除的元素，以真实会话及重新布局后的轨道高度验证关闭。
    assert!(desktop.read_with(cx, |v, _| v.timeline.bounds.size.height) > open_height + px(30.0));
    std::fs::remove_dir_all(root).unwrap();
}

/// 音频只显示音量属性，锁定节点不可改；dB 输入精确保存增益，锁定后预设和鼠标拖动不改变数据。
#[gpui::test]
fn curve_audio_and_locked_selection(cx: &mut TestAppContext) {
    let root = std::env::temp_dir().join(format!("dinuosi-curve-audio-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let store = ProjectStore::new(root.join("recent.json"));
    let entry = store.create("音量曲线", &root).unwrap();
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
                ("curve.wav".into(), MediaKind::Audio),
                "音频".into(),
                0,
                0,
                0..150,
                150,
            )
            .unwrap();
        v.timeline.selected = [id].into();
        v.zoom = 0.5;
        v.timeline
            .model
            .toggle_keyframe(&v.timeline.selected, K::Volume, 0);
        v.timeline
            .model
            .set_volume_at(&v.timeline.selected, 0.5, 60, true)
            .unwrap();
        v.commit_timeline(before, cx);
    });
    cx.run_until_parked();
    click(cx, "keyframe-curves");
    click(cx, "curve-node-60");
    assert!(cx.debug_bounds("curve-channel-5").is_some());
    assert!(cx.debug_bounds("curve-channel-0").is_none());
    click(cx, "curve-value");
    cx.simulate_keystrokes("ctrl-a");
    cx.simulate_input("-20 dB");
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    assert!((store.load_timeline(&entry).unwrap().clips[0].gain_at(60.0) - 0.1).abs() < 1e-8);
    desktop.update_in(cx, |v, _, cx| {
        let before = v.timeline.model.clone();
        v.timeline.model.clips[0].locked = true;
        v.commit_timeline(before, cx);
    });
    cx.run_until_parked();
    let before = store.load_timeline(&entry).unwrap();
    click(cx, "curve-easing-3");
    let from = cx.debug_bounds("curve-node-60").unwrap().center();
    let to = point(from.x + px(25.0), from.y - px(20.0));
    cx.simulate_mouse_down(from, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_move(to, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_up(to, MouseButton::Left, Modifiers::none());
    cx.run_until_parked();
    assert_eq!(store.load_timeline(&entry).unwrap(), before);
    std::fs::remove_dir_all(root).unwrap();
}

/// 旧项目保持存储和画面位置；Y 检查器、精确输入、微调、节点与切线拖动统一为向上正值。
#[gpui::test]
fn positive_y_controls_move_up_and_preserve_legacy_storage(cx: &mut TestAppContext) {
    let root = std::env::temp_dir().join(format!("dinuosi-y-ui-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let store = ProjectStore::new(root.join("recent.json"));
    let entry = store.create("Y 坐标", &root).unwrap();
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
                ("legacy.png".into(), MediaKind::Image),
                "旧图像".into(),
                0,
                0,
                0..150,
                150,
            )
            .unwrap();
        v.timeline.selected = [id].into();
        v.timeline
            .model
            .set_property(&v.timeline.selected, Property::Y, -100.0)
            .unwrap();
        v.timeline
            .model
            .toggle_keyframe(&v.timeline.selected, K::Position, 0);
        v.timeline
            .model
            .set_property_at(&v.timeline.selected, Property::Y, -200.0, 60, true)
            .unwrap();
        v.zoom = 0.5;
        v.session.timeline_position = 0.0;
        v.commit_timeline(before, cx);
    });
    cx.run_until_parked();
    let legacy = store.load_timeline(&entry).unwrap();
    assert_eq!(
        desktop.read_with(cx, |v, _| v.property_value(Property::Y)),
        100.0
    );
    click(cx, "keyframe-curves");
    click(cx, "curve-channel-2");
    let first = cx.debug_bounds("curve-node-0").unwrap().center();
    let last = cx.debug_bounds("curve-node-60").unwrap().center();
    assert!(last.y < first.y, "Y 正值较大的节点必须位于上方");
    assert_eq!(store.load_timeline(&entry).unwrap(), legacy);
    click(cx, "curve-node-0");
    click(cx, "curve-value");
    cx.simulate_keystrokes("ctrl-a");
    cx.simulate_input("150");
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    assert_eq!(
        store.load_timeline(&entry).unwrap().clips[0]
            .transform_at(0.0)
            .y,
        -150.0
    );
    assert_eq!(
        desktop.read_with(cx, |v, _| v.property_value(Property::Y)),
        150.0
    );
    let from = cx.debug_bounds("curve-node-0").unwrap().center();
    let to = point(from.x, from.y - px(10.0));
    cx.simulate_mouse_down(from, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_move(to, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_up(to, MouseButton::Left, Modifiers::none());
    cx.run_until_parked();
    assert!(
        store.load_timeline(&entry).unwrap().clips[0]
            .transform_at(0.0)
            .y
            < -150.0
    );
    click(cx, "curve-linked-false");
    let before = store.load_timeline(&entry).unwrap();
    let from = cx.debug_bounds("curve-out-0").unwrap().center();
    let to = point(from.x, from.y - px(10.0));
    cx.simulate_mouse_down(from, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_move(to, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_up(to, MouseButton::Left, Modifiers::none());
    cx.run_until_parked();
    let after = store.load_timeline(&entry).unwrap();
    assert!(
        after.clips[0].keyframes[0].curves[1]
            .unwrap()
            .outgoing
            .unwrap()[1]
            < before.clips[0].keyframes[0].curves[1]
                .unwrap()
                .outgoing
                .unwrap()[1]
    );
    click(cx, "inspector-value-Y");
    cx.simulate_keystrokes("ctrl-a");
    cx.simulate_input("300");
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    assert_eq!(
        store.load_timeline(&entry).unwrap().clips[0]
            .transform_at(0.0)
            .y,
        -300.0
    );
    click(cx, "inspector-step-Y-1");
    assert_eq!(
        store.load_timeline(&entry).unwrap().clips[0]
            .transform_at(0.0)
            .y,
        -301.0
    );
    cx.simulate_keystrokes("ctrl-z");
    cx.run_until_parked();
    assert_eq!(
        store.load_timeline(&entry).unwrap().clips[0]
            .transform_at(0.0)
            .y,
        -300.0
    );
    assert_eq!(Desktop::curve_value_label(2, -300.0), "300.00");
    let transform = store.load_timeline(&entry).unwrap().clips[0].transform_at(0.0);
    let canvas = gpui::Bounds::new(point(px(0.0), px(0.0)), size(px(640.0), px(360.0)));
    let geometry =
        crate::preview_geometry::Geometry::new(canvas, &transform, (640, 360), (1920, 1080));
    assert!(
        geometry.center.y < canvas.center().y,
        "预览选择框必须随正 Y 上移"
    );
    let mut pixels = [0, 0, 0, 255].repeat(8 * 8);
    let source = [255; 2 * 2 * 4];
    let transform = dinuosi_core::timeline::Transform {
        y: Desktop::property_coordinate(Property::Y, 2.0),
        ..Default::default()
    };
    dinuosi_core::composite::blend_pixels(
        &mut pixels,
        (8, 8),
        &source,
        (2, 2),
        &transform,
        (8, 8),
        (2.0, 2.0),
    )
    .unwrap();
    assert_eq!(pixels[11 * 4], 255, "导出共用像素合成路径必须随正 Y 上移");
    assert_eq!(pixels[(5 * 8 + 3) * 4], 0);
    std::fs::remove_dir_all(root).unwrap();
}
