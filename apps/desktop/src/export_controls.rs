//! 导出参数控件：项目预设、能力选项、下拉与键盘交互。
use crate::{
    export_dialog::ExportField,
    ui::{LINE, MUTED, PANEL, TEXT},
    video_presets as presets,
    view::Desktop,
};
use dinuosi_core::export::Container;
use dinuosi_icons::Icon;
use gpui::{Context, Div, MouseButton, div, prelude::*, px, rgb, svg};

impl ExportField {
    /// 返回控件的稳定标识与翻译键，顺序对应表单焦点循环；标识不进入视频元数据。
    fn info(self) -> (&'static str, &'static str, usize) {
        match self {
            Self::Aspect => ("export-aspect", "Aspect ratio", 9),
            Self::Resolution => ("export-resolution", "Resolution", 1),
            Self::Fps => ("export-fps", "Frame rate", 2),
            Self::Bitrate => ("export-bitrate", "Quality", 3),
            Self::Container => ("export-container", "Container", 4),
            Self::Encoder => ("export-encoder", "Encoder", 5),
        }
    }
}

impl Desktop {
    /// 绘制自定义导出数值；编码进行时只显示快照文本，防止仍可编辑已锁定的任务参数。
    pub(crate) fn export_number(&self, index: usize, cx: &gpui::App) -> Div {
        let d = self.export_dialog.as_ref().unwrap();
        if d.running {
            div()
                .h(px(34.0))
                .child(d.fields[index].read(cx).text.clone())
        } else {
            div().child(d.fields[index].clone())
        }
    }
    /// 枚举跟随项目、横竖屏/方形清晰度预设、自定义与质量档位；能力探测决定编码器列表。
    /// 保留非预设项目帧率，所有选择仅用于本次输出，不修改时间线。
    pub(crate) fn export_options(&self, field: ExportField, cx: &gpui::App) -> Vec<String> {
        let d = self.export_dialog.as_ref().unwrap();
        match field {
            ExportField::Aspect => presets::ASPECTS
                .iter()
                .map(|key| crate::i18n::t("export", key, cx).to_owned())
                .collect(),
            ExportField::Resolution => {
                let mut values = vec![format!(
                    "{} · {} × {}",
                    crate::i18n::t("export", "Match project", cx),
                    self.timeline.model.width,
                    self.timeline.model.height
                )];
                values.extend(
                    presets::resolutions(d.aspect)
                        .into_iter()
                        .map(|(w, h)| presets::resolution_label(w, h)),
                );
                values.push(crate::i18n::t("export", "Custom", cx).into());
                values
            }
            ExportField::Fps => std::iter::once(format!(
                "{} · {} fps",
                crate::i18n::t("export", "Match project", cx),
                self.timeline.model.fps
            ))
            .chain(
                presets::rates(self.timeline.model.fps)
                    .into_iter()
                    .map(|v| format!("{v} fps")),
            )
            .collect(),
            ExportField::Bitrate => presets::QUALITY
                .iter()
                .map(|key| crate::i18n::t("export", key, cx).to_owned())
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
        window: &mut gpui::Window,
        cx: &mut Context<Self>,
    ) {
        let Some(d) = &mut self.export_dialog else {
            return;
        };
        if d.running || d.choosing || d.probing {
            return;
        }
        match field {
            ExportField::Aspect => {
                (d.settings.width, d.settings.height) =
                    presets::change_aspect(d.settings.width, d.settings.height, index);
                d.aspect = index;
                d.follow_resolution = false;
                d.custom_resolution = index == 3;
            }
            ExportField::Resolution => {
                d.follow_resolution = index == 0;
                d.custom_resolution = index == 4;
                if d.custom_resolution {
                    d.aspect = 3;
                }
                if index == 0 {
                    d.settings.width = self.timeline.model.width;
                    d.settings.height = self.timeline.model.height;
                    d.aspect = presets::aspect(d.settings.width, d.settings.height);
                } else if let Some(&(w, h)) = presets::resolutions(d.aspect).get(index - 1) {
                    d.settings.width = w;
                    d.settings.height = h;
                    d.aspect = presets::aspect(w, h);
                }
            }
            ExportField::Fps => {
                d.follow_fps = index == 0;
                if index == 0 {
                    d.settings.fps = self.timeline.model.fps;
                } else if let Some(&v) = presets::rates(self.timeline.model.fps).get(index - 1) {
                    d.settings.fps = v;
                }
            }
            ExportField::Bitrate => {
                d.bitrate_mode = [
                    dinuosi_core::video_settings::BitrateMode::Standard,
                    dinuosi_core::video_settings::BitrateMode::High,
                    dinuosi_core::video_settings::BitrateMode::Custom,
                ][index];
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
        if matches!(field, ExportField::Aspect | ExportField::Resolution) {
            for (input, value) in d.fields.iter().zip([d.settings.width, d.settings.height]) {
                input.update(cx, |input, cx| {
                    use gpui::EntityInputHandler;
                    let end = input.text.encode_utf16().count();
                    input.replace_text_in_range(Some(0..end), &value.to_string(), window, cx);
                });
            }
        }
        d.settings.bitrate_kbps = d.bitrate_mode.bitrate(
            d.settings.width,
            d.settings.height,
            d.settings.fps,
            d.settings.bitrate_kbps,
        );
        d.menu = None;
        d.status = None;
        d.error = None;
        cx.notify();
    }

    /// 绘制统一下拉控件，菜单延后绘制以避开后续行遮挡；鼠标、Enter/Space 与方向键均可选择。
    pub(crate) fn export_dropdown(&self, field: ExportField, cx: &mut Context<Self>) -> Div {
        let d = self.export_dialog.as_ref().unwrap();
        let (id, label, focus) = field.info();
        let options = self.export_options(field, cx);
        let value = match field {
            ExportField::Aspect => crate::i18n::t("export", presets::ASPECTS[d.aspect], cx).into(),
            ExportField::Resolution if d.follow_resolution => options[0].clone(),
            ExportField::Resolution if d.custom_resolution => {
                crate::i18n::t("export", "Custom", cx).into()
            }
            ExportField::Resolution => {
                presets::resolution_label(d.settings.width, d.settings.height)
            }
            ExportField::Fps if d.follow_fps => options[0].clone(),
            ExportField::Fps => format!("{} fps", d.settings.fps),
            ExportField::Bitrate => crate::i18n::t(
                "export",
                presets::QUALITY[presets::quality_index(d.bitrate_mode)],
                cx,
            )
            .into(),
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
                                move |this, event: &gpui::KeyDownEvent, window, cx| {
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
                                            window,
                                            cx,
                                        ),
                                        "down" => this.select_export_option(
                                            field,
                                            (selected + 1).min(options.len().saturating_sub(1)),
                                            window,
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
                                                    .on_click(cx.listener(
                                                        move |this, _, window, cx| {
                                                            this.select_export_option(
                                                                field, index, window, cx,
                                                            )
                                                        },
                                                    ))
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
