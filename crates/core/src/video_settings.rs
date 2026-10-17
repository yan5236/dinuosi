//! 项目与导出共用的素材适配及目标码率规则；不依赖界面或编码器私有选项。
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MediaFit {
    #[default]
    Original,
    Fit,
    Fill,
}

impl MediaFit {
    /// 根据源显示尺寸和项目画布返回居中素材的覆盖尺寸；始终保持比例，不改写片段变换。
    /// Fit 允许放大小素材，Fill 放大至覆盖画布并由合成器裁边，Original 保留旧版仅缩小行为。
    pub fn size(self, source: (u32, u32), canvas: (u32, u32)) -> (f64, f64) {
        let source = (source.0.max(1) as f64, source.1.max(1) as f64);
        let ratios = (canvas.0 as f64 / source.0, canvas.1 as f64 / source.1);
        let scale = match self {
            Self::Original => ratios.0.min(ratios.1).min(1.0),
            Self::Fit => ratios.0.min(ratios.1),
            Self::Fill => ratios.0.max(ratios.1),
        };
        (source.0 * scale, source.1 * scale)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BitrateMode {
    Standard,
    High,
    #[default]
    Custom,
}

impl BitrateMode {
    /// 将质量档位换算为合法的目标视频 kbps；自定义原样返回，由提交边界校验范围。
    /// 标准/高质量按输出像素数与帧率估算并取整至 1000kbps，不承诺最终文件的实际平均码率。
    pub fn bitrate(self, width: u32, height: u32, fps: u32, custom: u32) -> u32 {
        let divisor = match self {
            Self::Standard => 5000,
            Self::High => 3000,
            Self::Custom => return custom,
        };
        let value = u64::from(width)
            .saturating_mul(u64::from(height))
            .saturating_mul(u64::from(fps))
            / divisor;
        (((value + 500) / 1000) * 1000).clamp(100, 1_000_000) as u32
    }
}
