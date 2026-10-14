//! SDR 示波器统计；输入为实际合成画面的 BGRA，不读项目参数推测信号。
#[derive(Clone)]
pub struct Scopes {
    pub histogram: [[u32; 256]; 3],
    pub waveform: Vec<u32>,
}
impl Scopes {
    /// 统计有效画面的 RGB 直方图与 256×128 亮度波形，排除透明像素与画布补边。
    /// size 为缓冲尺寸，content 为居中有效画布；尺寸非法返回错误，不分配无界缓冲。
    pub fn from_bgra(bytes: &[u8], size: (u32, u32), content: (u32, u32)) -> Result<Self, String> {
        if size.0 == 0
            || size.1 == 0
            || content.0 == 0
            || content.1 == 0
            || content.0 > size.0
            || content.1 > size.1
            || (size.0 as usize)
                .checked_mul(size.1 as usize)
                .and_then(|n| n.checked_mul(4))
                != Some(bytes.len())
        {
            return Err("示波器像素尺寸无效".into());
        }
        let mut s = Self {
            histogram: [[0; 256]; 3],
            waveform: vec![0; 256 * 128],
        };
        let left = (size.0 - content.0) / 2;
        let top = (size.1 - content.1) / 2;
        for y in top..top + content.1 {
            for x in left..left + content.0 {
                let i = ((y * size.0 + x) * 4) as usize;
                if bytes[i + 3] == 0 {
                    continue;
                }
                let rgb = [bytes[i + 2], bytes[i + 1], bytes[i]];
                for (c, value) in rgb.iter().enumerate() {
                    s.histogram[c][*value as usize] =
                        s.histogram[c][*value as usize].saturating_add(1);
                }
                let luma = crate::color_render::luma(rgb.map(|v| v as f64 / 255.0));
                let row = 127 - (luma * 127.0).round() as usize;
                let col = ((x - left) as usize * 256 / content.0 as usize).min(255);
                s.waveform[row * 256 + col] = s.waveform[row * 256 + col].saturating_add(1);
            }
        }
        Ok(s)
    }
}
