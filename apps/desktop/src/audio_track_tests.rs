//! 通过 GPUI 窗口指针事件验证音频拖入、类型检查器、音量滑块及磁盘事务。
use crate::{media::Media, view::Desktop};
use dinuosi_core::{ProjectStore, media::MediaKind, timeline::TrackKind};
use gpui::{Modifiers, MouseButton, TestAppContext, point, px, size};

/// 拖动使用窗口鼠标按下、移动及释放；不直接调用插入或音量接口，释放后等待界面稳定。
fn drag(
    cx: &mut gpui::VisualTestContext,
    from: gpui::Point<gpui::Pixels>,
    to: gpui::Point<gpui::Pixels>,
) {
    cx.simulate_mouse_down(from, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_move(to, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_up(to, MouseButton::Left, Modifiers::none());
    cx.run_until_parked();
}

/// 验证音频行与画面检查器同列对齐、端点拖拽、数值输入、微调、取消及磁盘事务。
/// 音频拖入主轨自动分轨且仅有音量；图片跨类型放置被拒绝，撤销/重做与重开保留结果。
#[gpui::test]
fn audio_drop_and_volume_slider_persist_pointer_results(cx: &mut TestAppContext) {
    let root = std::env::temp_dir().join(format!("dinuosi-audio-ui-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let store = ProjectStore::new(root.join("recent.json"));
    let entry = store.create("音频轨道", &root).unwrap();
    let (desktop, cx) = cx
        .add_window_view(|window, cx| Desktop::new(store.clone(), Some(entry.clone()), window, cx));
    cx.simulate_resize(size(px(1440.0), px(900.0)));
    desktop.update_in(cx, |v, _, cx| {
        v.zoom = 2.0;
        for (name, kind) in [
            ("audio.wav", MediaKind::Audio),
            ("image.png", MediaKind::Image),
        ] {
            v.session
                .media
                .push(Media::from_source(dinuosi_core::media::Media {
                    path: name.into(),
                    name: name.into(),
                    kind,
                    duration: 2.0,
                    has_audio: kind == MediaKind::Audio,
                    preview_size: (640, 360),
                    source_size: (1920, 1080),
                    thumbnail: None,
                    offline: false,
                }));
        }
        cx.notify();
    });
    cx.run_until_parked();
    // 空选择现在显示项目概览；先选择画面获得旧控件位置，再验证音频不会响应这些位置。
    desktop.update_in(cx, |v, _, cx| {
        let id = v
            .timeline
            .model
            .insert(
                ("layout.png".into(), MediaKind::Image),
                "布局".into(),
                0,
                0,
                0..30,
                30,
            )
            .unwrap();
        v.timeline.selected.insert(id);
        cx.notify();
    });
    cx.run_until_parked();
    let old_rotation = cx.debug_bounds("inspector-slider-Rotation").unwrap();
    let old_opacity = cx.debug_bounds("inspector-slider-Opacity").unwrap();
    desktop.update_in(cx, |v, _, cx| {
        v.timeline.model.clips.clear();
        v.timeline.selected.clear();
        cx.notify();
    });
    cx.run_until_parked();
    let card = cx.debug_bounds("media-0").unwrap().center();
    let drop = desktop.read_with(cx, |v, _| {
        let (y, h) = v.track_geometry(0);
        point(
            v.timeline.bounds.left() + px(v.timeline.header + v.pixels_per_second()),
            v.timeline.bounds.top() + px(y + h / 2.0),
        )
    });
    drag(cx, card, drop);
    let original = store.load_timeline(&entry).unwrap();
    assert_eq!(original.tracks[0].kind, TrackKind::Audio);
    assert_eq!(original.tracks[1].kind, TrackKind::Main);
    assert_eq!(original.clips[0].start, 30);
    assert_eq!(original.clips[0].gain, 1.0);
    assert!(cx.debug_bounds("audio-inspector").is_some());
    // GPUI 0.2.2 的 debug_bounds 缓存不清除消失元素，以旧旋转位置的真实事件验证已无画面参数监听。
    drag(
        cx,
        old_rotation.center(),
        point(old_rotation.right(), old_rotation.center().y),
    );
    assert_eq!(store.load_timeline(&entry).unwrap(), original);
    let slider = cx.debug_bounds("inspector-volume-slider").unwrap();
    let value = cx.debug_bounds("inspector-volume-value").unwrap();
    let label = cx.debug_bounds("inspector-volume-label").unwrap();
    assert_eq!(
        slider, old_opacity,
        "音频滑块须与既有画面参数同列、同宽、同高"
    );
    assert_eq!(label.size.width, px(63.0));
    assert!(label.right() < slider.left());
    assert_eq!(value.size, size(px(59.0), px(24.0)));
    assert!(value.left() > slider.right());
    // 越过边界仍由根节点捕获，最小严格静音；连续移动在释放前不写磁盘。
    cx.simulate_mouse_down(slider.center(), MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_move(
        point(slider.left() - px(20.0), slider.center().y),
        MouseButton::Left,
        Modifiers::none(),
    );
    assert_eq!(
        desktop.read_with(cx, |v, _| v.timeline.model.clips[0].gain),
        0.0
    );
    assert_eq!(store.load_timeline(&entry).unwrap(), original);
    cx.simulate_mouse_up(
        point(slider.left() - px(20.0), slider.center().y),
        MouseButton::Left,
        Modifiers::none(),
    );
    cx.run_until_parked();
    assert_eq!(store.load_timeline(&entry).unwrap().clips[0].gain, 0.0);
    desktop.update_in(cx, |v, _, cx| v.timeline_history(false, cx));
    assert_eq!(store.load_timeline(&entry).unwrap(), original);
    desktop.update_in(cx, |v, _, cx| v.timeline_history(true, cx));
    assert_eq!(store.load_timeline(&entry).unwrap().clips[0].gain, 0.0);
    drag(
        cx,
        slider.center(),
        point(slider.right() + px(20.0), slider.center().y),
    );
    assert_eq!(
        store.load_timeline(&entry).unwrap().clips[0].volume_db(),
        20.0
    );
    // 数值框与箭头都走真实窗口事件；输入 dB、取消和非法输入均校验具体保存结果。
    let down = cx.debug_bounds("inspector-volume-down").unwrap().center();
    cx.simulate_mouse_down(down, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_up(down, MouseButton::Left, Modifiers::none());
    cx.run_until_parked();
    assert!((store.load_timeline(&entry).unwrap().clips[0].volume_db() - 19.9).abs() < 1e-9);
    for (text, key, expected) in [
        ("-6.5dB", "enter", -6.5),
        ("15", "escape", -6.5),
        ("NaN", "enter", -6.5),
        ("-∞dB", "enter", f64::NEG_INFINITY),
        ("40", "enter", 20.0),
    ] {
        cx.simulate_mouse_down(value.center(), MouseButton::Left, Modifiers::none());
        cx.simulate_mouse_up(value.center(), MouseButton::Left, Modifiers::none());
        cx.run_until_parked();
        cx.simulate_keystrokes("ctrl-a");
        cx.simulate_input(text);
        cx.simulate_keystrokes(key);
        cx.run_until_parked();
        let db = store.load_timeline(&entry).unwrap().clips[0].volume_db();
        assert!(
            db == expected || (db - expected).abs() < 1e-9,
            "{text}: {db} != {expected}"
        );
        assert!(desktop.read_with(cx, |v, _| v.timeline.volume_input.is_none()));
    }
    let before = store.load_timeline(&entry).unwrap();
    let image = cx.debug_bounds("media-1").unwrap().center();
    let audio_row = desktop.read_with(cx, |v, _| {
        let (y, h) = v.track_geometry(0);
        point(
            v.timeline.bounds.left() + px(v.timeline.header + v.pixels_per_second()),
            v.timeline.bounds.top() + px(y + h / 2.0),
        )
    });
    drag(cx, image, audio_row);
    assert_eq!(
        store.load_timeline(&entry).unwrap(),
        before,
        "图片不能进入音频轨道"
    );
    let main_row = desktop.read_with(cx, |v, _| {
        let (y, h) = v.track_geometry(v.timeline.model.main_track());
        point(
            v.timeline.bounds.left() + px(v.timeline.header + v.pixels_per_second()),
            v.timeline.bounds.top() + px(y + h / 2.0),
        )
    });
    drag(cx, image, main_row);
    let opacity = cx.debug_bounds("inspector-slider-Opacity").unwrap();
    drag(
        cx,
        opacity.center(),
        point(opacity.left(), opacity.center().y),
    );
    assert_eq!(
        desktop.read_with(cx, |v, _| v.timeline.model.clips[1].transform.opacity),
        0.0
    );
    assert_eq!(
        desktop.read_with(cx, |v, _| v.timeline.model.clips[0].gain),
        10.0
    );
    let final_model = desktop.read_with(cx, |v, _| v.timeline.model.clone());
    assert_eq!(store.load_timeline(&entry).unwrap(), final_model);
    std::fs::remove_dir_all(root).unwrap();
}
