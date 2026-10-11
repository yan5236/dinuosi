//! 源文件探测和帧解码；只读取引用文件，FFmpeg 工作均由调用方放到后台执行。

use std::{
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::Arc,
};

pub const FRAME_WIDTH: u32 = 640;
pub const FRAME_HEIGHT: u32 = 360;
pub(crate) const VIDEO_FILTER: &str = "scale=640:360:force_original_aspect_ratio=decrease:reset_sar=1,format=bgra,pad=640:360:(ow-iw)/2:(oh-ih)/2:color=black@0";

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

/// 测试夹具和独立输出对照使用的工具命令；应用媒体接口均调用动态库。
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
    let info = dinuosi_media::probe(&path)?;
    let video = info.video;
    let has_audio = info.audio;
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
        (info.duration.is_finite() && info.duration > 0.0)
            .then_some(info.duration)
            .ok_or("无法确定素材时长")?
    };
    let thumbnail = if kind == MediaKind::Audio {
        None
    } else {
        Some(Arc::new(frame(&path, 0.0)?))
    };
    let size = if video {
        info.size
    } else {
        (FRAME_WIDTH, FRAME_HEIGHT)
    };
    let preview_size = preview_dimensions(size);
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
        source_size: size,
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

/// 通过原生 libav 解码指定秒数的一帧，透明补边和显示比例沿用既有滤镜。
/// 寻址到源尾端没有新帧时顺序读取最后一秒并保留真实尾帧；损坏源仍返回错误，不创建进程。
pub fn frame(path: &Path, seconds: f64) -> Result<Vec<u8>, String> {
    if !seconds.is_finite() || seconds < 0.0 {
        return Err("媒体寻址时间无效".into());
    }
    let cancel = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let mut decoder =
        dinuosi_media::Decoder::open(path, false, seconds, 1e12, VIDEO_FILTER, cancel.clone())?;
    let mut pixels = vec![0; (FRAME_WIDTH * FRAME_HEIGHT * 4) as usize];
    let count = decoder.read(&mut pixels)?;
    if count == pixels.len() {
        return Ok(pixels);
    }
    if count != 0 {
        return Err("素材画面不完整".into());
    }
    let info = dinuosi_media::probe(path)?;
    let start = (info.duration - 1.0).max(0.0);
    let mut tail = dinuosi_media::Decoder::open(path, false, start, 1e12, VIDEO_FILTER, cancel)?;
    let mut last = None;
    loop {
        let count = tail.read(&mut pixels)?;
        if count == 0 {
            break;
        }
        if count != pixels.len() {
            return Err("素材画面不完整".into());
        }
        last = Some(pixels.clone());
    }
    last.ok_or("素材没有可预览的画面".into())
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
