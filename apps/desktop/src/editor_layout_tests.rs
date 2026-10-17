//! 工作区分隔的尺寸边界与 GPUI 实际事件回归。
use super::*;
use dinuosi_core::ProjectStore;
use gpui::{Modifiers, TestAppContext, VisualTestContext, point, size};

/// 覆盖最小窗口、大屏、极端期望尺寸及曲线开关，三栏总宽和轨道最小高度必须守恒。
#[test]
fn panel_constraints_and_restore() {
    for (width, height) in [(1040.0, 640.0), (1440.0, 900.0), (2560.0, 1440.0)] {
        for requested in [None, Some(-10000.0), Some(10000.0)] {
            let panels = Panels {
                left: requested,
                right: requested,
                upper: requested,
                curve: requested,
                drag: None,
            };
            for open in [false, true] {
                let layout = Layout::resolve(width, height, &panels, open);
                assert!(
                    (layout.left + layout.preview + layout.right + GAP * 2.0 - width).abs() < 0.01
                );
                assert!(
                    layout.left >= MIN_LEFT
                        && layout.right >= MIN_RIGHT
                        && layout.preview >= MIN_PREVIEW - 0.01
                );
                assert!(layout.upper >= MIN_UPPER);
                let timeline =
                    layout.available - layout.upper - layout.curve - if open { GAP } else { 0.0 };
                assert!(timeline >= MIN_TIMELINE - 0.01);
                assert!(!open || layout.curve >= MIN_CURVE);
            }
        }
    }
    let panels = Panels {
        left: Some(530.0),
        upper: Some(480.0),
        curve: Some(230.0),
        ..Default::default()
    };
    let small = Layout::resolve(1040.0, 640.0, &panels, true);
    let large = Layout::resolve(1440.0, 900.0, &panels, false);
    assert!(small.left < large.left && small.upper < large.upper);
    assert_eq!(large.left, 530.0);
    assert_eq!(large.upper, 480.0);
    assert_eq!(Layout::resolve(1440.0, 900.0, &panels, true).curve, 230.0);
}

/// 按住鼠标逐段移动，在每次移动后、释放前检查分隔跟随；大幅越界拖动由调用方检查限位。
fn drag(cx: &mut VisualTestContext, selector: &'static str, dx: f32, dy: f32) {
    let from = cx.debug_bounds(selector).expect(selector).center();
    let to = from + point(px(dx), px(dy));
    cx.simulate_mouse_down(from, MouseButton::Left, Modifiers::none());
    cx.run_until_parked();
    for fraction in [0.25, 0.5, 0.75, 1.0] {
        let position = from + point(px(dx * fraction), px(dy * fraction));
        cx.simulate_mouse_move(position, MouseButton::Left, Modifiers::none());
        cx.run_until_parked();
        if dx.abs().max(dy.abs()) < 1000.0 {
            let actual = cx.debug_bounds(selector).unwrap().center();
            let error = if dx != 0.0 {
                actual.x - position.x
            } else {
                actual.y - position.y
            };
            assert!(
                error.abs() < px(0.1),
                "{selector} 在鼠标仍按下时没有跟随：{actual:?}, {position:?}"
            );
        }
    }
    cx.simulate_mouse_up(to, MouseButton::Left, Modifiers::none());
    cx.run_until_parked();
}

/// 真实拖动四条边，检查相邻区域交换空间、越界夹紧、曲线开关恢复及模型/播放头/历史均不变。
/// 最小窗口的素材预览必须把全部控制组放在面板内，不能用裁切隐藏全屏等现有入口。
#[gpui::test]
fn splitters_resize_without_editing_project(cx: &mut TestAppContext) {
    let root = std::env::temp_dir().join(format!("dinuosi-panels-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let store = ProjectStore::new(root.join("recent.json"));
    let entry = store.create("分隔", &root).unwrap();
    let original = store.load_timeline(&entry).unwrap();
    let (desktop, cx) = cx
        .add_window_view(|window, cx| Desktop::new(store.clone(), Some(entry.clone()), window, cx));
    cx.simulate_resize(size(px(1440.0), px(900.0)));
    cx.run_until_parked();
    let preview = cx.debug_bounds("preview-panel").unwrap();
    let inspector = cx.debug_bounds("inspector-panel").unwrap();
    drag(cx, "resize-library", 80.0, 0.0);
    let resized = cx.debug_bounds("preview-panel").unwrap();
    assert!((resized.left() - preview.left() - px(80.0)).abs() < px(0.1));
    assert_eq!(cx.debug_bounds("inspector-panel").unwrap(), inspector);
    drag(cx, "resize-inspector", -50.0, 0.0);
    assert!(
        (cx.debug_bounds("inspector-panel").unwrap().left() - inspector.left() + px(50.0)).abs()
            < px(0.1)
    );
    let upper = cx.debug_bounds("preview-panel").unwrap().size.height;
    drag(cx, "resize-timeline", 0.0, -100.0);
    assert!(
        (cx.debug_bounds("preview-panel").unwrap().size.height - upper + px(100.0)).abs() < px(0.1)
    );
    let closed_height = cx.debug_bounds("preview-panel").unwrap().size.height;
    let toggle = cx.debug_bounds("keyframe-curves").unwrap().center();
    cx.simulate_click(toggle, Modifiers::none());
    cx.run_until_parked();
    let curve = cx.debug_bounds("keyframe-curve-panel").unwrap();
    let upper = cx.debug_bounds("preview-panel").unwrap();
    drag(cx, "resize-curve", 0.0, 30.0);
    assert!(
        (cx.debug_bounds("keyframe-curve-panel").unwrap().size.height
            - curve.size.height
            - px(30.0))
        .abs()
            < px(0.1)
    );
    assert_eq!(cx.debug_bounds("preview-panel").unwrap(), upper);
    let curve_height = cx.debug_bounds("keyframe-curve-panel").unwrap().size.height;
    let close = cx.debug_bounds("curve-close").unwrap().center();
    cx.simulate_click(close, Modifiers::none());
    cx.run_until_parked();
    assert_eq!(
        cx.debug_bounds("preview-panel").unwrap().size.height,
        closed_height
    );
    let toggle = cx.debug_bounds("keyframe-curves").unwrap().center();
    cx.simulate_click(toggle, Modifiers::none());
    cx.run_until_parked();
    assert_eq!(
        cx.debug_bounds("keyframe-curve-panel").unwrap().size.height,
        curve_height
    );
    drag(cx, "resize-library", 5000.0, 0.0);
    assert!(
        (cx.debug_bounds("preview-panel").unwrap().size.width - px(MIN_PREVIEW)).abs() < px(0.1)
    );
    assert!(desktop.read_with(cx, |v, _| v.panels.drag.is_none()));
    cx.simulate_resize(size(px(1040.0), px(640.0)));
    cx.run_until_parked();
    assert!(cx.debug_bounds("timeline-panel").unwrap().size.height >= px(MIN_TIMELINE - 0.1));
    assert!(cx.debug_bounds("preview-panel").unwrap().size.height >= px(MIN_UPPER));
    desktop.update_in(cx, |view, _, cx| {
        let mut source = dinuosi_core::media::offline(root.join("preview.png"));
        source.offline = false;
        source.duration = 5.0;
        source.kind = dinuosi_core::media::MediaKind::Image;
        view.session.media = vec![crate::media::Media::from_source(source)];
        view.session.selected = Some(0);
        view.timeline.preview = false;
        cx.notify();
    });
    cx.run_until_parked();
    let preview = cx.debug_bounds("preview-panel").unwrap();
    for selector in [
        "add-to-track",
        "preview-back",
        "preview-next",
        "preview-volume",
        "volume-slider",
        "preview-fullscreen",
    ] {
        let control = cx.debug_bounds(selector).expect(selector);
        assert!(
            control.left() >= preview.left() && control.right() <= preview.right(),
            "{selector}: {control:?}, {preview:?}"
        );
        assert!(control.top() >= preview.top() && control.bottom() <= preview.bottom());
    }
    assert_eq!(store.load_timeline(&entry).unwrap(), original);
    assert!(
        desktop.read_with(cx, |v, _| !v.timeline.history.can_step(false)
            && v.session.timeline_position == 0.0)
    );
    std::fs::remove_dir_all(root).unwrap();
}
