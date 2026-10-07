//! 使用实际 GPUI 输入和定时器验证拖拽边缘滚动、片段间插入及空轨道事务。
use crate::{media::Media, view::Desktop};
use dinuosi_core::{
    ProjectEntry, ProjectStore,
    media::{FRAME_HEIGHT, FRAME_WIDTH, MediaKind},
    timeline::TrackKind,
};
use gpui::{
    Entity, Modifiers, MouseButton, Pixels, Point, TestAppContext, VisualTestContext, point, px,
    size,
};
use std::{path::PathBuf, time::Duration};

/// 创建隔离项目和缓存图片，按需放入多条有内容的轨道；每条片段从第 60 帧开始。
/// 返回窗口、存储和测试目录供真实指针、持久化断言及清理使用，不依赖 FFmpeg。
fn fixture<'a>(
    cx: &'a mut TestAppContext,
    name: &str,
    tracks: usize,
) -> (
    Entity<Desktop>,
    &'a mut VisualTestContext,
    ProjectStore,
    ProjectEntry,
    PathBuf,
) {
    let root = std::env::temp_dir().join(format!("dinuosi-{name}-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let source = root.join("scroll.png");
    // 可用性监测只检查在线源是否存在；画面使用下方缓存，不需要外部解码器。
    std::fs::write(&source, b"cached image fixture").unwrap();
    let store = ProjectStore::new(root.join("recent.json"));
    let entry = store.create(name, &root).unwrap();
    let (desktop, cx) = cx
        .add_window_view(|window, cx| Desktop::new(store.clone(), Some(entry.clone()), window, cx));
    cx.simulate_resize(size(px(1440.0), px(900.0)));
    desktop.update_in(cx, |v, _, cx| {
        v.zoom = 0.5;
        v.session
            .media
            .push(Media::from_source(dinuosi_core::media::Media {
                path: source.clone(),
                name: "滚动图片".into(),
                kind: MediaKind::Image,
                duration: 2.0,
                has_audio: false,
                preview_size: (640, 360),
                source_size: (1920, 1080),
                offline: false,
                thumbnail: Some(std::sync::Arc::new(
                    [40, 70, 180, 255].repeat((FRAME_WIDTH * FRAME_HEIGHT) as usize),
                )),
            }));
        for _ in 1..tracks {
            v.timeline.model.create_track(0, TrackKind::Media).unwrap();
        }
        for track in 0..tracks {
            v.timeline
                .model
                .insert(
                    (source.clone(), MediaKind::Image),
                    "滚动图片".into(),
                    track,
                    60,
                    0..30,
                    60,
                )
                .unwrap();
        }
        v.commit_timeline(Default::default(), cx);
    });
    cx.run_until_parked();
    (desktop, cx, store, entry, root)
}

/// 只推进真实定时器，不发送新的鼠标移动；用来证明鼠标停在边缘仍持续滚动。
fn hold(cx: &mut VisualTestContext, ticks: usize) {
    for _ in 0..ticks {
        cx.background_executor
            .advance_clock(Duration::from_millis(16));
        cx.run_until_parked();
    }
}

/// 发送一次完整窗口拖放；释放触发核心提交，随后等待持久化与重绘完成。
fn drag(cx: &mut VisualTestContext, from: Point<Pixels>, to: Point<Pixels>) {
    cx.simulate_mouse_down(from, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_move(to, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_up(to, MouseButton::Left, Modifiers::none());
    cx.run_until_parked();
}

/// 框选向右下/左上持续滚动且原始内容锚点不漂移；Ctrl 滚轮只改变横向，释放取消定时器。
#[gpui::test]
fn marquee_autoscroll_preserves_anchor_and_ctrl_wheel_is_horizontal(cx: &mut TestAppContext) {
    let (desktop, cx, _, _, root) = fixture(cx, "框选边缘", 10);
    let (start, edge, id) = desktop.read_with(cx, |v, _| {
        let (y, _) = v.track_geometry(0);
        (
            point(
                v.timeline.bounds.left() + px(v.timeline.header + v.pixels_per_second()),
                v.timeline.bounds.top() + px(y + 10.0),
            ),
            point(
                v.timeline.bounds.right() - px(20.0),
                v.timeline.bounds.bottom() - px(15.0),
            ),
            v.timeline.model.clips[0].id.clone(),
        )
    });
    cx.simulate_mouse_down(start, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_move(edge, MouseButton::Left, Modifiers::none());
    hold(cx, 35);
    desktop.read_with(cx, |v, _| {
        assert!(v.timeline.scroll_x > 400.0 && v.timeline.scroll_y > 300.0);
        assert!(
            v.timeline.selected.contains(&id),
            "滚动后框选起点不能跟着视口漂移"
        );
    });
    let reverse = desktop.read_with(cx, |v, _| {
        point(
            v.timeline.bounds.left() + px(v.timeline.header + 2.0),
            v.timeline.bounds.top() + px(40.0),
        )
    });
    cx.simulate_mouse_move(reverse, MouseButton::Left, Modifiers::none());
    hold(cx, 40);
    assert_eq!(
        desktop.read_with(cx, |v, _| (v.timeline.scroll_x, v.timeline.scroll_y)),
        (0.0, 0.0)
    );
    cx.simulate_mouse_up(reverse, MouseButton::Left, Modifiers::none());
    assert!(desktop.read_with(cx, |v, _| v.timeline.scroll_task.is_none()));
    desktop.update_in(cx, |v, _, cx| {
        v.timeline.scroll_x = 100.0;
        v.timeline.scroll_y = 100.0;
        cx.notify();
    });
    cx.run_until_parked();
    let wheel = desktop.read_with(cx, |v, _| {
        point(
            v.timeline.bounds.left() + px(v.timeline.header + 100.0),
            v.timeline.bounds.top() + px(20.0),
        )
    });
    cx.simulate_event(gpui::ScrollWheelEvent {
        position: wheel,
        delta: gpui::ScrollDelta::Pixels(point(px(0.0), px(-120.0))),
        modifiers: Modifiers::control(),
        ..Default::default()
    });
    assert_eq!(
        desktop.read_with(cx, |v, _| (
            v.timeline.scroll_x,
            v.timeline.scroll_y,
            v.zoom
        )),
        (220.0, 100.0, 0.5)
    );
    hold(cx, 5);
    assert_eq!(desktop.read_with(cx, |v, _| v.timeline.scroll_x), 220.0);
    std::fs::remove_dir_all(root).unwrap();
}

/// 片段、播放头和库素材停在右边缘持续滚动；浮块跟随鼠标，滚动差计入最终帧位置。
/// 库素材拖入竖向屏外的主轨后保存到正确行，原轨道和已有素材不被覆盖。
#[gpui::test]
fn clip_playhead_and_library_autoscroll_reach_offscreen_drop_positions(cx: &mut TestAppContext) {
    let (desktop, cx, store, entry, root) = fixture(cx, "拖放边缘", 10);
    let id = desktop.read_with(cx, |v, _| v.timeline.model.clips[0].id.clone());
    let selector = Box::leak(format!("clip-{id}").into_boxed_str());
    let clip = cx.debug_bounds(selector).unwrap();
    let from = clip.center();
    let edge = desktop.read_with(cx, |v, _| {
        point(v.timeline.bounds.right() - px(20.0), from.y)
    });
    cx.simulate_mouse_down(from, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_move(edge, MouseButton::Left, Modifiers::none());
    hold(cx, 35);
    let ghost = cx.debug_bounds("timeline-drag-ghost").unwrap();
    assert!((ghost.left() - clip.left() - (edge.x - from.x)).abs() < px(1.0));
    let expected = desktop.read_with(cx, |v, _| {
        60 + ((f32::from(edge.x - from.x) + v.timeline.scroll_x) / v.pixels_per_second()
            * v.timeline.model.fps as f32)
            .round() as i64
    });
    cx.simulate_mouse_up(edge, MouseButton::Left, Modifiers::none());
    cx.run_until_parked();
    assert_eq!(
        store
            .load_timeline(&entry)
            .unwrap()
            .clips
            .iter()
            .find(|c| c.id == id)
            .unwrap()
            .start,
        expected
    );
    let card = cx.debug_bounds("media-0").unwrap().center();
    let edge = desktop.read_with(cx, |v, _| {
        point(
            v.timeline.bounds.right() - px(20.0),
            v.timeline.bounds.bottom() - px(15.0),
        )
    });
    let before = desktop.read_with(cx, |v, _| (v.timeline.scroll_x, v.timeline.scroll_y));
    cx.simulate_mouse_down(card, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_move(edge, MouseButton::Left, Modifiers::none());
    hold(cx, 50);
    desktop.read_with(cx, |v, _| {
        assert!(v.timeline.scroll_x > before.0 + 600.0);
        assert!(v.timeline.scroll_y > before.1);
    });
    let drop = desktop.read_with(cx, |v, _| {
        let (top, h) = v.track_geometry(v.timeline.model.main_track());
        point(
            v.timeline.bounds.right() - px(60.0),
            v.timeline.bounds.top() + px(top + h / 2.0),
        )
    });
    cx.simulate_mouse_move(drop, MouseButton::Left, Modifiers::none());
    desktop.read_with(cx, |v, _| {
        assert!(matches!(
            v.timeline.gesture,
            Some(crate::timeline_state::Gesture::Library { .. })
        ));
        assert!(v.timeline.moved);
        assert!(
            v.timeline_drop_target(drop).is_some(),
            "主轨未进入视口: drop={drop:?}, bounds={:?}, scroll_y={}, height={}",
            v.timeline.bounds,
            v.timeline.scroll_y,
            v.timeline_content_height()
        );
    });
    let frame = desktop.read_with(cx, |v, _| v.timeline_frame_at(drop.x));
    cx.simulate_mouse_up(drop, MouseButton::Left, Modifiers::none());
    cx.run_until_parked();
    desktop.read_with(cx, |v, _| {
        assert!(
            v.timeline.gesture.is_none(),
            "释放未送达: {drop:?}, bounds={:?}",
            v.timeline.bounds
        );
        assert!(!v.session.media[0].offline);
    });
    let saved = store.load_timeline(&entry).unwrap();
    assert_eq!(
        saved.clips.len(),
        11,
        "保存错误: {:?}",
        desktop.read_with(cx, |v, _| v.notice.clone())
    );
    assert!(
        saved
            .clips
            .iter()
            .any(|c| c.track == saved.main_track() && c.start == frame)
    );
    let seek = cx.debug_bounds("timeline-seek").unwrap();
    let right = point(seek.right() - px(2.0), seek.center().y);
    let before = desktop.read_with(cx, |v, _| v.timeline.scroll_x);
    cx.simulate_mouse_down(seek.center(), MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_move(right, MouseButton::Left, Modifiers::none());
    hold(cx, 20);
    desktop.read_with(cx, |v, _| {
        assert!(v.timeline.scroll_x > before + 200.0);
        assert!(
            (v.session.timeline_position * v.timeline.model.fps as f64
                - v.timeline_frame_at(right.x) as f64)
                .abs()
                < 0.01
        );
    });
    cx.simulate_mouse_up(right, MouseButton::Left, Modifiers::none());
    assert!(desktop.read_with(cx, |v, _| v.timeline.scroll_task.is_none()));
    std::fs::remove_dir_all(root).unwrap();
}

/// 从素材库拖到首尾相接的片段边界，后续片段后移；新插轨、清空和撤销属于单次编辑事务。
#[gpui::test]
fn pointer_insertion_pushes_following_clips_and_empty_track_cleanup_is_undoable(
    cx: &mut TestAppContext,
) {
    let (desktop, cx, store, entry, root) = fixture(cx, "片段间插入", 1);
    desktop.update_in(cx, |v, _, cx| {
        v.timeline
            .model
            .insert(
                (v.session.media[0].path.clone(), MediaKind::Image),
                "第二段".into(),
                0,
                90,
                0..30,
                60,
            )
            .unwrap();
        v.commit_timeline(Default::default(), cx);
    });
    cx.run_until_parked();
    let before = store.load_timeline(&entry).unwrap();
    let card = cx.debug_bounds("media-0").unwrap().center();
    let boundary = desktop.read_with(cx, |v, _| {
        let (top, h) = v.track_geometry(0);
        point(
            v.timeline.bounds.left() + px(v.timeline.header + 3.0 * v.pixels_per_second()),
            v.timeline.bounds.top() + px(top + h / 2.0),
        )
    });
    cx.simulate_mouse_down(card, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_move(boundary, MouseButton::Left, Modifiers::none());
    assert!(cx.debug_bounds("timeline-clip-insert-line").is_some());
    cx.simulate_mouse_up(boundary, MouseButton::Left, Modifiers::none());
    cx.run_until_parked();
    let saved = store.load_timeline(&entry).unwrap();
    assert_eq!(saved.tracks.len(), 1);
    assert_eq!(
        saved.clips.iter().map(|c| c.start).collect::<Vec<_>>(),
        [60, 150, 90]
    );
    cx.simulate_keystrokes("ctrl-z");
    cx.run_until_parked();
    assert_eq!(store.load_timeline(&entry).unwrap(), before);
    cx.simulate_keystrokes("ctrl-y");
    cx.run_until_parked();
    assert_eq!(store.load_timeline(&entry).unwrap(), saved);
    let id = saved.clips[2].id.clone();
    let selector = Box::leak(format!("clip-{id}").into_boxed_str());
    let from = cx.debug_bounds(selector).unwrap().center();
    drag(cx, from, from - point(px(0.0), px(80.0)));
    assert_eq!(store.load_timeline(&entry).unwrap().tracks.len(), 2);
    cx.simulate_keystrokes("delete");
    cx.run_until_parked();
    assert_eq!(store.load_timeline(&entry).unwrap().tracks.len(), 1);
    cx.simulate_keystrokes("ctrl-z");
    cx.run_until_parked();
    assert_eq!(store.load_timeline(&entry).unwrap().tracks.len(), 2);
    std::fs::remove_dir_all(root).unwrap();
}
