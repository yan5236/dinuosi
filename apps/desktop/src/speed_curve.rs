//! 视频曲线变速预设与控制点画布；曲线定义归 Core，鼠标拖动复用时间线手势和保存事务。
use crate::{
    timeline_state::Gesture,
    ui::{LINE, MUTED, TEXT},
    view::Desktop,
};
use gpui::{Bounds, Context, Div, MouseButton, Pixels, Point, canvas, div, prelude::*, px, rgb};

pub(crate) const CUSTOM: &[[f64; 2]] =
    &[[0.0, 1.0], [0.25, 1.0], [0.5, 1.0], [0.75, 1.0], [1.0, 1.0]];
const PRESETS: &[(&str, &[[f64; 2]])] = &[
    ("None", &[]),
    ("Custom", CUSTOM),
    (
        "Montage",
        &[[0.0, 1.0], [0.25, 5.0], [0.5, 0.5], [0.75, 5.0], [1.0, 1.0]],
    ),
    (
        "Hero",
        &[
            [0.0, 1.0],
            [0.25, 3.0],
            [0.5, 0.25],
            [0.75, 3.0],
            [1.0, 1.0],
        ],
    ),
    (
        "Bullet",
        &[[0.0, 1.0], [0.35, 1.0], [0.5, 0.1], [0.65, 1.0], [1.0, 1.0]],
    ),
    (
        "Jump",
        &[[0.0, 0.5], [0.35, 0.5], [0.5, 5.0], [0.65, 0.5], [1.0, 0.5]],
    ),
    (
        "Flash in",
        &[[0.0, 5.0], [0.25, 5.0], [0.75, 1.0], [1.0, 1.0]],
    ),
    (
        "Flash out",
        &[[0.0, 1.0], [0.25, 1.0], [0.75, 5.0], [1.0, 5.0]],
    ),
];
const MIN_POINT_GAP: f64 = 0.001;

/// 将 0.1–10 倍按对数映射为画布纵比例，1 倍位于中线；上下端点夹紧。
fn vertical(rate: f64) -> f32 {
    ((1.0 - rate.clamp(0.1, 10.0).log10()) * 0.5) as f32
}

impl Desktop {
    /// 绘制八个曲线选择和活动曲线画布；预设立即保存，自定义控制点可添加/移除，端点不可删除。
    /// 选择“无”保留曲线模式但恢复 1 倍；标签仅为控件名称，不添加解释性文案。
    pub(crate) fn speed_curve_inspector(&self, enabled: bool, cx: &mut Context<Self>) -> Div {
        let points = self
            .speed_clip()
            .and_then(|c| c.speed.as_ref())
            .map_or_else(Vec::new, |s| s.curve.clone());
        let active = !points.is_empty();
        let selected_preset = PRESETS
            .iter()
            .position(|(_, p)| *p == points.as_slice())
            .unwrap_or(1);
        div()
            .flex()
            .flex_col()
            .gap(px(10.0))
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap(px(6.0))
                    .children(PRESETS.iter().enumerate().map(|(index, (label, preset))| {
                        let label = *label;
                        let values = preset.to_vec();
                        div()
                            .id(gpui::SharedString::from(format!("speed-preset-{index}")))
                            .debug_selector(move || format!("speed-preset-{index}"))
                            .w(gpui::relative(0.22))
                            .min_w(px(52.0))
                            .flex()
                            .flex_col()
                            .items_center()
                            .gap(px(4.0))
                            .text_size(px(11.0))
                            .cursor_pointer()
                            .child(
                                div()
                                    .h(px(42.0))
                                    .w_full()
                                    .bg(rgb(0x333337))
                                    .border_1()
                                    .border_color(rgb(if selected_preset == index {
                                        TEXT
                                    } else {
                                        LINE
                                    }))
                                    .child(curve_picture(values.clone(), false)),
                            )
                            .child(crate::i18n::t("inspector", label, cx))
                            .on_click(cx.listener(move |this, _, window, cx| {
                                if enabled {
                                    this.commit_inspector_input(cx);
                                    this.timeline.speed_curve_point = 2;
                                    let pitch = this
                                        .speed_clip()
                                        .and_then(|c| c.speed.as_ref())
                                        .is_some_and(|s| s.pitch);
                                    this.change_speed(1.0, &values, pitch, cx);
                                    this.focus_handle.focus(window);
                                }
                            }))
                    })),
            )
            .when(active, |s| {
                s.child(self.speed_curve_graph(points, enabled, cx)).child(
                    div()
                        .flex()
                        .justify_end()
                        .gap(px(6.0))
                        .children([false, true].map(|add| {
                            div()
                                .id(if add {
                                    "speed-point-add"
                                } else {
                                    "speed-point-remove"
                                })
                                .debug_selector(move || {
                                    if add {
                                        "speed-point-add".into()
                                    } else {
                                        "speed-point-remove".into()
                                    }
                                })
                                .w(px(24.0))
                                .h(px(22.0))
                                .flex()
                                .items_center()
                                .justify_center()
                                .bg(rgb(LINE))
                                .cursor_pointer()
                                .child(if add { "+" } else { "−" })
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    if enabled {
                                        this.edit_speed_point(add, cx);
                                    }
                                }))
                        })),
                )
            })
    }

    /// 绘制对数倍率曲线、1 倍中线及控制点；按下命中最近控制点，根节点捕获窗口内外的移动。
    /// 保存按下快照，连续拖动只产生一次历史；锁定状态忽略输入，画布边界防止控制点越界。
    fn speed_curve_graph(
        &self,
        points: Vec<[f64; 2]>,
        enabled: bool,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<Div> {
        let entity = cx.entity();
        let selected = self
            .timeline
            .speed_curve_point
            .min(points.len().saturating_sub(1));
        div()
            .id("speed-curve-graph")
            .debug_selector(|| "speed-curve-graph".into())
            .h(px(148.0))
            .w_full()
            .bg(rgb(0x212123))
            .relative()
            .child(curve_picture(points.clone(), true))
            .children(points.iter().enumerate().map(|(index, p)| {
                div()
                    .absolute()
                    .left(gpui::relative(p[0] as f32))
                    .top(gpui::relative(vertical(p[1])))
                    .ml(px(-4.0))
                    .mt(px(-4.0))
                    .size(px(8.0))
                    .rounded(px(4.0))
                    .border_1()
                    .border_color(rgb(TEXT))
                    .bg(rgb(if index == selected { TEXT } else { LINE }))
            }))
            .child(
                canvas(
                    move |bounds, window, _| {
                        window.insert_hitbox(bounds, gpui::HitboxBehavior::Normal)
                    },
                    move |bounds, hitbox, window, _| {
                        let points = points.clone();
                        window.on_mouse_event(
                            move |event: &gpui::MouseDownEvent, phase, window, cx| {
                                if !enabled
                                    || phase != gpui::DispatchPhase::Bubble
                                    || event.button != MouseButton::Left
                                    || !hitbox.is_hovered(window)
                                {
                                    return;
                                }
                                let nearest = points
                                    .iter()
                                    .enumerate()
                                    .map(|(i, p)| {
                                        let x = bounds.left() + bounds.size.width * p[0] as f32;
                                        let y = bounds.top() + bounds.size.height * vertical(p[1]);
                                        (
                                            i,
                                            f32::from(event.position.x - x)
                                                .hypot(f32::from(event.position.y - y)),
                                        )
                                    })
                                    .min_by(|a, b| a.1.total_cmp(&b.1));
                                if let Some((index, distance)) = nearest
                                    && distance <= 16.0
                                {
                                    entity.update(cx, |this, cx| {
                                        this.commit_inspector_input(cx);
                                        this.stop_timeline();
                                        this.focus_handle.focus(window);
                                        this.timeline.before = Some(this.timeline.model.clone());
                                        this.timeline.speed_curve_point = index;
                                        this.timeline.gesture =
                                            Some(Gesture::SpeedPoint { bounds, index });
                                        cx.notify();
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

    /// 从手势快照更新控制点，内点两侧保留最小间距；旧密集曲线保持横坐标但仍可修改倍率。
    /// 接近中线吸附 1 倍，端点横坐标固定，不将非法上下界传入浮点夹取。
    /// 失败显示真实错误并保持上次有效模型；成功仅刷新预览，不在移动事件写磁盘。
    pub(crate) fn move_speed_point(
        &mut self,
        point: Point<Pixels>,
        bounds: Bounds<Pixels>,
        index: usize,
        cx: &mut Context<Self>,
    ) {
        let Some(c) = self.speed_clip() else {
            return;
        };
        let Some(speed) = &c.speed else {
            return;
        };
        let mut curve = speed.curve.clone();
        if index >= curve.len() {
            return;
        }
        let pitch = speed.pitch;
        let ratio =
            (f32::from(point.y - bounds.top()) / f32::from(bounds.size.height)).clamp(0.0, 1.0);
        curve[index][1] = if (ratio - 0.5).abs() * f32::from(bounds.size.height) < 3.0 {
            1.0
        } else {
            10.0_f64.powf(1.0 - ratio as f64 * 2.0)
        };
        if index > 0 && index + 1 < curve.len() {
            let min = curve[index - 1][0] + MIN_POINT_GAP;
            let max = curve[index + 1][0] - MIN_POINT_GAP;
            if min <= max {
                let x = f32::from(point.x - bounds.left()) / f32::from(bounds.size.width);
                curve[index][0] = (x as f64).clamp(min, max);
            }
        }
        match self.preview_speed(1.0, &curve, pitch) {
            Ok(()) => {
                self.timeline.preview = true;
                self.refresh_timeline(cx);
            }
            Err(error) => self.notice = Some(error),
        }
    }

    /// 在当前选中点右侧区间中点加入控制点，或删除选中内点；最多 64 点，端点和两个最小点保持。
    /// 新点倍率取原曲线插值；中点两侧不足最小间距时忽略添加，防止产生无法拖动的密集点。
    /// 添加本身不改变时间映射，提交复用一次保存/撤销事务。
    fn edit_speed_point(&mut self, add: bool, cx: &mut Context<Self>) {
        let Some(speed) = self.speed_clip().and_then(|c| c.speed.as_ref()) else {
            return;
        };
        let mut curve = speed.curve.clone();
        let pitch = speed.pitch;
        if curve.len() < 2 {
            return;
        }
        let index = self.timeline.speed_curve_point.min(curve.len() - 1);
        if add {
            if curve.len() >= 64 {
                return;
            }
            let left = index.min(curve.len() - 2);
            let x = (curve[left][0] + curve[left + 1][0]) * 0.5;
            if x - curve[left][0] < MIN_POINT_GAP || curve[left + 1][0] - x < MIN_POINT_GAP {
                return;
            }
            curve.insert(left + 1, [x, speed.rate_at(x)]);
            self.timeline.speed_curve_point = left + 1;
        } else {
            if index == 0 || index + 1 == curve.len() {
                return;
            }
            curve.remove(index);
            self.timeline.speed_curve_point = index - 1;
        }
        self.change_speed(1.0, &curve, pitch, cx);
    }
}

/// 用真实 GPUI 路径绘制倍率曲线；小图只显示预设形状，编辑画布附带 10x/1x/0.1x 的倍率刻度。
fn curve_picture(points: Vec<[f64; 2]>, labels: bool) -> Div {
    div()
        .size_full()
        .relative()
        .child(
            canvas(
                |_, _, _| (),
                move |bounds, _, window, _| {
                    let origin = bounds.origin;
                    let width = bounds.size.width;
                    let height = bounds.size.height;
                    let mut line = gpui::PathBuilder::stroke(px(1.0));
                    line.move_to(gpui::point(origin.x, origin.y + height * 0.5));
                    line.line_to(gpui::point(origin.x + width, origin.y + height * 0.5));
                    if let Ok(path) = line.build() {
                        window.paint_path(path, rgb(MUTED));
                    }
                    let mut path = gpui::PathBuilder::stroke(px(1.5));
                    if points.is_empty() {
                        path.move_to(gpui::point(origin.x + width * 0.3, origin.y + height * 0.3));
                        path.line_to(gpui::point(origin.x + width * 0.7, origin.y + height * 0.7));
                    } else {
                        for (index, p) in points.iter().enumerate() {
                            let position = gpui::point(
                                origin.x + width * p[0] as f32,
                                origin.y + height * vertical(p[1]),
                            );
                            if index == 0 {
                                path.move_to(position);
                            } else {
                                path.line_to(position);
                            }
                        }
                    }
                    if let Ok(path) = path.build() {
                        window.paint_path(path, rgb(TEXT));
                    }
                },
            )
            .size_full(),
        )
        .when(labels, |s| {
            s.children(
                [(0.0, "10x"), (0.5, "1x"), (1.0, "0.1x")].map(|(top, label)| {
                    div()
                        .absolute()
                        .left(px(4.0))
                        .top(gpui::relative(top))
                        .mt(px(if top == 1.0 { -15.0 } else { 0.0 }))
                        .text_size(px(10.0))
                        .text_color(rgb(MUTED))
                        .child(label)
                }),
            )
        })
}
