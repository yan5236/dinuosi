//! 刀片切割模式下拉菜单；沿用设置页灰阶，选择只更新草稿，应用后持久化。
use crate::{
    settings::Settings,
    ui::{LINE, PANEL, TEXT},
};
use dinuosi_icons::Icon;
use gpui::{Context, Div, MouseButton, div, prelude::*, px, rgb, svg};

impl Settings {
    /// 绘制刀片切割的两项下拉；支持鼠标选择、Enter/Space 展开、方向键选择与 Esc 收起。
    /// 菜单延后绘制避免被后续设置行覆盖；不直接保存偏好，离开或点击其他控件时由设置根层关闭。
    pub(crate) fn blade_mode_dropdown(&self, cx: &mut Context<Self>) -> Div {
        let label = if self.draft.blade_requires_selection {
            "Require clip selection"
        } else {
            "No clip selection required"
        };
        div()
            .relative()
            .w(px(220.0))
            .child(
                div()
                    .id("blade-mode")
                    .debug_selector(|| "blade-mode".into())
                    .focusable()
                    .flex()
                    .items_center()
                    .justify_between()
                    .px(px(12.0))
                    .h(px(36.0))
                    .rounded(px(4.0))
                    .bg(rgb(PANEL))
                    .border_1()
                    .border_color(rgb(LINE))
                    .cursor_pointer()
                    .hover(|s| s.bg(rgb(LINE)))
                    .child(crate::i18n::t("settings", label, cx))
                    .child(
                        svg()
                            .path(Icon::ChevronDown.path())
                            .size(px(16.0))
                            .text_color(rgb(TEXT)),
                    )
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.blade_menu_open = !this.blade_menu_open;
                        cx.notify();
                    }))
                    .on_key_down(cx.listener(|this, event: &gpui::KeyDownEvent, _, cx| {
                        if event.keystroke.modifiers.modified() {
                            return;
                        }
                        match event.keystroke.key.as_str() {
                            "enter" | "space" if !event.is_held => {
                                this.blade_menu_open = !this.blade_menu_open
                            }
                            "up" => this.draft.blade_requires_selection = false,
                            "down" => this.draft.blade_requires_selection = true,
                            "escape" if this.blade_menu_open => this.blade_menu_open = false,
                            _ => return,
                        }
                        cx.stop_propagation();
                        cx.notify();
                    })),
            )
            .when(self.blade_menu_open, |s| {
                s.child(
                    gpui::deferred(
                        div()
                            .debug_selector(|| "blade-mode-menu".into())
                            .absolute()
                            .top(px(40.0))
                            .left_0()
                            .w(px(220.0))
                            .p(px(4.0))
                            .rounded(px(4.0))
                            .bg(rgb(PANEL))
                            .border_1()
                            .border_color(rgb(LINE))
                            .occlude()
                            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                            .children([false, true].into_iter().map(|required| {
                                let label = if required {
                                    "Require clip selection"
                                } else {
                                    "No clip selection required"
                                };
                                div()
                                    .id(if required {
                                        "blade-mode-selected"
                                    } else {
                                        "blade-mode-direct"
                                    })
                                    .debug_selector(move || {
                                        if required {
                                            "blade-mode-selected".into()
                                        } else {
                                            "blade-mode-direct".into()
                                        }
                                    })
                                    .px(px(8.0))
                                    .py(px(9.0))
                                    .rounded(px(3.0))
                                    .cursor_pointer()
                                    .when(self.draft.blade_requires_selection == required, |s| {
                                        s.bg(rgb(LINE))
                                    })
                                    .hover(|s| s.bg(rgb(LINE)))
                                    .child(crate::i18n::t("settings", label, cx))
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.draft.blade_requires_selection = required;
                                        this.blade_menu_open = false;
                                        cx.notify();
                                    }))
                            })),
                    )
                    .with_priority(2),
                )
            })
    }
}
