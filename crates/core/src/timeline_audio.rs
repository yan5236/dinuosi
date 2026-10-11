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
        Self::start_inner(t, offset, sources, None, |_| {})
    }

    /// 建立导出混音，逐片段检查取消且在解码器启动后立即登记其进程。
    /// register 只登记共享取消句柄；MIDI 长入点合成也可以中断，既有播放入口不受影响。
    pub(crate) fn start_cancellable(
        t: &Timeline,
        offset: f64,
        sources: &[Media],
        cancel: &std::sync::atomic::AtomicBool,
        register: impl Fn(&Decoder),
    ) -> Result<Self, String> {
        Self::start_inner(t, offset, sources, Some(cancel), register)
    }

    /// 共用轨道、裁剪与音量动画初始化；导出模式低并发解码并逐层登记取消句柄。
    /// 任一失败释放已启动的层与进程；缺少可播放音频的片段维持既有跳过语义。
    fn start_inner(
        t: &Timeline,
        offset: f64,
        sources: &[Media],
        cancel: Option<&std::sync::atomic::AtomicBool>,
        register: impl Fn(&Decoder),
    ) -> Result<Self, String> {
        let fps = t.fps as f64;
        let sample = |seconds: f64| (seconds.max(0.0) * 96000.0).round() as u64;
        let mut layers = Vec::new();
        for c in &t.clips {
            if cancel.is_some_and(|cancel| cancel.load(std::sync::atomic::Ordering::Relaxed)) {
                return Err("导出已取消".into());
            }
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
                .or_else(|| {
                    if cancel.is_none() {
                        crate::media::probe(&c.source).ok()
                    } else {
                        None
                    }
                })
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
            let mut decoder = match cancel {
                Some(cancel) => Decoder::start_cancellable(&audio, source_in, source_end, cancel)?,
                None => Decoder::start(&audio, source_in, source_end)?,
            };
            register(&decoder);
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

    /// 在混音消费结束后排空片段剩余 PCM 并确认解码成功，避免末尾 stderr 发布竞态。
    /// 必须由后台调用；每 4096 样本检查取消，异常不发布带损坏或缺失音频的导出文件。
    pub(crate) fn finish_cancellable(
        self,
        cancel: &std::sync::atomic::AtomicBool,
    ) -> Result<(), String> {
        for mut layer in self.layers {
            for (index, _) in layer.pcm.by_ref().enumerate() {
                if index % 4096 == 0 && cancel.load(std::sync::atomic::Ordering::Relaxed) {
                    return Err("导出已取消".into());
                }
            }
            layer.decoder.finish()?;
        }
        Ok(())
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
