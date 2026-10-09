//! 全屏观看与逐帧定位；复用现有素材/时间线播放器，不挂载任何编辑控件。
use crate::{
    sliders::Pointer,
    ui::{LINE, TEXT, tool},
    view::Desktop,
};
use dinuosi_icons::Icon;
use gpui::{
    Context, Div, KeyDownEvent, MouseButton, Window, deferred, div, prelude::*, px, relative, rgb,
    rgba,
};

impl Desktop {
    /// 按项目帧率暂停并前后移动一帧；direction 为 -1 或 1，夹紧到当前模式的可播放帧范围。
    /// 不修改素材选区或项目；时间线尾端停在最后有效帧，素材模式保留裁剪边界。
    pub(crate) fn step_preview_frame(&mut self, direction: i64, cx: &mut Context<Self>) {
        let fps = self.timeline.model.fps as f64;
        if self.timeline.preview {
            if self.timeline.model.clips.is_empty() {
                return;
            }
            self.stop_timeline();
            let frame = (self.timeline.model.frame(self.session.timeline_position) + direction)
                .clamp(0, (self.timeline.model.end() - 1).max(0));
            self.seek_timeline(frame as f64 / fps, true, cx);
        } else {
            if !self
                .session
                .selected
                .and_then(|i| self.session.media.get(i))
                .is_some_and(|m| !m.offline)
            {
                return;
            }
            self.stop_preview();
            let first = (self.session.trim_start * fps).ceil();
            let last = (self.session.trim_end * fps).ceil() - 1.0;
            self.session.position = (((self.session.position * fps).round() + direction as f64)
                .clamp(first, last.max(first))
                / fps)
                .clamp(self.session.trim_start, self.session.trim_end);
            self.request_frame(cx);
            cx.notify();
        }
    }

    /// 进入或退出观看模式并同步原生全屏；提交已有输入、结束手势、关闭菜单并将焦点移回播放器。
    /// 保留播放模式、位置与播放状态；已有参数输入按原事务提交，观看期间不提供项目编辑入口。
    pub(crate) fn toggle_preview_fullscreen(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.preview_fullscreen
            && !(self.timeline.preview && !self.timeline.model.clips.is_empty()
                || !self.timeline.preview
                    && self
                        .session
                        .selected
                        .and_then(|i| self.session.media.get(i))
                        .is_some_and(|m| !m.offline))
        {
            return;
        }
        self.commit_inspector_input(cx);
        self.end_pointer(window, cx);
        self.end_timeline_pointer(window, cx);
        self.menu_open = false;
        self.help_open = false;
        self.project_menu = None;
        self.session.media_menu = None;
        self.timeline.keyframe_menu = None;
        self.timeline.marker_menu = None;
        self.timeline.blade_pointer = None;
        self.preview_fullscreen = !self.preview_fullscreen;
        if window.is_fullscreen() != self.preview_fullscreen {
            window.toggle_fullscreen();
        }
        self.focus_handle.focus(window);
        cx.notify();
    }

    /// 全屏输入只处理 Escape 退出、空格播放及 Tab 聚焦；吞掉剪辑、删除和项目快捷键以防误操作。
    pub(crate) fn fullscreen_preview_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event.keystroke.key.as_str() {
            "escape" => self.toggle_preview_fullscreen(window, cx),
            "space"
                if !event.keystroke.modifiers.control
                    && !event.keystroke.modifiers.alt
                    && !event.keystroke.modifiers.platform =>
            {
                if self.timeline.preview {
                    self.play_timeline(window, cx);
                } else {
                    self.play_preview(window, cx);
                }
            }
            "tab" => {
                if event.keystroke.modifiers.shift {
                    window.focus_prev();
                } else {
                    window.focus_next();
                }
            }
            _ => {}
        }
        cx.stop_propagation();
    }

    /// 只绘制等比完整画面与底部控件；控件延后绘制，确保重绘时不会被画面遮住，支持 Tab/Enter。
    /// 进度条使用当前素材裁剪范围或完整时间线，拖动后由共用寻址逻辑恢复原播放状态。
    pub(crate) fn fullscreen_preview(&self, cx: &mut Context<Self>) -> Div {
        let (position, start, end) = if self.timeline.preview {
            (
                self.session.timeline_position,
                0.0,
                self.timeline.model.end() as f64 / self.timeline.model.fps as f64,
            )
        } else {
            (
                self.session.position,
                self.session.trim_start,
                self.session.trim_end,
            )
        };
        let ratio = if end > start {
            ((position - start) / (end - start)).clamp(0.0, 1.0) as f32
        } else {
            0.0
        };
        let playing =
            self.timeline.playing || self.session.player.is_some() || self.session.starting;
        div()
            .debug_selector(|| "fullscreen-preview".into())
            .size_full()
            .relative()
            .flex()
            .flex_col()
            .bg(rgb(0x000000))
            .text_color(rgb(TEXT))
            .capture_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                if event.keystroke.key == "escape" {
                    this.fullscreen_preview_key(event, window, cx);
                }
            }))
            .on_key_down(cx.listener(Self::fullscreen_preview_key))
            .on_mouse_move(
                cx.listener(|this, event: &gpui::MouseMoveEvent, window, cx| {
                    if event.pressed_button == Some(MouseButton::Left) {
                        this.move_pointer(event.position.x, cx);
                    } else {
                        this.end_pointer(window, cx);
                    }
                }),
            )
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, _, window, cx| this.end_pointer(window, cx)),
            )
            .on_mouse_up_out(
                MouseButton::Left,
                cx.listener(|this, _, window, cx| this.end_pointer(window, cx)),
            )
            .child(div().absolute().size_0().track_focus(&self.focus_handle))
            .child(self.preview_display(cx))
            .child(deferred(
                div()
                    .absolute()
                    .left(px(24.0))
                    .right(px(24.0))
                    .bottom(px(16.0))
                    .flex()
                    .flex_col()
                    .px(px(12.0))
                    .py(px(6.0))
                    .bg(rgba(0x000000b3))
                    .child(
                        div()
                            .id("fullscreen-progress")
                            .debug_selector(|| "fullscreen-progress".into())
                            .h(px(20.0))
                            .relative()
                            .cursor_pointer()
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(|this, event: &gpui::MouseDownEvent, _, cx| {
                                    this.begin_pointer(
                                        Pointer::PreviewProgress,
                                        event.position.x,
                                        cx,
                                    );
                                    cx.stop_propagation();
                                }),
                            )
                            .child(self.pointer_bounds(Pointer::PreviewProgress, cx))
                            .child(
                                div()
                                    .absolute()
                                    .top(px(8.0))
                                    .h(px(3.0))
                                    .w_full()
                                    .bg(rgb(LINE)),
                            )
                            .child(
                                div()
                                    .absolute()
                                    .top(px(8.0))
                                    .h(px(3.0))
                                    .w(relative(ratio))
                                    .bg(rgb(TEXT)),
                            )
                            .child(
                                div()
                                    .absolute()
                                    .top(px(4.0))
                                    .left(relative(ratio))
                                    .ml(px(-5.0))
                                    .size(px(11.0))
                                    .rounded_full()
                                    .bg(rgb(TEXT)),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .justify_between()
                            .child(
                                tool(
                                    "fullscreen-play",
                                    if playing { Icon::Pause } else { Icon::Play },
                                    true,
                                )
                                .focusable()
                                .tab_stop(true)
                                .on_key_down(cx.listener(
                                    |this, event: &KeyDownEvent, window, cx| {
                                        if matches!(event.keystroke.key.as_str(), "enter" | "space")
                                        {
                                            if this.timeline.preview {
                                                this.play_timeline(window, cx);
                                            } else {
                                                this.play_preview(window, cx);
                                            }
                                            cx.stop_propagation();
                                        }
                                    },
                                ))
                                .on_click(cx.listener(
                                    |this, _, window, cx| {
                                        if this.timeline.preview {
                                            this.play_timeline(window, cx);
                                        } else {
                                            this.play_preview(window, cx);
                                        }
                                    },
                                )),
                            )
                            .child(
                                tool("fullscreen-exit", Icon::ExitFullscreen, true)
                                    .focusable()
                                    .tab_stop(true)
                                    .on_key_down(cx.listener(
                                        |this, event: &KeyDownEvent, window, cx| {
                                            if matches!(
                                                event.keystroke.key.as_str(),
                                                "enter" | "space"
                                            ) {
                                                this.toggle_preview_fullscreen(window, cx);
                                                cx.stop_propagation();
                                            }
                                        },
                                    ))
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.toggle_preview_fullscreen(window, cx)
                                    })),
                            ),
                    ),
            ))
            .when_some(self.notice.as_ref(), |s, notice| {
                s.child(
                    div()
                        .absolute()
                        .top(px(16.0))
                        .left(px(24.0))
                        .text_size(px(13.0))
                        .child(crate::i18n::error(notice, cx)),
                )
            })
    }
}
