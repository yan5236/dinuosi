//! 预览变换的原生鼠标事件、持久化、撤销及画布边界回归。
use crate::{
    media::Media,
    preview_geometry::{Geometry, canvas_bounds},
    view::Desktop,
};
use dinuosi_core::{
    ProjectStore,
    media::MediaKind,
    timeline::{Property, Transform},
};
use gpui::{Bounds, Modifiers, MouseButton, TestAppContext, point, px, size};

/// 检查不同视口的实际画布、竖屏有效尺寸、旋转逆命中及背景手柄全部位于画面内。
#[test]
fn preview_geometry_matches_compositor_and_clips_controls() {
    for dimensions in [(800.0, 300.0), (320.0, 600.0), (640.0, 360.0)] {
        let bounds = canvas_bounds(Bounds::new(
            point(px(50.0), px(40.0)),
            size(px(dimensions.0), px(dimensions.1)),
        ));
        assert!(
            (f32::from(bounds.size.width) / f32::from(bounds.size.height) - 16.0 / 9.0).abs()
                < 0.001
        );
        let g = Geometry::new(bounds, &Transform::default(), (640, 360), (1920, 1080));
        assert_eq!(g.handles().len(), 4);
        for h in g.handles() {
            assert!(h.x >= bounds.left() + px(3.5) && h.x <= bounds.right() - px(3.5));
            assert!(h.y >= bounds.top() + px(3.5) && h.y <= bounds.bottom() - px(3.5));
        }
        let handle = g.rotation_handle().unwrap();
        assert!(handle.y + px(9.0) <= bounds.bottom());
        assert!(!g.contains(point(bounds.left() - px(1.0), bounds.center().y)));
        let rotated = Geometry::new(
            bounds,
            &Transform {
                scale: 50.0,
                rotation: 45.0,
                ..Default::default()
            },
            (180, 360),
            (1920, 1080),
        );
        assert!(rotated.contains(rotated.position(0.0, rotated.half.y * 0.8)));
        assert!(!rotated.contains(rotated.position(rotated.half.x + 2.0, 0.0)));
    }
}

/// 使用真实 GPUI 鼠标按下/移动/释放验证双向选择、平移、缩放、旋转与单手势单次撤销。
/// 磁盘断言验证持久化；隐藏、锁定、帧尾和素材预览不显示可编辑框，测试文件独占临时目录。
#[gpui::test]
fn preview_transform_mouse_updates_inspector_and_persistence(cx: &mut TestAppContext) {
    let root = std::env::temp_dir().join(format!("dinuosi-transform-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let store = ProjectStore::new(root.join("recent.json"));
    let entry = store.create("预览变换", &root).unwrap();
    let (desktop, cx) = cx
        .add_window_view(|window, cx| Desktop::new(store.clone(), Some(entry.clone()), window, cx));
    cx.simulate_resize(size(px(1440.0), px(900.0)));
    cx.run_until_parked();
    let ids = desktop.update_in(cx, |v, _, cx| {
        for (name, effective) in [("background.png", (640, 360)), ("portrait.png", (180, 360))] {
            v.session
                .media
                .push(Media::from_source(dinuosi_core::media::Media {
                    path: name.into(),
                    name: name.into(),
                    kind: MediaKind::Image,
                    duration: 5.0,
                    has_audio: false,
                    preview_size: effective,
                    offline: false,
                    thumbnail: Some(std::sync::Arc::new([40, 80, 180, 255].repeat(640 * 360))),
                }));
        }
        let bg = v
            .timeline
            .model
            .insert(
                ("background.png".into(), MediaKind::Image),
                "背景".into(),
                0,
                0,
                0..150,
                150,
            )
            .unwrap();
        let fg = v
            .timeline
            .model
            .insert(
                ("portrait.png".into(), MediaKind::Image),
                "前景".into(),
                0,
                0,
                0..150,
                150,
            )
            .unwrap();
        v.timeline
            .model
            .set_property(&[fg.clone()].into(), Property::Scale, 50.0)
            .unwrap();
        v.timeline.preview = true;
        v.zoom = 0.5;
        v.refresh_timeline(cx);
        (bg, fg)
    });
    cx.run_until_parked();
    // 点击时间线片段后，预览必须使用同一选择集。
    let selector = Box::leak(format!("clip-{}", ids.1).into_boxed_str());
    let clip_center = cx.debug_bounds(selector).unwrap().center();
    cx.simulate_click(clip_center, Modifiers::none());
    cx.run_until_parked();
    let g = desktop.read_with(cx, |v, _| {
        let shapes = v.preview_shapes();
        assert_eq!(shapes.iter().filter(|s| s.selected).count(), 1);
        shapes.iter().find(|s| s.id == ids.1).unwrap().geometry
    });
    // 前景外的背景可点选，随后点前景中心会切回上层，而非拖动原选背景。
    cx.simulate_click(
        point(g.canvas.left() + px(24.0), g.canvas.center().y),
        Modifiers::none(),
    );
    assert!(desktop.read_with(cx, |v, _| v.timeline.selected.contains(&ids.0)));
    cx.simulate_click(g.center, Modifiers::none());
    assert!(desktop.read_with(cx, |v, _| v.timeline.selected.contains(&ids.1)));
    let before = desktop.read_with(cx, |v, _| v.timeline.model.clone());
    let to = g.center + point(px(24.0), px(12.0));
    cx.simulate_mouse_down(g.center, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_move(
        g.center + point(px(8.0), px(4.0)),
        MouseButton::Left,
        Modifiers::none(),
    );
    cx.simulate_mouse_move(to, MouseButton::Left, Modifiers::none());
    desktop.read_with(cx, |v, _| {
        assert!(
            (v.property_value(Property::X)
                - 24.0 / f64::from(f32::from(g.canvas.size.width)) * 1920.0)
                .abs()
                < 0.01
        );
        assert!(
            (v.property_value(Property::Y)
                - 12.0 / f64::from(f32::from(g.canvas.size.height)) * 1080.0)
                .abs()
                < 0.01
        );
    });
    cx.simulate_mouse_up(to, MouseButton::Left, Modifiers::none());
    cx.run_until_parked();
    let moved = desktop.read_with(cx, |v, _| v.timeline.model.clone());
    assert_eq!(store.load_timeline(&entry).unwrap(), moved);
    cx.simulate_keystrokes("ctrl-z");
    assert_eq!(
        desktop.read_with(cx, |v, _| v.timeline.model.clone()),
        before
    );
    cx.simulate_keystrokes("ctrl-y");
    assert_eq!(
        desktop.read_with(cx, |v, _| v.timeline.model.clone()),
        moved
    );
    cx.run_until_parked();
    let g = desktop.read_with(cx, |v, _| v.preview_shapes()[0].geometry);
    let from = g.handles()[2];
    let to = g.center + (from - g.center) * 1.5;
    cx.simulate_mouse_down(from, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_move(to, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_up(to, MouseButton::Left, Modifiers::none());
    cx.run_until_parked();
    assert!((desktop.read_with(cx, |v, _| v.property_value(Property::Scale)) - 75.0).abs() < 0.01);
    let g = desktop.read_with(cx, |v, _| v.preview_shapes()[0].geometry);
    let from = g.rotation_handle().unwrap();
    let radius = from.y - g.center.y;
    let to = g.center + point(-radius, px(0.0));
    cx.simulate_mouse_down(from, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_move(to, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_up(to, MouseButton::Left, Modifiers::none());
    cx.run_until_parked();
    assert!(
        (desktop.read_with(cx, |v, _| v.property_value(Property::Rotation)) - 90.0).abs() < 0.01
    );
    assert_eq!(
        store.load_timeline(&entry).unwrap(),
        desktop.read_with(cx, |v, _| v.timeline.model.clone())
    );
    // 参数输入也驱动同一几何，非正方形在旋转后仍可正确点中。
    let input = cx.debug_bounds("inspector-value-X").unwrap().center();
    cx.simulate_click(input, Modifiers::none());
    cx.simulate_keystrokes("ctrl-a");
    cx.simulate_input("240");
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    desktop.read_with(cx, |v, _| {
        let g = v.preview_shapes()[0].geometry;
        assert!(
            (f32::from(g.center.x - g.canvas.center().x) / f32::from(g.canvas.size.width) - 0.125)
                .abs()
                < 0.001
        );
    });
    desktop.update_in(cx, |v, _, cx| {
        v.timeline.model.tracks[0].locked = true;
        assert!(v.preview_shapes().iter().all(|s| s.id != ids.1));
        v.timeline.model.tracks[0].locked = false;
        v.timeline.model.tracks[0].visible = false;
        assert!(v.preview_shapes().iter().all(|s| s.id != ids.1));
        v.timeline.model.tracks[0].visible = true;
        v.session.timeline_position = 5.0;
        assert!(v.preview_shapes().is_empty());
        v.session.timeline_position = 0.0;
        v.timeline.preview = false;
        assert!(v.preview_shapes().is_empty());
        cx.notify();
    });
    std::fs::remove_dir_all(root).unwrap();
}
