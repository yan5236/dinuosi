use dinuosi_core::{media, waveform::Waveform};

/// 生成真实音频并验证静音、弱/强包络、压缩格式与损坏解码反馈；源文件始终只读。
/// 显式依赖本机 FFmpeg/ffprobe，默认忽略，避免把没有解码环境的检查当成通过。
#[test]
#[ignore = "需要本机 FFmpeg 和 ffprobe"]
fn waveform_decodes_real_audio_and_reports_failure() {
    let root = std::env::temp_dir().join(format!("dinuosi-wave-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir(&root).unwrap();
    let wav = root.join("波形.wav");
    let input =
        "aevalsrc=if(lt(t\\,0.5)\\,0\\,if(lt(t\\,1)\\,0.1\\,0.7))*sin(2*PI*440*t):s=48000:d=2";
    assert!(
        media::command("ffmpeg")
            .args(["-v", "error", "-f", "lavfi", "-i", input])
            .arg(&wav)
            .status()
            .unwrap()
            .success()
    );
    let bytes = std::fs::read(&wav).unwrap();
    for extension in ["wav", "flac", "mp3", "m4a", "ogg"] {
        let path = root.join(format!("波形.{extension}"));
        if extension != "wav" {
            assert!(
                media::command("ffmpeg")
                    .args(["-v", "error", "-i"])
                    .arg(&wav)
                    .arg(&path)
                    .status()
                    .unwrap()
                    .success()
            );
        }
        let item = media::probe(&path).unwrap();
        let waveform = Waveform::decode(&item).unwrap();
        assert!(waveform.peak(0.1, 0.2).max < 0.001, "{extension} 静音");
        let quiet = waveform.peak(0.6, 0.7);
        let loud = waveform.peak(1.4, 1.5);
        assert!(quiet.max > 0.03 && quiet.max < 0.15, "{extension} 弱段");
        assert!(
            loud.max > quiet.max * 5.0 && loud.min < quiet.min * 5.0,
            "{extension} 强段"
        );
        assert!(Waveform::decode(&media::offline(path)).is_err());
    }
    assert_eq!(std::fs::read(&wav).unwrap(), bytes);
    let mut corrupted = media::probe(&wav).unwrap();
    corrupted.path = root.join("损坏.wav");
    std::fs::write(&corrupted.path, b"not audio").unwrap();
    assert!(Waveform::decode(&corrupted).err().unwrap().contains("解码"));
    std::fs::remove_dir_all(root).unwrap();
}
