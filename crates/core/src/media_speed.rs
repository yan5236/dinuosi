//! 变速 PCM：常规与曲线共用固定窗相位匹配保持音调，线性重采样实现变调；缓冲有界。
use super::Pcm;
use crate::timeline::Clip;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

pub(super) struct SpeedPcm {
    source: Pcm,
    clip: Clip,
    fps: f64,
    elapsed: f64,
    origin: f64,
    output: u64,
    curve: Option<super::curve::CurvePcm>,
    buffer: [f32; 4096],
    position: usize,
    length: usize,
    pair: [[f32; 2]; 2],
    pair_index: u64,
    initialized: bool,
    cancel: Arc<AtomicBool>,
}

impl SpeedPcm {
    /// 在后台继续消费变速包装内的原始选区；避免项目时长舍入或滤镜延迟掩盖尾部解码错误。
    /// 保留原始 PCM 的取消检查和共享诊断，不补写超过时间线时长的输出样本。
    pub(super) fn finish_cancellable(&mut self, cancel: &AtomicBool) -> Result<(), String> {
        self.source.finish_cancellable(cancel)
    }
    /// 接收唯一源 PCM 与变速片段；elapsed 为项目局部帧，取消和解码诊断沿用外层会话。
    /// 关闭变调时常规和曲线共用固定窗相位匹配器，按项目时间映射源位置，避免原生滤镜提前 EOF。
    /// 未设置变速时立即返回错误，不修改源文件；固定窗缓存不随选区长度增长。
    pub(super) fn new(
        source: Pcm,
        clip: Clip,
        fps: f64,
        elapsed: f64,
        cancel: Arc<AtomicBool>,
    ) -> Result<Self, String> {
        let pitch = clip.speed.as_ref().ok_or("片段未设置变速")?.pitch;
        let origin = clip.source_progress(elapsed);
        let curve = (!pitch).then(super::curve::CurvePcm::new);
        Ok(Self {
            source,
            clip,
            fps,
            elapsed,
            origin,
            output: 0,
            curve,
            buffer: [0.0; 4096],
            position: 0,
            length: 0,
            pair: [[0.0; 2]; 2],
            pair_index: 0,
            initialized: false,
            cancel,
        })
    }

    /// 将项目采样时间反解为源采样位置，成对线性插值并保持左右声道，提供可变速且变调的 PCM。
    /// 遇到源尾部补零，时间线负责精确限制输出时长；每个目标采样最多消费当前倍率对应的源样本。
    fn pitched(&mut self) -> f32 {
        if self.output.is_multiple_of(2) {
            if !self.initialized {
                for pair in &mut self.pair {
                    *pair = [
                        self.source.next().unwrap_or(0.0),
                        self.source.next().unwrap_or(0.0),
                    ];
                }
                self.initialized = true;
            }
            let local = self.elapsed + (self.output / 2) as f64 / 48000.0 * self.fps;
            let speed = self.clip.speed.as_ref().unwrap();
            let target = ((self.clip.source_progress(local) - self.origin)
                * (speed.source[1] - speed.source[0])
                / self.fps
                * 48000.0)
                .max(0.0);
            while self.pair_index < target.floor() as u64 {
                self.pair[0] = self.pair[1];
                self.pair[1] = [
                    self.source.next().unwrap_or(0.0),
                    self.source.next().unwrap_or(0.0),
                ];
                self.pair_index += 1;
            }
            let fraction = target.fract() as f32;
            for channel in 0..2 {
                self.buffer[channel] = self.pair[0][channel]
                    + (self.pair[1][channel] - self.pair[0][channel]) * fraction;
            }
        }
        let sample = self.buffer[(self.output % 2) as usize];
        self.output += 1;
        sample
    }
}

impl Iterator for SpeedPcm {
    type Item = f32;
    /// 按项目剩余时长消费变速样本；关闭变调时常规与曲线共用源时间调度，避免滤镜 EOF 截短声音。
    /// 实际解码错误沿用源会话诊断，取消立刻返回 EOF，源文件实际缺失的样本仍由固定窗补零。
    fn next(&mut self) -> Option<f32> {
        if self.cancel.load(Ordering::Relaxed) {
            return None;
        }
        let remaining = ((self.clip.duration as f64 - self.elapsed) / self.fps * 48000.0)
            .round()
            .max(0.0) as u64
            * 2;
        if self.output >= remaining {
            return None;
        }
        if let Some(curve) = &mut self.curve {
            if self.position == self.length {
                let speed = self.clip.speed.as_ref().unwrap();
                let span = (speed.source[1] - speed.source[0]) / self.fps * 48000.0;
                let local = self.elapsed + (self.output / 2) as f64 / 48000.0 * self.fps;
                let target = (self.clip.source_progress(local) - self.origin) * span;
                let source_pairs = ((1.0 - self.origin) * span).round().max(0.0) as u64;
                self.length =
                    curve.render(&mut self.source, target, source_pairs, &mut self.buffer);
                self.position = 0;
            }
            let sample = self.buffer[self.position];
            self.position += 1;
            self.output += 1;
            return Some(sample);
        }
        Some(self.pitched())
    }
}
