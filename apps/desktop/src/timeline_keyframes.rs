//! 时间轴合并关键帧标记：精确帧定位、直接寻址和沿用现有菜单样式的整组删除。
use crate::{
    ui::{LINE, TEXT},
    view::Desktop,
};
use dinuosi_core::timeline::Clip;
use dinuosi_icons::Icon;
use gpui::{
    Context, Div, FocusHandle, MouseButton, Pixels, Point, Stateful, Window, div, prelude::*, px,
    rgb, svg,
};

pub(crate) struct KeyframeMenu {
    pub clip_id: String,
    pub frame: i64,
    pub position: Point<Pixels>,
    pub focus: FocusHandle,
}
impl Desktop {
    /// 在裁剪把手层的上方绘制选中素材的合并菱形；中心严格对齐项目帧，首尾不向内夹紧。
    /// x 是片段横向位置；viewport 是时间区宽度，屏外标记不生成命中区。
    /// 左键停止片段拖动并寻址，右键仅打开该素材/帧的删除菜单；同帧不同类型仍显示一枚。
    pub(crate) fn timeline_keyframe_marks(
        &self,
        clip: &Clip,
        x: f32,
        viewport: f32,
        cx: &mut Context<Self>,
    ) -> Vec<Stateful<Div>> {
        clip.keyframe_frames(None)
            .into_iter()
            .filter_map(|frame| {
                let mark = frame as f32 / self.timeline.model.fps as f32 * self.pixels_per_second()
                    - self.timeline.scroll_x;
                if mark < 0.0 || mark > viewport {
                    return None;
                }
                let id = clip.id.clone();
                let right_id = id.clone();
                let selector = format!("timeline-keyframe-{id}-{frame}");
                Some(
                    div()
                        .id(gpui::SharedString::from(selector.clone()))
                        .debug_selector(move || selector.clone())
                        .absolute()
                        .left(px(mark - x.max(0.0) - 10.0))
                        .bottom(px(1.0))
                        .size(px(18.0))
                        .flex()
                        .items_center()
                        .justify_center()
                        .cursor_pointer()
                        .child(
                            svg()
                                .path(Icon::KeyframeFilled.path())
                                .size(px(12.0))
                                .text_color(rgb(TEXT)),
                        )
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |this, event: &gpui::MouseDownEvent, window, cx| {
                                this.click_timeline_keyframe(&id, frame, None, window, cx);
                                cx.stop_propagation();
                                this.timeline.pointer = event.position;
                            }),
                        )
                        .on_mouse_down(
                            MouseButton::Right,
                            cx.listener(move |this, event: &gpui::MouseDownEvent, window, cx| {
                                this.click_timeline_keyframe(
                                    &right_id,
                                    frame,
                                    Some(event.position),
                                    window,
                                    cx,
                                );
                                cx.stop_propagation();
                            }),
                        ),
                )
            })
            .collect()
    }
    /// 用最新模型确认点击目标仍存在且未锁定，先提交输入，选中目标素材并取消旧手势，恢复尚未提交的按下快照。
    /// position 为 None 时左键按帧寻址；Some 时右键记录独立删除目标，不移动播放头；命中片段为检查器主对象。
    fn click_timeline_keyframe(
        &mut self,
        id: &str,
        frame: i64,
        position: Option<Point<Pixels>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.commit_inspector_input(cx);
        if let Some(before) = self.timeline.before.take() {
            self.timeline.model = before;
            cx.notify();
        }
        self.timeline.gesture = None;
        self.timeline.scroll_task = None;
        if self.busy
            || !self.timeline.model.clips.iter().any(|c| {
                c.id == id
                    && !c.locked
                    && !self.timeline.model.tracks[c.track].locked
                    && c.keyframe_frames(None).contains(&frame)
            })
        {
            return;
        }
        self.stop_timeline();
        self.timeline.selected = [id.to_owned()].into();
        self.timeline.primary_clip = Some(id.to_owned());
        self.timeline.keyframe_menu = None;
        if let Some(position) = position {
            self.menu_open = false;
            self.help_open = false;
            self.project_menu = None;
            self.session.media_menu = None;
            let focus = cx.focus_handle();
            focus.focus(window);
            self.timeline.keyframe_menu = Some(KeyframeMenu {
                clip_id: id.to_owned(),
                frame,
                position,
                focus,
            });
            cx.notify();
        } else {
            self.focus_handle.focus(window);
            self.seek_timeline(frame as f64 / self.timeline.model.fps as f64, true, cx);
        }
    }
    /// 在窗口范围内绘制唯一的“删除关键帧”菜单项，复用素材菜单的灰阶、边框与图标。
    /// 鼠标/Enter/空格删除，Escape 关闭；菜单聚焦期间不让剪辑快捷键误删整个素材。
    pub(crate) fn timeline_keyframe_menu(&self, window: &Window, cx: &mut Context<Self>) -> Div {
        let menu = self
            .timeline
            .keyframe_menu
            .as_ref()
            .expect("菜单存在时绘制");
        let viewport = window.viewport_size();
        div()
            .debug_selector(|| "timeline-keyframe-menu".into())
            .absolute()
            .occlude()
            .left(px(
                f32::from(menu.position.x).clamp(0.0, (f32::from(viewport.width) - 164.0).max(0.0))
            ))
            .top(px(
                f32::from(menu.position.y).clamp(0.0, (f32::from(viewport.height) - 60.0).max(0.0))
            ))
            .w(px(164.0))
            .p(px(6.0))
            .bg(rgb(0x303034))
            .border_1()
            .border_color(rgb(LINE))
            .rounded(px(6.0))
            .child(
                div()
                    .id("timeline-keyframe-delete")
                    .debug_selector(|| "timeline-keyframe-delete".into())
                    .track_focus(&menu.focus)
                    .flex()
                    .items_center()
                    .gap(px(10.0))
                    .p(px(10.0))
                    .rounded(px(4.0))
                    .cursor_pointer()
                    .hover(|s| s.bg(rgb(LINE)))
                    .focus(|s| s.bg(rgb(LINE)))
                    .child(
                        svg()
                            .path(Icon::Trash.path())
                            .size(px(16.0))
                            .text_color(rgb(TEXT)),
                    )
                    .child(crate::i18n::t("timeline", "Delete keyframes", cx))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|_, _, _, cx| cx.stop_propagation()),
                    )
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.delete_timeline_keyframe(window, cx);
                        cx.stop_propagation();
                    }))
                    .on_key_down(cx.listener(|this, event: &gpui::KeyDownEvent, window, cx| {
                        cx.stop_propagation();
                        if !event.keystroke.modifiers.modified() && !event.is_held {
                            match event.keystroke.key.as_str() {
                                "enter" | "space" => this.delete_timeline_keyframe(window, cx),
                                "escape" => {
                                    this.timeline.keyframe_menu = None;
                                    this.focus_handle.focus(window);
                                    cx.notify();
                                }
                                _ => {}
                            }
                        }
                    })),
            )
    }
    /// 删除右键目标素材在该帧的全部类型；目标从菜单读取，和当前播放头/其他选择无关。
    /// 复用 Core 删除及保存/回滚/一次撤销；菜单过期、锁定或忙碌时不修改数据。
    fn delete_timeline_keyframe(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(menu) = self.timeline.keyframe_menu.take() else {
            return;
        };
        self.focus_handle.focus(window);
        if !self.busy {
            let before = self.timeline.model.clone();
            self.timeline
                .model
                .remove_keyframes(&menu.clip_id, menu.frame);
            self.commit_timeline(before, cx);
        }
        cx.notify();
    }
}
