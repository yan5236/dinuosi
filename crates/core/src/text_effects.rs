//! 可编辑文字投影与圆角底色，处理直通 Alpha，不改变正文排版或几何中心。
use crate::{text::TextStyle, text_render::over};

/// 根据背景内边距、阴影偏移及模糊计算对称外留白，保证效果不被文字栅格边界截断。
pub(crate) fn margin(style: &TextStyle) -> u32 {
    style
        .background
        .as_ref()
        .map_or(0, |b| b.padding)
        .max(style.shadow.as_ref().map_or(0, |s| {
            s.offset[0].unsigned_abs().max(s.offset[1].unsigned_abs()) + s.blur
        }))
}

/// 在文字下依次合成圆角背景和柔化投影，再覆盖描边/正文；没有效果时直接返回原缓冲。
/// 空正文保持透明，不创建整屏底色；缓冲尺寸由调用方校验，所有偏移都限制在实际范围内。
pub(crate) fn apply(foreground: Vec<u8>, size: (u32, u32), style: &TextStyle) -> Vec<u8> {
    if style.background.is_none() && style.shadow.is_none() {
        return foreground;
    }
    let (w, h) = (size.0 as usize, size.1 as usize);
    let mask: Vec<_> = foreground.as_chunks::<4>().0.iter().map(|p| p[3]).collect();
    let mut bounds = (w, h, 0, 0);
    for (i, &a) in mask.iter().enumerate().filter(|(_, a)| **a > 0) {
        let _ = a;
        bounds.0 = bounds.0.min(i % w);
        bounds.1 = bounds.1.min(i / w);
        bounds.2 = bounds.2.max(i % w + 1);
        bounds.3 = bounds.3.max(i / w + 1);
    }
    if bounds.0 >= bounds.2 {
        return foreground;
    }
    let mut result = vec![0; foreground.len()];
    if let Some(b) = &style.background {
        let pad = b.padding as usize;
        let (left, top, right, bottom) = (
            bounds.0.saturating_sub(pad),
            bounds.1.saturating_sub(pad),
            (bounds.2 + pad).min(w),
            (bounds.3 + pad).min(h),
        );
        let radius = (b.radius as f64)
            .min((right - left) as f64 / 2.0)
            .min((bottom - top) as f64 / 2.0);
        for y in top..bottom {
            for x in left..right {
                let dx = (x as f64 + 0.5 - (left as f64 + radius))
                    .min(0.0)
                    .abs()
                    .max((x as f64 + 0.5 - (right as f64 - radius)).max(0.0));
                let dy = (y as f64 + 0.5 - (top as f64 + radius))
                    .min(0.0)
                    .abs()
                    .max((y as f64 + 0.5 - (bottom as f64 - radius)).max(0.0));
                let coverage = if radius == 0.0 {
                    1.0
                } else {
                    (radius + 0.5 - dx.hypot(dy)).clamp(0.0, 1.0)
                };
                let pixel = color(b.color, coverage * b.opacity);
                let i = (y * w + x) * 4;
                result[i..i + 4].copy_from_slice(&pixel);
            }
        }
    }
    if let Some(s) = &style.shadow {
        let blurred = blur(&mask, w, h, s.blur as usize);
        for y in 0..h {
            for x in 0..w {
                let (sx, sy) = (x as i32 - s.offset[0], y as i32 - s.offset[1]);
                if sx >= 0 && sy >= 0 && sx < w as i32 && sy < h as i32 {
                    let i = (y * w + x) * 4;
                    over(
                        &mut result[i..i + 4],
                        color(
                            s.color,
                            blurred[sy as usize * w + sx as usize] as f64 / 255.0 * s.opacity,
                        ),
                    );
                }
            }
        }
    }
    for (p, f) in result
        .as_chunks_mut::<4>()
        .0
        .iter_mut()
        .zip(foreground.as_chunks::<4>().0)
    {
        over(p, *f);
    }
    result
}

/// 将合法 RGB 与单位透明度转换为直通 BGRA，供背景和投影共用。
fn color(rgb: u32, opacity: f64) -> [u8; 4] {
    [
        rgb as u8,
        (rgb >> 8) as u8,
        (rgb >> 16) as u8,
        (opacity * 255.0).round() as u8,
    ]
}

/// 用两个滑动窗口执行可分离盒式模糊，复杂度与半径无关，画布外按透明处理。
fn blur(mask: &[u8], width: usize, height: usize, radius: usize) -> Vec<u8> {
    if radius == 0 {
        return mask.to_vec();
    }
    let mut horizontal = vec![0; mask.len()];
    let mut output = vec![0; mask.len()];
    let span = (radius * 2 + 1) as u32;
    for y in 0..height {
        let mut sum: u32 = mask[y * width..y * width + (radius + 1).min(width)]
            .iter()
            .map(|&a| a as u32)
            .sum();
        for x in 0..width {
            horizontal[y * width + x] = (sum / span) as u8;
            if x >= radius {
                sum -= mask[y * width + x - radius] as u32;
            }
            if x + radius + 1 < width {
                sum += mask[y * width + x + radius + 1] as u32;
            }
        }
    }
    for x in 0..width {
        let mut sum: u32 = (0..(radius + 1).min(height))
            .map(|y| horizontal[y * width + x] as u32)
            .sum();
        for y in 0..height {
            output[y * width + x] = (sum / span) as u8;
            if y >= radius {
                sum -= horizontal[(y - radius) * width + x] as u32;
            }
            if y + radius + 1 < height {
                sum += horizontal[(y + radius + 1) * width + x] as u32;
            }
        }
    }
    output
}
