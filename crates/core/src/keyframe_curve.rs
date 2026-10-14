//! 参数曲线的贝塞尔切线与共享求值；横向长度按相邻节点间距归一化，裁剪和换帧率无需改写。
use crate::keyframes::Keyframe;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct KeyframeCurve {
    /// [相邻时间间距的比例, 相对节点的参数差值]；入切线向左，出切线向右。
    pub incoming: Option<[f64; 2]>,
    pub outgoing: Option<[f64; 2]>,
    pub linked: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Easing {
    Linear,
    EaseIn,
    EaseOut,
    EaseInOut,
}

/// 计算三次贝塞尔标量；端点与控制值均由调用方提供，不修改模型。
fn cubic(a: f64, b: f64, c: f64, d: f64, t: f64) -> f64 {
    let u = 1.0 - t;
    u * u * u * a + 3.0 * u * u * t * b + 3.0 * u * t * t * c + t * t * t * d
}

/// 按真实时间反解贝塞尔参数再求值；缺省切线等价线性，不把时间比例误当作贝塞尔参数。
/// 二分处理垂直/零长切线，支持常量段和过冲；业务范围由 Clip 的共享求值入口夹紧。
pub(crate) fn interpolate(a: &Keyframe, b: &Keyframe, component: usize, time: f64) -> f64 {
    let delta = b.values[component] - a.values[component];
    let out = a.curves[component].and_then(|c| c.outgoing);
    let input = b.curves[component].and_then(|c| c.incoming);
    if out.is_none() && input.is_none() {
        return a.values[component] + delta * time;
    }
    let out = out.unwrap_or([1.0 / 3.0, delta / 3.0]);
    let input = input.unwrap_or([1.0 / 3.0, -delta / 3.0]);
    let parameter = parameter_at(time, out[0], 1.0 - input[0]);
    cubic(
        a.values[component],
        a.values[component] + out[1],
        b.values[component] + input[1],
        b.values[component],
        parameter,
    )
}

/// 对归一化单调时间曲线二分求参数；时间端点直接返回，避免切分产生极小负长度。
fn parameter_at(time: f64, first: f64, second: f64) -> f64 {
    if time <= 0.0 {
        return 0.0;
    }
    if time >= 1.0 {
        return 1.0;
    }
    let (mut low, mut high) = (0.0, 1.0);
    for _ in 0..36 {
        let middle = (low + high) * 0.5;
        if cubic(0.0, first, second, 1.0, middle) < time {
            low = middle;
        } else {
            high = middle;
        }
    }
    (low + high) * 0.5
}

/// 用 De Casteljau 拆分当前区间，插入采样节点时保留原曲线；自动改值沿用拆分后的局部切线。
/// 仅改变相邻节点相接的一侧，其他分量/切线联动模式保留；无区间或线性区间不新增曲线数据。
pub(crate) fn split_for_key(
    clip: &mut crate::timeline::Clip,
    kind: crate::keyframes::KeyframeKind,
    frame: i64,
) -> [Option<KeyframeCurve>; 2] {
    let mut curves = [None; 2];
    let Some(a) = clip.curve_neighbor(kind, frame, true) else {
        return curves;
    };
    let Some(b) = clip.curve_neighbor(kind, frame, false) else {
        return curves;
    };
    let ai = clip
        .keyframes
        .iter()
        .position(|k| k.kind == kind && k.frame == a.frame)
        .unwrap();
    let bi = clip
        .keyframes
        .iter()
        .position(|k| k.kind == kind && k.frame == b.frame)
        .unwrap();
    let time = (frame - a.frame) as f64 / (b.frame - a.frame) as f64;
    for (i, target) in curves
        .iter_mut()
        .enumerate()
        .take(1 + usize::from(kind == crate::keyframes::KeyframeKind::Position))
    {
        let a = &clip.keyframes[ai];
        let b = &clip.keyframes[bi];
        if a.curves[i].and_then(|c| c.outgoing).is_none()
            && b.curves[i].and_then(|c| c.incoming).is_none()
        {
            continue;
        }
        let out = clip.curve_handle(kind, i, a.frame, false).unwrap();
        let input = clip.curve_handle(kind, i, b.frame, true).unwrap();
        let t = parameter_at(time, out[0], 1.0 - input[0]);
        let mix = |a: [f64; 2], b: [f64; 2]| [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t];
        let p = [
            [0.0, a.values[i]],
            [out[0], a.values[i] + out[1]],
            [1.0 - input[0], b.values[i] + input[1]],
            [1.0, b.values[i]],
        ];
        let q = [mix(p[0], p[1]), mix(p[1], p[2]), mix(p[2], p[3])];
        let r = [mix(q[0], q[1]), mix(q[1], q[2])];
        let s = mix(r[0], r[1]);
        let a_handle = [q[0][0] / time, q[0][1] - p[0][1]];
        let b_handle = [(1.0 - q[2][0]) / (1.0 - time), q[2][1] - p[3][1]];
        let incoming = [(time - r[0][0]) / time, r[0][1] - s[1]];
        let outgoing = [(r[1][0] - time) / (1.0 - time), r[1][1] - s[1]];
        clip.keyframes[ai].curves[i]
            .get_or_insert_with(KeyframeCurve::default)
            .outgoing = Some([a_handle[0].clamp(0.0, 1.0), a_handle[1]]);
        clip.keyframes[bi].curves[i]
            .get_or_insert_with(KeyframeCurve::default)
            .incoming = Some([b_handle[0].clamp(0.0, 1.0), b_handle[1]]);
        *target = Some(KeyframeCurve {
            incoming: Some([incoming[0].clamp(0.0, 1.0), incoming[1]]),
            outgoing: Some([outgoing[0].clamp(0.0, 1.0), outgoing[1]]),
            linked: true,
        });
    }
    curves
}

impl KeyframeCurve {
    /// 拒绝外部文件中的非有限切线、逆向时间和过大数值；零长切线允许且不会造成除零。
    pub(crate) fn validate(self, limit: f64) -> Result<(), String> {
        for [time, value] in [self.incoming, self.outgoing].into_iter().flatten() {
            if !time.is_finite()
                || !(0.0..=1.0).contains(&time)
                || !value.is_finite()
                || value.abs() > limit
            {
                return Err("关键帧曲线参数无效".into());
            }
        }
        Ok(())
    }
}
