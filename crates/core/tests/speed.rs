//! 变速领域回归：源选区、曲线寻址、分割/裁剪、锁定、保存兼容、动画与帧率。
use dinuosi_core::{
    ProjectStore,
    media::MediaKind,
    timeline::{Timeline, TrackKind},
};
use std::collections::BTreeSet;

/// 创建裁过源头尾且在时间线有偏移的视频，并返回选择集；不依赖媒体解码。
fn fixture() -> (Timeline, BTreeSet<String>) {
    let mut t = Timeline::default();
    let id = t
        .insert(
            ("speed.mp4".into(), MediaKind::Video),
            "视频".into(),
            0,
            100,
            20..80,
            120,
        )
        .unwrap();
    (t, [id].into())
}

/// 倍率改变时保留源选区，时间线后续同轨片段随时长变化移动，其他轨道保持原位；1 倍恢复时长。
#[test]
fn rate_mapping_ripple_and_locked_atomicity() {
    let (mut t, ids) = fixture();
    let next = t
        .insert(
            ("next.mp4".into(), MediaKind::Video),
            "后续".into(),
            0,
            180,
            0..30,
            30,
        )
        .unwrap();
    let audio_track = t.create_track(0, TrackKind::Audio).unwrap();
    t.insert(
        ("audio.wav".into(), MediaKind::Audio),
        "音频".into(),
        audio_track,
        0,
        0..120,
        120,
    )
    .unwrap();
    t.set_speed(&ids, 2.0, &[], false).unwrap();
    let c = &t.clips[0];
    assert_eq!(c.duration, 30);
    assert_eq!(c.source_bounds(), [20.0, 80.0]);
    assert_eq!(
        [
            c.source_frame(100),
            c.source_frame(115),
            c.source_frame(129)
        ],
        [20, 50, 78]
    );
    assert_eq!(t.clips[1].start, 150);
    assert_eq!(t.clips[2].start, 0);
    t.clips.iter_mut().find(|c| c.id == next).unwrap().locked = true;
    let before = t.clone();
    assert!(t.set_speed(&ids, 0.5, &[], false).is_err());
    assert_eq!(t, before);
    t.clips[1].locked = false;
    for rate in [0.1, 100.0, 1.0] {
        t.set_speed(&ids, rate, &[], true).unwrap();
    }
    assert_eq!(t.clips[0].duration, 60);
    assert_eq!(t.clips[1].start, 180);
    for rate in [0.0, -1.0, 100.1, f64::NAN, f64::INFINITY] {
        let before = t.clone();
        assert!(t.set_speed(&ids, rate, &[], false).is_err());
        assert_eq!(t, before);
    }
}

/// 慢放分割保留亚帧源边界；正放和倒放均不丢源选区，裁剪后可扩展至完整源范围。
#[test]
fn slow_split_trim_and_reverse_preserve_source_boundaries() {
    for reversed in [false, true] {
        let (mut t, ids) = fixture();
        t.clips[0].reversed = reversed;
        t.set_speed(&ids, 0.1, &[], false).unwrap();
        if reversed {
            assert!((100..110).all(|frame| t.clips[0].source_frame(frame) == 79));
        }
        let before = t.clips[0].clone();
        let right_ids = t.split(&ids, 401).unwrap();
        let right = t.clips.iter().find(|c| right_ids.contains(&c.id)).unwrap();
        assert!((t.clips[0].source_boundary(301.0) - right.source_boundary(0.0)).abs() < 1e-7);
        assert_eq!(right.source_frame(401), before.source_frame(401));
        assert_eq!(t.clips[0].duration + right.duration, 600);
        let id = ids.first().unwrap();
        t.trim(id, false, -20).unwrap();
        t.validate().unwrap();
        assert_eq!(t.clips[0].duration, 281);
        t.delete(&right_ids);
        t.trim(id, false, 10000).unwrap();
        t.validate().unwrap();
        let bounds = t.clips[0].source_bounds();
        if reversed {
            assert!(bounds[0].abs() < 1e-6);
        } else {
            assert!((bounds[1] - 120.0).abs() < 1e-6);
        }
    }
}

/// 曲线积分决定时长且在加速处跳过更多源帧；分割、裁边与帧率变更仍保留源时间。
#[test]
fn curve_integral_split_and_fps() {
    let (mut t, ids) = fixture();
    let curve = [[0.0, 1.0], [0.5, 4.0], [1.0, 1.0]];
    t.set_speed(&ids, 1.0, &curve, false).unwrap();
    let original = t.clips[0].clone();
    assert_eq!(original.duration, 28);
    let speed = original.speed.as_ref().unwrap();
    assert!((speed.integral(1.0) - 4.0_f64.ln() / 3.0).abs() < 1e-8);
    assert!((original.source_boundary(14.0) - 50.0).abs() < 1e-6);
    let right = t.split(&ids, 114).unwrap();
    assert!((t.clips[0].source_bounds()[1] - 50.0).abs() < 1e-6);
    assert!((t.clips[1].source_bounds()[0] - 50.0).abs() < 1e-6);
    t.trim(ids.first().unwrap(), true, 3).unwrap();
    t.validate().unwrap();
    t.delete(&right);
    let bounds = t.clips[0].source_bounds();
    t.configure(1920, 1080, 60, 12000).unwrap();
    assert!((t.clips[0].source_bounds()[0] - bounds[0] * 2.0).abs() < 1e-7);
    assert!((t.clips[0].source_bounds()[1] - bounds[1] * 2.0).abs() < 1e-7);
    for curve in [
        vec![[0.2, 1.0], [1.0, 1.0]],
        vec![[0.0, 1.0], [0.5, 2.0], [0.5, 1.0], [1.0, 1.0]],
        vec![[0.0, 0.0], [1.0, 1.0]],
    ] {
        let before = t.clone();
        assert!(t.set_speed(&ids, 1.0, &curve, false).is_err());
        assert_eq!(t, before);
    }
}

/// 满选区换到会向下舍入源尾的帧率时仍可保存；精确源尾不得超过新帧率下的可用源帧数。
#[test]
fn full_source_bounds_survive_frame_rate_rounding() {
    let mut t = Timeline::default();
    let id = t
        .insert(
            ("full.mp4".into(), MediaKind::Video),
            "全选区".into(),
            0,
            0,
            0..123,
            123,
        )
        .unwrap();
    t.set_speed(&[id].into(), 2.0, &[], false).unwrap();
    t.configure(1920, 1080, 24, 12000).unwrap();
    assert_eq!(t.clips[0].source_frames, 98);
    assert_eq!(t.clips[0].source_bounds(), [0.0, 98.0]);
    t.validate().unwrap();
}

/// 保存重开保留变速和精确源区间；旧文件缺字段仍为 1 倍，损坏曲线拒绝载入且不覆盖项目。
#[test]
fn speed_persistence_and_old_project_defaults() {
    let root = std::env::temp_dir().join(format!("dinuosi-speed-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let store = ProjectStore::new(root.join("recent.json"));
    let entry = store.create("变速", &root).unwrap();
    let (mut t, ids) = fixture();
    let original = t.clone();
    store.save_timeline(&entry, &t).unwrap();
    assert_eq!(store.load_timeline(&entry).unwrap(), original);
    t.set_speed(&ids, 1.0, &[[0.0, 0.5], [0.5, 2.0], [1.0, 1.0]], true)
        .unwrap();
    store.save_timeline(&entry, &t).unwrap();
    assert_eq!(store.load_timeline(&entry).unwrap(), t);
    let mut json: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&entry.path).unwrap()).unwrap();
    json["timeline"]["clips"][0]["speed"]["source"] = serde_json::json!([-1.0, 80.0]);
    std::fs::write(&entry.path, serde_json::to_vec(&json).unwrap()).unwrap();
    assert!(store.load_timeline(&entry).is_err());
    std::fs::remove_dir_all(root).unwrap();
}
