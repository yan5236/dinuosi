//! HSL 渐变语义、取样坐标及真实 GPUI 鼠标/取消/持久化回归。
use crate::{
    color_hsl::{PickFrame, sample_hue, track_color},
    view::Desktop,
};
use dinuosi_core::{ProjectStore, color::Control, media::MediaKind};
use gpui::{Bounds, Modifiers, TestAppContext, VisualTestContext, point, px, size};

/// 从最新布局点击实际控件并等待后台与重绘，目标缺失即失败。
fn click(cx: &mut VisualTestContext, id: &'static str) {
    let p = cx.debug_bounds(id).expect(id).center();
    cx.simulate_click(p, Modifiers::none());
    cx.run_until_parked();
}
/// 验证八色与取样色组的 H/S/L 渐变不同且连续，灰色/明暗端点和中心色符合工具语义。
#[test]
fn hsl_tracks_follow_group_and_parameter() {
    for group in 0..9 {
        let control = |i| {
            if group == 8 {
                Control::PickedHsl(i)
            } else {
                Control::Hsl(group, i)
            }
        };
        assert_ne!(
            track_color(control(0), Some(203.0), 0.0),
            track_color(control(0), Some(203.0), 1.0)
        );
        let gray = track_color(control(1), Some(203.0), 0.0);
        assert_eq!((gray >> 16) & 255, gray & 255);
        assert_eq!((gray >> 8) & 255, gray & 255);
        assert_ne!(gray, track_color(control(1), Some(203.0), 1.0));
        assert!(
            track_color(control(2), Some(203.0), 0.0) < track_color(control(2), Some(203.0), 1.0)
        );
    }
    assert_ne!(
        track_color(Control::Hsl(0, 1), None, 1.0),
        track_color(Control::Hsl(5, 1), None, 1.0)
    );
}
/// 核对竖屏裁剪中心补边、BGRA 顺序与四个边界；留白、灰色、透明和错误尺寸不能产生任意色相。
#[test]
fn hsl_sampler_maps_displayed_pixels() {
    let bounds = Bounds::new(point(px(100.0), px(60.0)), size(px(180.0), px(360.0)));
    let mut frame = PickFrame {
        pixels: [0, 0, 255, 255].repeat(640 * 360),
        size: (180, 360),
        frame: 0,
        revision: 0,
    };
    for y in 0..360 {
        for x in 230..410 {
            frame.pixels[(y * 640 + x) * 4..][..4].copy_from_slice(&[255, 0, 0, 255]);
        }
    }
    for p in [
        bounds.origin,
        bounds.center(),
        point(bounds.right() - px(0.1), bounds.bottom() - px(0.1)),
    ] {
        assert_eq!(sample_hue(&frame, bounds, p), Some(240.0));
    }
    assert!(sample_hue(&frame, bounds, point(px(99.0), px(60.0))).is_none());
    frame.pixels = [120, 120, 120, 255].repeat(640 * 360);
    assert!(sample_hue(&frame, bounds, bounds.center()).is_none());
    frame.pixels = [0, 0, 255, 0].repeat(640 * 360);
    assert!(sample_hue(&frame, bounds, bounds.center()).is_none());
    frame.pixels.clear();
    assert!(sample_hue(&frame, bounds, bounds.center()).is_none());
}
/// 在真实 GPUI 布局中用吸管取蓝色：保留选择/变换及原八色，生成独立参数和关键帧，取消释放像素。
#[gpui::test]
fn hsl_picker_selects_custom_group_and_preserves_clip(cx: &mut TestAppContext) {
    let root = std::env::temp_dir().join(format!("dinuosi-hsl-ui-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let store = ProjectStore::new(root.join("recent.json"));
    let entry = store.create("HSL", &root).unwrap();
    let (desktop, cx) =
        cx.add_window_view(|w, cx| Desktop::new(store.clone(), Some(entry.clone()), w, cx));
    cx.simulate_resize(size(px(1440.0), px(900.0)));
    cx.run_until_parked();
    desktop.update_in(cx, |v, _, cx| {
        let path = std::path::PathBuf::from("hsl.png");
        let pixels: Vec<u8> = (0..640 * 360)
            .flat_map(|i| {
                if i % 640 < 213 {
                    [0, 0, 255, 255]
                } else if i % 640 < 426 {
                    [0, 255, 0, 255]
                } else {
                    [255, 0, 0, 255]
                }
            })
            .collect();
        v.session.media.push(crate::media::Media::from_source(
            dinuosi_core::media::Media {
                path: path.clone(),
                name: "HSL".into(),
                kind: MediaKind::Image,
                duration: 5.0,
                has_audio: false,
                preview_size: (640, 360),
                source_size: (1280, 720),
                offline: false,
                thumbnail: Some(std::sync::Arc::new(pixels)),
            },
        ));
        let before = v.timeline.model.clone();
        let id = v
            .timeline
            .model
            .insert((path, MediaKind::Image), "图".into(), 0, 0, 0..150, 150)
            .unwrap();
        v.timeline.selected = [id.clone()].into();
        v.timeline.primary_clip = Some(id);
        v.timeline.preview = true;
        v.commit_timeline(before, cx);
    });
    cx.run_until_parked();
    click(cx, "inspector-color-tab");
    click(cx, "color-tool-1");
    let original = store.load_timeline(&entry).unwrap();
    click(cx, "color-hsl-picker");
    assert!(desktop.read_with(cx, |v, _| v.hsl_picking()
        && v.timeline.color.pick_frame.is_some()));
    let bounds = desktop.read_with(cx, |v, _| v.timeline.preview_bounds);
    let blue = point(bounds.left() + bounds.size.width * 0.75, bounds.center().y);
    desktop.read_with(cx, |v, _| {
        let frame = v.timeline.color.pick_frame.as_ref().unwrap();
        assert_eq!(sample_hue(frame, bounds, blue), Some(240.0));
        assert_eq!(frame.revision, v.timeline.revision);
    });
    cx.simulate_click(blue, Modifiers::none());
    cx.run_until_parked();
    let picked = store.load_timeline(&entry).unwrap();
    let sample = picked.clips[0].color.stages[0].picked.as_ref().unwrap();
    assert_eq!(sample.hue, 240.0);
    assert_eq!(sample.values, [0.0; 3]);
    assert_eq!(picked.clips[0].transform, original.clips[0].transform);
    assert_eq!(
        picked.clips[0].color.stages[0].hsl,
        original.clips[0].color.stages[0].hsl
    );
    assert!(desktop.read_with(cx, |v, _| v.timeline.color.group == 8
        && v.timeline.gesture.is_none()
        && v.timeline.selected.contains(&original.clips[0].id)
        && v.timeline.color.pick_frame.is_none()));
    click(cx, "keyframe-Color(0, PickedHsl(1))-0-empty");
    assert_eq!(
        store.load_timeline(&entry).unwrap().clips[0].keyframes[0].kind,
        dinuosi_core::keyframes::KeyframeKind::Color(0, Control::PickedHsl(1))
    );
    click(cx, "color-hsl-0");
    assert!(cx.debug_bounds("color-slider-hsl-0-1").is_some());
    click(cx, "color-hsl-picker");
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    assert!(
        desktop.read_with(cx, |v, _| v.timeline.color.picker.is_none()
            && v.timeline.color.pick_frame.is_none())
    );
    cx.simulate_resize(size(px(1024.0), px(680.0)));
    cx.run_until_parked();
    for id in ["color-hsl-picker", "color-hsl-7", "color-hsl-8"] {
        assert!(cx.debug_bounds(id).unwrap().right() <= px(1024.0));
    }
    std::fs::remove_dir_all(root).unwrap();
}
