//! 调色输入输出曲线与三色轮；绘图位置和命中使用同一实际边界。
use crate::{
    color_edit::ColorTarget,
    color_steps::{button, disclosure},
    ui::{LINE, MUTED, TEXT},
    view::Desktop,
};
use dinuosi_core::color::Control;
use gpui::{Bounds, Context, Div, Pixels, Point, canvas, div, prelude::*, px, rgb};
use std::sync::{Arc, OnceLock};

impl Desktop {
    /// 绘制总/R/G/B 通道选择、可加删拖动的输入输出曲线；通道切换保留全部参数。
    pub(crate) fn color_curves(&self, cx: &mut Context<Self>) -> Div {
        let channel = self.timeline.color.channel;
        let points =
            self.color_clip().unwrap().color.stages[self.color_stage()].curves[channel].clone();
        let entity = cx.entity();
        let color = [TEXT, 0xe16d6d, 0x74be83, 0x719fd5][channel];
        div()
            .child(
                div()
                    .flex()
                    .gap(px(5.0))
                    .mb(px(10.0))
                    .children(
                        ["RGB", "R", "G", "B"]
                            .into_iter()
                            .enumerate()
                            .map(|(i, label)| {
                                button(format!("color-channel-{i}"), label, i == channel, true, cx)
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.commit_inspector_input(cx);
                                        this.timeline.color.channel = i;
                                        cx.notify();
                                    }))
                            }),
                    )
                    .child(
                        button(
                            "color-curve-reset",
                            "Reset",
                            false,
                            self.color_enabled(),
                            cx,
                        )
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.edit_color(
                                |g, i| g.stages[i].curves[channel] = vec![[0.0, 0.0], [1.0, 1.0]],
                                cx,
                            )
                        })),
                    ),
            )
            .child(
                div()
                    .id("color-curve-graph")
                    .debug_selector(|| "color-curve-graph".into())
                    .w_full()
                    .h(px(178.0))
                    .relative()
                    .bg(rgb(0x212123))
                    .child(
                        canvas(
                            move |bounds, window, _| {
                                window.insert_hitbox(bounds, gpui::HitboxBehavior::Normal)
                            },
                            move |bounds, hitbox, window, _| {
                                let mut grid = gpui::PathBuilder::stroke(px(1.0));
                                for i in 0..=4 {
                                    let r = i as f32 / 4.0;
                                    grid.move_to(gpui::point(
                                        bounds.left() + bounds.size.width * r,
                                        bounds.top(),
                                    ));
                                    grid.line_to(gpui::point(
                                        bounds.left() + bounds.size.width * r,
                                        bounds.bottom(),
                                    ));
                                    grid.move_to(gpui::point(
                                        bounds.left(),
                                        bounds.top() + bounds.size.height * r,
                                    ));
                                    grid.line_to(gpui::point(
                                        bounds.right(),
                                        bounds.top() + bounds.size.height * r,
                                    ));
                                }
                                if let Ok(p) = grid.build() {
                                    window.paint_path(p, rgb(LINE));
                                }
                                let mut line = gpui::PathBuilder::stroke(px(1.5));
                                for (i, p) in points.iter().enumerate() {
                                    let p = gpui::point(
                                        bounds.left() + bounds.size.width * p[0] as f32,
                                        bounds.top() + bounds.size.height * (1.0 - p[1]) as f32,
                                    );
                                    if i == 0 {
                                        line.move_to(p);
                                    } else {
                                        line.line_to(p);
                                    }
                                }
                                if let Ok(p) = line.build() {
                                    window.paint_path(p, rgb(color));
                                }
                                window.on_mouse_event(
                                    move |e: &gpui::MouseDownEvent, phase, window, cx| {
                                        if phase == gpui::DispatchPhase::Bubble
                                            && hitbox.is_hovered(window)
                                            && matches!(
                                                e.button,
                                                gpui::MouseButton::Left | gpui::MouseButton::Right
                                            )
                                        {
                                            entity.update(cx, |this, cx| {
                                                this.begin_color_curve(
                                                    bounds,
                                                    e.position,
                                                    e.button == gpui::MouseButton::Right,
                                                    window,
                                                    cx,
                                                );
                                                cx.stop_propagation();
                                            });
                                        }
                                    },
                                );
                            },
                        )
                        .size_full(),
                    )
                    .children(
                        self.color_clip().unwrap().color.stages[self.color_stage()].curves[channel]
                            .iter()
                            .map(|p| {
                                div()
                                    .absolute()
                                    .left(gpui::relative(p[0] as f32))
                                    .top(gpui::relative((1.0 - p[1]) as f32))
                                    .ml(px(-3.0))
                                    .mt(px(-3.0))
                                    .size(px(6.0))
                                    .rounded(px(3.0))
                                    .bg(rgb(color))
                            }),
                    ),
            )
            .child(
                div()
                    .flex()
                    .justify_between()
                    .text_size(px(10.0))
                    .py(px(4.0))
                    .child("0")
                    .child(crate::i18n::t("color", "Input", cx))
                    .child("1"),
            )
    }
    /// 命中最近曲线控制点；空白左键添加，右键只删除内部点，最多 32 点且留最小横向间距。
    /// 添加及后续拖动属于同一按下快照，Esc 和撤销能恢复添加前的完整曲线。
    fn begin_color_curve(
        &mut self,
        bounds: Bounds<Pixels>,
        p: Point<Pixels>,
        remove: bool,
        window: &mut gpui::Window,
        cx: &mut Context<Self>,
    ) {
        self.commit_inspector_input(cx);
        if !self.color_enabled() {
            return;
        }
        let channel = self.timeline.color.channel;
        let stage = self.color_stage();
        let points = &self.color_clip().unwrap().color.stages[stage].curves[channel];
        let nearest = points
            .iter()
            .enumerate()
            .map(|(i, v)| {
                let q = gpui::point(
                    bounds.left() + bounds.size.width * v[0] as f32,
                    bounds.top() + bounds.size.height * (1.0 - v[1]) as f32,
                );
                (i, f32::from(q.x - p.x).hypot(f32::from(q.y - p.y)))
            })
            .min_by(|a, b| a.1.total_cmp(&b.1));
        if remove {
            if let Some((i, d)) = nearest
                && d < 12.0
                && i > 0
                && i + 1 < points.len()
            {
                self.edit_color(
                    |g, s| {
                        g.stages[s].curves[channel].remove(i);
                    },
                    cx,
                );
            }
            return;
        }
        if let Some((i, d)) = nearest
            && d < 12.0
        {
            self.begin_color_drag(ColorTarget::Curve(channel, i), bounds, p, window, cx);
            return;
        }
        let x =
            (f32::from(p.x - bounds.left()) / f32::from(bounds.size.width)).clamp(0.0, 1.0) as f64;
        if points.len() >= 32 || points.iter().any(|v| (v[0] - x).abs() < 0.001) {
            return;
        }
        let before = self.timeline.model.clone();
        let index = points.partition_point(|v| v[0] < x);
        let mut grade = self.color_grade().into_owned();
        let id = self.color_clip().unwrap().id.clone();
        grade.stages[stage].curves[channel].insert(
            index,
            [
                x,
                1.0 - (f32::from(p.y - bounds.top()) / f32::from(bounds.size.height))
                    .clamp(0.0, 1.0) as f64,
            ],
        );
        if let Err(e) = self.timeline.model.set_color_at(
            &[id].into(),
            &grade,
            self.keyframe_frame(),
            self.auto_keyframe(cx),
        ) {
            self.notice = Some(e);
            return;
        }
        self.begin_color_drag(ColorTarget::Curve(channel, index), bounds, p, window, cx);
        self.timeline.before = Some(before);
    }
    /// 显示与实际色偏方向一致的色轮与位置标记；轮心为零偏色，拖动保持在单位圆内。
    pub(crate) fn color_wheel(&self, index: usize, size: f32, cx: &mut Context<Self>) -> Div {
        let values = self.color_grade().stages[self.color_stage()].wheels[index];
        div().flex().justify_center().child(
            div()
                .id(gpui::ElementId::Name(format!("color-wheel-{index}").into()))
                .debug_selector(move || format!("color-wheel-{index}"))
                .size(px(size))
                .relative()
                .cursor_pointer()
                .child(gpui::img(wheel_image()).size_full())
                .child(
                    div()
                        .absolute()
                        .left(gpui::relative((values[0] / 200.0 + 0.5) as f32))
                        .top(gpui::relative((values[1] / 200.0 + 0.5) as f32))
                        .ml(px(-4.0))
                        .mt(px(-4.0))
                        .size(px(8.0))
                        .rounded(px(4.0))
                        .border_1()
                        .border_color(rgb(TEXT))
                        .bg(rgb(0x29292d)),
                )
                .child(self.color_hitbox(ColorTarget::Wheel(index), cx)),
        )
    }
}
/// 首次生成色轮 BGRA 位图，此后共享 GPU 图像；色相方向与核心三通道色偏公式一致。
fn wheel_image() -> Arc<gpui::RenderImage> {
    static IMAGE: OnceLock<Arc<gpui::RenderImage>> = OnceLock::new();
    IMAGE
        .get_or_init(|| {
            let mut bytes = vec![0; 128 * 128 * 4];
            for y in 0..128 {
                for x in 0..128 {
                    let dx = (x as f64 + 0.5) / 64.0 - 1.0;
                    let dy = (y as f64 + 0.5) / 64.0 - 1.0;
                    let radius = dx.hypot(dy);
                    if radius > 1.0 {
                        continue;
                    }
                    let tint = [dx, -dx * 0.5 - dy * 0.866, -dx * 0.5 + dy * 0.866];
                    let max = tint.iter().copied().reduce(f64::max).unwrap();
                    let min = tint.iter().copied().reduce(f64::min).unwrap();
                    let rgb = tint.map(|v| {
                        if max - min < 1e-6 {
                            0.14
                        } else if radius > 0.92 {
                            0.5 + ((v - min) / (max - min) - 0.5) * 0.8
                        } else {
                            0.14 + ((v - min) / (max - min) - 0.5) * radius * 0.18
                        }
                    });
                    let i = (y * 128 + x) * 4;
                    for c in 0..3 {
                        bytes[i + 2 - c] = (rgb[c] * 255.0) as u8;
                    }
                    bytes[i + 3] = 255;
                    if radius < 0.89 && (x == 64 || y == 64) {
                        bytes[i..i + 3].fill(58);
                    }
                }
            }
            Arc::new(gpui::RenderImage::new([image::Frame::new(
                image::RgbaImage::from_raw(128, 128, bytes).unwrap(),
            )]))
        })
        .clone()
}

impl Desktop {
    /// 三色轮并排保留共同明暗参考，亮度控制跟随各轮；精确色偏按需展开当前轮的数值。
    pub(crate) fn color_wheels(&self, cx: &mut Context<Self>) -> Div {
        let width = f32::from(self.timeline.bounds.size.width) * 0.259 - 56.0;
        let wheel_size = (width / 3.0 - 10.0).clamp(58.0, 96.0);
        let current = self.timeline.color.wheel;
        div()
            .child(
                div().flex().gap(px(8.0)).children(
                    ["Shadows", "Midtones", "Highlights"]
                        .into_iter()
                        .enumerate()
                        .map(|(i, label)| {
                            let control = Control::Wheel(i, 2);
                            let value = self.color_grade().stages[self.color_stage()].get(control);
                            div()
                                .flex_1()
                                .min_w_0()
                                .h(px(222.0))
                                .flex()
                                .flex_col()
                                .items_center()
                                .gap(px(6.0))
                                .text_size(px(11.0))
                                .child(
                                    div()
                                        .text_color(rgb(if current == i { TEXT } else { MUTED }))
                                        .child(crate::i18n::t("color", label, cx)),
                                )
                                .child(self.color_wheel(i, wheel_size, cx))
                                .child(
                                    div()
                                        .w_full()
                                        .h(px(64.0))
                                        .flex_none()
                                        .relative()
                                        .child(
                                            self.color_slider(control, cx)
                                                .absolute()
                                                .left_0()
                                                .right(px(46.0))
                                                .top_0()
                                                .flex_none(),
                                        )
                                        .child(
                                            self.keyframe_controls(
                                                dinuosi_core::keyframes::KeyframeKind::Color(
                                                    self.color_stage(),
                                                    control,
                                                ),
                                                cx,
                                            )
                                            .absolute()
                                            .right_0()
                                            .top_0(),
                                        )
                                        .child(
                                            div()
                                                .absolute()
                                                .top(px(36.0))
                                                .left(gpui::relative(0.5))
                                                .ml(px(-27.0))
                                                .child(self.color_number(
                                                    control,
                                                    value,
                                                    self.color_enabled(),
                                                    54.0,
                                                    cx,
                                                )),
                                        ),
                                )
                        }),
                ),
            )
            .child(
                div().mt(px(24.0)).child(
                    disclosure(
                        "color-wheel-values",
                        "Precise values",
                        self.timeline.color.wheel_values,
                        cx,
                    )
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.commit_inspector_input(cx);
                        this.timeline.color.wheel_values = !this.timeline.color.wheel_values;
                        cx.notify();
                    })),
                ),
            )
            .when(self.timeline.color.wheel_values, |s| {
                s.child(self.color_row(Control::Wheel(current, 0), "Chroma X", cx))
                    .child(self.color_row(Control::Wheel(current, 1), "Chroma Y", cx))
            })
    }
}
