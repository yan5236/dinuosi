//! 文字和艺术字字段协议，输入统一经过核心范围校验。
use crate::view::Desktop;
use dinuosi_core::{text::TextStyle, timeline::Clip};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum TextField {
    Font,
    Size,
    Width,
    LineHeight,
    Color,
    Outline,
    OutlineColor,
    Start,
    End,
    ShadowColor,
    ShadowOpacity,
    ShadowBlur,
    ShadowX,
    ShadowY,
    BackgroundColor,
    BackgroundOpacity,
    BackgroundPadding,
    BackgroundRadius,
}

impl TextField {
    /// 返回稳定字段 ID，供输入实体与实际窗口定位复用。
    pub(crate) fn id(self) -> &'static str {
        match self {
            Self::Font => "text-font",
            Self::Size => "text-size",
            Self::Width => "text-width",
            Self::LineHeight => "text-line-height",
            Self::Color => "text-color",
            Self::Outline => "text-outline",
            Self::OutlineColor => "text-outline-color",
            Self::Start => "text-start",
            Self::End => "text-end",
            Self::ShadowColor => "text-shadow-color",
            Self::ShadowOpacity => "text-shadow-opacity",
            Self::ShadowBlur => "text-shadow-blur",
            Self::ShadowX => "text-shadow-x",
            Self::ShadowY => "text-shadow-y",
            Self::BackgroundColor => "text-background-color",
            Self::BackgroundOpacity => "text-background-opacity",
            Self::BackgroundPadding => "text-background-padding",
            Self::BackgroundRadius => "text-background-radius",
        }
    }
    /// 颜色字段返回对应 RGB，非颜色或已关闭的效果返回 None。
    pub(crate) fn color(self, s: &TextStyle) -> Option<u32> {
        match self {
            Self::Color => Some(s.color),
            Self::OutlineColor => Some(s.outline_color),
            Self::ShadowColor => s.shadow.as_ref().map(|s| s.color),
            Self::BackgroundColor => s.background.as_ref().map(|b| b.color),
            _ => None,
        }
    }
    /// 以项目像素、百分比透明度或精确帧时间码显示输入；效果字段只在启用时绘制。
    pub(crate) fn value(self, c: &Clip, fps: u32) -> String {
        let s = &c.text.as_ref().unwrap().style;
        if let Some(color) = self.color(s) {
            return format!("#{color:06X}");
        }
        match self {
            Self::Font => s.font.clone(),
            Self::Size => format!("{:.1}", s.size),
            Self::Width => format!("{:.0}", s.width),
            Self::LineHeight => format!("{:.2}", s.line_height),
            Self::Outline => s.outline.to_string(),
            Self::Start | Self::End => crate::timeline_markers::timecode(
                if self == Self::Start {
                    c.start
                } else {
                    c.start + c.duration
                },
                fps,
            ),
            Self::ShadowOpacity => format!("{:.0}", s.shadow.as_ref().unwrap().opacity * 100.0),
            Self::ShadowBlur => s.shadow.as_ref().unwrap().blur.to_string(),
            Self::ShadowX => s.shadow.as_ref().unwrap().offset[0].to_string(),
            Self::ShadowY => s.shadow.as_ref().unwrap().offset[1].to_string(),
            Self::BackgroundOpacity => {
                format!("{:.0}", s.background.as_ref().unwrap().opacity * 100.0)
            }
            Self::BackgroundPadding => s.background.as_ref().unwrap().padding.to_string(),
            Self::BackgroundRadius => s.background.as_ref().unwrap().radius.to_string(),
            _ => String::new(),
        }
    }
    /// 修改单个样式值，RGB 必须六位十六进制；所有有限性、整数与范围由领域再次检查。
    fn set(self, s: &mut TextStyle, value: &str) -> Result<(), String> {
        let value = value.trim();
        let number = || value.parse::<f64>().map_err(|_| "请输入有效文本数值");
        let integer = || value.parse::<u32>().map_err(|_| "请输入有效文本整数");
        let color = || {
            let v = value.strip_prefix('#').unwrap_or(value);
            if v.len() != 6 {
                return Err("请输入六位 RGB 颜色");
            }
            u32::from_str_radix(v, 16).map_err(|_| "请输入六位 RGB 颜色")
        };
        match self {
            Self::Font => s.font = value.into(),
            Self::Size => s.size = number()?,
            Self::Width => s.width = number()?,
            Self::LineHeight => s.line_height = number()?,
            Self::Color => s.color = color()?,
            Self::Outline => s.outline = integer()?,
            Self::OutlineColor => s.outline_color = color()?,
            Self::ShadowColor
            | Self::ShadowOpacity
            | Self::ShadowBlur
            | Self::ShadowX
            | Self::ShadowY => {
                let t = s.shadow.as_mut().ok_or("阴影已关闭")?;
                match self {
                    Self::ShadowColor => t.color = color()?,
                    Self::ShadowOpacity => t.opacity = number()? / 100.0,
                    Self::ShadowBlur => t.blur = integer()?,
                    _ => {
                        t.offset[usize::from(self == Self::ShadowY)] =
                            value.parse().map_err(|_| "请输入有效阴影偏移")?
                    }
                }
            }
            Self::BackgroundColor
            | Self::BackgroundOpacity
            | Self::BackgroundPadding
            | Self::BackgroundRadius => {
                let b = s.background.as_mut().ok_or("背景已关闭")?;
                match self {
                    Self::BackgroundColor => b.color = color()?,
                    Self::BackgroundOpacity => b.opacity = number()? / 100.0,
                    Self::BackgroundPadding => b.padding = integer()?,
                    _ => b.radius = integer()?,
                }
            }
            _ => return Err("时间字段不能作为排版应用".into()),
        }
        s.validate()
    }
}

impl Desktop {
    /// 提交绑定片段的时间码或排版参数，不接受过期身份、非法数字、无效颜色或反向时间范围。
    pub(crate) fn apply_text_field(
        &mut self,
        id: &str,
        field: TextField,
        value: &str,
    ) -> Result<(), String> {
        let c = self
            .timeline
            .model
            .clips
            .iter()
            .find(|c| c.id == id)
            .ok_or("片段不存在")?;
        if matches!(field, TextField::Start | TextField::End) {
            let f = parse_timecode(value, self.timeline.model.fps)?;
            let (a, b) = if field == TextField::Start {
                (f, c.start + c.duration)
            } else {
                (c.start, f)
            };
            return self.timeline.model.set_text_range(id, a, b);
        }
        let mut style = c.text.as_ref().ok_or("所选片段不是文本")?.style.clone();
        field.set(&mut style, value)?;
        self.timeline
            .model
            .set_text_style(id, style, self.timeline.text.apply_group)
    }
}

/// 解析 HH:MM:SS:FF，先检查全部字段再整数换算，拒绝超过一天及超长数字。
pub(crate) fn parse_timecode(value: &str, fps: u32) -> Result<i64, String> {
    let p = value
        .trim()
        .split(':')
        .map(str::parse::<i64>)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| "请输入 HH:MM:SS:FF 时间码")?;
    if p.len() != 4
        || p.iter().any(|v| *v < 0)
        || p[0] > 24
        || p[1] > 59
        || p[2] > 59
        || p[3] >= fps as i64
    {
        return Err("文本时间码无效".into());
    }
    let f = ((p[0] * 60 + p[1]) * 60 + p[2]) * fps as i64 + p[3];
    if f > 86400 * fps as i64 {
        return Err("文本时间码超出范围".into());
    }
    Ok(f)
}
