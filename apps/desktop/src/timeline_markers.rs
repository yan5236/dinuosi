//! 项目标记的刻度绘制、排序列表和右键编辑；保存/回滚/撤销复用时间线事务。
use crate::{
    ui::{LINE, MUTED},
    view::Desktop,
};
use dinuosi_icons::Icon;
use gpui::{
    Context, Div, FocusHandle, MouseButton, Pixels, Point, Window, div, prelude::*, px, rgb, svg,
};
// 原 SVG 24×24 的竖杆在 x=5.5；缩放到 14px 后减去留白，使竖杆和时间线竖线精确重合。
pub(crate) const MARKER_POLE_X: f32 = 5.5 / 24.0 * 14.0;
pub struct MarkerMenu {
    pub id: String,
    pub position: Point<Pixels>,
    pub focus: FocusHandle,
}
/// 将项目整数帧格式化为 HH:MM:SS:FF，标记列表和项目时长使用同一帧精度。
pub(crate) fn timecode(frame: i64, fps: u32) -> String {
    let fps = fps.max(1) as i64;
    format!(
        "{:02}:{:02}:{:02}:{:02}",
        frame / fps / 3600,
        frame / fps / 60 % 60,
        frame / fps % 60,
        frame % fps
    )
}
impl Desktop {
    /// 判断当前选择是否含未锁定的图片或视频；忙碌、音频及锁轨不能启用镜像工具。
    pub(crate) fn can_mirror_clips(&self) -> bool {
        !self.busy
            && self.timeline.model.clips.iter().any(|c| {
                self.timeline.selected.contains(&c.id)
                    && c.kind != dinuosi_core::media::MediaKind::Audio
                    && !c.locked
                    && !self.timeline.model.tracks[c.track].locked
            })
    }

    /// 左右或上下翻转选中画面片段，先结束输入与拖动，再沿用保存、回滚和单次撤销事务。
    /// horizontal 为 true 表示左右镜像；成功后刷新预览并恢复时间线键盘焦点，不修改源素材。
    pub(crate) fn mirror_selected_clips(
        &mut self,
        horizontal: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.can_mirror_clips() {
            return;
        }
        self.commit_inspector_input(cx);
        self.end_timeline_pointer(window, cx);
        self.stop_timeline();
        let before = self.timeline.model.clone();
        self.timeline
            .model
            .mirror_clips(&self.timeline.selected, horizontal);
        self.stop_preview();
        self.timeline.preview = true;
        self.commit_timeline(before, cx);
        self.focus_handle.focus(window);
        cx.notify();
    }

    /// 在当前播放头添加唯一标记并切换标记页签；名称按当前语言生成，失败保留项目并显示真实错误。
    pub(crate) fn add_timeline_marker(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        self.commit_inspector_input(cx);
        let before = self.timeline.model.clone();
        let name = format!(
            "{} {}",
            crate::i18n::t("inspector", "Marker", cx),
            self.timeline.model.markers.len() + 1
        );
        let frame = self.keyframe_frame();
        match self.timeline.model.add_marker(frame, name) {
            Ok(_) => {
                self.timeline.markers_tab = true;
                self.commit_timeline(before, cx);
            }
            Err(e) => self.notice = Some(e),
        }
        self.focus_handle.focus(window);
        cx.notify();
    }
    /// 切换项目磁吸，默认开启；先完成已有输入与手势，保存失败时由时间线事务恢复原开关。
    pub(crate) fn toggle_snapping(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        self.commit_inspector_input(cx);
        self.end_timeline_pointer(window, cx);
        let before = self.timeline.model.clone();
        self.timeline.model.snapping = !self.timeline.model.snapping;
        self.commit_timeline(before, cx);
        self.focus_handle.focus(window);
        cx.notify();
    }
    /// 选中集合全部锁定时解锁，否则统一锁定；保留选择，结束旧手势与输入，每次只记录一条历史。
    pub(crate) fn toggle_clip_lock(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy || self.timeline.selected.is_empty() {
            return;
        }
        self.commit_inspector_input(cx);
        self.end_timeline_pointer(window, cx);
        self.stop_timeline();
        let locked = !self.selected_clips_locked();
        let before = self.timeline.model.clone();
        self.timeline
            .model
            .lock_clips(&self.timeline.selected, locked);
        self.commit_timeline(before, cx);
        self.focus_handle.focus(window);
        cx.notify();
    }
    /// 返回非空选择是否全部处于片段锁定；用于工具图标和解锁提示，不把锁轨状态当作片段锁。
    pub(crate) fn selected_clips_locked(&self) -> bool {
        let clips: Vec<_> = self
            .timeline
            .model
            .clips
            .iter()
            .filter(|c| self.timeline.selected.contains(&c.id))
            .collect();
        !clips.is_empty() && clips.iter().all(|c| c.locked)
    }
    /// 点击已保存标记跳到其项目帧并确保标记进入时间线视口；不改变片段选择，不创建编辑历史。
    pub(crate) fn seek_marker(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.commit_inspector_input(cx);
        let Some(frame) = self
            .timeline
            .model
            .markers
            .iter()
            .find(|m| m.id == id)
            .map(|m| m.frame)
        else {
            return;
        };
        self.stop_preview();
        self.focus_handle.focus(window);
        let x = frame as f32 / self.timeline.model.fps as f32 * self.pixels_per_second();
        let width = self.timeline_view_width();
        if x < self.timeline.scroll_x || x > self.timeline.scroll_x + width - 16.0 {
            self.timeline.scroll_x = (x - width / 2.0).max(0.0);
        }
        self.seek_timeline(frame as f64 / self.timeline.model.fps as f64, true, cx);
    }
    /// 标尺内按 SVG 竖杆中心对齐项目帧，不用图标包围盒中心；横滚裁切，右键打开目标菜单而不移动播放头。
    pub(crate) fn ruler_markers(
        &self,
        width: f32,
        cx: &mut Context<Self>,
    ) -> Vec<gpui::Stateful<Div>> {
        self.timeline
            .model
            .markers
            .iter()
            .filter_map(|m| {
                let x = m.frame as f32 / self.timeline.model.fps as f32 * self.pixels_per_second()
                    - self.timeline.scroll_x;
                if x < 0.0 || x > width {
                    return None;
                }
                let id = m.id.clone();
                let right_id = id.clone();
                Some(
                    div()
                        .id(gpui::SharedString::from(format!("ruler-marker-{id}")))
                        .debug_selector({
                            let id = id.clone();
                            move || format!("ruler-marker-{id}")
                        })
                        .absolute()
                        .left(px(x - MARKER_POLE_X))
                        .top(px(22.0))
                        .size(px(14.0))
                        .cursor_pointer()
                        .child(
                            svg()
                                .path(Icon::Marker.path())
                                .size_full()
                                .text_color(rgb(m.color)),
                        )
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |this, _, window, cx| {
                                this.seek_marker(&id, window, cx);
                                cx.stop_propagation();
                            }),
                        )
                        .on_mouse_down(
                            MouseButton::Right,
                            cx.listener(move |this, e: &gpui::MouseDownEvent, window, cx| {
                                this.open_marker_menu(&right_id, e.position, window, cx);
                                cx.stop_propagation();
                            }),
                        ),
                )
            })
            .collect()
    }
    /// 按时间、身份稳定排序所有标记；列表只展示颜色、名称和帧时间，点击跳转，右键管理。
    pub(crate) fn markers_inspector(&self, cx: &mut Context<Self>) -> Div {
        let mut markers: Vec<_> = self.timeline.model.markers.iter().collect();
        markers.sort_by_key(|m| (m.frame, &m.id));
        div().flex_1().min_h_0().child(
            div()
                .id("markers-list")
                .size_full()
                .overflow_y_scroll()
                .pt(px(12.0))
                .children(markers.into_iter().map(|m| {
                    let id = m.id.clone();
                    let right_id = id.clone();
                    div()
                        .id(gpui::SharedString::from(format!("marker-row-{id}")))
                        .debug_selector({
                            let id = id.clone();
                            move || format!("marker-row-{id}")
                        })
                        .h(px(38.0))
                        .flex()
                        .items_center()
                        .gap(px(10.0))
                        .px(px(20.0))
                        .text_size(px(12.0))
                        .cursor_pointer()
                        .hover(|s| s.bg(rgb(LINE)))
                        .child(
                            div()
                                .size(px(8.0))
                                .flex_none()
                                .bg(rgb(m.color))
                                .rounded(px(2.0)),
                        )
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .overflow_hidden()
                                .whitespace_nowrap()
                                .child(m.name.clone()),
                        )
                        .child(
                            div()
                                .flex_none()
                                .text_size(px(11.0))
                                .text_color(rgb(MUTED))
                                .child(timecode(m.frame, self.timeline.model.fps)),
                        )
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.seek_marker(&id, window, cx);
                        }))
                        .on_mouse_down(
                            MouseButton::Right,
                            cx.listener(move |this, e: &gpui::MouseDownEvent, window, cx| {
                                this.open_marker_menu(&right_id, e.position, window, cx);
                                cx.stop_propagation();
                            }),
                        )
                })),
        )
    }
    /// 为真实存在的标记打开唯一上下文菜单，清除其他菜单并独立聚焦，避免 Delete 误删片段。
    fn open_marker_menu(
        &mut self,
        id: &str,
        position: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.commit_inspector_input(cx);
        if self.busy || !self.timeline.model.markers.iter().any(|m| m.id == id) {
            return;
        }
        self.timeline.keyframe_menu = None;
        self.menu_open = false;
        self.help_open = false;
        self.session.media_menu = None;
        self.project_menu = None;
        let focus = cx.focus_handle();
        focus.focus(window);
        self.timeline.marker_menu = Some(MarkerMenu {
            id: id.into(),
            position,
            focus,
        });
        cx.notify();
    }
    /// 在窗口边界内绘制修改与删除菜单；修改打开名称/颜色联合弹窗，菜单本身不保存草稿。
    pub(crate) fn marker_menu(&self, window: &Window, cx: &mut Context<Self>) -> Div {
        let menu = self.timeline.marker_menu.as_ref().expect("标记菜单存在");
        let viewport = window.viewport_size();
        div()
            .absolute()
            .occlude()
            .left(px(
                f32::from(menu.position.x).clamp(0.0, (f32::from(viewport.width) - 184.0).max(0.0))
            ))
            .top(px(
                f32::from(menu.position.y).clamp(0.0, (f32::from(viewport.height) - 82.0).max(0.0))
            ))
            .w(px(184.0))
            .p(px(6.0))
            .bg(rgb(0x303034))
            .border_1()
            .border_color(rgb(LINE))
            .rounded(px(6.0))
            .child(
                div()
                    .id("marker-menu-focus")
                    .track_focus(&menu.focus)
                    .children(
                        [
                            ("marker-edit", "Edit marker", 0),
                            ("marker-delete", "Delete marker", 1),
                        ]
                        .into_iter()
                        .map(|(id, label, action)| {
                            div()
                                .id(id)
                                .debug_selector(move || id.into())
                                .h(px(34.0))
                                .px(px(10.0))
                                .flex()
                                .items_center()
                                .cursor_pointer()
                                .hover(|s| s.bg(rgb(LINE)))
                                .child(crate::i18n::t("inspector", label, cx))
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(|_, _, _, cx| cx.stop_propagation()),
                                )
                                .on_click(cx.listener(move |this, _, window, cx| {
                                    if action == 0 {
                                        this.begin_marker_edit(window, cx);
                                    } else {
                                        this.delete_menu_marker(window, cx);
                                    }
                                    cx.stop_propagation();
                                }))
                        }),
                    )
                    .on_key_down(cx.listener(|this, e: &gpui::KeyDownEvent, window, cx| {
                        if e.keystroke.key == "escape" {
                            this.timeline.marker_menu = None;
                            this.focus_handle.focus(window);
                            cx.notify();
                        }
                        cx.stop_propagation();
                    })),
            )
    }
    /// 按已应用用户偏好绘制标记竖线，线中心与旗帜竖杆同帧对齐；仅覆盖可视时间范围，不创建鼠标命中区。
    /// origin 为含把手留白的标尺原点，width 为可见时间宽度；开关草稿在设置应用前不会影响绘制。
    pub(crate) fn marker_lines(&self, origin: f32, width: f32, cx: &Context<Self>) -> Option<Div> {
        if !cx
            .global::<crate::preferences::UserPreferences>()
            .0
            .show_marker_lines
        {
            return None;
        }
        Some(
            div()
                .absolute()
                .left(px(origin))
                .right(px(12.0 + crate::trim_handle::WIDTH))
                .top(px(24.0))
                .bottom(px(12.0))
                .overflow_hidden()
                .children(self.timeline.model.markers.iter().filter_map(|marker| {
                    let x = marker.frame as f32 / self.timeline.model.fps as f32
                        * self.pixels_per_second()
                        - self.timeline.scroll_x;
                    if x < 0.0 || x > width {
                        return None;
                    }
                    let id = marker.id.clone();
                    Some(
                        div()
                            .debug_selector(move || format!("marker-line-{id}"))
                            .absolute()
                            .left(px(x - 0.5))
                            .top_0()
                            .bottom_0()
                            .w(px(1.0))
                            .bg(rgb(marker.color))
                            .opacity(0.55),
                    )
                })),
        )
    }
    /// 删除右键目标标记，菜单过期或忙碌时无操作；复用单次保存/撤销，恢复工作区焦点。
    fn delete_menu_marker(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        let Some(menu) = self.timeline.marker_menu.take() else {
            return;
        };
        let before = self.timeline.model.clone();
        self.timeline.model.delete_marker(&menu.id);
        self.commit_timeline(before, cx);
        self.focus_handle.focus(window);
        cx.notify();
    }
}
