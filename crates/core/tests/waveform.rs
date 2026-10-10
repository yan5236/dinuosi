use dinuosi_core::{media, waveform::Waveform};

/// 生成真实音频并验证静音、弱/强包络、压缩格式、损坏解码反馈及无 PCM 拒绝；源文件始终只读。
/// 显式依赖应用配套 FFmpeg/ffprobe，默认忽略，避免把没有解码环境的检查当成通过。
#[test]
#[ignore = "需要准备应用配套 FFmpeg 和 ffprobe"]
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
                    .args(["-strict", "-2", "-ac", "2"])
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
    let empty = root.join("空音频.wav");
    assert!(
        media::command("ffmpeg")
            .args(["-v", "error", "-f", "lavfi", "-i", "anullsrc", "-t", "0"])
            .arg(&empty)
            .status()
            .unwrap()
            .success()
    );
    corrupted.path = empty;
    assert_eq!(
        Waveform::decode(&corrupted).err().unwrap(),
        "音频解码没有产生样本"
    );
    std::fs::remove_dir_all(root).unwrap();
}

/// 生成固定码率 MP3 并损坏最后一个音频包，验证 FFmpeg 成功恢复后的 PCM、波形及共享错误状态。
/// 只使用独立临时素材；同时确认完全损坏时仍返回 FFmpeg 详情，避免把实际失败吞掉。
#[test]
#[ignore = "需要准备应用配套 FFmpeg 和 ffprobe"]
fn waveform_keeps_recoverable_mp3_and_rejects_fatal_damage() {
    let root = std::env::temp_dir().join(format!("dinuosi-bad-packet-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir(&root).unwrap();
    let path = root.join("坏尾包.mp3");
    assert!(
        media::command("ffmpeg")
            .args([
                "-v",
                "error",
                "-f",
                "lavfi",
                "-i",
                "sine=frequency=440:sample_rate=44100:duration=2",
                "-ac",
                "2",
                "-b:a",
                "128k",
            ])
            .arg(&path)
            .status()
            .unwrap()
            .success()
    );
    let item = media::probe(&path).unwrap();
    let packets = media::command("ffprobe")
        .args([
            "-v",
            "error",
            "-show_packets",
            "-show_entries",
            "packet=pos,size",
            "-of",
            "json",
        ])
        .arg(&path)
        .output()
        .unwrap();
    assert!(packets.status.success());
    let data: serde_json::Value = serde_json::from_slice(&packets.stdout).unwrap();
    let packets = data["packets"].as_array().unwrap();
    let last = packets.last().unwrap();
    let pos: usize = last["pos"].as_str().unwrap().parse().unwrap();
    let size: usize = last["size"].as_str().unwrap().parse().unwrap();
    let mut bytes = std::fs::read(&path).unwrap();
    bytes[pos + 4..pos + size].fill(0xff);
    std::fs::write(&path, &bytes).unwrap();
    let output = media::command("ffmpeg")
        .args(["-v", "error", "-ss", "0", "-i"])
        .arg(&path)
        .args(["-vn", "-ac", "2", "-ar", "48000", "-f", "f32le", "pipe:1"])
        .output()
        .unwrap();
    assert!(output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("Invalid data"));
    assert!(output.stdout.len() > 384000 * 19 / 10);
    let mut decoder =
        dinuosi_core::media_stream::Decoder::start(&item, 0.0, item.duration).unwrap();
    let error = decoder.error.clone();
    assert!(decoder.take_audio().unwrap().count() > 96000 * 19 / 10);
    decoder.finish().unwrap();
    assert!(
        error.lock().unwrap().is_none(),
        "可恢复坏包不向播放器发布失败"
    );
    let waveform = Waveform::decode(&item).unwrap();
    assert!(waveform.peak(0.1, 0.2).max > 0.03);
    assert!(waveform.peak(1.8, 1.9).min < -0.03);
    assert_eq!(std::fs::read(&path).unwrap(), bytes);
    for packet in packets {
        let pos: usize = packet["pos"].as_str().unwrap().parse().unwrap();
        let size: usize = packet["size"].as_str().unwrap().parse().unwrap();
        bytes[pos + 4..pos + size].fill(0xff);
    }
    std::fs::write(&path, bytes).unwrap();
    let mut decoder =
        dinuosi_core::media_stream::Decoder::start(&item, 0.0, item.duration).unwrap();
    let error = decoder.error.clone();
    assert_eq!(decoder.take_audio().unwrap().count(), 0);
    assert!(decoder.finish().unwrap_err().contains("媒体解码失败"));
    assert!(
        error
            .lock()
            .unwrap()
            .as_ref()
            .unwrap()
            .contains("Invalid data")
    );
    assert!(Waveform::decode(&item).is_err());
    std::fs::remove_dir_all(root).unwrap();
}
