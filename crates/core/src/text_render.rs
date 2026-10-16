//! 中文回退、多行排版和透明 BGRA 栅格；预览、画布命中及导出共用同一结果。
use crate::{
    text::{Alignment, Text},
    timeline::{Clip, Timeline},
};
use cosmic_text::{
    Align, Attrs, Buffer, CacheKeyFlags, Color, Family, FontSystem, Metrics, Shaping, Style,
    SwashCache, Weight, Wrap,
};
use std::sync::{Arc, Mutex, OnceLock};

pub struct Raster {
    pub size: (u32, u32),
    pub pixels: Vec<u8>,
}

struct Entry {
    text: Text,
    canvas: (u32, u32),
    raster: Arc<Raster>,
}

struct Engine {
    fonts: FontSystem,
    entries: Vec<Entry>,
}

/// 延迟建立系统字体库；只在首次使用文字时扫描，锁将字体状态与有限栅格缓存保护在一起。
fn engine() -> &'static Mutex<Engine> {
    static ENGINE: OnceLock<Mutex<Engine>> = OnceLock::new();
    ENGINE.get_or_init(|| {
        Mutex::new(Engine {
            fonts: FontSystem::new(),
            entries: Vec::new(),
        })
    })
}

/// 返回本机可用的字体系列名称并排序去重；不保存或复制字体文件，锁异常返回明确错误。
pub fn font_names() -> Result<Vec<String>, String> {
    let e = engine().lock().map_err(|_| "字体服务不可用")?;
    let mut names: Vec<_> = e
        .fonts
        .db()
        .faces()
        .flat_map(|f| f.families.iter().map(|(name, _)| name.clone()))
        .collect();
    names.sort();
    names.dedup();
    Ok(names)
}

/// 按项目像素排版文字，保留显式换行与按宽度自动折行；缺字使用系统字体回退。
/// 缓存总计最多 64MiB，按最近使用淘汰，不限制条目数以保留同时显示的多个短文本。
/// 编辑后按正文、样式和画布身份失效；超大缓冲返回错误。
pub fn raster(text: &Text, canvas: (u32, u32)) -> Result<Arc<Raster>, String> {
    text.validate()?;
    if canvas.0 == 0 || canvas.1 == 0 || canvas.0 > 16384 || canvas.1 > 16384 {
        return Err("文本画布尺寸无效".into());
    }
    let mut e = engine().lock().map_err(|_| "字体服务不可用")?;
    if let Some(index) = e
        .entries
        .iter()
        .position(|v| &v.text == text && v.canvas == canvas)
    {
        let entry = e.entries.remove(index);
        let result = entry.raster.clone();
        e.entries.push(entry);
        return Ok(result);
    }
    let image = Arc::new(draw(&mut e.fonts, text, canvas)?);
    while !e.entries.is_empty()
        && e.entries
            .iter()
            .map(|v| v.raster.pixels.len())
            .sum::<usize>()
            + image.pixels.len()
            > 64 * 1024 * 1024
    {
        e.entries.remove(0);
    }
    e.entries.push(Entry {
        text: text.clone(),
        canvas,
        raster: image.clone(),
    });
    Ok(image)
}

/// 用锁定版本的 COSMIC Text 进行统一排版，生成直通 Alpha 的 BGRA，描边仅扩展字形覆盖区域。
/// 缺失斜体字形由统一布局入口回退；文字超出画布高度时按文字框裁切，空正文保持透明。
fn draw(fonts: &mut FontSystem, text: &Text, canvas: (u32, u32)) -> Result<Raster, String> {
    let style = &text.style;
    let base_padding = (style.size * 0.3).ceil() as u32 + style.outline;
    let effects = crate::text_effects::margin(style);
    let padding = base_padding + effects;
    let width = (style.width.round() as u32).max(base_padding * 2 + 1) + effects * 2;
    let buffer = layout(fonts, text, (width - padding * 2) as f32, canvas.1 as f32);
    let height = (buffer
        .layout_runs()
        .map(|r| r.line_top + r.line_height)
        .fold(0.0_f32, f32::max)
        .ceil() as u32
        + padding * 2)
        .clamp(1, canvas.1);
    let count = width as usize * height as usize;
    if count > 16 * 1024 * 1024 {
        return Err("文本栅格超过安全尺寸".into());
    }
    let mut pixels = vec![0_u8; count * 4];
    let color = Color::rgb(
        (style.color >> 16) as u8,
        (style.color >> 8) as u8,
        style.color as u8,
    );
    buffer.draw(fonts, &mut SwashCache::new(), color, |x, y, w, h, color| {
        let [r, g, b, a] = color.as_rgba();
        for dy in 0..h as i32 {
            for dx in 0..w as i32 {
                let (x, y) = (x + dx + padding as i32, y + dy + padding as i32);
                if x >= 0 && y >= 0 && x < width as i32 && y < height as i32 {
                    let i = (y as usize * width as usize + x as usize) * 4;
                    over(&mut pixels[i..i + 4], [b, g, r, a]);
                }
            }
        }
    });
    if style.outline > 0 {
        let mut outline = vec![0_u8; count];
        let radius = style.outline as i32;
        for y in 0..height as i32 {
            for x in 0..width as i32 {
                let alpha = pixels[(y as usize * width as usize + x as usize) * 4 + 3];
                if alpha == 0 {
                    continue;
                }
                for dy in -radius..=radius {
                    for dx in -radius..=radius {
                        let (nx, ny) = (x + dx, y + dy);
                        if dx * dx + dy * dy <= radius * radius
                            && nx >= 0
                            && ny >= 0
                            && nx < width as i32
                            && ny < height as i32
                        {
                            let i = ny as usize * width as usize + nx as usize;
                            outline[i] = outline[i].max(alpha);
                        }
                    }
                }
            }
        }
        for (p, alpha) in pixels.as_chunks_mut::<4>().0.iter_mut().zip(outline) {
            let foreground = [p[0], p[1], p[2], p[3]];
            p.copy_from_slice(&[
                (style.outline_color & 255) as u8,
                ((style.outline_color >> 8) & 255) as u8,
                (style.outline_color >> 16) as u8,
                alpha,
            ]);
            over(p, foreground);
        }
    }
    pixels = crate::text_effects::apply(pixels, (width, height), style);
    Ok(Raster {
        size: (width, height),
        pixels,
    })
}

/// 优先使用真实斜体；只要缺字，就用同系列正常体及字体库的 14 度合成斜体重新排版。
/// 回退在同一字形栅格入口完成，保留中英混排、对齐与换行，不修改正文或项目中的字体名称。
fn layout(fonts: &mut FontSystem, text: &Text, width: f32, height: f32) -> Buffer {
    let style = &text.style;
    let mut buffer = Buffer::new(
        fonts,
        Metrics::new(style.size as f32, (style.size * style.line_height) as f32),
    );
    buffer.set_size(fonts, Some(width), Some(height));
    buffer.set_wrap(fonts, Wrap::WordOrGlyph);
    let mut attrs = Attrs::new()
        .family(if style.font.is_empty() {
            Family::SansSerif
        } else {
            Family::Name(&style.font)
        })
        .weight(if style.bold {
            Weight::BOLD
        } else {
            Weight::NORMAL
        })
        .style(if style.italic {
            Style::Italic
        } else {
            Style::Normal
        });
    let align = match style.alignment {
        Alignment::Left => Align::Left,
        Alignment::Center => Align::Center,
        Alignment::Right => Align::Right,
    };
    for fallback in [false, true] {
        if fallback {
            attrs = attrs
                .style(Style::Normal)
                .cache_key_flags(CacheKeyFlags::FAKE_ITALIC);
        }
        buffer.set_text(fonts, &text.body, &attrs, Shaping::Advanced);
        for line in &mut buffer.lines {
            line.set_align(Some(align));
        }
        buffer.shape_until_scroll(fonts, false);
        if !style.italic
            || fallback
            || buffer
                .layout_runs()
                .all(|r| r.glyphs.iter().all(|g| g.glyph_id != 0))
        {
            break;
        }
    }
    buffer
}

/// 在透明背景上按直通 Alpha 混合字形；重叠笔画和抗锯齿边缘不会重复变暗。
pub(crate) fn over(target: &mut [u8], source: [u8; 4]) {
    let a = source[3] as f64 / 255.0;
    let b = target[3] as f64 / 255.0 * (1.0 - a);
    let alpha = a + b;
    if alpha <= 0.0 {
        return;
    }
    for c in 0..3 {
        target[c] = ((source[c] as f64 * a + target[c] as f64 * b) / alpha).round() as u8;
    }
    target[3] = (alpha * 255.0).round() as u8;
}

/// 将文字栅格通过现有几何合成器绘入任意输出尺寸；预览和导出共用字体、布局、描边及变换。
/// 返回错误时不借用磁盘源或 CLI；输出尺寸和缓冲长度由通用混合入口再次检查。
pub fn blend(
    output: &mut [u8],
    output_size: (u32, u32),
    clip: &Clip,
    timeline: &Timeline,
    frame: f64,
) -> Result<(), String> {
    let image = raster(
        clip.text.as_ref().ok_or("文本数据缺失")?,
        (timeline.width, timeline.height),
    )?;
    crate::composite::blend_pixels(
        output,
        output_size,
        &image.pixels,
        image.size,
        &clip.transform_at(frame),
        (timeline.width, timeline.height),
        (image.size.0 as f64, image.size.1 as f64),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 检查实际字形身份而非只检查像素，中文正常体和斜体都不得使用 glyph 0 的缺字符号。
    /// 同时覆盖中英混排、粗斜体和多行，斜体栅格必须与正常体不同，避免回退后丢失倾斜效果。
    #[test]
    fn italic_cjk_keeps_real_glyphs_and_slant() {
        let mut e = engine().lock().unwrap();
        for body in ["中文字幕", "中文 Title\n第二行"] {
            for bold in [false, true] {
                let mut text = Text::new(body.into(), (640, 360), false);
                text.style.size = 40.0;
                text.style.bold = bold;
                let normal = draw(&mut e.fonts, &text, (640, 360)).unwrap();
                text.style.italic = true;
                let buffer = layout(&mut e.fonts, &text, 500.0, 360.0);
                let glyphs: Vec<_> = buffer.layout_runs().flat_map(|r| r.glyphs.iter()).collect();
                assert!(glyphs.len() >= 4);
                assert!(
                    glyphs.iter().all(|g| g.glyph_id != 0),
                    "缺失中文字形: {body}"
                );
                let italic = draw(&mut e.fonts, &text, (640, 360)).unwrap();
                assert_ne!(normal.pixels, italic.pixels);
                assert!(italic.pixels.as_chunks::<4>().0.iter().any(|p| p[3] > 0));
            }
        }
    }
}
