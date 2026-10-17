//! 编辑工作区尺寸与分隔拖动；只修改窗口会话，不触碰项目、媒体或撤销历史。
use crate::{
    ui::{BACKGROUND, LINE},
    view::Desktop,
};
use gpui::{Context, Div, MouseButton, Pixels, Point, Window, div, prelude::*, px, rgb};

pub(crate) const GAP: f32 = 6.0;
const MIN_LEFT: f32 = 280.0;
const MIN_RIGHT: f32 = 280.0;
const MIN_PREVIEW: f32 = 420.0;
const MIN_UPPER: f32 = 260.0;
const MIN_TIMELINE: f32 = 120.0;
const MIN_CURVE: f32 = 140.0;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Split {
    Library,
    Inspector,
    Timeline,
    Curve,
}

#[derive(Clone, Copy)]
struct Drag {
    split: Split,
    start: Point<Pixels>,
    layout: Layout,
}

#[derive(Default)]
pub(crate) struct Panels {
    left: Option<f32>,
    right: Option<f32>,
    upper: Option<f32>,
    curve: Option<f32>,
    drag: Option<Drag>,
}

/// 单帧的逻辑像素布局；三栏扣除分隔缝隙，曲线和轨道分别保留最小操作空间。
#[derive(Clone, Copy)]
pub(crate) struct Layout {
    pub title: f32,
    pub upper: f32,
    pub toolbar: f32,
    pub width: f32,
    pub footer: f32,
    pub left: f32,
    pub preview: f32,
    pub right: f32,
    pub curve: f32,
    pub track_header: f32,
    available: f32,
}

impl Layout {
    /// 返回基础尺寸，供管理页及菜单定位复用；剪辑页通过 resolve 另行应用面板限制。
    pub fn new(width: f32, height: f32) -> Self {
        Self {
            title: height * 0.067,
            upper: height * 0.506,
            toolbar: (height * 0.041).max(34.0),
            width,
            footer: height * 0.037,
            left: width * 0.302,
            right: width * 0.259,
            preview: width * 0.439 - GAP * 2.0,
            curve: 0.0,
            track_header: width * 0.15,
            available: height * 0.896 - (height * 0.041).max(34.0) - GAP,
        }
    }

    /// 将会话中的期望尺寸限制到当前窗口；缩小窗口只夹紧显示值，放大后恢复期望尺寸。
    /// 曲线优先保留上方区域与轨道的最小高度，关闭时恢复原上方高度，不重置曲线设置。
    fn resolve(width: f32, height: f32, panels: &Panels, curves: bool) -> Self {
        let available_width = width - GAP * 2.0;
        let left = panels
            .left
            .unwrap_or(width * 0.302)
            .clamp(MIN_LEFT, available_width - MIN_RIGHT - MIN_PREVIEW);
        let right = panels
            .right
            .unwrap_or(width * 0.259)
            .clamp(MIN_RIGHT, available_width - left - MIN_PREVIEW);
        let title = height * 0.067;
        let footer = height * 0.037;
        let toolbar = (height * 0.041).max(34.0);
        let available = height - title - footer - toolbar - GAP;
        let curve = if curves {
            panels
                .curve
                .unwrap_or((height * 0.24).clamp(150.0, 240.0))
                .clamp(MIN_CURVE, available - GAP - MIN_UPPER - MIN_TIMELINE)
        } else {
            0.0
        };
        let upper = panels.upper.unwrap_or(height * 0.506).clamp(
            MIN_UPPER,
            available - MIN_TIMELINE - curve - if curves { GAP } else { 0.0 },
        );
        Self {
            title,
            upper,
            toolbar,
            width,
            footer,
            left,
            right,
            curve,
            preview: available_width - left - right,
            track_header: width * 0.15,
            available,
        }
    }
}

impl Desktop {
    /// 按当前窗口和曲线开关生成布局；媒体和时间轴继续使用各自的实际绘制坐标。
    pub(crate) fn panel_layout(&self, window: &Window) -> Layout {
        let size = window.viewport_size();
        Layout::resolve(
            size.width.into(),
            size.height.into(),
            &self.panels,
            self.timeline.curves.open,
        )
    }

    /// 绘制整个缝隙作为拖动命中区；悬停显示细亮线，按下后固定起始尺寸并隔离编辑手势。
    pub(crate) fn panel_splitter(
        &self,
        split: Split,
        layout: Layout,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<Div> {
        let vertical = matches!(split, Split::Library | Split::Inspector);
        let id = match split {
            Split::Library => "resize-library",
            Split::Inspector => "resize-inspector",
            Split::Timeline => "resize-timeline",
            Split::Curve => "resize-curve",
        };
        div()
            .id(id)
            .debug_selector(move || id.into())
            .flex_none()
            .relative()
            .bg(rgb(BACKGROUND))
            .when(vertical, |s| s.w(px(GAP)).h_full().cursor_ew_resize())
            .when(!vertical, |s| s.h(px(GAP)).w_full().cursor_ns_resize())
            .hover(|s| s.bg(rgb(LINE)))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, event: &gpui::MouseDownEvent, window, cx| {
                    this.commit_inspector_input(cx);
                    this.end_pointer(window, cx);
                    this.end_timeline_pointer(window, cx);
                    this.blade_cursor.clear();
                    this.panels.drag = Some(Drag {
                        split,
                        start: event.position,
                        layout,
                    });
                    cx.stop_propagation();
                    cx.notify();
                }),
            )
    }

    /// 从按下位置计算尺寸，达到边界即停止，不推动另一侧面板；返回是否正在处理布局拖动。
    /// 曲线分隔只交换曲线与轨道高度；上下主分隔交换整个下方工作区与上方三栏的高度。
    pub(crate) fn move_panel_splitter(
        &mut self,
        position: Point<Pixels>,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(drag) = self.panels.drag else {
            return false;
        };
        let layout = drag.layout;
        let dx = f32::from(position.x - drag.start.x);
        let dy = f32::from(position.y - drag.start.y);
        match drag.split {
            Split::Library => {
                self.panels.left = Some((layout.left + dx).clamp(
                    MIN_LEFT,
                    layout.width - GAP * 2.0 - layout.right - MIN_PREVIEW,
                ))
            }
            Split::Inspector => {
                self.panels.right = Some((layout.right - dx).clamp(
                    MIN_RIGHT,
                    layout.width - GAP * 2.0 - layout.left - MIN_PREVIEW,
                ))
            }
            Split::Timeline => {
                self.panels.upper = Some((layout.upper + dy).clamp(
                    MIN_UPPER,
                    layout.available
                        - MIN_TIMELINE
                        - layout.curve
                        - if layout.curve > 0.0 { GAP } else { 0.0 },
                ))
            }
            Split::Curve => {
                self.panels.curve = Some((layout.curve + dy).clamp(
                    MIN_CURVE,
                    layout.available - GAP - layout.upper - MIN_TIMELINE,
                ))
            }
        }
        cx.notify();
        true
    }

    /// 释放或丢失左键时结束会话拖动；不保存项目、不增加编辑历史。
    pub(crate) fn end_panel_splitter(&mut self, cx: &mut Context<Self>) {
        if self.panels.drag.take().is_some() {
            cx.notify();
        }
    }

    /// 拖动覆盖层直接接收移动并即时重排，避免 occlude 阻断底层根节点后只在松手时更新。
    /// 保持方向光标并隔离预览/曲线编辑；丢失左键时结束拖动，窗口外释放仍由根节点收尾。
    pub(crate) fn panel_drag_overlay(&self, cx: &mut Context<Self>) -> Option<Div> {
        self.panels.drag.map(|drag| {
            div()
                .absolute()
                .inset_0()
                .occlude()
                .when(
                    matches!(drag.split, Split::Library | Split::Inspector),
                    |s| s.cursor_ew_resize(),
                )
                .when(matches!(drag.split, Split::Timeline | Split::Curve), |s| {
                    s.cursor_ns_resize()
                })
                .on_mouse_move(cx.listener(|this, event: &gpui::MouseMoveEvent, _, cx| {
                    if event.pressed_button == Some(MouseButton::Left) {
                        this.move_panel_splitter(event.position, cx);
                    } else {
                        this.end_panel_splitter(cx);
                    }
                    cx.stop_propagation();
                }))
        })
    }
}

#[cfg(test)]
#[path = "editor_layout_tests.rs"]
mod tests;
