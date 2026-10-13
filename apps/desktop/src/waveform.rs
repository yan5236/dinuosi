//! 音频波形的后台调度与 GPUI 绘制；时间线和素材预览共享源峰值缓存。

use crate::{media::MediaKind, view::Desktop};
use dinuosi_core::waveform::Waveform;
use gpui::{
    App, AppContext, Bounds, Context, Div, Element, ElementId, Entity, GlobalElementId,
    InspectorElementId, IntoElement, LayoutId, Pixels, RenderImage, Style, Window, div, prelude::*,
};
use std::sync::Arc;

const CYAN: u32 = 0x35cbdc;

#[cfg(test)]
thread_local! {
    pub(crate) static RASTERIZATIONS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

impl Desktop {
    /// 串行后台提取尚未处理的在线音频，导入、重开及离线恢复共用此入口。
    /// 每个素材每次载入只尝试一次；结果按路径与请求状态校验，移除/离线后的结果丢弃。
    /// 单一任务随会话销毁取消等待，失败沿用现有错误提示，不阻塞素材播放或界面输入。
    pub(crate) fn load_waveforms(&mut self, cx: &mut Context<Self>) {
        if self.session.waveform_task.is_some() {
            return;
        }
        let Some(media) = self
            .session
            .media
            .iter_mut()
            .find(|m| m.kind == MediaKind::Audio && !m.offline && !m.waveform_requested)
        else {
            return;
        };
        media.waveform_requested = true;
        let source = media.source.clone();
        self.session.waveform_task = Some(cx.spawn(async move |this, cx| {
            let path = source.path.clone();
            let result = cx
                .background_spawn(async move { Waveform::decode(&source).map(Arc::new) })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.session.waveform_task = None;
                if let Some(media) = this
                    .session
                    .media
                    .iter_mut()
                    .find(|m| m.path == path && !m.offline && m.waveform_requested)
                {
                    match result {
                        Ok(waveform) => media.waveform = Some(waveform),
                        Err(error) => this.notice = Some(format!("{}：{error}", media.name)),
                    }
                }
                this.load_waveforms(cx);
                cx.notify();
            });
        }));
    }
}

/// 按源时间和实际设备像素绘制波形；稳定 ID 必须在父元素内唯一，三处调用各自保留缓存。
/// duration 为负表示从 start 向前显示倒放源区间；源、裁剪、缩放、尺寸或 DPI 改变时重建。
/// 缓存随元素释放并清理 GPU 图片，不写项目或保留不可见片段的绘制结果。
pub(crate) fn waveform_view(
    id: &'static str,
    waveform: Option<Arc<Waveform>>,
    start: f64,
    duration: f64,
) -> Div {
    div().size_full().overflow_hidden().child(WaveformElement {
        id,
        waveform,
        start,
        duration,
    })
}

struct WaveformElement {
    id: &'static str,
    waveform: Option<Arc<Waveform>>,
    start: f64,
    duration: f64,
}

struct WaveformPaint {
    waveform: Arc<Waveform>,
    start: f64,
    duration: f64,
    dimensions: (u32, u32),
    scale: f32,
    image: Arc<RenderImage>,
}

impl IntoElement for WaveformElement {
    type Element = Self;

    /// 直接交给 GPUI 元素生命周期，避免每次 Render 创建新的图片缓存。
    fn into_element(self) -> Self {
        self
    }
}

impl Element for WaveformElement {
    type RequestLayoutState = ();
    type PrepaintState = Option<Arc<RenderImage>>;

    /// 使用调用点的稳定身份，片段由父级片段 ID 隔离，避免三处波形互相覆盖。
    fn id(&self) -> Option<ElementId> {
        Some(self.id.into())
    }

    /// 本元素不需要额外检查器位置，保持与原画布一致。
    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        None
    }

    /// 填满现有容器，不改变波形与选区、片段标签的布局关系。
    fn request_layout(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, ()) {
        let mut style = Style::default();
        style.size.width = gpui::relative(1.0).into();
        style.size.height = gpui::relative(1.0).into();
        (window.request_layout(style, [], cx), ())
    }

    /// 只在缓存键变化时按真实像素生成 BGRA；替换或元素移除时释放旧纹理。
    fn prepaint(
        &mut self,
        id: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        let scale = window.scale_factor();
        let dimensions = (
            (f32::from(bounds.size.width) * scale).ceil() as u32,
            (f32::from(bounds.size.height) * scale).ceil() as u32,
        );
        window.with_element_state(
            id.unwrap(),
            |cache: Option<Option<Entity<WaveformPaint>>>, window| {
                let mut cache = cache.flatten();
                let Some(waveform) = self.waveform.as_ref().filter(|_| {
                    dimensions.0 > 0
                        && dimensions.1 > 0
                        && self.duration != 0.0
                        && self.duration.is_finite()
                        && self.start.is_finite()
                }) else {
                    return (None, None);
                };
                let valid = cache.as_ref().is_some_and(|cache| {
                    let cache = cache.read(cx);
                    Arc::ptr_eq(&cache.waveform, waveform)
                        && cache.start == self.start
                        && cache.duration == self.duration
                        && cache.dimensions == dimensions
                        && cache.scale == scale
                });
                if !valid {
                    #[cfg(test)]
                    RASTERIZATIONS.with(|count| count.set(count.get() + 1));
                    let image = Arc::new(RenderImage::new([image::Frame::new(rasterize(
                        dimensions,
                        scale,
                        self.start,
                        self.duration,
                        |start, end| waveform.peak(start, end),
                    ))]));
                    cache = Some(cx.new(|_| WaveformPaint {
                        waveform: waveform.clone(),
                        start: self.start,
                        duration: self.duration,
                        dimensions,
                        scale,
                        image,
                    }));
                    cx.observe_release_in(cache.as_ref().unwrap(), window, |cache, window, _| {
                        if let Err(error) = window.drop_image(cache.image.clone()) {
                            eprintln!("释放波形纹理失败：{error}");
                        }
                    })
                    .detach();
                }
                let image = cache.as_ref().map(|cache| cache.read(cx).image.clone());
                (image, cache)
            },
        )
    }

    /// 每处波形仅提交一个图片绘制指令；GPUI 复用同一图片 ID 的 GPU 纹理。
    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut (),
        image: &mut Self::PrepaintState,
        window: &mut Window,
        _: &mut App,
    ) {
        if let Some(image) = image.take()
            && let Err(error) = window.paint_image(bounds, Default::default(), image, 0, false)
        {
            eprintln!("绘制波形失败：{error}");
        }
    }
}

/// 把每个设备像素覆盖的源区间转换为透明 BGRA 图片，保留瞬态、零基线和亚像素边缘。
/// dimensions 来自实际画布，scale 是 DPI 比例；负 duration 反向取源峰值，不交换峰值上下方向。
fn rasterize(
    dimensions: (u32, u32),
    scale: f32,
    start: f64,
    duration: f64,
    peak: impl Fn(f64, f64) -> dinuosi_core::waveform::Peak,
) -> image::RgbaImage {
    let (width, height) = dimensions;
    let mut image = image::RgbaImage::new(width, height);
    let center = height as f32 / 2.0;
    let amplitude = (center - scale).max(0.0);
    for x in 0..width {
        let a = start + duration * x as f64 / width as f64;
        let b = start + duration * (x + 1) as f64 / width as f64;
        let peak = peak(a.min(b), a.max(b));
        let top = (center - peak.max * amplitude)
            .min(center - scale / 2.0)
            .max(0.0);
        let bottom = (center - peak.min * amplitude)
            .max(center + scale / 2.0)
            .min(height as f32);
        for y in top.floor() as u32..bottom.ceil() as u32 {
            let alpha = ((bottom.min(y as f32 + 1.0) - top.max(y as f32)) * 255.0).round() as u8;
            image.put_pixel(
                x,
                y,
                image::Rgba([CYAN as u8, (CYAN >> 8) as u8, (CYAN >> 16) as u8, alpha]),
            );
        }
    }
    image
}

/// 验证 BGRA、零基线、瞬态峰值与源时间映射，在普通测试中覆盖无需解码的绘制边界。
#[test]
fn raster_keeps_source_peaks_color_and_dpi_baseline() {
    use dinuosi_core::waveform::Peak;
    let seen = std::cell::RefCell::new(Vec::new());
    let image = rasterize((4, 10), 1.0, 2.0, 1.0, |start, end| {
        seen.borrow_mut().push((start, end));
        if start == 2.5 {
            Peak {
                min: -1.0,
                max: 1.0,
            }
        } else {
            Peak::default()
        }
    });
    assert_eq!(
        &*seen.borrow(),
        &[(2.0, 2.25), (2.25, 2.5), (2.5, 2.75), (2.75, 3.0)]
    );
    assert_eq!(image.get_pixel(2, 1).0, [0xdc, 0xcb, 0x35, 255]);
    assert_eq!(image.get_pixel(0, 0).0, [0, 0, 0, 0]);
    assert_eq!(image.get_pixel(0, 4).0, [0xdc, 0xcb, 0x35, 128]);
    let image = rasterize((4, 20), 2.0, 0.0, 1.0, |_, _| Peak::default());
    assert_eq!(image.get_pixel(0, 9).0[3], 255);
    assert_eq!(image.get_pixel(0, 10).0[3], 255);
    assert_eq!(image.get_pixel(0, 8).0[3], 0);
    let reversed = rasterize((4, 10), 1.0, 3.0, -1.0, |start, _| {
        if start == 2.5 {
            Peak {
                min: -1.0,
                max: 1.0,
            }
        } else {
            Peak::default()
        }
    });
    assert_eq!(reversed.get_pixel(1, 1).0, [0xdc, 0xcb, 0x35, 255]);
    assert_eq!(reversed.get_pixel(2, 1).0[3], 0);
}
