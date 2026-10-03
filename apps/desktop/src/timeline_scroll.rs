//! 时间线拖拽边缘滚动；固定鼠标位置也持续更新内容坐标，释放即取消任务。
use crate::{sliders::Pointer, timeline_state::Gesture, view::Desktop};
use gpui::{Context, Pixels, Point, px};
use std::time::Duration;

impl Desktop {
    /// 根据当前捕获的手势和鼠标位置计算每 16ms 的横纵滚动速度，边缘越近速度越大。
    /// 只滚动时间线区域；播放头/裁边只横滚，轨道名称只竖滚，滑块和参数控件不触发。
    fn timeline_edge_velocity(&self) -> (f32, f32) {
        let seek = self.session.drag == Some(Pointer::Timeline);
        let (horizontal, vertical) = match self.timeline.gesture {
            Some(Gesture::Marquee { .. } | Gesture::Move { .. }) => (true, true),
            Some(Gesture::Library { .. }) if self.timeline.moved => (true, true),
            Some(Gesture::Trim { .. }) => (true, false),
            Some(Gesture::Track { .. }) if self.timeline.moved => (false, true),
            _ if seek => (true, false),
            _ => return (0.0, 0.0),
        };
        let b = self.timeline.bounds;
        let p = self.timeline.pointer;
        if p.x < b.left()
            || p.x > b.right() + px(40.0)
            || p.y < b.top() - px(16.0)
            || p.y > b.bottom() + px(40.0)
        {
            return (0.0, 0.0);
        }
        let x = if horizontal {
            edge_speed(
                f32::from(p.x),
                f32::from(b.left()) + self.timeline.header,
                f32::from(b.right()) - 12.0 - crate::trim_handle::WIDTH,
            )
        } else {
            0.0
        };
        let y = if vertical {
            edge_speed(
                f32::from(p.y),
                f32::from(b.top()) + self.timeline.ruler,
                f32::from(b.bottom()) - 12.0,
            )
        } else {
            0.0
        };
        (x, y)
    }

    /// 更新全局鼠标位置并启动唯一持续滚动任务；鼠标停在边缘仍每帧滚动，离开边缘即取消。
    /// 所有拖拽入口共用根节点调用，不需要为每种控件单独注册计时器。
    pub(crate) fn update_timeline_autoscroll(
        &mut self,
        point: Point<Pixels>,
        cx: &mut Context<Self>,
    ) {
        self.timeline.pointer = point;
        if self.timeline_edge_velocity() == (0.0, 0.0) {
            self.timeline.scroll_task = None;
            return;
        }
        if self.timeline.scroll_task.is_some() {
            return;
        }
        self.timeline.scroll_task = Some(cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(16))
                    .await;
                let keep = this.update(cx, |this, cx| {
                    let keep = this.advance_timeline_autoscroll(cx);
                    if !keep {
                        this.timeline.scroll_task = None;
                    }
                    keep
                });
                if !matches!(keep, Ok(true)) {
                    break;
                }
            }
        }));
    }

    /// 执行一次滚动并用当前鼠标重新计算框选、落点和播放头；没有可滚动范围时返回 false。
    /// 向右拖放可持续扩展可达时间，最长遵守核心的一天上限；竖向严格按实际轨道夹紧。
    pub(crate) fn advance_timeline_autoscroll(&mut self, cx: &mut Context<Self>) -> bool {
        let (x, y) = self.timeline_edge_velocity();
        let before = (self.timeline.scroll_x, self.timeline.scroll_y);
        let max_x = (86400.0 * self.pixels_per_second() - self.timeline_view_width()).max(0.0);
        let visible =
            (f32::from(self.timeline.bounds.size.height) - self.timeline.ruler - 12.0).max(1.0);
        let max_y = (self.timeline_content_height() - visible).max(0.0);
        self.timeline.scroll_x = (before.0 + x).clamp(0.0, max_x);
        self.timeline.scroll_y = (before.1 + y).clamp(0.0, max_y);
        if before == (self.timeline.scroll_x, self.timeline.scroll_y) {
            return false;
        }
        let point = self.timeline.pointer;
        self.move_timeline_pointer(point, cx);
        if self.session.drag == Some(Pointer::Timeline) {
            self.move_pointer(point.x, cx);
        }
        cx.notify();
        true
    }

    /// 处理时间线滚轮；Ctrl 或 Shift 将竖向滚轮映射到横向，触控板原生横滚保持可用。
    /// 通常滚轮只在已有内容范围内滚动；活动拖拽时重新计算手势落点，避免释放位置滞后。
    pub(crate) fn scroll_timeline(
        &mut self,
        event: &gpui::ScrollWheelEvent,
        cx: &mut Context<Self>,
    ) {
        let delta = event.delta.pixel_delta(px(30.0));
        if event.modifiers.control || event.modifiers.shift || delta.x.abs() > delta.y.abs() {
            let change = if (event.modifiers.control || event.modifiers.shift) && delta.y != px(0.0)
            {
                delta.y
            } else {
                delta.x
            };
            self.timeline.scroll_x = (self.timeline.scroll_x - f32::from(change))
                .clamp(0.0, self.timeline_width() - self.timeline_view_width());
        } else {
            let visible =
                (f32::from(self.timeline.bounds.size.height) - self.timeline.ruler - 12.0).max(1.0);
            self.timeline.scroll_y = (self.timeline.scroll_y - f32::from(delta.y))
                .clamp(0.0, (self.timeline_content_height() - visible).max(0.0));
        }
        self.move_timeline_pointer(self.timeline.pointer, cx);
        if self.session.drag == Some(Pointer::Timeline) {
            self.move_pointer(self.timeline.pointer.x, cx);
        }
        cx.notify();
        cx.stop_propagation();
    }
}

/// 将边缘 36px 范围映射到每帧最多 16px；中心区域返回零，越界仍保持最大速度。
fn edge_speed(position: f32, min: f32, max: f32) -> f32 {
    if position < min + 36.0 {
        -((min + 36.0 - position) / 36.0).clamp(0.0, 1.0) * 16.0
    } else if position > max - 36.0 {
        ((position - max + 36.0) / 36.0).clamp(0.0, 1.0) * 16.0
    } else {
        0.0
    }
}
