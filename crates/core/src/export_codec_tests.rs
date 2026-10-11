//! 新增编码器与帧缓冲复用回归，真实编码仍使用应用配套运行库。
use super::*;

/// 验证不同编码实现使用各自参数，软件 AV1/VP9 不接收 Windows MFT 的专用开关。
#[test]
fn encoder_arguments_match_the_selected_implementation() {
    let candidates = Encoder::candidates();
    assert_eq!(
        candidates.contains(&Encoder::H264MediaFoundation),
        cfg!(target_os = "windows")
    );
    assert_eq!(
        candidates.contains(&Encoder::H264VideoToolbox),
        cfg!(target_os = "macos")
    );
    assert_eq!(
        candidates.contains(&Encoder::H264OpenH264),
        !cfg!(target_os = "windows")
    );
    for (encoder, expected) in [
        (Encoder::Av1, "cpu-used=6:row-mt=1"),
        (Encoder::Vp9, "deadline=good:cpu-used=4:row-mt=1"),
        (Encoder::Mpeg4, ""),
        (Encoder::H264MediaFoundationHardware, "hw_encoding=1"),
        (Encoder::H264MediaFoundation, "hw_encoding=0"),
        (Encoder::H264VideoToolboxHardware, "allow_sw=0"),
        (Encoder::H264VideoToolbox, "allow_sw=1:require_sw=1"),
        (Encoder::H264OpenH264, ""),
    ] {
        assert_eq!(encoder.options(), expected);
    }
}

/// 真实试编码后对 AV1/VP9 的 MP4/MKV 输出验证格式、时长、裁剪混合像素与音量动画。
/// 固定受控运行库必须包含两种软件编码器；每次使用独立 QA 子目录，避免旧素材阻止重复运行。
#[test]
#[ignore = "需要准备包含 libaom 与 libvpx 的配套 FFmpeg"]
fn real_av1_vp9_exports() {
    let start = std::time::Instant::now();
    let available = capabilities().unwrap();
    println!("首次能力探测耗时 {:?}", start.elapsed());
    let start = std::time::Instant::now();
    assert_eq!(capabilities().unwrap(), available);
    println!("缓存能力读取耗时 {:?}", start.elapsed());
    assert!(available.contains(&Encoder::Av1));
    assert!(available.contains(&Encoder::Vp9));
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../.local/export-codecs-qa")
        .join(uuid::Uuid::new_v4().to_string());
    fs::create_dir_all(&root).unwrap();
    let (video, audio, image) = super::tests::support::fixture(&root);
    let timeline = super::tests::support::timeline(&video, &audio, &image);
    let items = [video, audio, image];
    for encoder in [Encoder::Av1, Encoder::Vp9] {
        for container in [Container::Mp4, Container::Matroska] {
            let settings = ExportSettings {
                width: 128,
                height: 128,
                fps: 24,
                bitrate_kbps: 3000,
                container,
                encoder,
            };
            let output = root.join(format!(
                "{}-{}.{}",
                encoder.label(),
                uuid::Uuid::new_v4(),
                container.extension()
            ));
            let start = std::time::Instant::now();
            export(
                &timeline,
                &items,
                &settings,
                &output,
                &AtomicBool::new(false),
                |_| {},
            )
            .unwrap();
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
            assert!(probe.status.success());
            let data: serde_json::Value = serde_json::from_slice(&probe.stdout).unwrap();
            let streams = data["streams"].as_array().unwrap();
            let video = streams.iter().find(|s| s["codec_type"] == "video").unwrap();
            assert_eq!(video["codec_name"], encoder.label().to_lowercase());
            assert_eq!(video["width"], 128);
            assert_eq!(video["height"], 128);
            assert_eq!(video["r_frame_rate"], "24/1");
            assert!(streams.iter().any(|s| s["codec_name"] == "aac"));
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
            let frame = media::frame(&output, 0.5).unwrap();
            let blue = &frame[(270 * 640 + 410) * 4..][..4];
            let overlay = &frame[(90 * 640 + 230) * 4..][..4];
            assert!(blue[0] > 230 && blue[2] < 20, "{blue:?}");
            assert!(
                overlay[0] > 160 && overlay[2] > 40 && overlay[2] < 100,
                "{overlay:?}"
            );
            assert!(
                super::tests::support::rms(&output, 0.65, 0.75)
                    > super::tests::support::rms(&output, 0.25, 0.35) * 1.8
            );
            fs::write(
                output.with_extension("json"),
                serde_json::to_vec_pretty(&data).unwrap(),
            )
            .unwrap();
            println!(
                "{} / {} 真实时间线导出及解码通过 {:?}",
                encoder.label(),
                container.label(),
                start.elapsed()
            );
        }
    }
}
