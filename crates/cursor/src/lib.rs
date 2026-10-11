//! 原生光标的最小平台边界；不依赖 GPUI、媒体、项目或绘制帧率。

#[cfg(target_os = "windows")]
mod windows;
#[cfg(target_os = "windows")]
pub use windows::NativeCursor;

/// 将预乘 RGBA 转为 Windows RT_CURSOR 的热点、32 位自底向上 DIB 及透明掩码。
/// 限制尺寸和热点并验证字节数，避免原生接口读取不完整图片；不访问窗口或磁盘。
#[cfg(any(target_os = "windows", test))]
fn cursor_resource(
    rgba: &[u8],
    width: u32,
    height: u32,
    hotspot: (u16, u16),
) -> Result<Vec<u8>, String> {
    if width == 0
        || height == 0
        || width > 256
        || height > 256
        || hotspot.0 as u32 >= width
        || hotspot.1 as u32 >= height
        || rgba.len() != width as usize * height as usize * 4
    {
        return Err("无效的原生光标尺寸、热点或像素数据".into());
    }
    let mask_stride = width.div_ceil(32) as usize * 4;
    let mut bytes = Vec::with_capacity(44 + rgba.len() + mask_stride * height as usize);
    bytes.extend_from_slice(&hotspot.0.to_le_bytes());
    bytes.extend_from_slice(&hotspot.1.to_le_bytes());
    for value in [40_u32, width, height * 2] {
        bytes.extend_from_slice(&value.to_le_bytes());
    }
    bytes.extend_from_slice(&1_u16.to_le_bytes());
    bytes.extend_from_slice(&32_u16.to_le_bytes());
    bytes.extend_from_slice(&[0; 24]);
    for row in rgba.chunks_exact(width as usize * 4).rev() {
        for pixel in row.as_chunks::<4>().0 {
            bytes.extend_from_slice(&[pixel[2], pixel[1], pixel[0], pixel[3]]);
        }
    }
    bytes.resize(bytes.len() + mask_stride * height as usize, 0);
    Ok(bytes)
}

/// 验证热点、DIB 方向、颜色顺序、掩码对齐与不完整输入拒绝，防止 FFI 图片越界。
#[test]
fn resource_preserves_hotspot_and_pixels_and_rejects_invalid_input() {
    let bytes = cursor_resource(&[1, 2, 3, 4, 5, 6, 7, 8], 1, 2, (0, 1)).unwrap();
    assert_eq!(&bytes[..4], &[0, 0, 1, 0]);
    assert_eq!(&bytes[12..16], &4_u32.to_le_bytes());
    assert_eq!(&bytes[44..52], &[7, 6, 5, 8, 3, 2, 1, 4]);
    assert_eq!(bytes.len(), 44 + 8 + 8);
    assert!(cursor_resource(&[0; 3], 1, 1, (0, 0)).is_err());
    assert!(cursor_resource(&[0; 4], 1, 1, (1, 0)).is_err());
    assert!(cursor_resource(&[], 0, 0, (0, 0)).is_err());
}
