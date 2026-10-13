//! 实际 GPUI 布局与鼠标/键盘回归：倍率、时长、变调、曲线拖动、保存、撤销和空选择。
use crate::view::Desktop;
use dinuosi_core::{ProjectStore, media::MediaKind};
use gpui::{Modifiers, MouseButton, TestAppContext, VisualTestContext, point, px, size};

/// 在真实布局中点击 debug_selector 目标并等待重绘，避免直接调用业务接口遗漏接线错误。
fn click(cx: &mut VisualTestContext, selector: &'static str) {
    let point = cx.debug_bounds(selector).unwrap().center();
    cx.simulate_click(point, Modifiers::none());
    cx.run_until_parked();
}

/// 验证无选择隐藏入口；常规倍率、时长、声音变调、滑块与曲线拖动均保存且可撤销，图片不显示变速。
#[gpui::test]
fn speed_controls_persist_and_hide_without_selection(cx: &mut TestAppContext) {
    let root = std::env::temp_dir().join(format!("dinuosi-speed-ui-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let store = ProjectStore::new(root.join("recent.json"));
    let entry = store.create("变速控件", &root).unwrap();
    let (desktop, cx) = cx
        .add_window_view(|window, cx| Desktop::new(store.clone(), Some(entry.clone()), window, cx));
    cx.simulate_resize(size(px(1440.0), px(900.0)));
    cx.run_until_parked();
    assert!(cx.debug_bounds("inspector-speed-tab").is_none());
    let id = desktop.update_in(cx, |v, _, cx| {
        let before = v.timeline.model.clone();
        let id = v
            .timeline
            .model
            .insert(
                ("speed-ui.mp4".into(), MediaKind::Video),
                "视频".into(),
                0,
                0,
                0..360,
                360,
            )
            .unwrap();
        v.timeline.selected.insert(id.clone());
        v.commit_timeline(before, cx);
        id
    });
    cx.run_until_parked();
    click(cx, "inspector-speed-tab");
    click(cx, "speed-rate");
    cx.simulate_keystrokes("ctrl-a");
    cx.simulate_input("2");
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    assert_eq!(store.load_timeline(&entry).unwrap().clips[0].duration, 180);
    click(cx, "speed-duration");
    cx.simulate_keystrokes("ctrl-a");
    cx.simulate_input("4");
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    assert_eq!(
        store.load_timeline(&entry).unwrap().clips[0]
            .speed
            .as_ref()
            .unwrap()
            .rate,
        3.0
    );
    click(cx, "speed-pitch");
    assert!(
        store.load_timeline(&entry).unwrap().clips[0]
            .speed
            .as_ref()
            .unwrap()
            .pitch
    );
    let before = store.load_timeline(&entry).unwrap();
    let slider = cx.debug_bounds("speed-slider").unwrap();
    let from = slider.center();
    let to = point(slider.left() + slider.size.width * 0.4, from.y);
    cx.simulate_mouse_down(from, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_move(to, MouseButton::Left, Modifiers::none());
    cx.run_until_parked();
    assert_eq!(store.load_timeline(&entry).unwrap(), before);
    cx.simulate_mouse_up(to, MouseButton::Left, Modifiers::none());
    cx.run_until_parked();
    assert!(
        (store.load_timeline(&entry).unwrap().clips[0]
            .speed
            .as_ref()
            .unwrap()
            .rate
            - 2.0)
            .abs()
            < 1e-5
    );
    click(cx, "timeline-undo");
    assert_eq!(store.load_timeline(&entry).unwrap(), before);
    cx.simulate_resize(size(px(1440.0), px(1200.0)));
    cx.run_until_parked();
    click(cx, "speed-curve-mode");
    click(cx, "speed-preset-1");
    let graph = cx.debug_bounds("speed-curve-graph").unwrap();
    let from = graph.center();
    let to = point(from.x, graph.top() + px(12.0));
    let original = store.load_timeline(&entry).unwrap();
    cx.simulate_mouse_down(from, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_move(to, MouseButton::Left, Modifiers::none());
    cx.run_until_parked();
    assert_eq!(store.load_timeline(&entry).unwrap(), original);
    cx.simulate_mouse_up(to, MouseButton::Left, Modifiers::none());
    cx.run_until_parked();
    let curved = store.load_timeline(&entry).unwrap();
    assert!(curved.clips[0].duration < 360);
    assert!(curved.clips[0].speed.as_ref().unwrap().curve[2][1] > 5.0);
    click(cx, "timeline-undo");
    assert_eq!(store.load_timeline(&entry).unwrap(), original);
    click(cx, "timeline-redo");
    assert_eq!(store.load_timeline(&entry).unwrap(), curved);
    click(cx, "speed-point-add");
    assert_eq!(
        store.load_timeline(&entry).unwrap().clips[0]
            .speed
            .as_ref()
            .unwrap()
            .curve
            .len(),
        6
    );
    click(cx, "speed-point-remove");
    assert_eq!(
        store.load_timeline(&entry).unwrap().clips[0]
            .speed
            .as_ref()
            .unwrap()
            .curve
            .len(),
        5
    );
    click(cx, "speed-reset");
    assert_eq!(store.load_timeline(&entry).unwrap().clips[0].duration, 360);
    cx.simulate_resize(size(px(1440.0), px(900.0)));
    cx.run_until_parked();
    click(cx, "timeline-lock");
    click(cx, "speed-pitch");
    assert!(
        !store.load_timeline(&entry).unwrap().clips[0]
            .speed
            .as_ref()
            .unwrap()
            .pitch
    );
    let old_tab = cx.debug_bounds("inspector-speed-tab").unwrap().center();
    desktop.update_in(cx, |v, _, cx| {
        v.timeline.selected.clear();
        v.timeline.inspector_tab = crate::timeline_state::InspectorTab::Basic;
        cx.notify();
    });
    cx.run_until_parked();
    // GPUI 0.2.2 不清理移除元素的 debug_bounds，点击旧入口坐标验证本帧已不存在它的事件处理器。
    cx.simulate_click(old_tab, Modifiers::none());
    cx.run_until_parked();
    assert!(desktop.read_with(cx, |v, _| v.speed_clip().is_none()
        && v.timeline.inspector_tab == crate::timeline_state::InspectorTab::Basic));
    desktop.update_in(cx, |v, _, cx| {
        let before = v.timeline.model.clone();
        let image = v
            .timeline
            .model
            .insert(
                ("still.png".into(), MediaKind::Image),
                "图片".into(),
                0,
                360,
                0..30,
                30,
            )
            .unwrap();
        v.timeline.selected = [image].into();
        v.commit_timeline(before, cx);
    });
    cx.run_until_parked();
    cx.simulate_click(old_tab, Modifiers::none());
    cx.run_until_parked();
    assert!(desktop.read_with(cx, |v, _| v.speed_clip().is_none()
        && v.timeline.inspector_tab == crate::timeline_state::InspectorTab::Basic));
    desktop.update_in(cx, |v, _, cx| {
        v.timeline.selected = [id].into();
        cx.notify();
    });
    cx.run_until_parked();
    assert!(cx.debug_bounds("inspector-speed-tab").is_some());
    std::fs::remove_dir_all(root).unwrap();
}

/// 验证连续添加在最小间距处停止；旧项目合法密集点仍能纵向拖动并保存、撤销，横坐标保持原值。
#[gpui::test]
fn dense_speed_points_preserve_order_and_drag_without_panicking(cx: &mut TestAppContext) {
    let root = std::env::temp_dir().join(format!("dinuosi-dense-speed-ui-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let store = ProjectStore::new(root.join("recent.json"));
    let entry = store.create("密集曲线", &root).unwrap();
    let (desktop, cx) = cx
        .add_window_view(|window, cx| Desktop::new(store.clone(), Some(entry.clone()), window, cx));
    cx.simulate_resize(size(px(1440.0), px(1200.0)));
    desktop.update_in(cx, |v, _, cx| {
        let before = v.timeline.model.clone();
        let id = v
            .timeline
            .model
            .insert(
                ("dense-speed.mp4".into(), MediaKind::Video),
                "视频".into(),
                0,
                0,
                0..360,
                360,
            )
            .unwrap();
        v.timeline.selected = [id].into();
        v.commit_timeline(before, cx);
    });
    cx.run_until_parked();
    click(cx, "inspector-speed-tab");
    click(cx, "speed-curve-mode");
    click(cx, "speed-preset-1");
    for _ in 0..9 {
        click(cx, "speed-point-add");
    }
    let added = store.load_timeline(&entry).unwrap();
    let points = &added.clips[0].speed.as_ref().unwrap().curve;
    assert_eq!(points.len(), 12);
    assert!(points.windows(2).all(|p| p[1][0] - p[0][0] >= 0.001));
    desktop.update_in(cx, |v, _, cx| {
        let before = v.timeline.model.clone();
        let dense = [
            [0.0, 1.0],
            [0.5, 1.0],
            [0.5005, 1.0],
            [0.501, 1.0],
            [1.0, 1.0],
        ];
        v.timeline
            .model
            .set_speed(&v.timeline.selected, 1.0, &dense, false)
            .unwrap();
        v.commit_timeline(before, cx);
        // 从磁盘重载密集曲线，覆盖已有项目而非仅覆盖新添加点。
        v.timeline.model = store.load_timeline(&entry).unwrap();
    });
    cx.run_until_parked();
    let before_drag = store.load_timeline(&entry).unwrap();
    let graph = cx.debug_bounds("speed-curve-graph").unwrap();
    let from = point(graph.left() + graph.size.width * 0.5005, graph.center().y);
    let to = point(graph.right(), graph.top() + px(12.0));
    cx.simulate_mouse_down(from, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_move(to, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_up(to, MouseButton::Left, Modifiers::none());
    cx.run_until_parked();
    let dragged = store.load_timeline(&entry).unwrap();
    let point = dragged.clips[0].speed.as_ref().unwrap().curve[2];
    assert_eq!(point[0], 0.5005);
    assert!(point[1] > 5.0);
    click(cx, "timeline-undo");
    assert_eq!(store.load_timeline(&entry).unwrap(), before_drag);
    std::fs::remove_dir_all(root).unwrap();
}
