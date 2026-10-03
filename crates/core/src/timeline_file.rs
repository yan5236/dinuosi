//! 时间线 V1 磁盘契约，与内存模型显式转换；旧项目缺失该字段时创建空时间线。
use crate::timeline::{Clip, Timeline, Track, TrackKind, Transform};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Serialize, Deserialize)]
pub(crate) struct TimelineV1 {
    fps: u32,
    canvas: [u32; 2],
    tracks: Vec<TrackV1>,
    clips: Vec<ClipV1>,
}
#[derive(Serialize, Deserialize)]
struct TrackV1 {
    #[serde(default)]
    kind: Option<String>,
    name: String,
    locked: bool,
    visible: bool,
    muted: bool,
}
#[derive(Serialize, Deserialize)]
struct ClipV1 {
    id: String,
    source: PathBuf,
    name: String,
    track: usize,
    start: i64,
    duration: i64,
    source_in: i64,
    source_frames: i64,
    transform: [f64; 5],
}
impl TimelineV1 {
    /// 显式复制领域数据到版本化字段，不保存选择、拖动和历史会话。
    pub fn from_timeline(t: &Timeline) -> Self {
        Self {
            fps: t.fps,
            canvas: [t.width, t.height],
            tracks: t
                .tracks
                .iter()
                .map(|v| TrackV1 {
                    kind: Some(
                        match v.kind {
                            TrackKind::Main => "main",
                            TrackKind::Media => "media",
                            TrackKind::Subtitle => "subtitle",
                        }
                        .into(),
                    ),
                    name: v.name.clone(),
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
                })
                .collect(),
        }
    }
    /// 校验外部字段并迁移无类型的旧三轨格式；载入后移除所有空非主轨，主轨始终保留。
    /// 旧重叠片段分配到上方新轨道，源选区和时间位置不变；损坏的数据不会进入编辑器。
    pub fn into_timeline(self) -> Result<Timeline, String> {
        let legacy = self.tracks.iter().all(|t| t.kind.is_none());
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
                Some("subtitle") => Ok(TrackKind::Subtitle),
                None if legacy => Ok([TrackKind::Subtitle, TrackKind::Media, TrackKind::Main][i]),
                _ => Err("轨道类型无效".to_owned()),
            })
            .collect::<Result<Vec<_>, _>>()?;
        let mut timeline = Timeline {
            fps: self.fps,
            width: self.canvas[0],
            height: self.canvas[1],
            tracks: self
                .tracks
                .into_iter()
                .zip(kinds)
                .map(|(v, kind)| Track {
                    kind,
                    name: v.name,
                    locked: v.locked,
                    visible: v.visible,
                    muted: v.muted,
                })
                .collect(),
            clips: self
                .clips
                .into_iter()
                .map(|c| Clip {
                    id: c.id,
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
                })
                .collect(),
        };
        if legacy {
            // 先检查所有时间和索引，再转换重叠，避免损坏输入进入索引/算术操作。
            let clips = std::mem::take(&mut timeline.clips);
            for c in clips {
                let mut single = timeline.clone();
                single.clips = vec![c.clone()];
                single.validate()?;
                timeline.clips.push(c);
            }
            for index in (0..2).rev() {
                if !timeline.clips.iter().any(|c| c.track == index) {
                    timeline.tracks.remove(index);
                    for c in &mut timeline.clips {
                        if c.track > index {
                            c.track -= 1;
                        }
                    }
                }
            }
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
