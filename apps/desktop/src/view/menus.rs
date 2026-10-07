//! 工作区标题栏与文件菜单，保持原有文字入口和灰阶样式。
use super::*;
impl Desktop {
    /// 构造剪辑标题栏；品牌导航创建独立管理窗口，系统命中区保留原生拖动与窗口控制。
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
                    .tooltip(|_, cx| cx.new(|_| ui::Hint("项目管理".into())).into())
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
                    .child("文件")
                    .on_click(cx.listener(|this, _, _, cx| {
                        if !this.busy {
                            this.menu_open = !this.menu_open;
                            this.help_open = false;
                            this.notice = None;
                            cx.notify();
                        }
                    })),
            )
            .child(
                div()
                    .id("edit-menu")
                    .px(px(14.0))
                    .py(px(8.0))
                    .cursor_pointer()
                    .hover(|s| s.bg(rgb(crate::ui::PANEL)))
                    .child("编辑")
                    .on_click(cx.listener(|this, _, _, cx| this.inform("暂无可编辑的片段", cx))),
            )
            .child(
                div()
                    .id("help-menu")
                    .px(px(14.0))
                    .py(px(8.0))
                    .cursor_pointer()
                    .hover(|s| s.bg(rgb(crate::ui::PANEL)))
                    .child("帮助")
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.help_open = !this.help_open;
                        this.menu_open = false;
                        cx.notify();
                    })),
            )
            .child(ui::title_drag())
            .child(
                pill("export", "导出", 110.0, 26.0)
                    .mr(px(18.0))
                    .on_click(cx.listener(|this, _, _, cx| this.inform("导出尚未开放", cx))),
            )
            .child(ui::window_controls(window, true))
    }
    /// 构造文件菜单，沿用原有样式；项目入口调用持久化流程，设置入口打开或聚焦通用设置窗口。
    /// 浮层遮挡底层点击；关闭窗口期间检查 busy，关闭成功后由窗口观察器重新打开管理窗口。
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
                    .child("新建项目")
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
                    .child("导入素材")
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
                    .child("项目管理")
                    .on_click(cx.listener(|this, _, window, cx| this.show_manager(window, cx))),
            )
            .child(
                div()
                    .id("menu-settings")
                    .debug_selector(|| "menu-settings".into())
                    .p(px(10.0))
                    .cursor_pointer()
                    .hover(|s| s.bg(rgb(LINE)))
                    .child("设置")
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.open_settings(crate::settings::Tab::General, cx)
                    })),
            )
            .child(
                div()
                    .id("menu-close")
                    .p(px(10.0))
                    .cursor_pointer()
                    .hover(|s| s.bg(rgb(LINE)))
                    .child("关闭窗口")
                    .on_click(cx.listener(|this, _, window, _| {
                        if !this.busy {
                            window.remove_window();
                        }
                    })),
            )
    }
}
