//! 真实动态库变速验收：PCM 音调、声音变调、跳转、倒放和视频预览/成片时间映射。
use super::*;
use crate::{composite, media_stream::Decoder, timeline_audio::TimelineAudio};

/// 在指定 PCM 秒级窗口数正向零交叉，验证实际音调；夹取窗口必须有足够的非静音数据。
fn frequency(samples: &[f32], start: f64, end: f64) -> f64 {
    let pairs = samples.as_chunks::<2>().0;
    let slice = &pairs[(start * 48000.0) as usize..(end * 48000.0) as usize];
    assert!(slice.iter().map(|p| p[0].abs()).sum::<f32>() / slice.len() as f32 > 0.1);
    slice
        .windows(2)
        .filter(|p| p[0][0] <= 0.0 && p[1][0] > 0.0)
        .count() as f64
        / (end - start)
}

/// 用真实 440Hz 选区检查常规变速的短片段、倒放、慢放分割和尾部能量，防止只补静音满足长度。
/// 倍率覆盖上下限以及 30/240fps，按实际项目时长检查每段最后十分之一仍有声音；结束清理独立素材。
#[test]
#[ignore = "需要配套媒体动态库及 FFmpeg 夹具工具"]
fn real_short_speed_pcm_preserves_tail_after_split() {
    let root = std::env::temp_dir().join(format!("dinuosi-short-speed-{}", uuid::Uuid::new_v4()));
    fs::create_dir_all(&root).unwrap();
    let (_, source, _) = super::tests::support::fixture(&root);
    for (fps, frames) in [30, 240]
        .into_iter()
        .flat_map(|fps| [1, 3, 6, 60].map(|frames| (fps, frames)))
    {
        for rate in [0.1, 0.5, 2.0, 100.0] {
            for reversed in [false, true] {
                let mut timeline = Timeline {
                    fps,
                    ..Default::default()
                };
                let samples_per_frame = 96000 / fps as usize;
                let id = timeline
                    .insert(
                        (source.path.clone(), source.kind),
                        "短选区".into(),
                        0,
                        0,
                        0..frames,
                        60,
                    )
                    .unwrap();
                let ids = [id].into();
                timeline.set_speed(&ids, rate, &[], false).unwrap();
                timeline.clips[0].reversed = reversed;
                if timeline.end() > 2 {
                    timeline.split(&ids, timeline.end() / 2).unwrap();
                }
                let pcm: Vec<_> =
                    TimelineAudio::start(&timeline, 0.0, std::slice::from_ref(&source))
                        .unwrap()
                        .collect();
                assert_eq!(pcm.len(), timeline.end() as usize * samples_per_frame);
                assert!(pcm.iter().all(|s| s.is_finite()));
                for clip in &timeline.clips {
                    let end = (clip.start + clip.duration) as usize * samples_per_frame;
                    let tail = &pcm[end - clip.duration as usize * (samples_per_frame / 10)..end];
                    let rms = (tail.iter().map(|s| (*s as f64).powi(2)).sum::<f64>()
                        / tail.len() as f64)
                        .sqrt();
                    assert!(
                        rms > 0.1,
                        "fps={fps}, frames={frames}, rate={rate}, reversed={reversed}, duration={}, tail RMS={rms}",
                        clip.duration
                    );
                }
            }
        }
    }
    fs::remove_dir_all(root).unwrap();
}

/// 用真实立体声 440Hz 文件验证 2 倍/半速音调、变调、倒放、跳转和取消，不依赖 Mock。
#[test]
#[ignore = "需要配套媒体动态库及 FFmpeg 夹具工具"]
fn real_speed_pcm_pitch_seek_reverse_and_cancel() {
    let root = std::env::temp_dir().join(format!("dinuosi-speed-pcm-{}", uuid::Uuid::new_v4()));
    fs::create_dir_all(&root).unwrap();
    let (_, audio, _) = super::tests::support::fixture(&root);
    let mut timeline = Timeline::default();
    let id = timeline
        .insert(
            (audio.path.clone(), audio.kind),
            "440Hz".into(),
            0,
            0,
            0..60,
            60,
        )
        .unwrap();
    let ids = [id].into();
    let sources = [audio];
    for rate in [0.5, 2.0] {
        for pitch in [false, true] {
            timeline.set_speed(&ids, rate, &[], pitch).unwrap();
            for reversed in [false, true] {
                timeline.clips[0].reversed = reversed;
                for seek in [0.0, 0.1] {
                    let pcm: Vec<_> = TimelineAudio::start(&timeline, seek, &sources)
                        .unwrap()
                        .collect();
                    assert_eq!(
                        pcm.len(),
                        ((timeline.end() as f64 / 30.0 - seek) * 96000.0).round() as usize
                    );
                    let freq = frequency(&pcm, 0.2, 0.7);
                    let expected = 440.0 * if pitch { rate } else { 1.0 };
                    assert!(
                        (freq - expected).abs() < 10.0,
                        "rate={rate} pitch={pitch} reversed={reversed} seek={seek} freq={freq}"
                    );
                    assert!(
                        pcm.as_chunks::<2>()
                            .0
                            .iter()
                            .all(|p| (p[0] - p[1]).abs() < 1e-6)
                    );
                }
            }
        }
    }
    timeline.clips[0].reversed = false;
    timeline.set_speed(&ids, 100.0, &[], false).unwrap();
    assert_eq!(
        TimelineAudio::start(&timeline, 0.0, &sources)
            .unwrap()
            .count(),
        3200
    );
    let mut decoder = Decoder::start(&sources[0], 0.0, 2.0).unwrap();
    let raw = decoder.take_audio().unwrap();
    let mut pcm = decoder.retime(raw, &timeline.clips[0], 30.0, 0.0).unwrap();
    decoder.cancellation().store(true, Ordering::Relaxed);
    assert_eq!(pcm.next(), None);
    drop(pcm);
    drop(decoder);
    let midi_path = root.join("speed.mid");
    let track = [
        0, 0xff, 0x51, 3, 7, 0xa1, 0x20, 0, 0xc0, 40, 0, 0x90, 69, 100, 0x87, 0x40, 0x80, 69, 0,
        0x87, 0x40, 0xff, 0x2f, 0,
    ];
    let mut bytes = b"MThd\0\0\0\x06\0\0\0\x01\x01\xe0MTrk".to_vec();
    bytes.extend_from_slice(&(track.len() as u32).to_be_bytes());
    bytes.extend_from_slice(&track);
    fs::write(&midi_path, bytes).unwrap();
    let midi = media::probe(&midi_path).unwrap();
    timeline.clips[0].source = midi_path;
    timeline.set_speed(&ids, 2.0, &[], false).unwrap();
    let pcm: Vec<_> = TimelineAudio::start(&timeline, 0.0, &[midi])
        .unwrap()
        .collect();
    assert_eq!(pcm.len(), 96000);
    assert!(pcm.iter().all(|s| s.is_finite()));
    assert!(pcm.iter().any(|s| s.abs() > 0.01));
    fs::remove_dir_all(root).unwrap();
}

/// 比较常规与曲线变速的预览/导出画面、成片时长和音调；保留可供真实 GUI 验收的项目及视频。
#[test]
#[ignore = "需要配套动态库及 FFmpeg/ffprobe 夹具工具"]
fn real_speed_preview_curve_and_export() {
    let root = std::env::var_os("DINUOSI_SPEED_QA")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            std::env::temp_dir().join(format!("dinuosi-speed-export-{}", uuid::Uuid::new_v4()))
        });
    fs::create_dir_all(&root).unwrap();
    let (video, audio, _) = super::tests::support::fixture(&root);
    let joined = root.join("with-audio.mp4");
    assert!(
        media::command("ffmpeg")
            .args(["-v", "error", "-threads", "1", "-i"])
            .arg(&video.path)
            .arg("-i")
            .arg(&audio.path)
            .args(["-c:v", "copy", "-c:a", "aac", "-threads", "1"])
            .arg(&joined)
            .status()
            .unwrap()
            .success()
    );
    let combined = media::probe(&joined).unwrap();
    let sources = [combined];
    let mut timeline = Timeline {
        width: 64,
        height: 64,
        ..Default::default()
    };
    let id = timeline
        .insert(
            (sources[0].path.clone(), sources[0].kind),
            "颜色与声音".into(),
            0,
            0,
            0..60,
            60,
        )
        .unwrap();
    let ids = [id].into();
    let settings = ExportSettings {
        width: 64,
        height: 64,
        fps: 30,
        bitrate_kbps: 3000,
        container: Container::Mp4,
        encoder: Encoder::Mpeg4,
    };
    let sessions = CancellationList::default();
    for (label, rate, curve) in [
        ("fast", 2.0, vec![]),
        ("slow", 0.5, vec![]),
        ("curve", 1.0, vec![[0.0, 1.0], [0.5, 4.0], [1.0, 1.0]]),
    ] {
        timeline.set_speed(&ids, rate, &curve, false).unwrap();
        let mut renderer = Renderer::new();
        for frame in [
            0,
            timeline.end() / 4,
            timeline.end() * 3 / 4,
            timeline.end() - 1,
        ] {
            let source = timeline.clips[0].source_frame(frame);
            let preview = composite::render(&timeline, frame, &sources).unwrap();
            let output = renderer
                .render(&timeline, frame as f64, &sources, &settings, &sessions)
                .unwrap();
            for pixel in [
                &preview[(180 * 640 + 320) * 4..][..4],
                &output[(32 * 64 + 32) * 4..][..4],
            ] {
                assert!(
                    pixel[if source < 30 { 2 } else { 0 }] > 220,
                    "label={label}, frame={frame}, source={source}, pixel={pixel:?}"
                );
            }
        }
        let path = root.join(format!("{label}.mp4"));
        export(
            &timeline,
            &sources,
            &settings,
            &path,
            &AtomicBool::new(false),
            |_| {},
        )
        .unwrap();
        let mut result = media::probe(&path).unwrap();
        assert!(result.has_audio);
        assert!((result.duration - timeline.end() as f64 / 30.0).abs() < 0.05);
        result.kind = media::MediaKind::Audio;
        let mut decoder = Decoder::start(&result, 0.0, result.duration).unwrap();
        let pcm: Vec<_> = decoder.take_audio().unwrap().collect();
        decoder.finish().unwrap();
        let freq = frequency(&pcm, 0.15, 0.65);
        assert!((freq - 440.0).abs() < 12.0, "{label}: {freq}");
        let duration = timeline.end() as f64 / timeline.fps as f64;
        let tail =
            &pcm[((duration - 0.06) * 96000.0) as usize..((duration - 0.02) * 96000.0) as usize];
        let rms =
            (tail.iter().map(|s| (*s as f64).powi(2)).sum::<f64>() / tail.len() as f64).sqrt();
        assert!(rms > 0.15, "{label}: premature silence at tail, rms={rms}");
        if !curve.is_empty() {
            for pitch in [false, true] {
                timeline.set_speed(&ids, rate, &curve, pitch).unwrap();
                for reversed in [false, true] {
                    timeline.clips[0].reversed = reversed;
                    let seek = 0.1;
                    let pcm: Vec<_> = TimelineAudio::start(&timeline, seek, &sources)
                        .unwrap()
                        .collect();
                    let frequency = frequency(&pcm, 0.15, 0.65);
                    let clip = &timeline.clips[0];
                    let expected = if pitch {
                        let source = (clip.source_boundary((seek + 0.65) * 30.0)
                            - clip.source_boundary((seek + 0.15) * 30.0))
                        .abs();
                        440.0 * source / 30.0 / 0.5
                    } else {
                        440.0
                    };
                    assert!(
                        (frequency - expected).abs() < 14.0,
                        "curve seek: pitch={pitch}, reverse={reversed}, actual={frequency}, expected={expected}"
                    );
                }
                timeline.clips[0].reversed = false;
            }
        }
    }
    timeline.set_speed(&ids, 1.0, &[], false).unwrap();
    super::tests::support::save_gui_fixture(&root.join("gui"), &timeline, &sources);
    if std::env::var_os("DINUOSI_SPEED_QA").is_none() {
        fs::remove_dir_all(root).unwrap();
    }
}
