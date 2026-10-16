//! UTF-8 SRT 字幕交换；文件时间转换到整数项目帧，不将格式化标签画到画面上。
use crate::{text::Text, timeline::Timeline};

pub struct Cue {
    pub start_ms: i64,
    pub end_ms: i64,
    pub body: String,
}

/// 解析有界 UTF-8 SRT，允许 BOM、CRLF 和不连续编号；完整校验后才返回，不产生部分字幕。
/// 支持常见格式标签转纯正文；拒绝损坏时间、空句、超长正文、超过一天和超过一万条的输入。
pub fn parse_srt(source: &str) -> Result<Vec<Cue>, String> {
    if source.len() > 4 * 1024 * 1024 {
        return Err("字幕文件超过 4MiB".into());
    }
    let source = source
        .trim_start_matches('\u{feff}')
        .replace("\r\n", "\n")
        .replace('\r', "\n");
    let mut cues = Vec::new();
    let mut lines = source.lines().peekable();
    while lines.peek().is_some() {
        while lines.peek().is_some_and(|s| s.trim().is_empty()) {
            lines.next();
        }
        let Some(number) = lines.next() else {
            break;
        };
        if number.trim().parse::<u32>().is_err() {
            return Err(format!("第 {} 条字幕编号无效", cues.len() + 1));
        }
        let timing = lines.next().ok_or("字幕缺少起止时间")?;
        let (start, end) = timing.split_once("-->").ok_or("字幕时间格式无效")?;
        let (start_ms, end_ms) = (parse_timestamp(start.trim())?, parse_timestamp(end.trim())?);
        if end_ms <= start_ms {
            return Err("字幕结束时间必须晚于开始时间".into());
        }
        let mut body = Vec::new();
        while lines.peek().is_some_and(|s| !s.trim().is_empty()) {
            body.push(lines.next().unwrap());
        }
        let body = plain_body(&body.join("\n"));
        if body.trim().is_empty() || body.chars().count() > 4096 {
            return Err("字幕正文为空或超过 4096 字符".into());
        }
        Text::new(body.clone(), (1920, 1080), false).validate()?;
        cues.push(Cue {
            start_ms,
            end_ms,
            body,
        });
        if cues.len() > 10000 {
            return Err("字幕最多导入 10000 条".into());
        }
    }
    if cues.is_empty() {
        return Err("字幕文件没有有效字幕".into());
    }
    Ok(cues)
}

/// 严格解析 HH:MM:SS,mmm，分秒和毫秒字段必须完整；范围夹在一天内，所有算术先验证边界。
fn parse_timestamp(value: &str) -> Result<i64, String> {
    let (clock, millis) = value.split_once(',').ok_or("SRT 时间必须为 HH:MM:SS,mmm")?;
    let parts: Vec<_> = clock.split(':').collect();
    if parts.len() != 3
        || parts[0].len() < 2
        || parts[1].len() != 2
        || parts[2].len() != 2
        || millis.len() != 3
        || !parts.iter().all(|p| p.bytes().all(|c| c.is_ascii_digit()))
        || !millis.bytes().all(|c| c.is_ascii_digit())
    {
        return Err("SRT 时间格式无效".into());
    }
    let parse = |s: &str| s.parse::<i64>().map_err(|_| "SRT 时间数值无效");
    let (h, m, s, ms) = (
        parse(parts[0])?,
        parse(parts[1])?,
        parse(parts[2])?,
        parse(millis)?,
    );
    if h > 24 || m > 59 || s > 59 || (h == 24 && (m != 0 || s != 0 || ms != 0)) {
        return Err("SRT 时间超出范围".into());
    }
    Ok(((h * 60 + m) * 60 + s) * 1000 + ms)
}

/// 移除 SRT 常见 b/i/u/font 标签并还原基础实体；未识别标签保留为正文，不任意删除用户字符。
fn plain_body(value: &str) -> String {
    let mut output = String::new();
    let mut rest = value;
    while let Some(index) = rest.find('<') {
        output.push_str(&rest[..index]);
        rest = &rest[index..];
        if let Some(end) = rest.find('>') {
            let tag = rest[1..end].trim().to_ascii_lowercase();
            if matches!(
                tag.as_str(),
                "b" | "/b" | "i" | "/i" | "u" | "/u" | "font" | "/font"
            ) || tag.starts_with("font ")
            {
                rest = &rest[end + 1..];
                continue;
            }
        }
        output.push('<');
        rest = &rest[1..];
    }
    output.push_str(rest);
    output
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&nbsp;", " ")
        .replace("&amp;", "&")
}

impl Timeline {
    /// 原子导入 SRT 为一个新字幕组，保留逐句时间；重叠句自动分轨，所有已有内容保持原样。
    /// 毫秒四舍五入到最近项目帧，极短句至少一帧；失败不创建轨道，返回所有新片段身份。
    pub fn import_srt(&mut self, source: &str) -> Result<Vec<String>, String> {
        let cues = parse_srt(source)?;
        let mut next = self.clone();
        let mut ids = Vec::new();
        let template = Text::new(String::new(), (self.width, self.height), true);
        for cue in cues {
            let start = (cue.start_ms * self.fps as i64 + 500) / 1000;
            let end = ((cue.end_ms * self.fps as i64 + 500) / 1000).max(start + 1);
            if end > 86400 * self.fps as i64 {
                return Err("字幕超出项目时间范围".into());
            }
            let mut text = template.clone();
            text.body = cue.body;
            ids.push(next.add_text(start, end - start, text)?);
        }
        next.validate()?;
        *self = next;
        Ok(ids)
    }

    /// 导出指定字幕组的纯正文 UTF-8 SRT，按起点/轨道排序并重新编号；不包含艺术字样式。
    /// 帧时间取最近毫秒，空组或空正文返回错误，不静默遗漏不可交换的字幕。
    pub fn export_srt(&self, group: &str) -> Result<String, String> {
        self.validate()?;
        let mut clips: Vec<_> = self
            .clips
            .iter()
            .filter(|c| c.text.as_ref().and_then(|t| t.group.as_deref()) == Some(group))
            .collect();
        if clips.is_empty() {
            return Err("没有可导出的字幕组".into());
        }
        clips.sort_by_key(|c| (c.start, c.track));
        let timestamp = |frame: i64| {
            let ms = (frame * 1000 + self.fps as i64 / 2) / self.fps as i64;
            format!(
                "{:02}:{:02}:{:02},{:03}",
                ms / 3_600_000,
                ms / 60_000 % 60,
                ms / 1000 % 60,
                ms % 1000
            )
        };
        let mut result = String::new();
        for (i, c) in clips.iter().enumerate() {
            let body = &c.text.as_ref().unwrap().body;
            if body.trim().is_empty() {
                return Err(format!("第 {} 条字幕正文为空", i + 1));
            }
            if body.lines().any(|line| line.trim().is_empty()) {
                return Err(format!("第 {} 条字幕包含 SRT 无法表示的空白行", i + 1));
            }
            let body = body
                .replace('&', "&amp;")
                .replace('<', "&lt;")
                .replace('>', "&gt;");
            result.push_str(&format!(
                "{}\n{} --> {}\n{}\n\n",
                i + 1,
                timestamp(c.start),
                timestamp(c.start + c.duration),
                body
            ));
        }
        Ok(result)
    }
}
