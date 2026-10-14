//! 调色动画契约回归：类型隔离、插值、步骤身份、时间编辑与磁盘拒绝路径。
use dinuosi_core::{
    ProjectStore,
    color::{Control as C, Grade, Stage},
    keyframe_curve::Easing,
    keyframes::KeyframeKind as K,
    media::MediaKind,
    timeline::Timeline,
};
use std::collections::BTreeSet;

/// 创建偏移一秒的画面片段，确保所有断言使用项目绝对帧而不是局部帧。
fn fixture() -> (Timeline, BTreeSet<String>) {
    let mut t = Timeline::default();
    let id = t
        .insert(
            ("source.png".into(), MediaKind::Image),
            "图".into(),
            0,
            30,
            0..150,
            150,
        )
        .unwrap();
    (t, [id].into())
}

/// 验证所有合法参数身份往返、独立动画与静态参数、贝塞尔求值、自动关闭及最后一帧烘焙。
#[test]
fn color_animation_reuses_parameter_keyframes() {
    for s in 0..8 {
        for c in C::all() {
            let k = K::Color(s, c);
            assert_eq!(K::parse(&k.name()).unwrap(), k);
        }
    }
    for bad in [
        "color/8/basic/0",
        "color/0/basic/10",
        "color/0/hsl/8/0",
        "color/0/mask/6",
        "color/0/basic/0/extra",
    ] {
        assert!(K::parse(bad).is_err());
    }
    for control in C::all() {
        let (mut t, ids) = fixture();
        if matches!(control, C::PickedHsl(_)) {
            t.clips[0].color.stages[0].picked = Some(dinuosi_core::color::PickedHsl {
                hue: 240.0,
                values: [0.0; 3],
            });
        }
        let kind = K::Color(0, control);
        let base = t.clips[0].color.stages[0].get(control);
        t.toggle_keyframe(&ids, kind, 30);
        let mut grade = t.clips[0].color_at(60.0).into_owned();
        let (min, max) = control.range();
        let target = if base == min { max } else { min };
        grade.stages[0].set(control, target).unwrap();
        t.set_color_at(&ids, &grade, 60, true).unwrap();
        assert_eq!(
            t.clips[0].keyframe_values(kind, 45.0)[0],
            (base + target) / 2.0
        );
        t.validate().unwrap();
    }
    let (mut t, ids) = fixture();
    let k = K::Color(0, C::Basic(0));
    t.toggle_keyframe(&ids, k, 30);
    let mut g = t.clips[0].color_at(60.0).into_owned();
    g.stages[0].basic[0] = 2.0;
    g.stages[0].basic[1] = 30.0;
    t.set_color_at(&ids, &g, 60, true).unwrap();
    assert_eq!(t.clips[0].keyframes.len(), 2);
    assert_eq!(t.clips[0].color_at(45.0).stages[0].basic[..2], [1.0, 30.0]);
    assert_eq!(t.clips[0].color_at(0.0).stages[0].basic[0], 0.0);
    assert_eq!(t.clips[0].color_at(300.0).stages[0].basic[0], 2.0);
    let id = t.clips[0].id.clone();
    t.ease_curve(&id, k, 0, 0, Easing::EaseIn).unwrap();
    let v = t.clips[0].color_at(45.0).stages[0].basic[0];
    assert!(v > 0.0 && v < 1.0);
    let mut g = t.clips[0].color_at(45.0).into_owned();
    g.stages[0].basic[0] += 0.5;
    t.set_color_at(&ids, &g, 45, false).unwrap();
    assert_eq!(t.clips[0].keyframes.len(), 2);
    assert!((t.clips[0].color_at(45.0).stages[0].basic[0] - v - 0.5).abs() < 1e-8);
    t.toggle_keyframe(&ids, k, 60);
    let value = t.clips[0].color_at(30.0).stages[0].basic[0];
    t.toggle_keyframe(&ids, k, 30);
    assert_eq!(t.clips[0].color.stages[0].basic[0], value);
    assert!(t.clips[0].keyframes.is_empty());
    t.validate().unwrap();
    let (mut t, ids) = fixture();
    let min = K::Color(0, C::Qualifier(4));
    let max = K::Color(0, C::Qualifier(5));
    t.clips[0].keyframes = vec![
        dinuosi_core::keyframes::Keyframe {
            frame: 0,
            kind: min,
            values: [0.9, 0.0],
            curves: [None; 2],
        },
        dinuosi_core::keyframes::Keyframe {
            frame: 0,
            kind: max,
            values: [0.1, 0.0],
            curves: [None; 2],
        },
    ];
    let before = t.clips[0].color_at(30.0).stages[0].qualifier.values;
    assert_eq!(&before[4..], &[0.1, 0.9]);
    t.toggle_keyframe(&ids, min, 30);
    assert_eq!(t.clips[0].color_at(30.0).stages[0].qualifier.values, before);
    t.validate().unwrap();
}

/// 验证步骤追加不丢动画、删除重映射、分割和换帧率保持求值；锁定、音频及非法输入无副作用。
#[test]
fn color_animation_preserves_steps_and_timeline_edits() {
    let (mut t, ids) = fixture();
    let id = t.clips[0].id.clone();
    t.toggle_keyframe(&ids, K::Color(0, C::Basic(0)), 30);
    let mut g = Grade::default();
    g.stages.push(Stage::default());
    t.set_color_at(&ids, &g, 30, true).unwrap();
    let k = K::Color(1, C::Basic(4));
    t.toggle_keyframe(&ids, k, 30);
    g.stages[1].basic[4] = -100.0;
    t.set_color_at(&ids, &g, 90, true).unwrap();
    t.reset_color(&id, Some(0), true);
    assert_eq!(t.clips[0].keyframes.len(), 2);
    assert!(
        t.clips[0]
            .keyframes
            .iter()
            .all(|k| k.kind == K::Color(0, C::Basic(4)))
    );
    assert_eq!(t.clips[0].color_at(60.0).stages[0].basic[4], -50.0);
    t.split(&ids, 60).unwrap();
    assert!(
        t.clips
            .iter()
            .all(|c| c.color_at(75.0).stages[0].basic[4] == -75.0)
    );
    t.configure(1920, 1080, 60, 12000).unwrap();
    assert!(
        t.clips
            .iter()
            .all(|c| c.color_at(150.0).stages[0].basic[4] == -75.0)
    );
    let before = t.clone();
    let mut bad = Grade::default();
    bad.stages[0].basic[0] = f64::NAN;
    assert!(t.set_color_at(&ids, &bad, 120, true).is_err());
    assert_eq!(t, before);
    t.clips[0].locked = true;
    let before = t.clone();
    t.toggle_keyframe(&ids, K::Color(0, C::Basic(0)), 90);
    assert_eq!(t, before);
    let audio = t
        .insert(
            ("sound.wav".into(), MediaKind::Audio),
            "声音".into(),
            0,
            0,
            0..150,
            150,
        )
        .unwrap();
    let before = t.clone();
    t.toggle_keyframe(&[audio].into(), K::Color(0, C::Basic(0)), 0);
    assert_eq!(t, before);
    t.validate().unwrap();
}

/// 验证保存/重开完整调色动画、缺失字段旧项目以及越界步骤身份拒绝加载。
#[test]
fn color_animation_roundtrip_and_invalid_stage_rejection() {
    let root = std::env::temp_dir().join(format!("dinuosi-color-keys-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&root).unwrap();
    let store = ProjectStore::new(root.join("recent.json"));
    let entry = store.create("调色动画", &root).unwrap();
    let (mut t, ids) = fixture();
    t.toggle_keyframe(&ids, K::Color(0, C::Basic(2)), 30);
    let mut g = t.clips[0].color_at(90.0).into_owned();
    g.stages[0].basic[2] = 60.0;
    t.set_color_at(&ids, &g, 90, true).unwrap();
    store.save_timeline(&entry, &t).unwrap();
    assert_eq!(store.load_timeline(&entry).unwrap(), t);
    let mut json: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&entry.path).unwrap()).unwrap();
    json["timeline"]["clips"][0]["keyframes"][0]["kind"] = "color/1/basic/2".into();
    std::fs::write(&entry.path, serde_json::to_vec(&json).unwrap()).unwrap();
    assert!(store.load_timeline(&entry).is_err());
    let clip = json["timeline"]["clips"][0].as_object_mut().unwrap();
    clip.remove("keyframes");
    clip.remove("color");
    std::fs::write(&entry.path, serde_json::to_vec(&json).unwrap()).unwrap();
    assert_eq!(
        store.load_timeline(&entry).unwrap().clips[0].color,
        Grade::default()
    );
    std::fs::remove_dir_all(root).unwrap();
}

/// 用曲线编辑入口建立交叉亮度选区，验证单端/双端节点按两种顺序删除及整帧删除都保持当前调色。
/// 删除无关曝光节点不能烘焙或改变亮度后备值；每次删除后必须仍满足项目保存校验。
#[test]
fn deleting_crossed_qualifier_keys_preserves_effective_range() {
    let min = K::Color(0, C::Qualifier(4));
    let max = K::Color(0, C::Qualifier(5));
    let exposure = K::Color(0, C::Basic(0));
    for mode in 0..3 {
        for order in 0..3 {
            let (mut t, ids) = fixture();
            let id = t.clips[0].id.clone();
            let stage = &mut t.clips[0].color.stages[0];
            stage.qualifier.enabled = true;
            stage.basic[0] = 1.0;
            if mode == 0 {
                stage.qualifier.values[4] = 0.9;
            } else if mode == 1 {
                stage.qualifier.values[5] = 0.1;
            }
            for (kind, value) in [(min, 0.9), (max, 0.1)] {
                if (kind == min && mode != 0) || (kind == max && mode != 1) {
                    t.toggle_keyframe(&ids, kind, 30);
                    t.edit_curve_node(&id, kind, 0, 0, 0, value).unwrap();
                }
            }
            t.toggle_keyframe(&ids, exposure, 30);
            let before = t.clips[0].color_at(30.0).into_owned();
            assert_eq!(&before.stages[0].qualifier.values[4..], &[0.1, 0.9]);
            let base = t.clips[0].color.stages[0].qualifier.values;
            t.toggle_keyframe(&ids, exposure, 30);
            assert_eq!(t.clips[0].color_at(30.0).as_ref(), &before);
            assert_eq!(t.clips[0].color.stages[0].qualifier.values, base);
            if order == 2 {
                t.remove_keyframes(&id, 30);
            } else {
                for kind in if order == 0 { [min, max] } else { [max, min] } {
                    if t.clips[0].keyframes.iter().any(|k| k.kind == kind) {
                        t.toggle_keyframe(&ids, kind, 30);
                        assert_eq!(t.clips[0].color_at(30.0).as_ref(), &before);
                        t.validate().unwrap();
                    }
                }
            }
            assert!(t.clips[0].keyframes.is_empty());
            assert_eq!(t.clips[0].color, before);
            t.validate().unwrap();
        }
    }
}
