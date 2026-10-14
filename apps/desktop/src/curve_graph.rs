//! 曲线画布、数值刻度与节点/切线命中；横坐标精确复用主时间线的缩放和滚动。
use crate::{
    curve_panel::CHANNELS,
    ui::{LINE, MUTED, TEXT},
    view::Desktop,
};
use dinuosi_core::timeline::Clip;
use gpui::{
    Bounds, Context, Div, MouseButton, Pixels, Point, canvas, div, point, prelude::*, px, rgb,
};

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct CurveView {
    pub first: f64,
    pub last: f64,
    pub min: f64,
    pub max: f64,
}
impl CurveView {
    /// 将项目帧/真实参数转换成视口比例；允许视口外结果以便绘制层统一裁切。
    pub fn ratio(self, frame: f64, value: f64) -> (f32, f32) {
        (
            ((frame - self.first) / (self.last - self.first)) as f32,
            ((self.max - value) / (self.max - self.min)) as f32,
        )
    }
    /// 从画布窗口坐标反算项目帧和参数；保留分数帧供求值，节点提交时再整帧取整。
    pub fn value(self, bounds: Bounds<Pixels>, p: Point<Pixels>) -> (f64, f64) {
        let x = f32::from(p.x - bounds.left()) / f32::from(bounds.size.width).max(1.0);
        let y = f32::from(p.y - bounds.top()) / f32::from(bounds.size.height).max(1.0);
        (
            self.first + x as f64 * (self.last - self.first),
            self.max - y as f64 * (self.max - self.min),
        )
    }
    /// 根据当前缩放和所有锚点/切线确定数值范围，常量段留出可拖动空间，不改变关键帧。
    fn new(desktop: &Desktop, clip: Option<&Clip>, channel: usize) -> Self {
        let (kind, component, _) = CHANNELS[channel];
        let mut values = Vec::new();
        if let Some(c) = clip {
            for k in c.keyframes.iter().filter(|k| k.kind == kind) {
                values.push(k.values[component]);
                if let Some(curve) = k.curves[component] {
                    values.extend(
                        [curve.incoming, curve.outgoing]
                            .into_iter()
                            .flatten()
                            .map(|h| k.values[component] + h[1]),
                    );
                }
            }
            if values.is_empty() {
                values.push(c.keyframe_values(kind, desktop.keyframe_frame() as f64)[component]);
            }
        }
        let mut min = values.iter().copied().reduce(f64::min).unwrap_or(0.0);
        let mut max = values.iter().copied().reduce(f64::max).unwrap_or(1.0);
        let pad = ((max - min) * 0.15).max(if channel == 5 { 0.1 } else { 10.0 });
        min -= pad;
        max += pad;
        let fps = desktop.timeline.model.fps as f64;
        let pps = desktop.pixels_per_second() as f64;
        Self {
            first: desktop.timeline.scroll_x as f64 / pps * fps,
            last: (desktop.timeline.scroll_x + desktop.timeline_view_width()) as f64 / pps * fps,
            min,
            max,
        }
    }
}

#[derive(Clone)]
pub(crate) struct CurveDrag {
    pub id: String,
    pub frame: i64,
    pub channel: usize,
    pub handle: Option<bool>,
    pub bounds: Bounds<Pixels>,
    pub view: CurveView,
    pub start: Point<Pixels>,
}

impl Desktop {
    /// 绘制时间/数值网格与真实求值曲线，节点选择不跳播放头；空白区域按主时间线规则寻址。
    /// 拖动期间冻结纵轴范围和按下几何，防止自动适配令节点跟随鼠标时跳变。
    pub(crate) fn curve_graph(&self, enabled: bool, cx: &mut Context<Self>) -> Div {
        let channel = self.curve_channel();
        let (kind, component, _) = CHANNELS[channel];
        let clip = self.curve_clip().cloned();
        let view = match &self.timeline.gesture {
            Some(crate::timeline_state::Gesture::Curve(drag)) => drag.view,
            _ => CurveView::new(self, clip.as_ref(), channel),
        };
        let fps = self.timeline.model.fps as f64;
        let ticks = dinuosi_core::timeline::ticks(
            self.timeline.model.fps,
            self.pixels_per_second() as f64,
            view.first / fps,
            self.timeline_view_width() as f64,
        );
        let playhead = view.ratio(self.keyframe_frame() as f64, 0.0).0;
        let labels: Vec<_> = ticks
            .iter()
            .filter(|t| t.major)
            .map(|t| (view.ratio(t.frame as f64, 0.0).0, t.label.clone()))
            .collect();
        let selected = self.curve_key().cloned();
        let entity = cx.entity();
        let plot_clip = clip.clone();
        div()
            .size_full()
            .relative()
            .children([0.0, 0.5, 1.0].map(|ratio| {
                let value = view.max - ratio * (view.max - view.min);
                div()
                    .absolute()
                    .right(px(f32::from(self.timeline.bounds.size.width)
                        - self.timeline.header
                        + 6.0))
                    .top(gpui::relative(ratio as f32))
                    .mt(px(if ratio == 1.0 { -14.0 } else { 0.0 }))
                    .text_size(px(10.0))
                    .text_color(rgb(MUTED))
                    .child(Self::curve_value_label(channel, value))
            }))
            .child(
                div()
                    .id("curve-graph")
                    .debug_selector(|| "curve-graph".into())
                    .absolute()
                    .left(px(self.timeline.header))
                    .right(px(12.0 + crate::trim_handle::WIDTH))
                    .top_0()
                    .bottom_0()
                    .bg(rgb(crate::ui::BACKGROUND))
                    .overflow_hidden()
                    .children(labels.into_iter().map(|(x, label)| {
                        div()
                            .absolute()
                            .left(gpui::relative(x))
                            .top(px(2.0))
                            .ml(px(4.0))
                            .text_size(px(9.0))
                            .text_color(rgb(MUTED))
                            .child(label)
                    }))
                    .on_scroll_wheel(cx.listener(|this, e, _, cx| this.scroll_timeline(e, cx)))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, e: &gpui::MouseDownEvent, window, cx| {
                            this.commit_inspector_input(cx);
                            this.focus_handle.focus(window);
                            this.begin_pointer(crate::sliders::Pointer::Timeline, e.position.x, cx);
                            cx.stop_propagation();
                        }),
                    )
                    .child(
                        canvas(
                            move |bounds, _, cx| {
                                entity.update(cx, |this, _| this.timeline.curves.bounds = bounds);
                            },
                            move |bounds, _, window, _| {
                                let at = |x: f32, y: f32| {
                                    point(
                                        bounds.left() + bounds.size.width * x,
                                        bounds.top() + bounds.size.height * y,
                                    )
                                };
                                let mut grid = gpui::PathBuilder::stroke(px(1.0));
                                for ratio in [0.0, 0.5, 1.0] {
                                    grid.move_to(at(0.0, ratio));
                                    grid.line_to(at(1.0, ratio));
                                }
                                for tick in ticks.iter().filter(|t| t.major) {
                                    let x = view.ratio(tick.frame as f64, 0.0).0;
                                    grid.move_to(at(x, 0.0));
                                    grid.line_to(at(x, 1.0));
                                }
                                if let Ok(path) = grid.build() {
                                    window.paint_path(path, rgb(LINE));
                                }
                                if let Some(c) = &plot_clip {
                                    let mut line = gpui::PathBuilder::stroke(px(1.5));
                                    let steps =
                                        (f32::from(bounds.size.width) / 2.0).ceil() as usize;
                                    for i in 0..=steps {
                                        let x = i as f32 / steps.max(1) as f32;
                                        let frame =
                                            view.first + x as f64 * (view.last - view.first);
                                        if frame < c.start as f64
                                            || frame > (c.start + c.duration - 1) as f64
                                        {
                                            continue;
                                        }
                                        let y = view
                                            .ratio(frame, c.keyframe_values(kind, frame)[component])
                                            .1;
                                        if i == 0
                                            || frame
                                                - (view.last - view.first) / (steps.max(1) as f64)
                                                < c.start as f64
                                        {
                                            line.move_to(at(x, y));
                                        } else {
                                            line.line_to(at(x, y));
                                        }
                                    }
                                    if let Ok(path) = line.build() {
                                        window.paint_path(path, rgb(TEXT));
                                    }
                                    if let Some(key) = &selected
                                        && key.curves[component].is_some()
                                    {
                                        let (x, y) = view.ratio(
                                            (c.start + key.frame) as f64,
                                            key.values[component],
                                        );
                                        let mut handles = gpui::PathBuilder::stroke(px(1.0));
                                        for incoming in [true, false] {
                                            if let Some((hx, hy)) =
                                                handle_position(c, key, channel, incoming, view)
                                            {
                                                handles.move_to(at(x, y));
                                                handles.line_to(at(hx, hy));
                                            }
                                        }
                                        if let Ok(path) = handles.build() {
                                            window.paint_path(path, rgb(MUTED));
                                        }
                                    }
                                }
                            },
                        )
                        .size_full(),
                    )
                    .when((0.0..=1.0).contains(&playhead), |s| {
                        s.child(
                            div()
                                .absolute()
                                .left(gpui::relative(playhead))
                                .top_0()
                                .bottom_0()
                                .w(px(1.0))
                                .bg(rgb(MUTED)),
                        )
                    })
                    .when_some(clip.as_ref(), |s, c| {
                        s.children(
                            c.keyframes
                                .iter()
                                .filter(|k| k.kind == kind && k.frame >= 0 && k.frame < c.duration)
                                .filter_map(|key| {
                                    let (x, y) = view
                                        .ratio((c.start + key.frame) as f64, key.values[component]);
                                    if !(0.0..=1.0).contains(&x) {
                                        return None;
                                    }
                                    Some(self.curve_point(
                                        (c.id.clone(), key.frame, None),
                                        (x, y),
                                        view,
                                        enabled,
                                        self.timeline.curves.selected.as_ref().is_some_and(
                                            |(id, frame)| id == &c.id && *frame == key.frame,
                                        ),
                                        cx,
                                    ))
                                }),
                        )
                        .when_some(
                            self.curve_key().filter(|k| k.curves[component].is_some()),
                            |s, key| {
                                s.children([true, false].into_iter().filter_map(|incoming| {
                                    let pos = handle_position(c, key, channel, incoming, view)?;
                                    let node = view
                                        .ratio((c.start + key.frame) as f64, key.values[component]);
                                    if pos == node {
                                        return None;
                                    }
                                    Some(self.curve_point(
                                        (c.id.clone(), key.frame, Some(incoming)),
                                        pos,
                                        view,
                                        enabled,
                                        false,
                                        cx,
                                    ))
                                }))
                            },
                        )
                    }),
            )
    }

    /// 以实际节点/手柄建立独立鼠标命中区；选择与寻址分离，锁定时允许选择但不开始修改手势。
    fn curve_point(
        &self,
        target: (String, i64, Option<bool>),
        position: (f32, f32),
        view: CurveView,
        enabled: bool,
        selected: bool,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<Div> {
        let channel = self.curve_channel();
        let (id, frame, handle) = target;
        let keyboard_id = id.clone();
        let debug = format!(
            "curve-{}-{frame}",
            handle.map_or("node", |input| if input { "in" } else { "out" })
        );
        div()
            .id(gpui::SharedString::from(debug.clone()))
            .debug_selector(move || debug.clone())
            .absolute()
            .left(gpui::relative(position.0))
            .top(gpui::relative(position.1))
            .ml(px(-6.0))
            .mt(px(-6.0))
            .size(px(12.0))
            .flex()
            .items_center()
            .justify_center()
            .cursor_pointer()
            .focusable()
            .tab_index(0)
            .child(
                div()
                    .size(px(if handle.is_some() { 6.0 } else { 8.0 }))
                    .rounded(px(4.0))
                    .border_1()
                    .border_color(rgb(TEXT))
                    .bg(rgb(if selected {
                        TEXT
                    } else {
                        crate::ui::BACKGROUND
                    })),
            )
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, e: &gpui::MouseDownEvent, window, cx| {
                    this.commit_inspector_input(cx);
                    this.end_timeline_pointer(window, cx);
                    this.focus_handle.focus(window);
                    this.timeline.curves.selected = Some((id.clone(), frame));
                    if enabled {
                        this.stop_timeline();
                        this.timeline.before = Some(this.timeline.model.clone());
                        this.timeline.gesture =
                            Some(crate::timeline_state::Gesture::Curve(CurveDrag {
                                id: id.clone(),
                                frame,
                                channel,
                                handle,
                                bounds: this.timeline.curves.bounds,
                                view,
                                start: e.position,
                            }));
                    }
                    cx.notify();
                    cx.stop_propagation();
                }),
            )
            .on_key_down(
                cx.listener(move |this, event: &gpui::KeyDownEvent, window, cx| {
                    if !event.is_held
                        && !event.keystroke.modifiers.modified()
                        && matches!(event.keystroke.key.as_str(), "enter" | "space")
                    {
                        this.timeline.curves.selected = Some((keyboard_id.clone(), frame));
                        this.focus_handle.focus(window);
                        cx.notify();
                        cx.stop_propagation();
                    }
                }),
            )
    }
}

/// 将入/出切线的相邻时间比例和参数差值换成绘图比例；无相邻节点时不绘制无效手柄。
fn handle_position(
    c: &Clip,
    key: &dinuosi_core::keyframes::Keyframe,
    channel: usize,
    incoming: bool,
    view: CurveView,
) -> Option<(f32, f32)> {
    let (kind, component, _) = CHANNELS[channel];
    let h = c.curve_handle(kind, component, key.frame, incoming)?;
    let neighbor = c.curve_neighbor(kind, key.frame, incoming)?;
    Some(view.ratio(
        (c.start + key.frame) as f64 + (neighbor.frame - key.frame) as f64 * h[0],
        key.values[component] + h[1],
    ))
}
