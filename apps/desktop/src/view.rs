mod events;
use crate::{
    dialogs::{Dialog, DialogKind},
    ui::{self, BACKGROUND, LINE, TEXT, brand, pill, tool},
};
use dinuosi_command::{ProjectCommand, ProjectOutcome, execute_project};
use dinuosi_core::{ProjectEntry, ProjectStore};
use dinuosi_icons::Icon;
use gpui::{
    Context, Div, FocusHandle, IntoElement, PathPromptOptions, Render, Window, div, prelude::*, px,
    rgb, svg,
};
/// 一个原生窗口的会话；项目数据由 Core 保存，窗口缩放与音量仅属于编辑会话。
pub struct Desktop {
    pub(crate) focus_handle: FocusHandle,
    pub(crate) store: ProjectStore,
    pub(crate) projects: Vec<ProjectEntry>,
    pub(crate) current: Option<ProjectEntry>,
    pub(crate) project_menu: Option<ProjectEntry>,
    pub(crate) project_menu_y: f32,
    menu_open: bool,
    pub(crate) dialog: Option<Dialog>,
    pub(crate) notice: Option<String>,
    pub(crate) busy: bool,
    pub(crate) zoom: f32,
    pub(crate) volume: f32,
    pub(crate) session: crate::preview::EditorSession,
    pub(crate) timeline: crate::timeline_state::TimelineSession,
}
impl Desktop {
    /// 初始化窗口角色和快捷键焦点；后台加载最近列表，损坏索引以错误提示呈现。
    /// 原生关闭在磁盘操作期间被阻止，避免窗口销毁后遗失错误反馈或重复创建。
    pub fn new(
        store: ProjectStore,
        current: Option<ProjectEntry>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let focus_handle = cx.focus_handle();
        focus_handle.focus(window);
        let weak = cx.weak_entity();
        window.on_window_should_close(cx, move |_, cx| {
            weak.read_with(cx, |view, _| !view.busy).unwrap_or(true)
        });
        let load_store = store.clone();
        let load_entry = current.clone();
        cx.spawn_in(window, async move |this, cx| {
            let result = cx
                .background_spawn(async move {
                    let projects = load_store.list()?;
                    let mut media = Vec::new();
                    let mut timeline = Default::default();
                    if let Some(entry) = load_entry {
                        timeline = load_store.load_timeline(&entry)?;
                        media = load_store
                            .load_media(&entry)?
                            .into_iter()
                            .map(crate::media::Media::from_source)
                            .collect();
                    }
                    Ok::<_, String>((projects, media, timeline))
                })
                .await;
            let _ = this.update_in(cx, |this, _, cx| {
                this.busy = false;
                match result {
                    Ok((entries, media, timeline)) => {
                        this.projects = entries;
                        this.session.media = media;
                        this.timeline.model = timeline;
                        this.timeline.preview = !this.timeline.model.clips.is_empty();
                        this.refresh_timeline(cx);
                    }
                    Err(error) => this.notice = Some(error),
                }
                cx.notify();
            });
        })
        .detach();
        let mut desktop = Self {
            focus_handle,
            store,
            current,
            projects: Vec::new(),
            project_menu: None,
            project_menu_y: 0.0,
            menu_open: false,
            dialog: None,
            notice: None,
            busy: true,
            zoom: 1.0,
            volume: 1.0,
            session: crate::preview::EditorSession::new(cx),
            timeline: Default::default(),
        };
        desktop.watch_media(window, cx);
        desktop
    }
    /// 打开新建或管理弹窗并聚焦名称/确认控件；已有后台操作期间忽略重复请求。
    pub(crate) fn show_dialog(
        &mut self,
        kind: DialogKind,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.busy {
            return;
        }
        let dialog = Dialog::new(kind, &self.store, cx);
        dialog.focus_first(window, cx);
        self.dialog = Some(dialog);
        self.project_menu = None;
        self.session.media_menu = None;
        self.menu_open = false;
        self.notice = None;
        cx.notify();
    }
    /// 关闭弹窗并恢复工作区快捷键焦点；持久化期间保持弹窗以展示最终结果。
    pub(crate) fn cancel_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        self.dialog = None;
        self.focus_handle.focus(window);
        cx.notify();
    }
    /// 执行共享项目命令于后台，成功后刷新真实列表；创建/打开通过独立窗口切换进入剪辑主体。
    /// 失败保留当前窗口和表单输入，错误显示在弹窗或工作区；busy 防止重复磁盘操作。
    pub(crate) fn run_project(
        &mut self,
        command: ProjectCommand,
        enter: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.busy {
            return;
        }
        self.busy = true;
        self.notice = None;
        if let Some(dialog) = &mut self.dialog {
            dialog.error = None;
        }
        let store = self.store.clone();
        cx.spawn_in(window, async move |this, cx| {
            let result = cx
                .background_spawn(async move {
                    let outcome = execute_project(&store, command)?;
                    let projects = store.list()?;
                    Ok::<_, String>((outcome, projects))
                })
                .await;
            let _ = this.update_in(cx, |this, window, cx| {
                this.busy = false;
                match result {
                    Ok((outcome, projects)) => {
                        this.projects = projects;
                        this.dialog = None;
                        this.project_menu = None;
                        this.focus_handle.focus(window);
                        if enter
                            && let ProjectOutcome::Entry(entry) = outcome
                            && let Err(error) =
                                crate::windows::replace(this.store.clone(), Some(entry), window, cx)
                        {
                            this.notice = Some(error);
                        }
                    }
                    Err(error) => {
                        if let Some(dialog) = &mut this.dialog {
                            dialog.error = Some(error);
                        } else {
                            this.notice = Some(error);
                        }
                    }
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }
    /// 通过原生文件选择器导入持久化项目；取消不修改列表，选择失败显示可读错误。
    pub(crate) fn import_project(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        self.busy = true;
        let receiver = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("打开 Dinuosi 项目".into()),
        });
        cx.spawn_in(window, async move |this, cx| {
            let result = receiver.await;
            let _ = this.update_in(cx, |this, window, cx| {
                this.busy = false;
                match result {
                    Ok(Ok(Some(paths))) => {
                        if let Some(path) = paths.into_iter().next() {
                            this.run_project(ProjectCommand::Open { path }, true, window, cx);
                        }
                    }
                    Ok(Ok(None)) => {}
                    Ok(Err(error)) => this.notice = Some(format!("选择项目失败：{error}")),
                    Err(error) => this.notice = Some(format!("文件选择器未完成：{error}")),
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }
    /// 从剪辑窗口返回独立管理窗口；成功创建管理窗口后销毁剪辑窗口，失败保留剪辑会话。
    fn show_manager(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy || self.current.is_none() {
            return;
        }
        if let Err(error) = crate::windows::replace(self.store.clone(), None, window, cx) {
            self.notice = Some(error);
            cx.notify();
        }
    }
    /// 按用户操作显示未接入功能说明；不修改持久化项目或自动生成示例数据。
    pub(crate) fn inform(&mut self, message: &str, cx: &mut Context<Self>) {
        self.notice = Some(message.to_owned());
        self.menu_open = false;
        cx.notify();
    }
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
                    .px(px(14.0))
                    .py(px(8.0))
                    .cursor_pointer()
                    .hover(|s| s.bg(rgb(crate::ui::PANEL)))
                    .child("文件")
                    .on_click(cx.listener(|this, _, _, cx| {
                        if !this.busy {
                            this.menu_open = !this.menu_open;
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
                        this.inform("Ctrl+N 新建项目 · Ctrl+O 打开项目 · Ctrl+H 项目管理", cx)
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
    /// 构造文件菜单，沿用原有样式；素材导入位于此处，项目入口调用持久化流程。
    /// 浮层遮挡底层点击；关闭窗口期间检查 busy，关闭成功后由窗口观察器重新打开管理窗口。
    fn file_menu(&self, top: f32, cx: &mut Context<Self>) -> Div {
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
impl Render for Desktop {
    /// 渲染当前窗口角色与浮层；工作区快捷键在根节点接收冒泡，焦点置于零尺寸节点以保留系统拖动。
    /// 弹窗存在时阻止工作区快捷键，并委派 Tab、Enter、Escape 到弹窗交互，避免编辑名称时意外新建。
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        for image in self.session.retired_frames.drain(..) {
            if !self
                .session
                .media
                .iter()
                .any(|m| m.thumbnail.as_ref().is_some_and(|t| t.id == image.id))
            {
                let _ = window.drop_image(image);
            }
        }
        let dimensions = window.viewport_size();
        let layout = crate::editor::Layout::new(dimensions.width.into(), dimensions.height.into());
        div()
            .relative()
            .size_full()
            .overflow_hidden()
            .bg(rgb(BACKGROUND))
            .text_color(rgb(TEXT))
            .text_size(px(14.0))
            .font_family("Microsoft YaHei UI")
            .on_mouse_down(
                gpui::MouseButton::Left,
                cx.listener(|this, _, window, cx| {
                    if this.timeline.input.is_some() || this.timeline.rename.is_some() {
                        this.commit_inspector_input(cx);
                        this.focus_handle.focus(window);
                    }
                }),
            )
            .on_mouse_move(
                cx.listener(|this, event: &gpui::MouseMoveEvent, window, cx| {
                    if event.pressed_button == Some(gpui::MouseButton::Left) {
                        this.move_pointer(event.position.x, cx);
                        this.move_timeline_pointer(event.position, cx);
                    } else {
                        this.end_pointer(window, cx);
                        this.end_timeline_pointer(window, cx);
                    }
                }),
            )
            .on_mouse_up(
                gpui::MouseButton::Left,
                cx.listener(|this, event: &gpui::MouseUpEvent, window, cx| {
                    this.timeline.pointer = event.position;
                    this.end_pointer(window, cx);
                    this.end_timeline_pointer(window, cx);
                }),
            )
            .on_mouse_up_out(
                gpui::MouseButton::Left,
                cx.listener(|this, event: &gpui::MouseUpEvent, window, cx| {
                    this.timeline.pointer = event.position;
                    this.end_pointer(window, cx);
                    this.end_timeline_pointer(window, cx);
                }),
            )
            .on_key_down(cx.listener(Self::workspace_key))
            .child(div().absolute().size_0().track_focus(&self.focus_handle))
            .child(if self.current.is_some() {
                self.editor(layout, window, cx)
            } else {
                self.manager(window, cx)
            })
            .children(self.library_drag_overlay())
            .when(
                self.menu_open || self.project_menu.is_some() || self.session.media_menu.is_some(),
                |s| {
                    s.child(
                        div()
                            .absolute()
                            .inset_0()
                            .occlude()
                            .id("menu-dismiss")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.menu_open = false;
                                this.project_menu = None;
                                this.session.media_menu = None;
                                cx.notify();
                            })),
                    )
                    .when(self.menu_open, |s| {
                        s.child(self.file_menu(layout.title, cx))
                    })
                    .when(self.session.media_menu.is_some(), |s| {
                        s.child(self.media_actions(window, cx))
                    })
                    .when_some(self.project_menu.clone(), |s, entry| {
                        s.child(self.project_actions(entry, cx))
                    })
                },
            )
            .when(self.dialog.is_some(), |s| s.child(self.dialog_view(cx)))
            .children(self.notice.clone().map(|notice| {
                div()
                    .absolute()
                    .bottom(px(24.0))
                    .right(px(22.0))
                    .max_w(px(650.0))
                    .flex()
                    .items_center()
                    .gap(px(12.0))
                    .pl(px(16.0))
                    .pr(px(6.0))
                    .py(px(6.0))
                    .rounded(px(6.0))
                    .bg(rgb(0x353539))
                    .border_1()
                    .border_color(rgb(LINE))
                    .text_size(px(13.0))
                    .child(
                        svg()
                            .path(Icon::Info.path())
                            .size(px(18.0))
                            .text_color(rgb(TEXT)),
                    )
                    .child(notice)
                    .child(
                        tool("dismiss-notice", Icon::Close, true).on_click(cx.listener(
                            |this, _, _, cx| {
                                this.notice = None;
                                cx.notify();
                            },
                        )),
                    )
            }))
    }
}
#[cfg(test)]
#[path = "view_tests.rs"]
mod tests;
