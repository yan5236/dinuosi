//! 新工具栏的 GPUI 指针回归：标记管理、跳转、磁吸与锁定的真实控件调用。
use crate::view::Desktop;
use dinuosi_core::{ProjectStore, media::MediaKind};
use gpui::{Modifiers, MouseButton, TestAppContext, px, size};
/// 点击本次绘制的控件边界并等待界面稳定，避免直接调用业务方法掩盖未接通的按钮。
fn click(cx: &mut gpui::VisualTestContext, selector: &'static str) {
    let point = cx.debug_bounds(selector).unwrap().center();
    cx.simulate_click(point, Modifiers::none());
    cx.run_until_parked();
}
/// 验证工具点击创建标记、联合弹窗草稿/取消/校验/确认、删除及单次历史；锁定后重选、拖动、输入、删除均保持数据不变。
#[gpui::test]
fn timeline_tools_clicks_persist_markers_and_readonly_selection(cx: &mut TestAppContext) {
    let root = std::env::temp_dir().join(format!("dinuosi-tools-ui-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let store = ProjectStore::new(root.join("recent.json"));
    let entry = store.create("辅助工具", &root).unwrap();
    let (desktop, cx) = cx
        .add_window_view(|window, cx| Desktop::new(store.clone(), Some(entry.clone()), window, cx));
    cx.simulate_resize(size(px(1440.0), px(900.0)));
    let id = desktop.update_in(cx, |v, _, cx| {
        let id = v
            .timeline
            .model
            .insert(
                ("tools-ui.png".into(), MediaKind::Image),
                "画面".into(),
                0,
                0,
                0..150,
                150,
            )
            .unwrap();
        v.session.timeline_position = 3.0;
        v.commit_timeline(Default::default(), cx);
        id
    });
    cx.run_until_parked();
    assert!(desktop.read_with(cx, |v, _| v.timeline.model.snapping));
    click(cx, "timeline-snap");
    assert!(!store.load_timeline(&entry).unwrap().snapping);
    click(cx, "timeline-snap");
    assert!(store.load_timeline(&entry).unwrap().snapping);
    click(cx, "timeline-marker");
    let first = store.load_timeline(&entry).unwrap().markers[0].id.clone();
    assert_eq!(store.load_timeline(&entry).unwrap().markers[0].frame, 90);
    desktop.update_in(cx, |v, _, cx| {
        v.session.timeline_position = 1.0;
        cx.notify();
    });
    cx.run_until_parked();
    click(cx, "timeline-marker");
    let early = store.load_timeline(&entry).unwrap().markers[1].id.clone();
    let first_row = Box::leak(format!("marker-row-{first}").into_boxed_str());
    let early_row = Box::leak(format!("marker-row-{early}").into_boxed_str());
    assert!(cx.debug_bounds(early_row).unwrap().top() < cx.debug_bounds(first_row).unwrap().top());
    click(cx, first_row);
    assert_eq!(desktop.read_with(cx, |v, _| v.keyframe_frame()), 90);
    let marker = Box::leak(format!("ruler-marker-{early}").into_boxed_str());
    let point = cx.debug_bounds(marker).unwrap().center();
    cx.simulate_mouse_down(point, MouseButton::Right, Modifiers::none());
    cx.simulate_mouse_up(point, MouseButton::Right, Modifiers::none());
    cx.run_until_parked();
    assert_eq!(desktop.read_with(cx, |v, _| v.keyframe_frame()), 90);
    let unchanged = store.load_timeline(&entry).unwrap();
    click(cx, "marker-edit");
    cx.simulate_keystrokes("ctrl-a");
    cx.simulate_input("取消草稿");
    click(cx, "marker-color-66c9cf");
    assert_eq!(store.load_timeline(&entry).unwrap(), unchanged);
    click(cx, "marker-dialog-cancel");
    assert_eq!(store.load_timeline(&entry).unwrap(), unchanged);
    cx.simulate_mouse_down(point, MouseButton::Right, Modifiers::none());
    cx.simulate_mouse_up(point, MouseButton::Right, Modifiers::none());
    cx.run_until_parked();
    click(cx, "marker-edit");
    cx.simulate_keystrokes("ctrl-a backspace");
    click(cx, "marker-dialog-submit");
    assert!(desktop.read_with(cx, |v, _| v.timeline.marker_dialog.is_some()));
    assert_eq!(store.load_timeline(&entry).unwrap(), unchanged);
    click(cx, "marker-dialog-name");
    cx.simulate_input("改名标记");
    click(cx, "marker-color-66c9cf");
    assert_eq!(store.load_timeline(&entry).unwrap(), unchanged);
    click(cx, "marker-dialog-submit");
    assert!(desktop.read_with(cx, |v, _| v.timeline.marker_dialog.is_none()));
    let edited = store.load_timeline(&entry).unwrap();
    assert_eq!(edited.markers[1].name, "改名标记");
    assert_eq!(edited.markers[1].color, 0x66c9cf);
    cx.simulate_keystrokes("ctrl-z");
    cx.run_until_parked();
    assert_eq!(store.load_timeline(&entry).unwrap(), unchanged);
    cx.simulate_keystrokes("ctrl-y");
    cx.run_until_parked();
    assert_eq!(store.load_timeline(&entry).unwrap(), edited);
    cx.simulate_mouse_down(point, MouseButton::Right, Modifiers::none());
    cx.simulate_mouse_up(point, MouseButton::Right, Modifiers::none());
    cx.run_until_parked();
    click(cx, "marker-delete");
    assert_eq!(store.load_timeline(&entry).unwrap().markers.len(), 1);
    cx.simulate_keystrokes("ctrl-z");
    cx.run_until_parked();
    assert_eq!(store.load_timeline(&entry).unwrap().markers.len(), 2);
    click(cx, "inspector-basic-tab");
    let clip = Box::leak(format!("clip-{id}").into_boxed_str());
    click(cx, clip);
    click(cx, "timeline-lock");
    let locked = store.load_timeline(&entry).unwrap();
    assert!(locked.clips[0].locked);
    desktop.update_in(cx, |v, _, cx| {
        v.timeline.selected.clear();
        cx.notify();
    });
    cx.run_until_parked();
    let start = cx.debug_bounds(clip).unwrap().center();
    cx.simulate_mouse_down(start, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_move(
        start + gpui::point(px(80.0), px(0.0)),
        MouseButton::Left,
        Modifiers::none(),
    );
    cx.simulate_mouse_up(
        start + gpui::point(px(80.0), px(0.0)),
        MouseButton::Left,
        Modifiers::none(),
    );
    cx.run_until_parked();
    assert!(desktop.read_with(cx, |v, _| v.timeline.selected.contains(&id)));
    assert!(desktop.read_with(cx, |v, _| v.timeline.gesture.is_none()));
    click(cx, "inspector-value-X");
    assert!(desktop.read_with(cx, |v, _| v.timeline.input.is_none()));
    cx.simulate_keystrokes("delete");
    cx.run_until_parked();
    assert_eq!(store.load_timeline(&entry).unwrap(), locked);
    click(cx, "timeline-lock");
    assert!(!store.load_timeline(&entry).unwrap().clips[0].locked);
    std::fs::remove_dir_all(root).unwrap();
}

/// 设置开关仅在应用后显示标记竖线；竖杆、竖线、项目帧在横滚和多种窗口尺寸下中心一致。
#[gpui::test]
fn marker_line_setting_and_flag_pole_follow_frame(context: &mut TestAppContext) {
    use crate::{
        preferences::UserPreferences,
        settings::{Settings, Tab},
        timeline_markers::MARKER_POLE_X,
    };
    let root = std::env::temp_dir().join(format!("dinuosi-marker-lines-ui-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let store = ProjectStore::new(root.join("recent.json"));
    let entry = store.create("标记竖线", &root).unwrap();
    let mut settings_context = context.clone();
    let (desktop, editor) = context
        .add_window_view(|window, cx| Desktop::new(store.clone(), Some(entry.clone()), window, cx));
    editor.simulate_resize(size(px(1440.0), px(900.0)));
    let id = desktop.update_in(editor, |v, _, cx| {
        let id = v.timeline.model.add_marker(90, "杆对齐".into()).unwrap();
        v.commit_timeline(Default::default(), cx);
        id
    });
    editor.run_until_parked();
    let no_lines = desktop.update_in(editor, |v, _, cx| {
        v.marker_lines(v.timeline.header, v.timeline_view_width(), cx)
            .is_none()
    });
    assert!(no_lines);
    let (settings, visual) = settings_context.add_window_view(|window, cx| {
        Settings::new(
            desktop.downgrade(),
            store.clone(),
            None,
            Tab::General,
            window,
            cx,
        )
    });
    visual.simulate_resize(size(px(860.0), px(600.0)));
    visual.run_until_parked();
    click(visual, "show-marker-lines");
    assert!(settings.read_with(visual, |v, _| v.draft.show_marker_lines));
    assert!(!visual.read(|cx| cx.global::<UserPreferences>().0.show_marker_lines));
    assert!(!store.load_preferences().unwrap().show_marker_lines);
    click(visual, "settings-apply");
    assert!(store.load_preferences().unwrap().show_marker_lines);
    let flag_selector = Box::leak(format!("ruler-marker-{id}").into_boxed_str());
    let line_selector = Box::leak(format!("marker-line-{id}").into_boxed_str());
    for (width, height, scroll) in [
        (1440.0, 900.0, 0.0),
        (1040.0, 640.0, 10.0),
        (2560.0, 1440.0, 22.0),
    ] {
        editor.simulate_resize(size(px(width), px(height)));
        desktop.update_in(editor, |v, _, cx| {
            v.timeline.scroll_x = scroll;
            cx.notify();
        });
        editor.run_until_parked();
        let frame_x = desktop.read_with(editor, |v, _| {
            v.timeline.bounds.left() + px(v.timeline.header + 3.0 * v.pixels_per_second() - scroll)
        });
        let flag = editor.debug_bounds(flag_selector).unwrap();
        let line = editor.debug_bounds(line_selector).unwrap();
        // GPUI 布局量化到半像素，理论帧坐标与最终绘制边界允许四分之一像素舍入。
        assert!((flag.left() + px(MARKER_POLE_X) - frame_x).abs() < px(0.3));
        assert!(
            (line.center().x - frame_x).abs() < px(0.3),
            "width={width} scroll={scroll} frame={frame_x:?} flag={flag:?} line={line:?}"
        );
    }
    click(visual, "show-marker-lines");
    click(visual, "settings-apply");
    assert!(!store.load_preferences().unwrap().show_marker_lines);
    assert!(desktop.update_in(editor, |v, _, cx| {
        v.marker_lines(v.timeline.header, v.timeline_view_width(), cx)
            .is_none()
    }));
    std::fs::remove_dir_all(root).unwrap();
}
