//! 点击实际时间线工具和检查器页签，验证接线、保存、撤回还原及锁定边界。
use crate::{timeline_state::InspectorTab, view::Desktop};
use dinuosi_core::{ProjectStore, media::MediaKind};
use gpui::{Modifiers, TestAppContext, VisualTestContext, px, size};

/// 在 GPUI 实际布局中点击指定控件并完成重绘，避免直接调用业务函数漏测按钮接线。
fn click(cx: &mut VisualTestContext, selector: &'static str) {
    let point = cx.debug_bounds(selector).unwrap().center();
    cx.simulate_click(point, Modifiers::none());
    cx.run_until_parked();
}

/// 验证按钮倒放、定格、撤回/还原及历史分叉；变速只存在空检查器页，锁定和空选择保持数据。
#[gpui::test]
fn playback_toolbar_history_and_empty_speed_tab(cx: &mut TestAppContext) {
    let root = std::env::temp_dir().join(format!("dinuosi-playback-ui-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let store = ProjectStore::new(root.join("recent.json"));
    let entry = store.create("倒放定格控件", &root).unwrap();
    let (desktop, cx) = cx
        .add_window_view(|window, cx| Desktop::new(store.clone(), Some(entry.clone()), window, cx));
    cx.simulate_resize(size(px(1440.0), px(900.0)));
    let id = desktop.update_in(cx, |v, _, cx| {
        let before = v.timeline.model.clone();
        let id = v
            .timeline
            .model
            .insert(
                ("playback-ui.mp4".into(), MediaKind::Video),
                "视频".into(),
                0,
                0,
                20..80,
                120,
            )
            .unwrap();
        v.timeline.selected.insert(id.clone());
        v.session.timeline_position = 0.5;
        v.commit_timeline(before, cx);
        id
    });
    cx.run_until_parked();
    let original = store.load_timeline(&entry).unwrap();
    click(cx, "timeline-reverse-playback");
    let reversed = store.load_timeline(&entry).unwrap();
    assert!(reversed.clips[0].reversed);
    desktop.update_in(cx, |v, _, _| {
        v.current.as_mut().unwrap().path = root.join("missing.dinuosi");
    });
    click(cx, "timeline-undo");
    assert!(desktop.read_with(cx, |v, _| {
        v.timeline.model == reversed
            && v.timeline.history.can_step(false)
            && !v.timeline.history.can_step(true)
            && v.notice.is_some()
    }));
    desktop.update_in(cx, |v, _, cx| {
        v.current.as_mut().unwrap().path = entry.path.clone();
        v.notice = None;
        cx.notify();
    });
    cx.run_until_parked();
    click(cx, "timeline-freeze-frame");
    let frozen = store.load_timeline(&entry).unwrap();
    assert_eq!(frozen.clips[0].freeze_frame, Some(64));
    assert_eq!(frozen.clips[0].duration, original.clips[0].duration);
    click(cx, "timeline-undo");
    assert_eq!(store.load_timeline(&entry).unwrap(), reversed);
    click(cx, "timeline-redo");
    assert_eq!(store.load_timeline(&entry).unwrap(), frozen);
    click(cx, "timeline-freeze-frame");
    click(cx, "timeline-reverse-playback");
    assert_eq!(store.load_timeline(&entry).unwrap(), original);
    click(cx, "inspector-speed-tab");
    assert!(desktop.read_with(cx, |v, _| v.timeline.inspector_tab == InspectorTab::Speed));
    assert!(cx.debug_bounds("inspector-speed-page").is_some());
    assert!(cx.debug_bounds("timeline-speed").is_none());
    click(cx, "inspector-markers-tab");
    assert!(desktop.read_with(cx, |v, _| v.timeline.inspector_tab == InspectorTab::Markers));
    click(cx, "inspector-basic-tab");
    click(cx, "timeline-lock");
    let locked = store.load_timeline(&entry).unwrap();
    click(cx, "timeline-reverse-playback");
    click(cx, "timeline-freeze-frame");
    assert_eq!(store.load_timeline(&entry).unwrap(), locked);
    click(cx, "timeline-undo");
    click(cx, "timeline-reverse-playback");
    let branch = store.load_timeline(&entry).unwrap();
    click(cx, "timeline-redo");
    assert_eq!(store.load_timeline(&entry).unwrap(), branch);
    desktop.update_in(cx, |v, _, cx| {
        v.timeline.selected.clear();
        cx.notify();
    });
    cx.run_until_parked();
    click(cx, "timeline-reverse-playback");
    click(cx, "timeline-freeze-frame");
    assert_eq!(store.load_timeline(&entry).unwrap(), branch);
    desktop.update_in(cx, |v, _, cx| {
        v.timeline.selected.insert(id);
        v.delete_clips(cx);
    });
    cx.run_until_parked();
    click(cx, "timeline-undo");
    assert_eq!(store.load_timeline(&entry).unwrap(), branch);
    click(cx, "timeline-redo");
    assert!(store.load_timeline(&entry).unwrap().clips.is_empty());
    click(cx, "timeline-redo");
    assert!(store.load_timeline(&entry).unwrap().clips.is_empty());
    std::fs::remove_dir_all(root).unwrap();
}
