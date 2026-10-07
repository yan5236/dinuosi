//! 通用设置的两项模式下拉；共享灰阶外观与交互，选择只更新草稿，应用后持久化。
use crate::{
    settings::Settings,
    ui::{LINE, PANEL, TEXT},
};
use dinuosi_icons::Icon;
use gpui::{Context, Div, MouseButton, div, prelude::*, px, rgb, svg};

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum GeneralDropdown {
    Blade,
    ClipDrag,
}

impl GeneralDropdown {
    /// 返回对应模式控件的稳定 ID，用于焦点、菜单和指针验证，不作为可见文案。
    fn id(self) -> &'static str {
        match self {
            Self::Blade => "blade-mode",
            Self::ClipDrag => "clip-drag-mode",
        }
    }

    /// 返回 false/true 两项的稳定 ID 与英文翻译键；拖动模式先列跟随鼠标，再列保留原块。
    fn options(self) -> [(&'static str, &'static str); 2] {
        match self {
            Self::Blade => [
                ("blade-mode-direct", "No clip selection required"),
                ("blade-mode-selected", "Require clip selection"),
            ],
            Self::ClipDrag => [
                ("clip-drag-follow", "Follow mouse"),
                ("clip-drag-keep", "Keep in timeline"),
            ],
        }
    }

    /// 读取传入偏好中对应模式的布尔值，只读取草稿或已保存的偏好，不修改状态。
    fn value(self, data: &dinuosi_core::Preferences) -> bool {
        match self {
            Self::Blade => data.blade_requires_selection,
            Self::ClipDrag => data.keep_dragged_clip,
        }
    }

    /// 更新传入偏好中对应模式的布尔值；调用方只传设置草稿，磁盘与全局应用值不受影响。
    fn set_value(self, data: &mut dinuosi_core::Preferences, value: bool) {
        match self {
            Self::Blade => data.blade_requires_selection = value,
            Self::ClipDrag => data.keep_dragged_clip = value,
        }
    }
}

impl Settings {
    /// 绘制指定通用模式的两项下拉；支持鼠标选择、Enter/Space 展开、方向键选择与 Esc 收起。
    /// 菜单延后绘制避免被后续设置行覆盖；不直接保存偏好，离开或点击其他控件时由设置根层关闭。
    pub(crate) fn mode_dropdown(&self, kind: GeneralDropdown, cx: &mut Context<Self>) -> Div {
        let id = kind.id();
        let options = kind.options();
        let label = options[usize::from(kind.value(&self.draft))].1;
        div()
            .relative()
            .w(px(220.0))
            .child(
                div()
                    .id(id)
                    .debug_selector(move || id.into())
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
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.general_menu = (this.general_menu != Some(kind)).then_some(kind);
                        cx.notify();
                    }))
                    .on_key_down(cx.listener(move |this, event: &gpui::KeyDownEvent, _, cx| {
                        if event.keystroke.modifiers.modified() {
                            return;
                        }
                        match event.keystroke.key.as_str() {
                            "enter" | "space" if !event.is_held => {
                                this.general_menu =
                                    (this.general_menu != Some(kind)).then_some(kind)
                            }
                            "up" => kind.set_value(&mut this.draft, false),
                            "down" => kind.set_value(&mut this.draft, true),
                            "escape" if this.general_menu.is_some() => this.general_menu = None,
                            _ => return,
                        }
                        cx.stop_propagation();
                        cx.notify();
                    })),
            )
            .when(self.general_menu == Some(kind), |s| {
                s.child(
                    gpui::deferred(
                        div()
                            .debug_selector(move || format!("{id}-menu"))
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
                            .children(options.into_iter().enumerate().map(
                                |(index, (id, label))| {
                                    let value = index == 1;
                                    div()
                                        .id(id)
                                        .debug_selector(move || id.into())
                                        .px(px(8.0))
                                        .py(px(9.0))
                                        .rounded(px(3.0))
                                        .cursor_pointer()
                                        .when(kind.value(&self.draft) == value, |s| s.bg(rgb(LINE)))
                                        .hover(|s| s.bg(rgb(LINE)))
                                        .child(crate::i18n::t("settings", label, cx))
                                        .on_click(cx.listener(move |this, _, _, cx| {
                                            kind.set_value(&mut this.draft, value);
                                            this.general_menu = None;
                                            cx.notify();
                                        }))
                                },
                            )),
                    )
                    .with_priority(2),
                )
            })
    }
}
