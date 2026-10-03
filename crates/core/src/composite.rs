//! 预览合成的像素变换：项目中心为 0,0，X 向右、Y 向下，按轨道从底到顶混合。
use crate::{
    media::{self, FRAME_HEIGHT, FRAME_WIDTH, MediaKind},
    timeline::{Timeline, Transform},
};

/// 将固定大小的 BGRA 源画面按项目坐标变换并混合到输出；无效长度返回错误，不越界读取。
/// 透明度、缩放和角度由核心约束，坐标从项目像素换算到预览像素，零缩放不绘制。
pub fn blend(
    output: &mut [u8],
    source: &[u8],
    t: &Transform,
    canvas: (u32, u32),
) -> Result<(), String> {
    let len = (FRAME_WIDTH * FRAME_HEIGHT * 4) as usize;
    if output.len() != len || source.len() != len || canvas.0 == 0 || canvas.1 == 0 {
        return Err("预览像素尺寸无效".into());
    }
    if t.scale <= 0.0 {
        return Ok(());
    }
    let angle = t.rotation.to_radians();
    let (sin, cos) = angle.sin_cos();
    let scale = t.scale / 100.0;
    let tx = t.x * FRAME_WIDTH as f64 / canvas.0 as f64;
    let ty = t.y * FRAME_HEIGHT as f64 / canvas.1 as f64;
    for y in 0..FRAME_HEIGHT {
        for x in 0..FRAME_WIDTH {
            let dx = x as f64 + 0.5 - FRAME_WIDTH as f64 / 2.0 - tx;
            let dy = y as f64 + 0.5 - FRAME_HEIGHT as f64 / 2.0 - ty;
            let sx = ((dx * cos + dy * sin) / scale + FRAME_WIDTH as f64 / 2.0).floor() as i32;
            let sy = ((-dx * sin + dy * cos) / scale + FRAME_HEIGHT as f64 / 2.0).floor() as i32;
            if sx < 0 || sy < 0 || sx >= FRAME_WIDTH as i32 || sy >= FRAME_HEIGHT as i32 {
                continue;
            }
            let i = ((y * FRAME_WIDTH + x) * 4) as usize;
            let j = ((sy as u32 * FRAME_WIDTH + sx as u32) * 4) as usize;
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

/// 寻址当前帧的各轨道素材并合成，图片使用缓存首帧；离线/音频不画入视频画布。
/// 调用方应在后台执行，解码错误原样返回；输出固定 640×360 BGRA，不依赖 GUI。
pub fn render(timeline: &Timeline, frame: i64, media: &[media::Media]) -> Result<Vec<u8>, String> {
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
                m.thumbnail
                    .as_ref()
                    .map(|b| b.as_ref().clone())
                    .ok_or("图片没有可用画面")?
            } else {
                media::frame(
                    &m.path,
                    (clip.source_in + frame - clip.start) as f64 / timeline.fps as f64,
                )?
            };
            blend(
                &mut out,
                &bytes,
                &clip.transform,
                (timeline.width, timeline.height),
            )?;
        }
    }
    Ok(out)
}
