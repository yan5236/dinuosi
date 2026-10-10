//! 帮助菜单与独立关于窗口，链接使用用户提供的正式目标。
use crate::{
    settings::{self, Tab},
    ui::{self, BACKGROUND, LINE, MUTED, TEXT},
    view::Desktop,
};
use gpui::{
    App, Bounds, Context, Div, Render, TitlebarOptions, Window, WindowBounds, WindowOptions, div,
    prelude::*, px, rgb, size,
};

pub const SOURCE_URL: &str = "https://github.com/yan5236/dinuosi";

/// 返回可拖动的独立辅助窗口参数；保持自绘标题栏，不设置模态父窗口。
pub(crate) fn auxiliary_options(title: &str, width: f32, height: f32, cx: &App) -> WindowOptions {
    WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
            None,
            size(px(width), px(height)),
            cx,
        ))),
        window_min_size: Some(size(px(width), px(height))),
        is_resizable: false,
        titlebar: Some(TitlebarOptions {
            title: Some(format!("Dinuosi - {title}").into()),
            appears_transparent: true,
            ..Default::default()
        }),
        ..Default::default()
    }
}

impl Desktop {
    /// 帮助下拉只包含用户要求的键盘快捷键与关于；菜单保持文字列表与点击遮挡。
    pub(crate) fn help_menu(&self, top: f32, cx: &mut Context<Self>) -> Div {
        div()
            .absolute()
            .top(px(top))
            .left(px(288.0))
            .w(px(200.0))
            .occlude()
            .bg(rgb(0x303034))
            .border_1()
            .border_color(rgb(LINE))
            .rounded(px(6.0))
            .p(px(5.0))
            .child(
                div()
                    .id("help-shortcuts")
                    .debug_selector(|| "help-shortcuts".into())
                    .p(px(10.0))
                    .cursor_pointer()
                    .hover(|s| s.bg(rgb(LINE)))
                    .child(crate::i18n::t("help", "Keyboard shortcuts", cx))
                    .on_click(cx.listener(|this, _, _, cx| this.open_settings(Tab::Shortcuts, cx))),
            )
            .child(
                div()
                    .id("help-about")
                    .debug_selector(|| "help-about".into())
                    .p(px(10.0))
                    .cursor_pointer()
                    .hover(|s| s.bg(rgb(LINE)))
                    .child(crate::i18n::t("help", "About", cx))
                    .on_click(cx.listener(|this, _, _, cx| this.open_about(cx))),
            )
    }

    /// 重用现有关于窗口；关闭工作区时自动关闭，不修改编辑状态和正在播放的媒体。
    pub(crate) fn open_about(&mut self, cx: &mut Context<Self>) {
        self.help_open = false;
        if let Some(handle) = self.about_window
            && handle.update(cx, |_, w, _| w.activate_window()).is_ok()
        {
            cx.notify();
            return;
        }
        let options = auxiliary_options(crate::i18n::t("help", "About", cx), 520.0, 390.0, cx);
        match cx.open_window(options, |window, cx| {
            cx.new(|cx| {
                cx.observe_global::<crate::i18n::Locale>(|_, cx| cx.notify())
                    .detach();
                let focus = cx.focus_handle();
                focus.focus(window);
                About { focus, error: None }
            })
        }) {
            Ok(handle) => {
                self.about_window = Some(handle);
                settings::close_with_owner(handle, cx);
            }
            Err(e) => self.notice = Some(format!("打开关于失败：{e}")),
        }
        cx.notify();
    }
}

pub struct About {
    focus: gpui::FocusHandle,
    error: Option<String>,
}
impl About {
    /// 定位当前程序旁的许可声明文件；存在时清除本窗口错误并委托系统默认查看器打开。
    /// cx 用于启动系统查看器及通知重绘；定位失败或文件缺失时保存本地化错误，不修改项目。
    /// 此入口不返回结果，也不接收系统查看器的启动/读取结果，不能确认外部查看器已成功打开。
    fn open_licenses(&mut self, cx: &mut Context<Self>) {
        let path = std::env::current_exe().ok().and_then(|exe| {
            exe.parent()
                .map(|root| root.join("licenses/THIRD_PARTY_NOTICES.txt"))
        });
        if let Some(path) = path.filter(|path| path.is_file()) {
            self.error = None;
            cx.open_with_system(&path);
        } else {
            self.error =
                Some(crate::i18n::t("help", "Third-party license file is unavailable", cx).into());
        }
        cx.notify();
    }
}
impl Render for About {
    /// 显示编译版本、品牌及必要许可入口；官网保持暂无，源代码链接交给浏览器。
    /// 许可文件来自配套安装目录，交给系统默认文本查看器打开，不联网或改变编辑状态。
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        window.set_window_title(&format!(
            "Dinuosi - {}",
            crate::i18n::t("help", "About", cx)
        ));
        div()
            .size_full()
            .flex()
            .flex_col()
            .bg(rgb(BACKGROUND))
            .text_color(rgb(TEXT))
            .font_family("Microsoft YaHei UI")
            .text_size(px(14.0))
            .child(div().absolute().size_0().track_focus(&self.focus))
            .on_key_down(|event, window, cx| {
                if event.keystroke.key == "escape" {
                    window.remove_window();
                }
                cx.stop_propagation();
            })
            .child(
                div()
                    .h(px(48.0))
                    .flex_none()
                    .px(px(16.0))
                    .flex()
                    .items_center()
                    .child(crate::i18n::t("help", "About", cx))
                    .child(ui::title_drag())
                    .child(ui::window_controls(window, false)),
            )
            .child(
                div()
                    .p(px(30.0))
                    .flex()
                    .flex_col()
                    .gap(px(22.0))
                    .child(ui::brand(72.0))
                    .child(crate::i18n::f(
                        "help",
                        "Version {version}",
                        &[("version", env!("CARGO_PKG_VERSION").into())],
                        cx,
                    ))
                    .child(
                        div()
                            .flex()
                            .gap(px(28.0))
                            .child(crate::i18n::t("help", "Website", cx))
                            .child(div().text_color(rgb(MUTED)).child(crate::i18n::t(
                                "help",
                                "Unavailable",
                                cx,
                            ))),
                    )
                    .child(
                        div()
                            .flex()
                            .gap(px(28.0))
                            .child(crate::i18n::t("help", "Source code", cx))
                            .child(
                                div()
                                    .id("source-link")
                                    .focusable()
                                    .cursor_pointer()
                                    .underline()
                                    .child(SOURCE_URL)
                                    .on_click(|_, _, cx| cx.open_url(SOURCE_URL)),
                            ),
                    )
                    .child(
                        div()
                            .id("third-party-licenses")
                            .focusable()
                            .cursor_pointer()
                            .underline()
                            .child(crate::i18n::t("help", "Third-party licenses", cx))
                            .on_click(cx.listener(|this, _, _, cx| this.open_licenses(cx))),
                    )
                    .children(
                        self.error
                            .as_ref()
                            .map(|message| div().text_color(rgb(0xdc5555)).child(message.clone())),
                    ),
            )
    }
}
