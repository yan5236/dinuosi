use dinuosi_core::{
    ProjectStore, composite,
    media::{FRAME_HEIGHT, FRAME_WIDTH},
    timeline::{History, Property, Timeline, Transform, ticks},
};
use std::collections::BTreeSet;

/// 验证帧精确插入、组移动、裁边、锁定、删除与撤销，不需要 GUI 或媒体解码。
#[test]
fn edits_preserve_source_limits_and_history() {
    let mut t = Timeline::default();
    let a = t
        .insert("a.mp4".into(), "a".into(), 2, 30, 15..105, 120)
        .unwrap();
    let b = t
        .insert("b.mp4".into(), "b".into(), 2, 150, 0..90, 120)
        .unwrap();
    let selected = BTreeSet::from([a.clone(), b.clone()]);
    let before = t.clone();
    let mut history = History::default();
    t.move_clips(&selected, -60, -1).unwrap();
    assert_eq!(
        (t.clips[0].start, t.clips[1].start, t.clips[0].track),
        (0, 120, 1)
    );
    t.trim(&a, true, 500).unwrap();
    assert_eq!((t.clips[0].duration, t.clips[0].source_in), (1, 104));
    t.trim(&a, false, 500).unwrap();
    assert_eq!(t.clips[0].duration, 16);
    history.record(before.clone(), &t);
    history.step(&mut t, false);
    assert_eq!(t, before);
    history.step(&mut t, true);
    assert_eq!(t.clips[0].duration, 16);
    t.tracks[1].locked = true;
    assert!(t.move_clips(&selected, 30, 0).is_err());
    t.delete(&selected);
    assert_eq!(t.clips.len(), 2);
    t.tracks[1].locked = false;
    t.delete(&selected);
    assert!(t.clips.is_empty());
}

/// 验证参数边界、跨轨框选和帧标尺；最大缩放在两帧处标注 2f，短线每帧出现。
#[test]
fn properties_selection_and_adaptive_ticks() {
    let mut t = Timeline::default();
    let id = t
        .insert("a.png".into(), "a".into(), 1, 30, 0..150, 150)
        .unwrap();
    let selected = BTreeSet::from([id]);
    assert_eq!(t.select_range(0, 31, 1, 2), selected);
    assert!(t.select_range(0, 30, 1, 2).is_empty());
    t.set_property(&selected, Property::Rotation, -500.0)
        .unwrap();
    t.set_property(&selected, Property::Scale, 900.0).unwrap();
    assert_eq!(t.clips[0].transform.rotation, -360.0);
    assert_eq!(t.clips[0].transform.scale, 500.0);
    assert!(t.set_property(&selected, Property::X, f64::NAN).is_err());
    let near = ticks(30, 1800.0, 0.0, 800.0);
    assert_eq!(near[1].frame, 1);
    assert_eq!(near[2].label, "2f");
    assert_eq!(near[4].label, "4f");
    let far = ticks(30, 20.0, 0.0, 800.0);
    assert!(far.iter().any(|t| t.label == "00:05"));
    assert_eq!(t.frame(1.0 / 30.0), 1);
}

/// 验证时间线落盘重开和旧 V1 项目兼容；素材库删除不会删除源文件或时间线引用。
#[test]
fn timeline_round_trip_and_legacy_project() {
    let root = std::env::temp_dir().join(format!("dinuosi-timeline-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&root).unwrap();
    let store = ProjectStore::new(root.join("recent.json"));
    let entry = store.create("时间线", &root).unwrap();
    let source = root.join("image.png");
    std::fs::write(&source, b"source").unwrap();
    store
        .add_media_paths(&entry, std::slice::from_ref(&source))
        .unwrap();
    let mut t = Timeline::default();
    t.insert(source.clone(), "图像".into(), 2, 60, 0..150, 150)
        .unwrap();
    t.clips[0].transform.x = -123.0;
    store.save_timeline(&entry, &t).unwrap();
    assert_eq!(store.load_timeline(&entry).unwrap(), t);
    store
        .remove_media(&entry, &std::fs::canonicalize(&source).unwrap())
        .unwrap();
    assert!(source.exists());
    assert_eq!(store.load_timeline(&entry).unwrap(), t);
    let mut json: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&entry.path).unwrap()).unwrap();
    json.as_object_mut().unwrap().remove("timeline");
    std::fs::write(&entry.path, serde_json::to_vec(&json).unwrap()).unwrap();
    assert!(store.load_timeline(&entry).unwrap().clips.is_empty());
    std::fs::remove_dir_all(root).unwrap();
}

/// 验证中心坐标、透明度和旋转确实改变预览像素，防止检查器仅改变显示数值。
#[test]
fn composite_applies_centered_transform_and_opacity() {
    let len = (FRAME_WIDTH * FRAME_HEIGHT * 4) as usize;
    let mut source = vec![0; len];
    let center = ((180 * FRAME_WIDTH + 320) * 4) as usize;
    source[center..center + 4].copy_from_slice(&[200, 100, 50, 255]);
    let mut out = vec![0; len];
    composite::blend(&mut out, &source, &Transform::default(), (1920, 1080)).unwrap();
    assert_eq!(&out[center..center + 4], &[200, 100, 50, 255]);
    out.fill(0);
    let t = Transform {
        x: 30.0,
        opacity: 50.0,
        ..Default::default()
    };
    composite::blend(&mut out, &source, &t, (1920, 1080)).unwrap();
    assert_eq!(&out[center + 40..center + 44], &[100, 50, 25, 255]);
    assert_eq!(out[center], 0);
    out.fill(0);
    composite::blend(
        &mut out,
        &source,
        &Transform {
            scale: 0.0,
            ..Default::default()
        },
        (1920, 1080),
    )
    .unwrap();
    assert!(out.iter().all(|v| *v == 0));
}

/// 使用真实解码器检查音频片段的延迟、叠加与轨道静音，确保拖入音频后仍可实际播放。
#[test]
#[ignore = "需要本机 FFmpeg 和 ffprobe"]
fn timeline_audio_mixes_and_respects_track_mute() {
    let root = std::env::temp_dir().join(format!("dinuosi-mix-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&root).unwrap();
    let source = root.join("constant.wav");
    assert!(
        dinuosi_core::media::command("ffmpeg")
            .args([
                "-v",
                "error",
                "-f",
                "lavfi",
                "-i",
                "aevalsrc=0.125|0.125:s=48000:d=0.2",
                "-ac",
                "2",
                "-c:a",
                "pcm_f32le"
            ])
            .arg(&source)
            .status()
            .unwrap()
            .success()
    );
    let media = dinuosi_core::media::probe(&source).unwrap();
    let mut timeline = Timeline::default();
    for track in [1, 2] {
        timeline
            .insert(media.path.clone(), "tone".into(), track, 3, 0..6, 6)
            .unwrap();
    }
    for (muted, expected) in [(false, 0.25), (true, 0.125)] {
        timeline.tracks[1].muted = muted;
        let mut audio = dinuosi_core::timeline_audio::TimelineAudio::start(
            &timeline,
            0.0,
            std::slice::from_ref(&media),
        )
        .unwrap();
        assert!(audio.by_ref().take(9600).all(|s| s == 0.0));
        let sample = audio.nth(1000).unwrap();
        assert!((sample - expected).abs() < 0.001, "{sample} != {expected}");
    }
    std::fs::remove_dir_all(root).unwrap();
}
