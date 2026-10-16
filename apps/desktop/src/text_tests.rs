//! GPUI 真实输入路由、字幕范围和持久化回归；测试数据放在独占临时目录。
use crate::{input::TextInput, view::Desktop};
use dinuosi_core::ProjectStore;
use gpui::{EntityInputHandler, Modifiers, MouseButton, TestAppContext, point, px, size};

/// 从本轮实际布局取得控件中心后单次点击，避免混用上下文的可变借用。
fn click(cx: &mut gpui::VisualTestContext, selector: &'static str) {
    let point = cx.debug_bounds(selector).unwrap().center();
    cx.simulate_click(point, Modifiers::none());
}

/// 真实点击描边开关验证两类文本默认关闭、参数保存、撤销及字幕整组/单句范围，锁定后不能修改。
#[gpui::test]
fn text_outline_switch_persists_and_respects_caption_scope(cx: &mut TestAppContext) {
    let root = std::env::temp_dir().join(format!("dinuosi-outline-ui-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let store = ProjectStore::new(root.join("recent.json"));
    let entry = store.create("描边开关", &root).unwrap();
    let (desktop, cx) = cx
        .add_window_view(|window, cx| Desktop::new(store.clone(), Some(entry.clone()), window, cx));
    cx.simulate_resize(size(px(1440.0), px(1600.0)));
    desktop.update_in(cx, |v, window, cx| v.add_text_clip(false, None, window, cx));
    cx.run_until_parked();
    let outlines = || {
        store
            .load_timeline(&entry)
            .unwrap()
            .clips
            .iter()
            .map(|c| c.text.as_ref().unwrap().style.outline)
            .collect::<Vec<_>>()
    };
    assert_eq!(outlines(), [0]);
    click(cx, "text-outline-toggle");
    cx.run_until_parked();
    assert_eq!(outlines(), [2]);
    assert!(cx.debug_bounds("text-outline-color").is_some());
    click(cx, "text-outline");
    cx.simulate_keystrokes("ctrl-a");
    cx.simulate_input("4");
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    assert_eq!(outlines(), [4]);
    desktop.update_in(cx, |v, _, cx| {
        v.change_text_style(|s| s.outline_color = 0x123456, cx)
    });
    cx.run_until_parked();
    click(cx, "text-outline-toggle");
    cx.run_until_parked();
    assert_eq!(outlines(), [0]);
    assert_eq!(
        store.load_timeline(&entry).unwrap().clips[0]
            .text
            .as_ref()
            .unwrap()
            .style
            .outline_color,
        0x123456
    );
    desktop.update_in(cx, |v, _, cx| v.timeline_history(false, cx));
    assert_eq!(outlines(), [4]);
    desktop.update_in(cx, |v, _, cx| v.timeline_history(true, cx));
    assert_eq!(outlines(), [0]);
    desktop.update_in(cx, |v, window, cx| {
        v.add_text_clip(true, None, window, cx);
        v.session.timeline_position = 4.0;
        v.add_text_clip(true, None, window, cx);
    });
    cx.run_until_parked();
    assert_eq!(outlines(), [0, 0, 0]);
    click(cx, "text-outline-toggle");
    cx.run_until_parked();
    assert_eq!(outlines(), [0, 2, 2]);
    click(cx, "text-apply-group");
    cx.run_until_parked();
    click(cx, "text-outline-toggle");
    cx.run_until_parked();
    assert_eq!(outlines(), [0, 2, 0]);
    desktop.update_in(cx, |v, _, cx| {
        v.timeline.model.clips[2].locked = true;
        cx.notify();
    });
    cx.run_until_parked();
    click(cx, "text-outline-toggle");
    cx.run_until_parked();
    assert_eq!(outlines(), [0, 2, 0]);
    assert!(desktop.read_with(cx, |v, _| v.notice.is_none()));
    std::fs::remove_dir_all(root).unwrap();
}

/// 从左侧文本分类创建正文并用 IME、多行 Enter、数值控件和画布拖动编辑，验证保存、撤销和字幕样式范围。
#[gpui::test]
fn text_creation_input_drag_and_caption_scope(cx: &mut TestAppContext) {
    let root = std::env::temp_dir().join(format!("dinuosi-text-ui-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let store = ProjectStore::new(root.join("recent.json"));
    let entry = store.create("文本界面", &root).unwrap();
    let (desktop, cx) = cx
        .add_window_view(|window, cx| Desktop::new(store.clone(), Some(entry.clone()), window, cx));
    cx.simulate_resize(size(px(1440.0), px(900.0)));
    cx.run_until_parked();
    click(cx, "filter-text");
    cx.run_until_parked();
    assert!(cx.debug_bounds("timeline-text").is_none());
    assert!(cx.debug_bounds("timeline-subtitles").is_none());
    click(cx, "library-add-text");
    cx.run_until_parked();
    let input = desktop.read_with(cx, |v, _| {
        v.timeline.text.body.as_ref().unwrap().input.clone()
    });
    input.update_in(cx, |input, window, cx| {
        input.replace_and_mark_text_in_range(None, "中文😀", Some(4..4), window, cx);
        assert!(input.is_composing());
        input.replace_text_in_range(None, "中文😀标题", window, cx);
    });
    cx.simulate_keystrokes("enter");
    cx.simulate_input("Title 01");
    cx.run_until_parked();
    assert_eq!(
        desktop.read_with(cx, |v, _| v.timeline.model.clips[0]
            .text
            .as_ref()
            .unwrap()
            .body
            .clone()),
        "中文😀标题\nTitle 01"
    );
    click(cx, "text-size");
    cx.simulate_keystrokes("ctrl-a");
    cx.simulate_input("64");
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    assert_eq!(
        store.load_timeline(&entry).unwrap().clips[0]
            .text
            .as_ref()
            .unwrap()
            .style
            .size,
        64.0
    );
    assert_eq!(
        store.load_timeline(&entry).unwrap().clips[0]
            .text
            .as_ref()
            .unwrap()
            .body,
        "中文😀标题\nTitle 01"
    );
    let shape = desktop.read_with(cx, |v, _| v.preview_shapes()[0].geometry);
    let start = shape.center;
    let delta = point(px(35.0), px(-18.0));
    cx.simulate_mouse_down(start, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_move(start + delta, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_up(start + delta, MouseButton::Left, Modifiers::none());
    cx.run_until_parked();
    assert!(store.load_timeline(&entry).unwrap().clips[0].transform.x > 0.0);
    cx.simulate_keystrokes("ctrl-z");
    cx.run_until_parked();
    assert_eq!(
        store.load_timeline(&entry).unwrap().clips[0].transform.x,
        0.0
    );
    click(cx, "library-add-subtitle");
    cx.simulate_input("第一句");
    desktop.update_in(cx, |v, window, cx| {
        v.session.timeline_position = 4.0;
        v.add_text_clip(true, None, window, cx);
    });
    cx.simulate_input("第二句");
    cx.run_until_parked();
    click(cx, "text-size");
    cx.simulate_keystrokes("ctrl-a");
    cx.simulate_input("48");
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    let group = store.load_timeline(&entry).unwrap();
    assert_eq!(group.clips.len(), 3);
    assert_eq!(group.clips[1].text.as_ref().unwrap().body, "第一句");
    assert_eq!(group.clips[2].text.as_ref().unwrap().body, "第二句");
    assert_eq!(group.clips[1].text.as_ref().unwrap().style.size, 48.0);
    assert_eq!(group.clips[2].text.as_ref().unwrap().style.size, 48.0);
    click(cx, "text-apply-group");
    click(cx, "text-size");
    cx.simulate_keystrokes("ctrl-a");
    cx.simulate_input("60");
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    let local = store.load_timeline(&entry).unwrap();
    assert_eq!(local.clips[1].text.as_ref().unwrap().style.size, 48.0);
    assert_eq!(local.clips[2].text.as_ref().unwrap().style.size, 60.0);
    assert!(desktop.read_with(cx, |v, _| v.notice.is_none()));
    std::fs::remove_dir_all(root).unwrap();
}

/// 单行行为保持原样，多行保留 CRLF/中文/代理对、字素删除与候选范围，Escape 不吞掉候选确认。
#[gpui::test]
fn multiline_input_preserves_unicode_newlines_and_selection(cx: &mut TestAppContext) {
    let (input, cx) = cx.add_window_view(|_, cx| {
        let mut i = TextInput::new("初始".into(), cx);
        i.multiline = true;
        i
    });
    input.update_in(cx, |input, window, cx| {
        input.focus.focus(window);
        input.replace_text_in_range(None, "第一行\r\nSecond 😀", window, cx);
    });
    cx.run_until_parked();
    assert_eq!(
        input.read_with(cx, |v, _| v.text.clone()),
        "第一行\nSecond 😀"
    );
    cx.simulate_keystrokes("backspace");
    assert_eq!(
        input.read_with(cx, |v, _| v.text.clone()),
        "第一行\nSecond "
    );
    cx.simulate_keystrokes("enter");
    cx.simulate_input("第三行");
    assert_eq!(
        input.read_with(cx, |v, _| v.text.clone()),
        "第一行\nSecond \n第三行"
    );
    cx.simulate_keystrokes("ctrl-a backspace");
    assert_eq!(input.read_with(cx, |v, _| v.text.clone()), "");
    input.update_in(cx, |input, window, cx| {
        input.replace_and_mark_text_in_range(None, "文😀", Some(3..3), window, cx);
        assert_eq!(input.marked_text_range(window, cx), Some(0..3));
        input.replace_text_in_range(None, "文字😀", window, cx);
        assert!(!input.is_composing());
    });
    assert_eq!(input.read_with(cx, |v, _| v.text.clone()), "文字😀");
}

/// 时间输入是整数帧时间码，不接受无效分秒、负数、帧率越界和一天以外的时间。
#[test]
fn caption_timecode_is_frame_precise() {
    use crate::text_fields::parse_timecode;
    assert_eq!(parse_timecode("00:01:02:15", 30).unwrap(), 1875);
    assert_eq!(parse_timecode("24:00:00:00", 30).unwrap(), 2_592_000);
    for value in [
        "00:00:00:30",
        "00:60:00:00",
        "24:00:00:01",
        "-1:00:00:00",
        "1.5",
        "00:00:00:99999999999999999999999",
    ] {
        assert!(parse_timecode(value, 30).is_err(), "{value}");
    }
}

/// 在同一编辑窗口命名保存效果预设、再应用到另一个文本；导入 SRT 只有一条历史，导出不覆盖文件。
#[gpui::test]
fn text_presets_and_srt_import_use_real_edit_transactions(cx: &mut TestAppContext) {
    let root = std::env::temp_dir().join(format!("dinuosi-text-assets-ui-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let store = ProjectStore::new(root.join("recent.json"));
    let entry = store.create("样式与字幕", &root).unwrap();
    let (desktop, cx) = cx
        .add_window_view(|window, cx| Desktop::new(store.clone(), Some(entry.clone()), window, cx));
    cx.simulate_resize(size(px(1440.0), px(900.0)));
    cx.run_until_parked();
    click(cx, "filter-text");
    cx.run_until_parked();
    click(cx, "library-add-text");
    cx.simulate_input("原正文");
    desktop.update_in(cx, |v, window, cx| {
        v.change_text_style(
            |s| {
                s.shadow = Some(Default::default());
                s.background = Some(Default::default());
                s.color = 0x55ddff;
            },
            cx,
        );
        v.begin_text_preset(window, cx);
    });
    cx.simulate_input("常用字幕艺术字");
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    let preset = store.load_text_presets().unwrap().remove(0);
    assert_eq!(preset.name, "常用字幕艺术字");
    assert!(preset.style.shadow.is_some());
    assert!(preset.style.background.is_some());
    desktop.update_in(cx, |v, window, cx| {
        v.session.timeline_position = 4.0;
        v.add_text_clip(false, None, window, cx);
    });
    cx.simulate_input("第二个正文");
    desktop.update_in(cx, |v, window, cx| {
        v.use_text_preset(&preset.id, window, cx)
    });
    cx.run_until_parked();
    let model = store.load_timeline(&entry).unwrap();
    assert_eq!(model.clips[1].text.as_ref().unwrap().body, "第二个正文");
    assert_eq!(model.clips[1].start, 120);
    assert_eq!(model.clips[1].text.as_ref().unwrap().style, preset.style);
    let file = root.join("captions.srt");
    std::fs::write(&file, "1\n00:00:01,000 --> 00:00:02,000\n导入第一句\n\n2\n00:00:03,000 --> 00:00:04,000\n导入第二句\n").unwrap();
    desktop.update_in(cx, |v, _, cx| v.import_caption_file(file.clone(), cx));
    cx.run_until_parked();
    let imported = store.load_timeline(&entry).unwrap();
    assert_eq!(imported.clips.len(), 4);
    assert_eq!(imported.clips[2].text.as_ref().unwrap().body, "导入第一句");
    let group = imported.clips[2]
        .text
        .as_ref()
        .unwrap()
        .group
        .as_ref()
        .unwrap();
    let content = imported.export_srt(group).unwrap();
    let output = root.join("output.srt");
    click(cx, "text-export-srt");
    desktop.update_in(cx, |v, window, _| {
        assert!(v.focus_handle.is_focused(window))
    });
    cx.simulate_new_path_selection(|_| Some(output.clone()));
    cx.run_until_parked();
    cx.simulate_keystrokes("enter");
    desktop.update_in(cx, |v, window, _| {
        assert!(v.focus_handle.is_focused(window))
    });
    assert_eq!(std::fs::read_to_string(&output).unwrap(), content);
    assert!(crate::caption_files::write_srt(&output, "不能覆盖").is_err());
    assert_eq!(std::fs::read_to_string(&output).unwrap(), content);
    desktop.update_in(cx, |v, _, cx| v.timeline_history(false, cx));
    assert_eq!(store.load_timeline(&entry).unwrap(), model);
    desktop.update_in(cx, |v, _, cx| v.timeline_history(true, cx));
    assert_eq!(store.load_timeline(&entry).unwrap(), imported);
    assert!(desktop.read_with(cx, |v, _| v.notice.is_none()));
    std::fs::remove_dir_all(root).unwrap();
}
