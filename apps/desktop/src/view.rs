use dinuosi_command::{EditCommand, execute};
use dinuosi_core::Project;
use dinuosi_icons::Icon;
use gpui::{Context, Div, FocusHandle, IntoElement, Render, Window, div, prelude::*, px, rgb, svg};

use crate::ui::{self, BACKGROUND, LINE, MUTED, PANEL, TEXT, brand, pill, tool};

/// 窗口会话与项目列表；返回管理页保留内存项目，关闭应用后不保留。
pub struct Desktop {
    focus_handle: FocusHandle,
    projects: Vec<Project>,
    current: Option<usize>,
    menu_open: bool,
    notice: Option<&'static str>,
    pub(crate) zoom: f32,
    pub(crate) volume: f32,
}

impl Desktop {
    /// 初始化管理页并聚焦快捷键节点；不生成示例项目，不读写文件系统。
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let focus_handle = cx.focus_handle();
        focus_handle.focus(window);
        Self {
            focus_handle,
            projects: Vec::new(),
            current: None,
            menu_open: false,
            notice: None,
            zoom: 1.0,
            volume: 1.0,
        }
    }

    /// 通过共享命令创建内存项目并切换剪辑页；校验失败只显示错误，不改变已有项目。
    fn create_project(&mut self, cx: &mut Context<Self>) {
        match execute(EditCommand::CreateProject {
            name: "未命名项目".into(),
        }) {
            Ok(project) => {
                self.projects.push(project);
                self.current = Some(self.projects.len() - 1);
                self.notice = None;
            }
            Err(error) => self.notice = Some(error),
        }
        self.menu_open = false;
        cx.notify();
    }

    /// 返回管理页并保留会话项目；仅变更导航状态，不执行保存或删除。
    fn show_manager(&mut self, cx: &mut Context<Self>) {
        self.current = None;
        self.menu_open = false;
        self.notice = None;
        cx.notify();
    }

    /// 按用户操作显示未接入功能的简短说明，不添加常驻的副标题。
    fn inform(&mut self, message: &'static str, cx: &mut Context<Self>) {
        self.notice = Some(message);
        self.menu_open = false;
        cx.notify();
    }

    /// 按管理草图构造顶部品牌、三个横向入口和纵向项目列表。
    /// 分界线遵循参考图比例，空状态不伪造最近项目；卡片点击进入同一内存项目。
    fn manager(&self, width: f32, height: f32, window: &Window, cx: &mut Context<Self>) -> Div {
        let row_height = height * 0.263;
        div()
            .size_full()
            .flex()
            .flex_col()
            .child(
                div()
                    .flex()
                    .items_center()
                    .flex_none()
                    .h(px(height * 0.10))
                    .px(px(26.0))
                    .child(brand(58.0))
                    .child(ui::title_drag())
                    .child(ui::window_controls(window, false)),
            )
            .child(
                div()
                    .flex()
                    .flex_none()
                    .items_center()
                    .gap(px(width * 0.018))
                    .px(px(28.0))
                    .h(px(height * 0.083))
                    .border_b_1()
                    .border_color(rgb(LINE))
                    .child(
                        pill("new-project", "新建项目", width * 0.17, 30.0)
                            .on_click(cx.listener(|this, _, _, cx| this.create_project(cx))),
                    )
                    .child(pill("import-project", "导入", width * 0.17, 30.0).on_click(
                        cx.listener(|this, _, _, cx| this.inform("项目导入尚未开放", cx)),
                    ))
                    .child(
                        pill("settings", "设置", width * 0.17, 30.0).on_click(
                            cx.listener(|this, _, _, cx| this.inform("设置尚未开放", cx)),
                        ),
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
                                .h(px(row_height))
                                .flex()
                                .items_center()
                                .justify_center()
                                .text_color(rgb(MUTED))
                                .text_size(px(14.0))
                                .child("暂无项目"),
                        )
                    })
                    .children(self.projects.iter().enumerate().map(|(index, project)| {
                        div()
                            .id(("project", index))
                            .flex()
                            .items_center()
                            .h(px(row_height))
                            .mx(px(16.0))
                            .px(px(24.0))
                            .gap(px(width * 0.036))
                            .border_b_1()
                            .border_color(rgb(LINE))
                            .cursor_pointer()
                            .hover(|s| s.bg(rgb(0x29292c)))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.current = Some(index);
                                this.notice = None;
                                cx.notify();
                            }))
                            .child(
                                div()
                                    .w(px(width * 0.22))
                                    .h(px(row_height * 0.77))
                                    .flex_none()
                                    .rounded(px(16.0))
                                    .border_1()
                                    .border_color(rgb(LINE))
                                    .bg(rgb(PANEL)),
                            )
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .flex()
                                    .flex_col()
                                    .gap(px(16.0))
                                    .child(
                                        div().text_size(px(24.0)).child(project.name().to_owned()),
                                    )
                                    .child(
                                        div()
                                            .flex()
                                            .flex_col()
                                            .gap(px(8.0))
                                            .text_size(px(14.0))
                                            .text_color(rgb(MUTED))
                                            .child("创建时间：本次会话")
                                            .child("修改时间：尚未修改"),
                                    ),
                            )
                            .child(
                                tool(format!("project-more-{index}"), Icon::More, true).on_click(
                                    cx.listener(|this, _, _, cx| {
                                        cx.stop_propagation();
                                        this.inform(
                                            "项目仅保留在本次会话，保存与文件管理尚未开放",
                                            cx,
                                        );
                                    }),
                                ),
                            )
                    })),
            )
    }

    /// 构造自绘剪辑标题栏；品牌返回管理页，文件菜单提供真实导航。
    /// 空白区保留原生拖动能力，未实现功能仅在用户点击时提示。
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
                    .cursor_pointer()
                    .mr(px(22.0))
                    .child(brand(42.0))
                    .tooltip(|_, cx| cx.new(|_| ui::Hint("项目管理".into())).into())
                    .on_click(cx.listener(|this, _, _, cx| this.show_manager(cx))),
            )
            .child(
                div()
                    .id("file-menu")
                    .px(px(14.0))
                    .py(px(8.0))
                    .cursor_pointer()
                    .hover(|s| s.bg(rgb(PANEL)))
                    .child("文件")
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.menu_open = !this.menu_open;
                        this.notice = None;
                        cx.notify();
                    })),
            )
            .child(
                div()
                    .id("edit-menu")
                    .px(px(14.0))
                    .py(px(8.0))
                    .cursor_pointer()
                    .hover(|s| s.bg(rgb(PANEL)))
                    .child("编辑")
                    .on_click(cx.listener(|this, _, _, cx| this.inform("暂无可编辑的片段", cx))),
            )
            .child(
                div()
                    .id("help-menu")
                    .px(px(14.0))
                    .py(px(8.0))
                    .cursor_pointer()
                    .hover(|s| s.bg(rgb(PANEL)))
                    .child("帮助")
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.inform("Ctrl+N 新建项目 · Ctrl+H 项目管理 · Esc 关闭菜单", cx)
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

    /// 构造文件菜单浮层，不改变底层布局；所有入口调用与主界面相同的处理方法。
    fn file_menu(&self, top: f32, cx: &mut Context<Self>) -> Div {
        div()
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
                    .on_click(cx.listener(|this, _, _, cx| this.create_project(cx))),
            )
            .child(
                div()
                    .id("menu-home")
                    .p(px(10.0))
                    .cursor_pointer()
                    .hover(|s| s.bg(rgb(LINE)))
                    .child("项目管理")
                    .on_click(cx.listener(|this, _, _, cx| this.show_manager(cx))),
            )
            .child(
                div()
                    .id("menu-close")
                    .p(px(10.0))
                    .cursor_pointer()
                    .hover(|s| s.bg(rgb(LINE)))
                    .child("关闭窗口")
                    .on_click(|_, window, _| window.remove_window()),
            )
    }
}

impl Render for Desktop {
    /// 根据窗口尺寸构造两个原生工作区；Ctrl+N 新建，Ctrl+H 返回，Escape 关闭浮层。
    /// 快捷键焦点放在零尺寸节点，避免 GPUI 自动聚焦吞掉 Windows 标题栏的默认鼠标行为。
    /// 状态只在输入回调中变更，渲染过程不写项目、不显示宣传性副标题。
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let size = window.viewport_size();
        let width = f32::from(size.width);
        let height = f32::from(size.height);
        let layout = crate::editor::Layout::new(width, height);
        let shortcuts = div()
            .absolute()
            .size_0()
            .track_focus(&self.focus_handle)
            .on_key_down(cx.listener(|this, event: &gpui::KeyDownEvent, _, cx| {
                let key = event.keystroke.key.as_str();
                if (event.keystroke.modifiers.control && key == "n")
                    || (this.current.is_none() && matches!(key, "enter" | "space"))
                {
                    this.create_project(cx);
                } else if event.keystroke.modifiers.control && key == "h" {
                    this.show_manager(cx);
                } else if key == "escape" {
                    this.menu_open = false;
                    this.notice = None;
                    cx.notify();
                }
            }));
        div()
            .relative()
            .size_full()
            .overflow_hidden()
            .bg(rgb(BACKGROUND))
            .text_color(rgb(TEXT))
            .text_size(px(14.0))
            .font_family("Microsoft YaHei UI")
            .child(shortcuts)
            .child(if self.current.is_some() {
                self.editor(layout, window, cx)
            } else {
                self.manager(width, height, window, cx)
            })
            .when(self.menu_open, |s| {
                s.child(
                    div()
                        .absolute()
                        .size_full()
                        .id("menu-dismiss")
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.menu_open = false;
                            cx.notify();
                        })),
                )
                .child(self.file_menu(layout.title, cx))
            })
            .children(self.notice.map(|notice| {
                div()
                    .absolute()
                    .bottom(px(44.0))
                    .right(px(22.0))
                    .max_w(px(480.0))
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
mod tests {
    use super::*;
    use gpui::{Modifiers, MouseButton, TestAppContext, VisualTestContext, point, size};

    /// 向标题栏及系统按钮分发真实鼠标按下事件，确保应用没有阻止 Windows 默认拖动或控制行为。
    /// 不模拟操作系统移动；坐标来自本帧布局，原生窗口状态另由桌面实测确认。
    fn assert_native_mouse_default(cx: &mut VisualTestContext, selectors: &[&'static str]) {
        for selector in selectors {
            let position = cx
                .debug_bounds(selector)
                .expect("缺少原生窗口命中区")
                .center();
            cx.simulate_mouse_down(position, MouseButton::Left, Modifiers::none());
            assert!(
                !cx.update(|window, _| window.default_prevented()),
                "{selector} 拦截了系统默认行为"
            );
            cx.simulate_mouse_up(
                point(px(-1.0), px(-1.0)),
                MouseButton::Left,
                Modifiers::none(),
            );
        }
    }

    /// 在真实 GPUI 布局及事件分发中检查右上窗口入口、新建、返回和标尺缩放。
    /// 覆盖默认及最小窗口；此测试检查几何和交互，不代替操作系统截图验收。
    #[gpui::test]
    fn workspace_navigation_and_window_controls(cx: &mut TestAppContext) {
        let (desktop, cx) = cx.add_window_view(Desktop::new);
        for (width, height) in [(1440.0, 900.0), (1040.0, 640.0)] {
            cx.simulate_resize(size(px(width), px(height)));
            cx.run_until_parked();
            let close = cx.debug_bounds("window-close").expect("管理页缺少关闭控件");
            let minimize = cx
                .debug_bounds("window-minimize")
                .expect("管理页缺少最小化控件");
            assert!(close.right() <= px(width) && close.left() > px(width - 100.0));
            assert!(minimize.right() <= close.left());
            let drag = cx.debug_bounds("title-drag").expect("缺少标题栏拖动区");
            assert!(drag.size.width > px(width * 0.5), "拖动区过窄：{drag:?}");
            assert!(
                drag.size.height >= px(height * 0.099),
                "拖动区高度不足：{drag:?}"
            );
            assert_native_mouse_default(cx, &["title-drag", "window-minimize", "window-close"]);
            // GPUI 0.2.2 不清理旧帧 debug_bounds；只在首屏检查不存在的控件。
            if width == 1440.0 {
                assert!(cx.debug_bounds("window-maximize").is_none());
            }
            let new_project = cx.debug_bounds("new-project").expect("缺少新建项目入口");
            cx.simulate_click(new_project.center(), Modifiers::none());
            assert!(cx.read(|app| desktop.read(app).current.is_some()));
            assert!(cx.debug_bounds("window-maximize").is_some());
            assert_native_mouse_default(
                cx,
                &[
                    "title-drag",
                    "window-minimize",
                    "window-maximize",
                    "window-close",
                ],
            );
            let media = cx.debug_bounds("素材库").expect("缺少素材面板");
            let preview = cx.debug_bounds("preview-panel").expect("缺少预览面板");
            let inspector = cx.debug_bounds("检查器").expect("缺少检查器");
            let toolbar = cx.debug_bounds("timeline-toolbar").expect("缺少工具栏");
            assert!((f32::from(media.right()) - width * 0.302).abs() < 1.0);
            assert!((f32::from(preview.right()) - width * 0.741).abs() < 1.0);
            assert!((f32::from(inspector.right()) - width).abs() < 1.0);
            assert!((f32::from(toolbar.top()) - height * 0.573).abs() < 1.0);
            let zoom = cx.debug_bounds("zoom-in").expect("缺少时间线缩放入口");
            let previous_zoom = cx.read(|app| desktop.read(app).zoom);
            cx.simulate_click(zoom.center(), Modifiers::none());
            assert!(cx.read(|app| desktop.read(app).zoom > previous_zoom));
            let count = cx.read(|app| desktop.read(app).projects.len());
            cx.simulate_keystrokes("ctrl-h");
            assert!(cx.read(|app| desktop.read(app).current.is_none()));
            assert_eq!(cx.read(|app| desktop.read(app).projects.len()), count);
        }
    }
}
