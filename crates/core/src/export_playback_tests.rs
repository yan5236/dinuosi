//! 真实动态库回归：倒放/定格画面、立体声逆序、播放跳转和成片导出。
use super::*;
use crate::{composite, media_stream::Decoder, timeline_audio::TimelineAudio};

/// 写入两路不同斜坡的浮点 WAV，逐样本断言倒放和声道顺序，避免正弦或相同声道掩盖错误。
fn stereo_fixture(root: &Path) -> Media {
    let path = root.join("stereo.wav");
    let frames = 120_000_u32;
    let bytes = frames * 8;
    let mut file = BufWriter::new(File::create(&path).unwrap());
    file.write_all(b"RIFF").unwrap();
    file.write_all(&(36 + bytes).to_le_bytes()).unwrap();
    file.write_all(b"WAVEfmt ").unwrap();
    file.write_all(&16_u32.to_le_bytes()).unwrap();
    for value in [3_u16, 2] {
        file.write_all(&value.to_le_bytes()).unwrap();
    }
    for value in [48000_u32, 384000] {
        file.write_all(&value.to_le_bytes()).unwrap();
    }
    for value in [8_u16, 32] {
        file.write_all(&value.to_le_bytes()).unwrap();
    }
    file.write_all(b"data").unwrap();
    file.write_all(&bytes.to_le_bytes()).unwrap();
    for index in 0..frames {
        let left = index as f32 / frames as f32 * 0.5;
        for sample in [left, -left - 0.1] {
            file.write_all(&sample.to_le_bytes()).unwrap();
        }
    }
    file.flush().unwrap();
    media::probe(&path).unwrap()
}

/// 真实解码比较三块跨界的采样对逆序、非零入点及播放跳转；定格视频不再继续原声。
#[test]
#[ignore = "需要准备应用配套动态媒体运行时"]
fn real_reverse_pcm_preserves_channels_seek_and_cancellation() {
    let root = std::env::temp_dir().join(format!("dinuosi-reverse-pcm-{}", uuid::Uuid::new_v4()));
    fs::create_dir_all(&root).unwrap();
    let audio = stereo_fixture(&root);
    let mut timeline = Timeline::default();
    let track = timeline
        .create_track(0, crate::timeline::TrackKind::Audio)
        .unwrap();
    let id = timeline
        .insert(
            (audio.path.clone(), media::MediaKind::Audio),
            "立体声".into(),
            track,
            0,
            3..75,
            75,
        )
        .unwrap();
    let sources = [audio];
    let forward: Vec<_> = TimelineAudio::start(&timeline, 0.0, &sources)
        .unwrap()
        .collect();
    timeline.reverse_clips(&[id].into());
    let reverse: Vec<_> = TimelineAudio::start(&timeline, 0.0, &sources)
        .unwrap()
        .collect();
    let expected: Vec<f32> = forward
        .as_chunks::<2>()
        .0
        .iter()
        .rev()
        .flatten()
        .copied()
        .collect();
    assert_eq!(reverse.len(), 230400);
    assert_eq!(reverse, expected);
    let seek: Vec<_> = TimelineAudio::start(&timeline, 0.35, &sources)
        .unwrap()
        .collect();
    assert_eq!(seek, reverse[33600..]);
    let settings = ExportSettings {
        width: 64,
        height: 64,
        fps: 24,
        bitrate_kbps: 3000,
        container: Container::Mp4,
        encoder: Encoder::Mpeg4,
    };
    let output = root.join("reverse-audio.mp4");
    export(
        &timeline,
        &sources,
        &settings,
        &output,
        &AtomicBool::new(false),
        |_| {},
    )
    .unwrap();
    let mut encoded = media::probe(&output).unwrap();
    assert!(encoded.has_audio);
    encoded.kind = media::MediaKind::Audio;
    let mut means = Vec::new();
    for offset in [0.2, 2.0] {
        let mut decoder = Decoder::start(&encoded, offset, offset + 0.2).unwrap();
        let pcm: Vec<_> = decoder.take_audio().unwrap().collect();
        decoder.finish().unwrap();
        let pairs = pcm.as_chunks::<2>().0;
        means.push(
            [0, 1]
                .map(|channel| pairs.iter().map(|p| p[channel]).sum::<f32>() / pairs.len() as f32),
        );
    }
    assert!(means[0][0] > means[1][0] + 0.25);
    assert!(means[0][1] < means[1][1] - 0.25);
    let mut decoder = Decoder::start_reverse(&sources[0], 0.1, 2.5).unwrap();
    let mut pcm = decoder.take_audio().unwrap();
    assert_eq!(pcm.next(), Some(expected[0]));
    decoder.cancellation().store(true, Ordering::Relaxed);
    assert_eq!(pcm.next(), None);
    let mut frozen = timeline.clone();
    frozen.clips[0].kind = media::MediaKind::Video;
    frozen.clips[0].freeze_frame = Some(20);
    assert!(
        TimelineAudio::start(&frozen, 0.0, &sources)
            .unwrap()
            .all(|v| v == 0.0)
    );
    drop(pcm);
    drop(decoder);
    let mut decoder = Decoder::start_reverse(&sources[0], 0.1, 2.5).unwrap();
    let mut pcm = decoder.take_audio().unwrap();
    fs::rename(&sources[0].path, root.join("removed.wav")).unwrap();
    assert_eq!(pcm.next(), None);
    assert_eq!(pcm.next(), None);
    assert!(decoder.finish().is_err());
    fs::remove_dir_all(root).unwrap();
}

/// 用裁剪后跨红蓝边界的视频检查预览、原尺寸渲染、24fps 成片倒序及固定画面一致。
#[test]
#[ignore = "需要准备应用配套 FFmpeg 和 ffprobe"]
fn real_reverse_and_freeze_preview_and_export() {
    let root =
        std::env::temp_dir().join(format!("dinuosi-playback-export-{}", uuid::Uuid::new_v4()));
    fs::create_dir_all(&root).unwrap();
    let (video, _, _) = super::tests::support::fixture(&root);
    let sources = [video];
    let mut timeline = Timeline {
        width: 64,
        height: 64,
        ..Default::default()
    };
    let id = timeline
        .insert(
            (sources[0].path.clone(), sources[0].kind),
            "颜色".into(),
            0,
            0,
            15..45,
            60,
        )
        .unwrap();
    let ids = [id].into();
    timeline.reverse_clips(&ids);
    let settings = ExportSettings {
        width: 64,
        height: 64,
        fps: 24,
        bitrate_kbps: 3000,
        container: Container::Mp4,
        encoder: Encoder::Mpeg4,
    };
    let sessions = CancellationList::default();
    for freeze in [false, true] {
        if freeze {
            timeline.freeze_clips(&ids, 5);
        }
        let mut renderer = Renderer::new();
        for frame in 0..30 {
            let blue = freeze || frame < 15;
            let preview = composite::render(&timeline, frame, &sources).unwrap();
            let output = renderer
                .render(&timeline, frame as f64, &sources, &settings, &sessions)
                .unwrap();
            let a = &preview[(180 * 640 + 320) * 4..][..4];
            let b = &output[(32 * 64 + 32) * 4..][..4];
            for pixel in [a, b] {
                assert!(
                    pixel[if blue { 0 } else { 2 }] > 220,
                    "freeze={freeze}, frame={frame}, pixel={pixel:?}"
                );
            }
        }
        let path = root.join(if freeze { "freeze.mp4" } else { "reverse.mp4" });
        export(
            &timeline,
            &sources,
            &settings,
            &path,
            &AtomicBool::new(false),
            |_| {},
        )
        .unwrap();
        let info = media::probe(&path).unwrap();
        assert!((info.duration - 1.0).abs() < 0.05);
        for offset in [0.1, 0.8] {
            let frame = media::frame(&path, offset).unwrap();
            let pixel = &frame[(180 * 640 + 320) * 4..][..4];
            assert!(pixel[if freeze || offset < 0.5 { 0 } else { 2 }] > 220);
        }
    }
    let low_path = root.join("low-rate.mp4");
    assert!(
        media::command("ffmpeg")
            .args(["-v", "error", "-threads", "1", "-filter_threads", "1", "-i"])
            .arg(&sources[0].path)
            .args(["-r", "24", "-c:v", "mpeg4", "-threads", "1"])
            .arg(&low_path)
            .status()
            .unwrap()
            .success()
    );
    let low = media::probe(&low_path).unwrap();
    let mut tail = timeline.clone();
    tail.clips[0].source = low.path.clone();
    tail.clips[0].source_in = 0;
    tail.clips[0].duration = 60;
    tail.clips[0].reversed = true;
    tail.clips[0].freeze_frame = None;
    for freeze in [false, true] {
        if freeze {
            tail.freeze_clips(&ids, 0);
        }
        let mut renderer = Renderer::new();
        let pixels = renderer
            .render(&tail, 0.0, std::slice::from_ref(&low), &settings, &sessions)
            .unwrap();
        assert!(
            pixels[(32 * 64 + 32) * 4] > 220,
            "低帧率尾帧不能变黑，freeze={freeze}"
        );
    }
    fs::remove_dir_all(root).unwrap();
}
