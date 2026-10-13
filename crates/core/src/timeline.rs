//! 与 GUI 无关的帧精确时间线、轨道约束、变换和事务历史。

use crate::media::MediaKind;
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
    /// 按素材自身中心翻转像素，独立于缩放、平移和旋转，不改变动画关键帧。
    pub mirror_horizontal: bool,
    pub mirror_vertical: bool,
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
            mirror_horizontal: false,
            mirror_vertical: false,
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
    /// 锁定只禁止片段编辑，时间线和检查器仍可选择。
    pub locked: bool,
    pub source: PathBuf,
    pub name: String,
    pub track: usize,
    pub start: i64,
    pub duration: i64,
    pub source_in: i64,
    pub source_frames: i64,
    /// 倒放使用当前源选区的逆序帧；定格保存绝对源帧，取消后恢复原选区和方向。
    pub reversed: bool,
    pub freeze_frame: Option<i64>,
    pub speed: Option<crate::speed::Speed>,
    pub transform: Transform,
    pub kind: MediaKind,
    /// 线性音频增益：1 为 0dB，0 为负无穷，10 为 20dB；避免 JSON 保存无穷数值。
    pub gain: f64,
    pub keyframes: Vec<crate::keyframes::Keyframe>,
}

/// 轨道职责与显示名分离；字幕类型保留，但新项目仅创建主轨。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TrackKind {
    Main,
    Media,
    Audio,
    Subtitle,
}

impl TrackKind {
    /// 按素材类型返回自由轨道职责；视频与图片共用媒体轨道，音频使用独立音频轨道。
    pub fn for_media(kind: MediaKind) -> Self {
        if kind == MediaKind::Audio {
            Self::Audio
        } else {
            Self::Media
        }
    }
    /// 判断轨道可接收的素材类型；主轨和媒体轨道仅支持图片、视频，字幕不接收这些素材。
    pub fn accepts(self, kind: MediaKind) -> bool {
        match self {
            Self::Main | Self::Media => kind != MediaKind::Audio,
            Self::Audio => kind == MediaKind::Audio,
            Self::Subtitle => false,
        }
    }
}

impl Clip {
    /// 把保存的线性增益转换为检查器 dB；零增益返回负无穷，默认增益返回 0.0。
    pub fn volume_db(&self) -> f64 {
        20.0 * self.gain.log10()
    }
}

/// 将滑块比例映射为线性音频增益；零端点严格静音，其余位置覆盖 -60 至 20dB。
/// 输入由指针夹紧，默认 0dB 位于 75%；保存有限增益，支持持久化与 PCM 混合。
pub fn volume_gain(ratio: f64) -> f64 {
    if ratio <= 0.0 {
        0.0
    } else {
        10.0_f64.powf((ratio.clamp(0.0, 1.0) * 80.0 - 60.0) / 20.0)
    }
}

/// 将有限线性增益转换为滑块比例；静音位于最左端，20dB 位于最右端。
pub fn volume_ratio(gain: f64) -> f64 {
    if gain <= 0.0 {
        0.0
    } else {
        ((20.0 * gain.log10() + 60.0) / 80.0).clamp(0.0, 1.0)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Track {
    pub kind: TrackKind,
    pub name: String,
    /// 用户重命名后原样展示；默认名称由界面按轨道类型翻译。
    pub custom_name: bool,
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
    pub bitrate_kbps: u32,
    pub tracks: Vec<Track>,
    pub clips: Vec<Clip>,
    pub markers: Vec<crate::timeline_tools::Marker>,
    pub snapping: bool,
}

impl Default for Timeline {
    /// 创建唯一空主轨并默认开启磁吸；标记为空，字幕、媒体与音频轨道按需创建，不生成示例素材。
    fn default() -> Self {
        Self {
            fps: 30,
            width: 1920,
            height: 1080,
            bitrate_kbps: 12000,
            clips: Vec::new(),
            markers: Vec::new(),
            snapping: true,
            tracks: vec![Track {
                kind: TrackKind::Main,
                name: "主轨".into(),
                custom_name: false,
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
    /// 校验载入/提交数据的画布、码率、轨道、片段、倒放类型、定格源帧及标记；错误不修改状态。
    /// 图片时长独立于默认源帧数；变速片段校验精确源区间，普通视频/音频不得超过可用源帧数。
    pub fn validate(&self) -> Result<(), String> {
        if !(1..=240).contains(&self.fps)
            || self.width == 0
            || self.height == 0
            || self.width > 16384
            || self.height > 16384
            || !(100..=1_000_000).contains(&self.bitrate_kbps)
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
        self.validate_markers()?;
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
                || (clip.kind == MediaKind::Image && clip.reversed)
                || clip.freeze_frame.is_some_and(|frame| {
                    clip.kind != MediaKind::Video || frame < 0 || frame >= clip.source_frames
                })
                || (clip.kind == MediaKind::Image && clip.duration > 86400 * self.fps as i64)
                || (clip.kind != MediaKind::Image
                    && clip.speed.is_none()
                    && clip.duration > clip.source_frames - clip.source_in)
            {
                return Err("片段身份或时间范围无效".into());
            }
            if let Some(speed) = &clip.speed {
                speed.validate(clip.source_frames)?;
                if clip.kind == MediaKind::Image
                    || (clip.kind == MediaKind::Audio && !speed.curve.is_empty())
                    || clip.duration > 86400 * self.fps as i64
                    || clip.source_in != speed.source[0].floor() as i64
                {
                    return Err("片段变速区间无效".into());
                }
            }
            if !self.tracks[clip.track].kind.accepts(clip.kind) {
                return Err("素材类型与轨道不匹配".into());
            }
            if !clip.gain.is_finite() || !(0.0..=10.0).contains(&clip.gain) {
                return Err("音量参数无效".into());
            }
            clip.validate_keyframes(self.fps)?;
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
    /// 对选中且未锁定的图片、视频设置同一变换参数；音频跳过，非法输入不更新任何片段。
    pub fn set_property(
        &mut self,
        ids: &BTreeSet<String>,
        property: Property,
        value: f64,
    ) -> Result<(), String> {
        Transform::default().set(property, value)?;
        for c in self.clips.iter_mut().filter(|c| {
            ids.contains(&c.id)
                && c.kind != MediaKind::Audio
                && !c.locked
                && !self.tracks[c.track].locked
        }) {
            c.transform.set(property, value)?;
        }
        Ok(())
    }
    /// 对选中且未锁定的音频片段设置线性增益；拒绝非有限数值及越界，图片、视频不受影响。
    /// 范围 0..=10 分别对应负无穷至 20dB；调用方在手势释放时保存并记录历史。
    pub fn set_volume(&mut self, ids: &BTreeSet<String>, gain: f64) -> Result<(), String> {
        if !gain.is_finite() || !(0.0..=10.0).contains(&gain) {
            return Err("音量参数无效".into());
        }
        for c in self.clips.iter_mut().filter(|c| {
            ids.contains(&c.id)
                && c.kind == MediaKind::Audio
                && !c.locked
                && !self.tracks[c.track].locked
        }) {
            c.gain = gain;
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
    /// 查询指定方向是否有可用快照，供宿主禁用空历史按钮；不消费或修改历史。
    pub fn can_step(&self, redo: bool) -> bool {
        if redo {
            !self.redo.is_empty()
        } else {
            !self.undo.is_empty()
        }
    }
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
/// 按实际像素密度生成整屏刻度，支持长项目全览时低于每秒一像素；主刻度至少相隔 90px。
/// 最大缩放每帧短线、每两帧标注，秒界显示时间码；非正密度以极小正值保护除法。
pub fn ticks(fps: u32, pixels_per_second: f64, first: f64, width: f64) -> Vec<Tick> {
    let fps = fps.max(1) as i64;
    let pixels_per_second = pixels_per_second.max(0.000001);
    let ppf = pixels_per_second / fps as f64;
    let mut steps = vec![1, 2, 5, 10, 15];
    steps.extend(
        [
            1, 2, 5, 10, 15, 30, 60, 120, 300, 600, 1800, 3600, 7200, 14400, 28800, 86400,
        ]
        .map(|s| s * fps),
    );
    steps.sort_unstable();
    steps.dedup();
    let major = steps
        .into_iter()
        .find(|s| *s as f64 * ppf >= 90.0)
        .unwrap_or(86400 * fps);
    let minor = if major < fps {
        1
    } else if major % 10 == 0 {
        major / 10
    } else {
        (major / 5).max(1)
    };
    let start = ((first.max(0.0) * fps as f64) as i64 / minor) * minor;
    let end = ((first.max(0.0) + width.max(0.0) / pixels_per_second) * fps as f64).ceil() as i64;
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
