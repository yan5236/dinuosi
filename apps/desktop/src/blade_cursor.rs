//! 专用实心剃刀图标用于平台光标，移动不依赖内容重绘；工具栏继续使用原始 Razor 图标。
use crate::view::Desktop;
use gpui::{Context, Div, canvas, prelude::*};

#[derive(Default)]
pub(crate) struct BladeCursor {
    #[cfg(all(target_os = "windows", not(test)))]
    native: Option<dinuosi_cursor::NativeCursor>,
    #[cfg(all(target_os = "windows", not(test)))]
    scale: Option<f32>,
}

impl BladeCursor {
    /// 全屏等不挂载时间线的界面立即清除原生活动区域，不遗留刀片系统光标。
    pub(crate) fn clear(&self) {
        #[cfg(all(target_os = "windows", not(test)))]
        if let Some(cursor) = &self.native {
            cursor.set_region(None);
        }
    }
}

impl Desktop {
    /// 在时间线边界已经计算后的预绘制阶段更新活动区域，菜单、工具切换和 DPI 变化即时生效。
    /// Windows 从现有 SVG 创建系统光标；其他平台或无原生句柄的测试窗口使用系统十字光标。
    pub(crate) fn blade_cursor_region(&self, cx: &mut Context<Self>) -> Div {
        let entity = cx.entity();
        gpui::div().absolute().inset_0().child(
            canvas(
                move |_, window, cx| {
                    entity.update(cx, |this, cx| {
                        #[cfg(all(target_os = "windows", not(test)))]
                        {
                            let scale = window.scale_factor();
                            if this.blade_cursor.scale != Some(scale) {
                                this.blade_cursor.scale = Some(scale);
                                let result = native_cursor(window, scale);
                                match result {
                                    Ok(cursor) => this.blade_cursor.native = Some(cursor),
                                    Err(error) => eprintln!("创建刀片原生光标失败：{error}"),
                                }
                            }
                            if let Some(cursor) = &this.blade_cursor.native {
                                let b = this.timeline.bounds;
                                cursor.set_region(
                                    this.blade_enabled().then_some(
                                        [
                                            (f32::from(b.left()) + this.timeline.header) * scale,
                                            (f32::from(b.top()) + 38.0) * scale,
                                            (f32::from(b.right()) - 12.0) * scale,
                                            (f32::from(b.bottom()) - 12.0) * scale,
                                        ]
                                        .map(|v| v.round() as i32),
                                    ),
                                );
                            }
                        }
                        let _ = (this, cx, window);
                    });
                },
                |_, _, _, _| {},
            )
            .absolute()
            .inset_0(),
        )
    }
}

/// 仅在窗口创建或 DPI 改变时创建专用剃刀原生光标，保持 24 个逻辑像素与左侧刀刃热点。
/// 图标库提供实心刀身及深色轮廓；SVG、像素或平台资源创建失败返回具体错误。
#[cfg(all(target_os = "windows", not(test)))]
fn native_cursor(
    window: &gpui::Window,
    scale: f32,
) -> Result<dinuosi_cursor::NativeCursor, String> {
    let image = raster_blade(scale)?;
    let dimension = image.width();
    dinuosi_cursor::NativeCursor::new(
        window,
        image.data(),
        dimension,
        dimension,
        blade_hotspot(scale),
    )
}

/// 从图标库栅格化专用实心剃刀，直接生成与系统光标尺寸一致的设备像素，不放大已有位图。
/// 按整数像素尺寸反算缩放，避免分数 DPI 下画布尺寸和 SVG 缩放不一致；无效尺寸或 SVG 返回错误。
#[cfg(target_os = "windows")]
fn raster_blade(scale: f32) -> Result<resvg::tiny_skia::Pixmap, String> {
    let tree =
        resvg::usvg::Tree::from_str(dinuosi_icons::Icon::RazorCursor.svg(), &Default::default())
            .map_err(|e| e.to_string())?;
    let dimension = (24.0 * scale).round() as u32;
    let mut image =
        resvg::tiny_skia::Pixmap::new(dimension, dimension).ok_or("无效的光标 DPI 尺寸")?;
    let pixel_scale = dimension as f32 / 24.0;
    resvg::render(
        &tree,
        resvg::tiny_skia::Transform::from_scale(pixel_scale, pixel_scale),
        &mut image.as_mut(),
    );
    Ok(image)
}

/// 专用剃刀的左侧外刃中点由 SVG 的 (12,17) 旋转 60° 得到约 (7.67,14.5)，取最近设备像素。
/// 热点随 DPI 缩放，切割虚线和鼠标点击使用同一位置，不再对齐刀片中心。
#[cfg(any(target_os = "windows", test))]
fn blade_hotspot(scale: f32) -> (u16, u16) {
    (
        (7.669_873 * scale).round() as u16,
        (14.5 * scale).round() as u16,
    )
}

/// 验证工具栏保留原图标，光标采用独立实心资源，并保持各 DPI 下的左侧刀刃热点。
#[test]
fn razor_rotation_and_left_edge_hotspot_follow_dpi() {
    assert!(
        dinuosi_icons::Icon::Razor
            .svg()
            .contains("rotate(-30 12 12)")
    );
    assert_eq!(
        dinuosi_icons::Icon::RazorCursor.path(),
        "icons/razor-cursor.svg"
    );
    assert!(
        dinuosi_icons::Icon::RazorCursor
            .svg()
            .contains("fill=\"currentColor\"")
    );
    assert_eq!(blade_hotspot(1.0), (8, 15));
    assert_eq!(blade_hotspot(1.25), (10, 18));
    assert_eq!(blade_hotspot(2.0), (15, 29));
}

/// 对实际光标资源验证设备像素尺寸、实心不透明刀身和刀刃热点覆盖，避免薄线放大产生模糊。
#[cfg(target_os = "windows")]
#[test]
fn solid_cursor_raster_matches_device_pixels_and_hotspot() {
    for (scale, dimension) in [(1.0, 24), (1.25, 30), (1.5, 36), (2.0, 48)] {
        let image = raster_blade(scale).unwrap();
        assert_eq!((image.width(), image.height()), (dimension, dimension));
        let opaque = image
            .pixels()
            .iter()
            .filter(|pixel| pixel.alpha() == 255 && pixel.red() >= 230)
            .count();
        assert!(
            opaque > (30.0 * scale * scale) as usize,
            "必须保留实心浅色刀身"
        );
        let (x, y) = blade_hotspot(scale);
        assert!(
            image.pixel(x as u32, y as u32).unwrap().alpha() > 220,
            "热点必须位于实际刀刃上"
        );
    }
}
