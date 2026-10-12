//! 剪辑主体布局：三栏、预览控制、分组工具栏、时间线和任务栏。

use dinuosi_icons::Icon;
use gpui::{Context, Div, Window, div, prelude::*, px, rgb};

use crate::ui::{LINE, MUTED, separator};
use crate::view::Desktop;

/// 草图主要边界的逻辑像素位置，随窗口尺寸按相同比例缩放。
#[derive(Clone, Copy)]
pub(crate) struct Layout {
    pub title: f32,
    upper: f32,
    toolbar: f32,
    pub width: f32,
    footer: f32,
    left: f32,
    preview: f32,
    right: f32,
    pub track_header: f32,
}

impl Layout {
    /// 将草图的归一化分界换算为逻辑像素；窗口最小尺寸由启动入口限制。
    /// 三栏宽度相加等于可用宽度，避免窗口缩放时累计取整偏差。
    pub fn new(width: f32, height: f32) -> Self {
        Self {
            title: height * 0.067,
            upper: height * 0.506,
            toolbar: height * 0.041,
            width,
            footer: height * 0.037,
            left: width * 0.302,
            preview: width * 0.439,
            right: width * 0.259,
            track_header: width * 0.15,
        }
    }
}

impl Desktop {
    /// 构造三栏与底部区域；预览控制只放中栏，左右面板延伸至工具栏上沿。
    /// 时间线与检查器委派独立视图，预览模式由用户的素材/时间线选择决定。
    /// 绘制前同步时间线横向尺寸，使首次显示和窗口缩放使用当帧宽度计算全览比例。
    pub(crate) fn editor(
        &mut self,
        layout: Layout,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Div {
        self.timeline.bounds.size.width = px(layout.width);
        self.timeline.header = layout.track_header + crate::trim_handle::WIDTH;
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
                        self.library(layout.left, cx)
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
                            .child(self.preview_display(cx))
                            .when(
                                !self.timeline.preview && self.session.selected.is_some(),
                                |s| s.child(self.preview_strip(cx)),
                            )
                            .child(self.preview_controls(layout.preview, cx)),
                    )
                    .child(
                        self.inspector(layout.right, cx)
                            .border_l_1()
                            .border_color(rgb(LINE)),
                    ),
            )
            .child(self.timeline_toolbar(layout.toolbar, cx))
            .child(self.timeline_panel(layout, cx))
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
                    .child(crate::i18n::t("editor", "Current task: None", cx)),
            )
    }

    /// 依草图顺序绘制分组工具，接通选择、刀片、磁吸、标记、片段锁与镜像；镜像仅在选中可编辑画面片段时启用。
    /// 工具提示使用立即向下显示的 Tooltip；缩放按钮仅改变标尺密度，不创建历史记录。
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
                    .children(group.iter().map(|icon| {
                        crate::tooltip::Tooltip::tool(
                            format!("timeline-{}", icon.name()),
                            *icon,
                            matches!(
                                *icon,
                                Icon::Select | Icon::Razor | Icon::Snap | Icon::Marker
                            ) || (matches!(*icon, Icon::Trash | Icon::Lock)
                                && !self.timeline.selected.is_empty())
                                || (matches!(*icon, Icon::MirrorHorizontal | Icon::MirrorVertical)
                                    && self.can_mirror_clips()),
                            if *icon == Icon::Lock {
                                crate::i18n::t(
                                    "editor",
                                    if self.selected_clips_locked() {
                                        "Unlock clip"
                                    } else {
                                        "Lock clip"
                                    },
                                    cx,
                                )
                            } else if *icon == Icon::Snap {
                                crate::i18n::t(
                                    "editor",
                                    if self.timeline.model.snapping {
                                        "Disable snapping"
                                    } else {
                                        "Enable snapping"
                                    },
                                    cx,
                                )
                            } else if *icon == Icon::Marker {
                                crate::i18n::t("editor", "Add marker", cx)
                            } else {
                                crate::i18n::t("icons", icon.name(), cx)
                            },
                        )
                        .selected(if *icon == Icon::Snap {
                            self.timeline.model.snapping
                        } else if *icon == Icon::Lock {
                            self.selected_clips_locked()
                        } else if self.timeline.blade {
                            *icon == Icon::Razor
                        } else {
                            *icon == Icon::Select
                        })
                        .on_click(cx.listener({
                            let icon = *icon;
                            move |this, _, window, cx| {
                                if icon == Icon::Snap {
                                    this.toggle_snapping(window, cx);
                                } else if icon == Icon::Marker {
                                    this.add_timeline_marker(window, cx);
                                } else if icon == Icon::Lock {
                                    this.toggle_clip_lock(window, cx);
                                } else if matches!(
                                    icon,
                                    Icon::MirrorHorizontal | Icon::MirrorVertical
                                ) {
                                    this.mirror_selected_clips(
                                        icon == Icon::MirrorHorizontal,
                                        window,
                                        cx,
                                    );
                                } else if icon == Icon::Trash {
                                    this.delete_clips(cx);
                                } else if matches!(icon, Icon::Select | Icon::Razor) {
                                    this.set_blade_tool(icon == Icon::Razor, window, cx);
                                }
                            }
                        }))
                    }))
            }))
            .child(div().flex_1())
            .child(
                crate::tooltip::Tooltip::tool(
                    "zoom-out",
                    Icon::ZoomOut,
                    true,
                    crate::i18n::t("editor", "Zoom out timeline", cx),
                )
                .on_click(cx.listener(|this, _, _, cx| this.change_zoom(-0.25, cx))),
            )
            .child(self.slider(crate::sliders::Pointer::Zoom, 110.0, cx))
            .child(
                crate::tooltip::Tooltip::tool(
                    "zoom-in",
                    Icon::ZoomIn,
                    true,
                    crate::i18n::t("editor", "Zoom in timeline", cx),
                )
                .on_click(cx.listener(|this, _, _, cx| this.change_zoom(0.25, cx))),
            )
    }

    /// 调整对数缩放参数；最小档回到时间零点全览，其他档保持视口左端时间并夹紧滚动范围。
    pub(crate) fn change_zoom(&mut self, delta: f32, cx: &mut Context<Self>) {
        let left = self.timeline.scroll_x / self.pixels_per_second();
        self.zoom = (self.zoom + delta).clamp(0.5, 4.0);
        self.timeline.scroll_x = if self.zoom == 0.5 {
            0.0
        } else {
            (left * self.pixels_per_second())
                .clamp(0.0, self.timeline_width() - self.timeline_view_width())
        };
        cx.notify();
    }
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
            assert!((layout.track_header / width - 0.15).abs() < 0.001);
            assert!(
                height - layout.title - layout.upper - layout.toolbar.max(34.0) - layout.footer
                    > height * 0.27
            );
        }
    }
}
