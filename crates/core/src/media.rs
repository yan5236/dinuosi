//! 源文件探测和帧解码；只读取引用文件，FFmpeg 工作均由调用方放到后台执行。

use serde_json::Value;
use std::{
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::Arc,
};

pub const FRAME_WIDTH: u32 = 640;
pub const FRAME_HEIGHT: u32 = 360;
const VIDEO_FILTER: &str = "scale=640:360:force_original_aspect_ratio=decrease:reset_sar=1,format=bgra,pad=640:360:(ow-iw)/2:(oh-ih)/2:color=black@0";

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
    pub source_size: (u32, u32),
    pub thumbnail: Option<Arc<Vec<u8>>>,
    pub offline: bool,
}

/// 构造应用配套目录中的媒体子进程，不读取 PATH，也不依赖启动时的工作目录。
/// 按本机可执行文件后缀定位工具，参数逐项传递且不经 shell；缺失工具或配套库由调用方报告。
/// 仅当操作系统无法定位本进程时终止，避免猜测路径而运行未经选择的工具。
pub fn command(program: &str) -> Command {
    let executable = std::env::current_exe().expect("无法定位应用程序路径");
    let root = executable.parent().expect("应用程序路径没有父目录");
    let mut command = Command::new(
        root.join("runtime")
            .join("ffmpeg")
            .join(format!("{program}{}", std::env::consts::EXE_SUFFIX)),
    );
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    // 固定工作目录；Unix 运行库通过相对 rpath 部署，不继承用户注入的动态库搜索路径。
    let runtime = root.join("runtime/ffmpeg");
    command.current_dir(runtime);
    if let Some(windows) = std::env::var_os("SystemRoot") {
        let windows = PathBuf::from(windows);
        let path = std::env::join_paths([windows.join("System32"), windows]).expect("系统路径无效");
        command.env("PATH", path);
    } else {
        command.env("PATH", "");
    }
    command.stdin(Stdio::null());
    #[cfg(unix)]
    for variable in [
        "LD_LIBRARY_PATH",
        "LD_PRELOAD",
        "DYLD_LIBRARY_PATH",
        "DYLD_FALLBACK_LIBRARY_PATH",
        "DYLD_INSERT_LIBRARIES",
    ] {
        command.env_remove(variable);
    }
    command
}

/// 返回媒体工具的诊断；没有 stderr 时保留退出状态，便于识别缺失 DLL 等启动失败。
/// 只读取进程结果，不修改源素材，成功结果也可用于诊断但由调用方决定是否报错。
fn process_error(output: &std::process::Output) -> String {
    let message = String::from_utf8_lossy(&output.stderr);
    if message.trim().is_empty() {
        format!("媒体工具退出异常：{}", output.status)
    } else {
        message.trim().to_owned()
    }
}
/// 规范化文件路径并读取真实流类型、时长和首帧；拒绝目录、未知类型及损坏文件。
/// MIDI 由采样合成器读取真实时长；图片默认五秒，保留计入旋转和像素比例的源尺寸。
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
            source_size: (FRAME_WIDTH, FRAME_HEIGHT),
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
            process_error(&output)
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
        source_size: streams
            .iter()
            .find(|s| s["codec_type"] == "video")
            .map(source_size)
            .unwrap_or((FRAME_WIDTH, FRAME_HEIGHT)),
        thumbnail,
        offline: false,
    })
}

/// 为失去源文件的已有项目引用保留可筛选的素材项；不删除引用或擅自搜索其他路径。
/// 时长与缩略图未知时留空，尺寸使用占位值，文件恢复后由后台重新探测。
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
        source_size: (FRAME_WIDTH, FRAME_HEIGHT),
        thumbnail: None,
        offline: true,
    }
}

/// 返回计入非方形像素和旋转的源显示尺寸；异常元数据回退为固定解码尺寸。
fn source_size(stream: &Value) -> (u32, u32) {
    let width = stream["width"].as_f64().unwrap_or(FRAME_WIDTH as f64);
    let height = stream["height"].as_f64().unwrap_or(FRAME_HEIGHT as f64);
    let sar = stream["sample_aspect_ratio"]
        .as_str()
        .and_then(|s| s.split_once(':'))
        .and_then(|(n, d)| Some(n.parse::<f64>().ok()? / d.parse::<f64>().ok()?))
        .filter(|s| s.is_finite() && *s > 0.0)
        .unwrap_or(1.0);
    let mut dimensions = (width * sar, height);
    let rotation = stream["side_data_list"]
        .as_array()
        .and_then(|items| items.iter().find_map(|s| s["rotation"].as_f64()))
        .or_else(|| stream["tags"]["rotate"].as_str()?.parse().ok())
        .unwrap_or(0.0);
    if rotation.abs().rem_euclid(180.0) == 90.0 {
        dimensions = (dimensions.1, dimensions.0);
    }
    if !dimensions.0.is_finite()
        || !dimensions.1.is_finite()
        || dimensions.0 <= 0.0
        || dimensions.1 <= 0.0
    {
        return (FRAME_WIDTH, FRAME_HEIGHT);
    }
    (
        dimensions.0.round().max(1.0) as u32,
        dimensions.1.round().max(1.0) as u32,
    )
}

/// 将源显示尺寸等比适配至固定解码画布，返回透明补边中的有效矩形。
fn preview_size(stream: &Value) -> (u32, u32) {
    preview_dimensions(source_size(stream))
}

/// 将任意合法宽高比适配至 640×360 内，供项目预览裁切和源缩略图共用。
pub fn preview_dimensions(dimensions: (u32, u32)) -> (u32, u32) {
    let ratio = dimensions.0.max(1) as f64 / dimensions.1.max(1) as f64;
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
        VIDEO_FILTER,
        "-pix_fmt",
        "bgra",
        "-f",
        "rawvideo",
    ]);
}

/// 解码指定秒数的一帧；源帧率与项目不同导致尾端无输出时，倒序解码最后一秒取得真实尾帧。
/// 只对成功退出但无帧的寻址补取尾帧；退出失败或像素不足仍返回实际错误，不留下子进程。
pub fn frame(path: &Path, seconds: f64) -> Result<Vec<u8>, String> {
    let mut decoder = command("ffmpeg");
    decoder
        .args(["-v", "error", "-ss", &format!("{seconds:.6}"), "-i"])
        .arg(path);
    video_arguments(&mut decoder);
    let mut output = decoder
        .args(["-frames:v", "1", "pipe:1"])
        .output()
        .map_err(|e| format!("无法解码预览：{e}"))?;
    if output.status.success() && output.stdout.is_empty() {
        let mut tail = command("ffmpeg");
        tail.args(["-v", "error", "-sseof", "-1", "-i"]).arg(path);
        video_arguments(&mut tail);
        output = tail
            .args([
                "-vf",
                &format!("{VIDEO_FILTER},reverse"),
                "-frames:v",
                "1",
                "pipe:1",
            ])
            .output()
            .map_err(|e| format!("无法解码预览：{e}"))?;
    }
    if !output.status.success() || output.stdout.len() != (FRAME_WIDTH * FRAME_HEIGHT * 4) as usize
    {
        return Err(format!("无法解码画面：{}", process_error(&output)));
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
