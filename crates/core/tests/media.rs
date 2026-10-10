use dinuosi_core::media::{self, MediaKind};

/// 用竖屏、方形、超宽及非方形像素素材验证比例与透明补边；采样区须完整不被裁切或拉伸。
/// 需真实 FFmpeg；静态帧与连续视频帧共享参数，检查 alpha 和有效画面矩形。
#[test]
#[ignore = "需要准备应用配套 FFmpeg 和 ffprobe"]
fn preview_padding_tracks_display_aspect_ratio() {
    let root = std::env::temp_dir().join(format!("dinuosi-aspect-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir(&root).unwrap();
    for (index, input, expected) in [
        (0, "color=blue:size=180x320", (203, 360)),
        (1, "color=blue:size=240x240", (360, 360)),
        (2, "color=blue:size=600x150", (640, 160)),
        (3, "color=blue:size=240x240,setsar=2", (640, 320)),
    ] {
        let path = root.join(format!("ratio-{index}.mp4"));
        assert!(
            media::command("ffmpeg")
                .args([
                    "-v", "error", "-f", "lavfi", "-i", input, "-t", "0.25", "-c:v", "mpeg4"
                ])
                .arg(&path)
                .status()
                .unwrap()
                .success()
        );
        let item = media::probe(&path).unwrap();
        assert_eq!(item.preview_size, expected);
        let frame = item.thumbnail.as_ref().unwrap();
        let (width, height) = expected;
        let (left, top) = ((640 - width) / 2, (360 - height) / 2);
        for y in 0..360 {
            for x in 0..640 {
                let inside = x >= left && x < left + width && y >= top && y < top + height;
                assert_eq!(
                    frame[((y * 640 + x) * 4 + 3) as usize],
                    if inside { 255 } else { 0 },
                    "({x},{y}) {index}"
                );
            }
        }
        let decoder = dinuosi_core::media_stream::Decoder::start(&item, 0.0, 0.2).unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
        while decoder.frames.lock().unwrap().is_none() && std::time::Instant::now() < deadline {
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        let dynamic = decoder.frames.lock().unwrap().take().unwrap();
        assert_eq!(dynamic[3], 0);
        assert_eq!(dynamic[(180 * 640 + 320) * 4 + 3], 255);
    }
    std::fs::remove_dir_all(root).unwrap();
}

/// 用 FFmpeg 生成真实视频、音频和图片，验证类型/时长、寻址帧、PCM 区间及流式画面。
/// 同时验证无头批量导入、去重、坏文件反馈、引用持久化和改名后的离线保留/恢复。
/// 需在测试程序旁部署配套 FFmpeg/ffprobe；显式运行，不从 PATH 补找工具。
#[test]
#[ignore = "需要准备应用配套 FFmpeg 和 ffprobe"]
fn ffmpeg_reads_real_media_and_seek_frames() {
    let root = std::env::temp_dir().join(format!("dinuosi-decode-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let video = root.join("测试视频.mp4");
    let audio = root.join("音乐.wav");
    let image = root.join("图片.png");
    for (path, args) in [
        (
            &video,
            vec![
                "-f",
                "lavfi",
                "-i",
                "testsrc2=size=320x180:rate=24",
                "-f",
                "lavfi",
                "-i",
                "sine=frequency=440:sample_rate=48000",
                "-t",
                "3",
                "-c:v",
                "mpeg4",
                "-pix_fmt",
                "yuv420p",
                "-c:a",
                "aac",
            ],
        ),
        (
            &audio,
            vec![
                "-f",
                "lavfi",
                "-i",
                "sine=frequency=660:sample_rate=48000",
                "-t",
                "3",
            ],
        ),
        (
            &image,
            vec![
                "-f",
                "lavfi",
                "-i",
                "color=blue:size=320x180",
                "-frames:v",
                "1",
            ],
        ),
    ] {
        assert!(
            media::command("ffmpeg")
                .args(["-v", "error", "-y"])
                .args(args)
                .arg(path)
                .status()
                .unwrap()
                .success()
        );
    }
    let video = media::probe(&video).unwrap();
    assert_eq!(video.kind, MediaKind::Video);
    assert!(video.has_audio);
    assert!((video.duration - 3.0).abs() < 0.1);
    let first = video.thumbnail.as_ref().unwrap();
    let later = media::frame(&video.path, 1.5).unwrap();
    assert_ne!(first.as_ref(), &later);
    assert_eq!(media::probe(&audio).unwrap().kind, MediaKind::Audio);
    assert_eq!(media::probe(&image).unwrap().kind, MediaKind::Image);
    let audio_media = media::probe(&audio).unwrap();
    let mut decoder = dinuosi_core::media_stream::Decoder::start(&audio_media, 0.25, 0.75).unwrap();
    let samples: Vec<_> = decoder.take_audio().unwrap().collect();
    assert_eq!(samples.len(), 48000);
    assert!(samples.iter().all(|s| s.is_finite()));
    assert!(samples.iter().any(|s| s.abs() > 0.05));
    assert!(decoder.take_audio().is_none());
    assert!(dinuosi_core::media_stream::Decoder::start(&audio_media, 1.0, 0.5).is_err());
    assert!(dinuosi_core::media_stream::Decoder::start(&audio_media, 0.0, 4.0).is_err());
    drop(decoder);
    let mut decoder = dinuosi_core::media_stream::Decoder::start(&video, 0.0, 0.5).unwrap();
    let samples: Vec<_> = decoder.take_audio().unwrap().collect();
    assert_eq!(samples.len(), 48000);
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
    while decoder.frames.lock().unwrap().is_none() && std::time::Instant::now() < deadline {
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    assert_eq!(
        decoder.frames.lock().unwrap().as_ref().unwrap().len(),
        640 * 360 * 4
    );
    assert!(decoder.error.lock().unwrap().is_none());
    drop(decoder);
    let renamed = video.path.with_file_name("已改名.mp4");
    std::fs::rename(&video.path, &renamed).unwrap();
    let lost = media::check_sources(vec![video.clone()]).remove(0);
    assert!(lost.offline);
    assert_eq!(lost.path, video.path);
    assert_eq!(lost.duration, video.duration);
    assert!(dinuosi_core::media_stream::Decoder::start(&lost, 0.0, 1.0).is_err());
    std::fs::rename(&renamed, &video.path).unwrap();
    let restored = media::check_sources(vec![lost]).remove(0);
    assert!(!restored.offline);
    assert!(restored.thumbnail.is_some());
    let bad = root.join("损坏.mp4");
    std::fs::write(&bad, b"not video").unwrap();
    assert!(media::probe(&bad).is_err());
    assert!(media::probe(&root).is_err());
    let store = dinuosi_core::ProjectStore::new(root.join("recent.json"));
    let entry = store.create("无头导入验证", &root).unwrap();
    let imported = store
        .import_media(
            &entry,
            &[video.path.clone(), audio.clone(), video.path.clone(), bad],
        )
        .unwrap();
    assert_eq!(imported.media.len(), 2);
    assert_eq!(imported.errors.len(), 1);
    assert_eq!(store.media_paths(&entry).unwrap().len(), 2);
    assert_eq!(store.load_media(&entry).unwrap().len(), 2);
    std::fs::rename(&video.path, &renamed).unwrap();
    let reopened = store.load_media(&entry).unwrap();
    assert_eq!(reopened.len(), 2);
    assert!(reopened[0].offline);
    assert_eq!(reopened[0].path, video.path);
    std::fs::rename(renamed, &video.path).unwrap();
    std::fs::remove_dir_all(root).unwrap();
}

/// 24fps 源在 30fps 项目最后采样点之后已无可解码新帧，仍返回真实尾帧；损坏源继续报错。
#[test]
#[ignore = "需要准备应用配套 FFmpeg 和 ffprobe"]
fn seek_after_last_source_frame_returns_real_tail_frame() {
    let root = std::env::temp_dir().join(format!("dinuosi-tail-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&root).unwrap();
    let path = root.join("tail.mp4");
    assert!(
        media::command("ffmpeg")
            .args([
                "-v",
                "error",
                "-f",
                "lavfi",
                "-i",
                "testsrc2=size=320x180:rate=24",
                "-t",
                "1",
                "-c:v",
                "mpeg4"
            ])
            .arg(&path)
            .status()
            .unwrap()
            .success()
    );
    let last = media::frame(&path, 23.0 / 24.0 - 0.000001).unwrap();
    assert_ne!(last, media::frame(&path, 0.0).unwrap());
    assert_eq!(media::frame(&path, 29.0 / 30.0).unwrap(), last);
    assert_eq!(media::frame(&path, 1.0).unwrap(), last);
    let broken = root.join("broken.mp4");
    std::fs::write(&broken, b"broken").unwrap();
    assert!(media::frame(&broken, 0.0).is_err());
    std::fs::remove_dir_all(root).unwrap();
}

/// 读取既有合成 H.264 样本，验证移除 GPL 编码器后仍能探测、寻址和输出真实视频像素。
/// 夹具由原有合成素材无损截取；断言媒体类型、输出缓冲长度及非灰度像素，不校验逐像素一致性。
#[test]
#[ignore = "需要 tools/prepare-media.ps1 准备固定运行时"]
fn lgpl_runtime_decodes_h264_fixture() {
    let path =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/video-h264.mp4");
    let item = media::probe(&path).unwrap();
    assert_eq!(item.kind, MediaKind::Video);
    let frame = media::frame(&item.path, 0.0).unwrap();
    assert_eq!(frame.len(), 640 * 360 * 4);
    assert!(
        frame
            .as_chunks::<4>()
            .0
            .iter()
            .any(|pixel| pixel[0] != pixel[1] || pixel[1] != pixel[2])
    );
}
