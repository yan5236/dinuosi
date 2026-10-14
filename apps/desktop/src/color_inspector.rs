//! 调色检查器：统一工具导航、克制的参数层级与按需校正步骤。
use crate::{
    color_controls::section,
    color_steps::{button, disclosure},
    switch::Switch,
    tooltip::Tooltip,
    ui::{MUTED, TEXT},
    view::Desktop,
};
use dinuosi_core::color::Control;
use dinuosi_icons::Icon;
use gpui::{Context, Div, div, prelude::*, px, rgb};
pub(crate) const BASIC: [&str; 10] = [
    "Exposure",
    "Contrast",
    "Temperature",
    "Tint",
    "Saturation",
    "Vibrance",
    "Highlights",
    "Shadows",
    "Whites",
    "Blacks",
];
pub(crate) const TOOLS: [&str; 5] = ["Basic", "HSL", "Curves", "Wheels", "Local"];
pub(crate) const COLORS: [u32; 8] = [
    0xe05656, 0xdf963f, 0xdbcd50, 0x65b866, 0x58c4c8, 0x6388d8, 0x9562d0, 0xce67bd,
];
impl Desktop {
    /// 固定总开关和五个工具页签；步骤管理默认收起，参数滚动区与底部工具始终分离。
    /// 所有操作沿用真实片段数据和原子事务，锁定时保留可读参数，不显示教学或装饰文案。
    pub(crate) fn color_inspector(&self, cx: &mut Context<Self>) -> Div {
        let Some(c) = self.color_clip() else {
            return div();
        };
        let enabled = self.color_enabled();
        let grade = &c.color;
        let paste_enabled = !self.busy
            && self.timeline.color.clipboard.is_some()
            && self.timeline.model.clips.iter().any(|c| {
                self.timeline.selected.contains(&c.id)
                    && c.kind != dinuosi_core::media::MediaKind::Audio
                    && !c.locked
                    && !self.timeline.model.tracks[c.track].locked
            });
        div()
            .debug_selector(|| "color-inspector".into())
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .child(
                div()
                    .h(px(44.0))
                    .flex_none()
                    .px(px(16.0))
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(6.0))
                            .opacity(if enabled { 1.0 } else { 0.4 })
                            .text_size(px(12.0))
                            .text_color(rgb(TEXT))
                            .child(Switch::new(
                                "color-enabled",
                                grade.enabled,
                                cx.listener(move |this, checked: &bool, _, cx| {
                                    if enabled {
                                        this.edit_color(|g, _| g.enabled = *checked, cx);
                                    }
                                }),
                            ))
                            .child(crate::i18n::t("color", "Enabled", cx)),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(4.0))
                            .child(button("color-copy", "Copy", false, true, cx).on_click(
                                cx.listener(|this, _, _, cx| {
                                    this.commit_inspector_input(cx);
                                    this.timeline.color.clipboard = this.color_clip().map(|c| {
                                        c.color_at(this.keyframe_frame() as f64).into_owned()
                                    });
                                    cx.notify();
                                }),
                            ))
                            .child(
                                button("color-paste", "Paste", false, paste_enabled, cx).on_click(
                                    cx.listener(move |this, _, _, cx| {
                                        if paste_enabled {
                                            this.paste_color(cx);
                                        }
                                    }),
                                ),
                            )
                            .child(
                                Tooltip::tool(
                                    "color-reset",
                                    Icon::ResetParameters,
                                    enabled,
                                    crate::i18n::t("color", "Reset", cx),
                                )
                                .on_click(cx.listener(
                                    move |this, _, _, cx| {
                                        if enabled {
                                            this.reset_color(true, false, cx);
                                            this.timeline.color.stage = 0;
                                        }
                                    },
                                )),
                            ),
                    ),
            )
            .child(
                div()
                    .h(px(38.0))
                    .flex_none()
                    .flex()
                    .mx(px(16.0))
                    .border_b_1()
                    .border_color(rgb(0x353539))
                    .children(TOOLS.into_iter().enumerate().map(|(i, label)| {
                        let id = format!("color-tool-{i}");
                        let selector = id.clone();
                        div()
                            .id(gpui::ElementId::Name(id.into()))
                            .debug_selector(move || selector.clone())
                            .flex_1()
                            .h_full()
                            .flex()
                            .items_center()
                            .justify_center()
                            .text_size(px(12.0))
                            .text_color(rgb(if self.timeline.color.tool == i {
                                TEXT
                            } else {
                                MUTED
                            }))
                            .border_b_2()
                            .border_color(if self.timeline.color.tool == i {
                                rgb(TEXT).into()
                            } else {
                                gpui::transparent_black()
                            })
                            .cursor_pointer()
                            .child(crate::i18n::t("color", label, cx))
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.commit_inspector_input(cx);
                                this.timeline.color.tool = i;
                                this.focus_handle.focus(window);
                                cx.notify();
                            }))
                    })),
            )
            .when(self.timeline.color.steps_open, |s| {
                s.child(self.color_steps(cx))
            })
            .child(
                div()
                    .id("color-parameters")
                    .debug_selector(|| "color-parameters".into())
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .px(px(20.0))
                    .py(px(14.0))
                    .child(match self.timeline.color.tool {
                        1 => self.color_hsl(cx),
                        2 => self.color_curves(cx),
                        3 => self.color_wheels(cx),
                        4 => self.color_local(cx),
                        _ => self.color_basic(cx),
                    })
                    .when(self.timeline.color.scopes, |s| {
                        s.child(self.color_scopes(cx))
                    }),
            )
            .child(
                div()
                    .h(px(36.0))
                    .flex_none()
                    .px(px(16.0))
                    .flex()
                    .items_center()
                    .justify_between()
                    .border_t_1()
                    .border_color(rgb(0x353539))
                    .child(
                        disclosure("color-steps", "Steps", self.timeline.color.steps_open, cx)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.commit_inspector_input(cx);
                                this.timeline.color.steps_open = !this.timeline.color.steps_open;
                                cx.notify();
                            })),
                    )
                    .child(
                        disclosure("color-scopes", "Scopes", self.timeline.color.scopes, cx)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.timeline.color.scopes = !this.timeline.color.scopes;
                                if !this.timeline.color.scopes
                                    && let Some(old) = this.timeline.color.scope_image.take()
                                {
                                    this.session.retired_frames.push(old);
                                }
                                this.timeline.preview = true;
                                this.refresh_timeline(cx);
                            })),
                    ),
            )
    }
    /// 基础页按曝光和色彩校正分组；明暗细节默认折叠，常用六项保持完整可见。
    fn color_basic(&self, cx: &mut Context<Self>) -> Div {
        div()
            .child(section("Light", cx))
            .children(
                [0, 1]
                    .into_iter()
                    .map(|i| self.color_row(Control::Basic(i), BASIC[i], cx)),
            )
            .child(
                div().mt(px(16.0)).child(
                    disclosure(
                        "color-tone-details",
                        "Tonal detail",
                        self.timeline.color.tones_open,
                        cx,
                    )
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.commit_inspector_input(cx);
                        this.timeline.color.tones_open = !this.timeline.color.tones_open;
                        cx.notify();
                    })),
                ),
            )
            .when(self.timeline.color.tones_open, |s| {
                s.children((6..10).map(|i| self.color_row(Control::Basic(i), BASIC[i], cx)))
            })
            .child(section("Color balance", cx).mt(px(16.0)))
            .children(
                [2, 3, 4, 5]
                    .into_iter()
                    .map(|i| self.color_row(Control::Basic(i), BASIC[i], cx)),
            )
    }
    /// 空间与颜色选区按功能分组，默认只显示开关，启用后展开必要控件，不堆叠停用参数。
    fn color_local(&self, cx: &mut Context<Self>) -> Div {
        let grade = self.color_grade();
        let stage = &grade.stages[self.color_stage()];
        let enabled = self.color_enabled();
        div()
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .mb(px(10.0))
                    .child(section("Mask", cx))
                    .child(Switch::new(
                        "color-mask-enabled",
                        stage.mask.enabled,
                        cx.listener(move |this, checked: &bool, _, cx| {
                            if enabled {
                                this.edit_color(|g, i| g.stages[i].mask.enabled = *checked, cx);
                            }
                        }),
                    )),
            )
            .when(stage.mask.enabled, |s| {
                s.child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(12.0))
                        .mb(px(10.0))
                        .child(
                            button(
                                "color-mask-ellipse",
                                "Ellipse",
                                stage.mask.ellipse,
                                enabled,
                                cx,
                            )
                            .on_click(cx.listener(
                                move |this, _, _, cx| {
                                    if enabled {
                                        this.edit_color(|g, i| g.stages[i].mask.ellipse = true, cx);
                                    }
                                },
                            )),
                        )
                        .child(
                            button(
                                "color-mask-rectangle",
                                "Rectangle",
                                !stage.mask.ellipse,
                                enabled,
                                cx,
                            )
                            .on_click(cx.listener(
                                move |this, _, _, cx| {
                                    if enabled {
                                        this.edit_color(
                                            |g, i| g.stages[i].mask.ellipse = false,
                                            cx,
                                        );
                                    }
                                },
                            )),
                        )
                        .child(div().flex_1())
                        .child(
                            button(
                                "color-mask-invert",
                                "Invert",
                                stage.mask.inverted,
                                enabled,
                                cx,
                            )
                            .on_click(cx.listener(
                                move |this, _, _, cx| {
                                    if enabled {
                                        this.edit_color(
                                            |g, i| {
                                                g.stages[i].mask.inverted =
                                                    !g.stages[i].mask.inverted
                                            },
                                            cx,
                                        );
                                    }
                                },
                            )),
                        ),
                )
                .children(
                    [
                        "Center X", "Center Y", "Width", "Height", "Rotation", "Feather",
                    ]
                    .into_iter()
                    .enumerate()
                    .map(|(i, label)| self.color_row(Control::Mask(i), label, cx)),
                )
            })
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .mt(px(18.0))
                    .mb(px(10.0))
                    .pt(px(12.0))
                    .border_t_1()
                    .border_color(rgb(0x353539))
                    .child(section("Color range", cx))
                    .child(Switch::new(
                        "color-qualifier-enabled",
                        stage.qualifier.enabled,
                        cx.listener(move |this, checked: &bool, _, cx| {
                            if enabled {
                                this.edit_color(
                                    |g, i| g.stages[i].qualifier.enabled = *checked,
                                    cx,
                                );
                            }
                        }),
                    )),
            )
            .when(stage.qualifier.enabled, |s| {
                s.children(
                    [
                        "Hue center",
                        "Hue width",
                        "Softness",
                        "Min saturation",
                        "Min luma",
                        "Max luma",
                    ]
                    .into_iter()
                    .enumerate()
                    .map(|(i, label)| self.color_row(Control::Qualifier(i), label, cx)),
                )
            })
    }
}
