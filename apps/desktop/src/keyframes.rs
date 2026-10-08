//! 检查器关键帧按钮与导航：只在单个选中素材覆盖播放头时显示，复用现有事务。
use crate::{
    ui::{MUTED, TEXT},
    view::Desktop,
};
use dinuosi_core::{keyframes::KeyframeKind, timeline::Clip};
use dinuosi_icons::Icon;
use gpui::{Context, Div, div, prelude::*, px, rgb, svg};

impl Desktop {
    /// 将播放头按项目帧率取整，所有按钮、参数输入和画布手势使用同一项目帧。
    pub(crate) fn keyframe_frame(&self) -> i64 {
        self.timeline.model.frame(self.session.timeline_position)
    }
    /// 读取已应用的全局自动添加偏好；设置草稿不会提前影响编辑。
    pub(crate) fn auto_keyframe(&self, cx: &gpui::App) -> bool {
        cx.global::<crate::preferences::UserPreferences>()
            .0
            .auto_keyframe
    }
    /// 返回唯一选中且与类型匹配的素材；多选时隐藏关键帧，避免导航目标不明确。
    fn keyframe_clip(&self, kind: KeyframeKind) -> Option<&Clip> {
        if self.timeline.selected.len() != 1 {
            return None;
        }
        self.timeline
            .model
            .clips
            .iter()
            .find(|c| self.timeline.selected.contains(&c.id) && kind.accepts(c.kind))
    }
    /// 绘制空心/实心菱形；两帧以上显示同类型前后导航，无邻帧时禁用对应方向。
    /// 播放头在片段外或无唯一选择时保留布局占位；锁定、忙碌时禁用输入，支持 Tab/Enter/空格。
    pub(crate) fn keyframe_controls(&self, kind: KeyframeKind, cx: &mut Context<Self>) -> Div {
        let frame = self.keyframe_frame();
        let clip = self.keyframe_clip(kind).filter(|c| c.contains_frame(frame));
        let keys = clip
            .map(|c| c.keyframe_frames(Some(kind)))
            .unwrap_or_default();
        let enabled =
            clip.is_some_and(|c| !self.busy && !self.timeline.model.tracks[c.track].locked);
        let filled = keys.contains(&frame);
        let nav = keys.len() >= 2;
        div()
            .w(px(46.0))
            .flex_none()
            .flex()
            .items_center()
            .justify_center()
            .when(clip.is_some(), |row| {
                row.children([-1_i8, 0, 1].into_iter().filter(|d| *d == 0 || nav).map(
                    |direction| {
                        let target = match direction {
                            -1 => keys.range(..frame).next_back().copied(),
                            1 => keys.range((frame + 1)..).next().copied(),
                            _ => Some(frame),
                        };
                        let usable = enabled && target.is_some();
                        let icon = match direction {
                            -1 => Icon::ChevronLeft,
                            1 => Icon::ChevronRight,
                            _ if filled => Icon::KeyframeFilled,
                            _ => Icon::Keyframe,
                        };
                        div()
                            .id(gpui::SharedString::from(format!(
                                "keyframe-{kind:?}-{direction}"
                            )))
                            .debug_selector(move || {
                                format!(
                                    "keyframe-{kind:?}-{direction}-{}",
                                    if direction == 0 && filled {
                                        "filled"
                                    } else {
                                        "empty"
                                    }
                                )
                            })
                            .when(usable, |s| s.focusable().tab_index(0).cursor_pointer())
                            .w(px(if direction == 0 { 18.0 } else { 14.0 }))
                            .h(px(24.0))
                            .flex()
                            .items_center()
                            .justify_center()
                            .opacity(if usable { 1.0 } else { 0.35 })
                            .hover(|s| s.bg(rgb(MUTED)))
                            .focus(|s| s.bg(rgb(MUTED)))
                            .child(
                                svg()
                                    .path(icon.path())
                                    .size(px(if direction == 0 { 18.0 } else { 12.0 }))
                                    .text_color(rgb(TEXT)),
                            )
                            .on_mouse_down(
                                gpui::MouseButton::Left,
                                cx.listener(|_, _, _, cx| cx.stop_propagation()),
                            )
                            .on_click(cx.listener(move |this, _, window, cx| {
                                if usable {
                                    this.activate_keyframe(kind, direction, window, cx);
                                }
                            }))
                            .on_key_down(cx.listener(
                                move |this, event: &gpui::KeyDownEvent, window, cx| {
                                    if usable
                                        && !event.keystroke.modifiers.modified()
                                        && matches!(event.keystroke.key.as_str(), "enter" | "space")
                                    {
                                        cx.stop_propagation();
                                        if !event.is_held {
                                            this.activate_keyframe(kind, direction, window, cx);
                                        }
                                    }
                                },
                            ))
                    },
                ))
            })
    }
    /// 先提交输入，再重新求目标帧；菱形切换当前类型，箭头直接寻址到严格前/后关键帧。
    /// 导航不改项目，增删与现有编辑共用保存/回滚/撤销；成功时停止播放保持精确落帧。
    fn activate_keyframe(
        &mut self,
        kind: KeyframeKind,
        direction: i8,
        window: &mut gpui::Window,
        cx: &mut Context<Self>,
    ) {
        self.commit_inspector_input(cx);
        let frame = self.keyframe_frame();
        let Some(clip) = self.keyframe_clip(kind).filter(|c| {
            c.contains_frame(frame) && !self.busy && !self.timeline.model.tracks[c.track].locked
        }) else {
            return;
        };
        let keys = clip.keyframe_frames(Some(kind));
        if direction == 0 {
            self.stop_timeline();
            self.timeline.preview = true;
            self.stop_preview();
            let before = self.timeline.model.clone();
            self.timeline
                .model
                .toggle_keyframe(&self.timeline.selected, kind, frame);
            self.commit_timeline(before, cx);
        } else {
            let target = if direction < 0 {
                keys.range(..frame).next_back().copied()
            } else {
                keys.range((frame + 1)..).next().copied()
            };
            if let Some(target) = target {
                self.seek_timeline(target as f64 / self.timeline.model.fps as f64, true, cx);
            }
        }
        self.focus_handle.focus(window);
    }
}
