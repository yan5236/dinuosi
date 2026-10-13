//! 变速检查器：复用既有输入、拨片、灰阶和单次保存事务，倍率与时长直接绑定 Core。
use crate::{
    timeline_state::Gesture,
    ui::{LINE, MUTED, PANEL, TEXT},
    view::Desktop,
};
use dinuosi_core::{media::MediaKind, timeline::Clip};
use gpui::{Context, Div, MouseButton, canvas, div, prelude::*, px, rgb};

/// 在剪映常规变速的六个刻度之间对数插值；1 倍位于第二个刻度，覆盖 0.1–100 倍。
pub(crate) fn rate_from_ratio(ratio: f64) -> f64 {
    let ticks: [f64; 6] = [0.1, 1.0, 2.0, 5.0, 10.0, 100.0];
    let position = ratio.clamp(0.0, 1.0) * 5.0;
    let index = (position.floor() as usize).min(4);
    ticks[index] * (ticks[index + 1] / ticks[index]).powf(position - index as f64)
}
/// 将合法倍率转为常规变速滑块比例；端点夹紧，与指针倍率计算互为反向。
fn rate_ratio(rate: f64) -> f32 {
    let ticks: [f64; 6] = [0.1, 1.0, 2.0, 5.0, 10.0, 100.0];
    let rate = rate.clamp(0.1, 100.0);
    let index = ticks.windows(2).position(|p| rate <= p[1]).unwrap_or(4);
    ((index as f64 + (rate / ticks[index]).ln() / (ticks[index + 1] / ticks[index]).ln()) / 5.0)
        as f32
}

impl Desktop {
    /// 读取首个选中视频/音频；空选择、失效身份及图片不显示变速入口，不以内部 UUID 作为可见文案。
    pub(crate) fn speed_clip(&self) -> Option<&Clip> {
        self.timeline
            .model
            .clips
            .iter()
            .find(|c| self.timeline.selected.contains(&c.id) && c.kind != MediaKind::Image)
    }
    /// 绘制常规/曲线模式、倍率、源时长/目标时长、声音变调及重置；音频仅提供常规模式。
    /// 锁定片段保持读取，所有输入和写入忽略；空选择由外层检查器切回项目概览。
    pub(crate) fn speed_inspector(&self, enabled: bool, cx: &mut Context<Self>) -> Div {
        let Some(clip) = self.speed_clip() else {
            return div();
        };
        let rate = clip.speed.as_ref().map_or(1.0, |s| s.rate);
        let pitch = clip.speed.as_ref().is_some_and(|s| s.pitch);
        let video = clip.kind == MediaKind::Video;
        let curve = video
            && (self.timeline.speed_curve_mode
                || clip.speed.as_ref().is_some_and(|s| !s.curve.is_empty()));
        let [a, b] = clip.source_bounds();
        let fps = self.timeline.model.fps as f64;
        let entity = cx.entity();
        let content = div()
            .flex_none()
            .px(px(20.0))
            .pt(px(18.0))
            .flex()
            .flex_col()
            .gap(px(16.0))
            .when(video, |s| {
                s.child(
                    div()
                        .h(px(26.0))
                        .flex()
                        .bg(rgb(0x212123))
                        .children([false, true].map(|is_curve| {
                            div()
                                .id(if is_curve {
                                    "speed-curve-mode"
                                } else {
                                    "speed-normal-mode"
                                })
                                .debug_selector(move || {
                                    if is_curve {
                                        "speed-curve-mode".into()
                                    } else {
                                        "speed-normal-mode".into()
                                    }
                                })
                                .flex_1()
                                .h_full()
                                .flex()
                                .items_center()
                                .justify_center()
                                .text_size(px(12.0))
                                .cursor_pointer()
                                .when(curve == is_curve, |s| s.bg(rgb(LINE)))
                                .child(crate::i18n::t(
                                    "inspector",
                                    if is_curve {
                                        "Curve speed"
                                    } else {
                                        "Normal speed"
                                    },
                                    cx,
                                ))
                                .on_click(cx.listener(move |this, _, window, cx| {
                                    this.commit_inspector_input(cx);
                                    this.timeline.speed_curve_mode = is_curve;
                                    if !is_curve && let Some(c) = this.speed_clip() {
                                        let rate = c.speed.as_ref().map_or(1.0, |s| s.rate);
                                        let pitch = c.speed.as_ref().is_some_and(|s| s.pitch);
                                        this.change_speed(rate, &[], pitch, cx);
                                    }
                                    this.focus_handle.focus(window);
                                    cx.notify();
                                }))
                        })),
                )
            })
            .when(!curve, |s| {
                s.child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(8.0))
                        .text_size(px(12.0))
                        .child(crate::i18n::t("inspector", "Multiplier", cx))
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap(px(12.0))
                                .child(self.speed_slider(rate, enabled, cx))
                                .child(self.speed_number(false, rate, enabled, cx)),
                        ),
                )
            })
            .when(curve, |s| s.child(self.speed_curve_inspector(enabled, cx)))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(8.0))
                    .text_size(px(12.0))
                    .child(crate::i18n::t("inspector", "Duration", cx))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(12.0))
                            .child(
                                div()
                                    .text_color(rgb(MUTED))
                                    .child(format!("{:.2}s", (b - a) / fps)),
                            )
                            .child(div().flex_1().h(px(1.0)).bg(rgb(LINE)))
                            .when(!curve, |s| {
                                s.child(self.speed_number(
                                    true,
                                    clip.duration as f64 / fps,
                                    enabled,
                                    cx,
                                ))
                            })
                            .when(curve, |s| {
                                s.child(format!("{:.2}s", clip.duration as f64 / fps))
                            }),
                    ),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .text_size(px(12.0))
                    .child(crate::i18n::t("inspector", "Change pitch", cx))
                    .child(crate::switch::Switch::new(
                        "speed-pitch",
                        pitch,
                        move |value, _, cx| {
                            if enabled {
                                entity.update(cx, |this, cx| {
                                    if let Some(c) = this.speed_clip() {
                                        let rate = c.speed.as_ref().map_or(1.0, |s| s.rate);
                                        let curve = c
                                            .speed
                                            .as_ref()
                                            .map_or_else(Vec::new, |s| s.curve.clone());
                                        this.change_speed(rate, &curve, *value, cx);
                                    }
                                });
                            }
                        },
                    )),
            );
        div()
            .debug_selector(|| "inspector-speed-page".into())
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .child(
                div()
                    .id("speed-scroll")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .child(content),
            )
            .child(
                div()
                    .px(px(20.0))
                    .py(px(12.0))
                    .flex_none()
                    .flex()
                    .justify_end()
                    .child(
                        div()
                            .id("speed-reset")
                            .debug_selector(|| "speed-reset".into())
                            .px(px(18.0))
                            .py(px(4.0))
                            .bg(rgb(LINE))
                            .text_size(px(12.0))
                            .cursor_pointer()
                            .child(crate::i18n::t("inspector", "Reset", cx))
                            .on_click(cx.listener(move |this, _, window, cx| {
                                if enabled {
                                    this.commit_inspector_input(cx);
                                    this.timeline.speed_curve_mode = false;
                                    this.change_speed(1.0, &[], false, cx);
                                    this.focus_handle.focus(window);
                                }
                            })),
                    ),
            )
    }

    /// 绘制带刻度和拖动点的倍率滑块；按下保存模型快照，根节点捕获移动，释放只提交一次历史。
    fn speed_slider(
        &self,
        rate: f64,
        enabled: bool,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<Div> {
        let entity = cx.entity();
        div()
            .id("speed-slider")
            .debug_selector(|| "speed-slider".into())
            .flex_1()
            .min_w(px(24.0))
            .h(px(26.0))
            .relative()
            .child(
                div()
                    .absolute()
                    .left_0()
                    .right_0()
                    .top(px(12.0))
                    .h(px(2.0))
                    .bg(rgb(MUTED)),
            )
            .children((0..=5).map(|index| {
                div()
                    .absolute()
                    .left(gpui::relative(index as f32 / 5.0))
                    .top(px(9.0))
                    .w(px(1.0))
                    .h(px(8.0))
                    .bg(rgb(MUTED))
            }))
            .child(
                div()
                    .absolute()
                    .left(gpui::relative(rate_ratio(rate)))
                    .ml(px(-5.0))
                    .top(px(7.0))
                    .size(px(11.0))
                    .rounded(px(6.0))
                    .bg(rgb(PANEL))
                    .border_1()
                    .border_color(rgb(TEXT)),
            )
            .child(
                canvas(
                    move |bounds, window, _| {
                        window.insert_hitbox(bounds, gpui::HitboxBehavior::Normal)
                    },
                    move |bounds, hitbox, window, _| {
                        window.on_mouse_event(
                            move |event: &gpui::MouseDownEvent, phase, window, cx| {
                                if enabled
                                    && phase == gpui::DispatchPhase::Bubble
                                    && event.button == MouseButton::Left
                                    && hitbox.is_hovered(window)
                                {
                                    entity.update(cx, |this, cx| {
                                        this.commit_inspector_input(cx);
                                        this.stop_timeline();
                                        this.focus_handle.focus(window);
                                        this.timeline.before = Some(this.timeline.model.clone());
                                        this.timeline.gesture = Some(Gesture::Speed { bounds });
                                        this.move_timeline_pointer(event.position, cx);
                                        cx.stop_propagation();
                                    });
                                }
                            },
                        );
                    },
                )
                .absolute()
                .inset_0(),
            )
    }

    /// 将变速修改发布为一次保存和撤销事务；同类选中片段共同修改，失败回滚且报告错误。
    pub(crate) fn change_speed(
        &mut self,
        rate: f64,
        curve: &[[f64; 2]],
        pitch: bool,
        cx: &mut Context<Self>,
    ) {
        if self.busy {
            return;
        }
        self.stop_timeline();
        self.stop_preview();
        let before = self.timeline.model.clone();
        let result = self.preview_speed(rate, curve, pitch);
        match result {
            Ok(()) => self.commit_timeline(before, cx),
            Err(error) => {
                self.notice = Some(error);
                cx.notify();
            }
        }
        self.timeline.preview = true;
        self.refresh_timeline(cx);
    }

    /// 为手势预览调用原子 Core 编辑，仅作用于首个选中素材的同类型选择；不写磁盘、不记录历史。
    pub(crate) fn preview_speed(
        &mut self,
        rate: f64,
        curve: &[[f64; 2]],
        pitch: bool,
    ) -> Result<(), String> {
        let Some(kind) = self.speed_clip().map(|c| c.kind) else {
            return Ok(());
        };
        let ids = self
            .timeline
            .model
            .clips
            .iter()
            .filter(|c| c.kind == kind && self.timeline.selected.contains(&c.id))
            .map(|c| c.id.clone())
            .collect();
        self.timeline.model.set_speed(&ids, rate, curve, pitch)
    }
}
