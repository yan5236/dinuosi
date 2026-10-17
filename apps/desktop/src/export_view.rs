//! 导出弹窗绘制：左侧项目预览，右侧参数，下方进度与提交；复用灰阶控件。
use crate::{
    export_dialog::ExportField,
    ui::{LINE, MUTED, PANEL, TEXT, pill, tool},
    view::Desktop,
};
use dinuosi_icons::Icon;
use gpui::{Context, Div, MouseButton, ObjectFit, div, img, prelude::*, px, rgb, rgba, svg};
use std::sync::atomic::Ordering;

impl Desktop {
    /// 在现有编辑器上绘制灰阶模态弹窗，项目预览只显示画面，不附加教学、项目统计或编码实现说明。
    /// 任务进行时锁定参数并展示进度，取消等待清理；完成与真实错误保留在同一表单中。
    pub(crate) fn export_view(&self, cx: &mut Context<Self>) -> gpui::Stateful<Div> {
        let d = self.export_dialog.as_ref().unwrap();
        let running = d.running;
        let available = !d.running && !d.probing && !d.choosing && !d.encoders.is_empty();
        let progress = d.progress.load(Ordering::Relaxed) as f32 / 10.0;
        let width = if d.custom_resolution {
            d.fields[0]
                .read(cx)
                .text
                .parse::<u32>()
                .unwrap_or(d.settings.width)
        } else {
            d.settings.width
        };
        let height = if d.custom_resolution {
            d.fields[1]
                .read(cx)
                .text
                .parse::<u32>()
                .unwrap_or(d.settings.height)
        } else {
            d.settings.height
        };
        let scale = (476.0 / width.max(1) as f32).min(360.0 / height.max(1) as f32);
        div()
            .id("export-overlay")
            .debug_selector(|| "export-overlay".into())
            .absolute()
            .inset_0()
            .occlude()
            .flex()
            .items_center()
            .justify_center()
            .bg(rgba(0x00000080))
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_mouse_down(MouseButton::Right, |_, _, cx| cx.stop_propagation())
            .child(
                div()
                    .id("export-form")
                    .w(px(880.0))
                    .max_h_full()
                    .overflow_y_scroll()
                    .p(px(24.0))
                    .rounded(px(10.0))
                    .bg(rgb(0x2b2b2e))
                    .border_1()
                    .border_color(rgb(LINE))
                    .flex()
                    .flex_col()
                    .gap(px(20.0))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, _, _, cx| {
                            this.export_dialog.as_mut().unwrap().menu = None;
                            cx.notify();
                        }),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_between()
                            .child(
                                div()
                                    .text_size(px(20.0))
                                    .child(crate::i18n::t("editor", "Export", cx)),
                            )
                            .child(tool("export-close", Icon::Close, true).on_click(
                                cx.listener(|this, _, window, cx| this.close_export(window, cx)),
                            )),
                    )
                    .child(
                        div()
                            .flex()
                            .gap(px(24.0))
                            .child(
                                div()
                                    .w(px(476.0))
                                    .h(px(360.0))
                                    .flex_none()
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .bg(rgb(PANEL))
                                    .debug_selector(|| "export-preview".into())
                                    .child(
                                        div()
                                            .relative()
                                            .w(px(width.max(1) as f32 * scale))
                                            .h(px(height.max(1) as f32 * scale))
                                            .bg(rgb(0x000000))
                                            .overflow_hidden()
                                            .debug_selector(|| "export-canvas".into())
                                            .when_some(self.timeline.frame.clone(), |s, frame| {
                                                s.child(
                                                    img(frame)
                                                        .absolute()
                                                        .size_full()
                                                        .object_fit(ObjectFit::Contain),
                                                )
                                            }),
                                    ),
                            )
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .flex()
                                    .flex_col()
                                    .gap(px(12.0))
                                    .child(self.export_dropdown(ExportField::Aspect, cx))
                                    .child(self.export_dropdown(ExportField::Resolution, cx))
                                    .when(d.custom_resolution, |s| {
                                        s.child(
                                            div()
                                                .flex()
                                                .items_center()
                                                .gap(px(8.0))
                                                .child(
                                                    div()
                                                        .flex_1()
                                                        .min_w_0()
                                                        .child(self.export_number(0, cx)),
                                                )
                                                .child("×")
                                                .child(
                                                    div()
                                                        .flex_1()
                                                        .min_w_0()
                                                        .child(self.export_number(1, cx)),
                                                ),
                                        )
                                    })
                                    .child(self.export_dropdown(ExportField::Fps, cx))
                                    .child(self.export_dropdown(ExportField::Bitrate, cx))
                                    .when(
                                        d.bitrate_mode
                                            == dinuosi_core::video_settings::BitrateMode::Custom,
                                        |s| {
                                            s.child(
                                                div()
                                                    .flex()
                                                    .items_center()
                                                    .gap(px(8.0))
                                                    .child(
                                                        div()
                                                            .flex_1()
                                                            .min_w_0()
                                                            .child(self.export_number(2, cx)),
                                                    )
                                                    .child("kbps"),
                                            )
                                        },
                                    )
                                    .child(self.export_dropdown(ExportField::Container, cx))
                                    .child(self.export_dropdown(ExportField::Encoder, cx))
                                    .child(
                                        div()
                                            .flex()
                                            .flex_col()
                                            .gap(px(6.0))
                                            .child(
                                                div().text_color(rgb(MUTED)).child(crate::i18n::t(
                                                    "export",
                                                    "File name",
                                                    cx,
                                                )),
                                            )
                                            .child(if running {
                                                div()
                                                    .h(px(34.0))
                                                    .child(d.name.read(cx).text.clone())
                                            } else {
                                                div().child(d.name.clone())
                                            }),
                                    )
                                    .child(
                                        div()
                                            .id("export-folder")
                                            .debug_selector(|| "export-folder".into())
                                            .track_focus(&d.focus[6])
                                            .focus(|s| s.border_color(rgb(0x64646a)))
                                            .flex()
                                            .items_center()
                                            .gap(px(8.0))
                                            .min_h(px(34.0))
                                            .px(px(10.0))
                                            .border_1()
                                            .border_color(rgb(LINE))
                                            .rounded(px(4.0))
                                            .cursor_pointer()
                                            .child(
                                                svg()
                                                    .path(Icon::Folder.path())
                                                    .size(px(16.0))
                                                    .text_color(rgb(TEXT)),
                                            )
                                            .child(
                                                div().flex_1().min_w_0().overflow_hidden().child(
                                                    d.directory.as_ref().map_or_else(
                                                        || {
                                                            crate::i18n::t(
                                                                "export",
                                                                "Output folder",
                                                                cx,
                                                            )
                                                            .to_owned()
                                                        },
                                                        |p| p.to_string_lossy().into_owned(),
                                                    ),
                                                ),
                                            )
                                            .on_click(cx.listener(|this, _, window, cx| {
                                                this.choose_export_directory(window, cx)
                                            })),
                                    ),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(12.0))
                            .child(
                                div()
                                    .id("export-status")
                                    .flex_1()
                                    .min_w_0()
                                    .max_h(px(72.0))
                                    .overflow_y_scroll()
                                    .text_size(px(12.0))
                                    .when(running, |s| {
                                        s.child(if d.cancel.load(Ordering::Relaxed) {
                                            crate::i18n::t("export", "Cancelling…", cx).to_owned()
                                        } else {
                                            format!(
                                                "{} {progress:.1}%",
                                                crate::i18n::t("export", "Exporting", cx)
                                            )
                                        })
                                    })
                                    .when_some(d.status, |s, status| {
                                        s.child(crate::i18n::t("export", status, cx))
                                    })
                                    .when_some(d.error.clone(), |s, error| {
                                        s.child(crate::i18n::error(&error, cx))
                                    }),
                            )
                            .child(
                                pill(
                                    "export-cancel",
                                    crate::i18n::t(
                                        "export",
                                        if running { "Cancel" } else { "Close" },
                                        cx,
                                    ),
                                    90.0,
                                    32.0,
                                )
                                .track_focus(&d.focus[7])
                                .focus(|s| s.border_color(rgb(0x64646a)))
                                .on_click(
                                    cx.listener(|this, _, window, cx| {
                                        this.close_export(window, cx)
                                    }),
                                ),
                            )
                            .child(
                                pill(
                                    "export-submit",
                                    crate::i18n::t("editor", "Export", cx),
                                    110.0,
                                    32.0,
                                )
                                .track_focus(&d.focus[8])
                                .focus(|s| s.border_color(rgb(0x64646a)))
                                .when(!available, |s| s.opacity(0.4))
                                .on_click(
                                    cx.listener(|this, _, window, cx| {
                                        this.submit_export(window, cx)
                                    }),
                                ),
                            ),
                    ),
            )
    }
}
