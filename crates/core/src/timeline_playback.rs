//! 片段倒放与定格的共享时间映射；编辑保留源引用、时长和时间线动画。
use crate::{
    media::MediaKind,
    timeline::{Clip, Timeline},
};
use std::collections::BTreeSet;

impl Clip {
    /// 将项目绝对帧映射为源帧；定格优先，倒放从当前选区尾帧开始，区间外夹紧到首尾。
    /// 返回项目帧率下的非负源帧，变速使用同一精确源区间和曲线，供预览和导出共同寻址。
    pub fn source_frame(&self, frame: i64) -> i64 {
        self.freeze_frame.unwrap_or_else(|| {
            let local = (frame - self.start).clamp(0, self.duration - 1);
            if self.speed.is_some() {
                let [a, b] = self.source_bounds();
                let boundary = self.source_boundary(local as f64);
                let source = if self.reversed {
                    (boundary - 1e-9).ceil() - 1.0
                } else {
                    (boundary + 1e-9).floor()
                };
                return source.clamp(a.floor(), (b.ceil() - 1.0).max(a.floor())) as i64;
            }
            self.source_in
                + if self.reversed {
                    self.duration - 1 - local
                } else {
                    local
                }
        })
    }
}

impl Timeline {
    /// 逐个切换选中视频、音频的倒放方向；锁片、锁轨、图片及未知身份忽略。
    /// 选区、时长和关键帧保留，定格仍优先显示固定源帧；宿主负责保存与历史事务。
    pub fn reverse_clips(&mut self, ids: &BTreeSet<String>) {
        for c in self.clips.iter_mut().filter(|c| {
            ids.contains(&c.id)
                && matches!(c.kind, MediaKind::Video | MediaKind::Audio)
                && !c.locked
                && !self.tracks[c.track].locked
        }) {
            c.reversed = !c.reversed;
        }
    }

    /// 切换选中视频的整段定格；开启时使用播放头对应源画面，片段外取最近首尾帧。
    /// 再次点击清除固定源帧，恢复原选区、方向和声音；锁片、锁轨及非视频忽略。
    pub fn freeze_clips(&mut self, ids: &BTreeSet<String>, frame: i64) {
        for c in self.clips.iter_mut().filter(|c| {
            ids.contains(&c.id)
                && c.kind == MediaKind::Video
                && !c.locked
                && !self.tracks[c.track].locked
        }) {
            c.freeze_frame = if c.freeze_frame.is_some() {
                None
            } else {
                Some(c.source_frame(frame))
            };
        }
    }
}
