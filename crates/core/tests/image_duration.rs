use dinuosi_core::{
    ProjectStore, composite,
    media::{FRAME_HEIGHT, FRAME_WIDTH, Media, MediaKind},
    timeline::{History, Timeline},
};

/// 图片可越过默认五秒向左右延长；验证零点、最短一帧、长时预览、历史、落盘和帧率换算。
/// 同时确保视频与音频仍受源入点及源帧数约束，锁轨拒绝裁边且不产生部分修改。
#[test]
fn image_duration_is_independent_of_source_frames() {
    let mut timeline = Timeline::default();
    let id = timeline
        .insert(
            ("still.png".into(), MediaKind::Image),
            "图片".into(),
            0,
            300,
            0..150,
            150,
        )
        .unwrap();
    let before = timeline.clone();
    timeline.trim(&id, false, 30 * 3600).unwrap();
    timeline.trim(&id, true, -600).unwrap();
    assert_eq!(timeline.clips[0].start, 0);
    assert_eq!(timeline.clips[0].source_in, 0);
    assert_eq!(timeline.clips[0].duration, 108450);
    assert_eq!(timeline.clips[0].source_frames, 150);
    timeline.validate().unwrap();
    let extended = timeline.clone();
    let mut history = History::default();
    history.record(before.clone(), &timeline);
    history.step(&mut timeline, false);
    assert_eq!(timeline, before);
    history.step(&mut timeline, true);
    assert_eq!(timeline, extended);

    let media = Media {
        path: "still.png".into(),
        name: "图片".into(),
        kind: MediaKind::Image,
        duration: 5.0,
        has_audio: false,
        preview_size: (FRAME_WIDTH, FRAME_HEIGHT),
        source_size: (1920, 1080),
        offline: false,
        thumbnail: Some(std::sync::Arc::new(
            [60, 80, 160, 255].repeat((FRAME_WIDTH * FRAME_HEIGHT) as usize),
        )),
    };
    let pixels = composite::render(&timeline, timeline.end() - 1, &[media]).unwrap();
    assert_eq!(&pixels[..4], &[60, 80, 160, 255]);
    let root = std::env::temp_dir().join(format!("dinuosi-still-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&root).unwrap();
    let store = ProjectStore::new(root.join("recent.json"));
    let entry = store.create("图片延长", &root).unwrap();
    store.save_timeline(&entry, &timeline).unwrap();
    assert_eq!(store.load_timeline(&entry).unwrap(), timeline);
    timeline.configure(1920, 1080, 60, 12000).unwrap();
    assert_eq!(timeline.clips[0].duration, 216900);
    store.save_timeline(&entry, &timeline).unwrap();
    assert_eq!(store.load_timeline(&entry).unwrap(), timeline);
    std::fs::remove_dir_all(root).unwrap();

    timeline.trim(&id, true, i64::MAX).unwrap();
    assert_eq!(timeline.clips[0].duration, 1);
    timeline.trim(&id, true, i64::MIN).unwrap();
    assert_eq!(timeline.clips[0].start, 0);
    assert_eq!(timeline.clips[0].source_in, 0);
    timeline.trim(&id, false, i64::MAX).unwrap();
    assert_eq!(timeline.clips[0].duration, 86400 * 60);
    timeline.validate().unwrap();
    let mut boundary = timeline.clone();
    boundary.clips[0].start = 86400 * 60;
    boundary.trim(&id, true, i64::MAX).unwrap();
    assert_eq!(boundary.clips[0].start, 86400 * 60);
    assert_eq!(boundary.clips[0].duration, 86400 * 60);
    boundary.validate().unwrap();
    timeline.trim(&id, false, i64::MIN).unwrap();
    assert_eq!(timeline.clips[0].duration, 1);
    timeline.clips[0].duration = 86400 * 60 + 1;
    assert!(timeline.validate().is_err());

    for kind in [MediaKind::Video, MediaKind::Audio] {
        let mut timed = Timeline::default();
        let id = timed
            .insert(("timed".into(), kind), "源".into(), 0, 300, 30..90, 150)
            .unwrap();
        timed.trim(&id, false, i64::MAX).unwrap();
        assert_eq!(timed.clips[0].duration, 120);
        timed.trim(&id, true, i64::MIN).unwrap();
        assert_eq!((timed.clips[0].start, timed.clips[0].source_in), (270, 0));
        assert_eq!(timed.clips[0].duration, 150);
        timed.validate().unwrap();
        timed.tracks[timed.clips[0].track].locked = true;
        let before = timed.clone();
        assert!(timed.trim(&id, false, -30).is_err());
        assert_eq!(timed, before);
        timed.clips[0].duration += 1;
        assert!(timed.validate().is_err());
    }
}
