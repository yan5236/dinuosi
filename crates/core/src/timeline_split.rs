//! 鼠标刀片与播放头共用的帧精确切割规则，不移动其他片段或修改源文件。
use crate::{media::MediaKind, timeline::Timeline};
use std::collections::BTreeSet;

impl Timeline {
    /// 在项目绝对帧 frame 切割 ids 中跨越该帧的未锁定片段，返回所有新右段的身份。
    /// 左段保留原身份；右段复制轨道、源引用、变换、增益和动画，关键帧局部时间减去左段时长。
    /// 两侧保留范围外插值锚点，视频/音频源入点按左段时长推进，图片入点不变。
    /// 锁轨、未知身份、片段外及首尾边界忽略，至少保留两侧各一帧；校验失败原子回滚，不产生部分切割。
    pub fn split(
        &mut self,
        ids: &BTreeSet<String>,
        frame: i64,
    ) -> Result<BTreeSet<String>, String> {
        let mut next = self.clone();
        let mut right_ids = BTreeSet::new();
        let mut rights = Vec::new();
        for clip in next.clips.iter_mut().filter(|c| ids.contains(&c.id)) {
            if clip.locked
                || next.tracks[clip.track].locked
                || frame <= clip.start
                || frame >= clip.start + clip.duration
            {
                continue;
            }
            let left_duration = frame - clip.start;
            let mut right = clip.clone();
            right.id = uuid::Uuid::new_v4().to_string();
            right.start = frame;
            for key in &mut right.keyframes {
                key.frame -= left_duration;
            }
            right.duration -= left_duration;
            if right.kind != MediaKind::Image {
                right.source_in += left_duration;
            }
            clip.duration = left_duration;
            right_ids.insert(right.id.clone());
            rights.push(right);
        }
        if !rights.is_empty() {
            next.clips.extend(rights);
            next.validate()?;
            *self = next;
        }
        Ok(right_ids)
    }
}
