//! 媒体交互回归：GPUI 原生鼠标事件与需要真实 FFmpeg 的解码检查。

use crate::{
    media::{Media, MediaKind},
    view::Desktop,
};
use dinuosi_core::{ProjectEntry, ProjectStore};
use gpui::{Modifiers, MouseButton, TestAppContext, point, px, size};

/// 用真实项目文件和 GPUI 右键/表单事件验证别名保存、移除后的选择修正及源文件保留。
/// 失联素材同样可管理；所有文件属于测试独占目录，不触碰用户项目。
#[gpui::test]
fn media_context_menu_persists_alias_and_removes_only_reference(cx: &mut TestAppContext) {
    let root = std::env::temp_dir().join(format!("dinuosi-library-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let store = ProjectStore::new(root.join("recent.json"));
    let entry = store.create("右键验证", &root).unwrap();
    let paths = [root.join("第一份.wav"), root.join("第二份.wav")];
    for path in &paths {
        std::fs::write(path, b"source bytes").unwrap();
    }
    store.add_media_paths(&entry, &paths).unwrap();
    let first = std::fs::canonicalize(&paths[0]).unwrap();
    let (desktop, cx) = cx
        .add_window_view(|window, cx| Desktop::new(store.clone(), Some(entry.clone()), window, cx));
    cx.simulate_resize(size(px(1440.0), px(900.0)));
    cx.run_until_parked();
    desktop.update_in(cx, |v, window, cx| v.select_media(1, window, cx));
    cx.run_until_parked();
    let open_menu = |cx: &mut gpui::VisualTestContext| {
        let card = cx.debug_bounds("media-0").unwrap().center();
        cx.simulate_mouse_down(card, MouseButton::Right, Modifiers::none());
        cx.simulate_mouse_up(card, MouseButton::Right, Modifiers::none());
        cx.run_until_parked();
    };
    open_menu(cx);
    let action = cx.debug_bounds("media-rename").unwrap().center();
    cx.simulate_click(action, Modifiers::none());
    cx.simulate_keystrokes("ctrl-a backspace enter");
    cx.run_until_parked();
    assert!(desktop.read_with(cx, |v, _| v.dialog.as_ref().unwrap().error.is_some()));
    cx.simulate_input("素材别名");
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    assert!(desktop.read_with(cx, |v, _| v.dialog.is_none()));
    assert_eq!(
        desktop.read_with(cx, |v, _| v.session.media[0].name.clone()),
        "素材别名"
    );
    assert_eq!(store.load_media(&entry).unwrap()[0].name, "素材别名");
    let stale = Media::from_source(dinuosi_core::media::offline(first));
    desktop.update_in(cx, |v, window, cx| {
        v.session.media[0].source.offline = false;
        v.apply_media_status(vec![stale], window, cx);
    });
    assert_eq!(
        desktop.read_with(cx, |v, _| v.session.media[0].name.clone()),
        "素材别名"
    );
    open_menu(cx);
    let action = cx.debug_bounds("media-remove").unwrap().center();
    cx.simulate_click(action, Modifiers::none());
    cx.run_until_parked();
    assert_eq!(desktop.read_with(cx, |v, _| v.session.selected), Some(0));
    assert_eq!(desktop.read_with(cx, |v, _| v.session.media.len()), 1);
    open_menu(cx);
    let action = cx.debug_bounds("media-remove").unwrap().center();
    cx.simulate_click(action, Modifiers::none());
    cx.run_until_parked();
    assert!(desktop.read_with(cx, |v, _| v.session.selected.is_none()
        && v.session.frame.is_none()
        && v.session.strip.is_empty()));
    assert!(store.load_media(&entry).unwrap().is_empty());
    for path in paths {
        assert_eq!(std::fs::read(path).unwrap(), b"source bytes");
    }
    std::fs::remove_dir_all(root).unwrap();
}

/// 以真实 GPUI 按下/移动/释放验证音量、缩放、预览选区、播放头、分类和文件名搜索。
/// 测试素材仅用于事件与业务状态验证；实际媒体解码由独立 FFmpeg 检查覆盖。
#[gpui::test]
fn media_controls_follow_actual_mouse_drag_and_filter(cx: &mut TestAppContext) {
    let entry = ProjectEntry {
        id: "test".into(),
        name: "测试".into(),
        path: "unused".into(),
        created_at: 0,
        modified_at: 0,
    };
    let store = ProjectStore::new(std::env::temp_dir().join("dinuosi-media-unused.json"));
    let (desktop, cx) =
        cx.add_window_view(|window, cx| Desktop::new(store, Some(entry), window, cx));
    cx.simulate_resize(size(px(1440.0), px(900.0)));
    cx.run_until_parked();
    let volume = cx.debug_bounds("volume-slider").unwrap();
    cx.simulate_mouse_down(volume.center(), MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_move(
        point(volume.left() - px(30.0), volume.center().y),
        MouseButton::Left,
        Modifiers::none(),
    );
    assert_eq!(desktop.read_with(cx, |v, _| v.volume), 0.0);
    cx.simulate_mouse_move(
        point(volume.right() + px(25.0), volume.center().y),
        MouseButton::Left,
        Modifiers::none(),
    );
    assert_eq!(desktop.read_with(cx, |v, _| v.volume), 1.0);
    cx.simulate_mouse_up(volume.center(), MouseButton::Left, Modifiers::none());
    assert!(desktop.read_with(cx, |v, _| v.session.drag.is_none()));
    let zoom = cx.debug_bounds("zoom-slider").unwrap();
    cx.simulate_mouse_down(zoom.center(), MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_move(
        point(zoom.left(), zoom.center().y),
        MouseButton::Left,
        Modifiers::none(),
    );
    cx.simulate_mouse_up(zoom.origin, MouseButton::Left, Modifiers::none());
    assert_eq!(desktop.read_with(cx, |v, _| v.zoom), 0.5);
    desktop.update_in(cx, |v, window, cx| {
        v.session.media = [
            ("电影.mp4", MediaKind::Video),
            ("音乐.wav", MediaKind::Audio),
            ("图片.png", MediaKind::Image),
        ]
        .into_iter()
        .map(|(name, kind)| {
            Media::from_source(dinuosi_core::media::Media {
                path: name.into(),
                name: name.into(),
                kind,
                duration: 12.0,
                has_audio: false,
                preview_size: (640, 360),
                thumbnail: None,
                offline: false,
            })
        })
        .collect();
        v.select_media(1, window, cx);
    });
    cx.run_until_parked();
    for (width, height) in [
        (1440.0, 900.0),
        (1040.0, 640.0),
        (1920.0, 1080.0),
        (1040.0, 1000.0),
    ] {
        cx.simulate_resize(size(px(width), px(height)));
        cx.run_until_parked();
        let strip = cx.debug_bounds("preview-seek").unwrap();
        let end = cx.debug_bounds("trim-end").unwrap();
        let start = cx.debug_bounds("trim-start").unwrap();
        let selection = cx.debug_bounds("preview-selection").unwrap();
        assert!(
            (end.left() - strip.right()).abs() < px(1.0),
            "size={width}x{height} strip={strip:?} end={end:?}"
        );
        assert!((start.right() - strip.left()).abs() < px(1.0));
        assert!((selection.right() - end.left()).abs() < px(1.0));
        let display = cx.debug_bounds("preview-display").unwrap();
        let viewport = cx.debug_bounds("preview-viewport").unwrap();
        assert!(viewport.left() > display.left() && viewport.right() < display.right());
        assert!(viewport.top() > display.top() && viewport.bottom() < display.bottom());
    }
    cx.simulate_resize(size(px(1440.0), px(900.0)));
    cx.run_until_parked();
    let strip = cx.debug_bounds("preview-seek").unwrap();
    let first_card = cx.debug_bounds("media-0").unwrap().center();
    cx.simulate_click(strip.center(), Modifiers::none());
    assert!((desktop.read_with(cx, |v, _| v.session.position) - 6.0).abs() < 0.1);
    let start = cx.debug_bounds("trim-start").unwrap().center();
    cx.simulate_mouse_down(start, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_move(
        point(strip.left() + strip.size.width * 0.25, start.y),
        MouseButton::Left,
        Modifiers::none(),
    );
    cx.simulate_mouse_up(start, MouseButton::Left, Modifiers::none());
    assert!((desktop.read_with(cx, |v, _| v.session.trim_start) - 3.0).abs() < 0.1);
    let end = cx.debug_bounds("trim-end").unwrap().center();
    cx.simulate_mouse_down(end, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_move(
        point(strip.left() + strip.size.width * 0.75, end.y),
        MouseButton::Left,
        Modifiers::none(),
    );
    cx.simulate_mouse_up(end, MouseButton::Left, Modifiers::none());
    assert!((desktop.read_with(cx, |v, _| v.session.trim_end) - 9.0).abs() < 0.1);
    cx.run_until_parked();
    let start_handle = cx.debug_bounds("trim-start").unwrap();
    let end_handle = cx.debug_bounds("trim-end").unwrap();
    assert!(start_handle.center().x < strip.left() + strip.size.width * 0.25);
    assert!(end_handle.center().x > strip.left() + strip.size.width * 0.75);
    cx.simulate_mouse_down(strip.center(), MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_move(
        point(strip.left() - px(30.0), strip.center().y),
        MouseButton::Left,
        Modifiers::none(),
    );
    assert!((desktop.read_with(cx, |v, _| v.session.position) - 3.0).abs() < 0.1);
    cx.simulate_mouse_move(
        point(strip.right() + px(30.0), strip.center().y),
        MouseButton::Left,
        Modifiers::none(),
    );
    assert!((desktop.read_with(cx, |v, _| v.session.position) - 9.0).abs() < 0.1);
    cx.simulate_mouse_up(strip.center(), MouseButton::Left, Modifiers::none());
    let ruler = cx.debug_bounds("timeline-seek").unwrap();
    cx.simulate_click(ruler.center(), Modifiers::none());
    assert!(desktop.read_with(cx, |v, _| v.session.timeline_position) > 0.0);
    let filter = cx.debug_bounds("filter-audio").unwrap().center();
    cx.simulate_click(filter, Modifiers::none());
    assert!(desktop.read_with(cx, |v, _| v.session.filter == crate::library::Filter::Audio));
    cx.simulate_click(first_card, Modifiers::none());
    assert_eq!(desktop.read_with(cx, |v, _| v.session.selected), Some(1));
    let search = cx.debug_bounds("media-search").unwrap().center();
    cx.simulate_click(search, Modifiers::none());
    cx.simulate_input("不存在");
    desktop.update_in(cx, |v, _, cx| {
        v.session.selected = None;
        cx.notify();
    });
    cx.run_until_parked();
    cx.simulate_click(first_card, Modifiers::none());
    assert_eq!(desktop.read_with(cx, |v, _| v.session.selected), None);
    cx.simulate_keystrokes("ctrl-a backspace");
    cx.simulate_click(first_card, Modifiers::none());
    assert_eq!(desktop.read_with(cx, |v, _| v.session.selected), Some(1));
}
