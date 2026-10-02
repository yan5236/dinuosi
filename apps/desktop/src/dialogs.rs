//! 项目创建、重命名与删除确认；在 GPUI 窗口内呈现模态表单与原生目录选择。

use crate::{
    input::TextInput,
    manager::display_path,
    ui::{LINE, MUTED, PANEL, TEXT, pill, tool},
    view::Desktop,
};
use dinuosi_command::ProjectCommand;
use dinuosi_core::{ProjectEntry, ProjectStore};
use dinuosi_icons::Icon;
use gpui::{
    Context, Div, Entity, FocusHandle, PathPromptOptions, Window, div, prelude::*, px, rgb, rgba,
    svg,
};
use std::path::PathBuf;

#[derive(Clone)]
pub(crate) enum DialogKind {
    Create,
    Rename(ProjectEntry),
    Delete(ProjectEntry),
}

pub(crate) struct Dialog {
    kind: DialogKind,
    name: Option<Entity<TextInput>>,
    pub(crate) directory: Option<PathBuf>,
    delete_folder: bool,
    can_delete_folder: bool,
    pub(crate) error: Option<String>,
    choose_focus: FocusHandle,
    cancel_focus: FocusHandle,
    submit_focus: FocusHandle,
}

impl Dialog {
    /// 初始化所选操作的表单；删除默认不勾选目录，新建目录必须由用户显式选择。
    pub(crate) fn new(kind: DialogKind, store: &ProjectStore, cx: &mut Context<Desktop>) -> Self {
        let name = match &kind {
            DialogKind::Create => Some("未命名项目".into()),
            DialogKind::Rename(entry) => Some(entry.name.clone()),
            DialogKind::Delete(_) => None,
        }
        .map(|text| cx.new(|cx| TextInput::new(text, cx)));
        let can_delete_folder = match &kind {
            DialogKind::Delete(entry) => store.can_delete_folder(entry),
            _ => false,
        };
        Self {
            kind,
            name,
            directory: None,
            delete_folder: false,
            can_delete_folder,
            error: None,
            choose_focus: cx.focus_handle(),
            cancel_focus: cx.focus_handle(),
            submit_focus: cx.focus_handle(),
        }
    }

    /// 返回弹窗内按 Tab 顺序排列的可操作焦点；危险删除默认先聚焦取消。
    fn focus_order(&self, cx: &Context<Desktop>) -> Vec<FocusHandle> {
        let mut handles = Vec::new();
        if let Some(name) = &self.name {
            handles.push(name.read(cx).focus.clone());
        }
        if matches!(self.kind, DialogKind::Create) || self.can_delete_folder {
            handles.push(self.choose_focus.clone());
        }
        handles.push(self.cancel_focus.clone());
        handles.push(self.submit_focus.clone());
        handles
    }

    /// 聚焦名称输入或删除取消按钮；不让 Enter 默认触发危险的文件删除。
    pub(crate) fn focus_first(&self, window: &mut Window, cx: &Context<Desktop>) {
        if let Some(name) = &self.name {
            name.read(cx).focus.focus(window);
        } else {
            self.cancel_focus.focus(window);
        }
    }
}

impl Desktop {
    /// 弹出操作系统目录选择器；取消保持当前表单，失败在弹窗中反馈。
    /// 原生选择与后台持久化共享 busy 防重复；目录只有成功选择后才写入表单。
    fn choose_directory(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        self.busy = true;
        let receiver = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some("选择项目存放目录".into()),
        });
        cx.spawn_in(window, async move |this, cx| {
            let result = receiver.await;
            let _ = this.update_in(cx, |this, _, cx| {
                this.busy = false;
                if let Some(dialog) = &mut this.dialog {
                    match result {
                        Ok(Ok(Some(paths))) => {
                            dialog.directory = paths.into_iter().next();
                            dialog.error = None;
                        }
                        Ok(Ok(None)) => {}
                        Ok(Err(error)) => dialog.error = Some(format!("选择目录失败：{error}")),
                        Err(error) => dialog.error = Some(format!("目录选择器未完成：{error}")),
                    }
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    /// 校验表单输入并调度共享持久化命令；目录未选择时保留表单，不创建任何文件。
    pub(crate) fn submit_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        let Some(dialog) = &mut self.dialog else {
            return;
        };
        let name = dialog
            .name
            .as_ref()
            .map(|input| input.read(cx).text.clone())
            .unwrap_or_default();
        let (command, enter) = match &dialog.kind {
            DialogKind::Create => {
                let Some(parent) = dialog.directory.clone() else {
                    dialog.error = Some("请先选择项目目录".into());
                    cx.notify();
                    return;
                };
                (ProjectCommand::Create { name, parent }, true)
            }
            DialogKind::Rename(entry) => (
                ProjectCommand::Rename {
                    entry: entry.clone(),
                    name,
                },
                false,
            ),
            DialogKind::Delete(entry) => (
                ProjectCommand::Delete {
                    entry: entry.clone(),
                    delete_folder: dialog.delete_folder,
                },
                false,
            ),
        };
        self.run_project(command, enter, window, cx);
    }

    /// 在弹窗内循环 Tab 焦点，Escape 取消，Enter/空格激活聚焦控件；组合输入提交前不触发创建。
    pub(crate) fn dialog_key(
        &mut self,
        event: &gpui::KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.busy {
            cx.stop_propagation();
            return;
        }
        let Some(dialog) = &self.dialog else {
            return;
        };
        if dialog
            .name
            .as_ref()
            .is_some_and(|name| name.read(cx).is_composing())
        {
            return;
        }
        let key = event.keystroke.key.as_str();
        if key == "tab" {
            let handles = dialog.focus_order(cx);
            let index = handles
                .iter()
                .position(|h| h.is_focused(window))
                .unwrap_or(0);
            let next = if event.keystroke.modifiers.shift {
                (index + handles.len() - 1) % handles.len()
            } else {
                (index + 1) % handles.len()
            };
            handles[next].focus(window);
        } else if key == "escape" {
            self.cancel_dialog(window, cx);
        } else if key == "enter"
            || (key == "space"
                && dialog
                    .name
                    .as_ref()
                    .is_none_or(|name| !name.read(cx).focus.is_focused(window)))
        {
            if dialog.cancel_focus.is_focused(window) {
                self.cancel_dialog(window, cx);
            } else if dialog.choose_focus.is_focused(window) {
                if matches!(dialog.kind, DialogKind::Create) {
                    self.choose_directory(window, cx);
                } else if let Some(dialog) = &mut self.dialog {
                    dialog.delete_folder = !dialog.delete_folder;
                    cx.notify();
                }
            } else {
                self.submit_dialog(window, cx);
            }
        } else {
            return;
        }
        cx.stop_propagation();
    }

    /// 绘制灰阶模态表单和清晰的错误反馈；遮罩拦截底层点击，关闭不会提交表单。
    /// 选择目录、新建、重命名及删除均使用真实存储结果；文件删除必须显式勾选。
    pub(crate) fn dialog_view(&self, cx: &mut Context<Self>) -> gpui::Stateful<Div> {
        let dialog = self.dialog.as_ref().expect("仅在弹窗存在时渲染");
        let (title, submit) = match dialog.kind {
            DialogKind::Create => ("新建项目", "创建"),
            DialogKind::Rename(_) => ("重命名项目", "保存"),
            DialogKind::Delete(_) => ("删除项目", "删除"),
        };
        div()
            .occlude()
            .absolute()
            .inset_0()
            .flex()
            .items_center()
            .justify_center()
            .bg(rgba(0x00000080))
            .id("dialog-overlay")
            .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .child(
                div()
                    .w(px(480.0))
                    .p(px(26.0))
                    .rounded(px(12.0))
                    .bg(rgb(0x2b2b2e))
                    .border_1()
                    .border_color(rgb(LINE))
                    .flex()
                    .flex_col()
                    .gap(px(20.0))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_between()
                            .child(div().text_size(px(20.0)).child(title))
                            .child(tool("dialog-close", Icon::Close, !self.busy).on_click(
                                cx.listener(|this, _, window, cx| this.cancel_dialog(window, cx)),
                            )),
                    )
                    .when_some(dialog.name.clone(), |s, name| {
                        s.child(
                            div()
                                .flex()
                                .flex_col()
                                .gap(px(8.0))
                                .child(
                                    div()
                                        .text_size(px(12.0))
                                        .text_color(rgb(MUTED))
                                        .child("项目名称"),
                                )
                                .child(name),
                        )
                    })
                    .when(matches!(dialog.kind, DialogKind::Create), |s| {
                        s.child(
                            div()
                                .flex()
                                .flex_col()
                                .gap(px(8.0))
                                .child(
                                    div()
                                        .text_size(px(12.0))
                                        .text_color(rgb(MUTED))
                                        .child("项目目录"),
                                )
                                .child(
                                    div()
                                        .id("choose-directory")
                                        .debug_selector(|| "choose-directory".into())
                                        .track_focus(&dialog.choose_focus)
                                        .flex()
                                        .items_center()
                                        .gap(px(10.0))
                                        .h(px(42.0))
                                        .px(px(12.0))
                                        .rounded(px(6.0))
                                        .bg(rgb(PANEL))
                                        .border_1()
                                        .border_color(rgb(LINE))
                                        .cursor_pointer()
                                        .hover(|s| s.border_color(rgb(MUTED)))
                                        .focus(|s| s.border_color(rgb(TEXT)))
                                        .child(
                                            svg()
                                                .path(Icon::Folder.path())
                                                .size(px(18.0))
                                                .text_color(rgb(MUTED)),
                                        )
                                        .child(
                                            div()
                                                .flex_1()
                                                .min_w_0()
                                                .overflow_hidden()
                                                .text_ellipsis()
                                                .text_size(px(13.0))
                                                .child(
                                                    dialog
                                                        .directory
                                                        .as_ref()
                                                        .map(|p| display_path(p))
                                                        .unwrap_or_else(|| "选择存放目录…".into()),
                                                ),
                                        )
                                        .on_click(cx.listener(|this, _, window, cx| {
                                            this.choose_directory(window, cx)
                                        })),
                                ),
                        )
                    })
                    .when_some(
                        match &dialog.kind {
                            DialogKind::Delete(entry) => Some(entry),
                            _ => None,
                        },
                        |s, entry| {
                            s.child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .gap(px(12.0))
                                    .child(format!("确定删除“{}”？", entry.name))
                                    .child(
                                        div()
                                            .id("delete-folder")
                                            .debug_selector(|| "delete-folder".into())
                                            .track_focus(&dialog.choose_focus)
                                            .flex()
                                            .items_center()
                                            .gap(px(10.0))
                                            .p(px(8.0))
                                            .rounded(px(4.0))
                                            .when(dialog.can_delete_folder, |s| {
                                                s.cursor_pointer()
                                                    .hover(|s| s.bg(rgb(PANEL)))
                                                    .focus(|s| s.bg(rgb(LINE)))
                                            })
                                            .child(
                                                div()
                                                    .size(px(18.0))
                                                    .flex_none()
                                                    .rounded(px(3.0))
                                                    .border_1()
                                                    .border_color(rgb(MUTED))
                                                    .when(dialog.delete_folder, |s| {
                                                        s.child(
                                                            svg()
                                                                .path(Icon::Check.path())
                                                                .size(px(16.0))
                                                                .text_color(rgb(TEXT)),
                                                        )
                                                    }),
                                            )
                                            .child(
                                                div()
                                                    .text_size(px(13.0))
                                                    .child("同时删除项目源文件夹（不可恢复）"),
                                            )
                                            .on_click(cx.listener(|this, _, _, cx| {
                                                if !this.busy
                                                    && let Some(dialog) = &mut this.dialog
                                                    && dialog.can_delete_folder
                                                {
                                                    dialog.delete_folder = !dialog.delete_folder;
                                                    cx.notify();
                                                }
                                            })),
                                    ),
                            )
                        },
                    )
                    .children(dialog.error.clone().map(|error| {
                        div()
                            .text_size(px(12.0))
                            .text_color(rgb(0xe0a7a7))
                            .child(error)
                    }))
                    .child(
                        div()
                            .flex()
                            .justify_end()
                            .gap(px(12.0))
                            .mt(px(4.0))
                            .child(
                                pill("dialog-cancel", "取消", 90.0, 34.0)
                                    .track_focus(&dialog.cancel_focus)
                                    .focus(|s| s.border_color(rgb(TEXT)))
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.cancel_dialog(window, cx)
                                    })),
                            )
                            .child(
                                pill(
                                    "dialog-submit",
                                    if self.busy { "处理中…" } else { submit },
                                    90.0,
                                    34.0,
                                )
                                .track_focus(&dialog.submit_focus)
                                .focus(|s| s.border_color(rgb(TEXT)))
                                .on_click(
                                    cx.listener(|this, _, window, cx| {
                                        this.submit_dialog(window, cx)
                                    }),
                                ),
                            ),
                    ),
            )
    }
}
