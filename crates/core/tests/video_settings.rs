//! 画幅适配、码率和旧项目兼容回归；真实媒体检查需显式启用配套运行库。
use dinuosi_core::{
    ProjectStore, composite,
    export::{self, Container, Encoder, ExportSettings},
    media::{self, MediaKind},
    timeline::Timeline,
    video_settings::{BitrateMode, MediaFit},
};
use std::{fs, io::Write, process::Stdio, sync::atomic::AtomicBool};

/// 验证横竖/大小素材的等比适配、填满裁切、仅缩小及目标码率边界，避免拉伸或整数溢出。
#[test]
fn fitting_and_quality_preserve_proportions() {
    assert_eq!(
        BitrateMode::High.bitrate(u32::MAX, u32::MAX, u32::MAX, 0),
        1_000_000
    );
    assert_eq!(
        MediaFit::Fit.size((320, 180), (1920, 1080)),
        (1920.0, 1080.0)
    );
    assert_eq!(
        MediaFit::Original.size((320, 180), (1920, 1080)),
        (320.0, 180.0)
    );
    assert_eq!(
        MediaFit::Fit.size((1920, 1080), (1080, 1920)),
        (1080.0, 607.5)
    );
    let filled = MediaFit::Fill.size((1920, 1080), (1080, 1920));
    assert_eq!(filled.1, 1920.0);
    assert!((filled.0 / filled.1 - 16.0 / 9.0).abs() < 1e-10);
    assert_eq!(
        MediaFit::Fit.size((2160, 3840), (1920, 1080)),
        (607.5, 1080.0)
    );
    assert_eq!(BitrateMode::Standard.bitrate(1920, 1080, 30, 7), 12000);
    assert_eq!(BitrateMode::Standard.bitrate(1080, 1920, 60, 7), 25000);
    assert_eq!(BitrateMode::High.bitrate(1920, 1080, 30, 7), 21000);
    assert_eq!(
        BitrateMode::Standard.bitrate(16384, 16384, 240, 7),
        1_000_000
    );
    assert_eq!(BitrateMode::Custom.bitrate(1920, 1080, 30, 12345), 12345);
}

/// 适配与质量随项目原子保存，缺字段旧项目保留原构图及数值码率；未知枚举不静默回退。
#[test]
fn project_modes_round_trip_and_preserve_legacy() {
    let root =
        std::env::temp_dir().join(format!("dinuosi-video-settings-{}", uuid::Uuid::new_v4()));
    fs::create_dir_all(&root).unwrap();
    let store = ProjectStore::new(root.join("recent.json"));
    let entry = store.create("画幅", &root).unwrap();
    let mut timeline = Timeline::default();
    assert_eq!(timeline.media_fit, MediaFit::Fit);
    for fit in [MediaFit::Fit, MediaFit::Fill, MediaFit::Original] {
        timeline.media_fit = fit;
        timeline.bitrate_mode = BitrateMode::High;
        store.save_timeline(&entry, &timeline).unwrap();
        assert_eq!(store.load_timeline(&entry).unwrap(), timeline);
    }
    let mut json: serde_json::Value =
        serde_json::from_slice(&fs::read(&entry.path).unwrap()).unwrap();
    let fields = json["timeline"].as_object_mut().unwrap();
    fields.remove("media_fit");
    fields.remove("bitrate_mode");
    fs::write(&entry.path, serde_json::to_vec(&json).unwrap()).unwrap();
    let old = store.load_timeline(&entry).unwrap();
    assert_eq!(old.media_fit, MediaFit::Original);
    assert_eq!(old.bitrate_mode, BitrateMode::Custom);
    assert_eq!(
        ExportSettings::for_timeline(&old, Encoder::Mpeg4).bitrate_kbps,
        12000
    );
    json["timeline"]["media_fit"] = "invalid".into();
    fs::write(&entry.path, serde_json::to_vec(&json).unwrap()).unwrap();
    assert!(store.load_timeline(&entry).is_err());
    fs::remove_dir_all(root).unwrap();
}

/// 从真实混合尺寸图像逐段导出三种画幅、三种适配，解码后与预览同坐标比色并检查帧率和尺寸。
/// 文件留在仓库忽略的验收目录供复查；工具仅生成夹具/读取结果，导出仍调用应用的动态库管线。
#[test]
#[ignore = "需要应用配套媒体动态库与测试用 ffmpeg/ffprobe"]
fn real_mixed_resolution_exports_match_preview() {
    let root =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.local/video-settings-qa");
    fs::create_dir_all(&root).unwrap();
    let mut sources = Vec::new();
    for (w, h) in [(64, 32), (32, 64), (16, 16), (1920, 1080)] {
        let video = w == 64;
        let frames = if video { 6 } else { 1 };
        let path = root.join(format!(
            "source-{w}x{h}.{}",
            if video { "mp4" } else { "bmp" }
        ));
        let mut pixels = Vec::new();
        for _ in 0..h {
            for x in 0..w {
                pixels.extend_from_slice(&match x * 3 / w {
                    0 => [0, 0, 240, 255],
                    1 => [0, 240, 0, 255],
                    _ => [240, 0, 0, 255],
                });
            }
        }
        let mut process = media::command("ffmpeg")
            .args([
                "-y",
                "-v",
                "error",
                "-threads",
                "1",
                "-f",
                "rawvideo",
                "-pixel_format",
                "bgra",
                "-video_size",
                &format!("{w}x{h}"),
                "-framerate",
                "30",
                "-i",
                "pipe:0",
                "-frames:v",
                &frames.to_string(),
                "-threads",
                "1",
            ])
            .args(if video {
                vec!["-c:v", "mpeg4", "-q:v", "1"]
            } else {
                vec![]
            })
            .arg(&path)
            .stdin(Stdio::piped())
            .spawn()
            .unwrap();
        process
            .stdin
            .take()
            .unwrap()
            .write_all(&pixels.repeat(frames))
            .unwrap();
        assert!(process.wait().unwrap().success());
        sources.push(media::probe(&path).unwrap());
    }
    let store = ProjectStore::new(root.join("gui-data/recent-projects.json"));
    let projects = root.join(format!("gui-projects-{}", uuid::Uuid::new_v4()));
    fs::create_dir_all(&projects).unwrap();
    let entry = store.create("画幅与素材适配验收", &projects).unwrap();
    store
        .add_media_paths(
            &entry,
            &sources.iter().map(|m| m.path.clone()).collect::<Vec<_>>(),
        )
        .unwrap();
    let mut gui = Timeline::default();
    for index in [3, 1, 2, 0] {
        let source = &sources[index];
        let duration = if source.kind == MediaKind::Video {
            6
        } else {
            150
        };
        gui.insert(
            (source.path.clone(), source.kind),
            source.name.clone(),
            0,
            gui.end(),
            0..duration,
            duration,
        )
        .unwrap();
    }
    store.save_timeline(&entry, &gui).unwrap();
    println!("GUI fixture: {}", entry.path.display());
    for (canvas, output) in [
        ((1920, 1080), (640, 360)),
        ((1080, 1920), (202, 360)),
        ((1080, 1080), (360, 360)),
    ] {
        for (fit, name) in [
            (MediaFit::Fit, "fit"),
            (MediaFit::Fill, "fill"),
            (MediaFit::Original, "original"),
        ] {
            let mut timeline = Timeline {
                width: canvas.0,
                height: canvas.1,
                media_fit: fit,
                ..Default::default()
            };
            for (i, source) in sources.iter().enumerate() {
                timeline
                    .insert(
                        (source.path.clone(), source.kind),
                        source.name.clone(),
                        0,
                        i as i64 * 6,
                        0..6,
                        if source.kind == MediaKind::Video {
                            6
                        } else {
                            150
                        },
                    )
                    .unwrap();
            }
            let settings = ExportSettings {
                width: output.0,
                height: output.1,
                fps: 60,
                bitrate_kbps: 4000,
                container: Container::Mp4,
                encoder: Encoder::Mpeg4,
            };
            let path = root.join(format!("{}x{}-{name}.mp4", output.0, output.1));
            if path.exists() {
                fs::remove_file(&path).unwrap();
            }
            export::export(
                &timeline,
                &sources,
                &settings,
                &path,
                &AtomicBool::new(false),
                |_| {},
            )
            .unwrap();
            let probe = media::command("ffprobe")
                .args(["-v", "error", "-show_streams", "-of", "json"])
                .arg(&path)
                .output()
                .unwrap();
            assert!(probe.status.success());
            let data: serde_json::Value = serde_json::from_slice(&probe.stdout).unwrap();
            let video = data["streams"]
                .as_array()
                .unwrap()
                .iter()
                .find(|s| s["codec_type"] == "video")
                .unwrap();
            assert_eq!(video["width"], output.0);
            assert_eq!(video["height"], output.1);
            assert_eq!(video["r_frame_rate"], "60/1");
            for i in 0..sources.len() {
                let preview = composite::render(&timeline, i as i64 * 6 + 3, &sources).unwrap();
                let decoded = media::frame(&path, i as f64 * 0.2 + 0.1).unwrap();
                if fit == MediaFit::Original {
                    // 极小色块的 4:2:0 边缘会混色；验证非黑覆盖边界，避免把正常色度采样误判为画幅缩放。
                    let coverage = |pixels: &[u8]| {
                        let mut bounds = (640i32, 360i32, -1i32, -1i32);
                        for (index, pixel) in pixels.as_chunks::<4>().0.iter().enumerate() {
                            if pixel[..3].iter().any(|v| *v > 60) {
                                let (x, y) = ((index % 640) as i32, (index / 640) as i32);
                                bounds = (
                                    bounds.0.min(x),
                                    bounds.1.min(y),
                                    bounds.2.max(x),
                                    bounds.3.max(y),
                                );
                            }
                        }
                        bounds
                    };
                    let a = coverage(&preview);
                    let b = coverage(&decoded);
                    assert!(a.2 >= a.0 && b.2 >= b.0, "素材必须可见");
                    for (left, right) in [a.0, a.1, a.2, a.3].into_iter().zip([b.0, b.1, b.2, b.3])
                    {
                        assert!(
                            (left - right).abs() <= 3,
                            "{canvas:?} source={i}: coverage {a:?} != {b:?}"
                        );
                    }
                    continue;
                }
                for (x, y) in [
                    (0, 0),
                    (320, 180),
                    (318, 178),
                    (322, 182),
                    (310, 170),
                    (330, 190),
                    (300, 180),
                    (340, 180),
                ] {
                    let offset = (y * 640 + x) * 4;
                    for c in 0..3 {
                        assert!(
                            (i16::from(preview[offset + c]) - i16::from(decoded[offset + c])).abs()
                                < 36,
                            "{name} {canvas:?} source={i} pixel=({x},{y}) channel={c}: {} != {}",
                            preview[offset + c],
                            decoded[offset + c]
                        );
                    }
                }
            }
            println!(
                "Verified {}: four source sizes, 60fps, preview/export pixels",
                path.display()
            );
        }
    }
}
