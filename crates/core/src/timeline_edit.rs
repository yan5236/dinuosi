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
                    TrackKind::Text => "文本轨道",
                    TrackKind::Audio => "音频轨道",
                    _ => "媒体轨道",
                }
                .into(),
                custom_name: false,
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
    /// 片段随轨道整体移动；含锁定片段时拒绝重排，轨道名称和开关保持不变，不产生多余空轨道。
    pub fn reorder_track(&mut self, from: usize, above: usize) -> Result<(), String> {
        if from >= self.main_track() || above > self.main_track() || self.tracks[from].locked {
            return Err("无法移动此轨道".into());
        }
        if self.clips.iter().any(|c| c.track == from && c.locked) {
            return Err("片段已锁定".into());
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
        if kind == MediaKind::Text {
            return Err("生成文字必须通过文本接口添加".into());
        }
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
            link: None,
            locked: false,
            source,
            name,
            track,
            start,
            duration: range.end - range.start,
            source_in: range.start,
            source_frames,
            reversed: false,
            freeze_frame: None,
            speed: None,
            transform: Transform::default(),
            color: crate::color::Grade::default(),
            text: None,
            kind,
            gain: 1.0,
            keyframes: Vec::new(),
        });
        self.resolve_overlap(&id);
        Ok(id)
    }

    /// 原子移动整组选中片段，保留时间间距，零点/一天边界整组夹紧；主轨也可留空隙。
    /// 非法、锁定或类型不匹配目标拒绝整个操作；重叠片段优先复用紧邻上方空轨道，否则新建，不覆盖已有素材。
    /// 文字以结束帧限制一天边界，其他媒体保留起点边界；整组使用最严格的可移动增量。
    pub fn move_clips(
        &mut self,
        ids: &BTreeSet<String>,
        delta: i64,
        track_delta: isize,
    ) -> Result<(), String> {
        let selected: Vec<_> = self.clips.iter().filter(|c| ids.contains(&c.id)).collect();
        for c in &selected {
            if c.locked {
                return Err("片段已锁定".into());
            }
            if self.tracks[c.track].locked {
                return Err("轨道已锁定".into());
            }
            let target = c.track as isize + track_delta;
            if target < 0 {
                return Err("无法移动到此轨道".into());
            }
            if self.tracks.get(target as usize).is_none_or(|t| {
                t.locked
                    || !t.kind.accepts(c.kind)
                    || (c.kind == MediaKind::Text && t.kind != c.track_kind())
            }) {
                return Err("此轨道不能接收此类素材".into());
            }
        }
        let min = selected.iter().map(|c| c.start).min().unwrap_or(0);
        let max = selected
            .iter()
            .map(|c| {
                c.start
                    + if c.kind == MediaKind::Text {
                        c.duration
                    } else {
                        0
                    }
            })
            .max()
            .unwrap_or(0);
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

    /// 按帧增量拖动片段左右边缘，最短一帧；正放左裁边调整源入点，倒放右裁边调整源入点。
    /// 变速按精确源边界重映射曲线，延伸使用端点倍率并夹紧源范围；关键帧随左裁边调整局部时间。
    /// 图片左右均可延长，不改变源入点；起点保持在零至一天内，时长遵守项目的一天范围。
    /// 左裁边同步平移关键帧局部位置，保留范围外锚点，使保留画面的动画连续。
    /// 文字右裁边的最大时长为一天减起点，确保结束帧始终通过文本校验。
    /// 延长后若与同轨片段相交，裁剪片段优先复用上方空轨道，否则新建；锁轨或不存在返回错误。
    pub fn trim(&mut self, id: &str, left: bool, delta: i64) -> Result<(), String> {
        let c = self
            .clips
            .iter_mut()
            .find(|c| c.id == id)
            .ok_or("片段不存在")?;
        if c.locked {
            return Err("片段已锁定".into());
        }
        if self.tracks[c.track].locked {
            return Err("轨道已锁定".into());
        }
        if c.speed.is_some() {
            let [a, b] = c.source_bounds();
            let speed = c.speed.as_ref().unwrap();
            let scale = c.duration as f64 / ((b - a) * speed.integral(1.0));
            let available = if left == c.reversed {
                c.source_frames as f64 - b
            } else {
                a
            };
            let edge = if left { 0.0 } else { 1.0 };
            let extension = (available / speed.rate_at(edge) * scale).floor() as i64;
            if left {
                let delta = delta.clamp(-extension.min(c.start), c.duration - 1);
                c.slice_speed(delta as f64, c.duration as f64);
                c.start += delta;
                for key in &mut c.keyframes {
                    key.frame -= delta;
                }
            } else {
                let duration = c
                    .duration
                    .saturating_add(delta)
                    .clamp(1, (c.duration + extension).min(86400 * self.fps as i64));
                c.slice_speed(0.0, duration as f64);
            }
            self.resolve_overlap(id);
            return Ok(());
        }
        if left {
            let (min, max) = if matches!(c.kind, MediaKind::Image | MediaKind::Text) {
                let limit = 86400 * self.fps as i64;
                (
                    (-c.start).max(c.duration - limit),
                    (c.duration - 1).min(limit - c.start),
                )
            } else if c.reversed {
                (
                    -(c.source_frames - c.source_in - c.duration).min(c.start),
                    c.duration - 1,
                )
            } else {
                (-c.source_in.min(c.start), c.duration - 1)
            };
            let delta = delta.clamp(min, max);
            c.start += delta;
            for key in &mut c.keyframes {
                key.frame -= delta;
            }
            if !matches!(c.kind, MediaKind::Image | MediaKind::Text) && !c.reversed {
                c.source_in += delta;
            }
            c.duration -= delta;
        } else {
            let max = if c.kind == MediaKind::Text {
                86400 * self.fps as i64 - c.start
            } else if c.kind == MediaKind::Image {
                86400 * self.fps as i64
            } else if c.reversed {
                c.duration + c.source_in
            } else {
                c.source_frames - c.source_in
            };
            let duration = c.duration.saturating_add(delta).clamp(1, max);
            if c.reversed {
                c.source_in += c.duration - duration;
            }
            c.duration = duration;
        }
        self.resolve_overlap(id);
        Ok(())
    }

    /// 删除选中的未锁定片段并清理单成员关联和空自由轨道；保留源文件及剩余片段时间位置。
    pub fn delete(&mut self, ids: &BTreeSet<String>) {
        self.clips
            .retain(|c| !ids.contains(&c.id) || c.locked || self.tracks[c.track].locked);
        self.remove_single_links();
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

    /// 判断片段间落点的剩余空隙是否不足，start/duration 为项目帧；排除正在移动的片段。
    /// 仅当新片段严格覆盖同轨后续片段的起点时返回 true；刚好放满空隙无需后移，不修改模型。
    pub fn insertion_requires_shift(
        &self,
        track: usize,
        start: i64,
        duration: i64,
        exclude: &BTreeSet<String>,
    ) -> bool {
        self.clips.iter().any(|c| {
            c.track == track
                && !exclude.contains(&c.id)
                && c.start >= start
                && c.start < start.saturating_add(duration)
        })
    }

    /// 在片段间插入新素材；剩余空隙足够时保留后续位置，不足时整体后移一个新片段时长，保留后续相对空隙。
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

    /// 将已有单个片段插入目标轨道的片段间，剩余空隙不足时后续片段按其时长后移；源轨道保留时间空隙。
    /// 锁轨、类型不匹配、片段不存在、前段相交或时间越界拒绝整个操作，其他轨道不受影响。
    pub fn move_clip_ripple(&mut self, id: &str, track: usize, start: i64) -> Result<(), String> {
        let mut next = self.clone();
        let index = next
            .clips
            .iter()
            .position(|c| c.id == id)
            .ok_or("片段不存在")?;
        if next.clips[index].locked {
            return Err("片段已锁定".into());
        }
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

    /// 检查片段间落点，空隙足够时不后移；不足时展开后续片段的关联并整体后移，保持音画时间差。
    /// start/duration 为项目帧；锁轨、前段相交或时间越界返回错误，避免一半成功一半失败。
    /// 后移后的关联成员不得覆盖插入区间；冲突返回错误，由调用方丢弃候选模型而非自动分轨。
    fn shift_following(&mut self, track: usize, start: i64, duration: i64) -> Result<(), String> {
        self.accept_track(track)?;
        let shift = if self.insertion_requires_shift(track, start, duration, &BTreeSet::new()) {
            duration
        } else {
            0
        };
        let following = if shift > 0 {
            self.linked_ids(
                &self
                    .clips
                    .iter()
                    .filter(|c| c.track == track && c.start >= start)
                    .map(|c| c.id.clone())
                    .collect(),
            )
        } else {
            BTreeSet::new()
        };
        if self
            .clips
            .iter()
            .any(|c| following.contains(&c.id) && (c.locked || self.tracks[c.track].locked))
        {
            return Err("片段已锁定".into());
        }
        if start < 0
            || duration < 1
            || self
                .clips
                .iter()
                .any(|c| c.track == track && c.start < start && c.start + c.duration > start)
            || self.clips.iter().any(|c| {
                following.contains(&c.id)
                    && c.start
                        .checked_add(shift)
                        .is_none_or(|s| s > 86400 * self.fps as i64)
            })
        {
            return Err("插入后片段超出时间范围".into());
        }
        for c in self.clips.iter_mut().filter(|c| following.contains(&c.id)) {
            c.start += shift;
        }
        if self.clips.iter().any(|c| {
            c.track == track
                && c.start < start.saturating_add(duration)
                && start < c.start + c.duration
        }) {
            return Err("同一轨道的片段不能重叠".into());
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
            let kind = c.track_kind();
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
