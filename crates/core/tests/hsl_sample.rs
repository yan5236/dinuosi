//! 取样色组的独立渲染、动画、存储兼容和非法输入回归。
use dinuosi_core::{
    ProjectStore,
    color::{Control, Grade, PickedHsl},
    color_render::apply,
    keyframes::KeyframeKind as K,
    media::MediaKind,
    timeline::Timeline,
};

/// 验证新增中性色组不改像素；蓝色去饱和只处理蓝色，原八组保持独立，非法色相拒绝。
#[test]
fn sampled_hsl_is_independent_and_validated() {
    let source = [0, 0, 255, 255, 0, 255, 0, 255, 255, 0, 0, 255];
    let mut grade = Grade::default();
    grade.stages[0].picked = Some(PickedHsl {
        hue: 240.0,
        values: [0.0; 3],
    });
    assert_eq!(
        apply(&source, (3, 1), (3, 1), &grade).unwrap().as_ref(),
        source
    );
    grade.stages[0].picked.as_mut().unwrap().values[1] = -100.0;
    let output = apply(&source, (3, 1), (3, 1), &grade).unwrap();
    assert_eq!(&output[..8], &source[..8]);
    assert_eq!(output[8], output[9]);
    assert_eq!(output[9], output[10]);
    assert_eq!(grade.stages[0].hsl, [[0.0; 3]; 8]);
    grade.stages[0].picked.as_mut().unwrap().hue = f64::NAN;
    assert!(grade.validate().is_err());
    grade.stages[0].picked.as_mut().unwrap().hue = 360.0;
    assert!(grade.validate().is_err());
}
/// 验证取样色组支持独立动画、重新取样保留参数，缺色组节点不能创建，重开与旧字段兼容。
#[test]
fn sampled_hsl_keyframes_and_storage() {
    let mut t = Timeline::default();
    let id = t
        .insert(
            ("hsl.png".into(), MediaKind::Image),
            "图".into(),
            0,
            0,
            0..90,
            90,
        )
        .unwrap();
    let ids = [id].into();
    let kind = K::Color(0, Control::PickedHsl(1));
    t.toggle_keyframe(&ids, kind, 0);
    assert!(t.clips[0].keyframes.is_empty());
    let mut grade = Grade::default();
    grade.stages[0].picked = Some(PickedHsl {
        hue: 240.0,
        values: [0.0; 3],
    });
    t.set_color_at(&ids, &grade, 0, true).unwrap();
    t.toggle_keyframe(&ids, kind, 0);
    grade.stages[0].picked.as_mut().unwrap().values[1] = -100.0;
    t.set_color_at(&ids, &grade, 30, true).unwrap();
    assert_eq!(
        t.clips[0].color_at(15.0).stages[0]
            .picked
            .as_ref()
            .unwrap()
            .values[1],
        -50.0
    );
    grade.stages[0].picked.as_mut().unwrap().hue = 120.0;
    t.set_color_at(&ids, &grade, 30, true).unwrap();
    assert_eq!(t.clips[0].keyframes.len(), 2);
    let root = std::env::temp_dir().join(format!("dinuosi-hsl-sample-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&root).unwrap();
    let store = ProjectStore::new(root.join("recent.json"));
    let entry = store.create("取样", &root).unwrap();
    store.save_timeline(&entry, &t).unwrap();
    assert_eq!(store.load_timeline(&entry).unwrap(), t);
    let mut json: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&entry.path).unwrap()).unwrap();
    json["timeline"]["clips"][0]["color"]["stages"][0]
        .as_object_mut()
        .unwrap()
        .remove("picked");
    std::fs::write(&entry.path, serde_json::to_vec(&json).unwrap()).unwrap();
    assert!(store.load_timeline(&entry).is_err());
    json["timeline"]["clips"][0]
        .as_object_mut()
        .unwrap()
        .remove("keyframes");
    std::fs::write(&entry.path, serde_json::to_vec(&json).unwrap()).unwrap();
    assert!(
        store.load_timeline(&entry).unwrap().clips[0].color.stages[0]
            .picked
            .is_none()
    );
    std::fs::remove_dir_all(root).unwrap();
}
