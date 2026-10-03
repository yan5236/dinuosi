//! 核心媒体的 GPUI 图像适配；探测、解码、路径状态均由 Core 提供。

use dinuosi_core::media as core;
pub use dinuosi_core::media::{FRAME_HEIGHT, FRAME_WIDTH, MediaKind, timecode};
use gpui::RenderImage;
use std::{ops::Deref, path::Path, sync::Arc};

#[derive(Clone)]
pub struct Media {
    pub source: core::Media,
    pub thumbnail: Option<Arc<RenderImage>>,
}

impl Media {
    /// 将核心媒体首帧适配为 GPUI 图像，原始元数据仍由核心模型持有。
    /// 只分配显示用像素，不读取文件；输入首帧尺寸由核心解码器保证。
    pub(crate) fn from_source(source: core::Media) -> Self {
        let thumbnail = source
            .thumbnail
            .as_ref()
            .map(|bytes| render_frame(bytes.as_ref().clone(), source.preview_size));
        Self { source, thumbnail }
    }
}

impl Deref for Media {
    type Target = core::Media;
    /// 借用核心元数据，避免桌面端重复定义媒体类型、路径、时长和离线状态。
    fn deref(&self) -> &Self::Target {
        &self.source
    }
}

/// 调用核心寻址解码并按已探测的画面大小移除透明补边；失败不产生图像。
pub fn frame(
    path: &Path,
    seconds: f64,
    preview_size: (u32, u32),
) -> Result<Arc<RenderImage>, String> {
    core::frame(path, seconds).map(|bytes| render_frame(bytes, preview_size))
}

/// 仅裁去解码器生成的透明补边，保留源素材自身的透明像素和黑色内容；输出保持真实比例。
/// 输入固定 BGRA 帧及有效画面大小；限制矩形在帧内，颜色顺序与 GPUI 上传格式一致。
pub fn render_frame(bytes: Vec<u8>, preview_size: (u32, u32)) -> Arc<RenderImage> {
    let buffer = image::RgbaImage::from_raw(FRAME_WIDTH, FRAME_HEIGHT, bytes)
        .expect("核心解码器帧尺寸已验证");
    let (width, height) = (
        preview_size.0.clamp(1, FRAME_WIDTH),
        preview_size.1.clamp(1, FRAME_HEIGHT),
    );
    let buffer = if (width, height) == (FRAME_WIDTH, FRAME_HEIGHT) {
        buffer
    } else {
        image::imageops::crop_imm(
            &buffer,
            (FRAME_WIDTH - width) / 2,
            (FRAME_HEIGHT - height) / 2,
            width,
            height,
        )
        .to_image()
    };
    Arc::new(RenderImage::new([image::Frame::new(buffer)]))
}
