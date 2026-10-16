//! 文本检查器：常用排版直接可见，字幕保留本组样式开关，逐句管理位于全局字幕页签。
use crate::{
    text_edit::TextField,
    ui::{LINE, MUTED, PANEL, TEXT},
    view::Desktop,
};
use dinuosi_core::{text::Alignment, timeline::Property};
use dinuosi_icons::Icon;
use gpui::{Context, Div, MouseButton, SharedString, div, prelude::*, px, rgb, svg};

impl Desktop {
    /// 按现有灰阶与检查器间距绘制正文、时间、排版和几何参数；小窗口通过主体滚动访问全部控件。
    /// 字幕额外显示本组样式范围；全局字幕列表不挤占当前排版参数区。
    pub(crate) fn text_inspector(&self, width: f32, enabled: bool, cx: &mut Context<Self>) -> Div {
        let clip = self.text_clip().unwrap();
        let enabled = enabled && !clip.locked && !self.timeline.model.tracks[clip.track].locked;
        let text = clip.text.as_ref().unwrap();
        let subtitle = text.group.is_some();
        let style = &text.style;
        let body = div().when_some(
            self.timeline.text.body.as_ref().filter(|e| e.id == clip.id),
            |s, e| s.child(e.input.clone()),
        );
        let editing = self
            .timeline
            .text
            .body
            .as_ref()
            .is_some_and(|e| e.id == clip.id);
        div().flex_1().min_h_0().child(
            div()
                .id("text-inspector")
                .debug_selector(|| "text-inspector".into())
                .size_full()
                .overflow_y_scroll()
                .p(px(18.0))
                .text_size(px(12.0))
                .when(subtitle, |s| {
                    s.child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.0))
                            .mb(px(12.0))
                            .child(crate::switch::Switch::new(
                                "text-apply-group",
                                self.timeline.text.apply_group,
                                cx.listener(|this, checked, _, cx| {
                                    this.commit_inspector_input(cx);
                                    this.timeline.text.apply_group = *checked;
                                    cx.notify();
                                }),
                            ))
                            .child(crate::i18n::t("text", "Apply to this subtitle group", cx)),
                    )
                })
                .child(body)
                .when(!editing, |s| {
                    s.child(
                        div()
                            .id("text-body-display")
                            .debug_selector(|| "text-body-display".into())
                            .h(px(116.0))
                            .w_full()
                            .p(px(8.0))
                            .rounded(px(4.0))
                            .border_1()
                            .border_color(rgb(LINE))
                            .bg(rgb(PANEL))
                            .overflow_hidden()
                            .cursor_text()
                            .child(text.body.clone())
                            .when(!enabled, |s| s.opacity(0.4))
                            .on_click(
                                cx.listener(|this, _, window, cx| this.begin_text_body(window, cx)),
                            ),
                    )
                })
                .child(div().h(px(12.0)))
                .child(self.text_field_row(TextField::Start, "Start", enabled, cx))
                .child(self.text_field_row(TextField::End, "End", enabled, cx))
                .child(div().h(px(8.0)))
                .child(self.text_field_row(TextField::Font, "Font", enabled, cx))
                .when(self.timeline.text.font_menu, |s| {
                    s.child(
                        div().relative().h_0().child(gpui::deferred(
                            div()
                                .id("text-font-list")
                                .debug_selector(|| "text-font-list".into())
                                .absolute()
                                .top_0()
                                .left_0()
                                .w(px(width - 36.0))
                                .bg(rgb(PANEL))
                                .occlude()
                                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                                .max_h(px(180.0))
                                .overflow_y_scroll()
                                .border_1()
                                .border_color(rgb(LINE))
                                .children(
                                    std::iter::once(String::new())
                                        .chain(self.timeline.text.fonts.iter().cloned())
                                        .enumerate()
                                        .map(|(i, name)| {
                                            let label = if name.is_empty() {
                                                crate::i18n::t("text", "System", cx).to_owned()
                                            } else {
                                                name.clone()
                                            };
                                            div()
                                                .id(SharedString::from(format!(
                                                    "text-font-option-{i}"
                                                )))
                                                .debug_selector(move || {
                                                    format!("text-font-option-{i}")
                                                })
                                                .h(px(26.0))
                                                .px(px(8.0))
                                                .overflow_hidden()
                                                .whitespace_nowrap()
                                                .cursor_pointer()
                                                .hover(|s| s.bg(rgb(LINE)))
                                                .child(label)
                                                .on_click(cx.listener(move |this, _, _, cx| {
                                                    this.change_text_style(
                                                        |s| s.font = name.clone(),
                                                        cx,
                                                    );
                                                }))
                                        }),
                                ),
                        )),
                    )
                })
                .child(self.text_field_row(TextField::Size, "Font size", enabled, cx))
                .child(
                    div()
                        .flex()
                        .gap(px(6.0))
                        .my(px(6.0))
                        .child(self.text_style_button(
                            "text-bold",
                            "B",
                            style.bold,
                            enabled,
                            cx,
                            |this, cx| this.change_text_style(|s| s.bold = !s.bold, cx),
                        ))
                        .child(self.text_style_button(
                            "text-italic",
                            "I",
                            style.italic,
                            enabled,
                            cx,
                            |this, cx| this.change_text_style(|s| s.italic = !s.italic, cx),
                        ))
                        .children(
                            [
                                (Alignment::Left, "Left", "text-align-left"),
                                (Alignment::Center, "Center", "text-align-center"),
                                (Alignment::Right, "Right", "text-align-right"),
                            ]
                            .into_iter()
                            .map(|(a, label, id)| {
                                self.text_style_button(
                                    id,
                                    crate::i18n::t("text", label, cx),
                                    style.alignment == a,
                                    enabled,
                                    cx,
                                    move |this, cx| this.change_text_style(|s| s.alignment = a, cx),
                                )
                            }),
                        ),
                )
                .child(self.text_field_row(TextField::Color, "Color", enabled, cx))
                .child(self.text_outline_controls(enabled, cx))
                .child(self.text_field_row(TextField::Width, "Text width", enabled, cx))
                .child(self.text_field_row(TextField::LineHeight, "Line spacing", enabled, cx))
                .child(self.text_effect_controls(enabled, cx))
                .child(self.text_preset_save(enabled, cx))
                .child(div().h(px(12.0)))
                .child(self.inspector_row(
                    crate::i18n::t("inspector", "Opacity", cx),
                    Property::Opacity,
                    enabled,
                    cx,
                ))
                .child(self.inspector_row(
                    crate::i18n::t("inspector", "Scale", cx),
                    Property::Scale,
                    enabled,
                    cx,
                ))
                .child(self.position_row(width, enabled, cx))
                .child(self.inspector_row(
                    crate::i18n::t("inspector", "Rotation", cx),
                    Property::Rotation,
                    enabled,
                    cx,
                )),
        )
    }

    /// 用相同字段行展示字体、排版和精确时间；颜色块打开调色器，旁边保留 RGB 精确输入。
    /// 字体箭头提供本机系列浮层并恢复键盘焦点，其余字段点击进入原生输入。
    pub(crate) fn text_field_row(
        &self,
        field: TextField,
        label: &'static str,
        enabled: bool,
        cx: &mut Context<Self>,
    ) -> Div {
        let clip = self.text_clip().unwrap();
        let value = field.value(clip, self.timeline.model.fps);
        let input = self
            .timeline
            .text
            .field
            .as_ref()
            .filter(|e| e.id == clip.id && e.field == field);
        div()
            .h(px(34.0))
            .flex()
            .items_center()
            .gap(px(8.0))
            .child(
                div()
                    .w(px(63.0))
                    .flex_none()
                    .text_color(rgb(MUTED))
                    .child(crate::i18n::t("text", label, cx)),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .when_some(input, |s, edit| s.child(edit.input.clone()))
                    .when(input.is_none(), |s| {
                        s.child(
                            div()
                                .id(field.id())
                                .debug_selector(move || field.id().into())
                                .h(px(25.0))
                                .px(px(7.0))
                                .flex()
                                .items_center()
                                .gap(px(5.0))
                                .bg(rgb(0x202023))
                                .rounded(px(3.0))
                                .overflow_hidden()
                                .cursor_text()
                                .when_some(
                                    field.color(&clip.text.as_ref().unwrap().style),
                                    |s, color| {
                                        s.child(
                                            div()
                                                .id(SharedString::from(format!(
                                                    "{}-swatch",
                                                    field.id()
                                                )))
                                                .debug_selector(move || {
                                                    format!("{}-swatch", field.id())
                                                })
                                                .w(px(34.0))
                                                .h(px(19.0))
                                                .flex_none()
                                                .bg(rgb(color))
                                                .border_1()
                                                .border_color(rgb(LINE))
                                                .cursor_pointer()
                                                .on_click(cx.listener(
                                                    move |this, event: &gpui::ClickEvent, window, cx| {
                                                        if enabled {
                                                            this.begin_text_color(
                                                                field,
                                                                event.position(),
                                                                window,
                                                                cx,
                                                            );
                                                        }
                                                        cx.stop_propagation();
                                                    },
                                                )),
                                        )
                                    },
                                )
                                .child(
                                    div()
                                        .flex_1()
                                        .min_w_0()
                                        .whitespace_nowrap()
                                        .overflow_hidden()
                                        .child(if field == TextField::Font && value.is_empty() {
                                            crate::i18n::t("text", "System", cx).to_owned()
                                        } else {
                                            value.clone()
                                        }),
                                )
                                .when(!enabled, |s| s.opacity(0.4))
                                .on_click(cx.listener(move |this, _, window, cx| {
                                    this.begin_text_field(field, window, cx)
                                })),
                        )
                    }),
            )
            .when(field == TextField::Font, |s| {
                s.child(
                    div()
                        .id("text-font-picker")
                        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                        .debug_selector(|| "text-font-picker".into())
                        .cursor_pointer()
                        .size(px(22.0))
                        .child(
                            svg()
                                .path(Icon::ChevronDown.path())
                                .size(px(16.0))
                                .text_color(rgb(TEXT)),
                        )
                        .on_click(cx.listener(move |this, _, window, cx| {
                            if !enabled {
                                return;
                            }
                            let open = !this.timeline.text.font_menu;
                            this.commit_inspector_input(cx);
                            if open && this.timeline.text.fonts.is_empty() {
                                match dinuosi_core::text_render::font_names() {
                                    Ok(fonts) => this.timeline.text.fonts = fonts,
                                    Err(error) => this.notice = Some(error),
                                }
                            }
                            this.timeline.text.font_menu = open;
                            this.focus_handle.focus(window);
                            cx.notify();
                        })),
                )
            })
    }

    /// 绘制紧凑的排版选项，选中态沿用灰阶；禁用时不提交任何样式修改。
    fn text_style_button(
        &self,
        id: &'static str,
        label: &'static str,
        active: bool,
        enabled: bool,
        cx: &mut Context<Self>,
        action: impl Fn(&mut Self, &mut Context<Self>) + 'static,
    ) -> gpui::Stateful<Div> {
        div()
            .id(id)
            .debug_selector(move || id.into())
            .h(px(25.0))
            .min_w(px(28.0))
            .px(px(7.0))
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(3.0))
            .border_1()
            .border_color(rgb(if active { TEXT } else { LINE }))
            .when(active, |s| s.bg(rgb(LINE)))
            .cursor_pointer()
            .child(label)
            .on_click(cx.listener(move |this, _, _, cx| {
                if enabled {
                    action(this, cx);
                }
            }))
    }
}
