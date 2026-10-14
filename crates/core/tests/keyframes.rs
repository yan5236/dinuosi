//! 关键帧业务回归：类型隔离、线性求值、时间编辑、磁盘兼容和非法数据拒绝。
use dinuosi_core::{
    ProjectStore,
    keyframes::{Keyframe, KeyframeKind as K},
    media::MediaKind,
    timeline::{History, Property, Timeline},
};
use std::collections::BTreeSet;
/// 创建不依赖解码的五秒片段，返回唯一选择集；音频自动创建独立轨道。
fn fixture(kind: MediaKind) -> (Timeline, BTreeSet<String>) {
    let mut t = Timeline::default();
    let id = t
        .insert(
            ("source.png".into(), kind),
            "source".into(),
            0,
            30,
            0..150,
            150,
        )
        .unwrap();
    (t, [id].into())
}
/// 验证手动首帧、同类自动添加、坐标组合、不同类型隔离和相邻帧导航数据。
#[test]
fn linear_keys_are_grouped_and_auto_add_only_changed_types() {
    let (mut t, ids) = fixture(MediaKind::Image);
    t.toggle_keyframe(&ids, K::Position, 30);
    t.toggle_keyframe(&ids, K::Scale, 30);
    t.set_property_at(&ids, Property::X, 100.0, 31, true)
        .unwrap();
    assert_eq!(t.clips[0].keyframes.len(), 3);
    assert_eq!(t.clips[0].keyframe_frames(None), [30, 31].into());
    t.set_property_at(&ids, Property::Y, 60.0, 31, true)
        .unwrap();
    assert_eq!(t.clips[0].keyframes.len(), 3);
    assert_eq!(t.clips[0].keyframe_values(K::Position, 30.5), [50.0, 30.0]);
    assert_eq!(t.clips[0].transform_at(100.0).x, 100.0);
    assert_eq!(t.clips[0].transform_at(0.0).x, 0.0);
    t.set_property_at(&ids, Property::Scale, 200.0, 31, true)
        .unwrap();
    assert_eq!(t.clips[0].keyframes.len(), 4);
    t.set_property_at(&ids, Property::Opacity, 50.0, 32, true)
        .unwrap();
    assert_eq!(t.clips[0].keyframes.len(), 4);
    assert_eq!(t.clips[0].transform_at(32.0).opacity, 50.0);
    let before = t.clone();
    t.set_property_at(&ids, Property::X, 100.0, 32, true)
        .unwrap();
    assert_eq!(t, before);
    t.toggle_keyframe(&ids, K::Position, 31);
    assert_eq!(t.clips[0].keyframes.len(), 3);
    t.toggle_keyframe(&ids, K::Position, 30);
    assert!(t.clips[0].keyframe_frames(Some(K::Position)).is_empty());
    t.validate().unwrap();
}
/// 验证关闭自动添加时帧间整体偏移、已有帧局部更新、端点约束及历史原子恢复。
#[test]
fn disabled_auto_offsets_curve_and_exact_frame_edits_one_key() {
    let (mut t, ids) = fixture(MediaKind::Image);
    t.toggle_keyframe(&ids, K::Position, 30);
    t.set_property_at(&ids, Property::X, 100.0, 60, true)
        .unwrap();
    let before = t.clone();
    t.set_property_at(&ids, Property::X, 60.0, 45, false)
        .unwrap();
    assert_eq!(t.clips[0].transform_at(30.0).x, 10.0);
    assert_eq!(t.clips[0].transform_at(60.0).x, 110.0);
    assert_eq!(t.clips[0].keyframes.len(), 2);
    t.set_property_at(&ids, Property::X, 20.0, 30, false)
        .unwrap();
    assert_eq!(t.clips[0].transform_at(60.0).x, 110.0);
    let mut history = History::default();
    history.record(before.clone(), &t);
    history.step(&mut t, false);
    assert_eq!(t, before);
    history.step(&mut t, true);
    assert_eq!(t.clips[0].transform_at(30.0).x, 20.0);
    let unchanged = t.clone();
    t.set_property_at(&ids, Property::X, 999.0, 180, true)
        .unwrap();
    assert_eq!(t, unchanged);
    t.tracks[0].locked = true;
    let unchanged = t.clone();
    t.toggle_keyframe(&ids, K::Position, 45);
    assert_eq!(t, unchanged);
}
/// 验证移动随片段、裁边保留锚点、分割两侧保持原动画，帧率换算保持绝对时间。
#[test]
fn edits_preserve_animation_and_reject_frame_collisions() {
    let (mut t, ids) = fixture(MediaKind::Image);
    t.toggle_keyframe(&ids, K::Position, 30);
    t.set_property_at(&ids, Property::X, 120.0, 90, true)
        .unwrap();
    t.move_clips(&ids, 30, 0).unwrap();
    assert_eq!(t.clips[0].transform_at(90.0).x, 60.0);
    let id = ids.first().unwrap();
    t.trim(id, true, 15).unwrap();
    assert_eq!(t.clips[0].transform_at(90.0).x, 60.0);
    let right = t.split(&ids, 90).unwrap();
    let c = t.clips.iter().find(|c| right.contains(&c.id)).unwrap();
    assert_eq!(c.transform_at(105.0).x, 90.0);
    t.configure(1920, 1080, 60, 12000).unwrap();
    for c in &t.clips {
        assert_eq!(c.transform_at(180.0).x, 60.0);
    }
    let (mut short, ids) = fixture(MediaKind::Image);
    short.toggle_keyframe(&ids, K::Scale, 30);
    short.toggle_keyframe(&ids, K::Scale, 31);
    let before = short.clone();
    assert!(short.configure(1920, 1080, 1, 12000).is_err());
    assert_eq!(short, before);
}
/// 验证音量仅记录音频、线性增益插值、静音和非法输入均遵守边界。
#[test]
fn volume_is_linear_and_invalid_inputs_do_not_mutate() {
    let (mut t, ids) = fixture(MediaKind::Audio);
    t.toggle_keyframe(&ids, K::Volume, 30);
    t.set_volume_at(&ids, 0.0, 60, true).unwrap();
    assert_eq!(t.clips[0].gain_at(45.0), 0.5);
    assert_eq!(t.clips[0].gain_at(90.0), 0.0);
    let before = t.clone();
    assert!(t.set_volume_at(&ids, f64::NAN, 45, true).is_err());
    assert_eq!(t, before);
    t.toggle_keyframe(&ids, K::Position, 45);
    assert_eq!(t, before);
    t.clips[0].keyframes.push(Keyframe {
        frame: 0,
        kind: K::Volume,
        values: [1.0, 0.0],
        curves: [None; 2],
    });
    assert!(t.validate().is_err());
}
/// 验证动画保存重开精确一致、旧文件缺字段默认空动画，损坏数据明确拒绝。
#[test]
fn project_roundtrip_legacy_and_corruption() {
    let root = std::env::temp_dir().join(format!("dinuosi-keyframes-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&root).unwrap();
    let store = ProjectStore::new(root.join("recent.json"));
    let entry = store.create("动画", &root).unwrap();
    let (mut t, ids) = fixture(MediaKind::Image);
    t.toggle_keyframe(&ids, K::Position, 30);
    t.set_property_at(&ids, Property::X, 200.0, 60, true)
        .unwrap();
    store.save_timeline(&entry, &t).unwrap();
    assert_eq!(store.load_timeline(&entry).unwrap(), t);
    let mut json: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&entry.path).unwrap()).unwrap();
    json["timeline"]["clips"][0]["keyframes"][0]["kind"] = "bad".into();
    std::fs::write(&entry.path, serde_json::to_vec(&json).unwrap()).unwrap();
    assert!(store.load_timeline(&entry).is_err());
    json["timeline"]["clips"][0]
        .as_object_mut()
        .unwrap()
        .remove("keyframes");
    std::fs::write(&entry.path, serde_json::to_vec(&json).unwrap()).unwrap();
    assert!(
        store.load_timeline(&entry).unwrap().clips[0]
            .keyframes
            .is_empty()
    );
    std::fs::remove_dir_all(root).unwrap();
}
/// 用实际 FFmpeg 解码恒定 PCM，验证音量渐变、左右声道一致、跳转和静音后恢复。
#[test]
#[ignore = "需要准备应用配套 FFmpeg 和 ffprobe"]
fn volume_keyframes_change_real_pcm_and_seek() {
    use dinuosi_core::{media, timeline_audio::TimelineAudio};
    let root = std::env::temp_dir().join(format!("dinuosi-volume-keys-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&root).unwrap();
    let path = root.join("constant.wav");
    assert!(
        media::command("ffmpeg")
            .args([
                "-v",
                "error",
                "-f",
                "lavfi",
                "-i",
                "aevalsrc=0.125|0.125:s=48000:d=1",
                "-ac",
                "2",
                "-c:a",
                "pcm_f32le"
            ])
            .arg(&path)
            .status()
            .unwrap()
            .success()
    );
    let source = media::probe(&path).unwrap();
    let mut t = Timeline::default();
    let id = t
        .insert((path, MediaKind::Audio), "恒定".into(), 0, 0, 0..30, 30)
        .unwrap();
    let ids = [id].into();
    t.set_volume(&ids, 0.0).unwrap();
    t.toggle_keyframe(&ids, K::Volume, 0);
    t.set_volume_at(&ids, 1.0, 15, true).unwrap();
    for offset in [0.0, 0.125, 0.5] {
        let samples: Vec<_> = TimelineAudio::start(&t, offset, std::slice::from_ref(&source))
            .unwrap()
            .collect();
        for i in [0, 1200, 6000] {
            let frame = (offset + (i / 2) as f64 / 48000.0) * 30.0;
            let expected = 0.125 * t.clips[0].gain_at(frame) as f32;
            assert!(
                (samples[i] - expected).abs() < 0.00001,
                "offset={offset}, i={i}"
            );
            assert_eq!(samples[i], samples[i + 1]);
        }
    }
    std::fs::remove_dir_all(root).unwrap();
}
/// 用真实 BGRA 合成验证透明度中点，防止只有检查器变化而画面不应用动画。
#[test]
fn composite_evaluates_opacity_keyframes() {
    let (mut t, ids) = fixture(MediaKind::Image);
    t.toggle_keyframe(&ids, K::Opacity, 30);
    t.set_property_at(&ids, Property::Opacity, 0.0, 60, true)
        .unwrap();
    let m = dinuosi_core::media::Media {
        path: "source.png".into(),
        name: "source".into(),
        kind: MediaKind::Image,
        duration: 5.0,
        has_audio: false,
        preview_size: (640, 360),
        source_size: (1920, 1080),
        offline: false,
        thumbnail: Some(std::sync::Arc::new([100, 100, 100, 255].repeat(640 * 360))),
    };
    for (frame, expected) in [(30, 100), (45, 50), (60, 0)] {
        let bytes = dinuosi_core::composite::render(&t, frame, std::slice::from_ref(&m)).unwrap();
        assert_eq!(&bytes[..4], &[expected, expected, expected, 255]);
    }
}
/// 验证整组删除只针对指定素材/帧，保留同帧其他素材和其他帧；最后一帧删除后烘焙静态值。
#[test]
fn remove_marker_deletes_all_its_types_and_respects_locks() {
    let (mut t, ids) = fixture(MediaKind::Image);
    let id = ids.first().unwrap().clone();
    t.toggle_keyframe(&ids, K::Position, 30);
    t.toggle_keyframe(&ids, K::Scale, 30);
    t.set_property_at(&ids, Property::X, 120.0, 60, true)
        .unwrap();
    let other = t
        .insert(
            ("other.png".into(), MediaKind::Image),
            "其他".into(),
            t.main_track(),
            30,
            0..150,
            150,
        )
        .unwrap();
    t.toggle_keyframe(&[other.clone()].into(), K::Position, 30);
    let untouched = t.clips.iter().find(|c| c.id == other).unwrap().clone();
    t.remove_keyframes(&id, 30);
    let c = t.clips.iter().find(|c| c.id == id).unwrap();
    assert_eq!(c.keyframes.len(), 1);
    assert_eq!(c.keyframes[0].kind, K::Position);
    assert_eq!(c.keyframes[0].frame, 30);
    assert_eq!(c.transform.scale, 100.0);
    assert_eq!(t.clips.iter().find(|c| c.id == other).unwrap(), &untouched);
    let track = c.track;
    t.tracks[track].locked = true;
    let before = t.clone();
    t.remove_keyframes(&id, 60);
    t.remove_keyframes("missing", 60);
    t.remove_keyframes(&id, i64::MIN);
    assert_eq!(t, before);
    t.tracks[track].locked = false;
    t.remove_keyframes(&id, 60);
    let c = t.clips.iter().find(|c| c.id == id).unwrap();
    assert!(c.keyframes.is_empty());
    assert_eq!(c.transform.x, 120.0);
    t.validate().unwrap();
}
