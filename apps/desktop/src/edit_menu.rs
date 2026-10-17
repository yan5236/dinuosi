//! 顶部编辑与素材右键菜单共用命令、禁用状态及键盘导航，沿用现有灰阶浮层。
use crate::{
    shortcuts::{self, Command},
    ui::{LINE, MUTED},
    view::Desktop,
};
use gpui::{
    Context, Div, FocusHandle, MouseButton, Pixels, Point, Window, div, prelude::*, px, rgb,
};

pub(crate) struct EditMenu {
    pub position: Option<Point<Pixels>>,
    pub focus: FocusHandle,
    pub active: Option<usize>,
}

const ITEMS: &[(Command, &str)] = &[
    (Command::Undo, "Undo"),
    (Command::Redo, "Redo"),
    (Command::Cut, "Cut"),
    (Command::Copy, "Copy"),
    (Command::Paste, "Paste"),
    (Command::Delete, "Delete"),
];

impl Desktop {
    /// 先提交草稿，再打开顶部或指针菜单；右键未选中素材时选择其关联组，已选中则保留多选。
    /// 不移动播放头，清除曲线节点选择，菜单焦点隔离工作区操作；保存正文失败则保留草稿。
    /// 打开时没有键盘活动项，不预先高亮任何菜单行；鼠标悬停和后续方向键导航分别决定反馈。
    pub(crate) fn open_edit_menu(
        &mut self,
        clip: Option<&str>,
        position: Option<Point<Pixels>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.busy
            || self.current.is_none()
            || self.timeline.gesture.is_some()
            || !self.commit_text_input(cx)
        {
            return;
        }
        self.commit_inspector_input(cx);
        if let Some(id) = clip {
            if !self.timeline.model.clips.iter().any(|c| c.id == id) {
                return;
            }
            if !self.timeline.selected.contains(id) {
                self.timeline.selected = self.timeline.model.linked_ids(&[id.to_owned()].into());
            }
            self.timeline.primary_clip = Some(id.to_owned());
        }
        self.timeline.curves.selected = None;
        self.menu_open = false;
        self.help_open = false;
        self.project_menu = None;
        self.session.media_menu = None;
        self.timeline.keyframe_menu = None;
        self.timeline.marker_menu = None;
        let focus = cx.focus_handle();
        focus.focus(window);
        self.timeline.edit_menu = Some(EditMenu {
            position,
            focus,
            active: None,
        });
        cx.notify();
    }

    /// 执行已启用的编辑命令并恢复工作区焦点；菜单和快捷键使用同一业务入口。
    pub(crate) fn run_edit_command(
        &mut self,
        command: Command,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.can_edit_command(command) {
            return;
        }
        self.timeline.edit_menu = None;
        self.focus_handle.focus(window);
        match command {
            Command::Undo => self.timeline_history(false, cx),
            Command::Redo => self.timeline_history(true, cx),
            Command::Cut => self.copy_timeline_clips(true, cx),
            Command::Copy => self.copy_timeline_clips(false, cx),
            Command::Paste => self.paste_timeline_clips(cx),
            Command::Delete => self.delete_clips(cx),
            _ => {}
        }
        cx.notify();
    }

    /// 渲染同样的文本菜单；顶部含撤销/重做，右键含剪切/复制/粘贴/删除，快捷键读取用户配置。
    /// 浮层夹在窗口内，禁用项不响应；方向键跳过禁用项，Enter/空格执行，Escape 关闭。
    /// 只有经过键盘导航的行显示活动背景；鼠标在菜单内移动时清除键盘活动项，改用真实悬停。
    pub(crate) fn edit_actions_menu(&self, window: &Window, cx: &mut Context<Self>) -> Div {
        let menu = self.timeline.edit_menu.as_ref().unwrap();
        let start = if menu.position.is_some() { 2 } else { 0 };
        let viewport = window.viewport_size();
        let position = menu.position.unwrap_or(gpui::point(
            px(225.0),
            px(crate::editor::Layout::new(viewport.width.into(), viewport.height.into()).title),
        ));
        let preferences = &cx.global::<crate::preferences::UserPreferences>().0;
        div()
            .absolute()
            .occlude()
            .debug_selector(|| "edit-actions-menu".into())
            .left(px(
                f32::from(position.x).clamp(0.0, (f32::from(viewport.width) - 260.0).max(0.0))
            ))
            .top(px(f32::from(position.y).clamp(
                0.0,
                (f32::from(viewport.height) - (ITEMS.len() - start) as f32 * 36.0 - 12.0).max(0.0),
            )))
            .w(px(260.0))
            .p(px(5.0))
            .bg(rgb(0x303034))
            .border_1()
            .border_color(rgb(LINE))
            .rounded(px(6.0))
            .child(div().size_0().track_focus(&menu.focus))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|_, _, _, cx| cx.stop_propagation()),
            )
            .on_key_down(cx.listener(|this, event: &gpui::KeyDownEvent, window, cx| {
                this.edit_menu_key(event, window, cx);
                cx.stop_propagation();
            }))
            .on_mouse_move(cx.listener(|this, _, _, cx| {
                if let Some(menu) = &mut this.timeline.edit_menu
                    && menu.active.take().is_some()
                {
                    cx.notify();
                }
            }))
            .children(
                ITEMS
                    .iter()
                    .enumerate()
                    .skip(start)
                    .map(|(index, &(command, label))| {
                        let enabled = self.can_edit_command(command);
                        let shortcut = shortcuts::SHORTCUTS
                            .iter()
                            .find(|s| s.command == command)
                            .unwrap();
                        let selector = format!("edit-action-{}", shortcut.id);
                        let keys = shortcuts::label(&shortcuts::bindings(shortcut, preferences));
                        div()
                            .id(gpui::SharedString::from(selector.clone()))
                            .debug_selector(move || selector.clone())
                            .h(px(36.0))
                            .px(px(10.0))
                            .flex()
                            .items_center()
                            .justify_between()
                            .when(!enabled, |s| s.text_color(rgb(MUTED)))
                            .when(enabled, |s| s.cursor_pointer().hover(|s| s.bg(rgb(LINE))))
                            .when(enabled && Some(index) == menu.active, |s| s.bg(rgb(LINE)))
                            .child(crate::i18n::t("editor", label, cx))
                            .child(div().text_size(px(11.0)).text_color(rgb(MUTED)).child(keys))
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.run_edit_command(command, window, cx);
                                cx.stop_propagation();
                            }))
                    }),
            )
    }

    /// 菜单独占键盘，循环选择可用项；快捷键仍委派相同命令，其他按键不会穿透到时间线。
    /// 尚无活动项时向下从首项、向上从末项开始；未导航直接确认不执行任何命令。
    fn edit_menu_key(
        &mut self,
        event: &gpui::KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(menu) = &self.timeline.edit_menu else {
            return;
        };
        let start = if menu.position.is_some() { 2 } else { 0 };
        let active = menu.active;
        let key = event.keystroke.key.as_str();
        if key == "escape" {
            self.timeline.edit_menu = None;
            self.focus_handle.focus(window);
        } else if matches!(key, "up" | "down" | "tab") {
            let backwards = key == "up" || (key == "tab" && event.keystroke.modifiers.shift);
            let count = ITEMS.len() - start;
            let active = active.unwrap_or(if backwards { start } else { ITEMS.len() - 1 });
            for step in 1..=count {
                let index =
                    start + (active - start + if backwards { count - step } else { step }) % count;
                if self.can_edit_command(ITEMS[index].0) {
                    self.timeline.edit_menu.as_mut().unwrap().active = Some(index);
                    break;
                }
            }
        } else if matches!(key, "enter" | "space") {
            if let Some(active) = active {
                self.run_edit_command(ITEMS[active].0, window, cx);
            }
        } else if let Some(command) = shortcuts::resolve(
            &event.keystroke,
            &cx.global::<crate::preferences::UserPreferences>().0,
        ) {
            self.run_edit_command(command, window, cx);
        }
        cx.notify();
    }
}
