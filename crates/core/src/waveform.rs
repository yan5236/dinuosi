//! 音频峰值缓存：保留左右声道的极值，以源时间查询，供任意界面缩放绘制。

use crate::{
    media::{Media, MediaKind},
    media_stream::Decoder,
};

const RATE: f64 = 48_000.0;
const BLOCK: usize = 16;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Peak {
    pub min: f32,
    pub max: f32,
}

impl Peak {
    /// 合并两个区间的上下峰值；零值保留为基线，不混合声道以免反相音频抵消。
    fn merge(self, other: Self) -> Self {
        Self {
            min: self.min.min(other.min),
            max: self.max.max(other.max),
        }
    }
}

pub struct Waveform {
    levels: Vec<Vec<Peak>>,
}

impl Waveform {
    /// 后台解码完整音频并构造多级峰值缓存，普通媒体与 MIDI 共用立体声 48kHz 解码器。
    /// 仅支持在线音频素材；读取或解码失败返回实际错误，不写源文件、不访问声音设备。
    pub fn decode(media: &Media) -> Result<Self, String> {
        if media.kind != MediaKind::Audio || !media.has_audio {
            return Err("素材没有可读取的音频波形".into());
        }
        let mut decoder = Decoder::start(media, 0.0, media.duration)?;
        let pcm = decoder.take_audio().ok_or("素材没有音频流")?;
        let waveform = Self::from_pcm(pcm);
        decoder.finish()?;
        if waveform.levels[0].is_empty() {
            return Err("音频解码没有产生样本".into());
        }
        Ok(waveform)
    }

    /// 消费交错双声道 PCM，每 16 个采样帧保留极值，再两两汇总供缩小查询。
    /// 内存随时长增长但不保留完整 PCM，最细分辨率约 0.33ms，覆盖时间线 1800px/s 上限。
    fn from_pcm(pcm: impl Iterator<Item = f32>) -> Self {
        let mut peaks = Vec::new();
        let mut peak = Peak::default();
        let mut count = 0;
        for sample in pcm {
            let sample = if sample.is_finite() {
                sample.clamp(-1.0, 1.0)
            } else {
                0.0
            };
            peak = peak.merge(Peak {
                min: sample,
                max: sample,
            });
            count += 1;
            if count == BLOCK * 2 {
                peaks.push(peak);
                peak = Peak::default();
                count = 0;
            }
        }
        if count > 0 {
            peaks.push(peak);
        }
        let mut levels = vec![peaks];
        while levels.last().unwrap().len() > 1 {
            let next = levels
                .last()
                .unwrap()
                .chunks(2)
                .map(|pair| pair[0].merge(*pair.get(1).unwrap_or(&Peak::default())))
                .collect();
            levels.push(next);
        }
        Self { levels }
    }

    /// 查询源时间半开区间的极值；按多级缓存分解区间，缩小时也保留短暂瞬态峰值。
    /// 边界按最细桶向外取整，越界部分忽略；空区间、非有限输入或没有样本时返回零基线。
    pub fn peak(&self, start: f64, end: f64) -> Peak {
        if !start.is_finite() || !end.is_finite() || end <= start || end <= 0.0 {
            return Peak::default();
        }
        let len = self.levels[0].len();
        let mut left = ((start.max(0.0) * RATE / BLOCK as f64).floor() as usize).min(len);
        let right = ((end.max(0.0) * RATE / BLOCK as f64).ceil() as usize).min(len);
        let mut peak = Peak::default();
        while left < right {
            let remaining_level = (right - left).ilog2() as usize;
            let aligned_level = if left == 0 {
                remaining_level
            } else {
                left.trailing_zeros() as usize
            };
            let level = aligned_level
                .min(remaining_level)
                .min(self.levels.len() - 1);
            peak = peak.merge(self.levels[level][left >> level]);
            left += 1 << level;
        }
        peak
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 对照逐桶穷举结果验证多级查询、反相双声道、瞬态、尾部及缩放/裁剪/滚动源时间。
    #[test]
    fn peaks_preserve_transients_and_source_time_at_every_zoom() {
        let samples = (0..48_000 * 2).flat_map(|frame| {
            let amplitude = if frame == 36_005 {
                0.95
            } else if frame < 24_000 {
                0.1
            } else {
                0.6
            };
            [amplitude, -amplitude]
        });
        let waveform = Waveform::from_pcm(samples);
        for left in 0..40 {
            for right in left + 1..70 {
                let expected = waveform.levels[0][left..right]
                    .iter()
                    .fold(Peak::default(), |a, b| a.merge(*b));
                assert_eq!(
                    waveform.peak(
                        left as f64 * BLOCK as f64 / RATE,
                        (right as f64 - 0.1) * BLOCK as f64 / RATE
                    ),
                    expected
                );
            }
        }
        assert_eq!(
            waveform.peak(0.0, 2.0),
            Peak {
                min: -0.95,
                max: 0.95
            }
        );
        for pps in [10.0, 120.0, 1800.0] {
            let source_in = 0.25;
            let scroll = 0.4 * pps;
            let x = (0.75 - source_in) * pps - scroll;
            let start = source_in + (scroll + x) / pps;
            assert_eq!(waveform.peak(start, start + 1.0 / pps).max, 0.95);
            assert_eq!(waveform.peak(0.1, 0.1 + 1.0 / pps).max, 0.1);
        }
        assert_eq!(waveform.peak(2.0, 3.0), Peak::default());
        assert_eq!(waveform.peak(f64::NAN, 1.0), Peak::default());
        assert_eq!(waveform.peak(1.0, 0.0), Peak::default());
        let tail = Waveform::from_pcm([0.3, -0.4].into_iter());
        assert_eq!(
            tail.peak(0.0, 1.0),
            Peak {
                min: -0.4,
                max: 0.3
            }
        );
    }
}
