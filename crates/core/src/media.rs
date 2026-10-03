//! 源文件探测和帧解码；只读取引用文件，FFmpeg 工作均由调用方放到后台执行。

use serde_json::Value;
use std::{
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::Arc,
};

pub const FRAME_WIDTH: u32 = 640;
pub const FRAME_HEIGHT: u32 = 360;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MediaKind {
    Video,
    Audio,
    Image,
}

#[derive(Clone)]
pub struct Media {
    pub path: PathBuf,
    pub name: String,
    pub kind: MediaKind,
    pub duration: f64,
    pub has_audio: bool,
    pub preview_size: (u32, u32),
    pub thumbnail: Option<Arc<Vec<u8>>>,
    pub offline: bool,
}

/// 构造无控制台的媒体子进程；优先使用程序旁的 FFmpeg，随后使用 PATH。
/// 参数通过 Command 逐项传递，文件名不会被 shell 解释；缺失程序由启动错误反馈。
pub fn command(program: &str) -> Command {
    let local = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.join(format!("{program}.exe"))));
    let mut command = Command::new(
        local
            .filter(|p| p.is_file())
            .unwrap_or_else(|| program.into()),
    );
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    command.stdin(Stdio::null());
    command
}

/// 规范化文件路径并读取真实流类型、时长和首帧；拒绝目录、未知类型及损坏文件。
/// MIDI 由采样合成器读取真实时长；静态图片默认五秒，不复制源素材或写缩略图文件。
pub fn probe(path: &Path) -> Result<Media, String> {
    let path =
        std::fs::canonicalize(path).map_err(|e| format!("素材不可用 {}：{e}", path.display()))?;
    if !path.is_file() {
        return Err("只能导入素材文件".into());
    }
    if crate::midi::is_midi(&path) {
        return Ok(Media {
            name: path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned(),
            duration: crate::midi::duration(&path)?,
            path,
            kind: MediaKind::Audio,
            has_audio: true,
            preview_size: (FRAME_WIDTH, FRAME_HEIGHT),
            thumbnail: None,
            offline: false,
        });
    }
    let output = command("ffprobe")
        .args([
            "-v",
            "error",
            "-show_streams",
            "-show_format",
            "-of",
            "json",
        ])
        .arg(&path)
        .output()
        .map_err(|e| format!("无法运行 ffprobe：{e}"))?;
    if !output.status.success() {
        return Err(format!(
            "无法读取素材 {}：{}",
            path.display(),
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    let data: Value =
        serde_json::from_slice(&output.stdout).map_err(|e| format!("素材信息无效：{e}"))?;
    let streams = data["streams"].as_array().ok_or("素材没有可读取的媒体流")?;
    let video = streams
        .iter()
        .any(|s| s["codec_type"] == "video" && s["disposition"]["attached_pic"] != 1);
    let has_audio = streams.iter().any(|s| s["codec_type"] == "audio");
    let extension = path
        .extension()
        .unwrap_or_default()
        .to_string_lossy()
        .to_ascii_lowercase();
    let image = matches!(
        extension.as_str(),
        "png" | "jpg" | "jpeg" | "bmp" | "webp" | "tif" | "tiff" | "avif" | "heic"
    );
    let kind = if image && video {
        MediaKind::Image
    } else if video {
        MediaKind::Video
    } else if has_audio {
        MediaKind::Audio
    } else {
        return Err("此文件没有可预览的视频、音频或图片".into());
    };
    let duration = if kind == MediaKind::Image {
        5.0
    } else {
        data["format"]["duration"]
            .as_str()
            .and_then(|s| s.parse::<f64>().ok())
            .or_else(|| {
                streams
                    .iter()
                    .filter_map(|s| s["duration"].as_str()?.parse::<f64>().ok())
                    .max_by(f64::total_cmp)
            })
            .filter(|d| d.is_finite() && *d > 0.0)
            .ok_or("无法确定素材时长")?
    };
    let thumbnail = if kind == MediaKind::Audio {
        None
    } else {
        Some(Arc::new(frame(&path, 0.0)?))
    };
    let preview_size = streams
        .iter()
        .find(|s| s["codec_type"] == "video")
        .map(preview_size)
        .unwrap_or((FRAME_WIDTH, FRAME_HEIGHT));
    Ok(Media {
        name: path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned(),
        path,
        kind,
        duration,
        has_audio,
        preview_size,
        thumbnail,
        offline: false,
    })
}

/// 为失去源文件的已有项目引用保留可筛选的素材项；不删除引用或擅自搜索其他路径。
/// 时长与缩略图未知时留空，文件回到原路径后由后台监测重新探测恢复。
pub fn offline(path: PathBuf) -> Media {
    let name = path
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned();
    let extension = path
        .extension()
        .unwrap_or_default()
        .to_string_lossy()
        .to_ascii_lowercase();
    let kind = if matches!(
        extension.as_str(),
        "mid" | "midi" | "mp3" | "wav" | "aac" | "m4a" | "flac" | "ogg" | "opus" | "wma" | "aiff"
    ) {
        MediaKind::Audio
    } else if matches!(
        extension.as_str(),
        "png" | "jpg" | "jpeg" | "bmp" | "webp" | "tif" | "tiff" | "avif" | "heic"
    ) {
        MediaKind::Image
    } else {
        MediaKind::Video
    };
    Media {
        path,
        name,
        kind,
        duration: 0.0,
        has_audio: false,
        preview_size: (FRAME_WIDTH, FRAME_HEIGHT),
        thumbnail: None,
        offline: true,
    }
}

/// 计算等比缩放后的有效画面尺寸，计入非方形像素和旋转；音频或无效尺寸采用固定帧尺寸。
/// 返回解码透明补边内的矩形大小，宿主可裁去补边再按任意预览面板比例完整适配。
fn preview_size(stream: &Value) -> (u32, u32) {
    let width = stream["width"].as_f64().unwrap_or(FRAME_WIDTH as f64);
    let height = stream["height"].as_f64().unwrap_or(FRAME_HEIGHT as f64);
    let sar = stream["sample_aspect_ratio"]
        .as_str()
        .and_then(|s| s.split_once(':'))
        .and_then(|(n, d)| Some(n.parse::<f64>().ok()? / d.parse::<f64>().ok()?))
        .filter(|s| s.is_finite() && *s > 0.0)
        .unwrap_or(1.0);
    let mut ratio = width * sar / height;
    let rotation = stream["side_data_list"]
        .as_array()
        .and_then(|items| items.iter().find_map(|s| s["rotation"].as_f64()))
        .or_else(|| stream["tags"]["rotate"].as_str()?.parse().ok())
        .unwrap_or(0.0);
    if rotation.abs().rem_euclid(180.0) == 90.0 {
        ratio = 1.0 / ratio;
    }
    if !ratio.is_finite() || ratio <= 0.0 {
        return (FRAME_WIDTH, FRAME_HEIGHT);
    }
    if ratio >= FRAME_WIDTH as f64 / FRAME_HEIGHT as f64 {
        (
            FRAME_WIDTH,
            (FRAME_WIDTH as f64 / ratio)
                .round()
                .clamp(1.0, FRAME_HEIGHT as f64) as u32,
        )
    } else {
        (
            (FRAME_HEIGHT as f64 * ratio)
                .round()
                .clamp(1.0, FRAME_WIDTH as f64) as u32,
            FRAME_HEIGHT,
        )
    }
}

/// 设置按显示比例缩放的 BGRA 输出；将像素比例归一，补边透明以显示宿主面板留白。
/// 固定输出尺寸限制内存；只替换解码产生的补边，不去除源画面本身的黑色内容。
pub fn video_arguments(command: &mut Command) {
    command.args([
        "-map",
        "0:v:0",
        "-an",
        "-vf",
        "scale=640:360:force_original_aspect_ratio=decrease:reset_sar=1,format=bgra,pad=640:360:(ow-iw)/2:(oh-ih)/2:color=black@0",
        "-pix_fmt",
        "bgra",
        "-f",
        "rawvideo",
    ]);
}

/// 解码指定秒数的一帧；FFmpeg 退出失败或输出不足时返回可展示的错误，不留下子进程。
pub fn frame(path: &Path, seconds: f64) -> Result<Vec<u8>, String> {
    let mut decoder = command("ffmpeg");
    decoder
        .args(["-v", "error", "-ss", &format!("{seconds:.6}"), "-i"])
        .arg(path);
    video_arguments(&mut decoder);
    let output = decoder
        .args(["-frames:v", "1", "pipe:1"])
        .output()
        .map_err(|e| format!("无法解码预览：{e}"))?;
    if !output.status.success() || output.stdout.len() != (FRAME_WIDTH * FRAME_HEIGHT * 4) as usize
    {
        return Err(format!(
            "无法解码画面：{}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    Ok(output.stdout)
}

/// 格式化真实媒体时间到百分之一秒；非法或负数归零，支持超过一小时的素材。
pub fn timecode(seconds: f64) -> String {
    let ticks = if seconds.is_finite() {
        (seconds.max(0.0) * 100.0).round() as u64
    } else {
        0
    };
    format!(
        "{:02}:{:02}:{:02}.{:02}",
        ticks / 360000,
        ticks / 6000 % 60,
        ticks / 100 % 60,
        ticks % 100
    )
}

/// 检查源文件是否仍存在；保留离线路径和已知信息，原路径恢复时重新探测。
/// 只读取给定引用，不扫描其他目录；探测失败时维持离线状态，调用方可在后台执行。
pub fn check_sources(items: Vec<Media>) -> Vec<Media> {
    items
        .into_iter()
        .map(|mut item| {
            if !item.path.is_file() {
                item.offline = true;
            } else if item.offline
                && let Ok(restored) = probe(&item.path)
            {
                let name = item.name.clone();
                item = restored;
                item.name = name;
            }
            item
        })
        .collect()
}
