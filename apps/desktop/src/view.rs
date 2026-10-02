use dinuosi_command::{EditCommand, execute};
use dinuosi_core::Project;
use dinuosi_icons::Icon;
use gpui::{
    Context, Div, FocusHandle, IntoElement, Render, Window, div, img, prelude::*, px, rgb, svg,
};

/// 当前窗口会话；项目模型不存储界面状态，初始化阶段项目仅存在于内存中。
pub struct Desktop {
    focus_handle: FocusHandle,
    project: Option<Project>,
    error: Option<&'static str>,
}

impl Desktop {
    /// 创建窗口会话并将键盘焦点交给根视图，让启动页可使用 Enter 或空格新建项目。
    /// 仅初始化 GPUI 会话，不创建项目或文件；焦点句柄由当前 Context 管理。
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let focus_handle = cx.focus_handle();
        focus_handle.focus(window);
        Self {
            focus_handle,
            project: None,
            error: None,
        }
    }
    /// 通过共享命令入口新建内存项目并切换到编辑工作区；校验失败时展示错误。
    /// 不保存项目或修改素材，成功和失败均通知 GPUI 刷新界面。
    fn create_project(&mut self, cx: &mut Context<Self>) {
        match execute(EditCommand::CreateProject {
            name: "未命名项目".into(),
        }) {
            Ok(project) => {
                self.project = Some(project);
                self.error = None;
            }
            Err(error) => self.error = Some(error),
        }
        cx.notify();
    }

    /// 构造启动页，展示品牌、可用的新建项目入口及尚未接入的项目管理功能。
    fn welcome(&self, cx: &mut Context<Self>) -> Div {
        div()
            .flex()
            .flex_col()
            .flex_1()
            .items_center()
            .justify_center()
            .gap_6()
            .child(img(crate::assets::logo_source()).size(px(96.0)))
            .child(div().text_3xl().child("Dinuosi · 迪诺斯"))
            .child(
                div()
                    .text_color(rgb(0x9fb2b8))
                    .child("本地优先，让创作保持自由。"),
            )
            .child(
                div()
                    .flex()
                    .gap_4()
                    .child(
                        div()
                            .id("new-project")
                            .flex()
                            .items_center()
                            .gap_2()
                            .px_5()
                            .py_3()
                            .rounded_md()
                            .bg(rgb(0x19c9cf))
                            .text_color(rgb(0x102c32))
                            .cursor_pointer()
                            .hover(|style| style.bg(rgb(0x52d7dc)))
                            .on_click(cx.listener(|this, _, _, cx| this.create_project(cx)))
                            .child(svg().path(Icon::Plus.path()).size(px(24.0)))
                            .child("新建项目 · Enter"),
                    )
                    .child(inactive_control(Icon::Folder, "导入项目 · 即将开放"))
                    .child(inactive_control(Icon::Settings, "设置 · 即将开放")),
            )
            .child(div().mt_8().text_color(rgb(0x9fb2b8)).child("最近项目"))
            .child(
                div()
                    .text_sm()
                    .text_color(rgb(0x9fb2b8))
                    .child("还没有最近项目，从一个新项目开始。"),
            )
    }

    /// 按 PRD 的区域划分渲染空编辑工作区；当前仅新建项目可用，媒体与剪辑功能待接入。
    fn editor(&self, project: &Project) -> Div {
        div()
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .p_4()
                    .border_b_1()
                    .border_color(rgb(0x28434b))
                    .child(format!("{} · 尚未保存", project.name()))
                    .child(inactive_control(Icon::ExportVideo, "导出 · 即将开放")),
            )
            .child(
                div()
                    .flex()
                    .flex_1()
                    .min_h_0()
                    .child(panel("素材库", Icon::ImportMedia, "尚未导入素材").w(px(240.0)))
                    .child(panel("实时预览", Icon::Video, "添加素材后在此预览").flex_1())
                    .child(panel("检查器", Icon::Keyframe, "选择片段后查看参数").w(px(240.0))),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_3()
                    .p_3()
                    .border_y_1()
                    .border_color(rgb(0x28434b))
                    .children(
                        [
                            Icon::Select,
                            Icon::Razor,
                            Icon::Undo,
                            Icon::Redo,
                            Icon::Snap,
                            Icon::Marker,
                            Icon::Lock,
                            Icon::MirrorHorizontal,
                            Icon::MirrorVertical,
                            Icon::ReversePlayback,
                            Icon::FreezeFrame,
                            Icon::Speed,
                            Icon::LinkClips,
                            Icon::UnlinkClips,
                            Icon::ReplaceMedia,
                            Icon::RippleDelete,
                            Icon::Trash,
                        ]
                        .into_iter()
                        .map(|icon| {
                            svg()
                                .path(icon.path())
                                .size(px(24.0))
                                .text_color(rgb(0x6c838c))
                        }),
                    )
                    .child(div().flex_1())
                    .child(
                        svg()
                            .path(Icon::ZoomOut.path())
                            .size(px(24.0))
                            .text_color(rgb(0x6c838c)),
                    )
                    .child(
                        svg()
                            .path(Icon::ZoomIn.path())
                            .size(px(24.0))
                            .text_color(rgb(0x6c838c)),
                    ),
            )
            .child(
                div()
                    .h(px(180.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .bg(rgb(0x102c32))
                    .text_color(rgb(0x9fb2b8))
                    .child("时间线为空 · 剪辑功能尚未接入"),
            )
            .child(
                div()
                    .p_3()
                    .text_sm()
                    .text_color(rgb(0x9fb2b8))
                    .child("当前任务 · 无正在处理的任务"),
            )
    }
}

impl Render for Desktop {
    /// 根据当前会话显示启动页或编辑工作区；统一应用品牌色和系统中文字体。
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .track_focus(&self.focus_handle)
            .on_key_down(cx.listener(|this, event: &gpui::KeyDownEvent, _, cx| {
                if this.project.is_none()
                    && matches!(event.keystroke.key.as_str(), "enter" | "space")
                {
                    this.create_project(cx);
                }
            }))
            .size_full()
            .flex()
            .flex_col()
            .bg(rgb(0x0b252c))
            .text_color(rgb(0xecf4f6))
            .font_family("Microsoft YaHei")
            .text_base()
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_3()
                    .px_5()
                    .py_3()
                    .border_b_1()
                    .border_color(rgb(0x28434b))
                    .child(img(crate::assets::logo_source()).size(px(36.0)))
                    .child("Dinuosi")
                    .child(div().flex_1())
                    .child(
                        div()
                            .text_sm()
                            .text_color(rgb(0x9fb2b8))
                            .child("Edit · 项目初始化版本"),
                    ),
            )
            .child(match &self.project {
                Some(project) => self.editor(project),
                None => self.welcome(cx),
            })
            .children(
                self.error
                    .map(|error| div().p_3().text_color(rgb(0xffc0b8)).child(error)),
            )
    }
}

/// 构造明确标注尚未开放的入口，保留中文文字与图标，且不注册点击行为。
fn inactive_control(icon: Icon, label: &'static str) -> Div {
    div()
        .flex()
        .items_center()
        .gap_2()
        .px_4()
        .py_3()
        .rounded_md()
        .border_1()
        .border_color(rgb(0x28434b))
        .text_color(rgb(0x9fb2b8))
        .child(svg().path(icon.path()).size(px(24.0)))
        .child(label)
}

/// 构造编辑区域的标题和空状态；返回可由调用方设置宽度或弹性比例的面板。
fn panel(title: &'static str, icon: Icon, empty: &'static str) -> Div {
    div()
        .flex()
        .flex_col()
        .border_r_1()
        .border_color(rgb(0x28434b))
        .p_4()
        .gap_4()
        .child(title)
        .child(
            div()
                .flex()
                .flex_col()
                .flex_1()
                .items_center()
                .justify_center()
                .gap_3()
                .text_color(rgb(0x9fb2b8))
                .text_sm()
                .child(svg().path(icon.path()).size(px(32.0)))
                .child(empty),
        )
}
