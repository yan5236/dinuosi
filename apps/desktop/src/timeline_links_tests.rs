//! 实际 GPUI 控件和指针验证关联按钮、联动拖动、Alt 编辑及持久化历史。
use crate::view::Desktop;
use dinuosi_core::{ProjectStore, media::MediaKind};
use gpui::{Modifiers, MouseButton, TestAppContext, point, px, size};

/// 在当前帧实际控件中心点击并稳定布局；不直接调用按钮业务方法。
fn click(cx: &mut gpui::VisualTestContext, selector: &'static str, modifiers: Modifiers) {
    let point = cx.debug_bounds(selector).unwrap().center();
    cx.simulate_click(point, modifiers);
    cx.run_until_parked();
}

/// 发送实际按下、移动和释放事件，拖动结果必须从已保存项目读取。
fn drag(
    cx: &mut gpui::VisualTestContext,
    from: gpui::Point<gpui::Pixels>,
    dx: f32,
    modifiers: Modifiers,
) {
    let to = from + point(px(dx), px(0.0));
    cx.simulate_mouse_down(from, MouseButton::Left, modifiers);
    cx.simulate_mouse_move(to, MouseButton::Left, modifiers);
    cx.simulate_mouse_up(to, MouseButton::Left, modifiers);
    cx.run_until_parked();
}

/// 从按钮创建关联，点击和 Ctrl 切换整组，移动保留偏移，Alt 单独编辑，解除后独立移动且可撤销。
#[gpui::test]
fn linked_toolbar_selection_drag_unlink_and_undo(cx: &mut TestAppContext) {
    let root = std::env::temp_dir().join(format!("dinuosi-links-ui-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let store = ProjectStore::new(root.join("recent.json"));
    let entry = store.create("关联工具", &root).unwrap();
    let (desktop, cx) = cx
        .add_window_view(|window, cx| Desktop::new(store.clone(), Some(entry.clone()), window, cx));
    cx.simulate_resize(size(px(1440.0), px(900.0)));
    let (a, b) = desktop.update_in(cx, |v, _, cx| {
        let before = v.timeline.model.clone();
        let a = v
            .timeline
            .model
            .insert(
                ("linked-ui.png".into(), MediaKind::Image),
                "画面".into(),
                0,
                60,
                0..150,
                150,
            )
            .unwrap();
        let b = v
            .timeline
            .model
            .insert(
                ("linked-ui.wav".into(), MediaKind::Audio),
                "声音".into(),
                0,
                90,
                0..150,
                150,
            )
            .unwrap();
        v.timeline.model.snapping = false;
        v.commit_timeline(before, cx);
        (a, b)
    });
    let a = Box::leak(format!("clip-{a}").into_boxed_str());
    let b = Box::leak(format!("clip-{b}").into_boxed_str());
    cx.run_until_parked();
    let original = store.load_timeline(&entry).unwrap();
    click(cx, "timeline-link-clips", Modifiers::none());
    assert_eq!(store.load_timeline(&entry).unwrap(), original);
    click(cx, a, Modifiers::none());
    click(cx, b, Modifiers::control());
    click(cx, "timeline-link-clips", Modifiers::none());
    let linked = store.load_timeline(&entry).unwrap();
    assert!(linked.clips[0].link.is_some());
    assert_eq!(linked.clips[0].link, linked.clips[1].link);
    assert!(desktop.read_with(cx, |v, _| !v.can_link_clips(false)
        && v.can_link_clips(true)));
    let handle = cx
        .debug_bounds(Box::leak(
            format!("trim-{}-true", linked.clips[0].id).into_boxed_str(),
        ))
        .unwrap()
        .center();
    drag(cx, handle, 25.0, Modifiers::none());
    let trimmed = store.load_timeline(&entry).unwrap();
    let delta = trimmed.clips[0].start - linked.clips[0].start;
    assert!(delta > 0);
    assert_eq!(trimmed.clips[1].start - linked.clips[1].start, delta);
    assert_eq!(trimmed.clips[0].duration, linked.clips[0].duration - delta);
    assert_eq!(trimmed.clips[1].duration, linked.clips[1].duration - delta);
    cx.simulate_keystrokes("ctrl-z");
    cx.run_until_parked();
    assert_eq!(store.load_timeline(&entry).unwrap(), linked);
    click(cx, "timeline-razor", Modifiers::none());
    click(cx, b, Modifiers::none());
    let split = store.load_timeline(&entry).unwrap();
    assert_eq!(split.clips.len(), 4);
    assert_eq!(split.clips[0].link, split.clips[1].link);
    assert_eq!(split.clips[2].link, split.clips[3].link);
    assert_ne!(split.clips[0].link, split.clips[2].link);
    cx.simulate_keystrokes("ctrl-z");
    cx.run_until_parked();
    assert_eq!(store.load_timeline(&entry).unwrap(), linked);
    click(cx, "timeline-select", Modifiers::none());
    click(cx, a, Modifiers::control());
    assert!(desktop.read_with(cx, |v, _| v.timeline.selected.is_empty()));
    click(cx, b, Modifiers::none());
    assert_eq!(desktop.read_with(cx, |v, _| v.timeline.selected.len()), 2);
    let from = cx.debug_bounds(b).unwrap().center();
    drag(cx, from, 70.0, Modifiers::none());
    let moved = store.load_timeline(&entry).unwrap();
    assert!(moved.clips[0].start > linked.clips[0].start);
    assert_eq!(moved.clips[1].start - moved.clips[0].start, 30);
    let from = cx.debug_bounds(a).unwrap().center();
    drag(cx, from, 35.0, Modifiers::alt());
    let solo = store.load_timeline(&entry).unwrap();
    assert_eq!(desktop.read_with(cx, |v, _| v.timeline.selected.len()), 1);
    assert_eq!(solo.clips[1], moved.clips[1]);
    assert!(solo.clips[0].start > moved.clips[0].start);
    click(cx, b, Modifiers::none());
    assert_eq!(desktop.read_with(cx, |v, _| v.timeline.selected.len()), 2);
    click(cx, "timeline-unlink-clips", Modifiers::none());
    let unlinked = store.load_timeline(&entry).unwrap();
    assert!(unlinked.clips.iter().all(|c| c.link.is_none()));
    cx.simulate_keystrokes("ctrl-z");
    cx.run_until_parked();
    assert_eq!(store.load_timeline(&entry).unwrap(), solo);
    cx.simulate_keystrokes("ctrl-shift-z");
    cx.run_until_parked();
    assert_eq!(store.load_timeline(&entry).unwrap(), unlinked);
    // 解除后重新选择单段，再拖动，避免保留多选被误认作关联仍生效。
    desktop.update_in(cx, |v, _, cx| {
        v.timeline.selected.clear();
        cx.notify();
    });
    cx.run_until_parked();
    let from = cx.debug_bounds(b).unwrap().center();
    drag(cx, from, 35.0, Modifiers::none());
    let independent = store.load_timeline(&entry).unwrap();
    assert_eq!(independent.clips[0], unlinked.clips[0]);
    assert!(independent.clips[1].start > unlinked.clips[1].start);
    std::fs::remove_dir_all(root).unwrap();
}

/// 两种插入顺序下实际点击关联视频/音频和预览；检查器跟随命中对象，变速只保存对应类型。
/// 组选择保持完整，Ctrl 取消选择后不读取过期主对象；不依赖消失控件的 debug_bounds 缓存判定。
#[gpui::test]
fn linked_inspector_follows_timeline_and_preview_target(cx: &mut TestAppContext) {
    for audio_first in [true, false] {
        let root = std::env::temp_dir().join(format!(
            "dinuosi-linked-inspector-{}-{audio_first}",
            std::process::id()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let store = ProjectStore::new(root.join("recent.json"));
        let entry = store.create("关联检查器", &root).unwrap();
        let (desktop, cx) = cx.add_window_view(|window, cx| {
            Desktop::new(store.clone(), Some(entry.clone()), window, cx)
        });
        cx.simulate_resize(size(px(1440.0), px(900.0)));
        cx.run_until_parked();
        let (audio, video) = desktop.update_in(cx, |v, _, cx| {
            let before = v.timeline.model.clone();
            for kind in if audio_first {
                [MediaKind::Audio, MediaKind::Video]
            } else {
                [MediaKind::Video, MediaKind::Audio]
            } {
                let name = if kind == MediaKind::Audio {
                    "linked.wav"
                } else {
                    "linked.mp4"
                };
                v.timeline
                    .model
                    .insert(
                        (name.into(), kind),
                        name.into(),
                        v.timeline.model.main_track(),
                        0,
                        0..150,
                        150,
                    )
                    .unwrap();
            }
            let audio = v
                .timeline
                .model
                .clips
                .iter()
                .find(|c| c.kind == MediaKind::Audio)
                .unwrap()
                .id
                .clone();
            let video = v
                .timeline
                .model
                .clips
                .iter()
                .find(|c| c.kind == MediaKind::Video)
                .unwrap()
                .id
                .clone();
            v.timeline
                .model
                .set_property(
                    &[video.clone()].into(),
                    dinuosi_core::timeline::Property::X,
                    160.0,
                )
                .unwrap();
            v.session.media.push(crate::media::Media::from_source(
                dinuosi_core::media::Media {
                    path: "linked.mp4".into(),
                    name: "linked.mp4".into(),
                    kind: MediaKind::Video,
                    duration: 5.0,
                    has_audio: false,
                    preview_size: (640, 360),
                    source_size: (1920, 1080),
                    offline: false,
                    thumbnail: None,
                },
            ));
            v.timeline.preview = true;
            v.commit_timeline(before, cx);
            (audio, video)
        });
        let audio_selector = Box::leak(format!("clip-{audio}").into_boxed_str());
        let video_selector = Box::leak(format!("clip-{video}").into_boxed_str());
        cx.run_until_parked();
        click(cx, audio_selector, Modifiers::none());
        click(cx, video_selector, Modifiers::control());
        click(cx, "timeline-link-clips", Modifiers::none());
        let linked = store.load_timeline(&entry).unwrap();
        click(cx, video_selector, Modifiers::none());
        desktop.read_with(cx, |v, _| {
            assert_eq!(v.timeline.selected.len(), 2);
            assert_eq!(v.inspector_clip().unwrap().id, video);
            assert_eq!(v.speed_clip().unwrap().kind, MediaKind::Video);
            assert_eq!(v.property_value(dinuosi_core::timeline::Property::X), 160.0);
        });
        click(cx, "inspector-value-X", Modifiers::none());
        cx.simulate_keystrokes("ctrl-a");
        cx.simulate_input("240");
        cx.simulate_keystrokes("enter");
        cx.run_until_parked();
        let positioned = store.load_timeline(&entry).unwrap();
        assert_eq!(
            positioned
                .clips
                .iter()
                .find(|c| c.id == video)
                .unwrap()
                .transform
                .x,
            240.0
        );
        assert_eq!(
            positioned.clips.iter().find(|c| c.id == audio),
            linked.clips.iter().find(|c| c.id == audio)
        );
        click(cx, "inspector-speed-tab", Modifiers::none());
        click(cx, "speed-rate", Modifiers::none());
        cx.simulate_keystrokes("ctrl-a");
        cx.simulate_input("2");
        cx.simulate_keystrokes("enter");
        cx.run_until_parked();
        let sped = store.load_timeline(&entry).unwrap();
        assert_eq!(
            sped.clips.iter().find(|c| c.id == video).unwrap().duration,
            75
        );
        assert_eq!(
            sped.clips.iter().find(|c| c.id == audio),
            linked.clips.iter().find(|c| c.id == audio)
        );
        click(cx, audio_selector, Modifiers::none());
        desktop.read_with(cx, |v, _| {
            assert_eq!(v.timeline.selected.len(), 2);
            assert_eq!(v.inspector_clip().unwrap().id, audio);
            assert_eq!(v.speed_clip().unwrap().kind, MediaKind::Audio);
        });
        click(cx, "speed-rate", Modifiers::none());
        cx.simulate_keystrokes("ctrl-a");
        cx.simulate_input("3");
        cx.simulate_keystrokes("enter");
        cx.run_until_parked();
        let sped_audio = store.load_timeline(&entry).unwrap();
        assert_eq!(
            sped_audio
                .clips
                .iter()
                .find(|c| c.id == audio)
                .unwrap()
                .duration,
            50
        );
        assert_eq!(
            sped_audio.clips.iter().find(|c| c.id == video),
            sped.clips.iter().find(|c| c.id == video)
        );
        click(cx, audio_selector, Modifiers::control());
        assert!(desktop.read_with(cx, |v, _| v.inspector_clip().is_none()));
        let preview = desktop.read_with(cx, |v, _| v.preview_shapes()[0].geometry.center);
        cx.simulate_click(preview, Modifiers::none());
        cx.run_until_parked();
        desktop.read_with(cx, |v, _| {
            assert_eq!(v.timeline.selected.len(), 2);
            assert_eq!(v.inspector_clip().unwrap().id, video);
            assert_eq!(v.speed_clip().unwrap().kind, MediaKind::Video);
        });
        std::fs::remove_dir_all(root).unwrap();
    }
}
