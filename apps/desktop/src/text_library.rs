//! 左侧文本分类：文本来源、SRT 交换和用户样式库，不占用时间线工具栏。
use crate::{
    ui::{LINE, MUTED, PANEL, TEXT},
    view::Desktop,
};
use dinuosi_icons::Icon;
use gpui::{Context, Div, div, prelude::*, px, rgb, svg};
impl Desktop {
    /// 绘制文字来源和用户预设；点击或 Enter/Space 创建后进入正文，选中字幕时可导出当前组。
    pub(crate) fn text_library(&self, cx: &mut Context<Self>) -> Div {
        let export = self
            .text_clip()
            .and_then(|c| c.text.as_ref())
            .is_some_and(|t| t.group.is_some());
        div()
            .flex_1()
            .min_w_0()
            .h_full()
            .p(px(12.0))
            .bg(rgb(PANEL))
            .flex()
            .flex_col()
            .child(
                div().flex().flex_wrap().gap(px(8.0)).mb(px(12.0)).children(
                    [
                        (false, "Import SRT", "text-import-srt"),
                        (true, "Export SRT", "text-export-srt"),
                    ]
                    .into_iter()
                    .map(|(saving, label, id)| {
                        let enabled = !self.busy && (!saving || export);
                        div()
                            .id(id)
                            .debug_selector(move || id.into())
                            .focusable()
                            .tab_index(0)
                            .px(px(8.0))
                            .py(px(5.0))
                            .rounded(px(3.0))
                            .border_1()
                            .border_color(rgb(LINE))
                            .text_size(px(12.0))
                            .cursor_pointer()
                            .text_color(rgb(if enabled { TEXT } else { MUTED }))
                            .child(crate::i18n::t("text", label, cx))
                            .on_click(cx.listener(move |this, _, window, cx| {
                                if enabled {
                                    if saving {
                                        this.choose_caption_export(window, cx);
                                    } else {
                                        this.choose_caption_file(window, cx);
                                    }
                                }
                            }))
                            .on_key_down(cx.listener(
                                move |this, e: &gpui::KeyDownEvent, window, cx| {
                                    if enabled
                                        && !e.is_held
                                        && matches!(e.keystroke.key.as_str(), "enter" | "space")
                                    {
                                        if saving {
                                            this.choose_caption_export(window, cx);
                                        } else {
                                            this.choose_caption_file(window, cx);
                                        }
                                        cx.stop_propagation();
                                    }
                                },
                            ))
                    }),
                ),
            )
            .child(
                div()
                    .id("text-library-content")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .child(
                        div().flex().flex_wrap().gap(px(12.0)).children(
                            [
                                (false, "Basic text", "library-add-text", Icon::Text),
                                (
                                    true,
                                    "Manual subtitle",
                                    "library-add-subtitle",
                                    Icon::Subtitles,
                                ),
                            ]
                            .into_iter()
                            .map(|(subtitle, label, id, icon)| {
                                div()
                                    .id(id)
                                    .debug_selector(move || id.into())
                                    .focusable()
                                    .tab_index(0)
                                    .w(px(130.0))
                                    .flex_none()
                                    .cursor_pointer()
                                    .rounded(px(4.0))
                                    .border_1()
                                    .border_color(rgb(LINE))
                                    .focus(|s| s.border_color(rgb(TEXT)))
                                    .hover(|s| s.border_color(rgb(MUTED)))
                                    .child(
                                        div()
                                            .h(px(76.0))
                                            .w_full()
                                            .flex()
                                            .items_center()
                                            .justify_center()
                                            .bg(rgb(0x242426))
                                            .child(
                                                svg()
                                                    .path(icon.path())
                                                    .size(px(28.0))
                                                    .text_color(rgb(TEXT)),
                                            ),
                                    )
                                    .child(
                                        div()
                                            .h(px(32.0))
                                            .flex()
                                            .items_center()
                                            .justify_center()
                                            .text_size(px(12.0))
                                            .child(crate::i18n::t("text", label, cx)),
                                    )
                                    .on_click(cx.listener(move |this, _, window, cx| {
                                        this.add_text_clip(subtitle, None, window, cx)
                                    }))
                                    .on_key_down(cx.listener(
                                        move |this, e: &gpui::KeyDownEvent, window, cx| {
                                            if !e.is_held
                                                && !e.keystroke.modifiers.modified()
                                                && matches!(
                                                    e.keystroke.key.as_str(),
                                                    "enter" | "space"
                                                )
                                            {
                                                this.add_text_clip(subtitle, None, window, cx);
                                                cx.stop_propagation();
                                            }
                                        },
                                    ))
                            }),
                        ),
                    )
                    .child(self.text_preset_library(cx)),
            )
    }
}
