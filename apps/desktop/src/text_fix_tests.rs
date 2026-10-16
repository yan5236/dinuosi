//! 文本审查修复回归：覆盖输入事务、全局字幕管理与真实控件事件。
use crate::view::Desktop;
use dinuosi_core::ProjectStore;
use gpui::{Modifiers, MouseButton, TestAppContext, point, px, size};

/// 点击本轮布局中的实际控件，定位失败直接报错，防止测试绕过视图路径。
fn click(cx: &mut gpui::VisualTestContext, selector: &'static str) {
    let point = cx.debug_bounds(selector).unwrap().center();
    cx.simulate_click(point, Modifiers::none());
    cx.run_until_parked();
}

/// 空选择和字幕主对象显示字幕页；其他媒体与普通文本隐藏入口，并让旧字幕页回退到正确检查器。
/// 多选以最后点击的主对象判断，不能因为选择集合中还含有字幕就继续展示字幕列表。
/// GPUI 的 bounds 会保留历史控件，显隐检查共享规则，并正向验证当前对象的检查器已绘制。
#[gpui::test]
fn captions_tab_follows_inspector_selection(cx: &mut TestAppContext) {
    use dinuosi_core::{media::MediaKind, text::Text};
    let root = std::env::temp_dir().join(format!("dinuosi-caption-tabs-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let store = ProjectStore::new(root.join("recent.json"));
    let entry = store.create("字幕页签范围", &root).unwrap();
    let (desktop, cx) =
        cx.add_window_view(|window, cx| Desktop::new(store.clone(), Some(entry), window, cx));
    cx.simulate_resize(size(px(1440.0), px(900.0)));
    cx.run_until_parked();
    let (subtitle, ordinary, media) = desktop.update_in(cx, |v, _, cx| {
        let before = v.timeline.model.clone();
        let mut media = Vec::new();
        for (i, kind) in [MediaKind::Image, MediaKind::Video, MediaKind::Audio]
            .into_iter()
            .enumerate()
        {
            media.push(
                v.timeline
                    .model
                    .insert(
                        (format!("tab-{i}.fixture").into(), kind),
                        "素材".into(),
                        0,
                        i as i64 * 90,
                        0..90,
                        90,
                    )
                    .unwrap(),
            );
        }
        let subtitle = v
            .timeline
            .model
            .add_text(0, 90, Text::new("字幕".into(), (1920, 1080), true))
            .unwrap();
        let ordinary = v
            .timeline
            .model
            .add_text(0, 90, Text::new("普通文本".into(), (1920, 1080), false))
            .unwrap();
        v.commit_timeline(before, cx);
        (subtitle, ordinary, media)
    });
    click(cx, "inspector-captions-tab");
    assert!(cx.debug_bounds("caption-list").is_some());
    for (id, selector) in media.iter().chain(std::iter::once(&ordinary)).zip([
        "inspector-slider-Opacity",
        "inspector-slider-Opacity",
        "audio-inspector",
        "text-inspector",
    ]) {
        desktop.update_in(cx, |v, _, cx| {
            v.timeline.selected = [id.clone()].into();
            v.timeline.primary_clip = Some(id.clone());
            assert!(!v.captions_tab_visible());
            cx.notify();
        });
        cx.run_until_parked();
        assert!(!desktop.read_with(cx, |v, _| v.captions_tab_visible()));
        assert!(
            cx.debug_bounds(selector).is_some(),
            "未回到当前对象的检查器"
        );
    }
    for primary in [&subtitle, &media[0]] {
        desktop.update_in(cx, |v, _, cx| {
            v.timeline.selected = [subtitle.clone(), media[0].clone()].into();
            v.timeline.primary_clip = Some(primary.clone());
            cx.notify();
        });
        cx.run_until_parked();
        assert_eq!(
            desktop.read_with(cx, |v, _| v.captions_tab_visible()),
            primary == &subtitle
        );
    }
    desktop.update_in(cx, |v, _, cx| {
        v.timeline.selected.clear();
        v.timeline.primary_clip = None;
        cx.notify();
    });
    cx.run_until_parked();
    assert!(cx.debug_bounds("inspector-captions-tab").is_some());
    assert!(cx.debug_bounds("caption-list").is_some());
    std::fs::remove_dir_all(root).unwrap();
}

/// 正文仍聚焦时通过工具栏删除，不得留下失效正文会话；后续失焦和撤销应保留准确的编辑顺序。
#[gpui::test]
fn deleting_focused_text_does_not_resurrect_it(cx: &mut TestAppContext) {
    let root = std::env::temp_dir().join(format!("dinuosi-delete-text-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let store = ProjectStore::new(root.join("recent.json"));
    let entry = store.create("正文删除", &root).unwrap();
    let (desktop, cx) = cx
        .add_window_view(|window, cx| Desktop::new(store.clone(), Some(entry.clone()), window, cx));
    cx.simulate_resize(size(px(1440.0), px(900.0)));
    cx.run_until_parked();
    click(cx, "filter-text");
    click(cx, "library-add-text");
    cx.simulate_input("应被删除的正文");
    click(cx, "timeline-trash");
    desktop.update_in(cx, |v, window, cx| {
        v.focus_handle.focus(window);
        v.commit_inspector_input(cx);
        assert!(v.timeline.text.body.is_none());
        assert!(v.timeline.model.clips.is_empty());
    });
    assert!(store.load_timeline(&entry).unwrap().clips.is_empty());
    desktop.update_in(cx, |v, _, cx| v.timeline_history(false, cx));
    assert_eq!(
        store.load_timeline(&entry).unwrap().clips[0]
            .text
            .as_ref()
            .unwrap()
            .body,
        "应被删除的正文"
    );
    std::fs::remove_dir_all(root).unwrap();
}

/// 全局列表覆盖跨组整行定位、不抢正文焦点、精确删除与锁定保护；撤销删除不恢复旧正文。
#[gpui::test]
fn global_captions_edit_delete_lock_and_undo(cx: &mut TestAppContext) {
    let root = std::env::temp_dir().join(format!("dinuosi-global-captions-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let store = ProjectStore::new(root.join("recent.json"));
    let entry = store.create("全局字幕", &root).unwrap();
    let (desktop, cx) = cx
        .add_window_view(|window, cx| Desktop::new(store.clone(), Some(entry.clone()), window, cx));
    cx.simulate_resize(size(px(1440.0), px(900.0)));
    cx.run_until_parked();
    let (a, b, title) = desktop.update_in(cx, |v, _, cx| {
        let before = v.timeline.model.clone();
        let text = |body: &str, subtitle| {
            dinuosi_core::text::Text::new(body.into(), (1920, 1080), subtitle)
        };
        let a = v
            .timeline
            .model
            .add_text(0, 90, text("第一组", true))
            .unwrap();
        let b = v
            .timeline
            .model
            .add_text(120, 90, text("第二组", true))
            .unwrap();
        let title = v
            .timeline
            .model
            .add_text(240, 90, text("保留标题", false))
            .unwrap();
        v.commit_timeline(before, cx);
        (a, b, title)
    });
    click(cx, "inspector-captions-tab");
    assert!(cx.debug_bounds("caption-body-0").is_some());
    assert!(cx.debug_bounds("caption-body-1").is_some());
    assert!(cx.debug_bounds("text-size").is_none());
    let row = cx.debug_bounds("caption-row-1").unwrap();
    cx.simulate_click(row.origin + point(px(5.0), px(5.0)), Modifiers::none());
    cx.run_until_parked();
    desktop.read_with(cx, |v, _| {
        assert_eq!(v.timeline.primary_clip.as_ref(), Some(&b));
        assert_eq!(
            v.session.timeline_position,
            120.0 / v.timeline.model.fps as f64
        );
        assert!(v.timeline.text.body.is_none());
    });
    click(cx, "caption-body-1");
    assert!(desktop.read_with(cx, |v, _| v.timeline.text.body.is_some()));
    cx.simulate_keystrokes("ctrl-a");
    cx.simulate_input("第二组已编辑");
    let row = cx.debug_bounds("caption-row-0").unwrap();
    cx.simulate_click(row.origin + point(px(5.0), px(5.0)), Modifiers::none());
    cx.run_until_parked();
    desktop.read_with(cx, |v, _| {
        assert_eq!(v.timeline.primary_clip.as_ref(), Some(&a));
        assert_eq!(v.session.timeline_position, 0.0);
        assert!(v.timeline.text.body.is_none());
    });
    click(cx, "Next");
    desktop.read_with(cx, |v, _| {
        assert_eq!(v.timeline.primary_clip.as_ref(), Some(&b));
        assert!(v.timeline.text.body.is_none());
    });
    click(cx, "caption-delete-0");
    let saved = store.load_timeline(&entry).unwrap();
    assert!(!saved.clips.iter().any(|c| c.id == a));
    assert_eq!(
        saved
            .clips
            .iter()
            .find(|c| c.id == b)
            .unwrap()
            .text
            .as_ref()
            .unwrap()
            .body,
        "第二组已编辑"
    );
    assert!(saved.clips.iter().any(|c| c.id == title));
    desktop.update_in(cx, |v, _, cx| {
        let before = v.timeline.model.clone();
        v.timeline
            .model
            .clips
            .iter_mut()
            .find(|c| c.id == b)
            .unwrap()
            .locked = true;
        v.commit_timeline(before, cx);
        v.timeline.selected = [b.clone(), title.clone()].into();
        v.timeline.primary_clip = Some(b.clone());
        v.timeline.inspector_tab = crate::timeline_state::InspectorTab::Basic;
    });
    cx.run_until_parked();
    click(cx, "text-bold");
    assert!(desktop.read_with(cx, |v, _| v.notice.is_none()));
    assert!(
        !store
            .load_timeline(&entry)
            .unwrap()
            .clips
            .iter()
            .find(|c| c.id == b)
            .unwrap()
            .text
            .as_ref()
            .unwrap()
            .style
            .bold
    );
    click(cx, "inspector-captions-tab");
    click(cx, "caption-delete-0");
    assert!(
        store
            .load_timeline(&entry)
            .unwrap()
            .clips
            .iter()
            .any(|c| c.id == b)
    );
    desktop.update_in(cx, |v, _, cx| {
        let before = v.timeline.model.clone();
        v.timeline
            .model
            .clips
            .iter_mut()
            .find(|c| c.id == b)
            .unwrap()
            .locked = false;
        v.commit_timeline(before, cx);
    });
    cx.run_until_parked();
    click(cx, "caption-delete-0");
    assert_eq!(store.load_timeline(&entry).unwrap().clips.len(), 1);
    assert!(desktop.read_with(cx, |v, _| v.timeline.inspector_tab
        == crate::timeline_state::InspectorTab::Captions));
    assert!(!desktop.read_with(cx, |v, _| v.captions_tab_visible()));
    desktop.update_in(cx, |v, _, cx| {
        v.timeline.selected.clear();
        v.timeline.primary_clip = None;
        cx.notify();
    });
    cx.run_until_parked();
    assert!(cx.debug_bounds("caption-list").is_some());
    desktop.update_in(cx, |v, _, cx| v.timeline_history(false, cx));
    assert_eq!(
        store
            .load_timeline(&entry)
            .unwrap()
            .clips
            .iter()
            .find(|c| c.id == b)
            .unwrap()
            .text
            .as_ref()
            .unwrap()
            .body,
        "第二组已编辑"
    );
    std::fs::remove_dir_all(root).unwrap();
}

/// 字体菜单浮在原布局上，滚动只移动菜单内容；选择字体与 Escape 都应正确关闭菜单。
#[gpui::test]
fn font_menu_scroll_does_not_move_inspector(cx: &mut TestAppContext) {
    let root = std::env::temp_dir().join(format!("dinuosi-font-scroll-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let store = ProjectStore::new(root.join("recent.json"));
    let entry = store.create("字体滚动", &root).unwrap();
    let (desktop, cx) =
        cx.add_window_view(|window, cx| Desktop::new(store.clone(), Some(entry), window, cx));
    cx.simulate_resize(size(px(1440.0), px(900.0)));
    cx.run_until_parked();
    click(cx, "filter-text");
    click(cx, "library-add-text");
    desktop.update_in(cx, |v, _, cx| {
        v.commit_inspector_input(cx);
        v.timeline.text.fonts = (0..100).map(|i| format!("Family {i:03}")).collect();
    });
    cx.run_until_parked();
    let before = cx.debug_bounds("text-size").unwrap();
    click(cx, "text-font-picker");
    assert_eq!(cx.debug_bounds("text-size").unwrap(), before);
    let first = cx.debug_bounds("text-font-option-0").unwrap();
    let center = cx.debug_bounds("text-font-list").unwrap().center();
    cx.simulate_event(gpui::ScrollWheelEvent {
        position: center,
        delta: gpui::ScrollDelta::Pixels(point(px(0.0), px(-60.0))),
        ..Default::default()
    });
    cx.run_until_parked();
    assert_eq!(cx.debug_bounds("text-size").unwrap(), before);
    assert!(cx.debug_bounds("text-font-option-0").unwrap().top() < first.top());
    click(cx, "text-font-option-3");
    assert!(desktop.read_with(cx, |v, _| !v.timeline.text.font_menu));
    assert_eq!(
        desktop.read_with(cx, |v, _| v
            .text_clip()
            .unwrap()
            .text
            .as_ref()
            .unwrap()
            .style
            .font
            .clone()),
        "Family 002"
    );
    click(cx, "text-font-picker");
    cx.simulate_keystrokes("escape");
    assert!(desktop.read_with(cx, |v, _| !v.timeline.text.font_menu));
    std::fs::remove_dir_all(root).unwrap();
}

/// 调色器通过真实鼠标拖动实时更新同组字幕，Escape 回滚；关闭仅保存一次，并可一次撤销/重做。
#[gpui::test]
fn color_picker_drag_cancel_commit_and_history(cx: &mut TestAppContext) {
    let root = std::env::temp_dir().join(format!("dinuosi-color-picker-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let store = ProjectStore::new(root.join("recent.json"));
    let entry = store.create("字幕取色", &root).unwrap();
    let (desktop, cx) = cx
        .add_window_view(|window, cx| Desktop::new(store.clone(), Some(entry.clone()), window, cx));
    cx.simulate_resize(size(px(1440.0), px(900.0)));
    cx.run_until_parked();
    click(cx, "filter-text");
    click(cx, "library-add-subtitle");
    desktop.update_in(cx, |v, window, cx| {
        v.session.timeline_position = 4.0;
        v.add_text_clip(true, None, window, cx);
        v.commit_inspector_input(cx);
    });
    cx.run_until_parked();
    let before = store.load_timeline(&entry).unwrap();
    for cancel in [true, false] {
        click(cx, "text-color-swatch");
        assert!(
            desktop.read_with(cx, |v, _| v.timeline.text.picker.is_some()
                && v.timeline.text.field.is_none())
        );
        let plane = cx.debug_bounds("text-color-plane").unwrap();
        cx.simulate_mouse_down(plane.center(), MouseButton::Left, Modifiers::none());
        cx.simulate_mouse_move(
            point(plane.right(), plane.top()),
            MouseButton::Left,
            Modifiers::none(),
        );
        cx.simulate_mouse_up(
            point(plane.right(), plane.top()),
            MouseButton::Left,
            Modifiers::none(),
        );
        cx.run_until_parked();
        assert!(desktop.read_with(cx, |v, _| {
            v.timeline
                .model
                .clips
                .iter()
                .all(|c| c.text.as_ref().unwrap().style.color == 0xff0000)
        }));
        assert_eq!(store.load_timeline(&entry).unwrap(), before);
        if cancel {
            cx.simulate_keystrokes("escape");
        } else {
            cx.simulate_click(point(px(20.0), px(100.0)), Modifiers::none());
        }
        cx.run_until_parked();
        if cancel {
            assert_eq!(
                desktop.read_with(cx, |v, _| v.timeline.model.clone()),
                before
            );
        }
    }
    let after = store.load_timeline(&entry).unwrap();
    assert!(
        after
            .clips
            .iter()
            .all(|c| c.text.as_ref().unwrap().style.color == 0xff0000)
    );
    desktop.update_in(cx, |v, _, cx| v.timeline_history(false, cx));
    assert_eq!(store.load_timeline(&entry).unwrap(), before);
    desktop.update_in(cx, |v, _, cx| v.timeline_history(true, cx));
    assert_eq!(store.load_timeline(&entry).unwrap(), after);
    click(cx, "text-color-swatch");
    let plane = cx.debug_bounds("text-color-plane").unwrap();
    cx.simulate_mouse_down(plane.center(), MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_up(plane.center(), MouseButton::Left, Modifiers::none());
    cx.run_until_parked();
    let backup = entry.path.with_extension("bak");
    std::fs::rename(&entry.path, &backup).unwrap();
    desktop.update_in(cx, |v, _, cx| {
        v.finish_text_color(false, cx);
        assert_eq!(v.timeline.model, after);
        assert!(v.notice.is_some());
    });
    std::fs::rename(backup, &entry.path).unwrap();
    assert_eq!(store.load_timeline(&entry).unwrap(), after);
    std::fs::remove_dir_all(root).unwrap();
}
