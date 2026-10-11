//! 导出参数控件：项目预设、能力选项、下拉与键盘交互。
use crate::{
    export_dialog::ExportField,
    ui::{LINE, MUTED, PANEL, TEXT},
    view::Desktop,
};
use dinuosi_core::export::Container;
use dinuosi_icons::Icon;
use gpui::{Context, Div, MouseButton, div, prelude::*, px, rgb, svg};

impl ExportField {
    /// 返回控件的稳定标识与翻译键，顺序对应表单焦点循环；标识不进入视频元数据。
    fn info(self) -> (&'static str, &'static str, usize) {
        match self {
            Self::Resolution => ("export-resolution", "Resolution", 1),
            Self::Fps => ("export-fps", "Frame rate", 2),
            Self::Bitrate => ("export-bitrate", "Bitrate", 3),
            Self::Container => ("export-container", "Container", 4),
            Self::Encoder => ("export-encoder", "Encoder", 5),
        }
    }
}

impl Desktop {
    /// 枚举当前项目默认值及常用参数；能力探测决定编码器列表，空列表显示探测状态。
    /// 保留非预设项目值，不通过换帧率修改时间线；只改变本次编码参数。
    pub(crate) fn export_options(&self, field: ExportField, cx: &gpui::App) -> Vec<String> {
        let d = self.export_dialog.as_ref().unwrap();
        match field {
            ExportField::Resolution => {
                export_resolutions(self.timeline.model.width, self.timeline.model.height)
                    .into_iter()
                    .map(|(w, h)| format!("{w} × {h}"))
                    .collect()
            }
            ExportField::Fps => export_rates(self.timeline.model.fps, &[24, 25, 30, 50, 60])
                .into_iter()
                .map(|v| format!("{v} fps"))
                .collect(),
            ExportField::Bitrate => export_rates(
                self.timeline.model.bitrate_kbps,
                &[2000, 4000, 8000, 12000, 20000, 40000],
            )
            .into_iter()
            .map(|v| format!("{v} kbps"))
            .collect(),
            ExportField::Container => vec!["MP4".into(), "Matroska (.mkv)".into()],
            ExportField::Encoder => d
                .encoders
                .iter()
                .map(|e| crate::i18n::t("export", e.label(), cx).to_owned())
                .collect(),
        }
    }

    /// 根据所选行更新单个参数并收起菜单，运行中忽略修改；选择不影响项目配置或源文件。
    pub(crate) fn select_export_option(
        &mut self,
        field: ExportField,
        index: usize,
        cx: &mut Context<Self>,
    ) {
        let Some(d) = &mut self.export_dialog else {
            return;
        };
        if d.running || d.choosing || d.probing {
            return;
        }
        match field {
            ExportField::Resolution => {
                if let Some(&(w, h)) =
                    export_resolutions(self.timeline.model.width, self.timeline.model.height)
                        .get(index)
                {
                    d.settings.width = w;
                    d.settings.height = h;
                }
            }
            ExportField::Fps => {
                if let Some(&v) =
                    export_rates(self.timeline.model.fps, &[24, 25, 30, 50, 60]).get(index)
                {
                    d.settings.fps = v;
                }
            }
            ExportField::Bitrate => {
                if let Some(&v) = export_rates(
                    self.timeline.model.bitrate_kbps,
                    &[2000, 4000, 8000, 12000, 20000, 40000],
                )
                .get(index)
                {
                    d.settings.bitrate_kbps = v;
                }
            }
            ExportField::Container => {
                d.settings.container = if index == 0 {
                    Container::Mp4
                } else {
                    Container::Matroska
                };
            }
            ExportField::Encoder => {
                if let Some(&encoder) = d.encoders.get(index) {
                    d.settings.encoder = encoder;
                }
            }
        }
        d.menu = None;
        d.status = None;
        d.error = None;
        cx.notify();
    }

    /// 绘制统一下拉控件，菜单延后绘制以避开后续行遮挡；鼠标、Enter/Space 与方向键均可选择。
    pub(crate) fn export_dropdown(&self, field: ExportField, cx: &mut Context<Self>) -> Div {
        let d = self.export_dialog.as_ref().unwrap();
        let (id, label, focus) = field.info();
        let value = match field {
            ExportField::Resolution => format!("{} × {}", d.settings.width, d.settings.height),
            ExportField::Fps => format!("{} fps", d.settings.fps),
            ExportField::Bitrate => format!("{} kbps", d.settings.bitrate_kbps),
            ExportField::Container => if d.settings.container == Container::Mp4 {
                "MP4"
            } else {
                "Matroska (.mkv)"
            }
            .into(),
            ExportField::Encoder => if d.probing {
                crate::i18n::t("export", "Checking encoders…", cx)
            } else if d.encoders.is_empty() {
                crate::i18n::t("export", "Unavailable", cx)
            } else {
                crate::i18n::t("export", d.settings.encoder.label(), cx)
            }
            .into(),
        };
        let options = self.export_options(field, cx);
        let selected = options.iter().position(|s| s == &value).unwrap_or(0);
        div()
            .flex()
            .flex_col()
            .gap(px(6.0))
            .child(
                div()
                    .text_color(rgb(MUTED))
                    .child(crate::i18n::t("export", label, cx)),
            )
            .child(
                div()
                    .relative()
                    .child(
                        div()
                            .id(id)
                            .debug_selector(move || id.into())
                            .track_focus(&d.focus[focus])
                            .focus(|s| s.border_color(rgb(0x64646a)))
                            .flex()
                            .items_center()
                            .justify_between()
                            .h(px(34.0))
                            .px(px(10.0))
                            .rounded(px(4.0))
                            .border_1()
                            .border_color(rgb(LINE))
                            .bg(rgb(PANEL))
                            .cursor_pointer()
                            .hover(|s| s.bg(rgb(LINE)))
                            .child(value)
                            .child(
                                svg()
                                    .path(Icon::ChevronDown.path())
                                    .size(px(14.0))
                                    .text_color(rgb(TEXT)),
                            )
                            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                            .on_click(cx.listener(move |this, _, window, cx| {
                                let d = this.export_dialog.as_mut().unwrap();
                                if !d.running && !d.choosing && !d.probing {
                                    d.focus[focus].focus(window);
                                    d.menu = (d.menu != Some(field)).then_some(field);
                                    cx.notify();
                                }
                            }))
                            .on_key_down(cx.listener(
                                move |this, event: &gpui::KeyDownEvent, _, cx| {
                                    if event.keystroke.modifiers.modified() {
                                        return;
                                    }
                                    let d = this.export_dialog.as_mut().unwrap();
                                    if d.running || d.choosing || d.probing {
                                        return;
                                    }
                                    match event.keystroke.key.as_str() {
                                        "enter" | "space" if !event.is_held => {
                                            d.menu = (d.menu != Some(field)).then_some(field)
                                        }
                                        "up" => this.select_export_option(
                                            field,
                                            selected.saturating_sub(1),
                                            cx,
                                        ),
                                        "down" => this.select_export_option(
                                            field,
                                            (selected + 1).min(options.len().saturating_sub(1)),
                                            cx,
                                        ),
                                        _ => return,
                                    }
                                    cx.stop_propagation();
                                    cx.notify();
                                },
                            )),
                    )
                    .when(d.menu == Some(field), |s| {
                        s.child(
                            gpui::deferred(
                                div()
                                    .absolute()
                                    .top(px(37.0))
                                    .left_0()
                                    .w_full()
                                    .p(px(4.0))
                                    .bg(rgb(PANEL))
                                    .border_1()
                                    .border_color(rgb(LINE))
                                    .rounded(px(4.0))
                                    .occlude()
                                    .on_mouse_down(MouseButton::Left, |_, _, cx| {
                                        cx.stop_propagation()
                                    })
                                    .children(
                                        self.export_options(field, cx).into_iter().enumerate().map(
                                            |(index, label)| {
                                                div()
                                                    .id((id, index))
                                                    .debug_selector(move || format!("{id}-{index}"))
                                                    .px(px(8.0))
                                                    .py(px(7.0))
                                                    .cursor_pointer()
                                                    .hover(|s| s.bg(rgb(LINE)))
                                                    .when(index == selected, |s| s.bg(rgb(LINE)))
                                                    .child(label)
                                                    .on_click(cx.listener(move |this, _, _, cx| {
                                                        this.select_export_option(field, index, cx)
                                                    }))
                                            },
                                        ),
                                    ),
                            )
                            .with_priority(3),
                        )
                    }),
            )
    }
}

/// 返回项目分辨率及常用编码尺寸，保持唯一且不把不同宽高比强制写入项目。
fn export_resolutions(width: u32, height: u32) -> Vec<(u32, u32)> {
    let mut values = vec![(width, height), (1280, 720), (1920, 1080), (3840, 2160)];
    values.sort_unstable();
    values.dedup();
    values
}

/// 把项目帧率或码率合并到预设并排序去重，保证非标准项目设置也能直接导出。
fn export_rates(current: u32, presets: &[u32]) -> Vec<u32> {
    let mut values = presets.to_vec();
    values.push(current);
    values.sort_unstable();
    values.dedup();
    values
}
