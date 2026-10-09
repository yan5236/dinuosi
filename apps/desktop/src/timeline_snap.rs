//! 屏幕像素容差与 Core 帧磁吸之间的适配，拖入、移动和预览使用相同落点。
use crate::{timeline_state::Gesture, view::Desktop};
use dinuosi_core::timeline::Timeline;
use gpui::{Pixels, Point};
impl Desktop {
    /// 把固定 8px 磁吸范围换算为当前缩放下的整数帧；高缩放时允许零帧精确匹配。
    pub(crate) fn snap_tolerance(&self) -> i64 {
        (8.0 / self.pixels_per_second() * self.timeline.model.fps as f32).floor() as i64
    }
    /// 对新片段拟放置首尾同时吸附，返回合法起点；不修改模型，关闭磁吸时只夹紧负时间。
    pub(crate) fn snap_clip_start(&self, model: &Timeline, start: i64, duration: i64) -> i64 {
        let start = start.max(0);
        start
            + model.snap_offset(
                &[start, start + duration],
                &Default::default(),
                self.keyframe_frame(),
                self.snap_tolerance(),
            )
    }
    /// 使用当前素材预览裁剪区计算库拖入起点；预览提示与释放提交共用，音视频范围不越过源时长。
    pub(crate) fn library_drop_start(&self, index: usize) -> i64 {
        let model = &self.timeline.model;
        let m = &self.session.media[index];
        let duration = if self.session.selected == Some(index) {
            model
                .frame(self.session.trim_end)
                .min(model.frame(m.duration))
                - model.frame(self.session.trim_start)
        } else {
            model.frame(m.duration)
        };
        self.snap_clip_start(
            model,
            self.timeline_frame_at(self.timeline.pointer.x),
            duration,
        )
    }
    /// 从按下坐标和滚动差计算整组位移，再吸附每个选中片段首尾；排除选择集合自身，保留组内间距。
    pub(crate) fn clip_move_delta(&self, model: &Timeline, start: Point<Pixels>) -> i64 {
        let start = self.timeline_gesture_start(start);
        let raw = (f32::from(self.timeline.pointer.x - start.x) / self.pixels_per_second()
            * model.fps as f32)
            .round() as i64;
        let clips: Vec<_> = model
            .clips
            .iter()
            .filter(|c| self.timeline.selected.contains(&c.id))
            .collect();
        let min = clips.iter().map(|c| c.start).min().unwrap_or(0);
        let max = clips.iter().map(|c| c.start).max().unwrap_or(0);
        let delta = raw.clamp(-min, 86400 * model.fps as i64 - max);
        let edges: Vec<_> = clips
            .iter()
            .flat_map(|c| [c.start + delta, c.start + c.duration + delta])
            .collect();
        (delta
            + model.snap_offset(
                &edges,
                &self.timeline.selected,
                self.keyframe_frame(),
                self.snap_tolerance(),
            ))
        .clamp(-min, 86400 * model.fps as i64 - max)
    }
    /// 在吸附后的边界绘制细竖线，只有移动组或库素材存在实际吸附修正时显示；关闭磁吸时隐藏。
    pub(crate) fn snap_guide_frame(&self) -> Option<i64> {
        if !self.timeline.model.snapping || !self.timeline.moved {
            return None;
        }
        let model = self
            .timeline
            .before
            .as_ref()
            .unwrap_or(&self.timeline.model);
        let (edges, exclude) = match self.timeline.gesture.as_ref()? {
            Gesture::Move { start, .. } => {
                let delta = self.clip_move_delta(model, *start);
                (
                    model
                        .clips
                        .iter()
                        .filter(|c| self.timeline.selected.contains(&c.id))
                        .flat_map(|c| [c.start + delta, c.start + c.duration + delta])
                        .collect::<Vec<_>>(),
                    self.timeline.selected.clone(),
                )
            }
            Gesture::Library { index, .. } => {
                let start = self.library_drop_start(*index);
                let duration = if self.session.selected == Some(*index) {
                    model.frame(self.session.trim_end) - model.frame(self.session.trim_start)
                } else {
                    model.frame(self.session.media[*index].duration)
                };
                (vec![start, start + duration], Default::default())
            }
            _ => return None,
        };
        [self.keyframe_frame(), 0]
            .into_iter()
            .chain(model.markers.iter().map(|m| m.frame))
            .chain(
                model
                    .clips
                    .iter()
                    .filter(|c| !exclude.contains(&c.id))
                    .flat_map(|c| [c.start, c.start + c.duration]),
            )
            .find(|f| edges.contains(f))
    }
}
