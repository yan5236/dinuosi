//! 时间线 V1 磁盘契约，与内存模型显式转换；旧项目缺失该字段时创建空时间线。
use crate::timeline::{Clip, Timeline, Track, Transform};
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
    /// 校验所有外部字段后返回内存模型；损坏的数据不会进入编辑器。
    pub fn into_timeline(self) -> Result<Timeline, String> {
        let timeline = Timeline {
            fps: self.fps,
            width: self.canvas[0],
            height: self.canvas[1],
            tracks: self
                .tracks
                .into_iter()
                .map(|v| Track {
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
        timeline.validate()?;
        Ok(timeline)
    }
}
