//! 滑块统一使用实际布局边界命中，根节点持续接收移动/释放，支持拖出滑块和窗口缩放。

use crate::{
    ui::{LINE, MUTED},
    view::Desktop,
};
use gpui::{Bounds, Context, Div, MouseButton, Pixels, Stateful, canvas, div, prelude::*, px, rgb};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Pointer {
    Volume,
    Zoom,
    Seek,
    TrimStart,
    TrimEnd,
    Timeline,
    PreviewProgress,
}

impl Desktop {
    /// 将实际布局边界保存到会话；Canvas 不通知重绘，避免布局阶段触发渲染循环。
    pub(crate) fn pointer_bounds(&self, kind: Pointer, cx: &mut Context<Self>) -> impl IntoElement {
        let entity = cx.entity();
        canvas(
            move |bounds, _, cx| {
                entity.update(cx, |this, _| {
                    this.session.bounds.insert(kind, bounds);
                });
            },
            |_, _, _, _| {},
        )
        .absolute()
        .inset_0()
    }

    /// 绘制真实音量/缩放滑块；按下立即定位，后续移动与释放由工作区根节点处理。
    pub(crate) fn slider(
        &self,
        kind: Pointer,
        width: f32,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let value = match kind {
            Pointer::Volume => self.volume,
            _ => (self.zoom - 0.5) / 3.5,
        };
        let selector = match kind {
            Pointer::Volume => "volume-slider",
            _ => "zoom-slider",
        };
        div()
            .id(selector)
            .debug_selector(move || selector.into())
            .flex_none()
            .w(px(width))
            .h(px(24.0))
            .relative()
            .cursor_pointer()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, event: &gpui::MouseDownEvent, _, cx| {
                    this.begin_pointer(kind, event.position.x, cx);
                    cx.stop_propagation();
                }),
            )
            .child(
                div()
                    .absolute()
                    .left(px(5.0))
                    .right(px(5.0))
                    .top(px(11.0))
                    .h(px(2.0))
                    .bg(rgb(LINE)),
            )
            .child(
                div()
                    .absolute()
                    .left(px((width - 10.0) * value.clamp(0.0, 1.0)))
                    .top(px(7.0))
                    .size(px(10.0))
                    .rounded(px(5.0))
                    .border_1()
                    .border_color(rgb(MUTED))
                    .bg(rgb(0x333336)),
            )
            .child(self.pointer_bounds(kind, cx))
    }

    /// 捕获指针；素材选区及全屏进度寻址暂停对应播放器，记住状态以便释放后恢复。
    pub(crate) fn begin_pointer(&mut self, kind: Pointer, x: Pixels, cx: &mut Context<Self>) {
        self.session.drag = Some(kind);
        self.session.resume_after_drag = if kind == Pointer::PreviewProgress
            && self.timeline.preview
        {
            self.timeline.playing
        } else {
            matches!(
                kind,
                Pointer::Seek | Pointer::TrimStart | Pointer::TrimEnd | Pointer::PreviewProgress
            ) && (self.session.player.is_some() || self.session.starting)
        };
        if self.session.resume_after_drag {
            if kind == Pointer::PreviewProgress && self.timeline.preview {
                self.stop_timeline();
            } else {
                self.stop_preview();
            }
        }
        self.move_pointer(x, cx);
    }

    /// 按真实控件边界夹紧鼠标比例；全屏进度映射当前播放范围，时间线遵循实时/释放预览偏好。
    pub(crate) fn move_pointer(&mut self, x: Pixels, cx: &mut Context<Self>) {
        let Some(kind) = self.session.drag else {
            return;
        };
        let key = if matches!(kind, Pointer::TrimStart | Pointer::TrimEnd) {
            Pointer::Seek
        } else {
            kind
        };
        let Some(bounds) = self.session.bounds.get(&key) else {
            return;
        };
        let ratio = normalized(*bounds, x, matches!(kind, Pointer::Volume | Pointer::Zoom));
        match kind {
            Pointer::Volume => {
                self.volume = ratio;
                if let Some(sound) = &self.timeline.sound {
                    sound.sink.set_volume(ratio);
                }
                if let Some(player) = &self.session.player {
                    player.volume(ratio);
                }
            }
            Pointer::Zoom => self.change_zoom(0.5 + ratio * 3.5 - self.zoom, cx),
            Pointer::Timeline => {
                let frame = self.timeline_frame_at(x);
                self.seek_timeline(frame as f64 / self.timeline.model.fps as f64, false, cx);
            }
            Pointer::Seek => {
                self.session.position = (ratio as f64 * self.session.duration())
                    .clamp(self.session.trim_start, self.session.trim_end);
                self.request_frame(cx);
            }
            Pointer::PreviewProgress => {
                if self.timeline.preview {
                    let end = self.timeline.model.end() as f64 / self.timeline.model.fps as f64;
                    self.seek_timeline(ratio as f64 * end, false, cx);
                } else {
                    self.session.position = self.session.trim_start
                        + ratio as f64 * (self.session.trim_end - self.session.trim_start);
                    self.request_frame(cx);
                }
            }
            Pointer::TrimStart => {
                self.session.trim_start = (ratio as f64 * self.session.duration())
                    .min((self.session.trim_end - 0.01).max(0.0));
                self.session.position = self.session.position.max(self.session.trim_start);
                self.request_frame(cx);
            }
            Pointer::TrimEnd => {
                self.session.trim_end = (ratio as f64 * self.session.duration())
                    .max(self.session.trim_start + 0.01)
                    .min(self.session.duration());
                self.session.position = self.session.position.min(self.session.trim_end);
                self.request_frame(cx);
            }
        }
        cx.notify();
    }

    /// 释放时确保时间线最终帧刷新，并仅恢复拖动前正在播放的对应播放器；重复释放无操作。
    pub(crate) fn end_pointer(&mut self, window: &mut gpui::Window, cx: &mut Context<Self>) {
        let Some(kind) = self.session.drag.take() else {
            return;
        };
        if kind == Pointer::Timeline || (kind == Pointer::PreviewProgress && self.timeline.preview)
        {
            self.seek_timeline(self.session.timeline_position, true, cx);
        }
        if std::mem::take(&mut self.session.resume_after_drag) {
            if kind == Pointer::PreviewProgress && self.timeline.preview {
                self.play_timeline(window, cx);
            } else {
                self.play_preview(window, cx);
            }
        }
        cx.notify();
    }
}

/// 将横坐标映射到 0–1；滑块减去圆点半径，预览和标尺使用完整有效宽度。
fn normalized(bounds: Bounds<Pixels>, x: Pixels, inset: bool) -> f32 {
    let padding = if inset { 5.0 } else { 0.0 };
    ((f32::from(x - bounds.left()) - padding)
        / (f32::from(bounds.size.width) - padding * 2.0).max(1.0))
    .clamp(0.0, 1.0)
}
