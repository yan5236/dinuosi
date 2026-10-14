//! 示波器后台像素绘制与检查器适配，不在主线程扫描视频帧。
use crate::{ui::MUTED, view::Desktop};
use gpui::{Context, Div, div, prelude::*, px, rgb};
pub(crate) const SIZE: (u32, u32) = (256, 144);

/// 在后台从实际合成帧统计并绘制 RGB 直方图及亮度波形，排除项目比例补边。
/// 两图分别使用对数密度，保持细小信号可见；统计或尺寸错误时返回 None。
pub(crate) fn scope_pixels(bytes: &[u8], canvas: (u32, u32)) -> Option<Vec<u8>> {
    let scopes = dinuosi_core::color_scopes::Scopes::from_bgra(
        bytes,
        (640, 360),
        dinuosi_core::media::preview_dimensions(canvas),
    )
    .ok()?;
    let mut out = [0x21, 0x21, 0x23, 255].repeat((SIZE.0 * SIZE.1) as usize);
    let max = scopes
        .histogram
        .iter()
        .flatten()
        .copied()
        .max()
        .unwrap_or(1)
        .max(1) as f64;
    for x in 0..256 {
        for c in 0..3 {
            let height =
                (f64::from(scopes.histogram[c][x]).ln_1p() / max.ln_1p() * 62.0).round() as usize;
            for y in 64 - height..64 {
                out[(y * 256 + x) * 4 + 2 - c] = [210, 185, 155][c];
            }
        }
    }
    let max = scopes.waveform.iter().copied().max().unwrap_or(1).max(1) as f64;
    for y in 0..72 {
        for x in 0..256 {
            let start = y * 128 / 72;
            let end = ((y + 1) * 128 / 72).max(start + 1);
            let count = (start..end)
                .map(|r| scopes.waveform[r * 256 + x])
                .max()
                .unwrap_or(0);
            let density = (f64::from(count).ln_1p() / max.ln_1p() * 230.0).round() as u8;
            let i = ((y + 72) * 256 + x) * 4;
            if count > 0 {
                out[i..i + 3].fill(density);
            }
        }
    }
    Some(out)
}
impl Desktop {
    /// 显示当前合成帧的统计位图与两图控件标签；开关关闭时不生成统计或持有图像。
    pub(crate) fn color_scopes(&self, _cx: &mut Context<Self>) -> Div {
        div()
            .debug_selector(|| "color-scopes-view".into())
            .mt(px(12.0))
            .text_size(px(10.0))
            .text_color(rgb(MUTED))
            .child(div().child(crate::i18n::t(
                "color",
                "RGB histogram / Luma waveform",
                _cx,
            )))
            .when_some(self.timeline.color.scope_image.as_ref(), |s, image| {
                s.child(gpui::img(image.clone()).w_full().h(px(144.0)).mt(px(4.0)))
            })
    }
}
