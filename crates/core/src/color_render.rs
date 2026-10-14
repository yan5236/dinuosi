//! 预览/导出共享的 SDR 浮点调色，源 alpha 与透明补边保持不变。
use crate::color::{Grade, Stage};
use std::borrow::Cow;

pub const HUES: [f64; 8] = [0.0, 30.0, 60.0, 120.0, 180.0, 240.0, 270.0, 300.0];

/// 返回显示 RGB 的 Rec.709 加权亮度；用于 SDR 分区、饱和度与波形，不是 HDR 场景亮度。
pub fn luma(rgb: [f64; 3]) -> f64 {
    rgb[0] * 0.2126 + rgb[1] * 0.7152 + rgb[2] * 0.0722
}

/// 将显示 RGB 转换为环状色相（度）、饱和度、亮度；无色像素色相记为零。
pub fn to_hsl(rgb: [f64; 3]) -> [f64; 3] {
    let [r, g, b] = rgb;
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let delta = max - min;
    let light = (max + min) / 2.0;
    if delta <= 1e-12 {
        return [0.0, 0.0, light];
    }
    let h = if max == r {
        (g - b) / delta
    } else if max == g {
        (b - r) / delta + 2.0
    } else {
        (r - g) / delta + 4.0
    };
    [
        (h * 60.0).rem_euclid(360.0),
        delta / (1.0 - (2.0 * light - 1.0).abs()).max(1e-12),
        light,
    ]
}
/// 从 HSL 生成有界显示 RGB；色相跨 0/360 度连续，灰度不带入任意色偏。
pub fn from_hsl([h, s, l]: [f64; 3]) -> [f64; 3] {
    // 无色时直接返回同一亮度，避免浮点残差让量化后的 RGB 相差一个灰阶。
    if s <= 1e-12 {
        return [l.clamp(0.0, 1.0); 3];
    }
    let h = h.rem_euclid(360.0) / 60.0;
    let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let x = c * (1.0 - (h % 2.0 - 1.0).abs());
    let m = l - c / 2.0;
    let rgb = match h as usize {
        0 => [c, x, 0.0],
        1 => [x, c, 0.0],
        2 => [0.0, c, x],
        3 => [0.0, x, c],
        4 => [x, 0.0, c],
        _ => [c, 0.0, x],
    };
    rgb.map(|v| (v + m).clamp(0.0, 1.0))
}
/// 计算环状色相距离，红色在 359/1 度之间不会被分成两个选区。
fn hue_distance(a: f64, b: f64) -> f64 {
    let d = (a - b).abs().rem_euclid(360.0);
    d.min(360.0 - d)
}

/// 计算曲线分段线性映射；调用方先校验点序列，输入夹紧，端点外使用端点值。
pub fn curve(points: &[[f64; 2]], x: f64) -> f64 {
    let x = x.clamp(0.0, 1.0);
    let i = points
        .partition_point(|p| p[0] < x)
        .clamp(1, points.len() - 1);
    let [a, b] = [points[i - 1], points[i]];
    a[1] + (b[1] - a[1]) * ((x - a[0]) / (b[0] - a[0])).clamp(0.0, 1.0)
}
/// 生成平滑阶跃权重，边缘宽度为零时产生明确阈值，不除零。
fn smooth(a: f64, b: f64, x: f64) -> f64 {
    if b <= a {
        return f64::from(x >= b);
    }
    let t = ((x - a) / (b - a)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// 计算素材归一化空间与输入颜色的选区覆盖率；aspect 为有效源画面宽高比，旋转在实际等距坐标内执行。
/// 蒙版反转只影响空间，双选区相乘；非法宽高比返回零覆盖，不产生非有限像素。
pub fn coverage(s: &Stage, x: f64, y: f64, rgb: [f64; 3], aspect: f64) -> f64 {
    if !aspect.is_finite() || aspect <= 0.0 {
        return 0.0;
    }
    let mut weight = 1.0;
    if s.mask.enabled {
        let [cx, cy, w, h, angle, feather] = s.mask.values;
        let (sin, cos) = angle.to_radians().sin_cos();
        let dx = (x - cx) * aspect;
        let dy = y - cy;
        let ux = (dx * cos + dy * sin) / (w * aspect / 2.0);
        let uy = (-dx * sin + dy * cos) / (h / 2.0);
        let distance = if s.mask.ellipse {
            ux.hypot(uy)
        } else {
            ux.abs().max(uy.abs())
        };
        let mask = if feather == 0.0 {
            f64::from(distance <= 1.0)
        } else {
            1.0 - smooth(1.0 - feather, 1.0, distance)
        };
        weight *= if s.mask.inverted { 1.0 - mask } else { mask };
    }
    if s.qualifier.enabled {
        let [center, width, soft, sat, min, max] = s.qualifier.values;
        let [h, saturation, _] = to_hsl(rgb);
        let light = luma(rgb);
        let hue = if soft == 0.0 {
            f64::from(hue_distance(h, center) <= width)
        } else {
            1.0 - smooth(width, width + soft, hue_distance(h, center))
        };
        weight *= hue
            * smooth(sat, (sat + 0.05).min(1.0), saturation)
            * f64::from(light >= min && light <= max);
    }
    weight
}
struct Prepared<'a> {
    stage: &'a Stage,
    basic: [Vec<f64>; 3],
    curves: [Vec<f64>; 4],
    hsl: bool,
    wheels: bool,
}
impl<'a> Prepared<'a> {
    /// 在每次帧处理前预计算一维映射，避免每像素进行曝光幂运算和曲线查点。
    fn new(stage: &'a Stage) -> Self {
        Self {
            stage,
            basic: std::array::from_fn(|channel| {
                (0..=1024)
                    .map(|i| basic(stage, i as f64 / 1024.0, channel))
                    .collect()
            }),
            curves: std::array::from_fn(|c| {
                (0..=1024)
                    .map(|i| curve(&stage.curves[c], i as f64 / 1024.0))
                    .collect()
            }),
            hsl: stage.hsl != [[0.0; 3]; 8]
                || stage.picked.as_ref().is_some_and(|p| p.values != [0.0; 3]),
            wheels: stage.wheels != [[0.0; 3]; 3],
        }
    }
    /// 按固定顺序校正一个像素，选区读取步骤输入色；步骤间保持浮点，最终才量化。
    fn pixel(&self, input: [f64; 3], x: f64, y: f64, aspect: f64) -> [f64; 3] {
        let s = self.stage;
        let weight = coverage(s, x, y, input, aspect);
        if weight == 0.0 {
            return input;
        }
        let mut rgb = std::array::from_fn(|i| lookup(&self.basic[i], input[i]));
        if self.wheels {
            let light = luma(rgb);
            let dark = 1.0 - smooth(0.0, 0.65, light);
            let high = smooth(0.35, 1.0, light);
            let mid = 1.0 - dark - high;
            for (w, part) in s.wheels.iter().zip([dark, mid, high]) {
                let [x, y, z] = w.map(|v| v / 100.0);
                let tint = [x, -x * 0.5 - y * 0.866, -x * 0.5 + y * 0.866];
                for i in 0..3 {
                    rgb[i] = (rgb[i] + (tint[i] * 0.25 + z * 0.25) * part).clamp(0.0, 1.0);
                }
            }
        }
        if s.basic[4] != 0.0 || s.basic[5] != 0.0 {
            let lum = luma(rgb);
            let spread = rgb.iter().copied().reduce(f64::max).unwrap()
                - rgb.iter().copied().reduce(f64::min).unwrap();
            let amount = (1.0 + s.basic[4] / 100.0) * (1.0 + s.basic[5] / 100.0 * (1.0 - spread));
            rgb = rgb.map(|v| (lum + (v - lum) * amount).clamp(0.0, 1.0));
        }
        if self.hsl {
            let [h, sat, light] = to_hsl(rgb);
            let mut delta = [0.0; 3];
            let mut total = 0.0;
            for (index, center) in HUES.iter().enumerate() {
                let w = 1.0 - smooth(0.0, 60.0, hue_distance(h, *center));
                total += w;
                for (i, d) in delta.iter_mut().enumerate() {
                    *d += s.hsl[index][i] * w;
                }
            }
            if total > 0.0 && sat > 0.0 {
                delta = delta.map(|v| v / total);
                if let Some(picked) = &s.picked {
                    let weight = 1.0 - smooth(0.0, 60.0, hue_distance(h, picked.hue));
                    for (i, d) in delta.iter_mut().enumerate() {
                        *d += picked.values[i] * weight;
                    }
                }
                rgb = from_hsl([
                    h + delta[0] * 0.6,
                    (sat * (1.0 + delta[1] / 100.0)).clamp(0.0, 1.0),
                    (light + delta[2] / 100.0 * 0.5 * sat).clamp(0.0, 1.0),
                ]);
            }
        }
        rgb = std::array::from_fn(|i| lookup(&self.curves[i + 1], lookup(&self.curves[0], rgb[i])));
        // 全覆盖直接使用校正结果，避免抵消运算在半灰量化边界制造单通道误差。
        if weight == 1.0 {
            rgb
        } else {
            std::array::from_fn(|i| input[i] + (rgb[i] - input[i]) * weight)
        }
    }
}
/// 对归一化查找表做线性插值，限制到 SDR 域，减少表量化带来的色阶误差。
fn lookup(table: &[f64], v: f64) -> f64 {
    let x = v.clamp(0.0, 1.0) * 1024.0;
    let i = (x as usize).min(1023);
    table[i] + (table[i + 1] - table[i]) * (x - i as f64)
}
/// 预计算白平衡、线性曝光、对比度及柔和明暗分区；sRGB 传递函数与文档契约一致。
fn basic(s: &Stage, v: f64, channel: usize) -> f64 {
    let b = s.basic;
    let linear = if v <= 0.04045 {
        v / 12.92
    } else {
        ((v + 0.055) / 1.055).powf(2.4)
    };
    let white = [
        b[2] * 0.003 - b[3] * 0.0015,
        b[3] * 0.003,
        -b[2] * 0.003 - b[3] * 0.0015,
    ][channel];
    let linear = linear * 2.0_f64.powf(b[0] + white);
    let mut out = if linear <= 0.0031308 {
        linear * 12.92
    } else {
        1.055 * linear.powf(1.0 / 2.4) - 0.055
    };
    out = (out - 0.5) * 2.0_f64.powf(b[1] / 100.0) + 0.5;
    let l = out.clamp(0.0, 1.0);
    out += (b[6] / 100.0 * smooth(0.4, 1.0, l) + b[7] / 100.0 * (1.0 - smooth(0.0, 0.6, l)))
        * l
        * (1.0 - l);
    out += b[8] / 100.0 * l * l * 0.25 + b[9] / 100.0 * (1.0 - l).powi(2) * 0.25;
    out.clamp(0.0, 1.0)
}
/// 调色任意尺寸 BGRA 源缓冲；content 为居中有效素材尺寸，排除预览补边。
/// 拒绝非法参数/尺寸；中性直接借用输入，其余只分配一份输出，透明像素与 alpha 保持原值。
pub fn apply<'a>(
    source: &'a [u8],
    size: (u32, u32),
    content: (u32, u32),
    grade: &Grade,
) -> Result<Cow<'a, [u8]>, String> {
    grade.validate()?;
    if size.0 == 0
        || size.1 == 0
        || content.0 == 0
        || content.1 == 0
        || content.0 > size.0
        || content.1 > size.1
        || (size.0 as usize)
            .checked_mul(size.1 as usize)
            .and_then(|n| n.checked_mul(4))
            != Some(source.len())
    {
        return Err("调色像素尺寸无效".into());
    }
    if grade.neutral() {
        return Ok(Cow::Borrowed(source));
    }
    let stages: Vec<_> = grade
        .stages
        .iter()
        .filter(|s| !s.neutral())
        .map(Prepared::new)
        .collect();
    let mut output = source.to_vec();
    let left = (size.0 - content.0) / 2;
    let top = (size.1 - content.1) / 2;
    for y in top..top + content.1 {
        for x in left..left + content.0 {
            let i = ((y * size.0 + x) * 4) as usize;
            if source[i + 3] == 0 {
                continue;
            }
            let mut rgb = [source[i + 2], source[i + 1], source[i]].map(|v| v as f64 / 255.0);
            for s in &stages {
                rgb = s.pixel(
                    rgb,
                    (x - left) as f64 / content.0 as f64 + 0.5 / content.0 as f64,
                    (y - top) as f64 / content.1 as f64 + 0.5 / content.1 as f64,
                    content.0 as f64 / content.1 as f64,
                );
            }
            for c in 0..3 {
                output[i + 2 - c] = (rgb[c].clamp(0.0, 1.0) * 255.0).round() as u8;
            }
        }
    }
    Ok(Cow::Owned(output))
}
