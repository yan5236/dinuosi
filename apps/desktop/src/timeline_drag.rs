//! 素材库拖动反馈：隐藏系统光标并在鼠标位置绘制素材缩略图，释放后自动恢复。
use crate::{
    timeline_state::Gesture,
    ui::{LINE, TEXT},
    view::Desktop,
};
use dinuosi_icons::Icon;
use gpui::{Div, ObjectFit, canvas, div, img, prelude::*, px, rgb, svg};
impl Desktop {
    /// 在根层绘制拖动缩略图与隐藏光标请求；仅真实拖动时启用，鼠标点击不改变光标。
    pub(crate) fn library_drag_overlay(&self) -> Option<Div> {
        let Gesture::Library { index, .. } = self.timeline.gesture.as_ref()? else {
            return None;
        };
        if !self.timeline.moved {
            return None;
        }
        let media = self.session.media.get(*index)?;
        Some(
            div()
                .absolute()
                .left(self.timeline.pointer.x - px(35.0))
                .top(self.timeline.pointer.y - px(22.0))
                .w(px(100.0))
                .h(px(62.0))
                .rounded(px(4.0))
                .overflow_hidden()
                .border_1()
                .border_color(rgb(TEXT))
                .bg(rgb(LINE))
                .opacity(0.85)
                .when_some(media.thumbnail.clone(), |s, image| {
                    s.child(img(image).size_full().object_fit(ObjectFit::Contain))
                })
                .when(media.thumbnail.is_none(), |s| {
                    s.flex().items_center().justify_center().child(
                        svg()
                            .path(Icon::Audio.path())
                            .size(px(28.0))
                            .text_color(rgb(TEXT)),
                    )
                })
                .child(
                    canvas(
                        |_, _, _| (),
                        |_, _, window, _| window.set_window_cursor_style(gpui::CursorStyle::None),
                    )
                    .absolute()
                    .inset_0(),
                ),
        )
    }
}
