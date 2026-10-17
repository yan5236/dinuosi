//! 剪辑主体布局：三栏、预览控制、分组工具栏、时间线和任务栏。

use dinuosi_icons::Icon;
use gpui::{Context, Div, Window, div, prelude::*, px, rgb};

pub(crate) use crate::editor_layout::Layout;
use crate::editor_layout::Split;
use crate::ui::{LINE, MUTED, separator};
use crate::view::Desktop;

impl Desktop {
    /// 构造三栏与底部区域；预览控制只放中栏，左右面板延伸至工具栏上沿。
    /// 时间线与检查器委派独立视图，预览模式由用户的素材/时间线选择决定。
    /// 绘制前同步时间线横向尺寸，使首次显示和窗口缩放使用当帧宽度计算全览比例。
    /// 缝隙提供三栏宽度、上下工作区和曲线高度拖动；尺寸约束由窗口布局统一计算。
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
                    .child(self.library(layout.left, cx).overflow_hidden())
                    .child(self.panel_splitter(Split::Library, layout, cx))
                    .child(
                        div()
                            .debug_selector(|| "preview-panel".into())
                            .flex()
                            .flex_col()
                            .flex_none()
                            .w(px(layout.preview))
                            .h_full()
                            .overflow_hidden()
                            .child(self.preview_display(cx))
                            .when(
                                !self.timeline.preview && self.session.selected.is_some(),
                                |s| s.child(self.preview_strip(cx)),
                            )
                            .child(self.preview_controls(layout.preview, cx)),
                    )
                    .child(self.panel_splitter(Split::Inspector, layout, cx))
                    .child(self.inspector(layout.right, cx)),
            )
            .child(self.panel_splitter(Split::Timeline, layout, cx))
            .child(self.timeline_toolbar(layout.toolbar, cx))
            .when(self.timeline.curves.open, |s| {
                s.child(self.curve_panel(layout.curve, cx).overflow_hidden())
                    .child(self.panel_splitter(Split::Curve, layout, cx))
            })
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

    /// 依草图顺序绘制分组工具，接通编辑历史、倒放和定格；工具按历史、素材类型及锁定状态启用。
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
                                    && self.can_mirror_clips())
                                || (matches!(*icon, Icon::LinkClips | Icon::UnlinkClips)
                                    && self.can_link_clips(*icon == Icon::UnlinkClips))
                                || (matches!(*icon, Icon::Undo | Icon::Redo)
                                    && !self.busy
                                    && self.timeline.history.can_step(*icon == Icon::Redo))
                                || (matches!(*icon, Icon::ReversePlayback | Icon::FreezeFrame)
                                    && self.can_change_clip_playback(*icon == Icon::FreezeFrame)),
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
                        } else if matches!(*icon, Icon::ReversePlayback | Icon::FreezeFrame) {
                            self.selected_playback_active(*icon == Icon::FreezeFrame)
                        } else if self.timeline.blade {
                            *icon == Icon::Razor
                        } else {
                            *icon == Icon::Select
                        })
                        .on_click(cx.listener({
                            let icon = *icon;
                            move |this, _, window, cx| {
                                if matches!(icon, Icon::Undo | Icon::Redo) {
                                    this.commit_inspector_input(cx);
                                    this.end_timeline_pointer(window, cx);
                                    this.timeline_history(icon == Icon::Redo, cx);
                                    this.focus_handle.focus(window);
                                } else if matches!(icon, Icon::ReversePlayback | Icon::FreezeFrame)
                                {
                                    this.change_clip_playback(
                                        icon == Icon::FreezeFrame,
                                        window,
                                        cx,
                                    );
                                } else if icon == Icon::Snap {
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
                                } else if matches!(icon, Icon::LinkClips | Icon::UnlinkClips) {
                                    this.link_selected_clips(icon == Icon::UnlinkClips, window, cx);
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
                    "keyframe-curves",
                    Icon::Keyframe,
                    !self.busy,
                    crate::i18n::t("timeline", "Keyframe curves", cx),
                )
                .selected(self.timeline.curves.open)
                .on_click(cx.listener(|this, _, window, cx| {
                    if !this.busy {
                        this.toggle_curve_panel(window, cx);
                    }
                })),
            )
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
