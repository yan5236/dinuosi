//! 可供 GUI、CLI 共用的流式解码；输出 BGRA/PCM，不依赖窗口或音频设备。

use crate::media::{self, FRAME_HEIGHT, FRAME_WIDTH, Media, MediaKind};
use std::{
    io::{BufReader, Read},
    process::{Child, ChildStdout, Stdio},
    sync::{Arc, Mutex},
};

pub struct Decoder {
    processes: Vec<Child>,
    pub frames: Arc<Mutex<Option<Vec<u8>>>>,
    pub error: Arc<Mutex<Option<String>>>,
    audio: Option<Pcm>,
    error_readers: Vec<std::thread::JoinHandle<()>>,
}

impl Decoder {
    /// 读取给定素材的秒级区间，视频输出固定 24fps BGRA，音频输出立体声 48kHz PCM。
    /// MIDI 通过内置采样库输出相同 PCM；拒绝离线和越界区间，失败清理进程。
    pub fn start(media: &Media, offset: f64, end: f64) -> Result<Self, String> {
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
            )?))));
        } else if media.has_audio {
            let mut command = media::command("ffmpeg");
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
            decoder
                .error_readers
                .extend(collect_errors(&mut child, decoder.error.clone()));
            decoder.audio = Some(Pcm(PcmSource::Ffmpeg(BufReader::new(
                child.stdout.take().ok_or("无法读取音频流")?,
            ))));
            decoder.processes.push(child);
        }
        if media.kind == MediaKind::Video {
            let mut command = media::command("ffmpeg");
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
            decoder
                .error_readers
                .extend(collect_errors(&mut child, decoder.error.clone()));
            let mut reader = BufReader::new(child.stdout.take().ok_or("无法读取视频流")?);
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

    /// 在调用方消费到音频 EOF 后等待进程和错误读取线程，确认完整解码是否成功。
    /// 必须先消费管道；进程失败或错误输出返回可显示错误，MIDI 无子进程时直接成功。
    pub fn finish(mut self) -> Result<(), String> {
        let mut failed = false;
        for process in &mut self.processes {
            failed |= !process
                .wait()
                .map_err(|e| format!("等待音频解码失败：{e}"))?
                .success();
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
            let _ = process.kill();
            let _ = process.wait();
        }
    }
}

/// 在后台消费 FFmpeg stderr，避免管道填满；实际错误提供给所有前端读取。
/// 返回读取线程以便离线提取在 EOF 后确认全部错误，实时播放仍可随时销毁解码器。
fn collect_errors(
    child: &mut Child,
    error: Arc<Mutex<Option<String>>>,
) -> Option<std::thread::JoinHandle<()>> {
    child.stderr.take().map(|mut stderr| {
        std::thread::spawn(move || {
            let mut message = String::new();
            let _ = stderr.read_to_string(&mut message);
            if !message.trim().is_empty() {
                *error.lock().unwrap() = Some(format!("媒体解码失败：{}", message.trim()));
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
