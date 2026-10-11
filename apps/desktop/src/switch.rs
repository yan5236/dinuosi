//! 通用受控拨片开关：只绘制状态并报告新值，业务草稿和持久化由调用方管理。
use crate::ui::{LINE, MUTED, TEXT};
use gpui::{App, IntoElement, RenderOnce, SharedString, Window, div, prelude::*, px, rgb};
use std::rc::Rc;

type ChangeHandler = Rc<dyn Fn(&bool, &mut Window, &mut App)>;

#[derive(IntoElement)]
pub(crate) struct Switch {
    id: SharedString,
    checked: bool,
    on_change: ChangeHandler,
}

impl Switch {
    /// 创建受控 Switch；id 必须在当前视图唯一，checked 来自调用方当前状态。
    /// 鼠标或键盘激活时回调接收目标布尔值；组件不保存业务值，也不读写磁盘。
    pub(crate) fn new(
        id: impl Into<SharedString>,
        checked: bool,
        on_change: impl Fn(&bool, &mut Window, &mut App) + 'static,
    ) -> Self {
        Self {
            id: id.into(),
            checked,
            on_change: Rc::new(on_change),
        }
    }
}

impl RenderOnce for Switch {
    /// 绘制 44×24 灰阶胶囊与 18px 拨片，外层提供 48×32 点击区和可见焦点边框。
    /// 支持 Tab 聚焦、空格/Enter 切换；忽略组合键和长按重复，阻止激活事件冒泡。
    fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
        let checked = self.checked;
        let click = self.on_change.clone();
        let keyboard = self.on_change;
        let selector = self.id.clone();
        let thumb_selector = format!("{}-thumb", self.id);
        div()
            .id(self.id)
            .debug_selector(move || selector.to_string())
            .focusable()
            .tab_index(0)
            .flex_none()
            .w(px(48.0))
            .h(px(32.0))
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(16.0))
            .border_1()
            .border_color(gpui::transparent_black())
            .focus(|s| s.border_color(rgb(0x64646a)))
            .cursor_pointer()
            .hover(|s| s.bg(rgb(0x333337)))
            .active(|s| s.bg(rgb(LINE)))
            .on_click(move |_, window, cx| {
                cx.stop_propagation();
                click(&!checked, window, cx);
            })
            .on_key_down(move |event, window, cx| {
                if !event.keystroke.modifiers.modified()
                    && matches!(event.keystroke.key.as_str(), "space" | "enter")
                {
                    cx.stop_propagation();
                    if !event.is_held {
                        keyboard(&!checked, window, cx);
                    }
                }
            })
            .child(
                div()
                    .relative()
                    .w(px(44.0))
                    .h(px(24.0))
                    .rounded(px(12.0))
                    .bg(rgb(if checked { TEXT } else { 0x303034 }))
                    .border_1()
                    .border_color(rgb(if checked { TEXT } else { MUTED }))
                    .child(
                        div()
                            .debug_selector(move || thumb_selector)
                            .absolute()
                            .top(px(2.0))
                            .left(px(if checked { 22.0 } else { 2.0 }))
                            .size(px(18.0))
                            .rounded(px(9.0))
                            .bg(rgb(if checked { 0x303034 } else { TEXT })),
                    ),
            )
    }
}
