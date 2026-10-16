//! 鼠标刀片与播放头共用的帧精确切割规则，不移动其他片段或修改源文件。
use crate::{media::MediaKind, timeline::Timeline};
use std::collections::BTreeSet;

impl Timeline {
    /// 在项目绝对帧 frame 切割 ids 中跨越该帧的未锁定片段，返回所有新右段的身份。
    /// 左段保留原身份和关联；右段复制源、变换、增益和动画，同组右段建立独立关联，单个右段不关联。
    /// 关键帧局部时间减去左段时长，宿主决定是否展开关联成员，允许 Alt 独立切割。
    /// 两侧保留范围外插值锚点；变速按源边界重映射曲线，倒放左段保留高端，定格源帧不变。
    /// 锁轨、未知身份、片段外及首尾边界忽略，至少保留两侧各一帧；校验失败原子回滚，不产生部分切割。
    pub fn split(
        &mut self,
        ids: &BTreeSet<String>,
        frame: i64,
    ) -> Result<BTreeSet<String>, String> {
        let mut next = self.clone();
        let mut right_ids = BTreeSet::new();
        let mut rights = Vec::new();
        let mut right_links = std::collections::BTreeMap::new();
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
            right.link = clip.link.as_ref().map(|link| {
                right_links
                    .entry(link.clone())
                    .or_insert_with(|| uuid::Uuid::new_v4().to_string())
                    .clone()
            });
            right.start = frame;
            for key in &mut right.keyframes {
                key.frame -= left_duration;
            }
            if clip.speed.is_some() {
                let duration = clip.duration;
                right.slice_speed(left_duration as f64, duration as f64);
                clip.slice_speed(0.0, left_duration as f64);
            } else {
                right.duration -= left_duration;
                if !matches!(right.kind, MediaKind::Image | MediaKind::Text) {
                    if clip.reversed {
                        clip.source_in += right.duration;
                    } else {
                        right.source_in += left_duration;
                    }
                }
            }
            clip.duration = left_duration;
            right_ids.insert(right.id.clone());
            rights.push(right);
        }
        if !rights.is_empty() {
            next.clips.extend(rights);
            next.remove_single_links();
            next.validate()?;
            *self = next;
        }
        Ok(right_ids)
    }
}
