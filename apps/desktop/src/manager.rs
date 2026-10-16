//! 固定尺寸的项目管理列表，复用现有品牌、轻线图标和灰阶控件。

use crate::{
    dialogs::DialogKind,
    ui::{self, LINE, MUTED, PANEL, TEXT, brand, pill, tool},
    view::Desktop,
};
use dinuosi_command::ProjectCommand;
use dinuosi_core::ProjectEntry;
use dinuosi_icons::Icon;
use gpui::{Context, Div, Window, div, prelude::*, px, rgb, svg};

impl Desktop {
    /// 绘制紧凑横向项目列表；固定标题与入口尺寸，滚动区只包含项目记录。
    /// 卡片打开持久化项目，设置进入独立窗口；列表仅保留项目名与真实创建/修改时间。
    pub(crate) fn manager(&self, window: &Window, cx: &mut Context<Self>) -> Div {
        div()
            .size_full()
            .flex()
            .flex_col()
            .child(
                div()
                    .flex()
                    .items_center()
                    .flex_none()
                    .h(px(76.0))
                    .px(px(24.0))
                    .child(brand(48.0))
                    .child(ui::title_drag())
                    .child(ui::window_controls(
                        window,
                        false,
                        cx.listener(|this, _, window, cx| {
                            this.close_window(window, cx);
                        }),
                    )),
            )
            .child(
                div()
                    .flex()
                    .flex_none()
                    .items_center()
                    .gap(px(16.0))
                    .px(px(28.0))
                    .h(px(58.0))
                    .border_b_1()
                    .border_color(rgb(LINE))
                    .child(
                        pill(
                            "new-project",
                            crate::i18n::t("manager", "New project", cx),
                            142.0,
                            32.0,
                        )
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.show_dialog(DialogKind::Create, window, cx)
                        })),
                    )
                    .child(
                        pill(
                            "import-project",
                            crate::i18n::t("manager", "Open project", cx),
                            142.0,
                            32.0,
                        )
                        .on_click(
                            cx.listener(|this, _, window, cx| this.import_project(window, cx)),
                        ),
                    )
                    .child(
                        pill(
                            "settings",
                            crate::i18n::t("manager", "Settings", cx),
                            142.0,
                            32.0,
                        )
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.open_settings(crate::settings::Tab::General, cx)
                        })),
                    ),
            )
            .child(
                div()
                    .id("recent-projects")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .when(self.projects.is_empty(), |s| {
                        s.child(
                            div()
                                .h(px(210.0))
                                .flex()
                                .items_center()
                                .justify_center()
                                .text_color(rgb(MUTED))
                                .child(crate::i18n::t("manager", "No projects", cx)),
                        )
                    })
                    .children(self.projects.iter().enumerate().map(|(index, entry)| {
                        let open_entry = entry.clone();
                        let menu_entry = entry.clone();
                        div()
                            .id(("project", index))
                            .debug_selector(move || format!("project-{index}"))
                            .flex()
                            .items_center()
                            .h(px(138.0))
                            .mx(px(16.0))
                            .px(px(12.0))
                            .gap(px(26.0))
                            .border_b_1()
                            .border_color(rgb(LINE))
                            .cursor_pointer()
                            .hover(|s| s.bg(rgb(0x29292c)))
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.run_project(
                                    ProjectCommand::Open {
                                        path: open_entry.path.clone(),
                                    },
                                    true,
                                    window,
                                    cx,
                                );
                            }))
                            .child(
                                div()
                                    .w(px(160.0))
                                    .h(px(94.0))
                                    .flex_none()
                                    .rounded(px(10.0))
                                    .border_1()
                                    .border_color(rgb(LINE))
                                    .bg(rgb(PANEL))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .child(
                                        svg()
                                            .path(Icon::Video.path())
                                            .size(px(30.0))
                                            .text_color(rgb(MUTED)),
                                    ),
                            )
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .flex()
                                    .flex_col()
                                    .gap(px(9.0))
                                    .child(
                                        div()
                                            .text_size(px(20.0))
                                            .overflow_hidden()
                                            .text_ellipsis()
                                            .child(entry.name.clone()),
                                    )
                                    .child(
                                        div()
                                            .flex()
                                            .gap(px(22.0))
                                            .text_size(px(12.0))
                                            .text_color(rgb(MUTED))
                                            .child(crate::i18n::f(
                                                "manager",
                                                "Created: {time}",
                                                &[("time", relative_time(entry.created_at, cx))],
                                                cx,
                                            ))
                                            .child(crate::i18n::f(
                                                "manager",
                                                "Modified: {time}",
                                                &[("time", relative_time(entry.modified_at, cx))],
                                                cx,
                                            )),
                                    ),
                            )
                            .child(
                                tool(format!("project-more-{index}"), Icon::More, true).on_click(
                                    cx.listener(move |this, event: &gpui::ClickEvent, _, cx| {
                                        cx.stop_propagation();
                                        if this.busy {
                                            return;
                                        }
                                        this.project_menu = Some(menu_entry.clone());
                                        this.project_menu_y = event.position().y.into();
                                        cx.notify();
                                    }),
                                ),
                            )
                    })),
            )
    }

    /// 在更多按钮附近显示项目管理菜单；不改变列表几何，操作使用所选持久化记录而非下标身份。
    pub(crate) fn project_actions(&self, entry: ProjectEntry, cx: &mut Context<Self>) -> Div {
        let rename = entry.clone();
        div()
            .occlude()
            .absolute()
            .top(px(self.project_menu_y.clamp(134.0, 492.0)))
            .right(px(24.0))
            .w(px(164.0))
            .p(px(6.0))
            .bg(rgb(0x303034))
            .border_1()
            .border_color(rgb(LINE))
            .rounded(px(6.0))
            .child(
                div()
                    .id("project-rename")
                    .debug_selector(|| "project-rename".into())
                    .flex()
                    .items_center()
                    .gap(px(10.0))
                    .p(px(10.0))
                    .rounded(px(4.0))
                    .cursor_pointer()
                    .hover(|s| s.bg(rgb(LINE)))
                    .child(
                        svg()
                            .path(Icon::Edit.path())
                            .size(px(16.0))
                            .text_color(rgb(TEXT)),
                    )
                    .child(crate::i18n::t("manager", "Rename", cx))
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.show_dialog(DialogKind::Rename(rename.clone()), window, cx)
                    })),
            )
            .child(
                div()
                    .id("project-delete")
                    .debug_selector(|| "project-delete".into())
                    .flex()
                    .items_center()
                    .gap(px(10.0))
                    .p(px(10.0))
                    .rounded(px(4.0))
                    .cursor_pointer()
                    .hover(|s| s.bg(rgb(LINE)))
                    .child(
                        svg()
                            .path(Icon::Trash.path())
                            .size(px(16.0))
                            .text_color(rgb(TEXT)),
                    )
                    .child(crate::i18n::t("manager", "Delete project", cx))
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.show_dialog(DialogKind::Delete(entry.clone()), window, cx)
                    })),
            )
    }
}

/// 为界面移除 Windows 扩展路径前缀，仅改变显示，不影响 Core 使用的规范文件路径。
pub(crate) fn display_path(path: &std::path::Path) -> String {
    let path = path.to_string_lossy();
    if let Some(unc) = path.strip_prefix("\\\\?\\UNC\\") {
        format!("\\\\{unc}")
    } else {
        path.strip_prefix("\\\\?\\").unwrap_or(&path).to_owned()
    }
}

/// 显示真实持久化时间距当前时刻的间隔；系统时钟倒退时不显示负数，不依赖新日期库。
fn relative_time(timestamp: u64, cx: &gpui::App) -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let seconds = now.saturating_sub(timestamp);
    if seconds < 60 {
        crate::i18n::t("manager", "Just now", cx).into()
    } else if seconds < 3600 {
        crate::i18n::f(
            "manager",
            if seconds / 60 == 1 {
                "{count} minute ago"
            } else {
                "{count} minutes ago"
            },
            &[("count", (seconds / 60).to_string())],
            cx,
        )
    } else if seconds < 86400 {
        crate::i18n::f(
            "manager",
            if seconds / 3600 == 1 {
                "{count} hour ago"
            } else {
                "{count} hours ago"
            },
            &[("count", (seconds / 3600).to_string())],
            cx,
        )
    } else {
        crate::i18n::f(
            "manager",
            if seconds / 86400 == 1 {
                "{count} day ago"
            } else {
                "{count} days ago"
            },
            &[("count", (seconds / 86400).to_string())],
            cx,
        )
    }
}
