//! 检查器项目概览；复用现有灰阶和字段行，不添加新的设置入口。
use crate::{
    timeline_state::InspectorTab,
    ui::{MUTED, TEXT},
    view::Desktop,
};
use gpui::{Context, Div, div, prelude::*, px, rgb};
impl Desktop {
    /// 未选中片段时显示当前项目名称、输出参数及剪辑时长；数据来自当前模型，修改设置后立即刷新。
    pub(crate) fn project_inspector(&self, cx: &mut Context<Self>) -> Div {
        let model = &self.timeline.model;
        let name = self
            .current
            .as_ref()
            .map_or("", |e| e.name.as_str())
            .to_owned();
        let fields = [
            ("Project name", name),
            ("Resolution", format!("{} × {}", model.width, model.height)),
            ("Frame rate", format!("{} fps", model.fps)),
            ("Bitrate", format!("{} kbps", model.bitrate_kbps)),
            (
                "Duration",
                crate::timeline_markers::timecode(model.end(), model.fps),
            ),
        ];
        div()
            .pt(px(26.0))
            .px(px(20.0))
            .children(fields.into_iter().map(|(label, value)| {
                div()
                    .flex()
                    .items_center()
                    .gap(px(12.0))
                    .min_h(px(37.0))
                    .text_size(px(12.0))
                    .child(
                        div()
                            .w(px(85.0))
                            .flex_none()
                            .text_color(rgb(MUTED))
                            .child(crate::i18n::t("inspector", label, cx)),
                    )
                    .child(div().flex_1().min_w_0().text_color(rgb(TEXT)).child(value))
            }))
    }
    /// 根据当前选择生成“基本/项目”“标记点”“变速”页签；仅选中视频/音频才显示变速，切换前提交输入。
    pub(crate) fn inspector_tabs(&self, empty: bool, cx: &mut Context<Self>) -> Div {
        div()
            .h(px(37.0))
            .flex()
            .gap(px(20.0))
            .pl(px(20.0))
            .border_b_1()
            .border_color(rgb(crate::ui::LINE))
            .children(
                [
                    InspectorTab::Basic,
                    InspectorTab::Markers,
                    InspectorTab::Speed,
                    InspectorTab::Color,
                ]
                .into_iter()
                .filter(|tab| *tab != InspectorTab::Speed || self.speed_clip().is_some())
                .filter(|tab| *tab != InspectorTab::Color || self.color_clip().is_some())
                .map(|tab| {
                    let id = match tab {
                        InspectorTab::Markers => "inspector-markers-tab",
                        InspectorTab::Speed => "inspector-speed-tab",
                        InspectorTab::Color => "inspector-color-tab",
                        InspectorTab::Basic => "inspector-basic-tab",
                    };
                    div()
                        .id(id)
                        .debug_selector(move || id.into())
                        .h_full()
                        .pt(px(7.0))
                        .pb(px(8.0))
                        .cursor_pointer()
                        .whitespace_nowrap()
                        .text_size(px(12.0))
                        .when(
                            (self.timeline.inspector_tab == tab
                                && (tab != InspectorTab::Speed || self.speed_clip().is_some()))
                                || (tab == InspectorTab::Basic
                                    && ((self.timeline.inspector_tab == InspectorTab::Speed
                                        && self.speed_clip().is_none())
                                        || (self.timeline.inspector_tab == InspectorTab::Color
                                            && self.color_clip().is_none()))),
                            |s| s.border_b_1().border_color(rgb(TEXT)),
                        )
                        .child(crate::i18n::t(
                            "inspector",
                            if tab == InspectorTab::Color {
                                "Color"
                            } else if tab == InspectorTab::Markers {
                                "Markers"
                            } else if tab == InspectorTab::Speed {
                                "Speed"
                            } else if empty {
                                "Project"
                            } else {
                                "Basic"
                            },
                            cx,
                        ))
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.commit_inspector_input(cx);
                            this.timeline.inspector_tab = tab;
                            this.focus_handle.focus(window);
                            cx.notify();
                        }))
                }),
            )
    }
}
