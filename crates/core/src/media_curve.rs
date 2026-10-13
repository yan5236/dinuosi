//! 常规与曲线音频共用固定窗 WSOLA：源位置由片段时间映射决定，窗口相位匹配后重叠相加，保留音调。
use super::Pcm;
use std::collections::VecDeque;

const HOP: usize = 512;
const SEARCH: i64 = 256;
const CAPACITY: usize = 4096;

pub(super) struct CurvePcm {
    input: VecDeque<[f32; 2]>,
    base: u64,
    next: u64,
    eof: bool,
    tail: [[f32; 2]; HOP],
    started: bool,
}

impl CurvePcm {
    /// 初始化固定长度源采样缓存和半窗尾部；不依赖原生滤镜，也不为整个素材分配 PCM。
    pub(super) fn new() -> Self {
        Self {
            input: VecDeque::with_capacity(CAPACITY),
            base: 0,
            next: 0,
            eof: false,
            tail: [[0.0; 2]; HOP],
            started: false,
        }
    }

    /// 读取到指定源采样对，保留最近 4096 对；EOF 以零补窗，解码错误仍由原始会话报告。
    fn fill(&mut self, source: &mut Pcm, end: u64) {
        while self.next < end {
            let pair = if self.eof {
                [0.0; 2]
            } else {
                match source.next() {
                    Some(left) => [left, source.next().unwrap_or(0.0)],
                    None => {
                        self.eof = true;
                        [0.0; 2]
                    }
                }
            };
            if self.input.len() == CAPACITY {
                self.input.pop_front();
                self.base += 1;
            }
            self.input.push_back(pair);
            self.next += 1;
        }
    }

    /// 按绝对源采样索引读取缓存，缓存外返回零；调用方先填充所需窗，不会越界访问。
    fn sample(&self, index: u64) -> [f32; 2] {
        index
            .checked_sub(self.base)
            .and_then(|i| self.input.get(i as usize))
            .copied()
            .unwrap_or([0.0; 2])
    }

    /// 计算候选窗与上个尾窗的归一化相关度；hop 是本选区可用的半窗长度，短选区不比较补零部分。
    /// 双声道共用同一偏移，避免左右声道相位不同步。
    /// 每八对抽样一次，控制实时搜索成本；静音窗口返回零分，由最近的预测位置优先。
    fn correlation(&self, start: u64, hop: usize) -> f64 {
        let (mut cross, mut a, mut b) = (0.0, 0.0, 0.0);
        for index in (0..hop).step_by(8) {
            let sample = self.sample(start + index as u64);
            for (channel, value) in sample.iter().enumerate() {
                let left = self.tail[index][channel] as f64;
                let right = *value as f64;
                cross += left * right;
                a += left * left;
                b += right * right;
            }
        }
        if a * b > 1e-16 {
            cross / (a * b).sqrt()
        } else {
            0.0
        }
    }

    /// 输出最多 512 对保持音调的样本；短选区按可用样本缩小半窗，避免高帧率一帧素材周期性静音。
    /// target 来自片段的绝对源时间映射，而不是累计倍率或滤镜时钟。
    /// 仅在预测点前后 256 对内匹配相位；窗口起点夹紧到完整分析窗，避免尾部搜索把声音替换为补零。
    /// 用余弦交叉淡化连接两个窗口，最多保留约 32KiB 源缓存。
    pub(super) fn render(
        &mut self,
        source: &mut Pcm,
        target: f64,
        source_pairs: u64,
        output: &mut [f32; 4096],
    ) -> usize {
        let hop = (source_pairs / 2).clamp(1, HOP as u64) as usize;
        let last_start = source_pairs.saturating_sub(hop as u64 * 2);
        let target = (target.round().max(0.0) as u64).min(last_start);
        self.fill(source, target + SEARCH as u64 + hop as u64 * 2);
        let mut best = target;
        if self.started {
            let mut score = self.correlation(best, hop);
            for distance in (4..=SEARCH).step_by(4) {
                for offset in [-distance, distance] {
                    let candidate = target as i64 + offset;
                    if candidate < self.base as i64
                        || candidate < 0
                        || candidate > last_start as i64
                    {
                        continue;
                    }
                    let candidate = candidate as u64;
                    let value = self.correlation(candidate, hop);
                    if value > score + 1e-6 {
                        score = value;
                        best = candidate;
                    }
                }
            }
        }
        for index in 0..hop {
            let current = self.sample(best + index as u64);
            let ratio = if self.started {
                (1.0 - (std::f32::consts::PI * index as f32 / hop as f32).cos()) * 0.5
            } else {
                1.0
            };
            for channel in 0..2 {
                output[index * 2 + channel] =
                    self.tail[index][channel] * (1.0 - ratio) + current[channel] * ratio;
            }
            self.tail[index] = self.sample(best + index as u64 + hop as u64);
        }
        self.started = true;
        hop * 2
    }
}
