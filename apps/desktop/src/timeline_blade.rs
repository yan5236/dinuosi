//! 刀片工具的命中、虚线、光标和编辑事务；核心切割由 Timeline::split 统一执行。
use crate::{ui::TEXT, view::Desktop};
use dinuosi_core::timeline::Clip;
use dinuosi_icons::Icon;
use gpui::{
    Context, Div, MouseDownEvent, Pixels, Point, Window, canvas, div, prelude::*, px, rgb, svg,
};
use std::collections::BTreeSet;

impl Desktop {
    /// 切换选择/刀片工具并把键盘焦点交回工作区；活动手势期间忽略切换，避免丢失未提交编辑。
    /// 工具属于窗口会话，不写入项目或撤销历史；立即按当前指针刷新刀片显示。
    pub(crate) fn set_blade_tool(
        &mut self,
        blade: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.busy || self.timeline.gesture.is_some() || self.session.drag.is_some() {
            return;
        }
        self.commit_inspector_input(cx);
        self.timeline.blade = blade;
        self.focus_handle.focus(window);
        self.update_blade_pointer(window.mouse_position(), cx);
        cx.notify();
    }

    /// 记录用于悬停的窗口坐标；切换到选择工具、离开窗口时仍由命中条件自动隐藏预览。
    /// 不更改播放头或项目，只在刀片启用时请求重绘。
    pub(crate) fn update_blade_pointer(&mut self, point: Point<Pixels>, cx: &mut Context<Self>) {
        self.timeline.blade_pointer = Some(point);
        if self.timeline.blade {
            cx.notify();
        }
    }

    /// 返回刀片活动内容区内的指针；标尺、轨道头、滚动条、弹窗/菜单和其他手势均不显示刀片。
    fn blade_point(&self) -> Option<Point<Pixels>> {
        let point = self.timeline.blade_pointer?;
        let b = self.timeline.bounds;
        (self.timeline.blade
            && self.current.is_some()
            && !self.busy
            && self.dialog.is_none()
            && !self.menu_open
            && !self.help_open
            && self.project_menu.is_none()
            && self.session.media_menu.is_none()
            && self.timeline.keyframe_menu.is_none()
            && self.timeline.gesture.is_none()
            && self.session.drag.is_none()
            && point.x >= b.left() + px(self.timeline.header)
            && point.x < b.right() - px(12.0)
            && point.y >= b.top() + px(38.0)
            && point.y < b.bottom() - px(12.0))
        .then_some(point)
    }

    /// 按实际片段矩形命中并吸附项目帧；锁轨、首尾边界及不足两帧的片段不提供有效切割。
    /// 未选中的片段仍可命中，供需要选择模式的第一次点击使用；不修改选择或模型。
    fn blade_hit(&self) -> Option<(&Clip, i64)> {
        let point = self.blade_point()?;
        let frame = self.timeline_frame_at(point.x);
        self.timeline.model.clips.iter().find_map(|clip| {
            let (top, height) = self.track_geometry(clip.track);
            let y = f32::from(point.y - self.timeline.bounds.top());
            (!self.timeline.model.tracks[clip.track].locked
                && y >= top + 4.0
                && y < top + height - 4.0
                && frame > clip.start
                && frame < clip.start + clip.duration)
                .then_some((clip, frame))
        })
    }

    /// 左键只处理当前命中的片段：直接模式立即切割，需要选择模式先选中，再次点击才切割。
    /// 两种模式均阻止向空白框选冒泡；锁轨和端点点击不产生编辑或移动播放头。
    pub(crate) fn click_blade_clip(
        &mut self,
        id: String,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        cx.stop_propagation();
        self.commit_inspector_input(cx);
        self.update_blade_pointer(event.position, cx);
        self.focus_handle.focus(window);
        let Some((clip, frame)) = self.blade_hit() else {
            return;
        };
        if clip.id != id {
            return;
        }
        let requires_selection = cx
            .global::<crate::preferences::UserPreferences>()
            .0
            .blade_requires_selection;
        self.timeline.preview = true;
        if requires_selection && !self.timeline.selected.contains(&id) {
            self.timeline.selected.clear();
            self.timeline.selected.insert(id);
            self.refresh_timeline(cx);
        } else {
            self.split_timeline_clips(BTreeSet::from([id]), frame, cx);
        }
    }

    /// 把鼠标或播放头切割提交为一次持久化及撤销事务；空选择、锁轨和边界位置不保存无效历史。
    /// 成功后原先选中的片段两侧均保持选中，未选中的直接切割保持未选中；保存失败不改变选择。
    pub(crate) fn split_timeline_clips(
        &mut self,
        ids: BTreeSet<String>,
        frame: i64,
        cx: &mut Context<Self>,
    ) {
        if self.busy || self.timeline.gesture.is_some() || self.session.drag.is_some() {
            return;
        }
        let before = self.timeline.model.clone();
        let selected_ids: BTreeSet<_> =
            ids.intersection(&self.timeline.selected).cloned().collect();
        match self.timeline.model.split(&ids, frame) {
            Ok(rights) if !rights.is_empty() => {
                let selected_rights: Vec<_> = self
                    .timeline
                    .model
                    .clips
                    .iter()
                    .filter(|right| {
                        rights.contains(&right.id)
                            && before.clips.iter().any(|left| {
                                selected_ids.contains(&left.id)
                                    && left.track == right.track
                                    && left.start < frame
                                    && frame < left.start + left.duration
                            })
                    })
                    .map(|c| c.id.clone())
                    .collect();
                self.stop_timeline();
                self.stop_preview();
                self.timeline.preview = true;
                self.commit_timeline(before.clone(), cx);
                if self.timeline.model != before {
                    self.timeline.selected.extend(selected_rights);
                }
                cx.notify();
            }
            Err(error) => {
                self.notice = Some(error);
                cx.notify();
            }
            _ => {}
        }
    }

    /// 在允许切割的悬停片段中绘制帧对齐虚线；选择要求直接读取已应用偏好，支持设置即时生效。
    /// 线只覆盖该素材块高度，交给内容区裁切，不截获鼠标事件。
    pub(crate) fn blade_cut_line(&self, cx: &gpui::App) -> Option<Div> {
        let (clip, frame) = self.blade_hit()?;
        if cx
            .global::<crate::preferences::UserPreferences>()
            .0
            .blade_requires_selection
            && !self.timeline.selected.contains(&clip.id)
        {
            return None;
        }
        let (top, h) = self.track_geometry(clip.track);
        let height = h - 8.0;
        let x = frame as f32 / self.timeline.model.fps as f32 * self.pixels_per_second()
            - self.timeline.scroll_x;
        Some(
            div()
                .debug_selector(|| "blade-cut-line".into())
                .absolute()
                .left(px(x + crate::trim_handle::WIDTH))
                .top(px(top - 34.0))
                .w(px(1.0))
                .h(px(height))
                .children((0..(height / 7.0).ceil() as usize).map(|n| {
                    div()
                        .absolute()
                        .top(px(n as f32 * 7.0))
                        .w(px(1.0))
                        .h(px((height - n as f32 * 7.0).min(4.0)))
                        .bg(rgb(TEXT))
                })),
        )
    }

    /// 使用现有 Razor SVG 替换内容区系统光标，窗口外或离开内容区自动恢复平台指针。
    /// 根层绘制避免片段边缘裁切；无命中时仍显示刀片，但不显示切割虚线。
    pub(crate) fn blade_cursor_overlay(&self) -> Option<Div> {
        let point = self.blade_point()?;
        Some(
            div()
                .debug_selector(|| "blade-cursor".into())
                .absolute()
                .left(point.x - px(12.0))
                .top(point.y - px(12.0))
                .size(px(24.0))
                .child(
                    svg()
                        .path(Icon::Razor.path())
                        .size_full()
                        .text_color(rgb(TEXT)),
                )
                .child(
                    canvas(
                        |_, _, _| (),
                        |_, _, window, _| {
                            window.set_window_cursor_style(gpui::CursorStyle::None);
                        },
                    )
                    .absolute()
                    .inset_0(),
                ),
        )
    }
}
