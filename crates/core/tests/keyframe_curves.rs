//! 参数曲线的数值、事务和兼容回归，独立参考值防止编辑器与求值器共同出错。
use dinuosi_core::{
    ProjectStore,
    keyframe_curve::{Easing, KeyframeCurve},
    keyframes::KeyframeKind as K,
    media::MediaKind,
    timeline::{Property, Timeline},
};

/// 构造两个缩放节点和可选中间节点，无需解码媒体；返回唯一片段身份。
fn fixture(middle: bool) -> (Timeline, String) {
    let mut t = Timeline::default();
    let id = t
        .insert(
            ("image.png".into(), MediaKind::Image),
            "图像".into(),
            0,
            30,
            0..150,
            150,
        )
        .unwrap();
    let ids = [id.clone()].into();
    t.toggle_keyframe(&ids, K::Scale, 30);
    t.set_property_at(&ids, Property::Scale, 200.0, 90, true)
        .unwrap();
    if middle {
        t.set_property_at(&ids, Property::Scale, 130.0, 60, true)
            .unwrap();
    }
    (t, id)
}

/// 独立 CSS 贝塞尔参考值验证时间反解、四种预设、端点保持和 X/Y 分量隔离。
#[test]
fn easing_uses_time_inverse_and_keeps_channels_independent() {
    let (mut t, id) = fixture(false);
    for (easing, expected) in [
        (Easing::Linear, 125.0),
        (Easing::EaseIn, 109.346465),
        (Easing::EaseOut, 137.813813),
        (Easing::EaseInOut, 112.916193),
    ] {
        t.ease_curve(&id, K::Scale, 0, 0, easing).unwrap();
        assert!((t.clips[0].transform_at(45.0).scale - expected).abs() < 0.0001);
        assert_eq!(t.clips[0].transform_at(0.0).scale, 100.0);
        assert_eq!(t.clips[0].transform_at(120.0).scale, 200.0);
    }
    let ids = [id.clone()].into();
    t.toggle_keyframe(&ids, K::Position, 30);
    t.set_property_at(&ids, Property::X, 100.0, 90, true)
        .unwrap();
    t.set_property_at(&ids, Property::Y, 200.0, 90, true)
        .unwrap();
    t.ease_curve(&id, K::Position, 1, 0, Easing::EaseInOut)
        .unwrap();
    let p = t.clips[0].transform_at(45.0);
    assert_eq!(p.x, 25.0);
    assert!((p.y - 25.832386).abs() < 0.0001);
}

/// 自由手柄互不影响，联动按左右真实间距匹配斜率；过冲求值夹紧，非法写入原子失败。
#[test]
fn handles_and_node_edits_respect_locks_collisions_and_ranges() {
    let (mut t, id) = fixture(true);
    t.link_curve_handles(&id, K::Scale, 0, 30, false).unwrap();
    let input = t.clips[0].curve_handle(K::Scale, 0, 30, true);
    t.edit_curve_handle(&id, K::Scale, 0, 30, false, [0.4, 40.0])
        .unwrap();
    assert_eq!(t.clips[0].curve_handle(K::Scale, 0, 30, true), input);
    t.edit_curve_node(&id, K::Scale, 0, 30, 20, 150.0).unwrap();
    t.link_curve_handles(&id, K::Scale, 0, 20, true).unwrap();
    t.edit_curve_handle(&id, K::Scale, 0, 20, false, [0.25, 50.0])
        .unwrap();
    let a = t.clips[0].curve_handle(K::Scale, 0, 20, true).unwrap();
    let b = t.clips[0].curve_handle(K::Scale, 0, 20, false).unwrap();
    assert!((a[1] / (-20.0 * a[0]) - b[1] / (40.0 * b[0])).abs() < 1e-8);
    let before = t.clone();
    for (target, value) in [
        (60, 200.0),
        (-1, 200.0),
        (150, 200.0),
        (21, f64::NAN),
        (21, 501.0),
    ] {
        assert!(
            t.edit_curve_node(&id, K::Scale, 0, 20, target, value)
                .is_err()
        );
        assert_eq!(t, before);
    }
    for h in [
        [-0.1, 1.0],
        [1.1, 1.0],
        [0.3, f64::INFINITY],
        [f64::NAN, 0.0],
    ] {
        assert!(t.edit_curve_handle(&id, K::Scale, 0, 20, false, h).is_err());
        assert_eq!(t, before);
    }
    t.clips[0].locked = true;
    let locked = t.clone();
    assert!(t.ease_curve(&id, K::Scale, 0, 20, Easing::EaseIn).is_err());
    assert!(t.edit_curve_node(&id, K::Scale, 0, 20, 21, 160.0).is_err());
    assert_eq!(t, locked);
    t.clips[0].locked = false;
    t.tracks[0].locked = true;
    assert!(t.link_curve_handles(&id, K::Scale, 0, 20, false).is_err());
    t.tracks[0].locked = false;
    t.edit_curve_handle(&id, K::Scale, 0, 20, false, [0.4, 1000.0])
        .unwrap();
    assert!(t.clips[0].transform_at(70.0).scale <= 500.0);
    t.validate().unwrap();
}

/// 非线性动画在裁剪、分割和换帧率后保留同一秒上的结果，范围外锚点及其切线随片段复制。
#[test]
fn time_edits_preserve_bezier_anchors() {
    let (mut t, id) = fixture(false);
    t.ease_curve(&id, K::Scale, 0, 0, Easing::EaseInOut)
        .unwrap();
    let expected = t.clips[0].transform_at(75.0).scale;
    t.trim(&id, true, 15).unwrap();
    assert!((t.clips[0].transform_at(75.0).scale - expected).abs() < 1e-8);
    let right = t.split(&[id].into(), 60).unwrap();
    let c = t.clips.iter().find(|c| right.contains(&c.id)).unwrap();
    assert!((c.transform_at(75.0).scale - expected).abs() < 1e-8);
    t.configure(1920, 1080, 60, 12000).unwrap();
    assert!(
        (t.clips
            .iter()
            .find(|c| right.contains(&c.id))
            .unwrap()
            .transform_at(150.0)
            .scale
            - expected)
            .abs()
            < 1e-8
    );
}

/// 在非线性区间插入当前采样值时，拆分后的两段逐帧保持原动画，自动改值也继承平滑切线。
#[test]
fn adding_a_key_splits_the_curve_without_changing_motion() {
    let (mut t, id) = fixture(false);
    t.ease_curve(&id, K::Scale, 0, 0, Easing::EaseInOut)
        .unwrap();
    let original = t.clone();
    t.toggle_keyframe(&[id.clone()].into(), K::Scale, 47);
    assert_eq!(t.clips[0].keyframes.len(), 3);
    for quarter in 120..=360 {
        let frame = quarter as f64 / 4.0;
        assert!(
            (t.clips[0].transform_at(frame).scale - original.clips[0].transform_at(frame).scale)
                .abs()
                < 1e-7
        );
    }
    t.validate().unwrap();
    let mut automatic = original;
    automatic
        .set_property_at(&[id].into(), Property::Scale, 160.0, 47, true)
        .unwrap();
    assert_eq!(automatic.clips[0].transform_at(47.0).scale, 160.0);
    assert!(
        automatic.clips[0]
            .keyframes
            .iter()
            .find(|k| k.frame == 17)
            .unwrap()
            .curves[0]
            .is_some()
    );
    automatic.validate().unwrap();
}

/// 曲线存储精确往返，旧线性项目无曲线字段时仍能打开；损坏切线与伪造分量必须明确拒绝。
#[test]
fn storage_roundtrip_legacy_and_invalid_curve() {
    let root = std::env::temp_dir().join(format!("dinuosi-curves-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&root).unwrap();
    let store = ProjectStore::new(root.join("recent.json"));
    let entry = store.create("曲线", &root).unwrap();
    let (mut t, id) = fixture(false);
    t.ease_curve(&id, K::Scale, 0, 0, Easing::EaseInOut)
        .unwrap();
    store.save_timeline(&entry, &t).unwrap();
    assert_eq!(store.load_timeline(&entry).unwrap(), t);
    let mut json: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&entry.path).unwrap()).unwrap();
    for key in json["timeline"]["clips"][0]["keyframes"]
        .as_array_mut()
        .unwrap()
    {
        for curve in key["curves"].as_array_mut().unwrap() {
            if let Some(curve) = curve.as_object_mut() {
                curve.remove("value_offset");
            }
        }
    }
    std::fs::write(&entry.path, serde_json::to_vec(&json).unwrap()).unwrap();
    assert_eq!(
        store.load_timeline(&entry).unwrap(),
        t,
        "旧切线对象缺少端点差仍兼容"
    );
    for invalid in [1.0, 1001.0] {
        json["timeline"]["clips"][0]["keyframes"][0]["curves"][0]["value_offset"] = invalid.into();
        std::fs::write(&entry.path, serde_json::to_vec(&json).unwrap()).unwrap();
        assert!(store.load_timeline(&entry).is_err());
    }
    json["timeline"]["clips"][0]["keyframes"][0]["curves"][0]["value_offset"] = 0.0.into();
    json["timeline"]["clips"][0]["keyframes"][0]["curves"][0]["outgoing"][0] = 2.0.into();
    std::fs::write(&entry.path, serde_json::to_vec(&json).unwrap()).unwrap();
    assert!(store.load_timeline(&entry).is_err());
    for key in json["timeline"]["clips"][0]["keyframes"]
        .as_array_mut()
        .unwrap()
    {
        key.as_object_mut().unwrap().remove("curves");
    }
    std::fs::write(&entry.path, serde_json::to_vec(&json).unwrap()).unwrap();
    let old = store.load_timeline(&entry).unwrap();
    assert_eq!(old.clips[0].transform_at(45.0).scale, 125.0);
    t.clips[0].keyframes[0].curves[1] = Some(KeyframeCurve::default());
    assert!(t.validate().is_err());
    std::fs::remove_dir_all(root).unwrap();
}

/// 在正/负过冲区反复插入合法采样值，逐四分之一帧保持动画，保存重开仍保留原始曲线几何。
#[test]
fn inserting_clamped_keys_preserves_overshoot_and_storage() {
    let root =
        std::env::temp_dir().join(format!("dinuosi-curve-overshoot-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&root).unwrap();
    let store = ProjectStore::new(root.join("recent.json"));
    let entry = store.create("过冲", &root).unwrap();
    for (base, handle) in [(0.0, 200.0), (100.0, -200.0)] {
        let (mut t, id) = fixture(false);
        let ids = [id.clone()].into();
        t.set_property(&ids, Property::Opacity, base).unwrap();
        t.toggle_keyframe(&ids, K::Opacity, 30);
        t.toggle_keyframe(&ids, K::Opacity, 90);
        t.edit_curve_handle(&id, K::Opacity, 0, 0, false, [1.0 / 3.0, handle])
            .unwrap();
        t.edit_curve_handle(&id, K::Opacity, 0, 60, true, [1.0 / 3.0, handle])
            .unwrap();
        let original = t.clone();
        for frame in [60, 45, 75, 53] {
            t.toggle_keyframe(&ids, K::Opacity, frame);
            t.validate().unwrap();
            for quarter in 120..=360 {
                let frame = quarter as f64 / 4.0;
                assert!(
                    (t.clips[0].transform_at(frame).opacity
                        - original.clips[0].transform_at(frame).opacity)
                        .abs()
                        < 1e-7,
                    "frame={frame}"
                );
            }
        }
        store.save_timeline(&entry, &t).unwrap();
        let reopened = store.load_timeline(&entry).unwrap();
        for quarter in 120..=360 {
            let frame = quarter as f64 / 4.0;
            assert!(
                (reopened.clips[0].transform_at(frame).opacity
                    - original.clips[0].transform_at(frame).opacity)
                    .abs()
                    < 1e-7
            );
        }
        t.edit_curve_node(&id, K::Opacity, 0, 30, 30, 50.0).unwrap();
        assert_eq!(t.clips[0].transform_at(60.0).opacity, 50.0);
        assert_eq!(
            t.clips[0]
                .keyframes
                .iter()
                .find(|k| k.kind == K::Opacity && k.frame == 30)
                .unwrap()
                .curves[0]
                .unwrap()
                .value_offset,
            0.0
        );
        t.validate().unwrap();
    }
    std::fs::remove_dir_all(root).unwrap();
}

/// 移动位置节点时间时，X/Y 与受影响相邻节点仍保持联动斜率；自由切线及无效写入保持不变。
#[test]
fn retiming_nodes_keeps_linked_tangents_for_both_components() {
    let (mut t, id) = fixture(false);
    let ids = [id.clone()].into();
    for frame in [30, 60, 90, 120] {
        t.toggle_keyframe(&ids, K::Position, frame);
    }
    for frame in [30, 60] {
        for component in [0, 1] {
            t.link_curve_handles(&id, K::Position, component, frame, true)
                .unwrap();
            t.edit_curve_handle(&id, K::Position, component, frame, false, [0.25, 30.0])
                .unwrap();
        }
    }
    t.edit_curve_node(&id, K::Position, 0, 30, 20, 10.0)
        .unwrap();
    let c = &t.clips[0];
    for frame in [20, 60] {
        for component in [0, 1] {
            let a = c.curve_handle(K::Position, component, frame, true).unwrap();
            let b = c
                .curve_handle(K::Position, component, frame, false)
                .unwrap();
            let left = (frame - c.curve_neighbor(K::Position, frame, true).unwrap().frame) as f64;
            let right = (c.curve_neighbor(K::Position, frame, false).unwrap().frame - frame) as f64;
            assert!((a[1] / (-left * a[0]) - b[1] / (right * b[0])).abs() < 1e-8);
        }
    }
    // 极陡联动手柄触及纵向上限时缩短长度，不能靠夹紧差值破坏斜率。
    t.edit_curve_handle(&id, K::Position, 0, 20, false, [0.0001, 400_000.0])
        .unwrap();
    let a = t.clips[0].curve_handle(K::Position, 0, 20, true).unwrap();
    let b = t.clips[0].curve_handle(K::Position, 0, 20, false).unwrap();
    let slope = b[1] / (40.0 * b[0]);
    assert!((a[1] / (-20.0 * a[0]) / slope - 1.0).abs() < 1e-8);
    for handle in [[0.0, 20.0], [1e-310, 20.0], [1e-310, 0.0]] {
        t.edit_curve_handle(&id, K::Position, 0, 20, false, handle)
            .unwrap();
        t.validate().unwrap();
    }
    t.validate().unwrap();
}
