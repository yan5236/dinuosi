//! 工作区标题栏与文件菜单，保持原有文字入口和灰阶样式。
use super::*;
impl Desktop {
    /// 构造剪辑标题栏；品牌导航创建独立管理窗口，导出打开后台编码表单，系统命中区保留拖动与窗口控制。
    /// 编辑入口打开含历史及剪贴板操作的菜单；文件、编辑和帮助浮层互斥。
    pub(crate) fn editor_titlebar(
        &self,
        height: f32,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Div {
        div()
            .h(px(height))
            .flex_none()
            .flex()
            .items_center()
            .px(px(12.0))
            .border_b_1()
            .border_color(rgb(LINE))
            .child(
                div()
                    .id("brand-home")
                    .debug_selector(|| "brand-home".into())
                    .cursor_pointer()
                    .mr(px(22.0))
                    .child(brand(42.0))
                    .tooltip(|_, cx| {
                        cx.new(|cx| {
                            ui::Hint(crate::i18n::t("editor", "Project manager", cx).into())
                        })
                        .into()
                    })
                    .on_click(cx.listener(|this, _, window, cx| this.show_manager(window, cx))),
            )
            .child(
                div()
                    .id("file-menu")
                    .debug_selector(|| "file-menu".into())
                    .px(px(14.0))
                    .py(px(8.0))
                    .cursor_pointer()
                    .hover(|s| s.bg(rgb(crate::ui::PANEL)))
                    .child(crate::i18n::t("editor", "File", cx))
                    .on_click(cx.listener(|this, _, _, cx| {
                        if !this.busy {
                            this.menu_open = !this.menu_open;
                            this.timeline.edit_menu = None;
                            this.help_open = false;
                            this.notice = None;
                            cx.notify();
                        }
                    })),
            )
            .child(
                div()
                    .id("edit-menu")
                    .debug_selector(|| "edit-menu".into())
                    .px(px(14.0))
                    .py(px(8.0))
                    .cursor_pointer()
                    .hover(|s| s.bg(rgb(crate::ui::PANEL)))
                    .child(crate::i18n::t("editor", "Edit", cx))
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.open_edit_menu(None, None, window, cx)
                    })),
            )
            .child(
                div()
                    .id("help-menu")
                    .px(px(14.0))
                    .py(px(8.0))
                    .cursor_pointer()
                    .hover(|s| s.bg(rgb(crate::ui::PANEL)))
                    .child(crate::i18n::t("editor", "Help", cx))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.help_open = !this.help_open;
                        this.timeline.edit_menu = None;
                        this.menu_open = false;
                        cx.notify();
                    })),
            )
            .child(ui::title_drag())
            .child(
                pill(
                    "export",
                    crate::i18n::t("editor", "Export", cx),
                    110.0,
                    26.0,
                )
                .mr(px(18.0))
                .on_click(cx.listener(|this, _, window, cx| this.open_export(window, cx))),
            )
            .child(ui::window_controls(
                window,
                true,
                cx.listener(|this, _, window, cx| {
                    this.close_window(window, cx);
                }),
            ))
    }
    /// 构造文件菜单，沿用原有样式；项目入口调用持久化流程，设置入口打开或聚焦通用设置窗口。
    /// 浮层遮挡底层点击；关闭前提交草稿，成功后由窗口观察器重新打开管理窗口。
    pub(super) fn file_menu(&self, top: f32, cx: &mut Context<Self>) -> Div {
        div()
            .occlude()
            .absolute()
            .top(px(top))
            .left(px(160.0))
            .w(px(200.0))
            .bg(rgb(0x303034))
            .border_1()
            .border_color(rgb(LINE))
            .rounded(px(6.0))
            .p(px(5.0))
            .child(
                div()
                    .id("menu-new")
                    .p(px(10.0))
                    .cursor_pointer()
                    .hover(|s| s.bg(rgb(LINE)))
                    .child(crate::i18n::t("editor", "New project", cx))
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.show_dialog(DialogKind::Create, window, cx)
                    })),
            )
            .child(
                div()
                    .id("menu-import-media")
                    .debug_selector(|| "menu-import-media".into())
                    .p(px(10.0))
                    .cursor_pointer()
                    .hover(|s| s.bg(rgb(LINE)))
                    .child(crate::i18n::t("editor", "Import media", cx))
                    .on_click(cx.listener(|this, event, window, cx| {
                        this.menu_open = false;
                        this.choose_media(event, window, cx);
                        cx.notify();
                    })),
            )
            .child(
                div()
                    .id("menu-home")
                    .p(px(10.0))
                    .cursor_pointer()
                    .hover(|s| s.bg(rgb(LINE)))
                    .child(crate::i18n::t("editor", "Project manager", cx))
                    .on_click(cx.listener(|this, _, window, cx| this.show_manager(window, cx))),
            )
            .child(
                div()
                    .id("menu-settings")
                    .debug_selector(|| "menu-settings".into())
                    .p(px(10.0))
                    .cursor_pointer()
                    .hover(|s| s.bg(rgb(LINE)))
                    .child(crate::i18n::t("editor", "Settings", cx))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.open_settings(crate::settings::Tab::General, cx)
                    })),
            )
            .child(
                div()
                    .id("menu-close")
                    .debug_selector(|| "menu-close".into())
                    .p(px(10.0))
                    .cursor_pointer()
                    .hover(|s| s.bg(rgb(LINE)))
                    .child(crate::i18n::t("editor", "Close window", cx))
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.close_window(window, cx);
                    })),
            )
    }
}
