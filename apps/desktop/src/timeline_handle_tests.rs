//! 相邻片段的外置裁边把手必须覆盖片段内容，并优先接收真实指针事件。
use crate::{timeline_state::Gesture, view::Desktop};
use dinuosi_core::{ProjectStore, media::MediaKind};
use gpui::{Modifiers, MouseButton, TestAppContext, point, px, size};

/// 在首尾相接的两段图片上验证单选、多选及反向存储顺序的两侧把手命中。
/// 从邻片内部按下把手后缩短当前片段，检查磁盘结果只改变目标片段，撤销恢复原模型。
#[gpui::test]
fn adjacent_clip_handles_take_priority_over_clip_bodies(cx: &mut TestAppContext) {
    let root = std::env::temp_dir().join(format!("dinuosi-handle-ui-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let store = ProjectStore::new(root.join("recent.json"));
    let entry = store.create("把手层级", &root).unwrap();
    let (desktop, cx) = cx
        .add_window_view(|window, cx| Desktop::new(store.clone(), Some(entry.clone()), window, cx));
    cx.simulate_resize(size(px(1440.0), px(900.0)));
    let ids = desktop.update_in(cx, |v, _, cx| {
        v.zoom = 0.5;
        let ids: Vec<_> = (0..2)
            .map(|index| {
                v.timeline
                    .model
                    .insert(
                        (format!("handle-{index}.png").into(), MediaKind::Image),
                        format!("图片{index}"),
                        0,
                        index * 150,
                        0..150,
                        150,
                    )
                    .unwrap()
            })
            .collect();
        cx.notify();
        ids
    });
    for reversed in [false, true] {
        desktop.update_in(cx, |v, _, _| {
            if reversed {
                v.timeline.model.clips.reverse();
            }
        });
        for multiple in [false, true] {
            for (index, left) in [(0, false), (1, true)] {
                let id = &ids[index];
                desktop.update_in(cx, |v, _, cx| {
                    v.timeline.selected = if multiple {
                        ids.iter().cloned().collect()
                    } else {
                        [id.clone()].into()
                    };
                    cx.notify();
                });
                cx.run_until_parked();
                let before = desktop.read_with(cx, |v, _| v.timeline.model.clone());
                let handle = cx
                    .debug_bounds(Box::leak(format!("trim-{id}-{left}").into_boxed_str()))
                    .unwrap();
                let neighbor = cx
                    .debug_bounds(Box::leak(
                        format!("clip-{}", ids[1 - index]).into_boxed_str(),
                    ))
                    .unwrap();
                assert!(
                    neighbor.contains(&handle.center()),
                    "把手必须实际覆盖相邻素材才能复现遮挡"
                );
                cx.simulate_mouse_down(handle.center(), MouseButton::Left, Modifiers::none());
                assert!(
                    desktop.read_with(cx, |v, _| matches!(
                        &v.timeline.gesture, Some(Gesture::Trim { id: target, left: edge, .. })
                            if target == id && *edge == left
                    )),
                    "相邻素材不能抢走把手命中：multiple={multiple}, reversed={reversed}"
                );
                let end = handle.center() + point(px(if left { 24.0 } else { -24.0 }), px(0.0));
                cx.simulate_mouse_move(end, MouseButton::Left, Modifiers::none());
                cx.simulate_mouse_up(end, MouseButton::Left, Modifiers::none());
                cx.run_until_parked();
                let saved = store.load_timeline(&entry).unwrap();
                let original = before.clips.iter().find(|c| &c.id == id).unwrap();
                assert!(
                    saved.clips.iter().find(|c| &c.id == id).unwrap().duration < original.duration
                );
                assert_eq!(
                    saved.clips.iter().find(|c| c.id == ids[1 - index]),
                    before.clips.iter().find(|c| c.id == ids[1 - index])
                );
                cx.simulate_keystrokes("ctrl-z");
                cx.run_until_parked();
                assert_eq!(store.load_timeline(&entry).unwrap(), before);
            }
        }
    }
    std::fs::remove_dir_all(root).unwrap();
}
