//! 真实导出测试的素材与PCM分析。
use super::*;

/// 写出按帧切换红/蓝的视频与正弦音频，素材构建仅使用受控内置 MPEG-4 和 PCM 编码。
pub(in crate::export) fn fixture(root: &Path) -> (Media, Media, Media) {
    let video_path = root.join("source.mp4");
    let mut child = media::command("ffmpeg")
        .args(["-threads", "1", "-filter_threads", "1"])
        .args([
            "-v",
            "error",
            "-f",
            "rawvideo",
            "-pixel_format",
            "bgra",
            "-video_size",
            "64x64",
            "-framerate",
            "30",
            "-i",
            "pipe:0",
            "-c:v",
            "mpeg4",
            "-q:v",
            "1",
        ])
        .args(["-threads", "1"])
        .arg(&video_path)
        .stdin(Stdio::piped())
        .spawn()
        .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    for frame in 0..60 {
        stdin
            .write_all(
                &if frame < 30 {
                    [0, 0, 255, 255]
                } else {
                    [255, 0, 0, 255]
                }
                .repeat(64 * 64),
            )
            .unwrap();
    }
    drop(stdin);
    assert!(child.wait().unwrap().success());
    let audio_path = root.join("source.wav");
    let mut child = media::command("ffmpeg")
        .args(["-threads", "1", "-filter_threads", "1"])
        .args([
            "-v",
            "error",
            "-f",
            "f32le",
            "-ar",
            "48000",
            "-ac",
            "2",
            "-i",
            "pipe:0",
            "-c:a",
            "pcm_f32le",
        ])
        .args(["-threads", "1"])
        .arg(&audio_path)
        .stdin(Stdio::piped())
        .spawn()
        .unwrap();
    let mut stdin = BufWriter::new(child.stdin.take().unwrap());
    for index in 0..96000 {
        let sample =
            ((index as f32 / 48000.0 * 440.0 * std::f32::consts::TAU).sin() * 0.5).to_le_bytes();
        stdin.write_all(&sample).unwrap();
        stdin.write_all(&sample).unwrap();
    }
    drop(stdin);
    assert!(child.wait().unwrap().success());
    let image_path = root.join("image.bmp");
    let mut child = media::command("ffmpeg")
        .args(["-threads", "1", "-filter_threads", "1"])
        .args([
            "-v",
            "error",
            "-f",
            "rawvideo",
            "-pixel_format",
            "bgra",
            "-video_size",
            "16x16",
            "-i",
            "pipe:0",
            "-frames:v",
            "1",
        ])
        .args(["-threads", "1"])
        .arg(&image_path)
        .stdin(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(&[0, 0, 255, 255].repeat(16 * 16))
        .unwrap();
    assert!(child.wait().unwrap().success());
    (
        media::probe(&video_path).unwrap(),
        media::probe(&audio_path).unwrap(),
        media::probe(&image_path).unwrap(),
    )
}

/// 构造实际多轨时间线：主视频裁掉红色半段，红图位移旋转半透明，音频延迟并改变增益。
pub(in crate::export) fn timeline(video: &Media, audio: &Media, image: &Media) -> Timeline {
    let mut timeline = Timeline {
        width: 64,
        height: 64,
        ..Timeline::default()
    };
    let mut track = timeline.tracks[0].clone();
    track.kind = TrackKind::Media;
    timeline.tracks.insert(0, track.clone());
    track.kind = TrackKind::Audio;
    timeline.tracks.insert(0, track);
    timeline
        .insert(
            (video.path.clone(), MediaKind::Video),
            "video".into(),
            2,
            0,
            30..60,
            60,
        )
        .unwrap();
    let id = timeline
        .insert(
            (image.path.clone(), MediaKind::Image),
            "image".into(),
            1,
            0,
            0..30,
            150,
        )
        .unwrap();
    let clip = timeline
        .clips
        .iter_mut()
        .find(|clip| clip.id == id)
        .unwrap();
    clip.source_in = 15;
    clip.transform = Transform {
        x: -16.0,
        y: -16.0,
        scale: 200.0,
        rotation: 90.0,
        opacity: 50.0,
        ..Default::default()
    };
    clip.keyframes = vec![
        Keyframe {
            frame: 0,
            kind: KeyframeKind::Opacity,
            values: [50.0, 0.0],
        },
        Keyframe {
            frame: 30,
            kind: KeyframeKind::Opacity,
            values: [0.0, 0.0],
        },
    ];
    let id = timeline
        .insert(
            (audio.path.clone(), MediaKind::Audio),
            "audio".into(),
            0,
            6,
            6..24,
            60,
        )
        .unwrap();
    let clip = timeline
        .clips
        .iter_mut()
        .find(|clip| clip.id == id)
        .unwrap();
    clip.keyframes = vec![
        Keyframe {
            frame: 0,
            kind: KeyframeKind::Volume,
            values: [0.2, 0.0],
        },
        Keyframe {
            frame: 18,
            kind: KeyframeKind::Volume,
            values: [0.8, 0.0],
        },
    ];
    timeline
}

/// 解码目标输出为 PCM 并计算指定秒级窗口 RMS，用真实音轨验证延迟、裁剪和音量动画。
pub(in crate::export) fn rms(path: &Path, start: f64, end: f64) -> f64 {
    let output = media::command("ffmpeg")
        .args(["-v", "error", "-i"])
        .arg(path)
        .args(["-vn", "-ac", "2", "-ar", "48000", "-f", "f32le", "pipe:1"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let samples: Vec<_> = output
        .stdout
        .as_chunks::<4>()
        .0
        .iter()
        .map(|bytes| f32::from_le_bytes(*bytes) as f64)
        .collect();
    let samples = &samples[(start * 96000.0) as usize..(end * 96000.0) as usize];
    (samples.iter().map(|value| value * value).sum::<f64>() / samples.len() as f64).sqrt()
}

/// 为真实鼠标验收保存独立项目及稳定生成素材引用；仅写入本任务 .local/export-qa。
/// 重复测试复用同一验收项目并更新时间线，不修改真实用户项目与应用默认数据目录。
pub(super) fn save_gui_fixture(artifacts: &Path, timeline: &Timeline, media: &[Media]) {
    let source_folder = artifacts.join("sources");
    fs::create_dir_all(&source_folder).unwrap();
    let mut saved = timeline.clone();
    let mut paths = Vec::new();
    for item in media {
        let destination = source_folder.join(item.path.file_name().unwrap());
        fs::copy(&item.path, &destination).unwrap();
        let destination = fs::canonicalize(destination).unwrap();
        for clip in saved
            .clips
            .iter_mut()
            .filter(|clip| clip.source == item.path)
        {
            clip.source = destination.clone();
        }
        paths.push(destination);
    }
    let projects = artifacts.join("projects");
    let user_data = artifacts.join("user-data");
    fs::create_dir_all(&projects).unwrap();
    fs::create_dir_all(&user_data).unwrap();
    let store = crate::ProjectStore::new(user_data.join("recent-projects.json"));
    let entry = store
        .list()
        .unwrap()
        .into_iter()
        .find(|entry| entry.name == "导出验收")
        .unwrap_or_else(|| store.create("导出验收", &projects).unwrap());
    store.add_media_paths(&entry, &paths).unwrap();
    store.save_timeline(&entry, &saved).unwrap();
    let preferences = crate::Preferences {
        language: "zh-CN".into(),
        ..Default::default()
    };
    store.save_preferences(&preferences).unwrap();
    assert_eq!(store.load_timeline(&entry).unwrap(), saved);
}

/// 把受控测试视频与 WAV 封装成带声源，实际验证视频原声、隐藏画面保留声音及轨道静音。
/// 保留原始多轨测试的素材与期望；三个短输出仅写入本次独占临时目录，串行、单线程运行。
pub(super) fn verify_video_audio(
    video: &Media,
    audio: &Media,
    timeline: &Timeline,
    settings: &ExportSettings,
    root: &Path,
    cancel: &AtomicBool,
) {
    let path = root.join("source-with-audio.mp4");
    assert!(
        media::command("ffmpeg")
            .args(["-v", "error", "-threads", "1", "-filter_threads", "1", "-i"])
            .arg(&video.path)
            .arg("-i")
            .arg(&audio.path)
            .args([
                "-map", "0:v:0", "-map", "1:a:0", "-c:v", "copy", "-c:a", "aac", "-threads", "1"
            ])
            .arg(&path)
            .status()
            .unwrap()
            .success()
    );
    let item = media::probe(&path).unwrap();
    assert!(item.has_audio);
    let mut timeline = timeline.clone();
    timeline.clips.retain(|clip| clip.kind == MediaKind::Video);
    timeline.clips[0].source = item.path.clone();
    let track = timeline.clips[0].track;
    let audible = root.join("video-audio.mp4");
    export(
        &timeline,
        std::slice::from_ref(&item),
        settings,
        &audible,
        cancel,
        |_| {},
    )
    .unwrap();
    assert!(rms(&audible, 0.3, 0.6) > 0.15, "视频原声必须按裁剪入点输出");
    timeline.tracks[track].visible = false;
    let hidden = root.join("video-audio-hidden.mp4");
    export(
        &timeline,
        std::slice::from_ref(&item),
        settings,
        &hidden,
        cancel,
        |_| {},
    )
    .unwrap();
    assert!(rms(&hidden, 0.3, 0.6) > 0.15, "隐藏视频画面不能静音原声");
    let frame = media::frame(&hidden, 0.5).unwrap();
    assert!(
        frame
            .as_chunks::<4>()
            .0
            .iter()
            .all(|pixel| pixel[..3].iter().all(|value| *value < 5))
    );
    timeline.tracks[track].muted = true;
    let muted = root.join("video-audio-muted.mp4");
    export(&timeline, &[item], settings, &muted, cancel, |_| {}).unwrap();
    assert!(rms(&muted, 0.3, 0.6) < 0.001, "静音轨道必须关闭视频原声");
}
