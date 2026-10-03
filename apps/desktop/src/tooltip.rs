//! 时间线共用的即时向下 Tooltip；浮层越过父级裁切，提示与图标资源标签分离。
use crate::ui::{TEXT, tool_button};
use dinuosi_icons::Icon;
use gpui::{
    App, Bounds, ClickEvent, Context, Div, FocusHandle, IntoElement, PathBuilder, Pixels,
    RenderOnce, SharedString, Stateful, TextRun, Window, anchored, canvas, deferred, div, point,
    prelude::*, px, rgb,
};
use std::{cell::Cell, rc::Rc};

const COLOR: u32 = 0x18181b;

struct TriggerState {
    hovered: bool,
    dismissed: bool,
    focus: FocusHandle,
    bounds: Rc<Cell<Bounds<Pixels>>>,
}

impl TriggerState {
    /// 创建当前触发元素的悬停、关闭及焦点状态；焦点变化由 Window 自动刷新绘制。
    fn new(_: &mut Window, cx: &mut Context<Self>) -> Self {
        let focus = cx.focus_handle().tab_stop(true);
        Self {
            hovered: false,
            dismissed: false,
            focus,
            bounds: Rc::new(Cell::new(Bounds::default())),
        }
    }
}

#[derive(IntoElement)]
pub(crate) struct Tooltip {
    id: SharedString,
    text: SharedString,
    trigger: Stateful<Div>,
}

impl Tooltip {
    /// 创建带业务文字的 30px 图标触发元素；禁用项仍能显示提示，但业务回调由调用方控制。
    pub(crate) fn tool(
        id: impl Into<SharedString>,
        icon: Icon,
        enabled: bool,
        text: impl Into<SharedString>,
    ) -> Self {
        let id = id.into();
        Self {
            trigger: tool_button(id.clone(), icon, enabled),
            id,
            text: text.into(),
        }
    }

    /// 追加原有点击行为，不在 Tooltip 中修改轨道或项目；浮层关闭由外层触发区负责。
    pub(crate) fn on_click(
        mut self,
        listener: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.trigger = self.trigger.on_click(listener);
        self
    }

    /// 保留时间线选择工具的选中底色，不改变 Tooltip 气泡颜色。
    pub(crate) fn selected(mut self, selected: bool) -> Self {
        if selected {
            self.trigger = self.trigger.bg(rgb(0x38383c));
        }
        self
    }
}

impl RenderOnce for Tooltip {
    /// 使用持久控件状态立即响应悬停和焦点；Escape/点击关闭提示，Tab 在触发元素间移动。
    /// 以真实字体宽度居中气泡并限制窗口边界，箭头始终指向触发区中心；不占用轨道布局。
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let state = window.use_keyed_state(self.id.clone(), cx, TriggerState::new);
        let current = state.read(cx);
        let focus = current.focus.clone();
        let bounds = current.bounds.clone();
        let visible = (current.hovered || focus.is_focused(window)) && !current.dismissed;
        let anchor = bounds.get();
        let run = TextRun {
            len: self.text.len(),
            font: gpui::font("Microsoft YaHei UI"),
            color: rgb(TEXT).into(),
            background_color: None,
            underline: None,
            strikethrough: None,
        };
        let width = window
            .text_system()
            .shape_line(self.text.clone(), px(12.0), &[run], None)
            .width
            + px(20.0);
        let x = (anchor.center().x - width / 2.0)
            .max(px(4.0))
            .min((window.viewport_size().width - width - px(4.0)).max(px(4.0)));
        let arrow = anchor.center().x - x;
        let label = self.text.clone();
        let hover = state.clone();
        let dismiss = state.clone();
        let keyboard = state.clone();
        div()
            .id(SharedString::from(format!("{}-trigger", self.id)))
            .relative()
            .flex_none()
            .size(px(30.0))
            .track_focus(&focus)
            .tab_index(0)
            .on_hover(move |over, window, cx| {
                hover.update(cx, |s, cx| {
                    s.hovered = *over;
                    s.dismissed = !*over && s.focus.is_focused(window);
                    cx.notify();
                })
            })
            .on_click(move |_, _, cx| {
                dismiss.update(cx, |s, cx| {
                    s.dismissed = true;
                    cx.notify();
                })
            })
            .on_key_down(move |event, window, cx| {
                match event.keystroke.key.as_str() {
                    "escape" => keyboard.update(cx, |s, cx| {
                        s.dismissed = true;
                        cx.notify();
                    }),
                    "tab" => {
                        keyboard.update(cx, |s, _| s.dismissed = false);
                        if event.keystroke.modifiers.shift {
                            window.focus_prev();
                        } else {
                            window.focus_next();
                        }
                    }
                    _ => return,
                }
                cx.stop_propagation();
            })
            .child(
                canvas(
                    move |rect, window, _| {
                        if bounds.replace(rect) != rect && visible {
                            // 滚动或窗口缩放改变控件位置时，用更新后的边界重绘浮层。
                            window.refresh();
                        }
                    },
                    |_, _, _, _| {},
                )
                .absolute()
                .inset_0(),
            )
            .child(self.trigger)
            .when(visible, |s| {
                s.child(
                    deferred(
                        anchored()
                            .position(point(x, anchor.bottom() + px(4.0)))
                            .child(
                                div()
                                    .debug_selector(|| "timeline-tooltip".into())
                                    .relative()
                                    .w(width)
                                    .pt(px(6.0))
                                    .child(
                                        canvas(
                                            |_, _, _| {},
                                            move |b, _, window, _| {
                                                let mut path = PathBuilder::fill();
                                                path.move_to(point(
                                                    b.left() + arrow - px(6.0),
                                                    b.bottom(),
                                                ));
                                                path.line_to(point(b.left() + arrow, b.top()));
                                                path.line_to(point(
                                                    b.left() + arrow + px(6.0),
                                                    b.bottom(),
                                                ));
                                                path.close();
                                                if let Ok(path) = path.build() {
                                                    window.paint_path(path, rgb(COLOR));
                                                }
                                            },
                                        )
                                        .absolute()
                                        .top_0()
                                        .left_0()
                                        .w_full()
                                        .h(px(6.0)),
                                    )
                                    .child(
                                        div()
                                            .debug_selector(move || format!("tooltip-{label}"))
                                            .px(px(10.0))
                                            .py(px(6.0))
                                            .rounded(px(4.0))
                                            .bg(rgb(COLOR))
                                            .font_family("Microsoft YaHei UI")
                                            .text_size(px(12.0))
                                            .text_color(rgb(TEXT))
                                            .whitespace_nowrap()
                                            .child(self.text),
                                    ),
                            ),
                    )
                    .with_priority(1),
                )
            })
    }
}
