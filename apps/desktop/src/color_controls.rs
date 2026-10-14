//! 调色数值、滑块和真实布局命中，复用原生文本输入及根层指针事务。
use crate::{
    color_edit::ColorTarget,
    ui::{MUTED, TEXT},
    view::Desktop,
};
use dinuosi_core::color::Control;
use dinuosi_icons::Icon;
use gpui::{Context, Div, canvas, div, prelude::*, px, rgb, svg};

/// 绘制调色工具的功能分组标签，保持 11px 字号与单一层级，不添加描述性文案。
pub(crate) fn section(label: &'static str, cx: &gpui::App) -> Div {
    div()
        .mb(px(5.0))
        .text_size(px(11.0))
        .text_color(rgb(MUTED))
        .child(crate::i18n::t("color", label, cx))
}

/// 为固定工具控件生成稳定命中身份，便于输入绑定与独立行为验证。
pub(crate) fn control_id(control: Control) -> String {
    match control {
        Control::Basic(i) => format!("basic-{i}"),
        Control::Hsl(g, i) => format!("hsl-{g}-{i}"),
        Control::PickedHsl(i) => format!("picked-{i}"),
        Control::Wheel(w, i) => format!("wheel-{w}-{i}"),
        Control::Mask(i) => format!("mask-{i}"),
        Control::Qualifier(i) => format!("qualifier-{i}"),
    }
}

/// 返回每个可动画标量的既有控件名称；固定索引来自 Core 已校验的调色身份。
pub(crate) fn control_label(c: Control) -> &'static str {
    match c {
        Control::Basic(i) => crate::color_inspector::BASIC[i],
        Control::Hsl(_, i) => ["Hue", "Saturation", "Lightness"][i],
        Control::PickedHsl(i) => ["Hue", "Saturation", "Lightness"][i],
        Control::Wheel(_, i) => ["Chroma X", "Chroma Y", "Lightness"][i],
        Control::Mask(i) => [
            "Center X", "Center Y", "Width", "Height", "Rotation", "Feather",
        ][i],
        Control::Qualifier(i) => [
            "Hue center",
            "Hue width",
            "Softness",
            "Min saturation",
            "Min luma",
            "Max luma",
        ][i],
    }
}

/// 为关键帧曲线列表生成步骤和参数名称；HSL/色轮/选区带所属组，避免同名参数不可区分。
pub(crate) fn channel_label(stage: usize, control: Control, cx: &gpui::App) -> String {
    let group = match control {
        Control::Basic(_) => None,
        Control::Hsl(g, _) => Some(
            [
                "Red", "Orange", "Yellow", "Green", "Cyan", "Blue", "Purple", "Magenta",
            ][g],
        ),
        Control::PickedHsl(_) => Some("Sampled color"),
        Control::Wheel(w, _) => Some(["Shadows", "Midtones", "Highlights"][w]),
        Control::Mask(_) => Some("Mask"),
        Control::Qualifier(_) => Some("Color range"),
    };
    let label = crate::i18n::t("color", control_label(control), cx);
    if let Some(group) = group {
        format!(
            "{:02} · {} · {label}",
            stage + 1,
            crate::i18n::t("color", group, cx)
        )
    } else {
        format!("{:02} · {label}", stage + 1)
    }
}
impl Desktop {
    /// 绘制标签、真实可拖动滑块、精确数值及上下微调；窄窗口保持数值区完整。
    pub(crate) fn color_row(
        &self,
        control: Control,
        label: &'static str,
        cx: &mut Context<Self>,
    ) -> Div {
        let value = self.color_grade().stages[self.color_stage()].get(control);
        let enabled = self.color_enabled();
        div()
            .h(px(38.0))
            .flex()
            .items_center()
            .gap(px(6.0))
            .text_size(px(12.0))
            .opacity(if enabled { 1.0 } else { 0.4 })
            .child(
                div()
                    .w(px(68.0))
                    .flex_none()
                    .text_color(rgb(TEXT))
                    .child(crate::i18n::t("color", label, cx)),
            )
            .child(self.color_slider(control, cx))
            .child(self.color_number(control, value, enabled, 66.0, cx))
            .child(self.keyframe_controls(
                dinuosi_core::keyframes::KeyframeKind::Color(self.color_stage(), control),
                cx,
            ))
    }
    /// 绘制以零为基准的轨道及中心刻度；白平衡和 HSL 色组显示参数语义渐变，其余沿用灰阶。
    /// 命中使用完整实际滑块边界，不以装饰像素估算拖动范围。
    pub(crate) fn color_slider(
        &self,
        control: Control,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<Div> {
        let value = self.color_grade().stages[self.color_stage()].get(control);
        let (min, max) = control.range();
        let ratio = ((value - min) / (max - min)) as f32;
        let zero = ((-min) / (max - min)).clamp(0.0, 1.0) as f32;
        let id = format!("color-slider-{}", control_id(control));
        let selector = id.clone();
        let colored = matches!(
            control,
            Control::Basic(2 | 3) | Control::Hsl(..) | Control::PickedHsl(_)
        );
        let picked = self.color_grade().stages[self.color_stage()]
            .picked
            .as_ref()
            .map(|p| p.hue);
        div()
            .id(gpui::ElementId::Name(id.into()))
            .debug_selector(move || selector.clone())
            .flex_1()
            .min_w(px(20.0))
            .h(px(24.0))
            .relative()
            .cursor_pointer()
            .child(
                div()
                    .absolute()
                    .left_0()
                    .right_0()
                    .top(px(11.0))
                    .h(px(3.0))
                    .rounded(px(2.0))
                    .overflow_hidden()
                    .bg(rgb(0x48484d))
                    .when(colored, |s| {
                        s.flex().children((0..32).map(|i| {
                            let t = i as f64 / 31.0;
                            let color = crate::color_hsl::track_color(control, picked, t);
                            div().flex_1().h_full().bg(rgb(color))
                        }))
                    }),
            )
            .when(!colored, |s| {
                s.child(
                    div()
                        .absolute()
                        .left(gpui::relative(ratio.min(zero)))
                        .w(gpui::relative((ratio - zero).abs()))
                        .top(px(11.0))
                        .h(px(3.0))
                        .bg(rgb(0x96969d)),
                )
            })
            .when(min < 0.0 && max > 0.0, |s| {
                s.child(
                    div()
                        .absolute()
                        .left(gpui::relative(zero))
                        .top(px(8.0))
                        .w(px(1.0))
                        .h(px(9.0))
                        .bg(rgb(0x66666c)),
                )
            })
            .child(
                div()
                    .absolute()
                    .left(gpui::relative(ratio))
                    .ml(px(-4.0))
                    .top(px(8.0))
                    .size(px(9.0))
                    .rounded(px(5.0))
                    .bg(rgb(TEXT)),
            )
            .child(self.color_hitbox(ColorTarget::Scalar(control), cx))
    }
    /// 绘制与现有检查器一致的深色数值框及微调箭头；调整步长由显示精度决定。
    pub(crate) fn color_number(
        &self,
        control: Control,
        value: f64,
        enabled: bool,
        width: f32,
        cx: &mut Context<Self>,
    ) -> Div {
        let active = self
            .timeline
            .color
            .input
            .as_ref()
            .filter(|(id, stage, c, _)| {
                self.color_clip().is_some_and(|clip| &clip.id == id)
                    && *stage == self.color_stage()
                    && *c == control
            });
        let id = format!("color-value-{}", control_id(control));
        let selector = id.clone();
        div()
            .flex()
            .flex_none()
            .w(px(width))
            .h(px(24.0))
            .child(
                div()
                    .id(gpui::ElementId::Name(id.into()))
                    .debug_selector(move || selector.clone())
                    .w(px(width - 14.0))
                    .h_full()
                    .border_1()
                    .border_color(rgb(0x353539))
                    .bg(rgb(0x222225))
                    .font_family(crate::ui::MONO_FONT)
                    .overflow_hidden()
                    .when(active.is_none(), |s| {
                        s.child(
                            div()
                                .size_full()
                                .flex()
                                .items_center()
                                .justify_center()
                                .child(format!("{:.*}", control.precision(), value)),
                        )
                    })
                    .when_some(active, |s, (_, _, _, input)| s.child(input.clone()))
                    .on_click(cx.listener(move |this, _, window, cx| {
                        if enabled {
                            this.begin_color_input(control, window, cx);
                        }
                    })),
            )
            .child(
                div()
                    .w(px(14.0))
                    .flex()
                    .flex_col()
                    .children([1.0, -1.0].into_iter().map(|direction| {
                        let id = format!(
                            "color-{}-{}",
                            if direction > 0.0 { "up" } else { "down" },
                            control_id(control)
                        );
                        let selector = id.clone();
                        div()
                            .id(gpui::ElementId::Name(id.into()))
                            .debug_selector(move || selector.clone())
                            .h(px(12.0))
                            .w_full()
                            .flex()
                            .items_center()
                            .justify_center()
                            .cursor_pointer()
                            .child(
                                svg()
                                    .path(if direction > 0.0 {
                                        Icon::ChevronUp.path()
                                    } else {
                                        Icon::ChevronDown.path()
                                    })
                                    .size(px(10.0))
                                    .text_color(rgb(TEXT)),
                            )
                            .on_click(cx.listener(move |this, _, _, cx| {
                                if enabled {
                                    this.commit_inspector_input(cx);
                                    let value =
                                        this.color_grade().stages[this.color_stage()].get(control);
                                    let step = 10.0_f64.powi(-(control.precision() as i32));
                                    let mut result = Ok(());
                                    this.edit_color(
                                        |g, i| {
                                            result = g.stages[i].set(
                                                control,
                                                (value / step).round() * step + direction * step,
                                            )
                                        },
                                        cx,
                                    );
                                    if let Err(e) = result {
                                        this.notice = Some(e);
                                        cx.notify();
                                    }
                                }
                            }))
                    })),
            )
    }
    /// 使用当帧命中矩形启动根层手势；鼠标移出控件仍连续预览，释放只有一次保存。
    pub(crate) fn color_hitbox(
        &self,
        target: ColorTarget,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let entity = cx.entity();
        canvas(
            move |bounds, window, _| window.insert_hitbox(bounds, gpui::HitboxBehavior::Normal),
            move |bounds, hitbox, window, _| {
                window.on_mouse_event(move |e: &gpui::MouseDownEvent, phase, window, cx| {
                    if phase == gpui::DispatchPhase::Bubble
                        && e.button == gpui::MouseButton::Left
                        && hitbox.is_hovered(window)
                    {
                        entity.update(cx, |this, cx| {
                            this.begin_color_drag(target, bounds, e.position, window, cx);
                            cx.stop_propagation();
                        });
                    }
                });
            },
        )
        .absolute()
        .inset_0()
    }
}
