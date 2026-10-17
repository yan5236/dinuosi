//! 样式卡片通过公共素材菜单重命名和移除；实际 GPUI 输入及磁盘结果回归。
use crate::view::Desktop;
use dinuosi_core::{ProjectStore, text::Text, timeline::Timeline};
use gpui::{Modifiers, MouseButton, TestAppContext, px, size};

/// 点击本帧绘制控件，避免通过业务函数绕过公共菜单及名称弹窗的事件路径。
fn click(cx: &mut gpui::VisualTestContext, selector: &'static str) {
    let point = cx.debug_bounds(selector).unwrap().center();
    cx.simulate_click(point, Modifiers::none());
    cx.run_until_parked();
}

/// 在卡片内派发右键按下/释放并等待菜单布局；卡片右键不得应用样式或改变片段。
fn right_click(cx: &mut gpui::VisualTestContext, selector: &'static str) {
    let point = cx.debug_bounds(selector).unwrap().center();
    cx.simulate_mouse_down(point, MouseButton::Right, Modifiers::none());
    cx.simulate_mouse_up(point, MouseButton::Right, Modifiers::none());
    cx.run_until_parked();
}

/// 样式卡无删除按钮，右键共用素材菜单；改名校验/取消/保存及移除均不影响已应用片段。
/// 覆盖菜单 Escape、Delete 隔离、写入失败保留弹窗和列表，以及重开样式库的名称持久化。
#[gpui::test]
fn text_preset_context_rename_remove_and_failure_keep_project(cx: &mut TestAppContext) {
    let root = std::env::temp_dir().join(format!("dinuosi-preset-menu-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let store = ProjectStore::new(root.join("recent.json"));
    let entry = store.create("样式菜单", &root).unwrap();
    let mut model = Timeline::default();
    let id = model
        .add_text(0, 90, Text::new("已应用文字".into(), (1920, 1080), false))
        .unwrap();
    store.save_timeline(&entry, &model).unwrap();
    let mut style = model.clips[0].text.as_ref().unwrap().style.clone();
    style.color = 0xff0000;
    let preset = store
        .save_text_preset("旧名称", style.clone())
        .unwrap()
        .remove(0);
    store.save_text_preset("其他样式", style).unwrap();
    let (desktop, cx) = cx
        .add_window_view(|window, cx| Desktop::new(store.clone(), Some(entry.clone()), window, cx));
    cx.simulate_resize(size(px(1440.0), px(900.0)));
    cx.run_until_parked();
    desktop.update_in(cx, |v, _, _| v.timeline.selected = [id].into());
    click(cx, "filter-text");
    let card = Box::leak(format!("text-preset-{}", preset.id).into_boxed_str());
    let removed_button = Box::leak(format!("remove-style-{}", preset.id).into_boxed_str());
    assert!(cx.debug_bounds(removed_button).is_none());
    right_click(cx, card);
    assert_eq!(store.load_timeline(&entry).unwrap(), model);
    cx.simulate_keystrokes("delete");
    cx.run_until_parked();
    assert_eq!(store.load_timeline(&entry).unwrap(), model);
    assert!(desktop.read_with(cx, |v, _| v.session.media_menu.is_some()));
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    assert!(desktop.read_with(cx, |v, _| v.session.media_menu.is_none()));
    right_click(cx, card);
    click(cx, "media-rename");
    cx.simulate_keystrokes("ctrl-a backspace enter");
    cx.run_until_parked();
    assert!(desktop.read_with(cx, |v, _| v.dialog.as_ref().unwrap().error.is_some()));
    assert_eq!(store.load_text_presets().unwrap()[0], preset);
    cx.simulate_input("其他样式");
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    assert!(desktop.read_with(cx, |v, _| v.dialog.as_ref().unwrap().error.is_some()));
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    assert_eq!(store.load_text_presets().unwrap()[0], preset);
    right_click(cx, card);
    click(cx, "media-rename");
    cx.simulate_keystrokes("ctrl-a");
    cx.simulate_input("新的中文名称");
    let path = root.join("text-presets.json");
    let backup = root.join("text-presets.bak");
    std::fs::rename(&path, &backup).unwrap();
    std::fs::create_dir(&path).unwrap();
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    assert!(
        desktop.read_with(cx, |v, _| v.dialog.as_ref().unwrap().error.is_some()
            && v.timeline.text.presets[0].preset == preset)
    );
    std::fs::remove_dir(&path).unwrap();
    std::fs::rename(&backup, &path).unwrap();
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    let saved = ProjectStore::new(root.join("recent.json"))
        .load_text_presets()
        .unwrap();
    assert_eq!(saved[0].id, preset.id);
    assert_eq!(saved[0].style, preset.style);
    assert_eq!(saved[0].name, "新的中文名称");
    assert!(desktop.read_with(cx, |v, _| v.dialog.is_none()
        && v.timeline.text.presets[0].preset.name == "新的中文名称"));
    assert_eq!(store.load_timeline(&entry).unwrap(), model);
    right_click(cx, card);
    click(cx, "media-remove");
    let remaining = store.load_text_presets().unwrap();
    assert_eq!(remaining.len(), 1);
    assert_ne!(remaining[0].id, preset.id);
    assert!(
        desktop.read_with(cx, |v, _| v.timeline.text.presets.len() == 1
            && v.session.media_menu.is_none())
    );
    assert_eq!(store.load_timeline(&entry).unwrap(), model);
    std::fs::remove_dir_all(root).unwrap();
}
