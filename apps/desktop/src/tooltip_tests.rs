//! 原生 GPUI 悬停、点击和焦点事件验证 Tooltip 文案、显示时机及定位。
use crate::view::Desktop;
use dinuosi_core::ProjectStore;
use gpui::{Modifiers, TestAppContext, VisualTestContext, point, px, size};

/// 鼠标进入后只完成一次事件绘制即检查气泡，避免推进延时任务掩盖“立即显示”退化。
/// 同时检查气泡位于触发区下方，且横向不溢出窗口。
fn hover(cx: &mut VisualTestContext, trigger: &'static str, text: &'static str, width: f32) {
    let button = cx.debug_bounds(trigger).unwrap();
    cx.simulate_mouse_move(button.center(), None, Modifiers::none());
    let selector = Box::leak(format!("tooltip-{text}").into_boxed_str());
    let bubble = cx
        .debug_bounds(selector)
        .expect("进入控件后没有立即显示正确文案");
    assert!(bubble.top() >= button.bottom() + px(6.0));
    assert!(bubble.left() >= px(0.0) && bubble.right() <= px(width));
}

/// 在默认及最小窗口检查轨道名称和可切换状态、禁用工具、缩放及键盘焦点。
/// 所有轨道操作使用原有事件回调，并验证核心模型的真实变化。
#[gpui::test]
fn timeline_tooltips_are_immediate_and_describe_actions(cx: &mut TestAppContext) {
    let directory = std::env::temp_dir().join(format!("dinuosi-tooltip-{}", std::process::id()));
    std::fs::create_dir_all(&directory).unwrap();
    let store = ProjectStore::new(directory.join("recent.json"));
    let entry = store.create("提示验证", &directory).unwrap();
    let (desktop, cx) = cx
        .add_window_view(|window, cx| Desktop::new(store.clone(), Some(entry.clone()), window, cx));
    for width in [1440.0, 1040.0] {
        cx.simulate_resize(size(px(width), px(900.0)));
        cx.run_until_parked();
        hover(cx, "track-rename-1", "重命名轨道名", width);
        hover(cx, "track-lock-1", "锁定轨道", width);
        let button = cx.debug_bounds("track-lock-1").unwrap().center();
        cx.simulate_click(button, Modifiers::none());
        assert!(desktop.read_with(cx, |v, _| v.timeline.model.tracks[1].locked));
        cx.simulate_mouse_move(point(px(600.0), px(400.0)), None, Modifiers::none());
        hover(cx, "track-lock-1", "解锁轨道", width);
        cx.simulate_click(button, Modifiers::none());
        assert!(!desktop.read_with(cx, |v, _| v.timeline.model.tracks[1].locked));
        hover(cx, "track-visible-1", "隐藏轨道", width);
        let button = cx.debug_bounds("track-visible-1").unwrap().center();
        cx.simulate_click(button, Modifiers::none());
        assert!(!desktop.read_with(cx, |v, _| v.timeline.model.tracks[1].visible));
        cx.simulate_mouse_move(point(px(600.0), px(400.0)), None, Modifiers::none());
        hover(cx, "track-visible-1", "显示轨道", width);
        cx.simulate_click(button, Modifiers::none());
        hover(cx, "timeline-razor", "刀片切割", width);
        hover(cx, "zoom-in", "放大时间线", width);
        cx.simulate_mouse_move(point(px(600.0), px(400.0)), None, Modifiers::none());
    }
    desktop.update_in(cx, |v, window, _| v.focus_handle.focus(window));
    cx.simulate_keystrokes("tab");
    cx.run_until_parked();
    assert!(
        cx.debug_bounds("tooltip-选择工具").is_some(),
        "键盘聚焦未触发提示"
    );
    cx.simulate_keystrokes("escape");
    let focused = cx.update(|window, app| window.focused(app).unwrap());
    cx.simulate_keystrokes("tab");
    assert_ne!(
        cx.update(|window, app| window.focused(app).unwrap()),
        focused
    );
    std::fs::remove_dir_all(directory).unwrap();
}
