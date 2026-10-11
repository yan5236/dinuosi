//! 标记编辑模态表单：名称与颜色仅在确认时一起保存，取消或关闭不修改项目。
use crate::{
    input::TextInput,
    ui::{LINE, MUTED, TEXT, pill, tool},
    view::Desktop,
};
use dinuosi_icons::Icon;
use gpui::{
    Context, Div, Entity, FocusHandle, MouseButton, Window, div, prelude::*, px, rgb, rgba,
};

pub(crate) const COLORS: [u32; 8] = [
    0xe3b85b, 0xe36b6b, 0xe69bc2, 0xad8bdb, 0x6d9ddd, 0x66c9cf, 0x7dbd82, 0xc7c7cc,
];
pub struct MarkerDialog {
    id: String,
    pub(crate) name: Entity<TextInput>,
    pub(crate) color: u32,
    error: Option<String>,
    colors_focus: FocusHandle,
    cancel_focus: FocusHandle,
    submit_focus: FocusHandle,
}
impl Desktop {
    /// 从右键目标初始化名称/颜色草稿，停止手势与播放并聚焦原生输入；不提前修改或保存标记。
    pub(crate) fn begin_marker_edit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        let Some(menu) = self.timeline.marker_menu.take() else {
            return;
        };
        let Some(marker) = self
            .timeline
            .model
            .markers
            .iter()
            .find(|m| m.id == menu.id)
            .cloned()
        else {
            return;
        };
        self.end_timeline_pointer(window, cx);
        self.stop_timeline();
        let name = cx.new(|cx| TextInput::new(marker.name, cx));
        name.read(cx).focus.focus(window);
        self.timeline.marker_dialog = Some(MarkerDialog {
            id: marker.id,
            name,
            color: marker.color,
            error: None,
            colors_focus: cx.focus_handle(),
            cancel_focus: cx.focus_handle(),
            submit_focus: cx.focus_handle(),
        });
        cx.notify();
    }
    /// 取消、关闭或 Escape 放弃整个名称/颜色草稿，恢复工作区焦点；不会保存或记录历史。
    pub(crate) fn cancel_marker_edit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.timeline.marker_dialog = None;
        self.focus_handle.focus(window);
        cx.notify();
    }
    /// 确认时先校验最新模型并原子保存，再发布名称/颜色和一条历史；非法输入或保存失败保留弹窗及原项目。
    pub(crate) fn submit_marker_edit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        let Some(draft) = &self.timeline.marker_dialog else {
            return;
        };
        let before = self.timeline.model.clone();
        let mut next = before.clone();
        let result = next
            .edit_marker(
                &draft.id,
                Some(draft.name.read(cx).text.clone()),
                Some(draft.color),
            )
            .and_then(|()| {
                if next == before {
                    return Ok(());
                }
                let entry = self.current.as_ref().ok_or("没有打开项目")?;
                self.store.save_timeline(entry, &next)
            });
        if let Err(error) = result {
            self.timeline.marker_dialog.as_mut().unwrap().error = Some(error);
        } else {
            self.timeline.model = next;
            self.timeline.history.record(before, &self.timeline.model);
            self.timeline.marker_dialog = None;
            self.focus_handle.focus(window);
            self.refresh_timeline(cx);
        }
        cx.notify();
    }
    /// 模态输入期间屏蔽剪辑快捷键；Tab 在名称、色板、取消和确定间循环，Enter 确认，Escape 放弃，保留 IME 合成。
    pub(crate) fn marker_dialog_key(
        &mut self,
        event: &gpui::KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(dialog) = &self.timeline.marker_dialog else {
            return;
        };
        if dialog.name.read(cx).is_composing() {
            return;
        }
        let key = event.keystroke.key.as_str();
        if key == "tab" {
            let handles = [
                dialog.name.read(cx).focus.clone(),
                dialog.colors_focus.clone(),
                dialog.cancel_focus.clone(),
                dialog.submit_focus.clone(),
            ];
            let current = handles
                .iter()
                .position(|h| h.is_focused(window))
                .unwrap_or(0);
            let next = (current
                + if event.keystroke.modifiers.shift {
                    handles.len() - 1
                } else {
                    1
                })
                % handles.len();
            handles[next].focus(window);
        } else if key == "escape" {
            self.cancel_marker_edit(window, cx);
        } else if !event.keystroke.modifiers.modified()
            && !event.is_held
            && (key == "enter"
                || (key == "space" && !dialog.name.read(cx).focus.is_focused(window)))
        {
            if dialog.cancel_focus.is_focused(window) {
                self.cancel_marker_edit(window, cx);
            } else {
                self.submit_marker_edit(window, cx);
            }
        } else if dialog.colors_focus.is_focused(window)
            && matches!(key, "left" | "right" | "up" | "down")
        {
            let current = COLORS.iter().position(|c| *c == dialog.color).unwrap_or(0);
            let step = if matches!(key, "left" | "up") {
                COLORS.len() - 1
            } else {
                1
            };
            self.timeline.marker_dialog.as_mut().unwrap().color =
                COLORS[(current + step) % COLORS.len()];
            cx.notify();
        } else {
            return;
        }
        cx.stop_propagation();
    }
    /// 绘制现有灰阶模态样式，名称输入与色板位于同一弹窗；色块点击只改草稿，确认后才关闭并保存。
    pub(crate) fn marker_dialog_view(&self, cx: &mut Context<Self>) -> gpui::Stateful<Div> {
        let draft = self
            .timeline
            .marker_dialog
            .as_ref()
            .expect("标记表单存在时绘制");
        div()
            .id("marker-dialog-overlay")
            .debug_selector(|| "marker-dialog-overlay".into())
            .absolute()
            .inset_0()
            .occlude()
            .flex()
            .items_center()
            .justify_center()
            .bg(rgba(0x00000080))
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_mouse_down(MouseButton::Right, |_, _, cx| cx.stop_propagation())
            .child(
                div()
                    .w(px(480.0))
                    .p(px(26.0))
                    .rounded(px(12.0))
                    .bg(rgb(0x2b2b2e))
                    .border_1()
                    .border_color(rgb(LINE))
                    .flex()
                    .flex_col()
                    .gap(px(20.0))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_between()
                            .child(div().text_size(px(20.0)).child(crate::i18n::t(
                                "dialogs",
                                "Edit marker",
                                cx,
                            )))
                            .child(tool("marker-dialog-close", Icon::Close, true).on_click(
                                cx.listener(|this, _, window, cx| {
                                    this.cancel_marker_edit(window, cx)
                                }),
                            )),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(8.0))
                            .child(
                                div()
                                    .text_size(px(12.0))
                                    .text_color(rgb(MUTED))
                                    .child(crate::i18n::t("dialogs", "Marker name", cx)),
                            )
                            .child(
                                div()
                                    .debug_selector(|| "marker-dialog-name".into())
                                    .child(draft.name.clone()),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(8.0))
                            .child(
                                div()
                                    .text_size(px(12.0))
                                    .text_color(rgb(MUTED))
                                    .child(crate::i18n::t("dialogs", "Color", cx)),
                            )
                            .child(
                                div()
                                    .id("marker-dialog-colors")
                                    .track_focus(&draft.colors_focus)
                                    .flex()
                                    .gap(px(12.0))
                                    .children(COLORS.into_iter().map(|color| {
                                        div()
                                            .id(gpui::SharedString::from(format!(
                                                "marker-color-{color:06x}"
                                            )))
                                            .debug_selector(move || {
                                                format!("marker-color-{color:06x}")
                                            })
                                            .size(px(32.0))
                                            .rounded(px(4.0))
                                            .bg(rgb(color))
                                            .border_2()
                                            .border_color(rgb(if color == draft.color {
                                                TEXT
                                            } else {
                                                LINE
                                            }))
                                            .cursor_pointer()
                                            .on_click(cx.listener(move |this, _, window, cx| {
                                                if let Some(draft) =
                                                    &mut this.timeline.marker_dialog
                                                {
                                                    draft.color = color;
                                                    draft.colors_focus.focus(window);
                                                    cx.notify();
                                                }
                                            }))
                                    })),
                            ),
                    )
                    .children(draft.error.as_ref().map(|error| {
                        div()
                            .text_size(px(12.0))
                            .text_color(rgb(0xe0a7a7))
                            .child(crate::i18n::error(error, cx))
                    }))
                    .child(
                        div()
                            .flex()
                            .justify_end()
                            .gap(px(12.0))
                            .mt(px(4.0))
                            .child(
                                pill(
                                    "marker-dialog-cancel",
                                    crate::i18n::t("dialogs", "Cancel", cx),
                                    90.0,
                                    34.0,
                                )
                                .track_focus(&draft.cancel_focus)
                                .focus(|s| s.border_color(rgb(0x64646a)))
                                .on_click(cx.listener(
                                    |this, _, window, cx| this.cancel_marker_edit(window, cx),
                                )),
                            )
                            .child(
                                pill(
                                    "marker-dialog-submit",
                                    crate::i18n::t("dialogs", "Confirm", cx),
                                    90.0,
                                    34.0,
                                )
                                .track_focus(&draft.submit_focus)
                                .focus(|s| s.border_color(rgb(0x64646a)))
                                .on_click(cx.listener(
                                    |this, _, window, cx| this.submit_marker_edit(window, cx),
                                )),
                            ),
                    ),
            )
    }
}
