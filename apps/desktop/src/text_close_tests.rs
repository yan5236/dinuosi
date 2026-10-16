//! 关闭前正文提交失败的草稿恢复、重试和撤销回归，直接调用原生关闭回调共用的入口。
use crate::view::Desktop;
use dinuosi_core::ProjectStore;
use gpui::{TestAppContext, px, size};

/// 磁盘项目暂不可写时拒绝连续关闭和失焦提交，保留正文；恢复后重试只保存一条历史，取消也可恢复。
#[gpui::test]
fn close_failure_preserves_text_draft_for_retry_or_cancel(cx: &mut TestAppContext) {
    let root = std::env::temp_dir().join(format!("dinuosi-text-close-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let store = ProjectStore::new(root.join("recent.json"));
    let entry = store.create("关闭草稿", &root).unwrap();
    let (desktop, cx) = cx
        .add_window_view(|window, cx| Desktop::new(store.clone(), Some(entry.clone()), window, cx));
    cx.simulate_resize(size(px(1440.0), px(900.0)));
    cx.run_until_parked();
    desktop.update_in(cx, |v, window, cx| {
        v.busy = true;
        assert!(!v.prepare_close(cx));
        v.busy = false;
        assert!(v.prepare_close(cx));
        v.add_text_clip(false, None, window, cx);
    });
    cx.run_until_parked();
    let before = store.load_timeline(&entry).unwrap();
    cx.simulate_keystrokes("ctrl-a");
    cx.simulate_input("未保存的正文\n第二行");
    cx.run_until_parked();
    let backup = entry.path.with_extension("bak");
    std::fs::rename(&entry.path, &backup).unwrap();
    desktop.update_in(cx, |v, window, cx| {
        for _ in 0..2 {
            assert!(!v.prepare_close(cx));
            assert_eq!(v.timeline.model, before);
            assert!(v.notice.is_some());
            let edit = v.timeline.text.body.as_ref().unwrap();
            assert_eq!(edit.input.read(cx).text, "未保存的正文\n第二行");
            assert_eq!(edit.before, before);
        }
        v.commit_inspector_input(cx);
        assert!(v.timeline.text.body.is_some());
        v.run_project(
            dinuosi_command::ProjectCommand::Open {
                path: entry.path.clone(),
            },
            true,
            window,
            cx,
        );
        assert!(!v.busy, "保存失败时不应开始切换项目");
    });
    cx.run_until_parked();
    let home = cx.debug_bounds("brand-home").unwrap().center();
    cx.simulate_click(home, gpui::Modifiers::none());
    desktop.update_in(cx, |v, _, cx| {
        assert!(v.current.is_some());
        assert!(v.timeline.text.body.is_some());
        v.menu_open = true;
        cx.notify();
    });
    cx.run_until_parked();
    let close = cx.debug_bounds("menu-close").unwrap().center();
    cx.simulate_click(close, gpui::Modifiers::none());
    assert!(desktop.read_with(cx, |v, _| v.timeline.text.body.is_some()
        && v.notice.is_some()));
    std::fs::rename(&backup, &entry.path).unwrap();
    assert_eq!(store.load_timeline(&entry).unwrap(), before);
    desktop.update_in(cx, |v, _, cx| assert!(v.prepare_close(cx)));
    let after = store.load_timeline(&entry).unwrap();
    assert_eq!(
        after.clips[0].text.as_ref().unwrap().body,
        "未保存的正文\n第二行"
    );
    assert!(desktop.read_with(cx, |v, _| v.timeline.text.body.is_none()));
    desktop.update_in(cx, |v, _, cx| v.timeline_history(false, cx));
    assert_eq!(store.load_timeline(&entry).unwrap(), before);
    desktop.update_in(cx, |v, window, cx| {
        v.timeline_history(true, cx);
        v.begin_text_body(window, cx);
    });
    cx.run_until_parked();
    cx.simulate_keystrokes("ctrl-a");
    cx.simulate_input("取消的草稿");
    cx.run_until_parked();
    std::fs::rename(&entry.path, &backup).unwrap();
    desktop.update_in(cx, |v, _, cx| {
        assert!(!v.prepare_close(cx));
        v.cancel_text_input(cx);
        assert_eq!(v.timeline.model, after);
    });
    std::fs::rename(&backup, &entry.path).unwrap();
    desktop.update_in(cx, |v, _, cx| assert!(v.prepare_close(cx)));
    assert_eq!(store.load_timeline(&entry).unwrap(), after);
    std::fs::remove_dir_all(root).unwrap();
}
