//! 变速数值输入与微调；沿用项目原生输入、失焦提交、取消与片段身份绑定。
use crate::{
    input::TextInput,
    ui::{LINE, MUTED},
    view::Desktop,
};
use dinuosi_icons::Icon;
use gpui::{Context, Div, Window, div, prelude::*, px, rgb, svg};
impl Desktop {
    /// 绘制沿用 59px 数值区与 17px 微调箭头的倍率/时长输入；失焦或 Enter 提交，Esc 取消。
    pub(crate) fn speed_number(
        &self,
        duration: bool,
        value: f64,
        enabled: bool,
        cx: &mut Context<Self>,
    ) -> Div {
        let id = if duration {
            "speed-duration"
        } else {
            "speed-rate"
        };
        let active = self
            .timeline
            .speed_input
            .as_ref()
            .filter(|(field, _, _)| *field == duration);
        div()
            .flex()
            .flex_none()
            .w(px(76.0))
            .h(px(24.0))
            .child(
                div()
                    .id(id)
                    .debug_selector(move || id.into())
                    .w(px(59.0))
                    .h_full()
                    .border_1()
                    .border_color(rgb(LINE))
                    .bg(rgb(0x212123))
                    .text_size(px(12.0))
                    .overflow_hidden()
                    .when(active.is_none(), |s| {
                        s.child(
                            div()
                                .size_full()
                                .flex()
                                .items_center()
                                .justify_center()
                                .child(format!("{value:.2}{}", if duration { "s" } else { "x" })),
                        )
                    })
                    .when_some(active, |s, (_, _, input)| s.child(input.clone()))
                    .on_click(cx.listener(move |this, _, window, cx| {
                        if enabled {
                            this.begin_speed_input(duration, window, cx);
                        }
                    })),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .w(px(17.0))
                    .children([1.0, -1.0].map(|delta| {
                        div()
                            .id(gpui::SharedString::from(format!("{id}-step-{delta}")))
                            .h(px(12.0))
                            .w_full()
                            .text_size(px(9.0))
                            .flex()
                            .items_center()
                            .justify_center()
                            .border_1()
                            .border_color(rgb(LINE))
                            .child(
                                svg()
                                    .path(if delta > 0.0 {
                                        Icon::ChevronUp.path()
                                    } else {
                                        Icon::ChevronDown.path()
                                    })
                                    .size(px(10.0))
                                    .text_color(rgb(MUTED)),
                            )
                            .on_click(cx.listener(move |this, _, _, cx| {
                                if enabled {
                                    this.commit_inspector_input(cx);
                                    this.apply_speed_number(duration, value + delta * 0.1, cx);
                                }
                            }))
                    })),
            )
    }

    /// 为当前片段创建原生紧凑输入，绑定片段身份防止失焦后把旧数字写到新选择。
    fn begin_speed_input(&mut self, duration: bool, window: &mut Window, cx: &mut Context<Self>) {
        if self
            .timeline
            .speed_input
            .as_ref()
            .is_some_and(|(field, _, _)| *field == duration)
        {
            return;
        }
        self.commit_inspector_input(cx);
        let Some(clip) = self.speed_clip() else {
            return;
        };
        let id = clip.id.clone();
        let value = if duration {
            clip.duration as f64 / self.timeline.model.fps as f64
        } else {
            clip.speed.as_ref().map_or(1.0, |s| s.rate)
        };
        let input = cx.new(|cx| {
            let mut input = TextInput::new(format!("{value:.4}"), cx);
            input.compact = true;
            input.show_border = false;
            input
        });
        let focus = input.read(cx).focus.clone();
        focus.focus(window);
        self.timeline.input_blur = Some(cx.on_blur(&focus, window, |this, _, cx| {
            this.commit_inspector_input(cx)
        }));
        self.timeline.speed_input = Some((duration, id, input));
        cx.notify();
    }

    /// 提交倍率或目标秒数；秒数反推倍率，有限范围检查由 Core 执行，非法输入保留原模型并显示真实错误。
    pub(crate) fn apply_speed_number(
        &mut self,
        duration: bool,
        value: f64,
        cx: &mut Context<Self>,
    ) {
        let Some(c) = self.speed_clip() else {
            return;
        };
        let [a, b] = c.source_bounds();
        let rate = if duration {
            (b - a) / (value * self.timeline.model.fps as f64)
        } else {
            value
        };
        let pitch = c.speed.as_ref().is_some_and(|s| s.pitch);
        self.change_speed(rate, &[], pitch, cx);
    }
}
