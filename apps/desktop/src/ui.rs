//! 原生工作区共用的灰阶、细描边按钮和自绘窗口控制。

use dinuosi_icons::Icon;
use gpui::{
    Context, Div, FontWeight, IntoElement, Render, SharedString, Stateful, Window,
    WindowControlArea, div, img, prelude::*, px, rgb, svg,
};

pub const BACKGROUND: u32 = 0x212123;
pub const PANEL: u32 = 0x262628;
pub const LINE: u32 = 0x414145;
pub const TEXT: u32 = 0xd7d7da;
pub const MUTED: u32 = 0x929297;
pub const DISABLED: u32 = 0x69696f;

/// 构造品牌组合，Logo 本身充当首字母 D；输入为逻辑像素高度。
/// 返回原生元素，不附加宣传语、版本副标题或重复的字母 D。
pub fn brand(height: f32) -> Div {
    div()
        .flex()
        .items_center()
        .flex_none()
        .child(img(crate::assets::logo_source()).size(px(height)))
        .child(
            div()
                .ml(px(-height * 0.09))
                .text_size(px(height * 0.48))
                .font_family("Segoe UI")
                .font_weight(FontWeight::SEMIBOLD)
                .child("inuosi"),
        )
}

/// 创建补充参考图中的空心胶囊按钮；宽高为逻辑像素，圆角为高度的一半。
/// 调用方负责绑定操作，默认提供悬停和按压样式。
pub fn pill(id: &'static str, label: &'static str, width: f32, height: f32) -> Stateful<Div> {
    div()
        .debug_selector(|| id.to_owned())
        .id(id)
        .flex()
        .flex_none()
        .items_center()
        .justify_center()
        .w(px(width))
        .h(px(height))
        .rounded(px(height / 2.0))
        .border_1()
        .border_color(rgb(LINE))
        .text_size(px(13.0))
        .cursor_pointer()
        .hover(|s| s.bg(rgb(0x303033)).border_color(rgb(0x64646a)))
        .active(|s| s.bg(rgb(0x39393d)))
        .child(label)
}

/// 创建紧凑图标控件；不可用项保持位置，通过悬停提示说明状态。
/// 此函数不修改业务数据，可用控件由调用方追加点击处理。
pub fn tool(id: impl Into<SharedString>, icon: Icon, enabled: bool) -> Stateful<Div> {
    tool_button(id, icon, enabled).tooltip(move |_, cx| {
        let text = if enabled {
            icon.label().to_string()
        } else {
            format!("{}（暂无可操作内容）", icon.label())
        };
        cx.new(|_| Hint(text)).into()
    })
}

/// 创建不绑定提示的图标按钮，尺寸和状态样式与通用工具一致。
/// 调用方负责按业务含义配置 Tooltip，避免将图标资源名作为操作名称。
pub fn tool_button(id: impl Into<SharedString>, icon: Icon, enabled: bool) -> Stateful<Div> {
    let id = id.into();
    div()
        .debug_selector(|| id.to_string())
        .id(id)
        .flex()
        .flex_none()
        .items_center()
        .justify_center()
        .w(px(30.0))
        .h(px(30.0))
        .rounded(px(4.0))
        .text_color(rgb(if enabled { TEXT } else { DISABLED }))
        .when(enabled, |s| {
            s.cursor_pointer().hover(move |s| {
                s.bg(rgb(if icon.name() == "close" {
                    0xb64040
                } else {
                    0x38383c
                }))
            })
        })
        .child(
            svg()
                .path(icon.path())
                .size(px(20.0))
                .text_color(rgb(if enabled { TEXT } else { DISABLED })),
        )
}

/// 绘制原生窗口操作入口；管理页只显示最小化和关闭，剪辑页额外显示最大化。
/// Windows 通过系统命中区域执行操作，避免点击回调与系统处理重复切换；其他平台使用 GPUI 回调。
/// 只关闭当前窗口，不访问磁盘；关闭剪辑窗口后的管理窗口恢复由 windows 模块处理。
pub fn window_controls(window: &Window, show_maximize: bool) -> Div {
    let maximize = if window.is_maximized() {
        Icon::Restore
    } else {
        Icon::Maximize
    };
    div()
        .flex()
        .flex_none()
        .items_center()
        .gap(px(3.0))
        .child(
            tool("window-minimize", Icon::Minimize, true)
                .w(px(42.0))
                .h(px(36.0))
                .window_control_area(WindowControlArea::Min)
                .when(!cfg!(target_os = "windows"), |button| {
                    button.on_click(|_, window, _| window.minimize_window())
                }),
        )
        .when(show_maximize, |controls| {
            controls.child(
                tool("window-maximize", maximize, true)
                    .w(px(42.0))
                    .h(px(36.0))
                    .window_control_area(WindowControlArea::Max)
                    .when(!cfg!(target_os = "windows"), |button| {
                        button.on_click(|_, window, _| window.zoom_window())
                    }),
            )
        })
        .child(
            tool("window-close", Icon::Close, true)
                .w(px(42.0))
                .h(px(36.0))
                .window_control_area(WindowControlArea::Close)
                .when(!cfg!(target_os = "windows"), |button| {
                    button.on_click(|_, window, _| window.remove_window())
                }),
        )
}

/// 返回标题栏空白拖动区，支持系统拖动、双击最大化和窗口贴靠。
/// 与菜单和按钮分离，避免操作控件时意外拖动窗口。
pub fn title_drag() -> Div {
    div()
        .debug_selector(|| "title-drag".to_owned())
        .h_full()
        .flex_1()
        .window_control_area(WindowControlArea::Drag)
}

/// 返回工具组之间的单像素分隔线；高度限制在工具栏内部。
pub fn separator() -> Div {
    div()
        .flex_none()
        .w(px(1.0))
        .h(px(23.0))
        .mx(px(5.0))
        .bg(rgb(LINE))
}

pub struct Hint(pub String);

impl Render for Hint {
    /// 按需展示操作名称或不可用原因，不占据工作区布局空间。
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .px(px(10.0))
            .py(px(6.0))
            .rounded(px(4.0))
            .bg(rgb(0x37373b))
            .border_1()
            .border_color(rgb(0x515158))
            .font_family("Microsoft YaHei UI")
            .text_size(px(12.0))
            .text_color(rgb(TEXT))
            .child(self.0.clone())
    }
}
