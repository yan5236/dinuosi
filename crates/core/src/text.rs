//! 生成文字和手动字幕：正文与样式独立，分组不随轨道重排或重叠分轨而改变。
use crate::{
    media::MediaKind,
    timeline::{Clip, Timeline, TrackKind, Transform},
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Alignment {
    Left,
    #[default]
    Center,
    Right,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TextStyle {
    /// 空名称使用系统无衬线字体；缺字由共享排版器选择回退字体。
    pub font: String,
    pub size: f64,
    pub width: f64,
    pub line_height: f64,
    pub bold: bool,
    pub italic: bool,
    pub color: u32,
    pub alignment: Alignment,
    /// 描边的项目像素半径；零表示不描边，颜色由 outline_color 指定。
    pub outline: u32,
    #[serde(default)]
    pub outline_color: u32,
    #[serde(default)]
    pub shadow: Option<Shadow>,
    #[serde(default)]
    pub background: Option<Background>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Shadow {
    pub color: u32,
    pub opacity: f64,
    pub blur: u32,
    pub offset: [i32; 2],
}

impl Default for Shadow {
    /// 默认柔和黑色投影，偏移和模糊均以项目像素计，不依赖预览缩放。
    fn default() -> Self {
        Self {
            color: 0,
            opacity: 0.65,
            blur: 4,
            offset: [4, 4],
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Background {
    pub color: u32,
    pub opacity: f64,
    pub padding: u32,
    pub radius: u32,
}

impl Default for Background {
    /// 默认半透明黑底和小圆角，按可见文字范围扩展内边距，避免整条文字框都变为色块。
    fn default() -> Self {
        Self {
            color: 0,
            opacity: 0.6,
            padding: 12,
            radius: 8,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Text {
    pub body: String,
    pub style: TextStyle,
    /// None 为普通文本；字幕使用稳定 UUID，明确“本组字幕”的样式作用范围。
    pub group: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TextPreset {
    pub id: String,
    pub name: String,
    pub style: TextStyle,
}

impl TextPreset {
    /// 校验用户库的稳定身份、名称与完整排版效果，拒绝控制字符和空名称，不允许路径式身份。
    pub fn validate(&self) -> Result<(), String> {
        if Uuid::parse_str(&self.id).is_err()
            || self.name.trim().is_empty()
            || self.name.chars().count() > 60
            || self.name.chars().any(char::is_control)
        {
            return Err("文本样式名称或身份无效".into());
        }
        self.style.validate()
    }
}

impl Text {
    /// 创建与画布比例匹配的白色普通文本或字幕，描边默认关闭；分组身份与片段身份独立。
    /// 返回纯领域数据，不创建轨道、文件或宿主状态，正文由调用方按语言提供。
    pub fn new(body: String, canvas: (u32, u32), subtitle: bool) -> Self {
        Self {
            body,
            group: subtitle.then(|| Uuid::new_v4().to_string()),
            style: TextStyle {
                font: String::new(),
                size: (canvas.1 as f64 * if subtitle { 0.045 } else { 0.07 }).clamp(8.0, 300.0),
                width: (canvas.0 as f64 * 0.8).max(16.0),
                line_height: 1.3,
                bold: false,
                italic: false,
                color: 0xffffff,
                alignment: Alignment::Center,
                outline: 0,
                outline_color: 0,
                shadow: None,
                background: None,
            },
        }
    }

    /// 校验外部正文、字体和字幕身份；允许编辑中暂时为空，拒绝控制字符和超过 4096 字符的正文。
    /// 样式使用项目像素，非法数字与过大宽度拒绝载入，不进行静默截断。
    pub fn validate(&self) -> Result<(), String> {
        if self.body.chars().count() > 4096
            || self
                .body
                .chars()
                .any(|c| c.is_control() && c != '\n' && c != '\t')
            || self
                .group
                .as_ref()
                .is_some_and(|g| Uuid::parse_str(g).is_err())
        {
            return Err("文本正文或字幕分组无效".into());
        }
        self.style.validate()
    }

    /// 从正文取首个非空行作为片段名称，最多 40 个字符；正文为空时保持可识别的文字标签。
    pub fn label(&self) -> String {
        self.body
            .lines()
            .find(|l| !l.trim().is_empty())
            .unwrap_or("Text")
            .chars()
            .take(40)
            .collect()
    }
}

impl TextStyle {
    /// 检查排版数值的有限性及范围，字体仅允许短名称，RGB 不携带隐藏的透明通道。
    pub fn validate(&self) -> Result<(), String> {
        if self.font.chars().count() > 128
            || self.font.chars().any(char::is_control)
            || !self.size.is_finite()
            || !(8.0..=300.0).contains(&self.size)
            || !self.width.is_finite()
            || !(16.0..=16384.0).contains(&self.width)
            || !self.line_height.is_finite()
            || !(0.8..=3.0).contains(&self.line_height)
            || self.color > 0xffffff
            || self.outline > 8
            || self.outline_color > 0xffffff
            || self.shadow.as_ref().is_some_and(|s| {
                s.color > 0xffffff
                    || !s.opacity.is_finite()
                    || !(0.0..=1.0).contains(&s.opacity)
                    || s.blur > 24
                    || s.offset.iter().any(|v| !(-100..=100).contains(v))
            })
            || self.background.as_ref().is_some_and(|b| {
                b.color > 0xffffff
                    || !b.opacity.is_finite()
                    || !(0.0..=1.0).contains(&b.opacity)
                    || b.padding > 100
                    || b.radius > 100
            })
        {
            return Err("文本样式参数无效".into());
        }
        Ok(())
    }
}

impl Clip {
    /// 返回片段实际职责；字幕分组独立于素材类型，用于移动和冲突分轨时保留字幕轨道。
    pub fn track_kind(&self) -> TrackKind {
        if self.text.as_ref().is_some_and(|t| t.group.is_some()) {
            TrackKind::Subtitle
        } else {
            TrackKind::for_media(self.kind)
        }
    }

    /// 校验生成文字与片段类型、轨道职责的对应，禁止伪造文件源、变速或音频参数。
    /// 普通文件媒体不得夹带文本数据；旧项目没有此字段时继续按文件媒体处理。
    pub(crate) fn validate_text(&self, track: TrackKind) -> Result<(), String> {
        match (&self.text, self.kind) {
            (Some(t), MediaKind::Text)
                if self.source.as_os_str().is_empty()
                    && self.source_in == 0
                    && self.source_frames == 1
                    && self.speed.is_none()
                    && !self.reversed
                    && self.freeze_frame.is_none()
                    && track == self.track_kind() =>
            {
                t.validate()
            }
            (None, kind) if kind != MediaKind::Text => Ok(()),
            _ => Err("文本片段与轨道类型不匹配".into()),
        }
    }
}

impl Timeline {
    /// 在指定帧创建文字并返回片段 UUID；复用无重叠的同类轨道，必要时在最上方建轨。
    /// 新字幕靠近画布下方，样式由调用方提供；不会覆盖、顺延已有片段，非法输入不修改模型。
    pub fn add_text(&mut self, start: i64, duration: i64, text: Text) -> Result<String, String> {
        text.validate()?;
        if start < 0
            || start > 86400 * self.fps as i64
            || duration < 1
            || duration > 86400 * self.fps as i64
            || start
                .checked_add(duration)
                .is_none_or(|end| end > 86400 * self.fps as i64)
        {
            return Err("文本时间范围无效".into());
        }
        let kind = if text.group.is_some() {
            TrackKind::Subtitle
        } else {
            TrackKind::Text
        };
        let track = match self.tracks.iter().enumerate().find(|(index, t)| {
            t.kind == kind
                && !t.locked
                && self.clips.iter().all(|c| {
                    c.track != *index
                        || c.start >= start + duration
                        || c.start + c.duration <= start
                })
        }) {
            Some((index, _)) => index,
            None => self.create_track(0, kind)?,
        };
        let id = Uuid::new_v4().to_string();
        let transform = Transform {
            y: if text.group.is_some() {
                self.height as f64 * 0.37
            } else {
                0.0
            },
            ..Default::default()
        };
        self.clips.push(Clip {
            id: id.clone(),
            link: None,
            locked: false,
            source: Default::default(),
            name: text.label(),
            track,
            start,
            duration,
            source_in: 0,
            source_frames: 1,
            reversed: false,
            freeze_frame: None,
            speed: None,
            transform,
            color: Default::default(),
            text: Some(text),
            kind: MediaKind::Text,
            gain: 1.0,
            keyframes: Vec::new(),
        });
        Ok(id)
    }

    /// 修改指定文字的正文并同步片段标签；仅当前句生效，锁定或非法正文时保持原值。
    pub fn set_text_body(&mut self, id: &str, body: String) -> Result<(), String> {
        let clip = self
            .clips
            .iter_mut()
            .find(|c| c.id == id)
            .ok_or("片段不存在")?;
        if clip.locked || self.tracks[clip.track].locked {
            return Err("文本已锁定".into());
        }
        let mut text = clip.text.clone().ok_or("所选片段不是文本")?;
        text.body = body;
        text.validate()?;
        clip.name = text.label();
        clip.text = Some(text);
        Ok(())
    }

    /// 应用样式到当前文字或明确的同组字幕；正文、时序和变换不传播。
    /// 本组任一句或轨道锁定则整次拒绝，避免“应用全部”产生不完整结果；失败不修改任何句子。
    /// 直接按身份和分组线性筛选，不为每句重复扫描完整身份表，避免大字幕组的平方级遍历。
    pub fn set_text_style(
        &mut self,
        id: &str,
        style: TextStyle,
        group: bool,
    ) -> Result<(), String> {
        style.validate()?;
        let text = self
            .clips
            .iter()
            .find(|c| c.id == id)
            .and_then(|c| c.text.as_ref())
            .ok_or("所选片段不是文本")?;
        let group_id = if group { text.group.clone() } else { None };
        let selected = |c: &Clip| {
            c.id == id
                || group_id
                    .as_ref()
                    .is_some_and(|g| c.text.as_ref().and_then(|t| t.group.as_ref()) == Some(g))
        };
        if self
            .clips
            .iter()
            .filter(|c| selected(c))
            .any(|c| c.locked || self.tracks[c.track].locked)
        {
            return Err("文本已锁定".into());
        }
        for c in self.clips.iter_mut().filter(|c| selected(c)) {
            c.text.as_mut().unwrap().style = style.clone();
        }
        Ok(())
    }

    /// 精确设置文字起止帧，保留局部动画；重叠按同类轨道分流，其他句子的时间保持原样。
    /// 新模型通过完整校验后才发布，越界、锁定或反向区间返回错误并保持原状态。
    pub fn set_text_range(&mut self, id: &str, start: i64, end: i64) -> Result<(), String> {
        if start < 0 || end <= start || end > 86400 * self.fps as i64 {
            return Err("文本时间范围无效".into());
        }
        let mut next = self.clone();
        let c = next
            .clips
            .iter_mut()
            .find(|c| c.id == id)
            .ok_or("片段不存在")?;
        if c.text.is_none() || c.locked || next.tracks[c.track].locked {
            return Err("文本不可编辑".into());
        }
        c.start = start;
        c.duration = end - start;
        next.resolve_overlap(id);
        next.validate()?;
        *self = next;
        Ok(())
    }
}
