//! 草图时间线：短刻度、窄轨道头、片段、播放头、框选和两个独立滚动条。
use crate::{
    editor::Layout,
    timeline_state::Gesture,
    ui::{LINE, MUTED, PANEL, TEXT},
    view::Desktop,
};
use dinuosi_icons::Icon;
use gpui::{Context, Div, MouseButton, canvas, div, prelude::*, px, rgb, rgba, svg};

impl Desktop {
    /// 使用实际绘制边界命中时间线；轨道头固定，横向滚动只移动内容和刻度。
    pub(crate) fn timeline_panel(&self, layout: Layout, cx: &mut Context<Self>) -> Div {
        let header = layout.track_header;
        let origin = header + crate::trim_handle::WIDTH;
        let width = (layout.width - origin - 12.0 - crate::trim_handle::WIDTH).max(1.0);
        let pps = self.pixels_per_second();
        let entity = cx.entity();
        let position = self.session.timeline_position as f32 * pps - self.timeline.scroll_x;
        let ticks = dinuosi_core::timeline::ticks(
            self.timeline.model.fps,
            pps as f64,
            (self.timeline.scroll_x / pps) as f64,
            width as f64,
        );
        div()
            .debug_selector(|| "timeline-panel".into())
            .flex_1()
            .min_h_0()
            .relative()
            .overflow_hidden()
            .on_scroll_wheel(
                cx.listener(|this, e: &gpui::ScrollWheelEvent, _, cx| this.scroll_timeline(e, cx)),
            )
            .child(
                canvas(
                    move |bounds, _, cx| {
                        entity.update(cx, |this, _| {
                            this.timeline.bounds = bounds;
                            this.timeline.header = origin;
                            this.timeline.ruler = 38.0;
                            this.timeline.content_height = this.timeline_content_height();
                            let visible = (f32::from(bounds.size.height) - 50.0).max(1.0);
                            this.timeline.scroll_y = this
                                .timeline
                                .scroll_y
                                .clamp(0.0, (this.timeline.content_height - visible).max(0.0));
                        });
                    },
                    |_, _, _, _| {},
                )
                .absolute()
                .inset_0(),
            )
            .child(
                div()
                    .id("timeline-content")
                    .debug_selector(|| "timeline-content".into())
                    .absolute()
                    .left(px(header))
                    .right(px(12.0))
                    .top(px(38.0))
                    .bottom(px(12.0))
                    .overflow_hidden()
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, e, window, cx| this.begin_marquee(e, window, cx)),
                    )
                    .children((0..self.timeline.model.tracks.len()).map(|i| {
                        let (top, h) = self.track_geometry(i);
                        div()
                            .absolute()
                            .left_0()
                            .right_0()
                            .top(px(top - 38.0))
                            .h(px(h))
                            .bg(rgb(PANEL))
                    }))
                    .children(
                        self.timeline
                            .model
                            .clips
                            .iter()
                            .filter_map(|clip| self.timeline_clip(clip, width, cx)),
                    )
                    .when(self.timeline.moved, |s| s.children(self.drop_preview(cx)))
                    .when_some(self.marquee_rectangle(), |s, (x, y, w, h)| {
                        s.child(
                            div()
                                .absolute()
                                .left(px(x))
                                .top(px(y - 38.0))
                                .w(px(w))
                                .h(px(h))
                                .bg(rgba(0xc7c7d022))
                                .border_1()
                                .border_color(rgba(0xd7d7da77)),
                        )
                    }),
            )
            .child(
                div()
                    .absolute()
                    .left_0()
                    .w(px(header))
                    .top(px(38.0))
                    .bottom(px(12.0))
                    .overflow_hidden()
                    .children(
                        (0..self.timeline.model.tracks.len())
                            .map(|i| self.track_header_view(i, header, cx)),
                    ),
            )
            .child(
                div()
                    .id("timeline-seek")
                    .debug_selector(|| "timeline-seek".into())
                    .absolute()
                    .left(px(origin))
                    .right(px(12.0 + crate::trim_handle::WIDTH))
                    .top_0()
                    .h(px(38.0))
                    .overflow_hidden()
                    .cursor_ew_resize()
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, e: &gpui::MouseDownEvent, window, cx| {
                            this.focus_handle.focus(window);
                            this.begin_pointer(crate::sliders::Pointer::Timeline, e.position.x, cx);
                            cx.stop_propagation();
                        }),
                    )
                    .child(self.pointer_bounds(crate::sliders::Pointer::Timeline, cx))
                    .children(ticks.into_iter().map(|tick| {
                        let x = tick.frame as f32 / self.timeline.model.fps as f32 * pps
                            - self.timeline.scroll_x;
                        div()
                            .absolute()
                            .left(px(x))
                            .top_0()
                            .w(px(1.0))
                            .h(px(if tick.major { 15.0 } else { 7.0 }))
                            .bg(rgb(MUTED))
                            .when(tick.major, |s| {
                                s.child(
                                    div()
                                        .absolute()
                                        .top(px(17.0))
                                        .left(px(4.0))
                                        .text_size(px(10.0))
                                        .font_family("Consolas")
                                        .text_color(rgb(MUTED))
                                        .whitespace_nowrap()
                                        .child(tick.label),
                                )
                            })
                    })),
            )
            .when(position >= 0.0 && position <= width, |s| {
                s.child(
                    div()
                        .absolute()
                        .left(px(origin + position))
                        .top(px(2.0))
                        .bottom(px(12.0))
                        .w(px(1.0))
                        .bg(rgb(TEXT))
                        .child(
                            div()
                                .id("timeline-playhead")
                                .debug_selector(|| "timeline-playhead".into())
                                .absolute()
                                .left(px(-10.0))
                                .top(px(-3.0))
                                .size(px(21.0))
                                .cursor_ew_resize()
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(|this, e: &gpui::MouseDownEvent, _, cx| {
                                        this.begin_pointer(
                                            crate::sliders::Pointer::Timeline,
                                            e.position.x,
                                            cx,
                                        );
                                        cx.stop_propagation();
                                    }),
                                )
                                .child(
                                    svg()
                                        .path(Icon::PlayheadHead.path())
                                        .size_full()
                                        .text_color(rgb(TEXT)),
                                ),
                        ),
                )
            })
            .child(self.timeline_scrollbar(false, cx))
            .child(self.timeline_scrollbar(true, cx))
            // 分隔线必须最后绘制，避免轨道头和浅色轨道背景盖住它；不占用外置把手预留区。
            .child(
                div()
                    .debug_selector(|| "timeline-header-divider".into())
                    .absolute()
                    .left(px(header - 1.0))
                    .top_0()
                    .bottom(px(12.0))
                    .w(px(1.0))
                    .bg(rgb(LINE)),
            )
    }
    /// 将框选的窗口坐标裁到可滚动内容，反向拖动得到相同矩形。
    fn marquee_rectangle(&self) -> Option<(f32, f32, f32, f32)> {
        let Gesture::Marquee { start, .. } = self.timeline.gesture.as_ref()? else {
            return None;
        };
        let start = self.timeline_gesture_start(*start);
        let p = self.timeline.pointer;
        let b = self.timeline.bounds;
        let x = f32::from(start.x.min(p.x) - b.left()) - self.timeline.header;
        let y = f32::from(start.y.min(p.y) - b.top());
        Some((
            x + crate::trim_handle::WIDTH,
            y,
            f32::from(start.x - p.x).abs(),
            f32::from(start.y - p.y).abs(),
        ))
    }
    /// 在轨道边界或冲突轨道上方绘制插轨线；素材落点预览与最终核心放置规则一致。
    fn drop_preview(&self, _cx: &mut Context<Self>) -> Option<Div> {
        use crate::timeline_state::DropTarget;
        let target = self.timeline_drop_target(self.timeline.pointer)?;
        let (DropTarget::Track(track) | DropTarget::Insert(track)) = target;
        let inserted = matches!(target, DropTarget::Insert(_));
        let insertion = if !inserted {
            match self.timeline.gesture.as_ref()? {
                Gesture::Library { .. } => {
                    self.timeline_clip_insertion(&self.timeline.model, track, &Default::default())
                }
                Gesture::Move { .. } if self.timeline.selected.len() == 1 => self
                    .timeline_clip_insertion(&self.timeline.model, track, &self.timeline.selected),
                _ => None,
            }
        } else {
            None
        };
        if let Some(frame) = insertion {
            let (top, height) = self.track_geometry(track);
            let x = frame as f32 / self.timeline.model.fps as f32 * self.pixels_per_second()
                - self.timeline.scroll_x;
            return Some(
                div()
                    .debug_selector(|| "timeline-clip-insert-line".into())
                    .absolute()
                    .left(px(x + crate::trim_handle::WIDTH))
                    .top(px(top - 38.0))
                    .w(px(2.0))
                    .h(px(height))
                    .bg(rgb(TEXT)),
            );
        }
        let mut line = inserted;
        let mut placed = None;
        match self.timeline.gesture.as_ref()? {
            Gesture::Library { index, .. } => {
                if !inserted
                    && (self.timeline.model.tracks[track].locked
                        || self.timeline.model.tracks[track].kind
                            == dinuosi_core::timeline::TrackKind::Subtitle)
                {
                    return None;
                }
                let m = self.session.media.get(*index)?;
                let start = self.timeline_frame_at(self.timeline.pointer.x);
                let duration = if self.session.selected == Some(*index) {
                    self.timeline.model.frame(self.session.trim_end)
                        - self.timeline.model.frame(self.session.trim_start)
                } else {
                    self.timeline.model.frame(m.duration)
                };
                line |= self.timeline.model.clips.iter().any(|c| {
                    c.track == track && c.start < start + duration && start < c.start + c.duration
                });
                if !line {
                    placed = Some((start, duration));
                }
            }
            Gesture::Move {
                start,
                track: origin,
            } => {
                let next = self.clip_drop_model(&self.timeline.model, *start, *origin)?;
                line |= next.tracks.len() > self.timeline.model.tracks.len();
                if !line {
                    placed = next
                        .clips
                        .iter()
                        .find(|c| self.timeline.selected.contains(&c.id))
                        .map(|c| (c.start, c.duration));
                }
            }
            Gesture::Track { index, .. } => {
                line = *index != track;
            }
            _ => return None,
        }
        let (top, h) = self.track_geometry(track);
        if line {
            Some(
                div()
                    .debug_selector(|| "timeline-insert-line".into())
                    .absolute()
                    .left_0()
                    .right_0()
                    .top(px((top - 44.0).max(0.0)))
                    .h(px(2.0))
                    .bg(rgb(TEXT)),
            )
        } else {
            let (start, duration) = placed?;
            let x = start as f32 / self.timeline.model.fps as f32 * self.pixels_per_second()
                - self.timeline.scroll_x;
            Some(
                div()
                    .absolute()
                    .left(px(x + crate::trim_handle::WIDTH))
                    .top(px(top - 34.0))
                    .h(px(h - 8.0))
                    .w(px(
                        duration as f32 / self.timeline.model.fps as f32 * self.pixels_per_second()
                    ))
                    .rounded(px(5.0))
                    .border_1()
                    .border_color(rgb(TEXT))
                    .bg(rgba(0xc7c7d055)),
            )
        }
    }
}
