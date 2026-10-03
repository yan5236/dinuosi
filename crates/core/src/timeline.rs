//! 与 GUI 无关的帧精确时间线、轨道约束、变换和事务历史。

use std::{collections::BTreeSet, path::PathBuf};
use uuid::Uuid;

/// 宿主设置接口：默认拖动中更新，备用模式在释放播放头后更新；不写入项目内容。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ScrubPreview {
    #[default]
    Live,
    OnRelease,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Transform {
    pub opacity: f64,
    pub scale: f64,
    pub x: f64,
    pub y: f64,
    pub rotation: f64,
}

impl Default for Transform {
    /// 默认以预览中心为原点、100% 大小与不透明度，无平移和旋转。
    fn default() -> Self {
        Self {
            opacity: 100.0,
            scale: 100.0,
            x: 0.0,
            y: 0.0,
            rotation: 0.0,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Property {
    Opacity,
    Scale,
    X,
    Y,
    Rotation,
}

impl Transform {
    /// 读取指定检查器参数；坐标单位为项目画布像素，旋转单位为度。
    pub fn get(&self, property: Property) -> f64 {
        match property {
            Property::Opacity => self.opacity,
            Property::Scale => self.scale,
            Property::X => self.x,
            Property::Y => self.y,
            Property::Rotation => self.rotation,
        }
    }
    /// 设置有限数值并夹紧业务范围；拒绝 NaN/无穷，不产生部分写入。
    pub fn set(&mut self, property: Property, value: f64) -> Result<(), String> {
        if !value.is_finite() {
            return Err("参数必须为有限数值".into());
        }
        let target = match property {
            Property::Opacity => &mut self.opacity,
            Property::Scale => &mut self.scale,
            Property::X => &mut self.x,
            Property::Y => &mut self.y,
            Property::Rotation => &mut self.rotation,
        };
        let (min, max) = property.range();
        *target = value.clamp(min, max);
        Ok(())
    }
}

impl Property {
    /// 返回检查器及所有宿主共用的合法范围；坐标上限防止异常输入溢出绘制。
    pub fn range(self) -> (f64, f64) {
        match self {
            Self::Opacity => (0.0, 100.0),
            Self::Scale => (0.0, 500.0),
            Self::Rotation => (-360.0, 360.0),
            _ => (-100_000.0, 100_000.0),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Clip {
    pub id: String,
    pub source: PathBuf,
    pub name: String,
    pub track: usize,
    pub start: i64,
    pub duration: i64,
    pub source_in: i64,
    pub source_frames: i64,
    pub transform: Transform,
}

/// 轨道职责与显示名分离；字幕类型保留，但新项目仅创建主轨。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TrackKind {
    Main,
    Media,
    Subtitle,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Track {
    pub kind: TrackKind,
    pub name: String,
    pub locked: bool,
    pub visible: bool,
    pub muted: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Timeline {
    /// 时间为 frame / fps 的有理数，不累计浮点秒误差。
    pub fps: u32,
    pub width: u32,
    pub height: u32,
    pub tracks: Vec<Track>,
    pub clips: Vec<Clip>,
}

impl Default for Timeline {
    /// 创建唯一空主轨；字幕与媒体轨道按需创建，不生成示例素材。
    fn default() -> Self {
        Self {
            fps: 30,
            width: 1920,
            height: 1080,
            clips: Vec::new(),
            tracks: vec![Track {
                kind: TrackKind::Main,
                name: "主轨".into(),
                locked: false,
                visible: true,
                muted: false,
            }],
        }
    }
}

impl Timeline {
    /// 将秒吸附到最近项目帧；非法/负数返回零，最长限制为一天。
    pub fn frame(&self, seconds: f64) -> i64 {
        if !seconds.is_finite() {
            return 0;
        }
        (seconds.clamp(0.0, 86400.0) * self.fps as f64).round() as i64
    }
    /// 返回最后片段结束帧，空项目返回零。
    pub fn end(&self) -> i64 {
        self.clips
            .iter()
            .map(|c| c.start + c.duration)
            .max()
            .unwrap_or(0)
    }
    /// 校验载入/提交数据的轨道、时长、唯一身份和参数；错误不修改状态。
    pub fn validate(&self) -> Result<(), String> {
        if !(1..=240).contains(&self.fps)
            || self.width == 0
            || self.height == 0
            || self.width > 16384
            || self.height > 16384
            || self.tracks.is_empty()
            || self.tracks.last().is_none_or(|t| t.kind != TrackKind::Main)
            || self
                .tracks
                .iter()
                .filter(|t| t.kind == TrackKind::Main)
                .count()
                != 1
        {
            return Err("时间线格式无效".into());
        }
        let mut ids = BTreeSet::new();
        for clip in &self.clips {
            if !ids.insert(&clip.id)
                || Uuid::parse_str(&clip.id).is_err()
                || clip.track >= self.tracks.len()
                || clip.start < 0
                || clip.start > 86400 * self.fps as i64
                || clip.duration < 1
                || clip.source_in < 0
                || clip.source_frames < 1
                || clip.source_in > clip.source_frames
                || clip.source_frames > 86400 * self.fps as i64
                || clip.duration > clip.source_frames - clip.source_in
            {
                return Err("片段身份或时间范围无效".into());
            }
            for property in [
                Property::Opacity,
                Property::Scale,
                Property::X,
                Property::Y,
                Property::Rotation,
            ] {
                let v = clip.transform.get(property);
                let (min, max) = property.range();
                if !v.is_finite() || v < min || v > max {
                    return Err("片段变换参数无效".into());
                }
            }
        }
        for (i, clip) in self.clips.iter().enumerate() {
            if self.clips[..i].iter().any(|other| {
                other.track == clip.track
                    && other.start < clip.start + clip.duration
                    && clip.start < other.start + other.duration
            }) {
                return Err("同一轨道的片段不能重叠".into());
            }
        }
        Ok(())
    }
    /// 对选中且未锁定片段设置同一检查器参数；输入先校验，避免部分更新。
    pub fn set_property(
        &mut self,
        ids: &BTreeSet<String>,
        property: Property,
        value: f64,
    ) -> Result<(), String> {
        Transform::default().set(property, value)?;
        for c in self
            .clips
            .iter_mut()
            .filter(|c| ids.contains(&c.id) && !self.tracks[c.track].locked)
        {
            c.transform.set(property, value)?;
        }
        Ok(())
    }
    /// 根据时间区间和轨道区间执行矩形相交选择，锁定轨道不参与编辑选择。
    pub fn select_range(
        &self,
        start: i64,
        end: i64,
        first_track: usize,
        last_track: usize,
    ) -> BTreeSet<String> {
        self.clips
            .iter()
            .filter(|c| {
                !self.tracks[c.track].locked
                    && c.track >= first_track
                    && c.track <= last_track
                    && c.start < end
                    && c.start + c.duration > start
            })
            .map(|c| c.id.clone())
            .collect()
    }
}

#[derive(Default)]
pub struct History {
    undo: Vec<Timeline>,
    redo: Vec<Timeline>,
}
impl History {
    /// 一次手势提交一条历史；相同状态不入栈，历史最多保留一百次编辑。
    pub fn record(&mut self, before: Timeline, after: &Timeline) {
        if &before != after {
            self.undo.push(before);
            self.redo.clear();
            if self.undo.len() > 100 {
                self.undo.remove(0);
            }
        }
    }
    /// 交换当前状态与历史快照；没有可撤销/重做的记录时保持原状。
    pub fn step(&mut self, state: &mut Timeline, redo: bool) {
        let (from, to) = if redo {
            (&mut self.redo, &mut self.undo)
        } else {
            (&mut self.undo, &mut self.redo)
        };
        if let Some(previous) = from.pop() {
            to.push(std::mem::replace(state, previous));
        }
    }
}

pub struct Tick {
    pub frame: i64,
    pub major: bool,
    pub label: String,
}
/// 按像素密度生成可见刻度；最大缩放每帧短线、每两帧标注，秒界显示时间码。
pub fn ticks(fps: u32, pixels_per_second: f64, first: f64, width: f64) -> Vec<Tick> {
    let fps = fps.max(1) as i64;
    let ppf = pixels_per_second.max(1.0) / fps as f64;
    let mut steps = vec![1, 2, 5, 10, 15];
    steps.extend([1, 2, 5, 10, 15, 30, 60, 120, 300, 600, 1800, 3600].map(|s| s * fps));
    steps.sort_unstable();
    steps.dedup();
    let major = steps
        .into_iter()
        .find(|s| *s as f64 * ppf >= 90.0)
        .unwrap_or(3600 * fps);
    let minor = if major < fps {
        1
    } else if major % 10 == 0 {
        major / 10
    } else {
        (major / 5).max(1)
    };
    let start = ((first.max(0.0) * fps as f64) as i64 / minor) * minor;
    let end =
        ((first.max(0.0) + width.max(0.0) / pixels_per_second.max(1.0)) * fps as f64).ceil() as i64;
    (start..=end)
        .step_by(minor as usize)
        .map(|frame| {
            let is_major = frame % major == 0;
            let label = if !is_major {
                String::new()
            } else if frame % fps == 0 {
                format!("{:02}:{:02}", frame / fps / 60, frame / fps % 60)
            } else {
                format!("{}f", frame % fps)
            };
            Tick {
                frame,
                major: is_major,
                label,
            }
        })
        .collect()
}
