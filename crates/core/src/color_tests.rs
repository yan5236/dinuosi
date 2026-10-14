//! 调色像素、边界、选区与持久化回归，断言可见业务结果。
use crate::{
    color::{Control, Grade, Stage},
    color_render::{apply, coverage, from_hsl, to_hsl},
    media::MediaKind,
    timeline::Timeline,
};

/// 验证中性/旁路逐字节一致、灰度、步骤累计、alpha 与透明源不变，曝光实际提亮。
#[test]
fn color_identity_bypass_alpha_and_serial_steps() {
    let bytes = [
        0, 0, 255, 255, 0, 255, 0, 128, 255, 0, 0, 0, 80, 80, 80, 255,
    ];
    let mut grade = Grade::default();
    assert!(matches!(
        apply(&bytes, (4, 1), (4, 1), &grade).unwrap(),
        std::borrow::Cow::Borrowed(_)
    ));
    grade.stages[0].basic[4] = -100.0;
    let gray = apply(&bytes, (4, 1), (4, 1), &grade).unwrap().into_owned();
    assert_eq!(&gray[0..3], &[54, 54, 54]);
    assert_eq!(&gray[4..7], &[182, 182, 182]);
    assert_eq!(&gray[8..12], &bytes[8..12]);
    assert_eq!(gray[7], 128);
    grade.stages.push(Stage::default());
    grade.stages[1].basic[0] = 1.0;
    let raised = apply(&bytes, (4, 1), (4, 1), &grade).unwrap();
    assert!(raised[0] > gray[0]);
    assert_eq!(raised[0], raised[1]);
    grade.stages[1].enabled = false;
    assert_eq!(
        apply(&bytes, (4, 1), (4, 1), &grade).unwrap().as_ref(),
        gray
    );
    grade.enabled = false;
    assert_eq!(
        apply(&bytes, (4, 1), (4, 1), &grade).unwrap().as_ref(),
        bytes
    );
}
/// 检查 HSL 环绕、灰色不染色、色轮分区与独立通道曲线，零点与端点不产生 NaN。
#[test]
fn color_hsl_wheels_and_curves() {
    for rgb in [[1.0, 0.0, 0.0], [0.2, 0.7, 0.4], [0.5; 3]] {
        let back = from_hsl(to_hsl(rgb));
        for i in 0..3 {
            assert!((rgb[i] - back[i]).abs() < 1e-9);
        }
    }
    assert_eq!(from_hsl([360.0, 1.0, 0.5]), [1.0, 0.0, 0.0]);
    let mut g = Grade::default();
    g.stages[0].hsl[0][0] = 100.0;
    let out = apply(
        &[0, 0, 255, 255, 0, 255, 0, 255, 90, 90, 90, 255],
        (3, 1),
        (3, 1),
        &g,
    )
    .unwrap();
    assert!(out[1] > 80);
    assert_eq!(&out[4..8], &[0, 255, 0, 255]);
    assert_eq!(&out[8..12], &[90, 90, 90, 255]);
    let mut g = Grade::default();
    g.stages[0].curves[1] = vec![[0.0, 0.1], [0.5, 0.8], [1.0, 1.0]];
    let out = apply(&[100, 100, 100, 255], (1, 1), (1, 1), &g).unwrap();
    assert!(out[2] > 150);
    assert_eq!(out[0], 100);
    let mut g = Grade::default();
    g.stages[0].wheels[0] = [100.0, 0.0, 0.0];
    let out = apply(&[40, 40, 40, 255, 240, 240, 240, 255], (2, 1), (2, 1), &g).unwrap();
    assert!(out[2] > out[1]);
    assert!(out[2] - 40 > out[6] - 240);
}
/// 蒙版坐标排除补边，移动/旋转/反转/羽化改变覆盖率，色相环绕限定器只选有色红色。
#[test]
fn color_local_masks_qualifier_and_padding() {
    let mut s = Stage::default();
    s.mask.enabled = true;
    s.mask.ellipse = false;
    s.mask.values = [0.5, 0.5, 0.4, 0.4, 0.0, 0.0];
    assert_eq!(coverage(&s, 0.5, 0.5, [1.0, 0.0, 0.0], 1.0), 1.0);
    assert_eq!(coverage(&s, 0.9, 0.5, [1.0, 0.0, 0.0], 1.0), 0.0);
    s.mask.inverted = true;
    assert_eq!(coverage(&s, 0.5, 0.5, [1.0, 0.0, 0.0], 1.0), 0.0);
    s.mask.inverted = false;
    s.mask.values[0] = 0.8;
    assert_eq!(coverage(&s, 0.8, 0.5, [1.0, 0.0, 0.0], 1.0), 1.0);
    s.mask.values = [0.5, 0.5, 0.8, 0.2, 90.0, 0.0];
    assert_eq!(coverage(&s, 0.5, 0.8, [1.0, 0.0, 0.0], 1.0), 1.0);
    s.mask.values = [0.5, 0.5, 0.4, 0.2, 45.0, 0.0];
    assert_eq!(coverage(&s, 0.606066, 0.712132, [1.0, 0.0, 0.0], 2.0), 1.0);
    assert_eq!(coverage(&s, 0.5495, 0.401, [1.0, 0.0, 0.0], 2.0), 0.0);
    s.mask.values = [0.5, 0.5, 0.5, 0.5, 0.0, 0.5];
    let w = coverage(&s, 0.70, 0.5, [1.0, 0.0, 0.0], 1.0);
    assert!(w > 0.0 && w < 1.0);
    s.mask.enabled = false;
    s.qualifier.enabled = true;
    assert_eq!(coverage(&s, 0.5, 0.5, [1.0, 0.0, 0.01], 1.0), 1.0);
    assert_eq!(coverage(&s, 0.5, 0.5, [0.0, 1.0, 0.0], 1.0), 0.0);
    assert_eq!(coverage(&s, 0.5, 0.5, [0.5; 3], 1.0), 0.0);
    let mut g = Grade::default();
    g.stages[0].basic[4] = -100.0;
    g.stages[0].mask.enabled = true;
    g.stages[0].mask.values = [0.5, 0.5, 1.0, 1.0, 0.0, 0.0];
    let data = [0, 0, 0, 0, 0, 0, 255, 255, 0, 0, 0, 0];
    let out = apply(&data, (1, 3), (1, 1), &g).unwrap();
    assert_eq!(&out[4..8], &[54, 54, 54, 255]);
    assert_eq!(&out[..4], &data[..4]);
}
/// 保存重开保留步骤；旧文件缺字段为中性，损坏数字/曲线拒绝，锁定与音频不被粘贴影响。
#[test]
fn color_persistence_legacy_validation_locks_and_split() {
    let mut t = Timeline::default();
    let image = t
        .insert(
            ("image.png".into(), MediaKind::Image),
            "画面".into(),
            0,
            0,
            0..90,
            90,
        )
        .unwrap();
    let audio = t
        .insert(
            ("audio.wav".into(), MediaKind::Audio),
            "声音".into(),
            0,
            0,
            0..90,
            90,
        )
        .unwrap();
    let mut g = Grade::default();
    g.stages[0].set(Control::Basic(0), 1.5).unwrap();
    g.stages.push(Stage::default());
    g.stages[1].curves[0] = vec![[0.0, 0.0], [0.5, 0.8], [1.0, 1.0]];
    let ids = [image.clone(), audio].into();
    t.set_color(&ids, &g).unwrap();
    assert_eq!(
        t.clips
            .iter()
            .find(|c| c.kind == MediaKind::Audio)
            .unwrap()
            .color,
        Grade::default()
    );
    t.clips.iter_mut().find(|c| c.id == image).unwrap().locked = true;
    t.set_color(&ids, &Grade::default()).unwrap();
    assert_eq!(t.clips.iter().find(|c| c.id == image).unwrap().color, g);
    t.clips.iter_mut().find(|c| c.id == image).unwrap().locked = false;
    t.split(&[image.clone()].into(), 45).unwrap();
    assert!(
        t.clips
            .iter()
            .filter(|c| c.kind == MediaKind::Image)
            .all(|c| c.color == g)
    );
    let disk = crate::timeline_file::TimelineV1::from_timeline(&t);
    let mut value = serde_json::to_value(disk).unwrap();
    let restored = serde_json::from_value::<crate::timeline_file::TimelineV1>(value.clone())
        .unwrap()
        .into_timeline()
        .unwrap();
    assert_eq!(restored, t);
    for clip in value["clips"].as_array_mut().unwrap() {
        clip.as_object_mut().unwrap().remove("color");
    }
    let legacy = serde_json::from_value::<crate::timeline_file::TimelineV1>(value)
        .unwrap()
        .into_timeline()
        .unwrap();
    assert!(legacy.clips.iter().all(|c| c.color == Grade::default()));
    let before = t.clone();
    g.stages[0].basic[0] = f64::NAN;
    assert!(t.set_color(&ids, &g).is_err());
    assert_eq!(t, before);
    let mut g = Grade::default();
    g.stages[0].curves[0] = vec![[0.0, 0.0], [0.0, 0.5], [1.0, 1.0]];
    assert!(g.validate().is_err());
    assert!(apply(&[0; 4], (2, 2), (2, 2), &Grade::default()).is_err());
}
/// 示波器使用真实像素统计，透明/补边排除，红绿信号落在各自亮度高度和横向位置。
#[test]
fn color_scopes_measure_pixels() {
    let s =
        crate::color_scopes::Scopes::from_bgra(&[0, 0, 255, 255, 0, 255, 0, 255], (2, 1), (2, 1))
            .unwrap();
    assert_eq!(s.histogram[0][255], 1);
    assert_eq!(s.histogram[1][255], 1);
    assert_eq!(s.histogram[2][0], 2);
    assert_eq!(s.waveform[(127 - 27) * 256], 1);
    assert_eq!(s.waveform[(127 - 91) * 256 + 128], 1);
}

/// 通过真实媒体动态库验证图片/视频预览与原尺寸渲染，并编码/重读成片检查去色、蒙版和步骤效果。
/// 夹具与原始像素证据写入项目内忽略目录；需要现有配套运行库及仅供夹具生成的 FFmpeg CLI。
#[test]
#[ignore = "需要准备应用配套媒体运行时"]
fn real_color_preview_export_and_encoded_output() {
    use crate::{
        composite::FrameCache,
        export::{Encoder, ExportSettings},
        export_render::Renderer,
        media,
    };
    use std::{io::Write, process::Stdio, sync::atomic::AtomicBool};
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../.local/color-qa")
        .join(format!("core-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&root).unwrap();
    let root = root.canonicalize().unwrap();
    let colors = [
        [20, 30, 220, 255],
        [30, 190, 30, 255],
        [210, 40, 30, 255],
        [40, 190, 210, 255],
        [185, 40, 190, 255],
        [130, 130, 130, 255],
    ];
    let mut raw = Vec::with_capacity(640 * 360 * 4);
    for y in 0..360 {
        for x in 0..640 {
            raw.extend_from_slice(&if y > 270 {
                let v = (x * 255 / 639) as u8;
                [v, v, v, 255]
            } else {
                colors[(x * 6 / 640).min(5)]
            });
        }
    }
    for video in [false, true] {
        let path = root.join(if video { "source.mp4" } else { "source.bmp" });
        let mut cmd = media::command("ffmpeg");
        cmd.args([
            "-v",
            "error",
            "-threads",
            "1",
            "-filter_threads",
            "1",
            "-f",
            "rawvideo",
            "-pixel_format",
            "bgra",
            "-video_size",
            "640x360",
            "-framerate",
            "30",
            "-i",
            "pipe:0",
        ]);
        if video {
            cmd.args(["-c:v", "mpeg4", "-q:v", "1", "-threads", "1"]);
        } else {
            cmd.args(["-frames:v", "1"]);
        }
        let mut child = cmd.arg(&path).stdin(Stdio::piped()).spawn().unwrap();
        let mut stdin = child.stdin.take().unwrap();
        for _ in 0..if video { 15 } else { 1 } {
            stdin.write_all(&raw).unwrap();
        }
        drop(stdin);
        assert!(child.wait().unwrap().success());
        let m = media::probe(&path).unwrap();
        let mut t = Timeline {
            width: 640,
            height: 360,
            ..Default::default()
        };
        t.insert((path, m.kind), "调色实测".into(), 0, 0, 0..15, 15)
            .unwrap();
        let mut grade = Grade::default();
        grade.stages[0].basic[4] = -100.0;
        grade.stages[0].mask.enabled = true;
        grade.stages[0].mask.ellipse = false;
        grade.stages[0].mask.values = [0.5, 0.5, 0.5, 1.0, 0.0, 0.0];
        let mut lift = Stage::default();
        lift.curves[0] = vec![[0.0, 0.0], [0.5, 0.65], [1.0, 1.0]];
        grade.stages.push(lift);
        t.set_color(&[t.clips[0].id.clone()].into(), &grade)
            .unwrap();
        let sources = [m];
        let settings = ExportSettings::for_timeline(&t, Encoder::Mpeg4);
        let clock = std::time::Instant::now();
        let preview = FrameCache::default().render(&t, 0, &sources).unwrap();
        let elapsed = clock.elapsed().as_millis();
        let native = Renderer::new()
            .render(&t, 0.0, &sources, &settings, &Default::default())
            .unwrap()
            .to_vec();
        assert_eq!(preview, native);
        let output = root.join(if video {
            "video-graded.mp4"
        } else {
            "image-graded.mp4"
        });
        crate::export::export(
            &t,
            &sources,
            &settings,
            &output,
            &AtomicBool::new(false),
            |_| {},
        )
        .unwrap();
        let encoded = media::frame(&output, 0.1).unwrap();
        for (x, y) in [(60, 120), (220, 120), (360, 120), (550, 120), (320, 310)] {
            let i = (y * 640 + x) * 4;
            for c in 0..3 {
                assert!(
                    (encoded[i + c] as i16 - preview[i + c] as i16).abs() <= 10,
                    "encoded color differs at {x},{y},{c}"
                );
            }
        }
        let center = (120 * 640 + 320) * 4;
        assert_eq!(preview[center], preview[center + 1]);
        assert_eq!(preview[center + 1], preview[center + 2]);
        let outside = (120 * 640 + 60) * 4;
        assert!(preview[outside + 2] > preview[outside + 1] + 80);
        let large = ExportSettings {
            width: 1280,
            height: 720,
            ..settings
        };
        let doubled = Renderer::new()
            .render(&t, 0.0, &sources, &large, &Default::default())
            .unwrap()
            .to_vec();
        for (x, y) in [(60, 120), (220, 120), (360, 120), (550, 120)] {
            let a = (y * 640 + x) * 4;
            let b = (y * 2 * 1280 + x * 2) * 4;
            assert_eq!(&preview[a..a + 4], &doubled[b..b + 4]);
        }
        let ids = [t.clips[0].id.clone()].into();
        let kind = crate::keyframes::KeyframeKind::Color(0, Control::Basic(4));
        t.toggle_keyframe(&ids, kind, 0);
        let mut animated = t.clips[0].color_at(14.0).into_owned();
        animated.stages[0].basic[4] = 0.0;
        t.set_color_at(&ids, &animated, 14, true).unwrap();
        let mut cache = FrameCache::default();
        let mut renderer = Renderer::new();
        let mut frames = Vec::new();
        for frame in [0, 7, 14] {
            let image = cache.render(&t, frame, &sources).unwrap();
            assert_eq!(
                image,
                renderer
                    .render(&t, frame as f64, &sources, &settings, &Default::default())
                    .unwrap()
            );
            frames.push(image);
        }
        assert_eq!(frames[0], preview);
        assert_ne!(frames[0], frames[1]);
        assert_ne!(frames[1], frames[2]);
        let output_animated = root.join(if video {
            "video-animated.mp4"
        } else {
            "image-animated.mp4"
        });
        crate::export::export(
            &t,
            &sources,
            &settings,
            &output_animated,
            &AtomicBool::new(false),
            |_| {},
        )
        .unwrap();
        for (index, frame) in [0, 7, 14].into_iter().enumerate() {
            let encoded = media::frame(&output_animated, frame as f64 / 30.0).unwrap();
            for (x, y) in [(220, 120), (360, 120)] {
                let i = (y * 640 + x) * 4;
                for c in 0..3 {
                    assert!(
                        (encoded[i + c] as i16 - frames[index][i + c] as i16).abs() <= 10,
                        "animated export mismatch at frame {frame}"
                    );
                }
            }
        }
        let mut sampled = t.clips[0].color_at(14.0).into_owned();
        sampled.stages[1].picked = Some(crate::color::PickedHsl {
            hue: to_hsl([220.0, 30.0, 20.0].map(|v| v / 255.0))[0],
            values: [0.0, -100.0, 0.0],
        });
        t.set_color_at(&ids, &sampled, 14, true).unwrap();
        let picked_preview = cache.render(&t, 14, &sources).unwrap();
        assert_eq!(
            picked_preview,
            renderer
                .render(&t, 14.0, &sources, &settings, &Default::default())
                .unwrap()
        );
        let red = (120 * 640 + 60) * 4;
        assert!(
            picked_preview[red..red + 3].iter().max().unwrap()
                - picked_preview[red..red + 3].iter().min().unwrap()
                <= 1
        );
        let output_picked = root.join(if video {
            "video-picked.mp4"
        } else {
            "image-picked.mp4"
        });
        crate::export::export(
            &t,
            &sources,
            &settings,
            &output_picked,
            &AtomicBool::new(false),
            |_| {},
        )
        .unwrap();
        let encoded = media::frame(&output_picked, 14.0 / 30.0).unwrap();
        for (x, y) in [(60, 120), (220, 120), (360, 120)] {
            let i = (y * 640 + x) * 4;
            for c in 0..3 {
                assert!(
                    (encoded[i + c] as i16 - picked_preview[i + c] as i16).abs() <= 10,
                    "sampled HSL export mismatch"
                );
            }
        }
        let name = if video { "video" } else { "image" };
        std::fs::write(root.join(format!("{name}-preview.bgra")), preview).unwrap();
        std::fs::write(root.join(format!("{name}-result.json")),serde_json::to_vec_pretty(&serde_json::json!({"preview_native":"pixel-identical","encoded_checks":15,"large_output_checks":4,"preview_ms":elapsed,"output":output})).unwrap()).unwrap();
    }
    println!("真实调色证据：{}", root.display());
}
