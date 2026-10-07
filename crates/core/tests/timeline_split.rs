use dinuosi_core::{
    ProjectStore,
    media::MediaKind,
    timeline::{History, Timeline, TrackKind},
};
use std::collections::BTreeSet;

/// 验证帧精确多轨切割、音视频入点连续、长图片、属性复制、边界/锁轨、原子失败及保存历史。
#[test]
fn split_preserves_sources_properties_and_transaction_boundaries() {
    let mut model = Timeline::default();
    let video = model
        .insert(
            ("video.mp4".into(), MediaKind::Video),
            "视频".into(),
            0,
            30,
            15..135,
            150,
        )
        .unwrap();
    model.create_track(0, TrackKind::Audio).unwrap();
    let audio = model
        .insert(
            ("audio.wav".into(), MediaKind::Audio),
            "音频".into(),
            0,
            30,
            20..140,
            150,
        )
        .unwrap();
    model.create_track(0, TrackKind::Media).unwrap();
    let image = model
        .insert(
            ("image.png".into(), MediaKind::Image),
            "图片".into(),
            0,
            30,
            0..150,
            150,
        )
        .unwrap();
    model.trim(&image, false, 900).unwrap();
    model.clips[0].transform.rotation = 17.0;
    model.clips[1].gain = 0.4;
    let ids = BTreeSet::from([video.clone(), audio.clone(), image.clone()]);
    let before = model.clone();
    let rights = model.split(&ids, 90).unwrap();
    assert_eq!(rights.len(), 3);
    assert_eq!(model.end(), before.end());
    for original in &before.clips {
        let left = model.clips.iter().find(|c| c.id == original.id).unwrap();
        let right = model
            .clips
            .iter()
            .find(|c| rights.contains(&c.id) && c.track == left.track)
            .unwrap();
        assert_eq!(
            (left.start, left.duration, right.start, right.duration),
            (30, 60, 90, original.duration - 60)
        );
        assert_eq!(
            right.source_in,
            if original.kind == MediaKind::Image {
                0
            } else {
                original.source_in + 60
            }
        );
        assert_eq!(right.source, original.source);
        assert_eq!(right.name, original.name);
        assert_eq!(right.source_frames, original.source_frames);
        assert_eq!(right.transform, original.transform);
        assert_eq!(right.gain, original.gain);
    }
    model.validate().unwrap();
    let split = model.clone();
    let mut history = History::default();
    history.record(before.clone(), &model);
    history.step(&mut model, false);
    assert_eq!(model, before);
    history.step(&mut model, true);
    assert_eq!(model, split);
    assert!(model.split(&ids, 30).unwrap().is_empty());
    assert!(model.split(&ids, 90).unwrap().is_empty());
    assert!(model.split(&ids, -1).unwrap().is_empty());
    assert!(model.split(&BTreeSet::new(), 60).unwrap().is_empty());
    assert_eq!(model, split);
    model.tracks[0].locked = true;
    let locked = model.clone();
    assert!(
        model
            .split(&BTreeSet::from([image.clone()]), 60)
            .unwrap()
            .is_empty()
    );
    assert_eq!(model, locked);
    model.tracks[0].locked = false;
    assert_eq!(model.split(&BTreeSet::from([video]), 31).unwrap().len(), 1);
    assert!(model.clips.iter().any(|c| c.duration == 1));
    // 延长图片超过默认源帧数仍能切割且保存为有效模型。
    let image_right = model
        .clips
        .iter()
        .find(|c| c.kind == MediaKind::Image && c.start == 90)
        .unwrap()
        .id
        .clone();
    assert_eq!(
        model
            .split(&BTreeSet::from([image_right]), 600)
            .unwrap()
            .len(),
        1
    );
    model.validate().unwrap();
    let root = std::env::temp_dir().join(format!("dinuosi-split-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&root).unwrap();
    let store = ProjectStore::new(root.join("recent.json"));
    let entry = store.create("切割验证", &root).unwrap();
    store.save_timeline(&entry, &model).unwrap();
    assert_eq!(store.load_timeline(&entry).unwrap(), model);
    std::fs::remove_dir_all(root).unwrap();

    // 右段起点超过现有一天上限时整组失败，不能留下已切开的其他片段。
    let mut invalid = before;
    invalid.clips[2].start = 86400 * 30;
    let before = invalid.clone();
    assert!(invalid.split(&ids, 86400 * 30 + 1).is_err());
    assert_eq!(invalid, before);
}
