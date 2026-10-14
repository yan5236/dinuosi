//! 串行步骤管理按需展开，避免挤占默认调色操作区。
use crate::{
    tooltip::Tooltip,
    ui::{MUTED, TEXT},
    view::Desktop,
};
use dinuosi_core::color::Stage;
use dinuosi_icons::Icon;
use gpui::{Context, Div, Stateful, div, prelude::*, px, rgb, svg};

/// 绘制明确的展开/收起入口；箭头随状态变化，整行可点且可键盘聚焦，保留灰阶文字菜单。
pub(crate) fn disclosure(
    id: &'static str,
    label: &'static str,
    open: bool,
    cx: &mut Context<Desktop>,
) -> Stateful<Div> {
    div()
        .id(id)
        .debug_selector(move || id.into())
        .h(px(30.0))
        .px(px(6.0))
        .flex()
        .items_center()
        .gap(px(6.0))
        .text_size(px(12.0))
        .text_color(rgb(TEXT))
        .focusable()
        .tab_index(0)
        .cursor_pointer()
        .hover(|s| s.bg(rgb(0x353539)))
        .focus(|s| s.bg(rgb(0x353539)))
        .child(
            svg()
                .path(if open {
                    Icon::ChevronDown.path()
                } else {
                    Icon::ChevronRight.path()
                })
                .size(px(14.0))
                .text_color(rgb(TEXT)),
        )
        .child(crate::i18n::t("color", label, cx))
        .on_key_down(cx.listener(move |this, event: &gpui::KeyDownEvent, _, cx| {
            if event.is_held
                || event.keystroke.modifiers.modified()
                || !matches!(event.keystroke.key.as_str(), "enter" | "space")
            {
                return;
            }
            this.commit_inspector_input(cx);
            match id {
                "color-steps" => this.timeline.color.steps_open = !this.timeline.color.steps_open,
                "color-tone-details" => {
                    this.timeline.color.tones_open = !this.timeline.color.tones_open
                }
                "color-wheel-values" => {
                    this.timeline.color.wheel_values = !this.timeline.color.wheel_values
                }
                "color-scopes" => {
                    this.timeline.color.scopes = !this.timeline.color.scopes;
                    if !this.timeline.color.scopes
                        && let Some(old) = this.timeline.color.scope_image.take()
                    {
                        this.session.retired_frames.push(old);
                    }
                    this.timeline.preview = true;
                    this.refresh_timeline(cx);
                }
                _ => return,
            }
            cx.stop_propagation();
            cx.notify();
        }))
}

/// 绘制克制的文字操作项，选中项使用清晰文字和底线，保留现有菜单风格。
pub(crate) fn button(
    id: impl Into<String>,
    label: &'static str,
    active: bool,
    enabled: bool,
    cx: &gpui::App,
) -> Stateful<Div> {
    let id = id.into();
    let selector = id.clone();
    div()
        .id(gpui::ElementId::Name(id.into()))
        .debug_selector(move || selector.clone())
        .px(px(5.0))
        .py(px(5.0))
        .text_size(px(11.0))
        .whitespace_nowrap()
        .border_b_1()
        .border_color(if active {
            rgb(TEXT).into()
        } else {
            gpui::transparent_black()
        })
        .text_color(rgb(if active { TEXT } else { MUTED }))
        .opacity(if enabled { 1.0 } else { 0.35 })
        .when(enabled, |s| {
            s.cursor_pointer().hover(|s| s.text_color(rgb(TEXT)))
        })
        .child(crate::i18n::t("color", label, cx))
}
impl Desktop {
    /// 展开编号串行步骤与当前步骤的独立旁路、重置、删除；最多八步，不提供任意连线。
    /// 管理区独立于参数滚动，增加、删除及切换保留原有片段与单次历史语义。
    pub(crate) fn color_steps(&self, cx: &mut Context<Self>) -> Div {
        let enabled = self.color_enabled();
        let grade = &self.color_clip().unwrap().color;
        let current = self.color_stage();
        div()
            .flex_none()
            .px(px(20.0))
            .py(px(8.0))
            .bg(rgb(0x222225))
            .border_b_1()
            .border_color(rgb(0x353539))
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(div().flex().flex_wrap().gap(px(6.0)).children(
                        grade.stages.iter().enumerate().map(|(index, stage)| {
                            let id = format!("color-stage-{index}");
                            let selector = id.clone();
                            div()
                                .id(gpui::ElementId::Name(id.into()))
                                .debug_selector(move || selector.clone())
                                .w(px(32.0))
                                .h(px(28.0))
                                .flex()
                                .items_center()
                                .justify_center()
                                .font_family(crate::ui::MONO_FONT)
                                .text_size(px(12.0))
                                .border_1()
                                .border_color(rgb(if current == index {
                                    0x929297
                                } else {
                                    0x414145
                                }))
                                .bg(rgb(if current == index { 0x414145 } else { 0x29292d }))
                                .text_color(rgb(TEXT))
                                .opacity(if stage.enabled { 1.0 } else { 0.35 })
                                .cursor_pointer()
                                .child(format!("{:02}", index + 1))
                                .on_click(cx.listener(move |this, _, window, cx| {
                                    this.commit_inspector_input(cx);
                                    this.timeline.color.stage = index;
                                    this.focus_handle.focus(window);
                                    cx.notify();
                                }))
                        }),
                    ))
                    .child(
                        Tooltip::tool(
                            "color-stage-add",
                            Icon::Plus,
                            enabled && grade.stages.len() < 8,
                            crate::i18n::t("color", "Add step", cx),
                        )
                        .on_click(cx.listener(move |this, _, _, cx| {
                            if enabled {
                                this.edit_color(
                                    |g, _| {
                                        if g.stages.len() < 8 {
                                            g.stages.push(Stage::default());
                                        }
                                    },
                                    cx,
                                );
                                this.timeline.color.stage =
                                    this.color_clip().map_or(0, |c| c.color.stages.len() - 1);
                            }
                        })),
                    ),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_end()
                    .gap(px(5.0))
                    .mt(px(5.0))
                    .child(
                        Tooltip::tool(
                            "color-stage-enabled",
                            if grade.stages[current].enabled {
                                Icon::Eye
                            } else {
                                Icon::EyeClosed
                            },
                            enabled,
                            crate::i18n::t(
                                "color",
                                if grade.stages[current].enabled {
                                    "Bypass step"
                                } else {
                                    "Enable step"
                                },
                                cx,
                            ),
                        )
                        .on_click(cx.listener(move |this, _, _, cx| {
                            if enabled {
                                this.edit_color(
                                    |g, i| g.stages[i].enabled = !g.stages[i].enabled,
                                    cx,
                                );
                            }
                        })),
                    )
                    .child(
                        Tooltip::tool(
                            "color-stage-reset",
                            Icon::ResetParameters,
                            enabled,
                            crate::i18n::t("color", "Reset step", cx),
                        )
                        .on_click(cx.listener(move |this, _, _, cx| {
                            if enabled {
                                this.reset_color(false, false, cx);
                            }
                        })),
                    )
                    .child(
                        Tooltip::tool(
                            "color-stage-remove",
                            Icon::Trash,
                            enabled && grade.stages.len() > 1,
                            crate::i18n::t("color", "Remove step", cx),
                        )
                        .on_click(cx.listener(move |this, _, _, cx| {
                            if enabled {
                                this.reset_color(false, true, cx);
                            }
                        })),
                    ),
            )
    }
}
