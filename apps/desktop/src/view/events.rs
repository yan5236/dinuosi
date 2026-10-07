//! 工作区键盘路由：输入框优先，时间线快捷键委派核心编辑。
use super::*;
impl Desktop {
    /// 按当前焦点与统一快捷键注册表路由；输入参数时只接受提交/取消，防止误删片段。
    /// 非输入状态下 Tab/Shift+Tab 遍历工具触发元素，以便键盘聚焦显示 Tooltip。
    pub(super) fn workspace_key(
        &mut self,
        event: &gpui::KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.dialog.is_some() {
            self.dialog_key(event, window, cx);
            return;
        }
        if self.busy {
            return;
        }
        let key = event.keystroke.key.as_str();
        if self.timeline.input.is_some()
            || self.timeline.volume_input.is_some()
            || self.timeline.rename.is_some()
        {
            if key == "enter" {
                self.commit_inspector_input(cx);
                self.focus_handle.focus(window);
            } else if key == "escape" {
                self.timeline.input = None;
                self.timeline.volume_input = None;
                self.timeline.input_blur = None;
                self.timeline.rename = None;
                self.focus_handle.focus(window);
                cx.notify();
            }
            return;
        }
        if key == "tab" {
            if event.keystroke.modifiers.shift {
                window.focus_prev();
            } else {
                window.focus_next();
            }
            cx.stop_propagation();
            return;
        }
        use crate::shortcuts::Command;
        let preferences = &cx.global::<crate::preferences::UserPreferences>().0;
        match crate::shortcuts::resolve(&event.keystroke, preferences) {
            Some(Command::New) => self.show_dialog(DialogKind::Create, window, cx),
            Some(Command::Open) => self.import_project(window, cx),
            Some(Command::Home) => self.show_manager(window, cx),
            Some(Command::Delete) => self.delete_clips(cx),
            Some(Command::Undo) => self.timeline_history(false, cx),
            Some(Command::Redo) => self.timeline_history(true, cx),
            Some(Command::Play) if self.current.is_some() => {
                if self.timeline.preview {
                    self.play_timeline(window, cx);
                } else {
                    self.play_preview(window, cx);
                }
            }
            Some(Command::Settings) => self.open_settings(crate::settings::Tab::General, cx),
            _ if self.current.is_none() && matches!(key, "enter" | "space") => {
                self.show_dialog(DialogKind::Create, window, cx);
            }
            _ if key == "escape" => {
                self.menu_open = false;
                self.help_open = false;
                self.project_menu = None;
                self.session.media_menu = None;
                self.notice = None;
                cx.notify();
            }
            _ => return,
        }
        cx.stop_propagation();
    }
}
