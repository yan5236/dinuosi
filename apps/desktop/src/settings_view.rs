//! 设置界面：顶部搜索、左侧分类和右侧参数；使用现有灰阶及原生输入控件。
use crate::{
    settings::{Settings, TABS, Tab},
    shortcuts,
    switch::Switch,
    ui::{self, BACKGROUND, LINE, MUTED, PANEL, TEXT},
};
use dinuosi_core::language::{self, LANGUAGES};
use dinuosi_icons::Icon;
use gpui::{Context, Div, Render, Stateful, Window, div, prelude::*, px, rgb};

/// 绘制可聚焦的矩形设置按钮，不引入新的品牌样式或装饰文案。
fn button(id: impl Into<gpui::ElementId>, text: impl Into<gpui::SharedString>) -> Stateful<Div> {
    div()
        .id(id)
        .focusable()
        .px(px(12.0))
        .py(px(8.0))
        .rounded(px(4.0))
        .bg(rgb(PANEL))
        .border_1()
        .border_color(rgb(LINE))
        .cursor_pointer()
        .hover(|s| s.bg(rgb(LINE)))
        .child(text.into())
}

/// 每个设置行左侧显示字段名，右侧提供固定区域控件，避免长文本挤出边界。
fn row(label: &'static str, control: impl IntoElement) -> Div {
    div()
        .flex()
        .items_center()
        .justify_between()
        .gap(px(20.0))
        .py(px(12.0))
        .border_b_1()
        .border_color(rgb(LINE))
        .child(label)
        .child(control)
}

impl Settings {
    /// 渲染当前分类的实际控件；通用页通过受控 Switch 修改草稿，应用前不保存偏好。
    /// 语言页读取统一索引并显示单选勾选、自称和当前已应用语言译名；插件显示空列表。
    fn body(&self, cx: &mut Context<Self>) -> Div {
        let content = div().flex().flex_col().gap(px(8.0));
        match self.tab {
            Tab::General => content.child(row(
                "拖动时实时预览",
                Switch::new(
                    "live-scrub",
                    self.draft.live_scrub,
                    cx.listener(|this, checked: &bool, _, cx| {
                        this.draft.live_scrub = *checked;
                        cx.notify();
                    }),
                ),
            )),
            Tab::Language => self.language_list(cx),
            Tab::Plugins => {
                content.child(div().py(px(20.0)).text_color(rgb(MUTED)).child("暂无插件"))
            }
            Tab::Project => content
                .child(row(
                    "主轨分辨率",
                    div()
                        .w(px(278.0))
                        .flex()
                        .items_center()
                        .gap(px(8.0))
                        .child(self.fields[0].clone())
                        .child("×")
                        .child(self.fields[1].clone()),
                ))
                .child(row(
                    "帧率（fps）",
                    div().w(px(160.0)).child(self.fields[2].clone()),
                ))
                .child(row(
                    "码率（kbps）",
                    div().w(px(160.0)).child(self.fields[3].clone()),
                )),
            Tab::Shortcuts => {
                let query = self.search.read(cx).text.trim().to_lowercase();
                content
                    .children(
                        shortcuts::SHORTCUTS
                            .iter()
                            .enumerate()
                            .filter(|(_, item)| {
                                query.is_empty()
                                    || Tab::Shortcuts.label().contains(&query)
                                    || item.label.contains(&query)
                                    || shortcuts::label(&shortcuts::bindings(item, &self.draft))
                                        .to_lowercase()
                                        .contains(&query)
                            })
                            .map(|(index, item)| {
                                let keys = shortcuts::bindings(item, &self.draft);
                                let text = if self.recording == Some(index) {
                                    "按下快捷键…".into()
                                } else {
                                    shortcuts::label(&keys)
                                };
                                row(
                                    item.label,
                                    div()
                                        .flex()
                                        .gap(px(6.0))
                                        .child(
                                            button(("binding", index), text)
                                                .min_w(px(165.0))
                                                .debug_selector(move || format!("binding-{index}"))
                                                .on_click(cx.listener(
                                                    move |this, _, window, cx| {
                                                        this.recording = Some(index);
                                                        this.error = None;
                                                        this.focus.focus(window);
                                                        cx.notify();
                                                    },
                                                )),
                                        )
                                        .child(button(("clear", index), "清除").on_click(
                                            cx.listener(move |this, _, _, cx| {
                                                this.draft
                                                    .shortcuts
                                                    .insert(item.id.into(), Vec::new());
                                                this.recording = None;
                                                this.error = None;
                                                cx.notify();
                                            }),
                                        )),
                                )
                            }),
                    )
                    .child(
                        button("reset-shortcuts", "恢复默认快捷键")
                            .debug_selector(|| "reset-shortcuts".into())
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.draft.shortcuts.clear();
                                this.recording = None;
                                this.error = None;
                                cx.notify();
                            })),
                    )
            }
        }
    }

    /// 绘制索引中的六个语言行，三列固定为 28px 勾选区、160px 自称及弹性译名区。
    /// 点击或空格/Enter 仅更新单选草稿；译名使用已应用偏好，auto 按系统语言解析。
    fn language_list(&self, cx: &mut Context<Self>) -> Div {
        let locale = language::resolve(
            &cx.global::<crate::preferences::UserPreferences>()
                .0
                .language,
            self.system_locale.as_deref(),
        );
        div()
            .flex()
            .flex_col()
            .children(LANGUAGES.iter().map(|item| {
                let id = item.id;
                let checked = self.draft.language == id;
                let name = item.name(locale);
                div()
                    .id(id)
                    .debug_selector(move || format!("language-{id}"))
                    .focusable()
                    .tab_index(0)
                    .flex()
                    .items_center()
                    .gap(px(16.0))
                    .px(px(12.0))
                    .h(px(48.0))
                    .border_b_1()
                    .border_color(rgb(LINE))
                    .cursor_pointer()
                    .hover(|s| s.bg(rgb(PANEL)))
                    .focus(|s| s.bg(rgb(LINE)))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.draft.language = id.into();
                        cx.notify();
                    }))
                    .on_key_down(cx.listener(move |this, event: &gpui::KeyDownEvent, _, cx| {
                        if !event.keystroke.modifiers.modified()
                            && matches!(event.keystroke.key.as_str(), "space" | "enter")
                        {
                            cx.stop_propagation();
                            if !event.is_held {
                                this.draft.language = id.into();
                                cx.notify();
                            }
                        }
                    }))
                    .child(
                        div().w(px(28.0)).flex_none().child(
                            div()
                                .debug_selector(move || format!("language-check-{id}"))
                                .size(px(if checked { 18.0 } else { 0.0 }))
                                .when(checked, |s| {
                                    s.child(
                                        gpui::svg()
                                            .path(Icon::Check.path())
                                            .size(px(18.0))
                                            .text_color(rgb(TEXT)),
                                    )
                                }),
                        ),
                    )
                    .child(
                        div()
                            .w(px(160.0))
                            .flex_none()
                            .debug_selector(move || format!("language-native-{id}"))
                            .child(if id == "auto" { name } else { item.native_name }),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .text_color(rgb(MUTED))
                            .debug_selector(move || format!("language-name-{id}"))
                            .child(name),
                    )
            }))
    }
}

impl Render for Settings {
    /// 渲染独立窗口与搜索结果；底部取消/应用复用导出胶囊按钮，错误只展示实际操作失败。
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let tabs: Vec<_> = TABS
            .into_iter()
            .filter(|t| (*t != Tab::Project || self.project) && self.matches(*t, cx))
            .collect();
        if !tabs.contains(&self.tab)
            && let Some(tab) = tabs.first()
        {
            self.tab = *tab;
        }
        div()
            .size_full()
            .flex()
            .flex_col()
            .bg(rgb(BACKGROUND))
            .text_color(rgb(TEXT))
            .text_size(px(14.0))
            .font_family("Microsoft YaHei UI")
            .on_key_down(cx.listener(Self::key))
            .child(div().absolute().size_0().track_focus(&self.focus))
            .child(
                div()
                    .h(px(48.0))
                    .flex_none()
                    .px(px(16.0))
                    .flex()
                    .items_center()
                    .child("设置")
                    .child(ui::title_drag())
                    .child(ui::window_controls(window, false)),
            )
            .child(div().px(px(18.0)).pb(px(16.0)).child(self.search.clone()))
            .child(
                div()
                    .flex()
                    .flex_1()
                    .min_h_0()
                    .border_t_1()
                    .border_color(rgb(LINE))
                    .child(
                        div()
                            .w(px(180.0))
                            .flex_none()
                            .p(px(12.0))
                            .border_r_1()
                            .border_color(rgb(LINE))
                            .children(tabs.iter().map(|tab| {
                                let tab = *tab;
                                div()
                                    .when(tab == Tab::Project, |s| {
                                        s.child(
                                            div()
                                                .pt(px(22.0))
                                                .pb(px(8.0))
                                                .px(px(12.0))
                                                .text_color(rgb(MUTED))
                                                .child("项目设置"),
                                        )
                                    })
                                    .child(
                                        div()
                                            .id(tab.label())
                                            .focusable()
                                            .debug_selector(move || {
                                                format!("settings-tab-{}", tab.label())
                                            })
                                            .px(px(12.0))
                                            .py(px(12.0))
                                            .rounded(px(4.0))
                                            .cursor_pointer()
                                            .when(tab == self.tab, |s| s.bg(rgb(LINE)))
                                            .hover(|s| s.bg(rgb(PANEL)))
                                            .child(tab.label())
                                            .on_click(cx.listener(move |this, _, window, cx| {
                                                this.tab = tab;
                                                this.recording = None;
                                                this.error = None;
                                                this.focus.focus(window);
                                                cx.notify();
                                            })),
                                    )
                            })),
                    )
                    .child(
                        div()
                            .id("settings-content")
                            .flex_1()
                            .min_w_0()
                            .overflow_y_scroll()
                            .p(px(24.0))
                            .when(!tabs.is_empty(), |s| {
                                s.child(
                                    div()
                                        .text_size(px(18.0))
                                        .mb(px(18.0))
                                        .child(self.tab.label()),
                                )
                                .child(self.body(cx))
                            })
                            .when(tabs.is_empty(), |s| {
                                s.child(div().text_color(rgb(MUTED)).child("无匹配设置"))
                            }),
                    ),
            )
            .child(
                div()
                    .flex_none()
                    .p(px(14.0))
                    .border_t_1()
                    .border_color(rgb(LINE))
                    .flex()
                    .items_center()
                    .gap(px(12.0))
                    .child(
                        div()
                            .flex_1()
                            .text_color(rgb(0xe6a39c))
                            .children(self.error.clone()),
                    )
                    .child(
                        ui::pill("settings-close", "取消", 110.0, 26.0)
                            .on_click(|_, window, _| window.remove_window()),
                    )
                    .when(!tabs.is_empty() && self.tab != Tab::Plugins, |s| {
                        s.child(
                            ui::pill("settings-apply", "应用", 110.0, 26.0)
                                .debug_selector(|| "settings-apply".into())
                                .on_click(cx.listener(|this, _, _, cx| this.apply(cx))),
                        )
                    }),
            )
    }
}
