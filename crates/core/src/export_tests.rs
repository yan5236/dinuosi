//! 导出边界与真实受控 FFmpeg 回归；集成测试需显式准备应用相邻运行库。
use super::*;
use crate::{
    keyframes::{Keyframe, KeyframeKind},
    media::MediaKind,
    timeline::{TrackKind, Transform},
};
use std::sync::Mutex;

/// 验证非法参数、已有输出及取消不产生文件，也不改变原文件内容。
#[test]
fn parameters_and_publication_preserve_existing_files() {
    let timeline = Timeline::default();
    let mut settings = ExportSettings::for_timeline(&timeline, Encoder::Mpeg4);
    assert!(settings.validate().is_ok());
    settings.width = 1919;
    assert!(settings.validate().is_err());
    settings.width = 1920;
    settings.fps = 0;
    assert!(settings.validate().is_err());
    let root = std::env::temp_dir().join(format!("dinuosi-export-unit-{}", uuid::Uuid::new_v4()));
    fs::create_dir(&root).unwrap();
    let source = root.join("source");
    let target = root.join("target");
    fs::write(&source, b"new").unwrap();
    fs::write(&target, b"original").unwrap();
    assert!(publish(&source, &target, &AtomicBool::new(false)).is_err());
    assert_eq!(fs::read(&target).unwrap(), b"original");
    assert_eq!(fs::read(&source).unwrap(), b"new");
    assert!(check_cancel(&AtomicBool::new(true)).is_err());
    fs::remove_dir_all(root).unwrap();
}

/// 证明新原尺寸合成入口与已有固定预览逐像素一致，覆盖缩放、透明度、位移与旋转。
#[test]
fn native_blend_matches_preview_semantics() {
    let source = [30, 90, 210, 128].repeat((media::FRAME_WIDTH * media::FRAME_HEIGHT) as usize);
    let transform = Transform {
        x: 40.0,
        y: -60.0,
        scale: 73.0,
        rotation: 23.0,
        opacity: 61.0,
    };
    let mut preview = [0, 0, 0, 255].repeat((media::FRAME_WIDTH * media::FRAME_HEIGHT) as usize);
    let mut exported = preview.clone();
    crate::composite::blend(&mut preview, &source, &transform, (1920, 1080)).unwrap();
    crate::composite::blend_pixels(
        &mut exported,
        (640, 360),
        &source,
        (640, 360),
        &transform,
        (1920, 1080),
        (1920.0, 1080.0),
    )
    .unwrap();
    assert_eq!(preview, exported);
    assert!(
        crate::composite::blend_pixels(
            &mut [0; 4],
            (2, 2),
            &source,
            (640, 360),
            &transform,
            (1920, 1080),
            (1920.0, 1080.0)
        )
        .is_err()
    );
    let mut wide = [0, 0, 0, 255].repeat(8);
    crate::composite::blend_pixels(
        &mut wide,
        (4, 2),
        &[255, 0, 0, 255].repeat(4),
        (2, 2),
        &Transform::default(),
        (2, 2),
        (2.0, 2.0),
    )
    .unwrap();
    assert_eq!(
        &wide[..16],
        &[0, 0, 0, 255, 255, 0, 0, 255, 255, 0, 0, 255, 0, 0, 0, 255]
    );
}

/// 验证长 MIDI 入点预取消在读取文件或初始化音源之前返回，避免取消后仍执行昂贵寻址。
#[test]
fn cancelled_midi_seek_stops_before_initialization() {
    let cancel = AtomicBool::new(true);
    let result = crate::midi::MidiPcm::start(
        Path::new("nonexistent-export-cancel.mid"),
        3600.0,
        3601.0,
        Some(&cancel),
    );
    assert!(matches!(result, Err(error) if error == "导出已取消"));
}

#[path = "export_test_support.rs"]
pub(super) mod support;
use support::{fixture, rms, save_gui_fixture, timeline, verify_video_audio};

/// 真实输出并通过 ffprobe、像素与 PCM 断言多轨/裁剪/动画，验证取消、缺源及已有文件保护。
#[test]
#[ignore = "需要准备应用配套 FFmpeg 和 ffprobe"]
fn real_timeline_export_and_failure_boundaries() {
    let root = std::env::temp_dir().join(format!("dinuosi-export-real-{}", uuid::Uuid::new_v4()));
    fs::create_dir(&root).unwrap();
    let (video, audio, image) = fixture(&root);
    let timeline = timeline(&video, &audio, &image);
    let sources = [video, audio, image];
    let settings = ExportSettings {
        width: 128,
        height: 128,
        fps: 24,
        bitrate_kbps: 3000,
        container: Container::Mp4,
        encoder: Encoder::Mpeg4,
    };
    let output = root.join("export.mp4");
    let cancel = AtomicBool::new(false);
    let progress = Mutex::new(Vec::new());
    export(&timeline, &sources, &settings, &output, &cancel, |value| {
        progress.lock().unwrap().push(value)
    })
    .unwrap();
    let progress = progress.into_inner().unwrap();
    assert_eq!(progress.last(), Some(&1.0));
    assert!(progress.windows(2).all(|pair| pair[0] <= pair[1]));
    let probe = media::command("ffprobe")
        .args([
            "-v",
            "error",
            "-show_streams",
            "-show_format",
            "-of",
            "json",
        ])
        .arg(&output)
        .output()
        .unwrap();
    let data: serde_json::Value = serde_json::from_slice(&probe.stdout).unwrap();
    let video = data["streams"]
        .as_array()
        .unwrap()
        .iter()
        .find(|stream| stream["codec_type"] == "video")
        .unwrap();
    assert_eq!(video["width"], 128);
    assert_eq!(video["height"], 128);
    assert_eq!(video["r_frame_rate"], "24/1");
    assert_eq!(video["codec_name"], "mpeg4");
    assert!(
        (data["format"]["duration"]
            .as_str()
            .unwrap()
            .parse::<f64>()
            .unwrap()
            - 1.0)
            .abs()
            < 0.05
    );
    assert!(
        data["streams"]
            .as_array()
            .unwrap()
            .iter()
            .any(|stream| stream["codec_name"] == "aac")
    );
    let frame = media::command("ffmpeg")
        .args(["-v", "error", "-ss", "0.5", "-i"])
        .arg(&output)
        .args([
            "-frames:v",
            "1",
            "-pix_fmt",
            "bgra",
            "-f",
            "rawvideo",
            "pipe:1",
        ])
        .output()
        .unwrap();
    assert_eq!(frame.stdout.len(), 128 * 128 * 4);
    let background = &frame.stdout[(96 * 128 + 96) * 4..][..4];
    assert!(
        background[0] > 230 && background[2] < 20,
        "裁剪后的背景必须为蓝色：{background:?}"
    );
    let overlay = &frame.stdout[(32 * 128 + 32) * 4..][..4];
    assert!(
        overlay[0] > 160 && overlay[2] > 40 && overlay[2] < 100,
        "半透明动画红图必须混合蓝背景：{overlay:?}"
    );
    assert!(rms(&output, 0.02, 0.15) < 0.001);
    assert!(rms(&output, 0.65, 0.75) > rms(&output, 0.25, 0.35) * 1.8);
    assert!(rms(&output, 0.9, 0.98) < 0.001);
    verify_video_audio(
        &sources[0],
        &sources[1],
        &timeline,
        &settings,
        &root,
        &cancel,
    );
    let audio_only = root.join("audio-only.mp4");
    let mut only_audio = timeline.clone();
    only_audio
        .clips
        .retain(|clip| clip.kind == MediaKind::Audio);
    export(
        &only_audio,
        &sources,
        &settings,
        &audio_only,
        &cancel,
        |_| {},
    )
    .unwrap();
    assert!(rms(&audio_only, 0.65, 0.75) > 0.15);
    let midi_path = root.join("source.mid");
    fs::write(
        &midi_path,
        [
            b"MThd".as_slice(),
            &[0, 0, 0, 6, 0, 0, 0, 1, 1, 224],
            b"MTrk",
            &[
                0, 0, 0, 16, 0, 192, 0, 0, 144, 60, 100, 135, 64, 128, 60, 0, 0, 255, 47, 0,
            ],
        ]
        .concat(),
    )
    .unwrap();
    let midi = media::probe(&midi_path).unwrap();
    let mut midi_timeline = only_audio.clone();
    midi_timeline.clips[0].source = midi.path.clone();
    midi_timeline.clips[0].source_frames = 30;
    let midi_output = root.join("midi.mp4");
    export(
        &midi_timeline,
        &[midi],
        &settings,
        &midi_output,
        &cancel,
        |_| {},
    )
    .unwrap();
    assert!(
        rms(&midi_output, 0.3, 0.5) > 0.0001,
        "MIDI 导出必须包含真实合成的音频"
    );
    let mut hidden = timeline.clone();
    hidden.tracks[1].visible = false;
    hidden.tracks[2].visible = false;
    let hidden_output = root.join("hidden.mp4");
    export(
        &hidden,
        &sources,
        &settings,
        &hidden_output,
        &cancel,
        |_| {},
    )
    .unwrap();
    let hidden_frame = media::frame(&hidden_output, 0.5).unwrap();
    assert!(
        hidden_frame
            .as_chunks::<4>()
            .0
            .iter()
            .all(|pixel| pixel[..3].iter().all(|value| *value < 5))
    );
    assert!(
        rms(&hidden_output, 0.65, 0.75) > 0.15,
        "隐藏画面不应静音独立音轨"
    );
    hidden.tracks[0].muted = true;
    let muted_output = root.join("muted.mp4");
    export(&hidden, &sources, &settings, &muted_output, &cancel, |_| {}).unwrap();
    assert!(rms(&muted_output, 0.65, 0.75) < 0.001);
    let previous = fs::read(&output).unwrap();
    assert!(export(&timeline, &sources, &settings, &output, &cancel, |_| {}).is_err());
    assert_eq!(fs::read(&output).unwrap(), previous);
    let cancelled = root.join("cancelled.mp4");
    assert!(
        export(
            &timeline,
            &sources,
            &settings,
            &cancelled,
            &cancel,
            |progress| {
                if progress > 0.3 {
                    cancel.store(true, Ordering::Relaxed);
                }
            }
        )
        .unwrap_err()
        .contains("取消")
    );
    assert!(!cancelled.exists());
    assert!(!fs::read_dir(&root).unwrap().any(|entry| {
        entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with(".dinuosi-export-")
    }));
    cancel.store(false, Ordering::Relaxed);
    let mut missing = timeline.clone();
    missing.clips[0].source = root.join("missing.mp4");
    assert!(
        export(
            &missing,
            &sources,
            &settings,
            &root.join("missing-output.mp4"),
            &cancel,
            |_| {}
        )
        .unwrap_err()
        .contains("不存在")
    );
    let mut matroska = settings.clone();
    matroska.container = Container::Matroska;
    export(
        &timeline,
        &sources,
        &matroska,
        &root.join("export.mkv"),
        &cancel,
        |_| {},
    )
    .unwrap();
    let broken_path = root.join("broken.mp4");
    fs::write(&broken_path, b"not a video").unwrap();
    let mut broken = timeline.clone();
    broken.clips[0].source = broken_path.clone();
    let mut broken_sources = sources.to_vec();
    broken_sources[0].path = broken_path;
    let broken_output = root.join("broken-output.mp4");
    assert!(
        export(
            &broken,
            &broken_sources,
            &settings,
            &broken_output,
            &cancel,
            |_| {}
        )
        .is_err()
    );
    assert!(!broken_output.exists());
    let available = capabilities().unwrap();
    assert!(available.contains(&Encoder::Mpeg4));
    for encoder in [
        Encoder::H264MediaFoundationHardware,
        Encoder::H264MediaFoundation,
    ] {
        let suffix = if encoder == Encoder::H264MediaFoundationHardware {
            "-hardware"
        } else {
            ""
        };
        if available.contains(&encoder) {
            let mut h264 = settings.clone();
            h264.encoder = encoder;
            h264.width = 320;
            h264.height = 240;
            let h264_output = root.join(format!("h264{suffix}.mp4"));
            export(&timeline, &sources, &h264, &h264_output, &cancel, |_| {}).unwrap();
            let probe = media::command("ffprobe")
                .args([
                    "-v",
                    "error",
                    "-show_streams",
                    "-show_format",
                    "-of",
                    "json",
                ])
                .arg(&h264_output)
                .output()
                .unwrap();
            let probe_data: serde_json::Value = serde_json::from_slice(&probe.stdout).unwrap();
            let stream = probe_data["streams"]
                .as_array()
                .unwrap()
                .iter()
                .find(|stream| stream["codec_type"] == "video")
                .unwrap();
            assert_eq!(stream["codec_name"], "h264");
            assert_eq!(stream["width"], 320);
            assert_eq!(stream["height"], 240);
            assert_eq!(stream["r_frame_rate"], "24/1");
            assert!(
                probe_data["streams"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|stream| stream["codec_name"] == "aac")
            );
            assert!(
                (probe_data["format"]["duration"]
                    .as_str()
                    .unwrap()
                    .parse::<f64>()
                    .unwrap()
                    - 1.0)
                    .abs()
                    < 0.05
            );
            assert!(rms(&h264_output, 0.65, 0.75) > rms(&h264_output, 0.25, 0.35) * 1.8);
            let frame = media::frame(&h264_output, 0.5).unwrap();
            let blue = &frame[(280 * 640 + 440) * 4..][..4];
            let red_overlay = &frame[(90 * 640 + 240) * 4..][..4];
            let letterbox = &frame[(180 * 640 + 100) * 4..][..4];
            assert!(blue[0] > 230 && blue[2] < 20);
            assert!(red_overlay[0] > 160 && red_overlay[2] > 40 && red_overlay[2] < 100);
            assert!(letterbox[..3].iter().all(|value| *value < 5));
            let artifacts = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.local/export-qa");
            fs::create_dir_all(&artifacts).unwrap();
            fs::copy(
                &h264_output,
                artifacts.join(format!("timeline-h264{suffix}.mp4")),
            )
            .unwrap();
            fs::write(
                artifacts.join(format!("ffprobe-h264{suffix}.json")),
                serde_json::to_vec_pretty(&probe_data).unwrap(),
            )
            .unwrap();
            println!(
                "{} 实际时间线导出通过：320x240 / 24fps / 1s / H.264 + AAC，裁剪/变换/音量动画与宽高比黑边通过",
                encoder.label()
            );
        } else {
            let mut unavailable = settings.clone();
            unavailable.encoder = encoder;
            let unavailable_output = root.join(format!("unavailable{suffix}.mp4"));
            assert!(
                export(
                    &timeline,
                    &sources,
                    &unavailable,
                    &unavailable_output,
                    &cancel,
                    |_| {}
                )
                .is_err()
            );
            assert!(!unavailable_output.exists());
        }
    }
    println!(
        "真实导出证据：{}，128x128 / 24fps / 1s / MPEG-4 + AAC；多轨裁剪/动画/音频/取消/缺源/无覆盖通过",
        output.display()
    );
    let artifacts = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.local/export-qa");
    fs::create_dir_all(&artifacts).unwrap();
    fs::copy(&output, artifacts.join("timeline-mpeg4.mp4")).unwrap();
    fs::write(
        artifacts.join("ffprobe-mpeg4.json"),
        serde_json::to_vec_pretty(&data).unwrap(),
    )
    .unwrap();
    save_gui_fixture(&artifacts, &timeline, &sources);
    fs::remove_dir_all(root).unwrap();
}
