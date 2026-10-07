//! 素材画面、长度选区和控制条，沿用现有灰阶和轻线图标。

use crate::{
    media::{self, MediaKind},
    sliders::Pointer,
    ui::{LINE, PANEL, TEXT, pill, tool},
    view::Desktop,
};
use dinuosi_icons::Icon;
use gpui::{Context, Div, MouseButton, ObjectFit, div, img, prelude::*, px, relative, rgb, svg};

impl Desktop {
    /// 按素材显示比例居中绘制完整画面；四周随面板尺寸留白，透明补边透出灰色背景。
    /// 图片采用绝对定位避免固有尺寸撑开布局；音频素材显示完整源波形，时间线叠加变换选择框。
    pub(crate) fn preview_display(&self, cx: &mut Context<Self>) -> Div {
        let selected = self
            .session
            .selected
            .and_then(|i| self.session.media.get(i));
        let offline = !self.timeline.preview && selected.is_some_and(|m| m.offline);
        let audio = !self.timeline.preview && selected.is_some_and(|m| m.kind == MediaKind::Audio);
        div()
            .debug_selector(|| "preview-display".into())
            .flex_1()
            .min_h_0()
            .relative()
            .bg(rgb(PANEL))
            .child(
                div()
                    .debug_selector(|| "preview-viewport".into())
                    .absolute()
                    .left(relative(0.04))
                    .right(relative(0.04))
                    .top(relative(0.04))
                    .bottom(relative(0.04))
                    .overflow_hidden()
                    .bg(rgb(PANEL))
                    .when(offline, |s| {
                        s.child(crate::media_status::offline_view(false, cx))
                    })
                    .when_some(
                        if self.timeline.preview {
                            self.timeline.frame.clone()
                        } else {
                            self.session.frame.clone().filter(|_| !offline)
                        },
                        |s, frame| {
                            s.child(
                                img(frame)
                                    .absolute()
                                    .size_full()
                                    .object_fit(ObjectFit::Contain),
                            )
                        },
                    )
                    .when(self.timeline.preview, |s| {
                        s.child(self.preview_transform_overlay(cx))
                    })
                    .when(audio && !offline, |s| {
                        s.child(
                            div()
                                .debug_selector(|| "preview-waveform".into())
                                .absolute()
                                .inset_0()
                                .child(crate::waveform::waveform_view(
                                    selected.and_then(|m| m.waveform.clone()),
                                    0.0,
                                    selected.map_or(0.0, |m| m.duration),
                                )),
                        )
                    }),
            )
    }

    /// 绘制预览长度选区、音频源波形、两端拖柄和可寻址播放头；范围与设备实际播放一致。
    /// 所有位置使用实际轨道宽度比例，与鼠标寻址共用边界；把手最后绘制以遮盖边框和播放线。
    pub(crate) fn preview_strip(&self, cx: &mut Context<Self>) -> Div {
        let duration = if self
            .session
            .selected
            .and_then(|i| self.session.media.get(i))
            .is_some_and(|m| m.offline)
        {
            0.0
        } else {
            self.session.duration()
        };
        let start = if duration > 0.0 {
            self.session.trim_start / duration
        } else {
            0.0
        };
        let end = if duration > 0.0 {
            self.session.trim_end / duration
        } else {
            1.0
        };
        let position = if duration > 0.0 {
            self.session.position / duration
        } else {
            0.0
        };
        div()
            .h(px(58.0))
            .flex_none()
            .px(px(24.0))
            .pt(px(14.0))
            .pb(px(7.0))
            .bg(rgb(PANEL))
            .when(duration > 0.0, |s| {
                s.child(
                    div()
                        .id("preview-seek")
                        .debug_selector(|| "preview-seek".into())
                        .size_full()
                        .relative()
                        .cursor_pointer()
                        .bg(rgb(0x242426))
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|this, event: &gpui::MouseDownEvent, _, cx| {
                                this.begin_pointer(Pointer::Seek, event.position.x, cx);
                                cx.stop_propagation();
                            }),
                        )
                        .child(self.pointer_bounds(Pointer::Seek, cx))
                        .children(self.session.strip.iter().enumerate().map(|(index, frame)| {
                            let count = self.session.strip.len().max(1) as f32;
                            div()
                                .absolute()
                                .left(relative(index as f32 / count))
                                .w(relative(1.0 / count))
                                .h_full()
                                .overflow_hidden()
                                .when_some(Some(frame.clone()), |s, frame| {
                                    s.child(img(frame).size_full().object_fit(ObjectFit::Cover))
                                })
                        }))
                        .when_some(
                            self.session
                                .selected
                                .and_then(|i| self.session.media.get(i))
                                .filter(|m| m.kind == MediaKind::Audio && !m.offline),
                            |s, media| {
                                s.child(
                                    div()
                                        .debug_selector(|| "preview-strip-waveform".into())
                                        .absolute()
                                        .inset_0()
                                        .child(crate::waveform::waveform_view(
                                            media.waveform.clone(),
                                            0.0,
                                            media.duration,
                                        )),
                                )
                            },
                        )
                        .child(
                            div()
                                .debug_selector(|| "preview-selection".into())
                                .absolute()
                                .left(relative(start as f32))
                                .w(relative((end - start) as f32))
                                .h_full()
                                .border_1()
                                .border_color(rgb(TEXT)),
                        )
                        .child(
                            div()
                                .debug_selector(|| "preview-position".into())
                                .absolute()
                                .left(relative(position as f32))
                                .top(px(-12.0))
                                .bottom_0()
                                .w(px(1.0))
                                .bg(rgb(TEXT))
                                .child(
                                    div()
                                        .id("preview-playhead")
                                        .debug_selector(|| "preview-playhead".into())
                                        .absolute()
                                        .left(px(-9.0))
                                        .top(px(-5.0))
                                        .size(px(18.0))
                                        .cursor_ew_resize()
                                        .on_mouse_down(
                                            MouseButton::Left,
                                            cx.listener(
                                                |this, event: &gpui::MouseDownEvent, _, cx| {
                                                    this.begin_pointer(
                                                        Pointer::Seek,
                                                        event.position.x,
                                                        cx,
                                                    );
                                                    cx.stop_propagation();
                                                },
                                            ),
                                        )
                                        .child(
                                            svg()
                                                .path(Icon::PlayheadHead.path())
                                                .size_full()
                                                .text_color(rgb(TEXT)),
                                        ),
                                ),
                        )
                        .children(
                            [
                                (Pointer::TrimStart, start, "trim-start"),
                                (Pointer::TrimEnd, end, "trim-end"),
                            ]
                            .into_iter()
                            .map(|(kind, ratio, id)| {
                                div()
                                    .absolute()
                                    .left(relative(ratio as f32))
                                    .h_full()
                                    .w_0()
                                    .child(
                                        crate::trim_handle::trim_handle(
                                            id,
                                            kind == Pointer::TrimStart,
                                        )
                                        .on_mouse_down(
                                            MouseButton::Left,
                                            cx.listener(
                                                move |this, event: &gpui::MouseDownEvent, _, cx| {
                                                    this.begin_pointer(kind, event.position.x, cx);
                                                    cx.stop_propagation();
                                                },
                                            ),
                                        ),
                                    )
                            }),
                        ),
                )
            })
    }

    /// 按素材或时间线模式绘制时间码、播放与定位；素材模式可将当前选区添加到主轨。
    pub(crate) fn preview_controls(&self, width: f32, cx: &mut Context<Self>) -> Div {
        let enabled = self.timeline.preview && !self.timeline.model.clips.is_empty()
            || self
                .session
                .selected
                .and_then(|i| self.session.media.get(i))
                .is_some_and(|m| !m.offline);
        div()
            .h(px(42.0))
            .flex_none()
            .flex()
            .items_center()
            .justify_between()
            .px(px(8.0))
            .border_t_1()
            .border_color(rgb(LINE))
            .child(
                div()
                    .font_family("Consolas")
                    .text_size(px(10.0))
                    .child(format!(
                        "{} / {}",
                        media::timecode(if self.timeline.preview {
                            self.session.timeline_position
                        } else {
                            self.session.position
                        }),
                        media::timecode(if self.timeline.preview {
                            self.timeline.model.end() as f64 / self.timeline.model.fps as f64
                        } else {
                            self.session.duration()
                        })
                    )),
            )
            .when(enabled && !self.timeline.preview, |s| {
                s.child(
                    pill(
                        "add-to-track",
                        crate::i18n::t("editor", "Add to track", cx),
                        82.0,
                        24.0,
                    )
                    .on_click(cx.listener(|this, _, window, cx| {
                        if let Some(index) = this.session.selected {
                            this.insert_media_clip(
                                index,
                                this.timeline.model.main_track(),
                                this.timeline.model.frame(this.session.timeline_position),
                                window,
                                cx,
                            );
                        }
                    })),
                )
            })
            .child(
                div()
                    .flex()
                    .items_center()
                    .child(
                        tool("preview-back", Icon::SkipBack, enabled).on_click(cx.listener(
                            |this, _, _, cx| {
                                if this.timeline.preview {
                                    this.seek_timeline(0.0, true, cx);
                                    return;
                                }
                                this.stop_preview();
                                this.session.position = this.session.trim_start;
                                this.request_frame(cx);
                                cx.notify();
                            },
                        )),
                    )
                    .child(
                        tool(
                            "preview-play",
                            if self.session.player.is_some()
                                || self.session.starting
                                || self.timeline.playing
                            {
                                Icon::Pause
                            } else {
                                Icon::Play
                            },
                            enabled,
                        )
                        .on_click(cx.listener(|this, _, window, cx| {
                            if this.timeline.preview {
                                this.play_timeline(window, cx);
                            } else {
                                this.play_preview(window, cx);
                            }
                        })),
                    )
                    .child(
                        tool("preview-next", Icon::SkipForward, enabled).on_click(cx.listener(
                            |this, _, _, cx| {
                                if this.timeline.preview {
                                    this.seek_timeline(
                                        this.timeline.model.end() as f64
                                            / this.timeline.model.fps as f64,
                                        true,
                                        cx,
                                    );
                                    return;
                                }
                                this.stop_preview();
                                this.session.position = this.session.trim_end;
                                this.request_frame(cx);
                                cx.notify();
                            },
                        )),
                    ),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(2.0))
                    .child(
                        tool(
                            "preview-volume",
                            if self.volume == 0.0 {
                                Icon::Mute
                            } else if self.volume < 0.5 {
                                Icon::VolumeLow
                            } else {
                                Icon::Volume
                            },
                            true,
                        )
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.volume = if this.volume == 0.0 { 1.0 } else { 0.0 };
                            if let Some(sound) = &this.timeline.sound {
                                sound.sink.set_volume(this.volume);
                            }
                            if let Some(player) = &this.session.player {
                                player.volume(this.volume);
                            }
                            cx.notify();
                        })),
                    )
                    .child(self.slider(Pointer::Volume, (width * 0.12).max(40.0), cx))
                    .child(
                        tool("preview-fullscreen", Icon::Fullscreen, enabled)
                            .on_click(cx.listener(|_, _, window, _| window.toggle_fullscreen())),
                    ),
            )
    }
}
