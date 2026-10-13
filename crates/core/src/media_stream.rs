//! 可供 GUI、CLI 共用的动态库流式解码；输出 BGRA/PCM，不依赖窗口或音频设备。
use crate::media::{self, FRAME_HEIGHT, FRAME_WIDTH, Media, MediaKind};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};

pub struct Decoder {
    cancel: Arc<AtomicBool>,
    pub frames: Arc<Mutex<Option<Vec<u8>>>>,
    pub error: Arc<Mutex<Option<String>>>,
    audio: Option<Pcm>,
    video: Option<std::thread::JoinHandle<()>>,
}

impl Decoder {
    /// 为倒放建立按秒分块的立体声 PCM；共享取消和错误状态，峰值缓冲不随素材长度增长。
    /// 输入须为在线音频区间，MIDI 同样按块合成；不创建视频线程或修改源文件。
    pub(crate) fn start_reverse(media: &Media, offset: f64, end: f64) -> Result<Self, String> {
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
        let cancel = Arc::new(AtomicBool::new(false));
        let error = Arc::new(Mutex::new(None));
        let pcm =
            reverse::ReversePcm::new(media.clone(), offset, end, cancel.clone(), error.clone());
        Ok(Self {
            cancel,
            frames: Arc::new(Mutex::new(None)),
            error,
            audio: Some(Pcm(PcmSource::Reverse(Box::new(pcm)))),
            video: None,
        })
    }
    /// 读取在线素材的秒级区间；视频为固定 24fps BGRA，音频为立体声 48kHz PCM。
    /// MIDI 维持内置采样合成，普通媒体调用 libav 动态库；非法区间或初始化失败返回错误。
    pub fn start(media: &Media, offset: f64, end: f64) -> Result<Self, String> {
        Self::start_inner(media, offset, end, None)
    }

    /// 为后台导出建立可取消解码；取消在初始化、MIDI 寻址及原生读包时均有检查。
    pub(crate) fn start_cancellable(
        media: &Media,
        offset: f64,
        end: f64,
        cancel: &AtomicBool,
    ) -> Result<Self, String> {
        Self::start_inner(media, offset, end, Some(cancel))
    }

    /// 共用区间验证和独占句柄生命周期；音频按消费速度读取，视频在后台按播放时钟输出。
    /// 每个原生上下文单线程使用，失败释放已初始化资源，不保存完整媒体到内存。
    fn start_inner(
        media: &Media,
        offset: f64,
        end: f64,
        cancel: Option<&AtomicBool>,
    ) -> Result<Self, String> {
        if cancel.is_some_and(|c| c.load(Ordering::Relaxed)) {
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
            cancel: Arc::new(AtomicBool::new(false)),
            frames: Arc::new(Mutex::new(None)),
            error: Arc::new(Mutex::new(None)),
            audio: None,
            video: None,
        };
        if media.has_audio && crate::midi::is_midi(&media.path) {
            decoder.audio = Some(Pcm(PcmSource::Midi(Box::new(crate::midi::MidiPcm::start(
                &media.path,
                offset,
                end,
                cancel,
            )?))));
        } else if media.has_audio {
            let native = dinuosi_media::Decoder::open(
                &media.path,
                true,
                offset,
                end,
                "",
                decoder.cancel.clone(),
            )?;
            decoder.audio = Some(Pcm(PcmSource::Native(Box::new(NativePcm {
                decoder: native,
                cancel: decoder.cancel.clone(),
                bytes: [0; 8192],
                position: 0,
                length: 0,
                done: false,
                error: decoder.error.clone(),
            }))));
        }
        if media.kind == MediaKind::Video {
            let mut native = dinuosi_media::Decoder::open(
                &media.path,
                false,
                offset,
                end,
                &format!("{},fps=24", media::VIDEO_FILTER),
                decoder.cancel.clone(),
            )?;
            let (frames, error, cancel) = (
                decoder.frames.clone(),
                decoder.error.clone(),
                decoder.cancel.clone(),
            );
            decoder.video = Some(std::thread::spawn(move || {
                let started = std::time::Instant::now();
                let mut index = 0;
                loop {
                    if cancel.load(Ordering::Relaxed) {
                        break;
                    }
                    let mut pixels = vec![0; (FRAME_WIDTH * FRAME_HEIGHT * 4) as usize];
                    match native.read(&mut pixels) {
                        Ok(0) => break,
                        Ok(count) if count == pixels.len() => {}
                        Ok(_) => {
                            *error.lock().unwrap() = Some("素材画面不完整".into());
                            break;
                        }
                        Err(message) => {
                            if !cancel.load(Ordering::Relaxed) {
                                *error.lock().unwrap() = Some(message);
                            }
                            break;
                        }
                    }
                    let due = std::time::Duration::from_secs_f64(index as f64 / 24.0);
                    while started.elapsed() < due && !cancel.load(Ordering::Relaxed) {
                        std::thread::sleep(std::time::Duration::from_millis(5));
                    }
                    if cancel.load(Ordering::Relaxed) {
                        break;
                    }
                    *frames.lock().unwrap() = Some(pixels);
                    index += 1;
                }
            }));
        }
        Ok(decoder)
    }

    /// 取走唯一 PCM 迭代器；调用方须保留 Decoder，释放 Decoder 会取消仍存活的音频读取。
    pub fn take_audio(&mut self) -> Option<Pcm> {
        self.audio.take()
    }

    /// 返回本会话的原子取消句柄，供导出监视器中断读包；不共享可变原生上下文。
    pub(crate) fn cancellation(&self) -> Arc<AtomicBool> {
        self.cancel.clone()
    }

    /// 在 PCM 消费结束后等待视频线程并确认真实解码诊断；未消费的音频不会被隐式解码。
    pub fn finish(mut self) -> Result<(), String> {
        if let Some(video) = self.video.take() {
            video.join().map_err(|_| "媒体解码线程失败")?;
        }
        let error = self.error.lock().unwrap().clone();
        error.map_or(Ok(()), Err)
    }
}

impl Drop for Decoder {
    /// 先设置原子取消解除原生读包及播放等待，再回收视频线程；不写入或删除源文件。
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
        if let Some(video) = self.video.take() {
            let _ = video.join();
        }
    }
}

pub struct Pcm(PcmSource);
enum PcmSource {
    Native(Box<NativePcm>),
    Midi(Box<crate::midi::MidiPcm>),
    Reverse(Box<reverse::ReversePcm>),
}
struct NativePcm {
    decoder: dinuosi_media::Decoder,
    cancel: Arc<AtomicBool>,
    bytes: [u8; 8192],
    position: usize,
    length: usize,
    done: bool,
    error: Arc<Mutex<Option<String>>>,
}

impl Iterator for Pcm {
    type Item = f32;
    /// 消费普通或倒放的原生/MIDI PCM，输出交错立体声样本；错误发布至共享状态并结束。
    /// 复用固定 PCM 缓冲，回调不为每个样本分配内存；取消后立即停止消费。
    fn next(&mut self) -> Option<f32> {
        match &mut self.0 {
            PcmSource::Reverse(pcm) => pcm.next(),
            PcmSource::Midi(pcm) => pcm.next(),
            PcmSource::Native(pcm) => {
                if pcm.done || pcm.cancel.load(Ordering::Relaxed) {
                    return None;
                }
                if pcm.position == pcm.length {
                    match pcm.decoder.read(&mut pcm.bytes) {
                        Ok(0) => {
                            pcm.done = true;
                            return None;
                        }
                        Ok(length) if length.is_multiple_of(4) => {
                            pcm.length = length;
                            pcm.position = 0;
                        }
                        Ok(_) => {
                            pcm.done = true;
                            *pcm.error.lock().unwrap() = Some("音频样本不完整".into());
                            return None;
                        }
                        Err(error) => {
                            pcm.done = true;
                            *pcm.error.lock().unwrap() = Some(error);
                            return None;
                        }
                    }
                }
                let sample = f32::from_le_bytes(
                    pcm.bytes[pcm.position..pcm.position + 4]
                        .try_into()
                        .unwrap(),
                );
                pcm.position += 4;
                Some(sample)
            }
        }
    }
}

#[path = "media_reverse.rs"]
mod reverse;
