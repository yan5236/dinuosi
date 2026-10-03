//! 指针手势到核心编辑事务的适配；拖动中只更新预览，释放时一次保存与记录历史。
use crate::{
    timeline_state::{DropTarget, Gesture},
    view::Desktop,
};
use dinuosi_core::timeline::{Property, Timeline, TrackKind};
use gpui::{Context, MouseDownEvent, Pixels, Point, Window};

impl Desktop {
    /// 开始片段移动/修剪，保持已选组；Ctrl/Shift 点击可追加选择，锁轨忽略输入。
    pub(crate) fn begin_clip(
        &mut self,
        id: String,
        edge: Option<bool>,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.commit_inspector_input(cx);
        let Some(clip) = self.timeline.model.clips.iter().find(|c| c.id == id) else {
            return;
        };
        if self.busy || self.timeline.model.tracks[clip.track].locked {
            return;
        }
        let track = clip.track;
        if event.modifiers.control || event.modifiers.shift {
            if !self.timeline.selected.insert(id.clone()) {
                self.timeline.selected.remove(&id);
            }
        } else if !self.timeline.selected.contains(&id) {
            self.timeline.selected.clear();
            self.timeline.selected.insert(id.clone());
        }
        self.focus_handle.focus(window);
        self.timeline.preview = true;
        self.stop_timeline();
        self.stop_preview();
        self.timeline.before = Some(self.timeline.model.clone());
        self.timeline.gesture_scroll = gpui::point(self.timeline.scroll_x, self.timeline.scroll_y);
        self.timeline.gesture = Some(match edge {
            Some(left) => Gesture::Trim {
                start: event.position,
                id,
                left,
            },
            None => Gesture::Move {
                start: event.position,
                track,
            },
        });
        self.timeline.moved = false;
        self.timeline.pointer = event.position;
        self.refresh_timeline(cx);
        cx.stop_propagation();
    }
    /// 在空白区按下时开始框选，不移动播放头；Shift/Ctrl 框选保留已有选择。
    pub(crate) fn begin_marquee(
        &mut self,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.commit_inspector_input(cx);
        if self.busy {
            return;
        }
        self.focus_handle.focus(window);
        let additive = if event.modifiers.shift || event.modifiers.control {
            self.timeline.selected.clone()
        } else {
            Default::default()
        };
        self.timeline.selected = additive.clone();
        self.timeline.gesture_scroll = gpui::point(self.timeline.scroll_x, self.timeline.scroll_y);
        self.timeline.gesture = Some(Gesture::Marquee {
            start: event.position,
            additive,
        });
        self.timeline.pointer = event.position;
        self.timeline.moved = false;
        cx.notify();
    }
    /// 记录素材库按下位置；超过四像素后显示缩略图光标和轨道落点，点击仍保留素材预览。
    pub(crate) fn begin_library_drag(
        &mut self,
        index: usize,
        point: Point<Pixels>,
        cx: &mut Context<Self>,
    ) {
        if self.busy || self.session.media.get(index).is_none_or(|m| m.offline) {
            return;
        }
        self.timeline.gesture = Some(Gesture::Library {
            start: point,
            index,
        });
        self.timeline.pointer = point;
        self.timeline.moved = false;
        cx.notify();
    }
    /// 处理活动手势的移动，业务修改每次从按下快照计算，避免累计漂移与重复历史。
    pub(crate) fn move_timeline_pointer(&mut self, point: Point<Pixels>, cx: &mut Context<Self>) {
        let Some(gesture) = &self.timeline.gesture else {
            return;
        };
        self.timeline.pointer = point;
        match gesture {
            Gesture::Marquee { start, additive } => {
                let start = self.timeline_gesture_start(*start);
                let (a, b) = (
                    self.timeline_frame_at(start.x),
                    self.timeline_frame_at(point.x),
                );
                let top = start.y.min(point.y);
                let bottom = start.y.max(point.y);
                let mut selected = additive.clone();
                for track in 0..self.timeline.model.tracks.len() {
                    let (y, h) = self.track_geometry(track);
                    let y = self.timeline.bounds.top() + gpui::px(y);
                    if bottom >= y && top <= y + gpui::px(h) {
                        selected.extend(self.timeline.model.select_range(
                            a.min(b),
                            a.max(b),
                            track,
                            track,
                        ));
                    }
                }
                self.timeline.selected = selected;
                self.timeline.moved = true;
            }
            Gesture::Move { start, .. } | Gesture::Track { start, .. } => {
                self.timeline.moved |=
                    f32::from(point.x - start.x).abs() + f32::from(point.y - start.y).abs() > 4.0;
            }
            Gesture::Trim { start, id, left } => {
                let start = self.timeline_gesture_start(*start);
                let delta = ((f32::from(point.x - start.x) / self.pixels_per_second())
                    * self.timeline.model.fps as f32)
                    .round() as i64;
                if let Some(before) = &self.timeline.before {
                    let mut next = before.clone();
                    let _ = next.trim(id, *left, delta);
                    self.timeline.model = next;
                }
                self.timeline.moved = true;
            }
            Gesture::Library { start, .. } => {
                self.timeline.moved |=
                    f32::from(point.x - start.x).abs() + f32::from(point.y - start.y).abs() > 4.0;
            }
            Gesture::Horizontal { start, offset } => {
                let visible = self.timeline_view_width();
                let total = self.timeline_width();
                let thumb = (visible / total * visible).max(18.0).min(visible);
                self.timeline.scroll_x = (*offset
                    + f32::from(point.x - start.x) * (total - visible)
                        / (visible - thumb).max(1.0))
                .clamp(0.0, total - visible);
            }
            Gesture::Vertical { start, offset } => {
                let visible =
                    (f32::from(self.timeline.bounds.size.height) - self.timeline.ruler - 12.0)
                        .max(1.0);
                let total = self.timeline.content_height.max(visible);
                let thumb = (visible / total * visible).max(18.0).min(visible);
                self.timeline.scroll_y = (*offset
                    + f32::from(point.y - start.y) * (total - visible)
                        / (visible - thumb).max(1.0))
                .clamp(0.0, (total - visible).max(0.0));
            }
            Gesture::Property { property, bounds } => {
                let ratio = (f32::from(point.x - bounds.left())
                    / f32::from(bounds.size.width).max(1.0))
                .clamp(0.0, 1.0);
                let (min, max) = property.range();
                let _ = self.timeline.model.set_property(
                    &self.timeline.selected,
                    *property,
                    (min + ratio as f64 * (max - min)).round(),
                );
                self.refresh_timeline(cx);
            }
        }
        cx.notify();
    }
    /// 释放手势并提交一次事务；素材仅在合法落点插入，窗口外释放取消素材拖入。
    pub(crate) fn end_timeline_pointer(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.timeline.scroll_task = None;
        let Some(gesture) = self.timeline.gesture.take() else {
            return;
        };
        match gesture {
            Gesture::Library { index, .. } if self.timeline.moved => {
                if f32::from(self.timeline.pointer.x - self.timeline.bounds.left())
                    >= self.timeline.header
                    && let Some(target) = self.timeline_drop_target(self.timeline.pointer)
                {
                    self.timeline.before = Some(self.timeline.model.clone());
                    let track = match target {
                        DropTarget::Track(i) => i,
                        DropTarget::Insert(i) => self
                            .timeline
                            .model
                            .create_track(i, TrackKind::Media)
                            .unwrap(),
                    };
                    self.insert_media_clip(
                        index,
                        track,
                        self.timeline_frame_at(self.timeline.pointer.x),
                        window,
                        cx,
                    );
                    if let Some(before) = self.timeline.before.take() {
                        self.timeline.model = before;
                    }
                }
            }
            Gesture::Move { start, track } => {
                if let Some(before) = self.timeline.before.take() {
                    if self.timeline.moved
                        && let Some(next) = self.clip_drop_model(&before, start, track)
                    {
                        self.timeline.model = next;
                    }
                    self.commit_timeline(before, cx);
                }
            }
            Gesture::Track { index, .. } => {
                if let Some(before) = self.timeline.before.take() {
                    if self.timeline.moved
                        && let Some(target) = self.timeline_drop_target(self.timeline.pointer)
                    {
                        let (DropTarget::Insert(above) | DropTarget::Track(above)) = target;
                        if let Err(error) = self.timeline.model.reorder_track(index, above) {
                            self.notice = Some(error);
                        }
                    }
                    self.commit_timeline(before, cx);
                }
            }
            _ => {
                if let Some(before) = self.timeline.before.take() {
                    self.commit_timeline(before, cx);
                }
            }
        }
        cx.notify();
    }
    /// 按鼠标落点建立片段，使用当前素材选区；片段间边界插入后移后续素材，其他重叠自动分轨。
    /// 成功后聚焦并选择新片段；失败回滚本次模型且显示实际错误。
    pub(crate) fn insert_media_clip(
        &mut self,
        index: usize,
        track: usize,
        start: i64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(m) = self.session.media.get(index).filter(|m| !m.offline) else {
            return;
        };
        let before = self
            .timeline
            .before
            .take()
            .unwrap_or_else(|| self.timeline.model.clone());
        let duration = self.timeline.model.frame(m.duration).max(1);
        let range = if self.session.selected == Some(index) {
            self.timeline.model.frame(self.session.trim_start)
                ..self
                    .timeline
                    .model
                    .frame(self.session.trim_end)
                    .min(duration)
        } else {
            0..duration
        };
        let insertion = self.timeline.model.insertion_frame(
            track,
            start,
            (8.0 / self.pixels_per_second() * self.timeline.model.fps as f32).round() as i64,
            &Default::default(),
        );
        let start = insertion.unwrap_or(start);
        let result = if insertion.is_some() {
            self.timeline.model.insert_ripple(
                m.path.clone(),
                m.name.clone(),
                track,
                start,
                range,
                duration,
            )
        } else {
            self.timeline.model.insert(
                m.path.clone(),
                m.name.clone(),
                track,
                start,
                range,
                duration,
            )
        };
        match result {
            Ok(id) => {
                self.timeline.selected.clear();
                self.timeline.selected.insert(id);
                self.timeline.preview = true;
                self.session.timeline_position = start as f64 / self.timeline.model.fps as f64;
                self.stop_preview();
                self.focus_handle.focus(window);
                self.commit_timeline(before, cx);
            }
            Err(error) => {
                self.timeline.model = before;
                self.notice = Some(error);
                cx.notify();
            }
        }
    }
    /// 提交前清理空非主轨并同步保存元数据；主轨保留，轨道清理与编辑共用一条撤销历史。
    /// 失败回滚本次编辑并显示实际错误，成功刷新轨道几何和预览。
    pub(crate) fn commit_timeline(&mut self, before: Timeline, cx: &mut Context<Self>) {
        self.timeline.model.remove_empty_tracks();
        self.timeline.rename = None;
        if before == self.timeline.model {
            return;
        }
        let result = self
            .current
            .as_ref()
            .ok_or_else(|| "没有打开项目".to_owned())
            .and_then(|entry| self.store.save_timeline(entry, &self.timeline.model));
        if let Err(error) = result {
            self.timeline.model = before;
            self.notice = Some(error);
        } else {
            self.timeline.history.record(before, &self.timeline.model);
        }
        self.refresh_timeline(cx);
    }
    /// 删除未锁定的选中片段，持久化成功后可撤销；不更改任何素材文件。
    pub(crate) fn delete_clips(&mut self, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        self.stop_timeline();
        let before = self.timeline.model.clone();
        self.timeline.model.delete(&self.timeline.selected);
        self.commit_timeline(before, cx);
        self.timeline
            .selected
            .retain(|id| self.timeline.model.clips.iter().any(|c| &c.id == id));
        cx.notify();
    }
    /// 撤销/重做由核心快照交换执行，保存失败时反向交换恢复历史和当前模型。
    pub(crate) fn timeline_history(&mut self, redo: bool, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        self.stop_timeline();
        let before = self.timeline.model.clone();
        self.timeline.history.step(&mut self.timeline.model, redo);
        if self.timeline.model != before
            && let Some(entry) = &self.current
        {
            if let Err(error) = self.store.save_timeline(entry, &self.timeline.model) {
                self.timeline.history.step(&mut self.timeline.model, !redo);
                self.notice = Some(error);
            }
            self.refresh_timeline(cx);
        }
    }
    /// 修改检查器参数，范围与合法性全部由核心校验，成功写入实时预览和项目。
    pub(crate) fn change_property(
        &mut self,
        property: Property,
        value: f64,
        cx: &mut Context<Self>,
    ) {
        let before = self.timeline.model.clone();
        if let Err(error) =
            self.timeline
                .model
                .set_property(&self.timeline.selected, property, value)
        {
            self.notice = Some(error);
        } else {
            self.commit_timeline(before, cx);
        }
        cx.notify();
    }
}
