//! 预览逐帧与全屏观看模式的 GPUI 输入和布局回归。
use crate::{media::Media, view::Desktop};
use dinuosi_core::{ProjectStore, media::MediaKind};
use gpui::{Modifiers, MouseButton, TestAppContext, point, px, size};

/// 在独立项目中点击真实左右按钮，检查不同项目帧率、素材裁剪边界及时间线最后有效帧。
#[gpui::test]
fn preview_buttons_step_one_project_frame(cx: &mut TestAppContext) {
    let root = std::env::temp_dir().join(format!("dinuosi-frame-step-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let store = ProjectStore::new(root.join("recent.json"));
    let entry = store.create("逐帧", &root).unwrap();
    let (desktop, cx) =
        cx.add_window_view(|window, cx| Desktop::new(store, Some(entry), window, cx));
    cx.simulate_resize(size(px(1440.0), px(900.0)));
    cx.run_until_parked();
    desktop.update_in(cx, |v, window, cx| {
        v.session
            .media
            .push(Media::from_source(dinuosi_core::media::Media {
                path: "frame.png".into(),
                name: "Frame".into(),
                kind: MediaKind::Image,
                duration: 5.0,
                has_audio: false,
                preview_size: (640, 360),
                source_size: (1920, 1080),
                offline: false,
                thumbnail: Some(std::sync::Arc::new([20, 40, 80, 255].repeat(640 * 360))),
            }));
        v.select_media(0, window, cx);
        v.timeline
            .model
            .insert(
                ("frame.png".into(), MediaKind::Image),
                "Frame".into(),
                0,
                0,
                0..150,
                150,
            )
            .unwrap();
        cx.notify();
    });
    cx.run_until_parked();
    for fps in [24, 30, 60] {
        desktop.update_in(cx, |v, _, cx| {
            v.timeline.model.fps = fps;
            v.timeline.preview = true;
            v.session.timeline_position = 1.0;
            cx.notify();
        });
        cx.run_until_parked();
        let position = cx.debug_bounds("preview-next").unwrap().center();
        cx.simulate_click(position, Modifiers::none());
        assert!(
            (desktop.read_with(cx, |v, _| v.session.timeline_position) - (1.0 + 1.0 / fps as f64))
                .abs()
                < 1e-9
        );
        let position = cx.debug_bounds("preview-back").unwrap().center();
        cx.simulate_click(position, Modifiers::none());
        assert_eq!(
            desktop.read_with(cx, |v, _| v.session.timeline_position),
            1.0
        );
    }
    desktop.update_in(cx, |v, _, cx| {
        v.session.timeline_position = 0.0;
        cx.notify();
    });
    let position = cx.debug_bounds("preview-back").unwrap().center();
    cx.simulate_click(position, Modifiers::none());
    assert_eq!(
        desktop.read_with(cx, |v, _| v.session.timeline_position),
        0.0
    );
    desktop.update_in(cx, |v, _, cx| {
        v.session.timeline_position = 149.0 / 60.0;
        cx.notify();
    });
    let position = cx.debug_bounds("preview-next").unwrap().center();
    cx.simulate_click(position, Modifiers::none());
    assert_eq!(
        desktop.read_with(cx, |v, _| v.session.timeline_position),
        149.0 / 60.0
    );
    desktop.update_in(cx, |v, _, cx| {
        v.timeline.preview = false;
        v.session.trim_start = 0.5;
        v.session.trim_end = 2.0;
        v.session.position = 1.0;
        cx.notify();
    });
    let position = cx.debug_bounds("preview-next").unwrap().center();
    cx.simulate_click(position, Modifiers::none());
    assert!((desktop.read_with(cx, |v, _| v.session.position) - 61.0 / 60.0).abs() < 1e-9);
    desktop.update_in(cx, |v, _, cx| {
        v.session.position = 0.5;
        cx.notify();
    });
    let position = cx.debug_bounds("preview-back").unwrap().center();
    cx.simulate_click(position, Modifiers::none());
    assert_eq!(desktop.read_with(cx, |v, _| v.session.position), 0.5);
    desktop.update_in(cx, |v, _, cx| {
        v.session.position = 119.0 / 60.0;
        cx.notify();
    });
    let position = cx.debug_bounds("preview-next").unwrap().center();
    cx.simulate_click(position, Modifiers::none());
    assert_eq!(
        desktop.read_with(cx, |v, _| v.session.position),
        119.0 / 60.0
    );
    std::fs::remove_dir_all(root).unwrap();
}

/// 全屏不挂载编辑器和变换层，真实画面拖动与删除/切割快捷键不改变模型；进度可拖动且 Escape/按钮可退出。
#[gpui::test]
fn fullscreen_preview_blocks_edits_and_seeks_current_mode(cx: &mut TestAppContext) {
    let root = std::env::temp_dir().join(format!("dinuosi-fullscreen-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let store = ProjectStore::new(root.join("recent.json"));
    let entry = store.create("全屏", &root).unwrap();
    let (desktop, cx) =
        cx.add_window_view(|window, cx| Desktop::new(store, Some(entry), window, cx));
    cx.simulate_resize(size(px(1440.0), px(900.0)));
    cx.run_until_parked();
    desktop.update_in(cx, |v, window, cx| {
        v.session
            .media
            .push(Media::from_source(dinuosi_core::media::Media {
                path: "frame.png".into(),
                name: "Frame".into(),
                kind: MediaKind::Image,
                duration: 5.0,
                has_audio: false,
                preview_size: (640, 360),
                source_size: (1920, 1080),
                offline: false,
                thumbnail: Some(std::sync::Arc::new([20, 40, 80, 255].repeat(640 * 360))),
            }));
        v.select_media(0, window, cx);
        let id = v
            .timeline
            .model
            .insert(
                ("frame.png".into(), MediaKind::Image),
                "Frame".into(),
                0,
                0,
                0..150,
                150,
            )
            .unwrap();
        v.timeline.selected.insert(id);
        v.seek_timeline(1.0, true, cx);
    });
    cx.run_until_parked();
    let before = desktop.read_with(cx, |v, _| {
        (v.timeline.model.clone(), v.timeline.selected.clone())
    });
    let position = cx.debug_bounds("preview-fullscreen").unwrap().center();
    cx.simulate_click(position, Modifiers::none());
    cx.run_until_parked();
    assert!(desktop.read_with(cx, |v, _| v.preview_fullscreen));
    // GPUI 0.2.2 的 debug_bounds 保留旧帧标签，改为断言当前全屏布局及真实事件不会编辑。
    assert_eq!(
        cx.debug_bounds("fullscreen-preview").unwrap().size,
        size(px(1440.0), px(900.0))
    );
    let picture = cx.debug_bounds("preview-viewport").unwrap();
    assert_eq!(picture.origin, point(px(0.0), px(0.0)));
    cx.simulate_mouse_down(picture.center(), MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_move(
        picture.center() + point(px(100.0), px(100.0)),
        Some(MouseButton::Left),
        Modifiers::none(),
    );
    cx.simulate_mouse_up(picture.center(), MouseButton::Left, Modifiers::none());
    cx.simulate_keystrokes("delete ctrl-b c ctrl-z ctrl-n");
    assert_eq!(
        desktop.read_with(cx, |v, _| (
            v.timeline.model.clone(),
            v.timeline.selected.clone()
        )),
        before
    );
    assert!(!desktop.read_with(cx, |v, _| v.timeline.blade));
    assert!(desktop.read_with(cx, |v, _| v.dialog.is_none()));
    let bar = cx.debug_bounds("fullscreen-progress").unwrap();
    cx.simulate_mouse_down(bar.center(), MouseButton::Left, Modifiers::none());
    assert_eq!(
        desktop.read_with(cx, |v, _| v.session.timeline_position),
        2.5
    );
    let outside = point(bar.right() + px(10.0), bar.center().y);
    cx.simulate_mouse_move(outside, Some(MouseButton::Left), Modifiers::none());
    cx.simulate_mouse_up(outside, MouseButton::Left, Modifiers::none());
    assert_eq!(
        desktop.read_with(cx, |v, _| v.session.timeline_position),
        5.0
    );
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    assert!(!desktop.read_with(cx, |v, _| v.preview_fullscreen));
    assert!(cx.debug_bounds("preview-panel").is_some());
    desktop.update_in(cx, |v, window, cx| {
        v.select_media(0, window, cx);
        v.session.trim_start = 1.0;
        v.session.trim_end = 3.0;
        v.toggle_preview_fullscreen(window, cx);
    });
    cx.run_until_parked();
    let bar = cx.debug_bounds("fullscreen-progress").unwrap();
    cx.simulate_click(bar.center(), Modifiers::none());
    assert_eq!(desktop.read_with(cx, |v, _| v.session.position), 2.0);
    let position = cx.debug_bounds("fullscreen-exit").unwrap().center();
    cx.simulate_click(position, Modifiers::none());
    cx.run_until_parked();
    assert!(!desktop.read_with(cx, |v, _| v.preview_fullscreen));
    assert_eq!(desktop.read_with(cx, |v, _| v.session.position), 2.0);
    desktop.update_in(cx, |v, window, cx| v.toggle_preview_fullscreen(window, cx));
    cx.run_until_parked();
    cx.simulate_keystrokes("tab tab enter");
    assert!(!desktop.read_with(cx, |v, _| v.preview_fullscreen));
    std::fs::remove_dir_all(root).unwrap();
}
