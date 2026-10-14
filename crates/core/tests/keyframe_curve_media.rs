//! 真实媒体验收：曲线从 Core 预览、连续 PCM 到编码文件，使用独立像素/采样对照。
use dinuosi_core::{
    ProjectStore,
    export::{self, Container, Encoder, ExportSettings},
    keyframe_curve::Easing,
    keyframes::KeyframeKind as K,
    media::{self, MediaKind},
    timeline::{Property, Timeline},
    timeline_audio::TimelineAudio,
};
use std::{fs, sync::atomic::AtomicBool};

/// 生成白图与恒定 PCM，导出非线性透明度/音量并重新解码；容差仅用于有损 MPEG-4/AAC。
/// 可选 DINUOSI_CURVE_QA_ROOT 保留本次隔离项目用于原生界面验收；默认创建并清理独占临时目录。
#[test]
#[ignore = "需要准备应用配套 FFmpeg 和 ffprobe"]
fn real_curve_preview_pcm_and_export() {
    let retained = std::env::var_os("DINUOSI_CURVE_QA_ROOT");
    let root = retained
        .as_ref()
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| {
            std::env::temp_dir().join(format!("dinuosi-curves-media-{}", uuid::Uuid::new_v4()))
        });
    fs::create_dir_all(&root).unwrap();
    let image_path = root.join("white.png");
    let ppm_path = root.join("white.ppm");
    let mut ppm = b"P6\n64 64\n255\n".to_vec();
    ppm.extend([255; 64 * 64 * 3]);
    fs::write(&ppm_path, ppm).unwrap();
    assert!(
        media::command("ffmpeg")
            .args(["-v", "error", "-i"])
            .arg(&ppm_path)
            .args(["-frames:v", "1", "-threads", "1"])
            .arg(&image_path)
            .status()
            .unwrap()
            .success()
    );
    let audio_path = root.join("constant.wav");
    assert!(
        media::command("ffmpeg")
            .args([
                "-v",
                "error",
                "-threads",
                "1",
                "-filter_threads",
                "1",
                "-f",
                "lavfi",
                "-i",
                "aevalsrc=0.125|0.125:s=48000:d=1",
                "-ac",
                "2",
                "-c:a",
                "pcm_f32le"
            ])
            .arg(&audio_path)
            .status()
            .unwrap()
            .success()
    );
    let image = media::probe(&image_path).unwrap();
    let audio = media::probe(&audio_path).unwrap();
    let mut t = Timeline {
        width: 64,
        height: 64,
        ..Default::default()
    };
    let video_id = t
        .insert(
            (image.path.clone(), MediaKind::Image),
            "白图".into(),
            0,
            0,
            0..30,
            30,
        )
        .unwrap();
    let ids = [video_id.clone()].into();
    t.set_property(&ids, Property::Opacity, 0.0).unwrap();
    t.toggle_keyframe(&ids, K::Opacity, 0);
    t.set_property_at(&ids, Property::Opacity, 100.0, 24, true)
        .unwrap();
    t.ease_curve(&video_id, K::Opacity, 0, 0, Easing::EaseInOut)
        .unwrap();
    let audio_id = t
        .insert(
            (audio.path.clone(), MediaKind::Audio),
            "恒定音频".into(),
            0,
            0,
            0..30,
            30,
        )
        .unwrap();
    let ids = [audio_id.clone()].into();
    t.set_volume(&ids, 0.0).unwrap();
    t.toggle_keyframe(&ids, K::Volume, 0);
    t.set_volume_at(&ids, 1.0, 24, true).unwrap();
    t.ease_curve(&audio_id, K::Volume, 0, 0, Easing::EaseInOut)
        .unwrap();
    let sources = [image, audio];
    let expected_gain = 0.12916193;
    let preview = dinuosi_core::composite::render(&t, 6, &sources).unwrap();
    assert!((preview[(180 * 640 + 320) * 4] as f64 - 255.0 * expected_gain).abs() < 1.0);
    for offset in [0.0, 0.1, 0.2] {
        let pcm: Vec<_> = TimelineAudio::start(&t, offset, &sources)
            .unwrap()
            .take(24002)
            .collect();
        for pair in [0, 4800, 12000] {
            let frame = (offset + pair as f64 / 48000.0) * 30.0;
            let expected = 0.125
                * t.clips
                    .iter()
                    .find(|c| c.id == audio_id)
                    .unwrap()
                    .gain_at(frame) as f32;
            assert!((pcm[pair * 2] - expected).abs() < 1e-6);
            assert_eq!(pcm[pair * 2], pcm[pair * 2 + 1]);
        }
    }
    let settings = ExportSettings {
        width: 64,
        height: 64,
        fps: 30,
        bitrate_kbps: 1000,
        container: Container::Mp4,
        encoder: Encoder::Mpeg4,
    };
    let output = root.join("curve-export.mp4");
    export::export(
        &t,
        &sources,
        &settings,
        &output,
        &AtomicBool::new(false),
        |_| {},
    )
    .unwrap();
    let pixels = media::command("ffmpeg")
        .args(["-v", "error", "-ss", "0.2", "-i"])
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
    assert!(pixels.status.success());
    assert_eq!(pixels.stdout.len(), 64 * 64 * 4);
    assert!((pixels.stdout[(32 * 64 + 32) * 4] as f64 - 255.0 * expected_gain).abs() < 4.0);
    let pcm = media::command("ffmpeg")
        .args(["-v", "error", "-i"])
        .arg(&output)
        .args(["-vn", "-ac", "2", "-ar", "48000", "-f", "f32le", "pipe:1"])
        .output()
        .unwrap();
    assert!(pcm.status.success());
    let at = 9600 * 2 * 4;
    let decoded = f32::from_le_bytes(pcm.stdout[at..at + 4].try_into().unwrap());
    assert!(
        (decoded as f64 - 0.125 * expected_gain).abs() < 0.003,
        "AAC={decoded}"
    );
    // 零增益节点也能通过切线形成音量隆起，不能被混音器的全静音优化跳过。
    t.set_volume_at(&ids, 0.0, 24, true).unwrap();
    t.edit_curve_handle(&audio_id, K::Volume, 0, 0, false, [1.0 / 3.0, 1.0])
        .unwrap();
    t.edit_curve_handle(&audio_id, K::Volume, 0, 24, true, [1.0 / 3.0, 1.0])
        .unwrap();
    let bump = TimelineAudio::start(&t, 0.4, &sources)
        .unwrap()
        .next()
        .unwrap();
    assert!((bump - 0.09375).abs() < 1e-6);
    if retained.is_some() {
        let store = ProjectStore::new(root.join("recent-projects.json"));
        let entry = store.create("关键帧曲线验收", &root).unwrap();
        let imported = store
            .import_media(&entry, &[image_path, audio_path])
            .unwrap();
        assert!(imported.errors.is_empty());
        store.save_timeline(&entry, &t).unwrap();
        println!("曲线验收项目：{}", entry.path.display());
    } else {
        fs::remove_dir_all(root).unwrap();
    }
}
