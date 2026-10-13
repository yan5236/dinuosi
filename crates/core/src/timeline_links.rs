//! 片段关联与组裁边；关联只保存身份，选择由宿主决定，允许 Alt 临时独立编辑。
use crate::timeline::Timeline;
use std::collections::{BTreeMap, BTreeSet};

impl Timeline {
    /// 展开已知片段身份的关联成员，忽略不存在的身份；不修改模型或原选择集合。
    pub fn linked_ids(&self, ids: &BTreeSet<String>) -> BTreeSet<String> {
        let links: BTreeSet<_> = self
            .clips
            .iter()
            .filter(|c| ids.contains(&c.id))
            .filter_map(|c| c.link.as_ref())
            .collect();
        self.clips
            .iter()
            .filter(|c| {
                ids.contains(&c.id) || c.link.as_ref().is_some_and(|link| links.contains(link))
            })
            .map(|c| c.id.clone())
            .collect()
    }

    /// 判断关联操作是否产生有效变化；整组必须可编辑，关联至少两段，已属同组时不重复关联。
    /// 解除操作作用于选择触及的完整关联组；空选择、未知身份及锁定成员均禁用。
    pub fn can_link_clips(&self, ids: &BTreeSet<String>, unlink: bool) -> bool {
        let expanded = self.linked_ids(ids);
        if ids.is_empty() || !ids.is_subset(&expanded) {
            return false;
        }
        let clips: Vec<_> = self
            .clips
            .iter()
            .filter(|c| expanded.contains(&c.id))
            .collect();
        if clips
            .iter()
            .any(|c| c.locked || self.tracks[c.track].locked)
        {
            return false;
        }
        if unlink {
            clips.iter().any(|c| c.link.is_some())
        } else {
            clips.len() >= 2
                && (clips[0].link.is_none() || clips.iter().any(|c| c.link != clips[0].link))
        }
    }

    /// 关联选择触及的全部成员或解除其整组关联；保持所有时间、源引用和参数不变。
    /// 无有效变化或含锁定成员时返回 false 且不修改模型；关联使用新的 UUID，支持多个旧组合并。
    pub fn link_clips(&mut self, ids: &BTreeSet<String>, unlink: bool) -> bool {
        if !self.can_link_clips(ids, unlink) {
            return false;
        }
        let expanded = self.linked_ids(ids);
        let link = (!unlink).then(|| uuid::Uuid::new_v4().to_string());
        for clip in self.clips.iter_mut().filter(|c| expanded.contains(&c.id)) {
            clip.link = link.clone();
        }
        true
    }

    /// 校验持久化关联身份必须为 UUID 且至少有两名成员；损坏关联拒绝载入或提交。
    pub(crate) fn validate_links(&self) -> Result<(), String> {
        let mut counts = BTreeMap::new();
        for link in self.clips.iter().filter_map(|c| c.link.as_ref()) {
            if uuid::Uuid::parse_str(link).is_err() {
                return Err("片段关联数据无效".into());
            }
            *counts.entry(link).or_insert(0) += 1;
        }
        if counts.values().any(|count| *count < 2) {
            return Err("片段关联数据无效".into());
        }
        Ok(())
    }

    /// 删除或切割后清理仅剩一个成员的关联；其他组的身份保持不变，避免遗留不可解除的单段组。
    pub(crate) fn remove_single_links(&mut self) {
        let mut counts = BTreeMap::new();
        for link in self.clips.iter().filter_map(|c| c.link.clone()) {
            *counts.entry(link).or_insert(0) += 1;
        }
        for clip in &mut self.clips {
            if clip.link.as_ref().is_some_and(|link| counts[link] < 2) {
                clip.link = None;
            }
        }
    }

    /// 原子裁剪指定集合的同侧边缘，复用单段规则，以所有成员允许的共同帧增量保持时间间距。
    /// 不自动展开关联，供宿主支持 Alt 独立裁边；未知或锁定成员返回错误，失败不留下部分修改。
    pub fn trim_clips(
        &mut self,
        ids: &BTreeSet<String>,
        left: bool,
        delta: i64,
    ) -> Result<(), String> {
        let mut trial = self.clone();
        let mut common = delta;
        for id in ids {
            let clip = self
                .clips
                .iter()
                .find(|c| &c.id == id)
                .ok_or("片段不存在")?;
            let edge = if left {
                clip.start
            } else {
                clip.start + clip.duration
            };
            trial.trim(id, left, delta)?;
            let clip = trial.clips.iter().find(|c| &c.id == id).unwrap();
            let applied = if left {
                clip.start
            } else {
                clip.start + clip.duration
            } - edge;
            common = if delta >= 0 {
                common.min(applied)
            } else {
                common.max(applied)
            };
        }
        let mut next = self.clone();
        for id in ids {
            next.trim(id, left, common)?;
        }
        next.validate()?;
        *self = next;
        Ok(())
    }
}
