//! 关联身份、裁边、切割和项目兼容性回归，无需外部媒体工具。
use dinuosi_core::{
    ProjectStore,
    media::MediaKind,
    timeline::{History, Timeline},
};
use std::collections::BTreeSet;

/// 创建两个有时间偏移和不同可延伸范围的跨轨片段，用于检查关联不对齐及共同裁边边界。
fn pair() -> (Timeline, BTreeSet<String>) {
    let mut t = Timeline::default();
    let a = t
        .insert(
            ("linked.png".into(), MediaKind::Image),
            "画面".into(),
            0,
            60,
            0..120,
            120,
        )
        .unwrap();
    let b = t
        .insert(
            ("linked.wav".into(), MediaKind::Audio),
            "声音".into(),
            0,
            90,
            30..150,
            180,
        )
        .unwrap();
    (t, [a, b].into())
}

/// 关联仅改身份；重复关联、锁定、未知身份无操作，合并旧组与解除整组不留下孤立成员。
#[test]
fn linking_merging_unlinking_and_locks() {
    let (mut t, ids) = pair();
    let before = t.clone();
    assert!(!t.link_clips(&BTreeSet::new(), false));
    assert!(!t.link_clips(&[ids.first().unwrap().clone()].into(), false));
    assert!(!t.link_clips(
        &[ids.first().unwrap().clone(), "missing".into()].into(),
        false
    ));
    assert_eq!(t, before);
    assert!(t.link_clips(&ids, false));
    assert_eq!(t.clips[0].link, t.clips[1].link);
    let mut restored = t.clone();
    for c in &mut restored.clips {
        c.link = None;
    }
    assert_eq!(restored, before);
    assert!(!t.link_clips(&ids, false));
    let one = [ids.first().unwrap().clone()].into();
    assert_eq!(t.linked_ids(&one), ids);
    t.clips[1].locked = true;
    let locked = t.clone();
    assert!(!t.link_clips(&one, true));
    assert_eq!(t, locked);
    t.clips[1].locked = false;
    let extra = t
        .insert(
            ("extra.png".into(), MediaKind::Image),
            "另一个画面".into(),
            t.main_track(),
            240,
            0..60,
            60,
        )
        .unwrap();
    assert!(t.link_clips(&[ids.first().unwrap().clone(), extra.clone()].into(), false));
    assert_eq!(t.linked_ids(&[extra.clone()].into()).len(), 3);
    assert!(t.link_clips(&[extra].into(), true));
    assert!(t.clips.iter().all(|c| c.link.is_none()));
    t.validate().unwrap();
}

/// 联动移动保留偏移；左右裁边按共同源范围夹紧，Alt 单段裁边保留关联，锁定时原子拒绝。
#[test]
fn grouped_movement_and_trim_keep_relative_offsets() {
    let (mut t, ids) = pair();
    t.link_clips(&ids, false);
    t.move_clips(&ids, -1000, 0).unwrap();
    assert_eq!((t.clips[0].start, t.clips[1].start), (0, 30));
    t.move_clips(&ids, 60, 0).unwrap();
    t.trim_clips(&ids, true, -1000).unwrap();
    assert_eq!((t.clips[0].start, t.clips[1].start), (30, 60));
    assert_eq!((t.clips[0].duration, t.clips[1].duration), (150, 150));
    t.trim_clips(&ids, false, 1000).unwrap();
    assert_eq!((t.clips[0].duration, t.clips[1].duration), (180, 180));
    t.trim_clips(&ids, false, i64::MIN).unwrap();
    assert!(t.clips.iter().all(|c| c.duration == 1));
    t.trim_clips(&ids, false, 100).unwrap();
    let audio = t.clips[1].clone();
    t.trim_clips(&[t.clips[0].id.clone()].into(), true, 10)
        .unwrap();
    assert_eq!(t.clips[1], audio);
    t.clips[1].locked = true;
    let locked = t.clone();
    assert!(t.trim_clips(&ids, true, 20).is_err());
    assert_eq!(t, locked);
    t.clips[1].locked = false;
    t.tracks[t.clips[1].track].locked = true;
    let locked = t.clone();
    assert!(t.trim_clips(&ids, false, 20).is_err());
    assert_eq!(t, locked);
}

/// 成组切割生成左右独立关联，Alt 单段切割的右段独立；删除只剩一段时自动清理关联。
#[test]
fn splits_form_separate_left_and_right_links() {
    let (mut t, ids) = pair();
    t.link_clips(&ids, false);
    let old_link = t.clips[0].link.clone();
    let mut solo = t.clone();
    let solo_right = solo.split(&[solo.clips[0].id.clone()].into(), 120).unwrap();
    assert!(
        solo.clips
            .iter()
            .find(|c| solo_right.contains(&c.id))
            .unwrap()
            .link
            .is_none()
    );
    assert_eq!(solo.linked_ids(&ids), ids);
    let rights = t.split(&ids, 120).unwrap();
    assert_eq!(rights.len(), 2);
    assert_eq!(
        t.linked_ids(&[rights.first().unwrap().clone()].into()),
        rights
    );
    assert_eq!(t.linked_ids(&[ids.first().unwrap().clone()].into()), ids);
    assert_ne!(
        t.clips
            .iter()
            .find(|c| rights.contains(&c.id))
            .unwrap()
            .link,
        old_link
    );
    t.delete(&[rights.first().unwrap().clone()].into());
    assert!(
        t.clips
            .iter()
            .find(|c| rights.contains(&c.id))
            .unwrap()
            .link
            .is_none()
    );
    t.validate().unwrap();
}

/// 后移插入同时移动后续片段的关联音频，碰到跨轨锁定成员时整次回滚。
#[test]
fn ripple_preserves_links_and_rejects_locked_peer() {
    let (mut t, ids) = pair();
    t.link_clips(&ids, false);
    t.insert(
        ("lead.png".into(), MediaKind::Image),
        "前段".into(),
        t.main_track(),
        0,
        0..60,
        60,
    )
    .unwrap();
    let before = t.clone();
    let track = t.main_track();
    t.tracks[0].locked = true;
    let locked = t.clone();
    assert!(
        t.insert_ripple(
            ("insert.png".into(), MediaKind::Image),
            "插入".into(),
            track,
            60,
            0..30,
            30
        )
        .is_err()
    );
    assert_eq!(t, locked);
    t = before;
    t.insert_ripple(
        ("insert.png".into(), MediaKind::Image),
        "插入".into(),
        track,
        60,
        0..30,
        30,
    )
    .unwrap();
    assert_eq!((t.clips[0].start, t.clips[1].start), (90, 120));
    t.validate().unwrap();
}

/// 关联跨越插入边界时，新素材或单段移动均原子拒绝，不能把插入片段自动分到上方轨道。
#[test]
fn ripple_rejects_linked_members_covering_insertion() {
    let mut t = Timeline::default();
    let a = t
        .insert(
            ("a.png".into(), MediaKind::Image),
            "A".into(),
            0,
            0,
            0..60,
            60,
        )
        .unwrap();
    let b = t
        .insert(
            ("b.png".into(), MediaKind::Image),
            "B".into(),
            0,
            60,
            0..60,
            60,
        )
        .unwrap();
    assert!(t.link_clips(&[a, b].into(), false));
    let track = t.main_track();
    assert_eq!(t.insertion_frame(track, 60, 0, &BTreeSet::new()), Some(60));
    let before = t.clone();
    assert_eq!(
        t.insert_ripple(
            ("insert.png".into(), MediaKind::Image),
            "插入".into(),
            track,
            60,
            0..30,
            30
        ),
        Err("同一轨道的片段不能重叠".into())
    );
    assert_eq!(t, before);
    let moving = t
        .insert(
            ("moving.png".into(), MediaKind::Image),
            "移动".into(),
            track,
            180,
            0..30,
            30,
        )
        .unwrap();
    let before = t.clone();
    assert_eq!(
        t.move_clip_ripple(&moving, track, 60),
        Err("同一轨道的片段不能重叠".into())
    );
    assert_eq!(t, before);
}

/// 关联可保存、重开和撤销重做；旧文件缺字段仍可读取，损坏身份与单成员组被拒绝。
#[test]
fn persistence_history_legacy_and_invalid_links() {
    let root = std::env::temp_dir().join(format!("dinuosi-links-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&root).unwrap();
    let store = ProjectStore::new(root.join("recent.json"));
    let entry = store.create("关联验证", &root).unwrap();
    let (mut t, ids) = pair();
    let before = t.clone();
    t.link_clips(&ids, false);
    let mut history = History::default();
    history.record(before.clone(), &t);
    history.step(&mut t, false);
    assert_eq!(t, before);
    history.step(&mut t, true);
    store.save_timeline(&entry, &t).unwrap();
    assert_eq!(store.load_timeline(&entry).unwrap(), t);
    let saved: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&entry.path).unwrap()).unwrap();
    let mut bad = saved.clone();
    bad["timeline"]["clips"][0]["link"] = "invalid".into();
    std::fs::write(&entry.path, serde_json::to_vec(&bad).unwrap()).unwrap();
    assert!(store.load_timeline(&entry).is_err());
    let mut bad = saved.clone();
    bad["timeline"]["clips"][0]
        .as_object_mut()
        .unwrap()
        .remove("link");
    std::fs::write(&entry.path, serde_json::to_vec(&bad).unwrap()).unwrap();
    assert!(store.load_timeline(&entry).is_err());
    let mut old = saved;
    for clip in old["timeline"]["clips"].as_array_mut().unwrap() {
        clip.as_object_mut().unwrap().remove("link");
    }
    std::fs::write(&entry.path, serde_json::to_vec(&old).unwrap()).unwrap();
    assert_eq!(store.load_timeline(&entry).unwrap(), before);
    std::fs::remove_dir_all(root).unwrap();
}
