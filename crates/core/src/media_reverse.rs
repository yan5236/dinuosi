//! 倒放 PCM 只缓存一秒，并按立体声采样对反转，保留左右声道及原生取消句柄。
use super::{NativePcm, Pcm, PcmSource};
use crate::media::Media;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};

pub(super) struct ReversePcm {
    media: Media,
    start: u64,
    end: u64,
    samples: Vec<f32>,
    position: usize,
    cancel: Arc<AtomicBool>,
    error: Arc<Mutex<Option<String>>>,
}

impl ReversePcm {
    /// 记录合法源区间为 48kHz 采样边界；延迟到消费时解码，不分配整段音频。
    pub(super) fn new(
        media: Media,
        start: f64,
        end: f64,
        cancel: Arc<AtomicBool>,
        error: Arc<Mutex<Option<String>>>,
    ) -> Self {
        Self {
            media,
            start: (start * 48000.0).round() as u64,
            end: (end * 48000.0).round() as u64,
            samples: Vec::new(),
            position: 0,
            cancel,
            error,
        }
    }

    /// 从剩余区间尾部向前解码最多一秒；短尾用静音补齐，解码错误或取消立即结束。
    /// 原生句柄复用外层取消标志；MIDI 保留合成器错误与取消检查，不改变声道顺序。
    fn refill(&mut self) -> Result<(), String> {
        let start = self.end.saturating_sub(48000).max(self.start);
        let expected = ((self.end - start) * 2) as usize;
        let offset = start as f64 / 48000.0;
        let end = (self.end as f64 / 48000.0).min(self.media.duration);
        let mut pcm = if crate::midi::is_midi(&self.media.path) {
            Pcm(PcmSource::Midi(Box::new(crate::midi::MidiPcm::start(
                &self.media.path,
                offset,
                end,
                Some(&self.cancel),
            )?)))
        } else {
            let decoder = dinuosi_media::Decoder::open(
                &self.media.path,
                true,
                offset,
                end,
                "",
                self.cancel.clone(),
            )?;
            Pcm(PcmSource::Native(Box::new(NativePcm {
                decoder,
                cancel: self.cancel.clone(),
                bytes: [0; 8192],
                position: 0,
                length: 0,
                done: false,
                error: self.error.clone(),
            })))
        };
        self.samples.clear();
        for sample in pcm.by_ref() {
            if self.cancel.load(Ordering::Relaxed) {
                return Err("导出已取消".into());
            }
            if self.samples.len() < expected {
                self.samples.push(sample);
            }
        }
        if let Some(error) = self.error.lock().unwrap().clone() {
            return Err(error);
        }
        self.samples.resize(expected, 0.0);
        self.position = 0;
        self.end = start;
        Ok(())
    }
}

impl Iterator for ReversePcm {
    type Item = f32;
    /// 按尾部采样对到首部采样对输出 PCM；每对仍先左后右，块耗尽才解码下一块。
    /// 错误发布至 Decoder 的共享状态并结束；取消后不继续分配或寻址。
    fn next(&mut self) -> Option<f32> {
        if self.cancel.load(Ordering::Relaxed) {
            return None;
        }
        if self.position == self.samples.len() {
            if self.end <= self.start {
                return None;
            }
            if let Err(error) = self.refill() {
                *self.error.lock().unwrap() = Some(error);
                self.end = self.start;
                self.samples.clear();
                self.position = 0;
                return None;
            }
        }
        let index = self.samples.len() - 2 - (self.position / 2) * 2 + self.position % 2;
        self.position += 1;
        Some(self.samples[index])
    }
}
