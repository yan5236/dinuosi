//! 时间线导出：受控编码器、后台进度、可取消处理和无覆盖的最终文件发布。
use crate::{
    export_process::{Process, ProcessList, register},
    export_render::Renderer,
    media::{self, Media},
    timeline::Timeline,
    timeline_audio::TimelineAudio,
};
use std::{
    fs::{self, File},
    io::{BufWriter, Write},
    path::{Path, PathBuf},
    process::Stdio,
    sync::atomic::{AtomicBool, Ordering},
};

pub use crate::export_capabilities::capabilities;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Container {
    Mp4,
    Matroska,
}
impl Container {
    /// 返回封装格式的文件扩展名，用于输出路径；不包含前导点。
    pub fn extension(self) -> &'static str {
        match self {
            Self::Mp4 => "mp4",
            Self::Matroska => "mkv",
        }
    }
    /// 返回参数控件中的封装名称。
    pub fn label(self) -> &'static str {
        match self {
            Self::Mp4 => "MP4",
            Self::Matroska => "Matroska",
        }
    }
    /// 返回受控 FFmpeg 的 muxer 名称，显式选择避免临时文件扩展名影响封装。
    fn muxer(self) -> &'static str {
        match self {
            Self::Mp4 => "mp4",
            Self::Matroska => "matroska",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Encoder {
    H264MediaFoundationHardware,
    H264MediaFoundation,
    Mpeg4,
}
impl Encoder {
    /// 返回 FFmpeg 编码器 ID；仅允许项目许可边界内的实现，不接受任意编码器字符串。
    pub fn id(self) -> &'static str {
        match self {
            Self::H264MediaFoundationHardware | Self::H264MediaFoundation => "h264_mf",
            Self::Mpeg4 => "mpeg4",
        }
    }
    /// 返回设置控件的编码器名称，明确区分编码格式与 Windows 实现。
    pub fn label(self) -> &'static str {
        match self {
            Self::H264MediaFoundationHardware => "H.264 (Windows hardware)",
            Self::H264MediaFoundation => "H.264 (Windows software)",
            Self::Mpeg4 => "MPEG-4",
        }
    }
    /// 返回当前编码实现支持的 4:2:0 输入格式；Media Foundation 使用通用的 NV12。
    pub(crate) fn pixel_format(self) -> &'static str {
        match self {
            Self::H264MediaFoundationHardware | Self::H264MediaFoundation => "nv12",
            Self::Mpeg4 => "yuv420p",
        }
    }
    /// 将选中 Media Foundation 模式显式追加至输出参数，探测和真实导出共用。
    /// 硬件仅枚举硬件 MFT，软件仅使用同步软件 MFT；不启动进程，也不为 MPEG-4 添加私有参数。
    pub(crate) fn configure(self, command: &mut std::process::Command) {
        if self != Self::Mpeg4 {
            command.args([
                "-hw_encoding",
                if self == Self::H264MediaFoundationHardware {
                    "1"
                } else {
                    "0"
                },
            ]);
        }
    }
}

#[derive(Clone, Debug)]
pub struct ExportSettings {
    pub width: u32,
    pub height: u32,
    pub fps: u32,
    pub bitrate_kbps: u32,
    pub container: Container,
    pub encoder: Encoder,
}
impl ExportSettings {
    /// 用项目画布和输出默认值创建参数；编码器仍必须从当前机器能力检测结果选择。
    pub fn for_timeline(timeline: &Timeline, encoder: Encoder) -> Self {
        Self {
            width: timeline.width,
            height: timeline.height,
            fps: timeline.fps,
            bitrate_kbps: timeline.bitrate_kbps,
            container: Container::Mp4,
            encoder,
        }
    }
    /// 校验有限工作负载及编码基本约束，非法参数在创建任何输出文件前返回错误。
    pub fn validate(&self) -> Result<(), String> {
        if !(16..=8192).contains(&self.width)
            || !(16..=8192).contains(&self.height)
            || !self.width.is_multiple_of(2)
            || !self.height.is_multiple_of(2)
            || !(1..=240).contains(&self.fps)
            || !(100..=1_000_000).contains(&self.bitrate_kbps)
        {
            Err(
                "导出分辨率必须为 16 至 8192 的偶数，帧率为 1 至 240，码率为 100 至 1000000 kbps"
                    .into(),
            )
        } else {
            Ok(())
        }
    }
}

/// 阻塞导出不可变时间线快照；宿主必须在后台调用，progress 报告 0..1，取消以错误返回。
/// 在输出同目录创建独立临时目录，编码完成且所有媒体成功后无覆盖发布；任何失败清理临时文件。
/// 源素材从引用路径读取，已有输出永不覆盖；运行期间输出路径被占用也返回错误。
pub fn export(
    timeline: &Timeline,
    sources: &[Media],
    settings: &ExportSettings,
    output: &Path,
    cancel: &AtomicBool,
    progress: impl Fn(f32),
) -> Result<(), String> {
    timeline.validate()?;
    settings.validate()?;
    if timeline.end() <= 0 {
        return Err("时间线为空，无法导出".into());
    }
    if output.exists() {
        return Err("输出文件已存在，请选择其他文件名".into());
    }
    if !output.parent().is_some_and(Path::is_dir) {
        return Err("输出文件夹不存在".into());
    }
    let processes = ProcessList::default();
    let stop = AtomicBool::new(false);
    std::thread::scope(|scope| {
        scope.spawn(|| {
            while !stop.load(Ordering::Relaxed) {
                if cancel.load(Ordering::Relaxed) {
                    for child in processes
                        .lock()
                        .unwrap()
                        .iter()
                        .filter_map(std::sync::Weak::upgrade)
                    {
                        let _ = child.lock().unwrap().kill();
                    }
                }
                std::thread::sleep(std::time::Duration::from_millis(30));
            }
        });
        let result = run(
            timeline, sources, settings, output, cancel, &progress, &processes,
        );
        stop.store(true, Ordering::Relaxed);
        if result.is_err() && cancel.load(Ordering::Relaxed) {
            Err("导出已取消".into())
        } else {
            result
        }
    })
}

/// 校验参与输出的已载入源引用；隐藏且无声音的片段不读取，不启动无法取消的探测。
/// 源文件缺失、离线或尺寸超出安全范围返回具体错误，防止预览跳过行为掩盖数据丢失。
fn sources(timeline: &Timeline, items: &[Media]) -> Result<Vec<Media>, String> {
    let mut sources = Vec::new();
    for clip in &timeline.clips {
        let track = &timeline.tracks[clip.track];
        if (!track.visible || clip.kind == media::MediaKind::Audio) && track.muted {
            continue;
        }
        if sources.iter().any(|m: &Media| m.path == clip.source) {
            continue;
        }
        let item = items.iter().find(|m| m.path == clip.source);
        if !track.visible && item.is_some_and(|item| !item.has_audio) {
            continue;
        }
        if !clip.source.is_file() {
            return Err(format!("导出素材不存在：{}", clip.source.display()));
        }
        let item =
            item.ok_or_else(|| format!("导出素材信息尚未载入：{}", clip.source.display()))?;
        if item.offline {
            return Err(format!("导出素材离线：{}", clip.source.display()));
        }
        if item.source_size.0 == 0
            || item.source_size.1 == 0
            || item.source_size.0 as u64 * item.source_size.1 as u64 > 67_108_864
        {
            return Err(format!("素材尺寸过大或无效：{}", clip.source.display()));
        }
        sources.push(item.clone());
    }
    Ok(sources)
}

struct Temporary(PathBuf);
impl Drop for Temporary {
    /// 删除本次创建的专属临时目录，仅包含自身生成的 PCM 和视频；保留用户输出与源文件。
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// 先写精确时间线 PCM，再顺序合成并编码视频；两阶段共享取消监视和进度。
/// 完成编码后使用同卷硬链接原子创建最终文件，目标已存在时失败，避免 rename 覆盖平台差异。
fn run(
    timeline: &Timeline,
    items: &[Media],
    settings: &ExportSettings,
    output: &Path,
    cancel: &AtomicBool,
    progress: &impl Fn(f32),
    processes: &ProcessList,
) -> Result<(), String> {
    check_cancel(cancel)?;
    let sources = sources(timeline, items)?;
    let temporary_path = output
        .parent()
        .unwrap()
        .join(format!(".dinuosi-export-{}", uuid::Uuid::new_v4()));
    fs::create_dir(&temporary_path).map_err(|e| format!("无法创建导出临时目录：{e}"))?;
    let temporary = Temporary(temporary_path);
    let audio_path = temporary.0.join("audio.f32");
    let video_path = temporary.0.join("video.part");
    let duration = timeline.end() as f64 / timeline.fps as f64;
    let total_samples = (duration * 96000.0).round() as u64;
    let mut audio = TimelineAudio::start_cancellable(timeline, 0.0, &sources, cancel, |decoder| {
        for child in decoder.cancellation_processes() {
            register(processes, &child);
        }
    })?;
    let mut writer =
        BufWriter::new(File::create(&audio_path).map_err(|e| format!("无法创建导出音频：{e}"))?);
    progress(0.0);
    for index in 0..total_samples {
        if index % 4096 == 0 {
            check_cancel(cancel)?;
            progress(index as f32 / total_samples as f32 * 0.15);
        }
        writer
            .write_all(&audio.next().unwrap_or(0.0).to_le_bytes())
            .map_err(|e| format!("写入导出音频失败：{e}"))?;
    }
    writer
        .flush()
        .map_err(|e| format!("写入导出音频失败：{e}"))?;
    drop(writer);
    audio.finish_cancellable(cancel)?;
    check_cancel(cancel)?;
    let mut command = media::command("ffmpeg");
    command
        .args([
            "-v",
            "error",
            "-n",
            "-f",
            "rawvideo",
            "-pixel_format",
            "bgra",
            "-video_size",
            &format!("{}x{}", settings.width, settings.height),
            "-framerate",
            &settings.fps.to_string(),
            "-i",
            "pipe:0",
            "-f",
            "f32le",
            "-ar",
            "48000",
            "-ac",
            "2",
            "-i",
        ])
        .arg(&audio_path);
    command.args(["-threads", "2", "-filter_threads", "1"]);
    command.args([
        "-map",
        "0:v:0",
        "-map",
        "1:a:0",
        "-c:v",
        settings.encoder.id(),
        "-pix_fmt",
        settings.encoder.pixel_format(),
        "-b:v",
        &format!("{}k", settings.bitrate_kbps),
        "-c:a",
        "aac",
        "-b:a",
        "192k",
        "-t",
        &format!("{duration:.9}"),
        "-f",
        settings.container.muxer(),
    ]);
    settings.encoder.configure(&mut command);
    if settings.container == Container::Mp4 {
        command.args(["-movflags", "+faststart"]);
    }
    command
        .arg(&video_path)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped());
    let mut encoder = Process::spawn(&mut command, processes)?;
    let mut stdin = encoder
        .child
        .lock()
        .unwrap()
        .stdin
        .take()
        .ok_or("无法写入视频编码器")?;
    let mut renderer = Renderer::new();
    let frame_count = (duration * settings.fps as f64).ceil() as u64;
    for frame in 0..frame_count {
        check_cancel(cancel)?;
        let pixels = renderer.render(
            timeline,
            frame as f64 * timeline.fps as f64 / settings.fps as f64,
            &sources,
            settings,
            processes,
        )?;
        if let Err(error) = stdin.write_all(&pixels) {
            drop(stdin);
            return Err(encoder
                .finish()
                .err()
                .unwrap_or_else(|| format!("写入视频编码器失败：{error}")));
        }
        progress(0.15 + (frame + 1) as f32 / frame_count as f32 * 0.8);
    }
    drop(stdin);
    encoder.finish()?;
    check_cancel(cancel)?;
    publish(&video_path, output, cancel)?;
    progress(1.0);
    Ok(())
}

/// 优先通过硬链接原子发布；不支持硬链接的文件系统采用 create_new 复制，绝不覆盖已有文件。
/// 复制失败或取消时仅删除本次创建的未完成输出，源临时文件由会话清理。
fn publish(source: &Path, output: &Path, cancel: &AtomicBool) -> Result<(), String> {
    if fs::hard_link(source, output).is_ok() {
        return Ok(());
    }
    let mut input = File::open(source).map_err(|e| format!("无法读取完成的视频：{e}"))?;
    let mut target = File::options()
        .write(true)
        .create_new(true)
        .open(output)
        .map_err(|e| format!("无法保存导出视频（已有文件不会覆盖）：{e}"))?;
    let result = (|| {
        use std::io::Read;
        let mut bytes = [0; 65536];
        loop {
            check_cancel(cancel)?;
            let count = input
                .read(&mut bytes)
                .map_err(|e| format!("读取完成的视频失败：{e}"))?;
            if count == 0 {
                break;
            }
            target
                .write_all(&bytes[..count])
                .map_err(|e| format!("保存导出视频失败：{e}"))?;
        }
        target
            .sync_all()
            .map_err(|e| format!("保存导出视频失败：{e}"))
    })();
    drop(target);
    if result.is_err() {
        let _ = fs::remove_file(output);
    }
    result
}

/// 在工作边界读取取消标志；返回固定状态供界面识别，避免继续写入临时数据。
fn check_cancel(cancel: &AtomicBool) -> Result<(), String> {
    if cancel.load(Ordering::Relaxed) {
        Err("导出已取消".into())
    } else {
        Ok(())
    }
}

#[cfg(test)]
#[path = "export_tests.rs"]
mod tests;
