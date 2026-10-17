//! 项目画幅、分辨率、帧率、素材适配和质量草稿；应用前不修改编辑会话。
use crate::{
    settings::Settings,
    settings_view::row,
    ui::{LINE, PANEL, TEXT},
    video_presets as presets,
};
use dinuosi_core::video_settings::{BitrateMode, MediaFit};
use dinuosi_icons::Icon;
use gpui::{Context, Div, EntityInputHandler, MouseButton, Window, div, prelude::*, px, rgb, svg};

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum ProjectDropdown {
    Aspect,
    Resolution,
    Fps,
    Fit,
    Quality,
}

impl ProjectDropdown {
    /// 返回稳定的控件标识和翻译键，供鼠标、键盘与真实窗口验证共同定位。
    fn info(self) -> (&'static str, &'static str) {
        match self {
            Self::Aspect => ("project-aspect", "Aspect ratio"),
            Self::Resolution => ("project-resolution", "Resolution"),
            Self::Fps => ("project-fps", "Frame rate (fps)"),
            Self::Fit => ("project-fit", "Media fitting"),
            Self::Quality => ("project-quality", "Export quality"),
        }
    }
}

impl Settings {
    /// 读取项目草稿的合法数值，编辑中无效输入仅用于菜单显示回退，提交时仍严格拒绝。
    fn project_value(&self, index: usize, cx: &gpui::App) -> u32 {
        self.fields[index].read(cx).text.trim().parse().unwrap_or(0)
    }

    /// 用输入实体的标准替换路径更新预设数值，同时重置选择与输入法状态；不直接写磁盘。
    fn set_project_value(
        &mut self,
        index: usize,
        value: u32,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.fields[index].update(cx, |input, cx| {
            let end = input.text.encode_utf16().count();
            input.replace_text_in_range(Some(0..end), &value.to_string(), window, cx);
        });
    }

    /// 构造项目参数下拉列表及当前索引；保留非预设帧率，自定义尺寸始终有独立入口。
    fn project_options(&self, kind: ProjectDropdown, cx: &gpui::App) -> (Vec<String>, usize) {
        let labels = |values: &[&'static str]| {
            values
                .iter()
                .map(|key| crate::i18n::t("settings", key, cx).to_owned())
                .collect()
        };
        match kind {
            ProjectDropdown::Aspect => (labels(presets::ASPECTS), self.project_aspect),
            ProjectDropdown::Fit => (labels(presets::FITS), presets::fit_index(self.media_fit)),
            ProjectDropdown::Quality => (
                labels(presets::QUALITY),
                presets::quality_index(self.bitrate_mode),
            ),
            ProjectDropdown::Fps => {
                let current = self.project_value(2, cx);
                let values = presets::rates(current);
                let index = values.iter().position(|v| *v == current).unwrap_or(0);
                (values.iter().map(|v| format!("{v} fps")).collect(), index)
            }
            ProjectDropdown::Resolution => {
                let values = presets::resolutions(self.project_aspect);
                let current = (self.project_value(0, cx), self.project_value(1, cx));
                let index = if self.custom_resolution {
                    3
                } else {
                    values.iter().position(|v| *v == current).unwrap_or(3)
                };
                let mut labels: Vec<_> = values
                    .iter()
                    .map(|(w, h)| presets::resolution_label(*w, *h))
                    .collect();
                labels.push(crate::i18n::t("settings", "Custom", cx).into());
                (labels, index)
            }
        }
    }

    /// 应用选中的预设至草稿并关闭菜单；画幅切换保持清晰度档，素材适配不修改已有变换。
    fn select_project_option(
        &mut self,
        kind: ProjectDropdown,
        index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match kind {
            ProjectDropdown::Aspect => {
                let (w, h) = presets::change_aspect(
                    self.project_value(0, cx),
                    self.project_value(1, cx),
                    index,
                );
                self.project_aspect = index;
                self.custom_resolution = index == 3;
                self.set_project_value(0, w, window, cx);
                self.set_project_value(1, h, window, cx);
            }
            ProjectDropdown::Resolution => {
                self.custom_resolution = index == 3;
                if self.custom_resolution {
                    self.project_aspect = 3;
                }
                if let Some(&(w, h)) = presets::resolutions(self.project_aspect).get(index) {
                    self.project_aspect = presets::aspect(w, h);
                    self.set_project_value(0, w, window, cx);
                    self.set_project_value(1, h, window, cx);
                }
            }
            ProjectDropdown::Fps => {
                if let Some(&value) = presets::rates(self.project_value(2, cx)).get(index) {
                    self.set_project_value(2, value, window, cx);
                }
            }
            ProjectDropdown::Fit => {
                self.media_fit = [MediaFit::Fit, MediaFit::Fill, MediaFit::Original][index]
            }
            ProjectDropdown::Quality => {
                self.bitrate_mode = [
                    BitrateMode::Standard,
                    BitrateMode::High,
                    BitrateMode::Custom,
                ][index]
            }
        }
        self.project_menu = None;
        self.error = None;
        cx.notify();
    }

    /// 绘制现有灰阶样式的参数下拉，支持鼠标及键盘；浮层自动夹入窗口边界，长菜单不越出底边。
    fn project_dropdown(&self, kind: ProjectDropdown, cx: &mut Context<Self>) -> Div {
        let (id, _) = kind.info();
        let (options, selected) = self.project_options(kind, cx);
        let value = options[selected].clone();
        let count = options.len();
        div()
            .relative()
            .w(px(278.0))
            .child(
                div()
                    .id(id)
                    .debug_selector(move || id.into())
                    .focusable()
                    .flex()
                    .items_center()
                    .justify_between()
                    .px(px(12.0))
                    .h(px(36.0))
                    .rounded(px(4.0))
                    .bg(rgb(PANEL))
                    .border_1()
                    .border_color(rgb(LINE))
                    .cursor_pointer()
                    .hover(|s| s.bg(rgb(LINE)))
                    .child(value)
                    .child(
                        svg()
                            .path(Icon::ChevronDown.path())
                            .size(px(16.0))
                            .text_color(rgb(TEXT)),
                    )
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.general_menu = None;
                        this.project_menu = (this.project_menu != Some(kind)).then_some(kind);
                        cx.notify();
                    }))
                    .on_key_down(cx.listener(
                        move |this, event: &gpui::KeyDownEvent, window, cx| {
                            if event.keystroke.modifiers.modified() {
                                return;
                            }
                            match event.keystroke.key.as_str() {
                                "enter" | "space" if !event.is_held => {
                                    this.project_menu =
                                        (this.project_menu != Some(kind)).then_some(kind)
                                }
                                "up" => this.select_project_option(
                                    kind,
                                    selected.saturating_sub(1),
                                    window,
                                    cx,
                                ),
                                "down" => this.select_project_option(
                                    kind,
                                    (selected + 1).min(count - 1),
                                    window,
                                    cx,
                                ),
                                "escape" if this.project_menu.is_some() => this.project_menu = None,
                                _ => return,
                            }
                            cx.stop_propagation();
                            cx.notify();
                        },
                    )),
            )
            .when(self.project_menu == Some(kind), |s| {
                s.child(
                    gpui::deferred(
                        gpui::anchored()
                            .position_mode(gpui::AnchoredPositionMode::Local)
                            .position(gpui::point(px(0.0), px(4.0)))
                            .snap_to_window()
                            .child(
                                div()
                                    .w(px(278.0))
                                    .p(px(4.0))
                                    .rounded(px(4.0))
                                    .bg(rgb(PANEL))
                                    .border_1()
                                    .border_color(rgb(LINE))
                                    .occlude()
                                    .on_mouse_down(MouseButton::Left, |_, _, cx| {
                                        cx.stop_propagation()
                                    })
                                    .children(options.into_iter().enumerate().map(
                                        |(index, label)| {
                                            div()
                                                .id((id, index))
                                                .debug_selector(move || format!("{id}-{index}"))
                                                .px(px(8.0))
                                                .py(px(7.0))
                                                .cursor_pointer()
                                                .when(index == selected, |s| s.bg(rgb(LINE)))
                                                .hover(|s| s.bg(rgb(LINE)))
                                                .child(label)
                                                .on_click(cx.listener(
                                                    move |this, _, window, cx| {
                                                        this.select_project_option(
                                                            kind, index, window, cx,
                                                        )
                                                    },
                                                ))
                                        },
                                    )),
                            ),
                    )
                    .with_priority(3),
                )
            })
    }

    /// 按原设置行样式组合画幅/尺寸/帧率/素材适配/质量；只有选择自定义才显示数值输入。
    pub(crate) fn project_body(&self, cx: &mut Context<Self>) -> Div {
        let mut content = div().flex().flex_col().gap(px(8.0));
        for kind in [ProjectDropdown::Aspect, ProjectDropdown::Resolution] {
            content = content.child(row(
                crate::i18n::t("settings", kind.info().1, cx),
                self.project_dropdown(kind, cx),
            ));
        }
        content = content.when(self.custom_resolution, |s| {
            s.child(row(
                crate::i18n::t("settings", "Size (pixels)", cx),
                div()
                    .w(px(278.0))
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .child(self.fields[0].clone())
                    .child("×")
                    .child(self.fields[1].clone()),
            ))
        });
        for kind in [
            ProjectDropdown::Fps,
            ProjectDropdown::Fit,
            ProjectDropdown::Quality,
        ] {
            content = content.child(row(
                crate::i18n::t("settings", kind.info().1, cx),
                self.project_dropdown(kind, cx),
            ));
        }
        content.when(self.bitrate_mode == BitrateMode::Custom, |s| {
            s.child(row(
                crate::i18n::t("settings", "Bitrate (kbps)", cx),
                div().w(px(278.0)).child(self.fields[3].clone()),
            ))
        })
    }
}
