//! 片段变速的源区间、曲线积分与编辑规则；所有源时间使用项目帧，可保留慢放分割的亚帧边界。
use crate::{
    media::MediaKind,
    timeline::{Clip, Timeline},
};
use std::collections::BTreeSet;

#[derive(Clone, Debug, PartialEq)]
pub struct Speed {
    pub source: [f64; 2],
    pub rate: f64,
    /// 控制点的横坐标为播放方向上的源进度，纵坐标为倍率；空集合表示常规变速。
    pub curve: Vec<[f64; 2]>,
    pub pitch: bool,
}

impl Speed {
    /// 检查持久化边界：源区间有效，常规倍率为 0.1–100，曲线倍率为 0.1–10，控制点严格排序。
    pub(crate) fn validate(&self, frames: i64) -> Result<(), String> {
        if self.source.iter().any(|v| !v.is_finite())
            || self.source[0] < 0.0
            || self.source[1] > frames as f64 + 1e-7
            || self.source[1] <= self.source[0]
            || !self.rate.is_finite()
            || !(0.1..=100.0).contains(&self.rate)
            || (!self.curve.is_empty()
                && (self.curve.len() < 2
                    || self.curve.len() > 64
                    || self.curve[0][0] != 0.0
                    || self.curve.last().unwrap()[0] != 1.0
                    || self.curve.iter().any(|p| {
                        !p[0].is_finite()
                            || !p[1].is_finite()
                            || !(0.0..=1.0).contains(&p[0])
                            || !(0.1..=10.0).contains(&p[1])
                    })
                    || self.curve.windows(2).any(|p| p[0][0] >= p[1][0])))
        {
            return Err("变速参数无效".into());
        }
        Ok(())
    }

    /// 对源进度求瞬时倍率，相邻控制点线性连接；端点外保持首尾倍率，用于裁剪扩展和 PCM 调度。
    pub fn rate_at(&self, progress: f64) -> f64 {
        if self.curve.is_empty() {
            return self.rate;
        }
        for p in self.curve.windows(2) {
            if progress <= p[1][0] {
                let fraction = ((progress - p[0][0]) / (p[1][0] - p[0][0])).clamp(0.0, 1.0);
                return p[0][1] + (p[1][1] - p[0][1]) * fraction;
            }
        }
        self.curve.last().unwrap()[1]
    }

    /// 积分源进度上的倍率倒数，得到单位源长度所需播放时间；直线段使用解析积分避免逐帧累计漂移。
    pub fn integral(&self, progress: f64) -> f64 {
        if progress < 0.0 {
            return progress / self.rate_at(0.0);
        }
        if progress > 1.0 {
            return self.integral(1.0) + (progress - 1.0) / self.rate_at(1.0);
        }
        if self.curve.is_empty() {
            return progress / self.rate;
        }
        let mut sum = 0.0;
        for p in self.curve.windows(2) {
            let width = (progress.min(p[1][0]) - p[0][0]).max(0.0);
            let slope = (p[1][1] - p[0][1]) / (p[1][0] - p[0][0]);
            sum += if slope.abs() < 1e-9 {
                width / p[0][1]
            } else {
                (1.0 + slope * width / p[0][1]).ln() / slope
            };
        }
        sum
    }

    /// 将播放时间归一比例解析反解为源进度；区间外按端点倍率扩展，音频逐采样调用也无需迭代搜索。
    pub fn progress(&self, time: f64) -> f64 {
        let target = time * self.integral(1.0);
        if time < 0.0 {
            return target * self.rate_at(0.0);
        }
        if time > 1.0 {
            return 1.0 + (target - self.integral(1.0)) * self.rate_at(1.0);
        }
        if self.curve.is_empty() {
            return time;
        }
        let mut remaining = target;
        for p in self.curve.windows(2) {
            let width = p[1][0] - p[0][0];
            let slope = (p[1][1] - p[0][1]) / width;
            let span = if slope.abs() < 1e-9 {
                width / p[0][1]
            } else {
                (p[1][1] / p[0][1]).ln() / slope
            };
            if remaining <= span {
                let distance = if slope.abs() < 1e-9 {
                    remaining * p[0][1]
                } else {
                    (remaining * slope).exp_m1() * p[0][1] / slope
                };
                return (p[0][0] + distance).clamp(p[0][0], p[1][0]);
            }
            remaining -= span;
        }
        1.0
    }

    /// 根据源区间与曲线计算项目帧时长；舍入为至少一帧，保存时仍保留完整源选区。
    pub fn duration(&self) -> i64 {
        ((self.source[1] - self.source[0]) * self.integral(1.0))
            .round()
            .max(1.0) as i64
    }
}

impl Clip {
    /// 返回精确源区间；旧片段沿用入点加时长，变速片段保留裁剪/分割产生的亚帧边界。
    pub fn source_bounds(&self) -> [f64; 2] {
        self.speed.as_ref().map_or(
            [
                self.source_in as f64,
                (self.source_in + self.duration) as f64,
            ],
            |s| s.source,
        )
    }

    /// 将局部项目时间映射为播放方向上的源进度；普通片段为线性映射，变速片段使用曲线积分反解。
    pub fn source_progress(&self, local: f64) -> f64 {
        self.speed
            .as_ref()
            .map_or(local / self.duration as f64, |s| {
                s.progress(local / self.duration as f64)
            })
    }

    /// 将播放方向上的源进度转换为绝对源边界；边界可处于两帧之间，不应用定格。
    pub fn source_boundary(&self, local: f64) -> f64 {
        let [a, b] = self.source_bounds();
        let distance = self.source_progress(local) * (b - a);
        if self.reversed {
            b - distance
        } else {
            a + distance
        }
    }

    /// 截取变速片段的局部时间范围并重映射曲线；支持端点倍率扩展，调用方负责夹紧源边界。
    /// 保留倍率与声音变调，更新源入点、播放时长；不改片段起点与关键帧。
    pub(crate) fn slice_speed(&mut self, from: f64, to: f64) {
        let Some(mut speed) = self.speed.clone() else {
            return;
        };
        let p = self.source_progress(from);
        let q = self.source_progress(to);
        let a = self.source_boundary(from);
        let b = self.source_boundary(to);
        if !speed.curve.is_empty() {
            speed.curve = std::iter::once([0.0, speed.rate_at(p)])
                .chain(
                    speed
                        .curve
                        .iter()
                        .filter(|k| k[0] > p && k[0] < q)
                        .map(|k| [(k[0] - p) / (q - p), k[1]]),
                )
                .chain(std::iter::once([1.0, speed.rate_at(q)]))
                .collect();
        }
        speed.source = [a.min(b).max(0.0), a.max(b).min(self.source_frames as f64)];
        self.source_in = speed.source[0].floor() as i64;
        self.duration = (to - from).round().max(1.0) as i64;
        self.speed = Some(speed);
    }
}

impl Timeline {
    /// 原子修改选中未锁定视频/音频的倍率、曲线与声音变调；保留源选区，调整同轨后续片段并保留间隙。
    /// 图片和未知身份忽略；非法倍率、锁定后续片段、动画帧折叠或时间越界拒绝整次编辑，不改源文件。
    pub fn set_speed(
        &mut self,
        ids: &BTreeSet<String>,
        rate: f64,
        curve: &[[f64; 2]],
        pitch: bool,
    ) -> Result<(), String> {
        let mut next = self.clone();
        let mut indices: Vec<_> = next
            .clips
            .iter()
            .enumerate()
            .filter(|(_, c)| {
                ids.contains(&c.id)
                    && c.kind != MediaKind::Image
                    && !c.locked
                    && !next.tracks[c.track].locked
            })
            .map(|(i, _)| i)
            .collect();
        indices.sort_by_key(|&i| std::cmp::Reverse(next.clips[i].start));
        for index in indices {
            let old = next.clips[index].clone();
            let speed = Speed {
                source: old.source_bounds(),
                rate,
                curve: curve.to_vec(),
                pitch,
            };
            speed.validate(old.source_frames)?;
            if old.kind == MediaKind::Audio && !curve.is_empty() {
                return Err("音频仅支持常规变速".into());
            }
            let duration = speed.duration();
            if duration > 86400 * next.fps as i64 {
                return Err("变速后时长超出一天".into());
            }
            let delta = duration - old.duration;
            for c in next
                .clips
                .iter_mut()
                .filter(|c| c.track == old.track && c.start >= old.start + old.duration)
            {
                if delta != 0 && c.locked {
                    return Err("后续片段已锁定".into());
                }
                c.start += delta;
            }
            let clip = &mut next.clips[index];
            for key in &mut clip.keyframes {
                key.frame =
                    (key.frame as f64 * duration as f64 / old.duration as f64).round() as i64;
            }
            clip.duration = duration;
            clip.speed = Some(speed);
        }
        next.validate()?;
        *self = next;
        Ok(())
    }
}
