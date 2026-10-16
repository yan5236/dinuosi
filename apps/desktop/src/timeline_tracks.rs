//! 时间线固定轨道头和滚动条，位置由会话中的实际内容尺寸计算。
use crate::{
    timeline_state::Gesture,
    tooltip::Tooltip,
    ui::{PANEL, TEXT},
    view::Desktop,
};
use dinuosi_icons::Icon;
use gpui::{Context, Div, MouseButton, div, prelude::*, px, rgb, svg};

/// 按职责翻译默认轨道名；用户明确重命名后的名称保持原文，即使与某个默认名相同。
pub(crate) fn track_name(track: &dinuosi_core::timeline::Track, cx: &gpui::App) -> String {
    if track.custom_name {
        return track.name.clone();
    }
    let key = match track.kind {
        dinuosi_core::timeline::TrackKind::Main => "Main track",
        dinuosi_core::timeline::TrackKind::Media => "Media track",
        dinuosi_core::timeline::TrackKind::Audio => "Audio track",
        dinuosi_core::timeline::TrackKind::Subtitle => "Subtitle track",
        dinuosi_core::timeline::TrackKind::Text => "Text track",
    };
    crate::i18n::t("timeline", key, cx).to_owned()
}
impl Desktop {
    /// 绘制实际轨道的名称、重命名、锁定、可见和静音控件，行高与片段严格共用。
    /// 自由轨道名称接入整体拖动；媒体、字幕和音频图标及静音入口按类型判断，不依赖行号。
    pub(crate) fn track_header_view(
        &self,
        index: usize,
        width: f32,
        cx: &mut Context<Self>,
    ) -> Div {
        let track = &self.timeline.model.tracks[index];
        let (top, h) = self.track_geometry(index);
        div()
            .absolute()
            .left_0()
            .top(px(top - 38.0))
            .w(px(width))
            .h(px(h))
            .overflow_hidden()
            .flex()
            .items_center()
            .gap(px(3.0))
            .px(px(8.0))
            .bg(rgb(PANEL))
            .child(
                svg()
                    .path(match track.kind {
                        dinuosi_core::timeline::TrackKind::Subtitle
                        | dinuosi_core::timeline::TrackKind::Text => Icon::Text.path(),
                        dinuosi_core::timeline::TrackKind::Audio => Icon::Audio.path(),
                        _ => Icon::Video.path(),
                    })
                    .size(px(19.0))
                    .flex_none()
                    .text_color(rgb(TEXT)),
            )
            .child(
                div()
                    .id(gpui::SharedString::from(format!("track-drag-{index}")))
                    .debug_selector(move || format!("track-drag-{index}"))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, e, window, cx| {
                            this.begin_track_drag(index, e, window, cx)
                        }),
                    )
                    .flex_1()
                    .min_w_0()
                    .text_size(px(13.0))
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .when(
                        self.timeline
                            .rename
                            .as_ref()
                            .is_none_or(|(i, _)| *i != index),
                        |s| s.child(track_name(track, cx)),
                    )
                    .when_some(
                        self.timeline.rename.as_ref().filter(|(i, _)| *i == index),
                        |s, (_, input)| s.child(input.clone()),
                    ),
            )
            .child(
                Tooltip::tool(
                    format!("track-rename-{index}"),
                    Icon::Edit,
                    true,
                    crate::i18n::t("timeline", "Rename track", cx),
                )
                .on_click(
                    cx.listener(move |this, _, window, cx| {
                        this.begin_track_name(index, window, cx)
                    }),
                ),
            )
            .child(
                Tooltip::tool(
                    format!("track-lock-{index}"),
                    if track.locked {
                        Icon::Lock
                    } else {
                        Icon::Unlock
                    },
                    true,
                    if track.locked {
                        crate::i18n::t("timeline", "Unlock track", cx)
                    } else {
                        crate::i18n::t("timeline", "Lock track", cx)
                    },
                )
                .on_click(cx.listener(move |this, _, _, cx| this.toggle_track(index, 0, cx))),
            )
            .child(
                Tooltip::tool(
                    format!("track-visible-{index}"),
                    if track.visible {
                        Icon::Eye
                    } else {
                        Icon::EyeClosed
                    },
                    true,
                    if track.visible {
                        crate::i18n::t("timeline", "Hide track", cx)
                    } else {
                        crate::i18n::t("timeline", "Show track", cx)
                    },
                )
                .on_click(cx.listener(move |this, _, _, cx| this.toggle_track(index, 1, cx))),
            )
            .when(
                !matches!(
                    track.kind,
                    dinuosi_core::timeline::TrackKind::Subtitle
                        | dinuosi_core::timeline::TrackKind::Text
                ),
                |s| {
                    s.child(
                        Tooltip::tool(
                            format!("track-mute-{index}"),
                            if track.muted {
                                Icon::Mute
                            } else {
                                Icon::Volume
                            },
                            true,
                            if track.muted {
                                crate::i18n::t("timeline", "Unmute track", cx)
                            } else {
                                crate::i18n::t("timeline", "Mute track", cx)
                            },
                        )
                        .on_click(
                            cx.listener(move |this, _, _, cx| this.toggle_track(index, 2, cx)),
                        ),
                    )
                },
            )
    }
    /// 绘制真实比例滚动条，拖拽滑块按内容/视口比例移动，点击轨道跳转相应位置。
    pub(crate) fn timeline_scrollbar(
        &self,
        vertical: bool,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<Div> {
        let visible = if vertical {
            (f32::from(self.timeline.bounds.size.height) - 50.0).max(1.0)
        } else {
            self.timeline_view_width()
        };
        let total = if vertical {
            self.timeline.content_height.max(visible)
        } else {
            self.timeline_width()
        };
        let offset = if vertical {
            self.timeline.scroll_y
        } else {
            self.timeline.scroll_x
        };
        let thumb = (visible / total * visible).max(18.0).min(visible);
        let position = offset / (total - visible).max(1.0) * (visible - thumb);
        div()
            .id(if vertical {
                "timeline-vscroll-track"
            } else {
                "timeline-hscroll-track"
            })
            .absolute()
            .when(vertical, |s| {
                s.right_0().top(px(38.0)).bottom(px(12.0)).w(px(12.0))
            })
            .when(!vertical, |s| {
                s.left(px(self.timeline.header))
                    .right(px(12.0 + crate::trim_handle::WIDTH))
                    .bottom_0()
                    .h(px(12.0))
            })
            .bg(rgb(0x252527))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, event: &gpui::MouseDownEvent, _, cx| {
                    let local = if vertical {
                        f32::from(event.position.y - this.timeline.bounds.top()) - 38.0
                    } else {
                        f32::from(event.position.x - this.timeline.bounds.left())
                            - this.timeline.header
                    };
                    let offset = ((local - thumb / 2.0) / (visible - thumb).max(1.0)
                        * (total - visible))
                        .clamp(0.0, (total - visible).max(0.0));
                    if vertical {
                        this.timeline.scroll_y = offset;
                    } else {
                        this.timeline.scroll_x = offset;
                    }
                    cx.notify();
                    cx.stop_propagation();
                }),
            )
            .child(
                div()
                    .id(if vertical {
                        "timeline-vscroll"
                    } else {
                        "timeline-hscroll"
                    })
                    .debug_selector(move || {
                        if vertical {
                            "timeline-vscroll".into()
                        } else {
                            "timeline-hscroll".into()
                        }
                    })
                    .absolute()
                    .rounded(px(4.0))
                    .bg(rgb(0x68686d))
                    .when(vertical, |s| {
                        s.top(px(position)).left(px(3.0)).w(px(6.0)).h(px(thumb))
                    })
                    .when(!vertical, |s| {
                        s.left(px(position)).top(px(3.0)).h(px(6.0)).w(px(thumb))
                    })
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, e: &gpui::MouseDownEvent, _, cx| {
                            this.timeline.gesture = Some(if vertical {
                                Gesture::Vertical {
                                    start: e.position,
                                    offset: this.timeline.scroll_y,
                                }
                            } else {
                                Gesture::Horizontal {
                                    start: e.position,
                                    offset: this.timeline.scroll_x,
                                }
                            });
                            cx.stop_propagation();
                        }),
                    ),
            )
    }
}
