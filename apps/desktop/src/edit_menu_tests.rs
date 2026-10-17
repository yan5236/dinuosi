//! 用实际 GPUI 鼠标与按键验证顶部菜单、右键菜单和剪贴板持久化事务。
use crate::view::Desktop;
use dinuosi_core::{ProjectStore, media::MediaKind};
use gpui::{Modifiers, MouseButton, TestAppContext, px, size};

/// 点击当前绘制控件并稳定布局；测试必须经过真实菜单命中及事件路由。
fn click(cx: &mut gpui::VisualTestContext, selector: &'static str) {
    let bounds = cx.debug_bounds(selector).unwrap();
    cx.simulate_click(bounds.center(), Modifiers::none());
    cx.run_until_parked();
}

/// 顶部复制/粘贴、撤销/重做与右键剪切/删除均写入磁盘；键盘粘贴复用快照，单次撤销恢复。
/// 两种菜单初开无活动行，未导航确认不执行；方向键跳过禁用项，鼠标悬停清除旧键盘活动项。
#[gpui::test]
fn edit_menu_and_context_clipboard_roundtrip(cx: &mut TestAppContext) {
    let root = std::env::temp_dir().join(format!("dinuosi-edit-menu-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let store = ProjectStore::new(root.join("recent.json"));
    let entry = store.create("编辑菜单", &root).unwrap();
    let (desktop, cx) = cx
        .add_window_view(|window, cx| Desktop::new(store.clone(), Some(entry.clone()), window, cx));
    cx.simulate_resize(size(px(1440.0), px(900.0)));
    cx.run_until_parked();
    let id = desktop.update_in(cx, |v, _, cx| {
        let before = v.timeline.model.clone();
        let id = v
            .timeline
            .model
            .insert(
                ("edit.png".into(), MediaKind::Image),
                "图片".into(),
                0,
                30,
                0..60,
                60,
            )
            .unwrap();
        v.timeline.selected = [id.clone()].into();
        v.commit_timeline(before, cx);
        id
    });
    cx.run_until_parked();
    let original = store.load_timeline(&entry).unwrap();
    click(cx, "edit-menu");
    assert!(desktop.read_with(cx, |v, _| {
        v.timeline.edit_menu.as_ref().unwrap().active.is_none()
    }));
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    assert_eq!(store.load_timeline(&entry).unwrap(), original);
    cx.simulate_keystrokes("down");
    cx.run_until_parked();
    assert_eq!(
        desktop.read_with(cx, |v, _| v.timeline.edit_menu.as_ref().unwrap().active),
        Some(0)
    );
    let copy_point = cx.debug_bounds("edit-action-copy").unwrap().center();
    cx.simulate_mouse_move(copy_point, None, Modifiers::none());
    cx.run_until_parked();
    assert!(desktop.read_with(cx, |v, _| {
        v.timeline.edit_menu.as_ref().unwrap().active.is_none()
    }));
    click(cx, "edit-action-copy");
    assert_eq!(store.load_timeline(&entry).unwrap(), original);
    desktop.update_in(cx, |v, _, _| v.session.timeline_position = 5.0);
    click(cx, "edit-menu");
    click(cx, "edit-action-paste");
    let pasted = store.load_timeline(&entry).unwrap();
    assert_eq!(pasted.clips.len(), 2);
    assert_eq!(pasted.clips[1].start, 150);
    assert_ne!(pasted.clips[1].id, id);
    click(cx, "edit-menu");
    click(cx, "edit-action-undo");
    assert_eq!(store.load_timeline(&entry).unwrap(), original);
    click(cx, "edit-menu");
    click(cx, "edit-action-redo");
    assert_eq!(store.load_timeline(&entry).unwrap(), pasted);
    let selector = Box::leak(format!("clip-{id}").into_boxed_str());
    let point = cx.debug_bounds(selector).unwrap().center();
    cx.simulate_mouse_down(point, MouseButton::Right, Modifiers::none());
    cx.simulate_mouse_up(point, MouseButton::Right, Modifiers::none());
    cx.run_until_parked();
    assert!(desktop.read_with(cx, |v, _| v.timeline.selected == [id.clone()].into()));
    assert!(desktop.read_with(cx, |v, _| {
        v.timeline.edit_menu.as_ref().unwrap().active.is_none()
    }));
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    assert_eq!(store.load_timeline(&entry).unwrap(), pasted);
    cx.simulate_keystrokes("up");
    cx.run_until_parked();
    assert_eq!(
        desktop.read_with(cx, |v, _| v.timeline.edit_menu.as_ref().unwrap().active),
        Some(5)
    );
    cx.simulate_keystrokes("down");
    cx.run_until_parked();
    assert_eq!(
        desktop.read_with(cx, |v, _| v.timeline.edit_menu.as_ref().unwrap().active),
        Some(2)
    );
    click(cx, "edit-action-cut");
    assert_eq!(store.load_timeline(&entry).unwrap().clips.len(), 1);
    cx.simulate_keystrokes("ctrl-z");
    cx.run_until_parked();
    assert_eq!(store.load_timeline(&entry).unwrap(), pasted);
    desktop.update_in(cx, |v, _, _| v.session.timeline_position = 8.0);
    cx.simulate_keystrokes("ctrl-v");
    cx.run_until_parked();
    let after = store.load_timeline(&entry).unwrap();
    assert_eq!(after.clips.len(), 3);
    assert_eq!(after.clips[2].start, 240);
    click(cx, "edit-menu");
    click(cx, "edit-action-delete");
    assert_eq!(store.load_timeline(&entry).unwrap().clips.len(), 2);
    cx.simulate_keystrokes("ctrl-z");
    cx.run_until_parked();
    assert_eq!(store.load_timeline(&entry).unwrap(), after);
    click(cx, "edit-menu");
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    assert!(desktop.read_with(cx, |v, _| v.timeline.edit_menu.is_none()));
    std::fs::remove_dir_all(root).unwrap();
}

/// 剪切/粘贴保存失败恢复选择、模型与旧剪贴板；锁定与空选择禁用编辑，文字 Ctrl+C/V 不复制素材。
#[gpui::test]
fn edit_clipboard_save_failure_locks_and_text_focus(cx: &mut TestAppContext) {
    use crate::shortcuts::Command;
    let root = std::env::temp_dir().join(format!("dinuosi-edit-failure-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let store = ProjectStore::new(root.join("recent.json"));
    let entry = store.create("剪贴板回滚", &root).unwrap();
    let (desktop, cx) = cx
        .add_window_view(|window, cx| Desktop::new(store.clone(), Some(entry.clone()), window, cx));
    cx.simulate_resize(size(px(1440.0), px(900.0)));
    cx.run_until_parked();
    desktop.update_in(cx, |v, _, cx| {
        assert!(!v.can_edit_command(Command::Copy));
        assert!(!v.can_edit_command(Command::Paste));
        let before = v.timeline.model.clone();
        let a = v
            .timeline
            .model
            .insert(
                ("old.png".into(), MediaKind::Image),
                "旧剪贴板".into(),
                0,
                0,
                0..30,
                30,
            )
            .unwrap();
        let b = v
            .timeline
            .model
            .insert(
                ("new.png".into(), MediaKind::Image),
                "新选区".into(),
                0,
                60,
                0..30,
                30,
            )
            .unwrap();
        v.commit_timeline(before, cx);
        v.timeline.selected = [a].into();
        v.copy_timeline_clips(false, cx);
        v.timeline.selected = [b].into();
    });
    let before = store.load_timeline(&entry).unwrap();
    let backup = entry.path.with_extension("bak");
    std::fs::rename(&entry.path, &backup).unwrap();
    cx.simulate_keystrokes("ctrl-x");
    cx.run_until_parked();
    desktop.update_in(cx, |v, _, _| {
        assert_eq!(v.timeline.model, before);
        assert_eq!(v.timeline.selected.len(), 1);
        assert_eq!(
            v.timeline.clipboard.as_ref().unwrap().clips[0].name,
            "旧剪贴板"
        );
        assert!(v.notice.is_some());
        v.session.timeline_position = 6.0;
    });
    cx.simulate_keystrokes("ctrl-v");
    cx.run_until_parked();
    assert!(desktop.read_with(cx, |v, _| v.timeline.model == before
        && v.timeline.selected.len() == 1));
    std::fs::rename(backup, &entry.path).unwrap();
    desktop.update_in(cx, |v, window, cx| {
        v.timeline.model.clips[1].locked = true;
        assert!(!v.can_edit_command(Command::Cut));
        assert!(!v.can_edit_command(Command::Delete));
        assert!(v.can_edit_command(Command::Copy));
        v.timeline.model.clips[1].locked = false;
        v.add_text_clip(false, None, window, cx);
        v.begin_text_body(window, cx);
    });
    cx.run_until_parked();
    cx.simulate_keystrokes("ctrl-a");
    cx.simulate_input("文本剪贴板");
    cx.simulate_keystrokes("ctrl-a ctrl-c ctrl-v");
    cx.run_until_parked();
    assert!(desktop.read_with(
        cx,
        |v, _| v.timeline.clipboard.as_ref().unwrap().clips[0].name == "旧剪贴板"
    ));
    desktop.update_in(cx, |v, _, cx| {
        assert!(v.commit_text_input(cx));
    });
    let text = store.load_timeline(&entry).unwrap();
    assert_eq!(
        text.clips.last().unwrap().text.as_ref().unwrap().body,
        "文本剪贴板"
    );
    std::fs::remove_dir_all(root).unwrap();
}
