//! 预览合成的像素变换：项目中心为 0,0，X 向右、Y 向下，按轨道从底到顶混合。
use crate::{
    media::{self, FRAME_HEIGHT, FRAME_WIDTH, MediaKind},
    timeline::{Timeline, Transform},
};
use std::{path::PathBuf, sync::Arc};

/// 仅保留当前可见视频的源帧；参数变换不触发解码，寻址、离线及重新载入素材自动失效。
/// 缓存随预览会话持有并交给串行后台任务，不使用全局状态或跨线程锁。
#[derive(Default)]
pub struct FrameCache {
    frames: Vec<CachedFrame>,
}

struct CachedFrame {
    path: PathBuf,
    frame: i64,
    fps: u32,
    generation: Option<Arc<Vec<u8>>>,
    bytes: Arc<Vec<u8>>,
}

/// 将固定大小的 BGRA 源画面按项目坐标变换并混合到输出；无效长度返回错误，不越界读取。
/// 透明度、缩放和角度由核心约束，坐标从项目像素换算到预览像素，零缩放不绘制。
/// 完全透明像素跳过，完全不透明像素直接覆盖，避免常见视频/图片逐通道浮点混合。
pub fn blend(
    output: &mut [u8],
    source: &[u8],
    t: &Transform,
    canvas: (u32, u32),
) -> Result<(), String> {
    blend_mapped(
        output,
        source,
        t,
        canvas,
        (canvas.0 as f64, canvas.1 as f64),
        (FRAME_WIDTH, FRAME_HEIGHT),
    )
}

/// 在项目像素坐标内逆变换采样，输出仍使用固定缓冲；有效项目矩形外保持黑色。
/// source_extent 为整个带透明边源缓冲在项目中的覆盖范围，支持小素材不放大。
fn blend_mapped(
    output: &mut [u8],
    source: &[u8],
    t: &Transform,
    canvas: (u32, u32),
    source_extent: (f64, f64),
    output_size: (u32, u32),
) -> Result<(), String> {
    blend_region(
        output,
        source,
        t,
        canvas,
        source_extent,
        ((FRAME_WIDTH, FRAME_HEIGHT), (FRAME_WIDTH, FRAME_HEIGHT)),
        output_size,
    )
}

/// 将任意尺寸 BGRA 素材按预览相同的项目像素变换合成至完整输出画布。
/// source_extent 是源缓冲在项目中的覆盖尺寸；输出比例不同时居中适配项目画布并保留黑边。
/// 输出尺寸可以独立于项目尺寸，错误不越界采样，不将非同宽高比的项目强行拉伸。
pub fn blend_pixels(
    output: &mut [u8],
    output_size: (u32, u32),
    source: &[u8],
    source_size: (u32, u32),
    t: &Transform,
    canvas: (u32, u32),
    source_extent: (f64, f64),
) -> Result<(), String> {
    let fit = (output_size.0 as f64 / canvas.0.max(1) as f64)
        .min(output_size.1 as f64 / canvas.1.max(1) as f64);
    let region = (
        (canvas.0 as f64 * fit).round() as u32,
        (canvas.1 as f64 * fit).round() as u32,
    );
    blend_region(
        output,
        source,
        t,
        canvas,
        source_extent,
        (output_size, source_size),
        region,
    )
}

/// 在输出中心矩形内使用统一逆变换算法采样；预览补边和原尺寸导出共用透明度与旋转语义。
/// sizes 分别为输出和源缓冲尺寸，region 为有效项目矩形；拒绝无效长度和矩形。
fn blend_region(
    output: &mut [u8],
    source: &[u8],
    t: &Transform,
    canvas: (u32, u32),
    source_extent: (f64, f64),
    sizes: ((u32, u32), (u32, u32)),
    region: (u32, u32),
) -> Result<(), String> {
    let (output_size, source_size) = sizes;
    if (output_size.0 as usize)
        .checked_mul(output_size.1 as usize)
        .and_then(|pixels| pixels.checked_mul(4))
        != Some(output.len())
        || (source_size.0 as usize)
            .checked_mul(source_size.1 as usize)
            .and_then(|pixels| pixels.checked_mul(4))
            != Some(source.len())
        || source_size.0 == 0
        || source_size.1 == 0
        || canvas.0 == 0
        || canvas.1 == 0
        || region.0 == 0
        || region.1 == 0
        || region.0 > output_size.0
        || region.1 > output_size.1
        || !source_extent.0.is_finite()
        || source_extent.0 <= 0.0
        || !source_extent.1.is_finite()
        || source_extent.1 <= 0.0
    {
        return Err("预览像素尺寸无效".into());
    }
    if t.scale <= 0.0 {
        return Ok(());
    }
    let angle = t.rotation.to_radians();
    let (sin, cos) = angle.sin_cos();
    let scale = t.scale / 100.0;
    let left = (output_size.0 - region.0) / 2;
    let top = (output_size.1 - region.1) / 2;
    for y in top..top + region.1 {
        for x in left..left + region.0 {
            let dx = (x as f64 + 0.5 - left as f64 - region.0 as f64 / 2.0) * canvas.0 as f64
                / region.0 as f64
                - t.x;
            let dy = (y as f64 + 0.5 - top as f64 - region.1 as f64 / 2.0) * canvas.1 as f64
                / region.1 as f64
                - t.y;
            let sx = ((dx * cos + dy * sin) / scale * source_size.0 as f64 / source_extent.0
                + source_size.0 as f64 / 2.0)
                .floor() as i32;
            let sy = ((-dx * sin + dy * cos) / scale * source_size.1 as f64 / source_extent.1
                + source_size.1 as f64 / 2.0)
                .floor() as i32;
            if sx < 0 || sy < 0 || sx >= source_size.0 as i32 || sy >= source_size.1 as i32 {
                continue;
            }
            let i = ((y * output_size.0 + x) * 4) as usize;
            let j = ((sy as u32 * source_size.0 + sx as u32) * 4) as usize;
            if source[j + 3] == 0 || t.opacity == 0.0 {
                output[i + 3] = 255;
                continue;
            }
            if source[j + 3] == 255 && t.opacity == 100.0 {
                output[i..i + 4].copy_from_slice(&source[j..j + 4]);
                continue;
            }
            let a = source[j + 3] as f64 / 255.0 * t.opacity / 100.0;
            for channel in 0..3 {
                output[i + channel] = (source[j + channel] as f64 * a
                    + output[i + channel] as f64 * (1.0 - a))
                    .round() as u8;
            }
            output[i + 3] = 255;
        }
    }
    Ok(())
}

#[cfg(test)]
#[path = "composite_tests.rs"]
mod tests;

/// 一次性寻址并合成；持续预览应持有 FrameCache 复用解码结果。
/// 调用方应在后台执行，解码错误原样返回；输出固定 640×360 BGRA，不依赖 GUI。
pub fn render(timeline: &Timeline, frame: i64, media: &[media::Media]) -> Result<Vec<u8>, String> {
    FrameCache::default().render(timeline, frame, media)
}

impl FrameCache {
    /// 按轨道从底到顶合成，视频按路径、源帧、帧率及素材载入代次复用，图片直接借用缩略图。
    /// 小素材保持原始显示尺寸，大素材等比缩小到项目画布；未覆盖区域为黑色。
    /// 每次只保留当前可见源帧，避免播放/寻址累积内存；失败不缓存错误，下一次允许重试。
    pub fn render(
        &mut self,
        timeline: &Timeline,
        frame: i64,
        media: &[media::Media],
    ) -> Result<Vec<u8>, String> {
        let mut previous = std::mem::take(&mut self.frames);
        let mut out = vec![0; (FRAME_WIDTH * FRAME_HEIGHT * 4) as usize];
        for p in out.as_chunks_mut::<4>().0 {
            p[3] = 255;
        }
        for track in (0..timeline.tracks.len()).rev() {
            if !timeline.tracks[track].visible {
                continue;
            }
            for clip in timeline
                .clips
                .iter()
                .filter(|c| c.track == track && frame >= c.start && frame < c.start + c.duration)
            {
                let Some(m) = media
                    .iter()
                    .find(|m| m.path == clip.source)
                    .cloned()
                    .or_else(|| crate::media::probe(&clip.source).ok())
                    .filter(|m| !m.offline && m.kind != MediaKind::Audio)
                else {
                    continue;
                };
                let bytes = if m.kind == MediaKind::Image {
                    m.thumbnail.as_ref().cloned().ok_or("图片没有可用画面")?
                } else {
                    let source_frame = clip.source_in + frame - clip.start;
                    let matches = |cached: &&CachedFrame| {
                        cached.path == m.path
                            && cached.frame == source_frame
                            && cached.fps == timeline.fps
                            && match (&cached.generation, &m.thumbnail) {
                                (Some(a), Some(b)) => Arc::ptr_eq(a, b),
                                (None, None) => true,
                                _ => false,
                            }
                    };
                    if let Some(cached) = self.frames.iter().find(matches) {
                        cached.bytes.clone()
                    } else {
                        let bytes = match previous.iter().position(|c| matches(&c)) {
                            Some(index) => previous.swap_remove(index).bytes,
                            None => Arc::new(media::frame(
                                &m.path,
                                source_frame as f64 / timeline.fps as f64,
                            )?),
                        };
                        self.frames.push(CachedFrame {
                            path: m.path.clone(),
                            frame: source_frame,
                            fps: timeline.fps,
                            generation: m.thumbnail.clone(),
                            bytes: bytes.clone(),
                        });
                        bytes
                    }
                };
                let fitted = fitted_size(m.source_size, (timeline.width, timeline.height));
                blend_mapped(
                    &mut out,
                    &bytes,
                    &clip.transform_at(frame as f64),
                    (timeline.width, timeline.height),
                    (
                        fitted.0 * FRAME_WIDTH as f64 / m.preview_size.0.max(1) as f64,
                        fitted.1 * FRAME_HEIGHT as f64 / m.preview_size.1.max(1) as f64,
                    ),
                    media::preview_dimensions((timeline.width, timeline.height)),
                )?;
            }
        }
        Ok(out)
    }
}

/// 返回素材在项目中的初始显示大小：仅缩小超出画布的素材，小素材不放大。
pub fn fitted_size(source: (u32, u32), canvas: (u32, u32)) -> (f64, f64) {
    let scale = (canvas.0 as f64 / source.0.max(1) as f64)
        .min(canvas.1 as f64 / source.1.max(1) as f64)
        .min(1.0);
    (
        source.0.max(1) as f64 * scale,
        source.1.max(1) as f64 * scale,
    )
}
