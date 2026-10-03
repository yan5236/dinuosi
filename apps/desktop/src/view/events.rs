//! 工作区键盘路由：输入框优先，时间线快捷键委派核心编辑。
use super::*;
impl Desktop {
    /// 按当前焦点路由快捷键，输入参数时只接受提交/取消，防止误删片段。
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
        if self.timeline.input.is_some() || self.timeline.rename.is_some() {
            if key == "enter" {
                self.commit_inspector_input(cx);
                self.focus_handle.focus(window);
            } else if key == "escape" {
                self.timeline.input = None;
                self.timeline.rename = None;
                self.focus_handle.focus(window);
                cx.notify();
            }
            return;
        }
        if key == "delete" || key == "backspace" {
            self.delete_clips(cx);
            return;
        }
        if event.keystroke.modifiers.control && key == "z" {
            self.timeline_history(event.keystroke.modifiers.shift, cx);
            return;
        }
        if event.keystroke.modifiers.control && key == "y" {
            self.timeline_history(true, cx);
            return;
        }
        if key == "space" && self.timeline.preview {
            self.play_timeline(window, cx);
            return;
        }
        if (event.keystroke.modifiers.control && key == "n")
            || (self.current.is_none() && matches!(key, "enter" | "space"))
        {
            self.show_dialog(DialogKind::Create, window, cx);
        } else if event.keystroke.modifiers.control && key == "o" {
            self.import_project(window, cx);
        } else if event.keystroke.modifiers.control && key == "h" {
            self.show_manager(window, cx);
        } else if key == "escape" {
            self.menu_open = false;
            self.project_menu = None;
            self.session.media_menu = None;
            self.notice = None;
            cx.notify();
        }
    }
}
