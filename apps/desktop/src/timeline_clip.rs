//! 片段绘制：缩略图在内容内部裁切，统一裁剪把手在片段端点外侧绘制。
use crate::{
    trim_handle::{WIDTH, trim_handle},
    ui::TEXT,
    view::Desktop,
};
use dinuosi_core::timeline::Clip;
use gpui::{Context, Div, MouseButton, ObjectFit, Stateful, div, img, prelude::*, px, rgb};
impl Desktop {
    /// 仅绘制可见片段与真实端点把手；水平滚动截断处不伪造端点，零秒处预留把手空间。
    pub(crate) fn timeline_clip(
        &self,
        clip: &Clip,
        width: f32,
        cx: &mut Context<Self>,
    ) -> Option<Stateful<Div>> {
        let pps = self.pixels_per_second();
        let x = clip.start as f32 / self.timeline.model.fps as f32 * pps - self.timeline.scroll_x;
        let w = clip.duration as f32 / self.timeline.model.fps as f32 * pps;
        if x + w < 0.0 || x > width {
            return None;
        }
        let (top, h) = self.track_geometry(clip.track);
        let selected = self.timeline.selected.contains(&clip.id);
        let id = clip.id.clone();
        let thumbnail = self
            .session
            .media
            .iter()
            .find(|m| m.path == clip.source)
            .and_then(|m| m.thumbnail.clone());
        let visible = (w + x.min(0.0)).min(width - x.max(0.0)).max(2.0);
        Some(
            div()
                .id(gpui::SharedString::from(format!("clip-{}", clip.id)))
                .debug_selector({
                    let id = id.clone();
                    move || format!("clip-{id}")
                })
                .absolute()
                .left(px(x.max(0.0) + WIDTH))
                .top(px(top - 34.0))
                .w(px(visible))
                .h(px(h - 8.0))
                .rounded(px(4.0))
                .bg(rgb(0x414147))
                .border_1()
                .border_color(rgb(if selected { TEXT } else { 0x68686e }))
                .cursor_grab()
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, e, window, cx| {
                        this.begin_clip(id.clone(), None, e, window, cx)
                    }),
                )
                .child(
                    div()
                        .absolute()
                        .inset_0()
                        .overflow_hidden()
                        .rounded(px(3.0))
                        .when_some(thumbnail, |s, image| {
                            let tile = (h - 8.0) * 16.0 / 9.0;
                            s.children((0..(visible / tile).ceil() as usize).map(|n| {
                                div()
                                    .absolute()
                                    .left(px(n as f32 * tile))
                                    .top_0()
                                    .w(px(tile))
                                    .h_full()
                                    .child(
                                        img(image.clone())
                                            .size_full()
                                            .object_fit(ObjectFit::Cover)
                                            .opacity(0.7),
                                    )
                            }))
                        })
                        .child(
                            div()
                                .absolute()
                                .left(px(7.0))
                                .right(px(5.0))
                                .top(px(3.0))
                                .text_size(px(11.0))
                                .overflow_hidden()
                                .whitespace_nowrap()
                                .child(clip.name.clone()),
                        ),
                )
                .children(
                    [true, false]
                        .into_iter()
                        .filter(|left| selected && if *left { x >= 0.0 } else { x + w <= width })
                        .map(|left| {
                            let id = clip.id.clone();
                            div()
                                .absolute()
                                .top_0()
                                .h_full()
                                .w_0()
                                .when(left, |s| s.left_0())
                                .when(!left, |s| s.right_0())
                                .child(
                                    trim_handle(format!("trim-{id}-{left}"), left).on_mouse_down(
                                        MouseButton::Left,
                                        cx.listener(move |this, e, window, cx| {
                                            this.begin_clip(id.clone(), Some(left), e, window, cx)
                                        }),
                                    ),
                                )
                        }),
                ),
        )
    }
}
