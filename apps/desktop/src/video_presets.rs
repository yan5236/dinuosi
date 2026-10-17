//! 主要设置与导出共享的画幅、分辨率和整数帧率预设；自定义尺寸保持原值。
pub(crate) const FRAME_RATES: &[u32] = &[24, 25, 30, 50, 60, 120, 240];
pub(crate) const ASPECTS: &[&str] = &["Landscape 16:9", "Portrait 9:16", "Square 1:1", "Custom"];
pub(crate) const QUALITY: &[&str] = &["Standard", "High", "Custom"];
pub(crate) const FITS: &[&str] = &["Fit entire image", "Fill and crop", "Only downscale"];

/// 根据现有宽高识别画幅；非标准比例归入自定义，避免打开设置时悄悄改变项目。
pub(crate) fn aspect(width: u32, height: u32) -> usize {
    if u64::from(width) * 9 == u64::from(height) * 16 {
        0
    } else if u64::from(width) * 16 == u64::from(height) * 9 {
        1
    } else if width == height {
        2
    } else {
        3
    }
}

/// 按画幅返回 720p、1080p、4K 三档尺寸；方形以短边为边长，自定义沿用横屏列表供显式切换。
pub(crate) fn resolutions(aspect: usize) -> Vec<(u32, u32)> {
    [(1280, 720), (1920, 1080), (3840, 2160)]
        .into_iter()
        .map(|(w, h)| match aspect {
            1 => (h, w),
            2 => (h, h),
            _ => (w, h),
        })
        .collect()
}

/// 切换画幅时优先保留现有分辨率档次；非预设值按短边选最近档，选择自定义不改变尺寸。
pub(crate) fn change_aspect(width: u32, height: u32, target: usize) -> (u32, u32) {
    if target == 3 {
        return (width, height);
    }
    let short = width.min(height);
    let index = [720u32, 1080, 2160]
        .iter()
        .enumerate()
        .min_by_key(|(_, v)| v.abs_diff(short))
        .unwrap()
        .0;
    resolutions(target)[index]
}

/// 返回当前帧率和内置整数预设的有序唯一集合，保留旧项目的非预设帧率。
pub(crate) fn rates(current: u32) -> Vec<u32> {
    let mut values = FRAME_RATES.to_vec();
    values.push(current);
    values.sort_unstable();
    values.dedup();
    values
}

/// 显示分辨率档次及明确宽高；非预设尺寸仍显示完整数值。
pub(crate) fn resolution_label(width: u32, height: u32) -> String {
    let presets = resolutions(aspect(width, height));
    let prefix = presets.iter().position(|v| *v == (width, height)).map(|i| {
        if i == 2 && width == height {
            "2160p"
        } else {
            ["720p", "1080p", "4K"][i]
        }
    });
    prefix.map_or_else(
        || format!("{width} × {height}"),
        |p| format!("{p} · {width} × {height}"),
    )
}

/// 把领域素材适配模式映射为共享下拉顺序，不改变存储的枚举值。
pub(crate) fn fit_index(fit: dinuosi_core::video_settings::MediaFit) -> usize {
    use dinuosi_core::video_settings::MediaFit;
    match fit {
        MediaFit::Fit => 0,
        MediaFit::Fill => 1,
        MediaFit::Original => 2,
    }
}

/// 把质量档位映射为共享下拉顺序；旧项目的明确码率归入自定义。
pub(crate) fn quality_index(mode: dinuosi_core::video_settings::BitrateMode) -> usize {
    use dinuosi_core::video_settings::BitrateMode;
    match mode {
        BitrateMode::Standard => 0,
        BitrateMode::High => 1,
        BitrateMode::Custom => 2,
    }
}
