use crate::view::Desktop;
use dinuosi_core::{ProjectStore, media};
use gpui::{Modifiers, MouseButton, TestAppContext, point, px, size};
use std::sync::Arc;

/// 用真实导入及 GPUI 点击/缩放/裁边事件验证两处波形共用缓存、源入点、离线恢复及重开。
/// 需要本机 FFmpeg；仅使用独立临时项目，不访问音频设备或修改用户项目。
#[gpui::test]
#[ignore = "需要本机 FFmpeg 和 ffprobe"]
fn waveform_preview_and_timeline_follow_real_audio(cx: &mut TestAppContext) {
    let root = std::env::temp_dir().join(format!("dinuosi-wave-ui-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let path = root.join("音频.wav");
    assert!(
        media::command("ffmpeg")
            .args([
                "-v",
                "error",
                "-y",
                "-f",
                "lavfi",
                "-i",
                "aevalsrc=if(lt(t\\,1)\\,0.1\\,0.7)*sin(2*PI*440*t):s=48000:d=4"
            ])
            .arg(&path)
            .status()
            .unwrap()
            .success()
    );
    let store = ProjectStore::new(root.join("recent.json"));
    let entry = store.create("波形验证", &root).unwrap();
    let (desktop, cx) = cx
        .add_window_view(|window, cx| Desktop::new(store.clone(), Some(entry.clone()), window, cx));
    cx.simulate_resize(size(px(1440.0), px(900.0)));
    cx.run_until_parked();
    desktop.update_in(cx, |v, window, cx| {
        v.import_media(vec![path.clone()], window, cx)
    });
    cx.run_until_parked();
    let cached = desktop.read_with(cx, |v, _| v.session.media[0].waveform.clone().unwrap());
    assert!(cached.peak(1.5, 1.6).max > cached.peak(0.1, 0.2).max * 5.0);
    let card = cx.debug_bounds("media-0").unwrap().center();
    cx.simulate_click(card, Modifiers::none());
    cx.run_until_parked();
    let viewport = cx.debug_bounds("preview-viewport").unwrap();
    assert_eq!(cx.debug_bounds("preview-waveform").unwrap(), viewport);
    let strip = cx.debug_bounds("preview-seek").unwrap();
    assert_eq!(cx.debug_bounds("preview-strip-waveform").unwrap(), strip);
    cx.simulate_click(strip.center(), Modifiers::none());
    assert!((desktop.read_with(cx, |v, _| v.session.position) - 2.0).abs() < 0.01);
    let add = cx.debug_bounds("add-to-track").unwrap().center();
    cx.simulate_click(add, Modifiers::none());
    cx.run_until_parked();
    let id = desktop.read_with(cx, |v, _| v.timeline.model.clips[0].id.clone());
    let wave_selector: &'static str = Box::leak(format!("waveform-{id}").into_boxed_str());
    let clip_selector: &'static str = Box::leak(format!("clip-{id}").into_boxed_str());
    let trim_selector: &'static str = Box::leak(format!("trim-{id}-true").into_boxed_str());
    let wave = cx.debug_bounds(wave_selector).unwrap();
    let clip = cx.debug_bounds(clip_selector).unwrap();
    assert!(wave.top() > clip.top() && wave.bottom() < clip.bottom());
    let trim = cx.debug_bounds(trim_selector).unwrap().center();
    let pps = desktop.read_with(cx, |v, _| v.pixels_per_second());
    let target = trim + point(px(pps * 0.5), px(0.0));
    cx.simulate_mouse_down(trim, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_move(target, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_up(target, MouseButton::Left, Modifiers::none());
    cx.run_until_parked();
    assert_eq!(store.load_timeline(&entry).unwrap().clips[0].source_in, 15);
    let zoom = cx.debug_bounds("zoom-slider").unwrap();
    cx.simulate_mouse_down(zoom.center(), MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_move(
        point(zoom.right(), zoom.center().y),
        MouseButton::Left,
        Modifiers::none(),
    );
    cx.simulate_mouse_up(
        point(zoom.right(), zoom.center().y),
        MouseButton::Left,
        Modifiers::none(),
    );
    cx.run_until_parked();
    assert_eq!(desktop.read_with(cx, |v, _| v.pixels_per_second()), 1800.0);
    assert!(Arc::ptr_eq(
        &cached,
        &desktop.read_with(cx, |v, _| v.session.media[0].waveform.clone().unwrap())
    ));
    desktop.update_in(cx, |v, _, cx| {
        v.timeline.scroll_x = 1800.0;
        cx.notify();
    });
    cx.run_until_parked();
    assert!(cx.debug_bounds(wave_selector).unwrap().size.width > px(900.0));
    let source = desktop.read_with(cx, |v, _| v.session.media[0].path.clone());
    desktop.update_in(cx, |v, window, cx| {
        v.apply_media_status(
            vec![crate::media::Media::from_source(media::offline(
                source.clone(),
            ))],
            window,
            cx,
        )
    });
    assert!(desktop.read_with(cx, |v, _| v.session.media[0].waveform.is_none()));
    let restored = media::probe(&source).unwrap();
    desktop.update_in(cx, |v, window, cx| {
        v.apply_media_status(vec![crate::media::Media::from_source(restored)], window, cx)
    });
    cx.run_until_parked();
    assert!(desktop.read_with(cx, |v, _| v.session.media[0].waveform.is_some()));
    let (reopened, cx) = cx
        .add_window_view(|window, cx| Desktop::new(store.clone(), Some(entry.clone()), window, cx));
    cx.run_until_parked();
    assert!(reopened.read_with(cx, |v, _| v.session.media[0].waveform.is_some()));
    assert_eq!(store.load_timeline(&entry).unwrap().clips.len(), 1);
    std::fs::remove_dir_all(root).unwrap();
}
