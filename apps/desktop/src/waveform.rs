//! 音频波形的后台调度与 GPUI 绘制；时间线和素材预览共享源峰值缓存。

use crate::{media::MediaKind, view::Desktop};
use dinuosi_core::waveform::Waveform;
use gpui::{Bounds, Context, Div, canvas, div, fill, point, prelude::*, px, rgb, size};
use std::sync::Arc;

const CYAN: u32 = 0x35cbdc;

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

/// 在给定源起点与秒数范围绘制青蓝色上下峰值；每个像素查询自身时间区间，保留瞬态。
/// duration 表示本画布覆盖的源时长，时间线由可见宽度/缩放计算，预览使用完整素材时长。
/// 缓存缺失时保留空画布；绘制裁切在实际边界内，不生成拉伸位图或额外说明文字。
pub(crate) fn waveform_view(waveform: Option<Arc<Waveform>>, start: f64, duration: f64) -> Div {
    div().size_full().overflow_hidden().child(
        canvas(
            move |_, _, _| (),
            move |bounds, _, window, _| {
                let Some(waveform) = waveform else {
                    return;
                };
                let width = f32::from(bounds.size.width);
                let height = f32::from(bounds.size.height);
                if width <= 0.0 || height <= 0.0 || duration <= 0.0 {
                    return;
                }
                let center = height / 2.0;
                let scale = (center - 1.0).max(0.0);
                for x in 0..width.ceil() as usize {
                    let left = x as f32;
                    let right = (left + 1.0).min(width);
                    let peak = waveform.peak(
                        start + duration * left as f64 / width as f64,
                        start + duration * right as f64 / width as f64,
                    );
                    let top = (center - peak.max * scale).min(center - 0.5).max(0.0);
                    let bottom = (center - peak.min * scale).max(center + 0.5).min(height);
                    window.paint_quad(fill(
                        Bounds::new(
                            bounds.origin + point(px(left), px(top)),
                            size(px(right - left), px(bottom - top)),
                        ),
                        rgb(CYAN),
                    ));
                }
            },
        )
        .size_full(),
    )
}
