//! 时间线 V1 磁盘契约，与内存模型显式转换；旧项目缺失该字段时创建空时间线。
use crate::media::{self, MediaKind};
use crate::timeline::{Clip, Timeline, Track, TrackKind, Transform};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Serialize, Deserialize)]
pub(crate) struct TimelineV1 {
    fps: u32,
    canvas: [u32; 2],
    #[serde(default = "default_bitrate")]
    bitrate_kbps: u32,
    tracks: Vec<TrackV1>,
    clips: Vec<ClipV1>,
    #[serde(default)]
    markers: Vec<MarkerV1>,
    #[serde(default = "default_snapping")]
    snapping: bool,
}
#[derive(Serialize, Deserialize)]
struct TrackV1 {
    #[serde(default)]
    kind: Option<String>,
    name: String,
    #[serde(default)]
    custom_name: Option<bool>,
    locked: bool,
    visible: bool,
    muted: bool,
}
#[derive(Serialize, Deserialize)]
struct ClipV1 {
    id: String,
    #[serde(default)]
    locked: bool,
    source: PathBuf,
    name: String,
    track: usize,
    start: i64,
    duration: i64,
    source_in: i64,
    source_frames: i64,
    transform: [f64; 5],
    #[serde(default)]
    kind: Option<String>,
    #[serde(default = "default_gain")]
    gain: f64,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    keyframes: Vec<KeyframeV1>,
}
#[derive(Serialize, Deserialize)]
struct MarkerV1 {
    id: String,
    frame: i64,
    name: String,
    color: u32,
}
/// 旧项目默认启用磁吸，不要求重新保存后才能拖放。
fn default_snapping() -> bool {
    true
}
#[derive(Serialize, Deserialize)]
struct KeyframeV1 {
    frame: i64,
    kind: String,
    values: [f64; 2],
}
/// 旧项目未保存音量时使用单位增益，即 0.0dB，不改变原有声音大小。
fn default_gain() -> f64 {
    1.0
}
/// 旧项目未指定码率时使用 12Mbps，保持旧文件可直接打开。
fn default_bitrate() -> u32 {
    12000
}
impl TimelineV1 {
    /// 显式复制领域数据、片段锁定、标记、磁吸、码率及关键帧到版本化字段，不保存选择、拖动和历史。
    pub fn from_timeline(t: &Timeline) -> Self {
        Self {
            fps: t.fps,
            snapping: t.snapping,
            markers: t
                .markers
                .iter()
                .map(|m| MarkerV1 {
                    id: m.id.clone(),
                    frame: m.frame,
                    name: m.name.clone(),
                    color: m.color,
                })
                .collect(),
            canvas: [t.width, t.height],
            bitrate_kbps: t.bitrate_kbps,
            tracks: t
                .tracks
                .iter()
                .map(|v| TrackV1 {
                    kind: Some(
                        match v.kind {
                            TrackKind::Main => "main",
                            TrackKind::Media => "media",
                            TrackKind::Audio => "audio",
                            TrackKind::Subtitle => "subtitle",
                        }
                        .into(),
                    ),
                    name: v.name.clone(),
                    custom_name: Some(v.custom_name),
                    locked: v.locked,
                    visible: v.visible,
                    muted: v.muted,
                })
                .collect(),
            clips: t
                .clips
                .iter()
                .map(|c| ClipV1 {
                    id: c.id.clone(),
                    locked: c.locked,
                    source: c.source.clone(),
                    name: c.name.clone(),
                    track: c.track,
                    start: c.start,
                    duration: c.duration,
                    source_in: c.source_in,
                    source_frames: c.source_frames,
                    transform: [
                        c.transform.opacity,
                        c.transform.scale,
                        c.transform.x,
                        c.transform.y,
                        c.transform.rotation,
                    ],
                    kind: Some(
                        match c.kind {
                            MediaKind::Audio => "audio",
                            MediaKind::Video => "video",
                            MediaKind::Image => "image",
                        }
                        .into(),
                    ),
                    gain: c.gain,
                    keyframes: c
                        .keyframes
                        .iter()
                        .map(|k| KeyframeV1 {
                            frame: k.frame,
                            kind: k.kind.name().into(),
                            values: k.values,
                        })
                        .collect(),
                })
                .collect(),
        }
    }
    /// 校验外部字段及码率并迁移无类型的旧三轨格式；载入后移除所有空非主轨，主轨始终保留。
    /// 旧音频片段迁移至独立音频轨道且音量默认 0dB；源选区、时间、轨道开关保持不变。
    /// 缺失自定义名称标记时仅将与旧默认名一致的轨道判为默认，其余名称保留用户原文。
    /// 旧重叠片段分配到上方新轨道；缺失关键帧和标记使用空集合，片段默认解锁、磁吸默认开启。
    /// 损坏类型、增益、重复关键帧或非法动画值返回错误，不进入编辑器。
    pub fn into_timeline(self) -> Result<Timeline, String> {
        let legacy = self.tracks.iter().all(|t| t.kind.is_none());
        let migrate_audio = self.clips.iter().any(|c| c.kind.is_none());
        if legacy && self.tracks.len() != 3 {
            return Err("旧时间线轨道格式无效".into());
        }
        let kinds = self
            .tracks
            .iter()
            .enumerate()
            .map(|(i, t)| match t.kind.as_deref() {
                Some("main") => Ok(TrackKind::Main),
                Some("media") => Ok(TrackKind::Media),
                Some("audio") => Ok(TrackKind::Audio),
                Some("subtitle") => Ok(TrackKind::Subtitle),
                None if legacy => Ok([TrackKind::Subtitle, TrackKind::Media, TrackKind::Main][i]),
                _ => Err("轨道类型无效".to_owned()),
            })
            .collect::<Result<Vec<_>, _>>()?;
        let mut timeline = Timeline {
            fps: self.fps,
            snapping: self.snapping,
            markers: self
                .markers
                .into_iter()
                .map(|m| crate::timeline_tools::Marker {
                    id: m.id,
                    frame: m.frame,
                    name: m.name,
                    color: m.color,
                })
                .collect(),
            width: self.canvas[0],
            height: self.canvas[1],
            bitrate_kbps: self.bitrate_kbps,
            tracks: self
                .tracks
                .into_iter()
                .zip(kinds)
                .map(|(v, kind)| Track {
                    kind,
                    custom_name: v.custom_name.unwrap_or(
                        v.name
                            != match kind {
                                TrackKind::Main => "主轨",
                                TrackKind::Media => "媒体轨道",
                                TrackKind::Audio => "音频轨道",
                                TrackKind::Subtitle => "字幕轨道",
                            },
                    ),
                    name: v.name,
                    locked: v.locked,
                    visible: v.visible,
                    muted: v.muted,
                })
                .collect(),
            clips: self
                .clips
                .into_iter()
                .map(|c| {
                    let kind = match c.kind.as_deref() {
                        Some("audio") => MediaKind::Audio,
                        Some("image") => MediaKind::Image,
                        Some("video") => MediaKind::Video,
                        None => {
                            media::probe(&c.source)
                                .unwrap_or_else(|_| media::offline(c.source.clone()))
                                .kind
                        }
                        _ => return Err("素材类型无效".to_owned()),
                    };
                    Ok(Clip {
                        id: c.id,
                        locked: c.locked,
                        source: c.source,
                        name: c.name,
                        track: c.track,
                        start: c.start,
                        duration: c.duration,
                        source_in: c.source_in,
                        source_frames: c.source_frames,
                        transform: Transform {
                            opacity: c.transform[0],
                            scale: c.transform[1],
                            x: c.transform[2],
                            y: c.transform[3],
                            rotation: c.transform[4],
                        },
                        kind,
                        gain: c.gain,
                        keyframes: c
                            .keyframes
                            .into_iter()
                            .map(|k| {
                                Ok(crate::keyframes::Keyframe {
                                    frame: k.frame,
                                    kind: crate::keyframes::KeyframeKind::parse(&k.kind)?,
                                    values: k.values,
                                })
                            })
                            .collect::<Result<Vec<_>, String>>()?,
                    })
                })
                .collect::<Result<Vec<_>, _>>()?,
        };
        // 迁移插轨会重映射索引，必须先拒绝越界输入，避免损坏文件导致索引加法溢出。
        if timeline
            .clips
            .iter()
            .any(|c| c.track >= timeline.tracks.len())
        {
            return Err("片段轨道索引无效".into());
        }
        if migrate_audio {
            for index in (0..timeline.tracks.len()).rev() {
                if !timeline
                    .clips
                    .iter()
                    .any(|c| c.track == index && c.kind == MediaKind::Audio)
                {
                    continue;
                }
                if timeline.tracks[index].kind == TrackKind::Media
                    && timeline
                        .clips
                        .iter()
                        .filter(|c| c.track == index)
                        .all(|c| c.kind == MediaKind::Audio)
                {
                    timeline.tracks[index].kind = TrackKind::Audio;
                    if timeline.tracks[index].name == "媒体轨道" {
                        timeline.tracks[index].name = "音频轨道".into();
                    }
                } else if timeline.tracks[index].kind.accepts(MediaKind::Audio) {
                    continue;
                } else {
                    let old = timeline.tracks[index].clone();
                    timeline.create_track(index, TrackKind::Audio)?;
                    timeline.tracks[index].locked = old.locked;
                    timeline.tracks[index].muted = old.muted;
                    timeline.tracks[index].visible = old.visible;
                    for c in &mut timeline.clips {
                        if c.track == index + 1 && c.kind == MediaKind::Audio {
                            c.track = index;
                        }
                    }
                }
            }
        }
        if legacy {
            // 先检查所有时间和索引，再转换重叠，避免损坏输入进入索引/算术操作。
            let clips = std::mem::take(&mut timeline.clips);
            for c in clips {
                let mut single = timeline.clone();
                single.clips = vec![c.clone()];
                single.validate()?;
                timeline.clips.push(c);
            }
            timeline.remove_empty_tracks();
            let ids: Vec<_> = timeline.clips.iter().map(|c| c.id.clone()).collect();
            // 后加入的旧片段原本绘制在上层，迁移时优先把它分配到上方新轨道。
            for id in ids.into_iter().rev() {
                timeline.resolve_overlap(&id);
            }
        }
        timeline.validate()?;
        timeline.remove_empty_tracks();
        Ok(timeline)
    }
}
