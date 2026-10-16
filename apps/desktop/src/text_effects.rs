//! 艺术字常用效果，启用后展开字段，复用现有受控拨片和输入行。
use crate::{text_edit::TextField, ui::MUTED, view::Desktop};
use gpui::{Context, Div, div, prelude::*, px, rgb};
impl Desktop {
    /// 普通文本与字幕始终显示描边开关；零宽度关闭描边，开启后展开宽度和颜色。
    /// 复用已有样式事务以遵守锁定、字幕作用范围及撤销；关闭不清除描边颜色。
    pub(crate) fn text_outline_controls(&self, enabled: bool, cx: &mut Context<Self>) -> Div {
        let active = self
            .text_clip()
            .unwrap()
            .text
            .as_ref()
            .unwrap()
            .style
            .outline
            > 0;
        div()
            .child(
                div()
                    .h(px(36.0))
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .text_color(rgb(MUTED))
                            .child(crate::i18n::t("text", "Outline", cx)),
                    )
                    .child(crate::switch::Switch::new(
                        "text-outline-toggle",
                        active,
                        cx.listener(move |this, checked: &bool, _, cx| {
                            if enabled {
                                this.change_text_style(
                                    |s| s.outline = if *checked { 2 } else { 0 },
                                    cx,
                                );
                            }
                        }),
                    )),
            )
            .when(active, |s| {
                s.child(self.text_field_row(TextField::Outline, "Outline width", enabled, cx))
                    .child(self.text_field_row(
                        TextField::OutlineColor,
                        "Outline color",
                        enabled,
                        cx,
                    ))
            })
    }

    /// 绘制投影与圆角背景；所有修改遵守单句/本组样式范围和现有撤销事务。
    pub(crate) fn text_effect_controls(&self, enabled: bool, cx: &mut Context<Self>) -> Div {
        let style = &self.text_clip().unwrap().text.as_ref().unwrap().style;
        div().children(
            [
                (true, "Shadow", style.shadow.is_some()),
                (false, "Background", style.background.is_some()),
            ]
            .into_iter()
            .map(|(shadow, label, active)| {
                div()
                    .child(
                        div()
                            .h(px(36.0))
                            .flex()
                            .items_center()
                            .justify_between()
                            .child(
                                div()
                                    .text_color(rgb(MUTED))
                                    .child(crate::i18n::t("text", label, cx)),
                            )
                            .child(crate::switch::Switch::new(
                                if shadow {
                                    "text-shadow-toggle"
                                } else {
                                    "text-background-toggle"
                                },
                                active,
                                cx.listener(move |this, checked: &bool, _, cx| {
                                    if enabled {
                                        this.change_text_style(
                                            |s| {
                                                if shadow {
                                                    s.shadow = checked.then(Default::default);
                                                } else {
                                                    s.background = checked.then(Default::default);
                                                }
                                            },
                                            cx,
                                        );
                                    }
                                }),
                            )),
                    )
                    .when(active, |s| {
                        s.children(
                            (if shadow {
                                vec![
                                    (TextField::ShadowColor, "Shadow color"),
                                    (TextField::ShadowOpacity, "Shadow opacity"),
                                    (TextField::ShadowBlur, "Shadow blur"),
                                    (TextField::ShadowX, "Shadow X"),
                                    (TextField::ShadowY, "Shadow Y"),
                                ]
                            } else {
                                vec![
                                    (TextField::BackgroundColor, "Background color"),
                                    (TextField::BackgroundOpacity, "Background opacity"),
                                    (TextField::BackgroundPadding, "Padding"),
                                    (TextField::BackgroundRadius, "Corner radius"),
                                ]
                            })
                            .into_iter()
                            .map(|(field, label)| self.text_field_row(field, label, enabled, cx)),
                        )
                    })
            }),
        )
    }
}
