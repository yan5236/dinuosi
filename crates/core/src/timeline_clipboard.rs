//! 时间线选区快照及原子粘贴；仅引用源文件，不复制媒体数据。
use crate::timeline::{Timeline, TrackKind};
use std::collections::{BTreeMap, BTreeSet};
use uuid::Uuid;

impl Timeline {
    /// 复制明确选中的片段及轨道定义，保留源引用、全部参数和相对位置。
    /// 不隐式展开关联；仅复制部分组时清理单成员关联，空选择返回 None，原模型不变。
    pub fn copy_clips(&self, ids: &BTreeSet<String>) -> Option<Self> {
        let mut copy = self.clone();
        copy.clips.retain(|c| ids.contains(&c.id));
        if copy.clips.is_empty() {
            return None;
        }
        copy.markers.clear();
        copy.remove_single_links();
        Some(copy)
    }

    /// 在指定项目帧原子粘贴选区快照，返回新身份集合；最早片段对齐落点，其他成员保持时间差。
    /// 复用项目换帧率校验，复制完整参数并重新生成片段、关联及字幕组身份；新片段解锁。
    /// 优先原职责轨道，重叠时整行分轨，缺失轨道按职责重建；锁轨、非法数据或越界均不修改模型。
    pub fn paste_clips(&mut self, copy: &Self, frame: i64) -> Result<BTreeSet<String>, String> {
        copy.validate()?;
        if copy.clips.is_empty() {
            return Ok(BTreeSet::new());
        }
        let mut copy = copy.clone();
        copy.configure(copy.width, copy.height, self.fps, copy.bitrate_kbps)?;
        let origin = copy.clips.iter().map(|c| c.start).min().unwrap();
        for clip in &mut copy.clips {
            clip.start = clip
                .start
                .checked_sub(origin)
                .and_then(|s| s.checked_add(frame))
                .filter(|s| (0..=86400 * self.fps as i64).contains(s))
                .ok_or("片段身份或时间范围无效")?;
        }
        let mut next = self.clone();
        let mut tracks = BTreeMap::new();
        let used: BTreeSet<_> = copy.clips.iter().map(|c| c.track).collect();
        for source in used.into_iter().rev() {
            let kind = copy.tracks[source].kind;
            let ceiling = tracks.values().copied().min().unwrap_or(next.tracks.len());
            let target = if kind == TrackKind::Main {
                Some(next.main_track())
            } else {
                let accepts = |i: &usize| *i < ceiling && next.tracks[*i].kind == kind;
                (source < next.main_track() && accepts(&source))
                    .then_some(source)
                    .or_else(|| (0..next.main_track()).rev().find(accepts))
            };
            if target.is_some_and(|i| next.tracks[i].locked) {
                return Err("轨道已锁定".into());
            }
            let collision = target.is_some_and(|track| {
                copy.clips.iter().filter(|c| c.track == source).any(|c| {
                    next.clips.iter().any(|other| {
                        other.track == track
                            && other.start < c.start + c.duration
                            && c.start < other.start + other.duration
                    })
                })
            });
            let target = if let Some(track) = target.filter(|_| !collision) {
                track
            } else {
                let index = target.unwrap_or(ceiling.min(next.main_track()));
                next.create_track(
                    index,
                    if kind == TrackKind::Main {
                        TrackKind::Media
                    } else {
                        kind
                    },
                )?;
                for track in tracks.values_mut() {
                    if *track >= index {
                        *track += 1;
                    }
                }
                index
            };
            tracks.insert(source, target);
        }
        let mut links = BTreeMap::new();
        let mut groups = BTreeMap::new();
        let mut ids = BTreeSet::new();
        for mut clip in copy.clips {
            clip.id = Uuid::new_v4().to_string();
            clip.locked = false;
            clip.track = tracks[&clip.track];
            if let Some(link) = &mut clip.link {
                *link = links
                    .entry(link.clone())
                    .or_insert_with(|| Uuid::new_v4().to_string())
                    .clone();
            }
            if let Some(group) = clip.text.as_mut().and_then(|t| t.group.as_mut()) {
                *group = groups
                    .entry(group.clone())
                    .or_insert_with(|| Uuid::new_v4().to_string())
                    .clone();
            }
            ids.insert(clip.id.clone());
            next.clips.push(clip);
        }
        next.validate()?;
        *self = next;
        Ok(ids)
    }
}
