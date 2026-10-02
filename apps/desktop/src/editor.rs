//! 剪辑主体布局：三栏、预览控制、分组工具栏、时间线和任务栏。

use dinuosi_icons::Icon;
use gpui::{Context, Div, Window, div, prelude::*, px, rgb, svg};

use crate::ui::{LINE, MUTED, PANEL, TEXT, separator, tool};
use crate::view::Desktop;

/// 草图主要边界的逻辑像素位置，随窗口尺寸按相同比例缩放。
#[derive(Clone, Copy)]
pub(crate) struct Layout {
    pub title: f32,
    upper: f32,
    toolbar: f32,
    ruler: f32,
    footer: f32,
    left: f32,
    preview: f32,
    right: f32,
    track_header: f32,
}

impl Layout {
    /// 将草图的归一化分界换算为逻辑像素；窗口最小尺寸由启动入口限制。
    /// 三栏宽度相加等于可用宽度，避免窗口缩放时累计取整偏差。
    pub fn new(width: f32, height: f32) -> Self {
        Self {
            title: height * 0.067,
            upper: height * 0.506,
            toolbar: height * 0.041,
            ruler: height * 0.039,
            footer: height * 0.037,
            left: width * 0.302,
            preview: width * 0.439,
            right: width * 0.259,
            track_header: width * 0.259,
        }
    }
}

impl Desktop {
    /// 构造三栏与底部区域；预览控制只放中栏，左右面板延伸至工具栏上沿。
    /// 无素材时不伪造媒体、片段或播放状态，剪辑操作呈现禁用状态。
    pub(crate) fn editor(&self, layout: Layout, window: &Window, cx: &mut Context<Self>) -> Div {
        div()
            .size_full()
            .flex()
            .flex_col()
            .child(self.editor_titlebar(layout.title, window, cx))
            .child(
                div()
                    .flex()
                    .flex_none()
                    .h(px(layout.upper))
                    .child(
                        empty_panel("素材库", layout.left)
                            .border_r_1()
                            .border_color(rgb(LINE)),
                    )
                    .child(
                        div()
                            .debug_selector(|| "preview-panel".into())
                            .flex()
                            .flex_col()
                            .flex_none()
                            .w(px(layout.preview))
                            .h_full()
                            .child(
                                div().flex_1().min_h_0().relative().bg(rgb(0x242426)).child(
                                    div()
                                        .absolute()
                                        .top(px(68.0))
                                        .w_full()
                                        .text_center()
                                        .text_color(rgb(MUTED))
                                        .child("实时预览"),
                                ),
                            )
                            .child(self.preview_controls(layout.preview, cx)),
                    )
                    .child(
                        empty_panel("检查器", layout.right)
                            .border_l_1()
                            .border_color(rgb(LINE)),
                    ),
            )
            .child(self.timeline_toolbar(layout.toolbar, cx))
            .child(self.timeline(layout))
            .child(
                div()
                    .flex()
                    .items_center()
                    .flex_none()
                    .h(px(layout.footer))
                    .px(px(16.0))
                    .border_t_1()
                    .border_color(rgb(LINE))
                    .text_size(px(12.0))
                    .text_color(rgb(MUTED))
                    .child("当前任务：无"),
            )
    }

    /// 绘制时间码、播放、音量和全屏控件；无媒体时禁用播放和全屏。
    /// 扬声器切换会话音量，尚未连接媒体引擎，不会伪造正在播放状态。
    fn preview_controls(&self, width: f32, cx: &mut Context<Self>) -> Div {
        div()
            .h(px(42.0))
            .flex_none()
            .flex()
            .items_center()
            .justify_between()
            .px(px(10.0))
            .border_t_1()
            .border_color(rgb(LINE))
            .child(
                div()
                    .text_size(px(10.0))
                    .font_family("Consolas")
                    .text_color(rgb(MUTED))
                    .child("00:00:00 / 00:00:00"),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(1.0))
                    .child(tool("preview-back", Icon::SkipBack, false))
                    .child(tool("preview-play", Icon::Play, false))
                    .child(tool("preview-next", Icon::SkipForward, false)),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(4.0))
                    .child(
                        tool(
                            "preview-volume",
                            if self.volume == 0.0 {
                                Icon::Mute
                            } else {
                                Icon::Volume
                            },
                            true,
                        )
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.volume = if this.volume == 0.0 { 1.0 } else { 0.0 };
                            cx.notify();
                        })),
                    )
                    .child(slider(width * 0.14, self.volume))
                    .child(tool("preview-fullscreen", Icon::Fullscreen, false)),
            )
    }

    /// 依草图顺序绘制分组工具，缩放控件独立靠右；空时间线中的剪辑工具禁用。
    /// 缩放按钮实际改变标尺密度，不修改项目或创建历史记录。
    fn timeline_toolbar(&self, height: f32, cx: &mut Context<Self>) -> Div {
        let groups: &[&[Icon]] = &[
            &[Icon::Select, Icon::Razor],
            &[Icon::Undo, Icon::Redo],
            &[Icon::Snap, Icon::Marker, Icon::Lock],
            &[
                Icon::MirrorHorizontal,
                Icon::MirrorVertical,
                Icon::ReversePlayback,
                Icon::FreezeFrame,
                Icon::Speed,
            ],
            &[Icon::LinkClips, Icon::UnlinkClips],
            &[Icon::ReplaceMedia],
            &[Icon::RippleDelete, Icon::Trash],
        ];
        div()
            .debug_selector(|| "timeline-toolbar".into())
            .h(px(height))
            .min_h(px(34.0))
            .flex_none()
            .flex()
            .items_center()
            .px(px(8.0))
            .border_y_1()
            .border_color(rgb(LINE))
            .children(groups.iter().enumerate().map(|(index, group)| {
                div()
                    .flex()
                    .items_center()
                    .when(index > 0, |s| s.child(separator()))
                    .children(
                        group
                            .iter()
                            .map(|icon| tool(format!("timeline-{}", icon.name()), *icon, false)),
                    )
            }))
            .child(div().flex_1())
            .child(
                tool("zoom-out", Icon::ZoomOut, true)
                    .on_click(cx.listener(|this, _, _, cx| this.change_zoom(-0.25, cx))),
            )
            .child(slider(110.0, (self.zoom - 0.5) / 3.5))
            .child(
                tool("zoom-in", Icon::ZoomIn, true)
                    .on_click(cx.listener(|this, _, _, cx| this.change_zoom(0.25, cx))),
            )
    }

    /// 限制缩放到 0.5–4 倍并重绘；输入为相对变化量，不影响项目内容。
    fn change_zoom(&mut self, delta: f32, cx: &mut Context<Self>) {
        self.zoom = (self.zoom + delta).clamp(0.5, 4.0);
        cx.notify();
    }

    /// 绘制空时间线、标尺和起点播放头，轨道标题分界位于宽度约 26% 处。
    /// 标尺表示可编辑时间坐标，不代表素材时长；空项目不生成示例片段。
    fn timeline(&self, layout: Layout) -> Div {
        div()
            .debug_selector(|| "timeline-panel".into())
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .relative()
            .child(
                div()
                    .h(px(layout.ruler))
                    .flex_none()
                    .flex()
                    .border_b_1()
                    .border_color(rgb(LINE))
                    .child(div().w(px(layout.track_header)).flex_none())
                    .child(
                        div()
                            .flex_1()
                            .relative()
                            .overflow_hidden()
                            .children((0..30).map(|index| {
                                let step = 100.0 * self.zoom;
                                div()
                                    .absolute()
                                    .left(px(index as f32 * step))
                                    .h_full()
                                    .border_l_1()
                                    .border_color(rgb(LINE))
                                    .pl(px(7.0))
                                    .pt(px(4.0))
                                    .text_size(px(10.0))
                                    .font_family("Consolas")
                                    .text_color(rgb(MUTED))
                                    .child(format!("{:02}:{:02}", index * 5 / 60, index * 5 % 60))
                            })),
                    ),
            )
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .child(
                        div()
                            .w(px(layout.track_header))
                            .flex_none()
                            .border_r_1()
                            .border_color(rgb(LINE)),
                    )
                    .child(
                        div().flex_1().relative().child(
                            div()
                                .absolute()
                                .top(px(76.0))
                                .w_full()
                                .text_center()
                                .text_color(rgb(MUTED))
                                .child("时间线"),
                        ),
                    ),
            )
            .child(
                div()
                    .absolute()
                    .left(px(layout.track_header - 1.0))
                    .top(px(6.0))
                    .bottom_0()
                    .w(px(1.0))
                    .bg(rgb(0xc3c3c8))
                    .child(
                        svg()
                            .path(Icon::Marker.path())
                            .absolute()
                            .left(px(-6.0))
                            .top(px(-3.0))
                            .size(px(13.0))
                            .text_color(rgb(TEXT)),
                    ),
            )
    }
}

/// 创建与草图位置一致的面板名称；不添加介绍文字，保留空工作区。
fn empty_panel(label: &'static str, width: f32) -> Div {
    div()
        .debug_selector(|| label.to_owned())
        .flex_none()
        .w(px(width))
        .h_full()
        .relative()
        .bg(rgb(PANEL))
        .child(
            div()
                .absolute()
                .top(px(62.0))
                .w_full()
                .text_center()
                .text_color(rgb(MUTED))
                .child(label),
        )
}

/// 绘制音量或缩放读数；值限制为 0–1，圆点不会越过两端。
/// 当前调节由相邻按钮提供，此元素不宣称可拖动。
fn slider(width: f32, value: f32) -> Div {
    div()
        .flex_none()
        .w(px(width))
        .h(px(20.0))
        .relative()
        .child(
            div()
                .absolute()
                .left_0()
                .right_0()
                .top(px(9.0))
                .h(px(2.0))
                .bg(rgb(LINE)),
        )
        .child(
            div()
                .absolute()
                .left(px((width - 10.0) * value.clamp(0.0, 1.0)))
                .top(px(5.0))
                .size(px(10.0))
                .rounded(px(5.0))
                .border_1()
                .border_color(rgb(MUTED))
                .bg(rgb(0x333336)),
        )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 在最小、默认和大屏尺寸验证三栏与上下分界，防止缩放后布局退化。
    #[test]
    fn layout_preserves_reference_boundaries_at_supported_sizes() {
        for (width, height) in [(1040.0, 640.0), (1440.0, 900.0), (2560.0, 1440.0)] {
            let layout = Layout::new(width, height);
            assert!((layout.left + layout.preview + layout.right - width).abs() < 0.01);
            assert!(((layout.title + layout.upper) / height - 0.573).abs() < 0.001);
            assert!((layout.track_header / width - 0.259).abs() < 0.001);
            assert!(
                height
                    - layout.title
                    - layout.upper
                    - layout.toolbar.max(34.0)
                    - layout.ruler
                    - layout.footer
                    > height * 0.27
            );
        }
    }
}
