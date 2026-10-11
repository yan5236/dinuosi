//! 可供 GUI、CLI 共用的流式解码；输出 BGRA/PCM，不依赖窗口或音频设备。

use crate::media::{self, FRAME_HEIGHT, FRAME_WIDTH, Media, MediaKind};
use std::{
    io::{BufReader, Read},
    process::{Child, ChildStdout, Stdio},
    sync::{Arc, Mutex},
};

pub struct Decoder {
    processes: Vec<Arc<Mutex<Child>>>,
    pub frames: Arc<Mutex<Option<Vec<u8>>>>,
    pub error: Arc<Mutex<Option<String>>>,
    audio: Option<Pcm>,
    error_readers: Vec<std::thread::JoinHandle<()>>,
}

impl Decoder {
    /// 读取给定素材的秒级区间，视频输出固定 24fps BGRA，音频输出立体声 48kHz PCM。
    /// MIDI 通过内置采样库输出相同 PCM；拒绝离线和越界区间，失败清理进程。
    pub fn start(media: &Media, offset: f64, end: f64) -> Result<Self, String> {
        Self::start_inner(media, offset, end, None)
    }

    /// 为后台导出建立可取消、低并发的流式解码；取消标志同时传入 MIDI 入点合成。
    /// 保留普通播放入口行为，导出 FFmpeg 音视频解码与滤镜各限制一个线程。
    pub(crate) fn start_cancellable(
        media: &Media,
        offset: f64,
        end: f64,
        cancel: &std::sync::atomic::AtomicBool,
    ) -> Result<Self, String> {
        Self::start_inner(media, offset, end, Some(cancel))
    }

    /// 共用媒体区间校验与进程生命周期，导出模式在初始化及 MIDI 寻址时检查取消。
    /// cancel 为 None 时保持既有播放解码参数；失败释放已经启动的进程。
    fn start_inner(
        media: &Media,
        offset: f64,
        end: f64,
        cancel: Option<&std::sync::atomic::AtomicBool>,
    ) -> Result<Self, String> {
        if cancel.is_some_and(|cancel| cancel.load(std::sync::atomic::Ordering::Relaxed)) {
            return Err("导出已取消".into());
        }
        if media.offline || !media.path.is_file() {
            return Err("媒体离线".into());
        }
        if !offset.is_finite()
            || !end.is_finite()
            || offset < 0.0
            || end <= offset
            || end > media.duration
        {
            return Err("媒体播放区间无效".into());
        }
        let mut decoder = Self {
            processes: Vec::new(),
            frames: Arc::new(Mutex::new(None)),
            error: Arc::new(Mutex::new(None)),
            audio: None,
            error_readers: Vec::new(),
        };
        if media.has_audio && crate::midi::is_midi(&media.path) {
            decoder.audio = Some(Pcm(PcmSource::Midi(Box::new(crate::midi::MidiPcm::start(
                &media.path,
                offset,
                end,
                cancel,
            )?))));
        } else if media.has_audio {
            let mut command = media::command("ffmpeg");
            if cancel.is_some() {
                command.args(["-threads", "1", "-filter_threads", "1"]);
            }
            command
                .args(["-v", "error", "-ss", &format!("{offset:.6}"), "-i"])
                .arg(&media.path)
                .args([
                    "-t",
                    &format!("{:.6}", end - offset),
                    "-map",
                    "0:a:0",
                    "-vn",
                    "-ac",
                    "2",
                    "-ar",
                    "48000",
                    "-f",
                    "f32le",
                    "pipe:1",
                ])
                .stdout(Stdio::piped())
                .stderr(Stdio::piped());
            let mut child = command
                .spawn()
                .map_err(|e| format!("无法启动音频解码：{e}"))?;
            let stdout = child.stdout.take().ok_or("无法读取音频流")?;
            let child = Arc::new(Mutex::new(child));
            decoder
                .error_readers
                .extend(collect_errors(&child, decoder.error.clone()));
            decoder.audio = Some(Pcm(PcmSource::Ffmpeg(BufReader::new(stdout))));
            decoder.processes.push(child);
        }
        if media.kind == MediaKind::Video {
            let mut command = media::command("ffmpeg");
            if cancel.is_some() {
                command.args(["-threads", "1", "-filter_threads", "1"]);
            }
            command
                .args(["-v", "error", "-re", "-ss", &format!("{offset:.6}"), "-i"])
                .arg(&media.path);
            media::video_arguments(&mut command);
            command
                .args(["-t", &format!("{:.6}", end - offset), "-r", "24", "pipe:1"])
                .stdout(Stdio::piped())
                .stderr(Stdio::piped());
            let mut child = command
                .spawn()
                .map_err(|e| format!("无法启动视频解码：{e}"))?;
            let mut reader = BufReader::new(child.stdout.take().ok_or("无法读取视频流")?);
            let child = Arc::new(Mutex::new(child));
            decoder
                .error_readers
                .extend(collect_errors(&child, decoder.error.clone()));
            let frames = decoder.frames.clone();
            std::thread::spawn(move || {
                loop {
                    let mut frame = vec![0; (FRAME_WIDTH * FRAME_HEIGHT * 4) as usize];
                    if reader.read_exact(&mut frame).is_err() {
                        break;
                    }
                    *frames.lock().unwrap() = Some(frame);
                }
            });
            decoder.processes.push(child);
        }
        Ok(decoder)
    }

    /// 取走唯一音频样本迭代器，供设备输出或 CLI 消费；无音频或已取走时返回 None。
    /// 迭代器的进程由 Decoder 持有，调用方须在读取期间保留 Decoder。
    pub fn take_audio(&mut self) -> Option<Pcm> {
        self.audio.take()
    }

    /// 返回当前解码进程的共享取消句柄，供后台导出监视器在管道阻塞时终止进程。
    /// 句柄不能代替 Decoder 的清理职责；仅用于取消本次已启动的解码。
    pub(crate) fn cancellation_processes(&self) -> Vec<Arc<Mutex<Child>>> {
        self.processes.clone()
    }

    /// 在调用方消费到音频 EOF 后等待进程和错误读取线程，确认完整解码是否成功。
    /// 必须先消费管道；轮询等待不持锁阻塞导出取消，成功容忍可恢复坏包日志，失败返回详细错误。
    pub fn finish(mut self) -> Result<(), String> {
        let mut failed = false;
        for process in &mut self.processes {
            let status = loop {
                if let Some(status) = process
                    .lock()
                    .unwrap()
                    .try_wait()
                    .map_err(|e| format!("等待音频解码失败：{e}"))?
                {
                    break status;
                }
                std::thread::sleep(std::time::Duration::from_millis(10));
            };
            failed |= !status.success();
        }
        for reader in self.error_readers.drain(..) {
            let _ = reader.join();
        }
        if let Some(error) = self.error.lock().unwrap().clone() {
            return Err(error);
        }
        if failed {
            return Err("音频解码进程失败".into());
        }
        Ok(())
    }
}

impl Drop for Decoder {
    /// 终止并回收所有解码进程，解除管道阻塞；不写入、复制或删除源素材。
    fn drop(&mut self) {
        for process in &mut self.processes {
            let mut process = process.lock().unwrap();
            let _ = process.kill();
            let _ = process.wait();
        }
    }
}

/// 后台排空 FFmpeg stderr，再按子进程退出状态向波形和实时播放调用方报告失败。
/// 成功退出的可恢复坏包日志不发布；失败保留日志，无日志或状态查询失败时提供具体错误。
/// 返回线程供 EOF 确认；等待退出时不持进程锁，让 Drop 始终可以终止仍在运行的解码器。
fn collect_errors(
    child: &Arc<Mutex<Child>>,
    error: Arc<Mutex<Option<String>>>,
) -> Option<std::thread::JoinHandle<()>> {
    let stderr = child.lock().unwrap().stderr.take();
    let child = child.clone();
    stderr.map(|mut stderr| {
        std::thread::spawn(move || {
            let mut message = String::new();
            let _ = stderr.read_to_string(&mut message);
            loop {
                let status = child.lock().unwrap().try_wait();
                match status {
                    Ok(Some(status)) => {
                        if !status.success() {
                            *error.lock().unwrap() = Some(if message.trim().is_empty() {
                                format!("媒体解码进程失败：{status}")
                            } else {
                                format!("媒体解码失败：{}", message.trim())
                            });
                        }
                        break;
                    }
                    Ok(None) => std::thread::sleep(std::time::Duration::from_millis(10)),
                    Err(e) => {
                        *error.lock().unwrap() = Some(format!("读取媒体解码进程状态失败：{e}"));
                        break;
                    }
                }
            }
        })
    })
}

pub struct Pcm(PcmSource);

enum PcmSource {
    Ffmpeg(BufReader<ChildStdout>),
    Midi(Box<crate::midi::MidiPcm>),
}

impl Iterator for Pcm {
    type Item = f32;
    /// 消费 FFmpeg 管道或 MIDI 合成器，均返回交错立体声 48kHz 样本；EOF 时结束。
    fn next(&mut self) -> Option<f32> {
        match &mut self.0 {
            PcmSource::Midi(pcm) => pcm.next(),
            PcmSource::Ffmpeg(reader) => {
                let mut bytes = [0; 4];
                reader.read_exact(&mut bytes).ok()?;
                Some(f32::from_le_bytes(bytes))
            }
        }
    }
}
