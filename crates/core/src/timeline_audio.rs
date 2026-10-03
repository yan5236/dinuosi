//! 时间线音频混合：按片段入点、起始帧及轨道静音输出连续 PCM，空隙输出静音。
use crate::{
    media::{Media, MediaKind},
    media_stream::{Decoder, Pcm},
    timeline::Timeline,
};

struct Layer {
    start: u64,
    end: u64,
    pcm: Pcm,
    decoder: Decoder,
    gain: f32,
}
pub struct TimelineAudio {
    layers: Vec<Layer>,
    position: u64,
    end: u64,
}

impl TimelineAudio {
    /// 在后台为时间线剩余音频建立流式解码；所有时间转为交错立体声 48kHz 样本。
    /// 静音轨道、零增益和离线素材跳过；各片段增益进入 PCM 混合，失败清理解码器。
    pub fn start(t: &Timeline, offset: f64, sources: &[Media]) -> Result<Self, String> {
        let fps = t.fps as f64;
        let sample = |seconds: f64| (seconds.max(0.0) * 96000.0).round() as u64;
        let mut layers = Vec::new();
        for c in &t.clips {
            if t.tracks[c.track].muted
                || c.gain == 0.0
                || (c.start + c.duration) as f64 / fps <= offset
            {
                continue;
            }
            let Some(m) = sources
                .iter()
                .find(|m| m.path == c.source)
                .cloned()
                .or_else(|| crate::media::probe(&c.source).ok())
                .filter(|m| m.has_audio && !m.offline)
            else {
                continue;
            };
            let source_in = c.source_in as f64 / fps + (offset - c.start as f64 / fps).max(0.0);
            let source_end = ((c.source_in + c.duration) as f64 / fps).min(m.duration);
            if source_in >= source_end {
                continue;
            }
            let mut audio = m.clone();
            audio.kind = MediaKind::Audio;
            let mut decoder = Decoder::start(&audio, source_in, source_end)?;
            if let Some(pcm) = decoder.take_audio() {
                layers.push(Layer {
                    start: sample(c.start as f64 / fps - offset),
                    end: sample((c.start + c.duration) as f64 / fps - offset),
                    pcm,
                    decoder,
                    gain: c.gain as f32,
                });
            }
        }
        Ok(Self {
            layers,
            position: 0,
            end: sample(t.end() as f64 / fps - offset),
        })
    }
    /// 提取解码器报告的实际错误，供宿主向用户展示；没有错误时返回 None。
    pub fn error(&self) -> Option<String> {
        self.layers
            .iter()
            .find_map(|l| l.decoder.error.lock().unwrap().take())
    }
}
impl Iterator for TimelineAudio {
    type Item = f32;
    /// 按片段增益乘以 PCM 后求和并限制输出振幅；非活动片段不消费样本，末尾结束迭代。
    fn next(&mut self) -> Option<f32> {
        if self.position >= self.end {
            return None;
        }
        let mut value = 0.0;
        for layer in &mut self.layers {
            if self.position >= layer.start && self.position < layer.end {
                value += layer.pcm.next().unwrap_or(0.0) * layer.gain;
            }
        }
        self.position += 1;
        Some(value.clamp(-1.0, 1.0))
    }
}
