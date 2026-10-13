//! 刀片工具的命中、虚线、光标和编辑事务；核心切割由 Timeline::split 统一执行。
use crate::{ui::TEXT, view::Desktop};
use dinuosi_core::timeline::Clip;
use gpui::{Context, Div, MouseDownEvent, Pixels, Point, Window, div, prelude::*, px, rgb};
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
    /// 不更改播放头或项目；仅命中片段或切割帧改变时刷新虚线，同一帧内移动不重建主界面。
    pub(crate) fn update_blade_pointer(&mut self, point: Point<Pixels>, cx: &mut Context<Self>) {
        let before = self
            .blade_hit()
            .map(|(clip, frame)| (clip.id.clone(), frame));
        self.timeline.blade_pointer = Some(point);
        let after = self
            .blade_hit()
            .map(|(clip, frame)| (clip.id.clone(), frame));
        if before != after {
            cx.notify();
        }
    }

    /// 判定工具是否可作用于内容区；弹窗、菜单、全屏和其他手势期间恢复平台指针。
    pub(crate) fn blade_enabled(&self) -> bool {
        self.timeline.blade
            && self.current.is_some()
            && !self.preview_fullscreen
            && !self.busy
            && self.dialog.is_none()
            && !self.menu_open
            && !self.help_open
            && self.project_menu.is_none()
            && self.session.media_menu.is_none()
            && self.timeline.keyframe_menu.is_none()
            && self.timeline.marker_menu.is_none()
            && self.timeline.marker_dialog.is_none()
            && self.timeline.gesture.is_none()
            && self.session.drag.is_none()
    }

    /// 返回刀片活动内容区内的指针；标尺、轨道头和滚动条不提供切割预览。
    fn blade_point(&self) -> Option<Point<Pixels>> {
        let point = self.timeline.blade_pointer?;
        let b = self.timeline.bounds;
        (self.blade_enabled()
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
            (!clip.locked
                && !self.timeline.model.tracks[clip.track].locked
                && y >= top + 4.0
                && y < top + height - 4.0
                && frame > clip.start
                && frame < clip.start + clip.duration)
                .then_some((clip, frame))
        })
    }

    /// 左键处理命中片段及关联成员；Alt 仅切命中单段，需要选择模式先选组、再次点击切割。
    /// 两种模式均阻止向空白框选冒泡；锁轨和端点点击不产生编辑或移动播放头，选中目标作为检查器主对象。
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
        let ids = if event.modifiers.alt {
            BTreeSet::from([id.clone()])
        } else {
            self.timeline.model.linked_ids(&[id.clone()].into())
        };
        if requires_selection && !self.timeline.selected.contains(&id) {
            self.timeline.selected = ids;
            self.timeline.primary_clip = Some(id);
            self.refresh_timeline(cx);
        } else {
            self.timeline.primary_clip = self.timeline.selected.contains(&id).then_some(id);
            self.split_timeline_clips(ids, frame, cx);
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
}
