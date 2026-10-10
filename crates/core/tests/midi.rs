use dinuosi_core::{media, media_stream::Decoder};
use std::{fs, path::Path, time::Instant};

/// 创建两秒标准 MIDI，按给定 GM 乐器和音高写入音符及速度事件；仅写独占测试目录。
fn write_midi(path: &Path, program: u8, note: u8) {
    let track = [
        0, 0xff, 0x51, 3, 7, 0xa1, 0x20, 0, 0xc0, program, 0, 0x90, note, 100, 0x87, 0x40, 0x80,
        note, 0, 0x87, 0x40, 0xff, 0x2f, 0,
    ];
    let mut bytes = b"MThd\0\0\0\x06\0\0\0\x01\x01\xe0MTrk".to_vec();
    bytes.extend_from_slice(&(track.len() as u32).to_be_bytes());
    bytes.extend_from_slice(&track);
    fs::write(path, bytes).unwrap();
}

/// 验证无需 FFmpeg 的 MIDI 探测、GM 乐器切换、准确寻址、波形峰值及损坏文件拒绝。
#[test]
fn sampled_midi_respects_instruments_and_seek() {
    let root = std::env::temp_dir().join(format!("dinuosi-midi-{}", uuid::Uuid::new_v4()));
    fs::create_dir(&root).unwrap();
    let piano = root.join("钢琴.MID");
    let violin = root.join("小提琴.midi");
    write_midi(&piano, 0, 60);
    write_midi(&violin, 40, 60);
    let piano = media::probe(&piano).unwrap();
    assert_eq!(piano.kind, media::MediaKind::Audio);
    assert_eq!(piano.duration, 2.0);
    let render = |item, start, end| {
        Decoder::start(item, start, end)
            .unwrap()
            .take_audio()
            .unwrap()
            .collect::<Vec<_>>()
    };
    let full = render(&piano, 0.0, 2.0);
    assert_eq!(full.len(), 192000);
    assert!(full.iter().all(|s| s.is_finite() && s.abs() <= 1.0));
    assert!(full.iter().any(|s| s.abs() > 0.01));
    let waveform = dinuosi_core::waveform::Waveform::decode(&piano).unwrap();
    assert_eq!(
        waveform.peak(0.0, 2.0).max,
        full.iter().copied().fold(0.0, f32::max)
    );
    assert_eq!(
        waveform.peak(0.0, 2.0).min,
        full.iter().copied().fold(0.0, f32::min)
    );
    assert_eq!(render(&piano, 0.25, 0.75), full[24000..72000]);
    let violin = media::probe(&violin).unwrap();
    assert_ne!(render(&violin, 0.0, 1.0), full[..96000]);
    let lost = media::offline(root.join("失联.mid"));
    assert_eq!(lost.kind, media::MediaKind::Audio);
    fs::write(root.join("坏.mid"), b"not MIDI").unwrap();
    assert!(media::probe(&root.join("坏.mid")).is_err());
    fs::remove_dir_all(root).unwrap();
}

/// 显式读取环境指定的真实 MIDI，验证起点和中段采样及寻址耗时；不修改用户素材。
/// DINUOSI_VERIFY_MIDI 为平台路径列表，输出短试听 WAV 到指定验证目录。
#[test]
#[ignore = "需要 DINUOSI_VERIFY_MIDI 指定真实素材"]
fn real_midi_preview_samples() {
    let paths = std::env::var_os("DINUOSI_VERIFY_MIDI").expect("指定实际 MIDI 文件");
    let output = std::env::var_os("DINUOSI_VERIFY_OUTPUT").map(std::path::PathBuf::from);
    for (index, path) in std::env::split_paths(&paths).enumerate() {
        let item = media::probe(&path).unwrap();
        for start in [0.0, item.duration / 2.0] {
            let began = Instant::now();
            let mut decoder =
                Decoder::start(&item, start, (start + 2.0).min(item.duration)).unwrap();
            let initialized = began.elapsed();
            let samples: Vec<_> = decoder.take_audio().unwrap().collect();
            assert!(!samples.is_empty());
            assert!(samples.iter().all(|s| s.is_finite()));
            assert!(samples.iter().any(|s| s.abs() > 0.001));
            eprintln!(
                "{} start={start:.3} duration={:.3} seek={initialized:?} total={:?}",
                path.display(),
                item.duration,
                began.elapsed()
            );
            if start == 0.0
                && let Some(output) = &output
            {
                write_wav(&output.join(format!("midi-{index}.wav")), &samples);
            }
        }
    }
}

/// 将验证 PCM 写为立体声 48kHz、16 位 WAV；仅供人工对听，不参与产品缓存或源文件保存。
fn write_wav(path: &Path, samples: &[f32]) {
    let bytes = (samples.len() * 2) as u32;
    let mut wav = b"RIFF".to_vec();
    wav.extend_from_slice(&(bytes + 36).to_le_bytes());
    wav.extend_from_slice(b"WAVEfmt \x10\0\0\0\x01\0\x02\0");
    wav.extend_from_slice(&48000u32.to_le_bytes());
    wav.extend_from_slice(&192000u32.to_le_bytes());
    wav.extend_from_slice(b"\x04\0\x10\0data");
    wav.extend_from_slice(&bytes.to_le_bytes());
    for sample in samples {
        wav.extend_from_slice(&((sample.clamp(-1.0, 1.0) * 32767.0) as i16).to_le_bytes());
    }
    fs::write(path, wav).unwrap();
}

/// 验证内嵌音源的 128 种 GM 乐器均输出有限且有声的 PCM，并检查第十声道低音鼓有声音。
/// 使用独占临时 MIDI，逐音色尝试低/中/高音区；通过后清理目录，不代表逐项覆盖全部打击乐音色。
#[test]
fn embedded_font_plays_all_gm_instruments_and_percussion() {
    let root = std::env::temp_dir().join(format!("dinuosi-gm-{}", uuid::Uuid::new_v4()));
    fs::create_dir(&root).unwrap();
    let path = root.join("乐器.mid");
    for program in 0..128 {
        let mut audible = false;
        for note in [36, 48, 60, 72, 84] {
            write_midi(&path, program, note);
            let item = media::probe(&path).unwrap();
            let samples: Vec<_> = Decoder::start(&item, 0.0, 0.5)
                .unwrap()
                .take_audio()
                .unwrap()
                .collect();
            assert_eq!(samples.len(), 48000);
            assert!(samples.iter().all(|s| s.is_finite() && s.abs() <= 1.0));
            audible |= samples.iter().any(|s| s.abs() > 0.0001);
            if audible {
                break;
            }
        }
        assert!(audible, "GM 乐器 {program} 在各个测试音域均没有声音");
    }
    write_midi(&path, 0, 60);
    let mut bytes = fs::read(&path).unwrap();
    for index in 22..bytes.len() - 2 {
        if bytes[index] == 0x90 || bytes[index] == 0x80 {
            bytes[index] |= 9;
            bytes[index + 1] = 36;
        }
    }
    fs::write(&path, bytes).unwrap();
    let item = media::probe(&path).unwrap();
    let samples: Vec<_> = Decoder::start(&item, 0.0, 0.2)
        .unwrap()
        .take_audio()
        .unwrap()
        .collect();
    assert!(samples.iter().any(|s| s.abs() > 0.001), "GM 打击乐没有声音");
    fs::remove_dir_all(root).unwrap();
}
