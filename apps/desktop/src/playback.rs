//! 核心解码器的桌面音频设备适配与播放时钟，不在桌面端调用 FFmpeg。

use crate::media::Media;
use dinuosi_core::media_stream::Decoder;
use rodio::{OutputStream, OutputStreamBuilder, Sink, Source};
use std::{
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

pub struct Playback {
    decoder: Option<Decoder>,
    pub frames: Arc<Mutex<Option<Vec<u8>>>>,
    sink: Option<Sink>,
    _stream: Option<OutputStream>,
    started: Instant,
    offset: f64,
    pub error: Arc<Mutex<Option<String>>>,
}

impl Playback {
    /// 从指定秒数启动持续解码；视频帧以 24fps 输出，音频以立体声 48kHz 连接默认设备。
    /// 初始化失败返回错误并通过 Drop 清理已启动进程；长文件不整段加载到内存。
    pub fn start(media: &Media, offset: f64, end: f64, volume: f32) -> Result<Self, String> {
        let mut player = Self {
            decoder: None,
            frames: Arc::new(Mutex::new(None)),
            sink: None,
            _stream: None,
            started: Instant::now(),
            offset,
            error: Arc::new(Mutex::new(None)),
        };
        let mut decoder = Decoder::start(&media.source, offset, end)?;
        player.frames = decoder.frames.clone();
        player.error = decoder.error.clone();
        if let Some(audio) = decoder.take_audio() {
            let stream = OutputStreamBuilder::open_default_stream()
                .map_err(|e| format!("无法打开音频设备：{e}"))?;
            let sink = Sink::connect_new(stream.mixer());
            sink.set_volume(volume);
            sink.append(Pcm(audio));
            player.sink = Some(sink);
            player._stream = Some(stream);
        }
        player.decoder = Some(decoder);
        player.started = Instant::now();
        Ok(player)
    }

    /// 返回源素材播放位置；有音频时使用设备消费时钟，静音视频/图片使用单调时钟。
    pub fn position(&self) -> f64 {
        self.offset
            + self
                .sink
                .as_ref()
                .map(|s| s.get_pos().as_secs_f64())
                .unwrap_or_else(|| self.started.elapsed().as_secs_f64())
    }

    /// 实时调整音频设备增益；零表示静音，不重启解码器或改变播放位置。
    pub fn volume(&self, value: f32) {
        if let Some(sink) = &self.sink {
            sink.set_volume(value.clamp(0.0, 1.0));
        }
    }
}

impl Drop for Playback {
    /// 先杀死解码器解除管道读取，再释放音频源；防止关闭窗口后仍播放或留下子进程。
    fn drop(&mut self) {
        self.decoder = None;
        if let Some(sink) = &self.sink {
            sink.stop();
        }
    }
}

struct Pcm(dinuosi_core::media_stream::Pcm);

impl Iterator for Pcm {
    type Item = f32;
    /// 直接消费核心的 PCM 样本；桌面端只适配 Rodio 音频设备接口。
    fn next(&mut self) -> Option<f32> {
        self.0.next()
    }
}

impl Source for Pcm {
    /// 固定输出格式在整个流中不变，未知跨度允许 Rodio 连续消费管道。
    fn current_span_len(&self) -> Option<usize> {
        None
    }
    /// 核心管道和 MIDI 合成均输出立体声，不依赖源文件声道布局。
    fn channels(&self) -> u16 {
        2
    }
    /// 核心管道和 MIDI 合成均输出 48kHz，不依赖源文件采样率。
    fn sample_rate(&self) -> u32 {
        48000
    }
    /// 流式解码的生命周期由预览选区控制，不报告整段内存音频时长。
    fn total_duration(&self) -> Option<Duration> {
        None
    }
}
