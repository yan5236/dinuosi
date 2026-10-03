//! 素材预览与时间线片段共用的裁剪把手，仅负责外观和命中范围。
use crate::ui::{MUTED, TEXT};
use gpui::{Div, SharedString, Stateful, div, prelude::*, px, rgb};
pub const WIDTH: f32 = 12.0;

/// 在片段端点的零宽锚点外侧绘制把手：左端向左、右端向右伸出 12px。
/// 仅外侧两个角为圆角，中间竖线与预览一致；父容器须为把手保留空间，调用方绑定裁剪事件。
pub fn trim_handle(id: impl Into<SharedString>, left: bool) -> Stateful<Div> {
    let id = id.into();
    let selector = id.clone();
    div()
        .id(id)
        .debug_selector(move || selector.to_string())
        .absolute()
        .left(px(if left { -WIDTH } else { 0.0 }))
        .w(px(WIDTH))
        .h_full()
        .bg(rgb(TEXT))
        .when(left, |s| s.rounded_l(px(4.0)))
        .when(!left, |s| s.rounded_r(px(4.0)))
        .cursor_ew_resize()
        .occlude()
        .child(
            div()
                .absolute()
                .left(px(5.5))
                .top(px(9.0))
                .bottom(px(9.0))
                .w(px(1.0))
                .bg(rgb(MUTED)),
        )
}
