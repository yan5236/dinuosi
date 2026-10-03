//! 验证全览缩放随时长和窗口宽度适配，以及真实滑块/按钮回到时间零点。
use crate::view::Desktop;
use dinuosi_core::{ProjectStore, media::MediaKind, timeline::TrackKind};
use gpui::{Modifiers, MouseButton, TestAppContext, point, px, size};

/// 覆盖空项目、单帧、短片、长音频及一天时间线；窗口变化后首尾和右侧把手均留在可见区。
/// 时间范围按最晚结束而非最长单片计算，标尺必须覆盖整屏且主标签不挤在一起。
#[gpui::test]
fn overview_fits_all_content_and_ruler_at_each_window_size(cx: &mut TestAppContext) {
    let root = std::env::temp_dir().join(format!("dinuosi-zoom-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let store = ProjectStore::new(root.join("recent.json"));
    let entry = store.create("全览缩放", &root).unwrap();
    let (desktop, cx) =
        cx.add_window_view(|window, cx| Desktop::new(store, Some(entry), window, cx));
    desktop.update_in(cx, |v, _, cx| {
        v.timeline.model.create_track(0, TrackKind::Audio).unwrap();
        v.change_zoom(-10.0, cx);
    });
    for end in [0, 1, 882, 7719, 108000, 2592000] {
        desktop.update_in(cx, |v, _, cx| {
            v.timeline.model.clips.clear();
            if end > 0 {
                let start = end / 3;
                let id = v
                    .timeline
                    .model
                    .insert(
                        ("overview.wav".into(), MediaKind::Audio),
                        "全览片段".into(),
                        0,
                        start,
                        0..end - start,
                        end,
                    )
                    .unwrap();
                let short = v
                    .timeline
                    .model
                    .insert(
                        ("short.png".into(), MediaKind::Image),
                        "短片段".into(),
                        1,
                        0,
                        0..end.min(90),
                        end,
                    )
                    .unwrap();
                v.timeline.selected = [id, short].into();
            }
            cx.notify();
        });
        for width in [1440.0, 1040.0, 2560.0] {
            cx.simulate_resize(size(px(width), px(900.0)));
            cx.run_until_parked();
            let ruler = cx.debug_bounds("timeline-seek").unwrap();
            desktop.read_with(cx, |v, _| {
                let pps = v.pixels_per_second();
                let visible = v.timeline_view_width();
                assert!((visible - f32::from(ruler.size.width)).abs() < 1.0);
                assert!(pps.is_finite() && pps > 0.0);
                let duration = end as f32 / 30.0;
                assert!(duration * pps < visible);
                if end >= 882 {
                    assert!((duration * pps / visible - 1.0 / 1.15).abs() < 0.001);
                }
                if end == 0 {
                    assert!((visible / pps - 60.0).abs() < 0.001);
                }
                let ticks = dinuosi_core::timeline::ticks(30, pps as f64, 0.0, visible as f64);
                let last = ticks.last().unwrap().frame as f32 / 30.0 * pps;
                assert!(last > visible * 0.8, "标尺应覆盖视口右侧");
                let major: Vec<_> = ticks.iter().filter(|t| t.major).collect();
                for pair in major.windows(2) {
                    assert!((pair[1].frame - pair[0].frame) as f32 / 30.0 * pps >= 89.9);
                }
            });
            for id in desktop.read_with(cx, |v, _| {
                v.timeline
                    .model
                    .clips
                    .iter()
                    .map(|c| c.id.clone())
                    .collect::<Vec<_>>()
            }) {
                let handle = cx
                    .debug_bounds(Box::leak(format!("trim-{id}-false").into_boxed_str()))
                    .unwrap();
                assert!(handle.right() <= ruler.right() + px(crate::trim_handle::WIDTH));
            }
        }
    }
    // 从放大且滚动后的视口拖至最小档，必须重新显示零点；按钮也走相同行为。
    desktop.update_in(cx, |v, _, cx| {
        v.change_zoom(2.0, cx);
        v.timeline.scroll_x = 500.0;
    });
    cx.run_until_parked();
    let slider = cx.debug_bounds("zoom-slider").unwrap();
    let left = point(slider.left(), slider.center().y);
    cx.simulate_mouse_down(slider.center(), MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_move(left, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_up(left, MouseButton::Left, Modifiers::none());
    cx.run_until_parked();
    desktop.read_with(cx, |v, _| {
        assert_eq!(v.zoom, 0.5);
        assert_eq!(v.timeline.scroll_x, 0.0);
    });
    let zoom_in = cx.debug_bounds("zoom-in").unwrap().center();
    cx.simulate_click(zoom_in, Modifiers::none());
    desktop.update_in(cx, |v, _, _| v.timeline.scroll_x = 500.0);
    let zoom_out = cx.debug_bounds("zoom-out").unwrap().center();
    cx.simulate_click(zoom_out, Modifiers::none());
    desktop.read_with(cx, |v, _| {
        assert_eq!(v.zoom, 0.5);
        assert_eq!(v.timeline.scroll_x, 0.0);
    });
    // 裁边中使用按下快照，释放快照后按新时长适配。
    desktop.update_in(cx, |v, _, _| {
        let pps = v.pixels_per_second();
        v.timeline.before = Some(v.timeline.model.clone());
        v.timeline.model.clips[0].duration /= 2;
        assert_eq!(v.pixels_per_second(), pps);
        v.timeline.before = None;
        assert!(v.pixels_per_second() > pps);
        v.zoom = 4.0;
        assert!((v.pixels_per_second() - 1800.0).abs() < 0.01);
    });
    std::fs::remove_dir_all(root).unwrap();
}
