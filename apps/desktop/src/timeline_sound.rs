//! 时间线 PCM 的 Rodio 设备适配；采样调度、混合及静音规则均由 Core 决定。
use dinuosi_core::timeline_audio::TimelineAudio;
use rodio::{OutputStream, OutputStreamBuilder, Sink, Source};
use std::{
    sync::{Arc, Mutex},
    time::Duration,
};

pub struct TimelineSound {
    pub sink: Sink,
    _stream: OutputStream,
    pub error: Arc<Mutex<Option<String>>>,
}
impl TimelineSound {
    /// 连接核心混音器到系统默认设备，错误原样上报；设备时钟驱动时间线播放进度。
    pub fn new(audio: TimelineAudio, volume: f32) -> Result<Self, String> {
        let stream = OutputStreamBuilder::open_default_stream()
            .map_err(|e| format!("无法打开音频设备：{e}"))?;
        let sink = Sink::connect_new(stream.mixer());
        sink.set_volume(volume);
        let error = Arc::new(Mutex::new(None));
        sink.append(Samples {
            audio,
            error: error.clone(),
            count: 0,
        });
        Ok(Self {
            sink,
            _stream: stream,
            error,
        })
    }
}
struct Samples {
    audio: TimelineAudio,
    error: Arc<Mutex<Option<String>>>,
    count: usize,
}
impl Iterator for Samples {
    type Item = f32;
    /// 转发核心样本并每半秒提取解码错误，避免在每个样本上加锁。
    fn next(&mut self) -> Option<f32> {
        self.count += 1;
        if self.count.is_multiple_of(48000)
            && let Some(error) = self.audio.error()
        {
            *self.error.lock().unwrap() = Some(error);
        }
        self.audio.next()
    }
}
impl Source for Samples {
    /// 核心输出格式在流内固定，设备可连续消费。
    fn current_span_len(&self) -> Option<usize> {
        None
    }
    /// 返回交错立体声声道数。
    fn channels(&self) -> u16 {
        2
    }
    /// 返回核心固定的每声道采样率。
    fn sample_rate(&self) -> u32 {
        48000
    }
    /// 解码器管理流结束，不提供额外内存缓冲的时长。
    fn total_duration(&self) -> Option<Duration> {
        None
    }
}
