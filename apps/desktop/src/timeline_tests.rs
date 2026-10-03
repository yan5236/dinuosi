//! 使用 GPUI 真实指针事件验证素材拖入、组选择、裁边、参数写入与连续寻址。
use crate::{media::Media, view::Desktop};
use dinuosi_core::{
    ProjectStore,
    media::{FRAME_HEIGHT, FRAME_WIDTH, MediaKind},
    timeline::{Property, ScrubPreview},
};
use gpui::{Modifiers, MouseButton, TestAppContext, point, px, size};

/// 从素材卡拖到轨道，再框选、移动、裁边、输入参数、删除和撤销，并验证重开磁盘结果。
#[gpui::test]
fn timeline_pointer_editing_persists_real_results(cx: &mut TestAppContext) {
    let root = std::env::temp_dir().join(format!("dinuosi-timeline-ui-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let store = ProjectStore::new(root.join("recent.json"));
    let entry = store.create("交互验证", &root).unwrap();
    let (desktop, cx) = cx
        .add_window_view(|window, cx| Desktop::new(store.clone(), Some(entry.clone()), window, cx));
    cx.simulate_resize(size(px(1440.0), px(900.0)));
    cx.run_until_parked();
    desktop.update_in(cx, |v, _, cx| {
        let mut bytes = vec![0; (FRAME_WIDTH * FRAME_HEIGHT * 4) as usize];
        for p in bytes.as_chunks_mut::<4>().0 {
            p.copy_from_slice(&[30, 70, 160, 255]);
        }
        v.session
            .media
            .push(Media::from_source(dinuosi_core::media::Media {
                path: "test.png".into(),
                name: "测试图".into(),
                kind: MediaKind::Image,
                duration: 5.0,
                has_audio: false,
                preview_size: (640, 360),
                thumbnail: Some(std::sync::Arc::new(bytes)),
                offline: false,
            }));
        cx.notify();
    });
    cx.run_until_parked();
    let card = cx.debug_bounds("media-0").unwrap().center();
    let target = desktop.read_with(cx, |v, _| {
        let (y, h) = v.track_geometry(2);
        point(
            v.timeline.bounds.left() + px(v.timeline.header + 25.0),
            v.timeline.bounds.top() + px(y + h / 2.0),
        )
    });
    cx.simulate_mouse_down(card, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_move(target, MouseButton::Left, Modifiers::none());
    assert!(desktop.read_with(cx, |v, _| v.timeline.moved));
    cx.simulate_mouse_up(target, MouseButton::Left, Modifiers::none());
    cx.run_until_parked();
    assert_eq!(
        desktop.read_with(cx, |v, _| v.timeline.model.clips.len()),
        1
    );
    let id = desktop.read_with(cx, |v, _| v.timeline.model.clips[0].id.clone());
    let selector: &'static str = Box::leak(format!("clip-{id}").into_boxed_str());
    let clip = cx.debug_bounds(selector).unwrap();
    let from = clip.center();
    let to = from + point(px(70.0), px(0.0));
    let start = desktop.read_with(cx, |v, _| v.timeline.model.clips[0].start);
    cx.simulate_mouse_down(from, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_move(to, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_up(to, MouseButton::Left, Modifiers::none());
    cx.run_until_parked();
    assert!(desktop.read_with(cx, |v, _| v.timeline.model.clips[0].start > start));
    let clip = cx.debug_bounds(selector).unwrap();
    let from = point(clip.left() - px(10.0), clip.top() - px(15.0));
    let to = clip.bottom_right() + point(px(10.0), px(8.0));
    cx.simulate_mouse_down(from, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_move(to, MouseButton::Left, Modifiers::none());
    assert!(desktop.read_with(cx, |v, _| v.timeline.selected.contains(&id)));
    cx.simulate_mouse_up(to, MouseButton::Left, Modifiers::none());
    cx.run_until_parked();
    let field = cx
        .debug_bounds("inspector-value-Rotation")
        .unwrap()
        .center();
    cx.simulate_click(field, Modifiers::none());
    cx.simulate_keystrokes("ctrl-a");
    cx.simulate_input("-360");
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    assert_eq!(
        desktop.read_with(cx, |v, _| v.property_value(Property::Rotation)),
        -360.0
    );
    assert_eq!(
        store.load_timeline(&entry).unwrap().clips[0]
            .transform
            .rotation,
        -360.0
    );
    cx.simulate_keystrokes("delete");
    cx.run_until_parked();
    assert!(store.load_timeline(&entry).unwrap().clips.is_empty());
    cx.simulate_keystrokes("ctrl-z");
    cx.run_until_parked();
    assert_eq!(store.load_timeline(&entry).unwrap().clips.len(), 1);
    desktop.update_in(cx, |v, _, cx| {
        v.timeline.model.clips[0].start = 0;
        v.timeline.selected.insert(id.clone());
        cx.notify();
    });
    cx.run_until_parked();
    let clip = cx.debug_bounds(selector).unwrap();
    let left = cx
        .debug_bounds(Box::leak(format!("trim-{id}-true").into_boxed_str()))
        .unwrap();
    let right = cx
        .debug_bounds(Box::leak(format!("trim-{id}-false").into_boxed_str()))
        .unwrap();
    let ruler = cx.debug_bounds("timeline-seek").unwrap();
    let content = cx.debug_bounds("timeline-content").unwrap();
    assert_eq!(left.size.width, px(crate::trim_handle::WIDTH));
    assert_eq!(right.size.width, left.size.width);
    assert!((left.right() - clip.left()).abs() <= px(1.0));
    assert!((right.left() - clip.right()).abs() <= px(1.0));
    assert!(left.left() >= content.left());
    assert_eq!(ruler.left() - content.left(), px(crate::trim_handle::WIDTH));
    // 从外侧把手裁剪，验证命中区域确实能改变核心时长，而不是触发片段移动。
    let duration = desktop.read_with(cx, |v, _| v.timeline.model.clips[0].duration);
    let end = right.center() - point(px(18.0), px(0.0));
    cx.simulate_mouse_down(right.center(), MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_move(end, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_up(end, MouseButton::Left, Modifiers::none());
    cx.run_until_parked();
    assert!(desktop.read_with(cx, |v, _| v.timeline.model.clips[0].duration < duration));
    assert_eq!(
        desktop.read_with(cx, |v, _| v.timeline.model.clips[0].start),
        0
    );
    desktop.update_in(cx, |v, _, cx| {
        v.change_zoom(4.0, cx);
        v.timeline.scroll_x = v.pixels_per_second() * 5.0;
        v.change_zoom(-2.0, cx);
        assert!((v.timeline.scroll_x / v.pixels_per_second() - 5.0).abs() < 0.001);
        assert!(v.timeline.scroll_x <= v.timeline_width() - v.timeline_view_width());
    });
    std::fs::remove_dir_all(root).unwrap();
}

/// 使用两段不同颜色的真实核心像素，按下/移动但不释放鼠标时检查画面；备用模式保留上一帧直到释放。
#[gpui::test]
fn live_scrub_updates_preview_before_mouse_release(cx: &mut TestAppContext) {
    let entry = dinuosi_core::ProjectEntry {
        id: "test".into(),
        name: "test".into(),
        path: "unused".into(),
        created_at: 0,
        modified_at: 0,
    };
    let (desktop, cx) = cx.add_window_view(|window, cx| {
        Desktop::new(
            ProjectStore::new(std::env::temp_dir().join("unused-timeline.json")),
            Some(entry),
            window,
            cx,
        )
    });
    cx.simulate_resize(size(px(1440.0), px(900.0)));
    cx.run_until_parked();
    desktop.update_in(cx, |v, _, cx| {
        v.zoom = 2.0;
        for (index, color) in [[220, 0, 0, 255], [0, 0, 220, 255]].into_iter().enumerate() {
            let path = std::path::PathBuf::from(format!("color-{index}.png"));
            let bytes = color.repeat((FRAME_WIDTH * FRAME_HEIGHT) as usize);
            v.session
                .media
                .push(Media::from_source(dinuosi_core::media::Media {
                    path: path.clone(),
                    name: "color".into(),
                    kind: MediaKind::Image,
                    duration: 2.0,
                    has_audio: false,
                    preview_size: (640, 360),
                    thumbnail: Some(std::sync::Arc::new(bytes)),
                    offline: false,
                }));
            v.timeline
                .model
                .insert(path, "color".into(), 2, index as i64 * 60, 0..60, 60)
                .unwrap();
        }
        cx.notify();
    });
    cx.run_until_parked();
    let ruler = cx.debug_bounds("timeline-seek").unwrap();
    let pps = desktop.read_with(cx, |v, _| v.pixels_per_second());
    let first = point(ruler.left() + px(pps), ruler.center().y);
    let second = point(ruler.left() + px(pps * 2.5), ruler.center().y);
    cx.simulate_mouse_down(first, MouseButton::Left, Modifiers::none());
    cx.run_until_parked();
    assert!(desktop.read_with(cx, |v, _| v.session.drag.is_some()));
    assert_eq!(
        desktop.read_with(cx, |v, _| v
            .timeline
            .frame
            .as_ref()
            .unwrap()
            .as_bytes(0)
            .unwrap()[0]),
        220
    );
    cx.simulate_mouse_move(second, MouseButton::Left, Modifiers::none());
    cx.run_until_parked();
    assert!(desktop.read_with(cx, |v, _| v.session.drag.is_some()));
    assert_eq!(
        desktop.read_with(cx, |v, _| v
            .timeline
            .frame
            .as_ref()
            .unwrap()
            .as_bytes(0)
            .unwrap()[2]),
        220
    );
    cx.simulate_mouse_up(second, MouseButton::Left, Modifiers::none());
    cx.run_until_parked();
    desktop.update_in(cx, |v, _, _| {
        v.timeline.scrub_preview = ScrubPreview::OnRelease
    });
    cx.simulate_mouse_down(first, MouseButton::Left, Modifiers::none());
    cx.run_until_parked();
    assert_eq!(
        desktop.read_with(cx, |v, _| v
            .timeline
            .frame
            .as_ref()
            .unwrap()
            .as_bytes(0)
            .unwrap()[2]),
        220
    );
    cx.simulate_mouse_up(first, MouseButton::Left, Modifiers::none());
    cx.run_until_parked();
    assert_eq!(
        desktop.read_with(cx, |v, _| v
            .timeline
            .frame
            .as_ref()
            .unwrap()
            .as_bytes(0)
            .unwrap()[0]),
        220
    );
}
