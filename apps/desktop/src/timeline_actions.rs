//! 指针手势到核心编辑事务的适配；拖动中只更新预览，释放时一次保存与记录历史。
use crate::{
    timeline_state::{DropTarget, Gesture},
    view::Desktop,
};
use dinuosi_core::timeline::{Property, TrackKind};
use gpui::{Context, MouseDownEvent, Pixels, Point, Window};

impl Desktop {
    /// 刀片模式委派切割；默认选择完整关联组，Ctrl/Shift 切换整组，Alt 临时只选择单段。
    /// 点击的已选成员成为检查器主对象，显示与变速按其类型读取，不改变整组联动集合。
    /// 锁定片段仍可选择；整组有锁定片段或轨道时不启动移动/裁边，直接点击锁轨忽略输入。
    /// 进入片段选择时清除曲线节点选择，恢复 Delete 的素材删除语义。
    pub(crate) fn begin_clip(
        &mut self,
        id: String,
        edge: Option<bool>,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.timeline.blade
            && !self
                .timeline
                .model
                .clips
                .iter()
                .any(|c| c.id == id && c.locked)
        {
            self.click_blade_clip(id, event, window, cx);
            return;
        }
        self.commit_inspector_input(cx);
        self.timeline.curves.selected = None;
        let Some(clip) = self.timeline.model.clips.iter().find(|c| c.id == id) else {
            return;
        };
        if self.busy || self.timeline.model.tracks[clip.track].locked {
            return;
        }
        let track = clip.track;
        let locked = clip.locked;
        let ids = if event.modifiers.alt {
            [id.clone()].into()
        } else {
            self.timeline.model.linked_ids(&[id.clone()].into())
        };
        if event.modifiers.control || event.modifiers.shift {
            if ids.is_subset(&self.timeline.selected) {
                self.timeline
                    .selected
                    .retain(|selected| !ids.contains(selected));
            } else {
                self.timeline.selected.extend(ids);
            }
        } else if event.modifiers.alt || !self.timeline.selected.contains(&id) {
            self.timeline.selected = ids;
        } else {
            self.timeline.selected.extend(ids);
        }
        self.timeline.primary_clip = self.timeline.selected.contains(&id).then(|| id.clone());
        self.focus_handle.focus(window);
        self.timeline.preview = true;
        self.stop_timeline();
        self.stop_preview();
        if locked
            || self.timeline.model.clips.iter().any(|c| {
                self.timeline.selected.contains(&c.id)
                    && (c.locked || self.timeline.model.tracks[c.track].locked)
            })
        {
            self.refresh_timeline(cx);
            cx.stop_propagation();
            return;
        }
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
    /// 空白区开始框选且不移动播放头；Shift/Ctrl 保留已有选择，Alt 不展开关联，刀片工具忽略空白。
    /// 框选没有唯一点击目标，清除检查器主对象并沿用首个选中成员。
    /// 框选同时清除曲线节点选择，避免旧节点拦截工作区删除操作。
    pub(crate) fn begin_marquee(
        &mut self,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.commit_inspector_input(cx);
        self.timeline.curves.selected = None;
        if self.busy || self.timeline.blade {
            return;
        }
        self.focus_handle.focus(window);
        let additive = if event.modifiers.shift || event.modifiers.control {
            self.timeline.selected.clone()
        } else {
            Default::default()
        };
        self.timeline.selected = additive.clone();
        self.timeline.primary_clip = None;
        self.timeline.gesture_scroll = gpui::point(self.timeline.scroll_x, self.timeline.scroll_y);
        self.timeline.gesture = Some(Gesture::Marquee {
            start: event.position,
            additive,
            linked: !event.modifiers.alt,
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
    /// 处理活动手势的移动，片段修改从按下快照计算；音量按比例映射增益并停止旧声音。
    /// 裁边对当前选择中的关联成员使用共同帧增量；参数与预览变换只更新内存，释放时保存一次历史。
    /// 曲线节点与切线委派独立适配器，使用按下时冻结的坐标范围及模型快照。
    /// 检查器滑块输入按界面坐标转换，Y 向上为正；预览手势继续使用存储坐标。
    pub(crate) fn move_timeline_pointer(&mut self, point: Point<Pixels>, cx: &mut Context<Self>) {
        let Some(gesture) = &self.timeline.gesture else {
            return;
        };
        self.timeline.pointer = point;
        match gesture {
            Gesture::Color(drag) => {
                let drag = drag.clone();
                self.move_color_drag(&drag, point, cx);
            }
            Gesture::Curve(drag) => {
                let drag = drag.clone();
                self.move_curve_point(point, drag, cx);
            }
            Gesture::Preview(_) => self.move_preview_transform(point, cx),
            Gesture::Marquee {
                start,
                additive,
                linked,
            } => {
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
                self.timeline.selected = if *linked {
                    self.timeline.model.linked_ids(&selected)
                } else {
                    selected
                };
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
                    let ids = before
                        .linked_ids(&[id.clone()].into())
                        .intersection(&self.timeline.selected)
                        .cloned()
                        .collect();
                    let delta = before
                        .clips
                        .iter()
                        .find(|c| &c.id == id)
                        .map_or(delta, |c| {
                            let edge = if *left { c.start } else { c.start + c.duration };
                            delta
                                + before.snap_offset(
                                    &[edge + delta],
                                    &ids,
                                    self.keyframe_frame(),
                                    self.snap_tolerance(),
                                )
                        });
                    let _ = next.trim_clips(&ids, *left, delta);
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
            Gesture::Volume { bounds } => {
                let ratio = (f32::from(point.x - bounds.left())
                    / f32::from(bounds.size.width).max(1.0))
                .clamp(0.0, 1.0);
                if let Some(before) = &self.timeline.before {
                    self.timeline.model = before.clone();
                }
                let _ = self.timeline.model.set_volume_at(
                    &self.timeline.selected,
                    dinuosi_core::timeline::volume_gain(ratio as f64),
                    self.keyframe_frame(),
                    self.auto_keyframe(cx),
                );
                self.stop_timeline();
            }
            Gesture::Property { property, bounds } => {
                let ratio = (f32::from(point.x - bounds.left())
                    / f32::from(bounds.size.width).max(1.0))
                .clamp(0.0, 1.0);
                let (min, max) = property.range();
                // 每次拖动从快照计算，防止关闭自动添加时整条动画累计偏移。
                if let Some(before) = &self.timeline.before {
                    self.timeline.model = before.clone();
                }
                let _ = self.timeline.model.set_property_at(
                    &self.timeline.selected,
                    *property,
                    Self::property_coordinate(
                        *property,
                        (min + ratio as f64 * (max - min)).round(),
                    ),
                    self.keyframe_frame(),
                    self.auto_keyframe(cx),
                );
                self.refresh_timeline(cx);
            }
            Gesture::Speed { bounds } => {
                let ratio = (f32::from(point.x - bounds.left())
                    / f32::from(bounds.size.width).max(1.0))
                .clamp(0.0, 1.0);
                if let Some(before) = &self.timeline.before {
                    self.timeline.model = before.clone();
                }
                let pitch = self
                    .speed_clip()
                    .and_then(|c| c.speed.as_ref())
                    .is_some_and(|s| s.pitch);
                match self.preview_speed(
                    crate::speed_inspector::rate_from_ratio(ratio as f64),
                    &[],
                    pitch,
                ) {
                    Ok(()) => {
                        self.timeline.preview = true;
                        self.refresh_timeline(cx);
                    }
                    Err(error) => self.notice = Some(error),
                }
            }
            Gesture::SpeedPoint { bounds, index } => {
                let (bounds, index) = (*bounds, *index);
                if let Some(before) = &self.timeline.before {
                    self.timeline.model = before.clone();
                }
                self.move_speed_point(point, bounds, index, cx);
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
                            .create_track(i, TrackKind::for_media(self.session.media[index].kind))
                            .unwrap(),
                    };
                    self.insert_media_clip(
                        index,
                        track,
                        self.library_drop_start(index),
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
    /// 按鼠标落点建立片段，使用当前素材选区；片段间剩余空隙足够时直接放入，不足才后移后续素材，其他重叠自动分轨。
    /// 音频按核心规则分轨，成功后选择新片段并切换对应检查器；失败回滚并显示实际错误。
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
        let start = self.snap_clip_start(&self.timeline.model, start, range.end - range.start);
        let insertion = self.timeline.model.tracks[track]
            .kind
            .accepts(m.kind)
            .then(|| {
                self.timeline
                    .model
                    .insertion_frame(track, start, 0, &Default::default())
            })
            .flatten();
        let start = insertion.unwrap_or(start);
        let result = if insertion.is_some() {
            self.timeline.model.insert_ripple(
                (m.path.clone(), m.kind),
                m.name.clone(),
                track,
                start,
                range,
                duration,
            )
        } else {
            self.timeline.model.insert(
                (m.path.clone(), m.kind),
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
                self.timeline.primary_clip = Some(id.clone());
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
    /// 停止播放并按当前项目帧修改检查器参数；核心处理类型隔离、自动关键帧及范围校验。
    /// 输入为界面参数（Y 向上为正），转换到旧项目坐标后保存；失败显示真实错误并保持原参数。
    pub(crate) fn change_property(
        &mut self,
        property: Property,
        value: f64,
        cx: &mut Context<Self>,
    ) {
        self.stop_timeline();
        let before = self.timeline.model.clone();
        if let Err(error) = self.timeline.model.set_property_at(
            &self.timeline.selected,
            property,
            Self::property_coordinate(property, value),
            self.keyframe_frame(),
            self.auto_keyframe(cx),
        ) {
            self.notice = Some(error);
        } else {
            self.commit_timeline(before, cx);
        }
        cx.notify();
    }
}
