//! 动态轨道和片段放置规则；所有时间均为项目帧，主轨与自由轨道都允许空隙。
use crate::media::MediaKind;
use crate::timeline::{Clip, Timeline, Track, TrackKind, Transform};
use std::{collections::BTreeSet, path::PathBuf};
use uuid::Uuid;

impl Timeline {
    /// 返回唯一主轨索引；有效模型中主轨始终位于最下方。
    pub fn main_track(&self) -> usize {
        self.tracks.len() - 1
    }

    /// 在指定行上方创建媒体、音频或字幕轨道并重映射片段索引；非法位置或重复主轨返回错误。
    /// 返回新轨道索引；手势中暂留空轨道供落点使用，提交时统一清理。
    pub fn create_track(&mut self, index: usize, kind: TrackKind) -> Result<usize, String> {
        if index >= self.tracks.len() || kind == TrackKind::Main {
            return Err("无法在此处创建轨道".into());
        }
        self.tracks.insert(
            index,
            Track {
                kind,
                name: match kind {
                    TrackKind::Subtitle => "字幕轨道",
                    TrackKind::Audio => "音频轨道",
                    _ => "媒体轨道",
                }
                .into(),
                locked: false,
                visible: true,
                muted: false,
            },
        );
        for clip in &mut self.clips {
            if clip.track >= index {
                clip.track += 1;
            }
        }
        Ok(index)
    }

    /// 将已有自由轨道移动到指定插入线，主轨保持最下方；锁定轨道或非法位置拒绝修改。
    /// 片段随轨道整体移动，轨道名称和开关保持不变，不产生多余空轨道。
    pub fn reorder_track(&mut self, from: usize, above: usize) -> Result<(), String> {
        if from >= self.main_track() || above > self.main_track() || self.tracks[from].locked {
            return Err("无法移动此轨道".into());
        }
        let to = if above > from { above - 1 } else { above };
        let track = self.tracks.remove(from);
        self.tracks.insert(to, track);
        for clip in &mut self.clips {
            clip.track = if clip.track == from {
                to
            } else if from < to && (from + 1..=to).contains(&clip.track) {
                clip.track - 1
            } else if to < from && (to..from).contains(&clip.track) {
                clip.track + 1
            } else {
                clip.track
            };
        }
        Ok(())
    }

    /// 按落点插入片段；source 为已探测的路径及类型，源选区单位为项目帧。
    /// 音频落在主轨/媒体轨道时在其上方建音频轨道；图片、视频拒绝音频轨道。
    /// 冲突复用紧邻上方同类型空轨道或新建；非法输入不修改模型。
    pub fn insert(
        &mut self,
        source: (PathBuf, MediaKind),
        name: String,
        track: usize,
        start: i64,
        range: std::ops::Range<i64>,
        source_frames: i64,
    ) -> Result<String, String> {
        self.accept_track(track)?;
        let (source, kind) = source;
        if kind != MediaKind::Audio && !self.tracks[track].kind.accepts(kind) {
            return Err("此轨道不能接收此类素材".into());
        }
        if range.start < 0
            || range.end <= range.start
            || range.end > source_frames
            || source_frames > 86400 * self.fps as i64
            || start < 0
            || start > 86400 * self.fps as i64
        {
            return Err("素材选区无效".into());
        }
        let track = if self.tracks[track].kind.accepts(kind) {
            track
        } else {
            self.create_track(track, TrackKind::Audio)?
        };
        let id = Uuid::new_v4().to_string();
        self.clips.push(Clip {
            id: id.clone(),
            source,
            name,
            track,
            start,
            duration: range.end - range.start,
            source_in: range.start,
            source_frames,
            transform: Transform::default(),
            kind,
            gain: 1.0,
        });
        self.resolve_overlap(&id);
        Ok(id)
    }

    /// 原子移动整组选中片段，保留时间间距，零点/一天边界整组夹紧；主轨也可留空隙。
    /// 非法、锁定或类型不匹配目标拒绝整个操作；重叠片段优先复用紧邻上方空轨道，否则新建，不覆盖已有素材。
    pub fn move_clips(
        &mut self,
        ids: &BTreeSet<String>,
        delta: i64,
        track_delta: isize,
    ) -> Result<(), String> {
        let selected: Vec<_> = self.clips.iter().filter(|c| ids.contains(&c.id)).collect();
        for c in &selected {
            self.accept_track(c.track)?;
            let target = c.track as isize + track_delta;
            if target < 0 {
                return Err("无法移动到此轨道".into());
            }
            self.accept_track(target as usize)?;
            if !self.tracks[target as usize].kind.accepts(c.kind) {
                return Err("此轨道不能接收此类素材".into());
            }
        }
        let min = selected.iter().map(|c| c.start).min().unwrap_or(0);
        let max = selected.iter().map(|c| c.start).max().unwrap_or(0);
        let delta = delta.clamp(-min, 86400 * self.fps as i64 - max);
        let mut next = self.clone();
        for c in next.clips.iter_mut().filter(|c| ids.contains(&c.id)) {
            c.start += delta;
            c.track = (c.track as isize + track_delta) as usize;
        }
        for id in ids {
            next.resolve_overlap(id);
        }
        *self = next;
        Ok(())
    }

    /// 拖动片段左右边缘，最短一帧且不超出源时长；左边缘同步调整源入点。
    /// 延长后若与同轨片段相交，裁剪片段优先复用上方空轨道，否则新建；锁轨或不存在返回错误。
    pub fn trim(&mut self, id: &str, left: bool, delta: i64) -> Result<(), String> {
        let c = self
            .clips
            .iter_mut()
            .find(|c| c.id == id)
            .ok_or("片段不存在")?;
        if self.tracks[c.track].locked {
            return Err("轨道已锁定".into());
        }
        if left {
            let delta = delta.clamp(-c.source_in.min(c.start), c.duration - 1);
            c.start += delta;
            c.source_in += delta;
            c.duration -= delta;
        } else {
            c.duration = c
                .duration
                .saturating_add(delta)
                .clamp(1, c.source_frames - c.source_in);
        }
        self.resolve_overlap(id);
        Ok(())
    }

    /// 删除选中的未锁定片段并清理空自由轨道；保留主轨、素材库、源文件和剩余片段时间位置。
    pub fn delete(&mut self, ids: &BTreeSet<String>) {
        self.clips
            .retain(|c| !ids.contains(&c.id) || self.tracks[c.track].locked);
        self.remove_empty_tracks();
    }

    /// 在编辑提交或项目载入时删除所有空自由轨道并重映射索引；唯一主轨始终保留。
    /// 手势预览期间不调用，确保刚腾空的源轨道可先被冲突片段复用。
    pub fn remove_empty_tracks(&mut self) {
        for index in (0..self.tracks.len()).rev() {
            if self.tracks[index].kind != TrackKind::Main
                && !self.clips.iter().any(|c| c.track == index)
            {
                self.tracks.remove(index);
                for clip in &mut self.clips {
                    if clip.track > index {
                        clip.track -= 1;
                    }
                }
            }
        }
    }

    /// 查找两个相邻片段之间的插入位置；空隙内保留帧落点，相接边界按像素换算的容差吸附。
    /// 排除正在移动的片段；容差限制为相邻片段最短时长的三分之一，避免短片段整个成为插入区。
    pub fn insertion_frame(
        &self,
        track: usize,
        frame: i64,
        tolerance: i64,
        exclude: &BTreeSet<String>,
    ) -> Option<i64> {
        let mut clips: Vec<_> = self
            .clips
            .iter()
            .filter(|c| c.track == track && !exclude.contains(&c.id))
            .collect();
        clips.sort_by_key(|c| c.start);
        clips.windows(2).find_map(|pair| {
            let end = pair[0].start + pair[0].duration;
            let next = pair[1].start;
            let tolerance = tolerance
                .max(0)
                .min(pair[0].duration.min(pair[1].duration) / 3);
            if end > next || frame < end - tolerance || frame > next + tolerance {
                return None;
            }
            Some(frame.clamp(end, next))
        })
    }

    /// 在片段间插入新素材并把同轨后续片段整体后移一个新片段时长，保留后续相对空隙。
    /// source 为路径及已探测类型；类型、源选区、锁轨或时间上限错误返回且整个模型不变；调用方先确认插入边界，不覆盖前段。
    pub fn insert_ripple(
        &mut self,
        source: (PathBuf, MediaKind),
        name: String,
        track: usize,
        start: i64,
        range: std::ops::Range<i64>,
        source_frames: i64,
    ) -> Result<String, String> {
        let mut next = self.clone();
        next.accept_track(track)?;
        if !next.tracks[track].kind.accepts(source.1) {
            return Err("此轨道不能接收此类素材".into());
        }
        next.shift_following(track, start, range.end.saturating_sub(range.start))?;
        let id = next.insert(source, name, track, start, range, source_frames)?;
        next.validate()?;
        *self = next;
        Ok(id)
    }

    /// 将已有单个片段插入目标轨道的片段间，后续片段按其时长后移；源轨道保留时间空隙。
    /// 锁轨、类型不匹配、片段不存在、前段相交或时间越界拒绝整个操作，其他轨道不受影响。
    pub fn move_clip_ripple(&mut self, id: &str, track: usize, start: i64) -> Result<(), String> {
        let mut next = self.clone();
        let index = next
            .clips
            .iter()
            .position(|c| c.id == id)
            .ok_or("片段不存在")?;
        next.accept_track(next.clips[index].track)?;
        next.accept_track(track)?;
        if !next.tracks[track].kind.accepts(next.clips[index].kind) {
            return Err("此轨道不能接收此类素材".into());
        }
        let mut clip = next.clips.remove(index);
        next.shift_following(track, start, clip.duration)?;
        clip.start = start;
        clip.track = track;
        next.clips.push(clip);
        next.validate()?;
        *self = next;
        Ok(())
    }

    /// 检查并后移同轨起点及之后的片段；先验证全部时间上限，避免一半成功一半失败。
    fn shift_following(&mut self, track: usize, start: i64, duration: i64) -> Result<(), String> {
        self.accept_track(track)?;
        if start < 0
            || duration < 1
            || self
                .clips
                .iter()
                .any(|c| c.track == track && c.start < start && c.start + c.duration > start)
            || self.clips.iter().any(|c| {
                c.track == track
                    && c.start >= start
                    && c.start
                        .checked_add(duration)
                        .is_none_or(|s| s > 86400 * self.fps as i64)
            })
        {
            return Err("插入后片段超出时间范围".into());
        }
        for c in self
            .clips
            .iter_mut()
            .filter(|c| c.track == track && c.start >= start)
        {
            c.start += duration;
        }
        Ok(())
    }

    /// 检查目标轨道是否能接收媒体；字幕类型保留独立用途，锁定或不存在返回错误。
    fn accept_track(&self, index: usize) -> Result<(), String> {
        if self
            .tracks
            .get(index)
            .is_none_or(|t| t.locked || t.kind == TrackKind::Subtitle)
        {
            return Err("此轨道不能接收素材".into());
        }
        Ok(())
    }

    /// 消除指定片段与同轨其他片段的严格相交；首尾相接合法，冲突时只移动指定片段。
    /// 先复用紧邻上方未锁定的同类型空轨道，再按需插轨并统一更新索引；不依赖显示名。
    pub(crate) fn resolve_overlap(&mut self, id: &str) {
        let Some(index) = self.clips.iter().position(|c| c.id == id) else {
            return;
        };
        let c = &self.clips[index];
        if self.clips.iter().any(|other| {
            other.id != c.id
                && other.track == c.track
                && other.start < c.start + c.duration
                && c.start < other.start + other.duration
        }) {
            let track = c.track;
            let kind = TrackKind::for_media(c.kind);
            if track > 0
                && self.tracks[track - 1].kind == kind
                && !self.tracks[track - 1].locked
                && !self.clips.iter().any(|c| c.track == track - 1)
            {
                self.clips[index].track = track - 1;
            } else {
                self.create_track(track, kind).expect("有效片段轨道");
                self.clips[index].track = track;
            }
        }
    }
}
