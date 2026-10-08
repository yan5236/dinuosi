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
    keys: Vec<(f64, f64)>,
    key_index: usize,
    fps: f64,
    offset: f64,
}
pub struct TimelineAudio {
    layers: Vec<Layer>,
    position: u64,
    end: u64,
}

impl TimelineAudio {
    /// 在后台为时间线剩余音频建立流式解码；所有时间转为交错立体声 48kHz 样本。
    /// 静音轨道、始终静音和离线素材跳过；音量动画排序后以游标消费，失败清理解码器。
    pub fn start(t: &Timeline, offset: f64, sources: &[Media]) -> Result<Self, String> {
        let fps = t.fps as f64;
        let sample = |seconds: f64| (seconds.max(0.0) * 96000.0).round() as u64;
        let mut layers = Vec::new();
        for c in &t.clips {
            if t.tracks[c.track].muted
                || (c.gain == 0.0
                    && !c.keyframes.iter().any(|k| {
                        k.kind == crate::keyframes::KeyframeKind::Volume && k.values[0] > 0.0
                    }))
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
                let mut keys: Vec<_> = c
                    .keyframes
                    .iter()
                    .filter(|k| k.kind == crate::keyframes::KeyframeKind::Volume)
                    .map(|k| ((c.start + k.frame) as f64, k.values[0]))
                    .collect();
                keys.sort_by(|a, b| a.0.total_cmp(&b.0));
                layers.push(Layer {
                    start: sample(c.start as f64 / fps - offset),
                    end: sample((c.start + c.duration) as f64 / fps - offset),
                    pcm,
                    decoder,
                    gain: c.gain as f32,
                    keys,
                    key_index: 0,
                    fps,
                    offset,
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
    /// 在立体声采样对的同一时刻线性插值增益，再混合并限制振幅；跳转从 offset 开始求值。
    /// 非活动片段不消费样本，音量游标仅前进，末尾结束迭代。
    fn next(&mut self) -> Option<f32> {
        if self.position >= self.end {
            return None;
        }
        let mut value = 0.0;
        for layer in &mut self.layers {
            if self.position >= layer.start && self.position < layer.end {
                let frame = (layer.offset + (self.position / 2) as f64 / 48000.0) * layer.fps;
                // 关键帧游标只前进，音频回调不分配内存，也不为每个采样遍历整条动画。
                while layer.key_index + 1 < layer.keys.len()
                    && layer.keys[layer.key_index + 1].0 <= frame
                {
                    layer.key_index += 1;
                }
                if let Some(&(a, gain)) = layer.keys.get(layer.key_index) {
                    layer.gain = match layer.keys.get(layer.key_index + 1) {
                        Some(&(b, next)) if frame > a => {
                            (gain + (next - gain) * ((frame - a) / (b - a))) as f32
                        }
                        _ => gain as f32,
                    };
                }
                value += layer.pcm.next().unwrap_or(0.0) * layer.gain;
            }
        }
        self.position += 1;
        Some(value.clamp(-1.0, 1.0))
    }
}
