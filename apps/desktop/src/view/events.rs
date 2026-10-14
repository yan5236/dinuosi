//! 工作区键盘路由：输入框优先，时间线快捷键委派核心编辑。
use super::*;
impl Desktop {
    /// 按当前焦点与统一快捷键注册表路由；输入参数时只接受提交/取消，防止误删片段。
    /// 全屏模式只接受播放/退出与焦点导航；其余非输入状态下 Tab 遍历工具以显示 Tooltip。
    /// 曲线/调色输入优先接收提交/取消；两类拖动都可取消，选中动画曲线节点的 Delete 只删除该属性节点。
    /// Esc 优先退出 HSL 取样模式并释放像素，不落入片段选择或项目快捷键。
    pub(super) fn workspace_key(
        &mut self,
        event: &gpui::KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.export_dialog.is_some() {
            self.export_key(event, window, cx);
            return;
        }
        if self.preview_fullscreen {
            self.fullscreen_preview_key(event, window, cx);
            return;
        }
        if self.timeline.marker_dialog.is_some() {
            self.marker_dialog_key(event, window, cx);
            return;
        }
        if self.dialog.is_some() {
            self.dialog_key(event, window, cx);
            return;
        }
        if self.busy {
            return;
        }
        let key = event.keystroke.key.as_str();
        if self.timeline.color.picker.is_some() && key == "escape" {
            self.cancel_hsl_picker();
            cx.notify();
            cx.stop_propagation();
            return;
        }
        if self.timeline.curves.input.is_some() {
            if key == "enter" {
                self.commit_curve_input(cx);
                self.focus_handle.focus(window);
            } else if key == "escape" {
                self.timeline.curves.input = None;
                self.timeline.curves.blur = None;
                self.focus_handle.focus(window);
                cx.notify();
            }
            return;
        }
        if key == "escape"
            && matches!(
                self.timeline.gesture,
                Some(
                    crate::timeline_state::Gesture::Curve(_)
                        | crate::timeline_state::Gesture::Color(_)
                )
            )
        {
            let curve = matches!(
                self.timeline.gesture,
                Some(crate::timeline_state::Gesture::Curve(_))
            );
            self.timeline.gesture = None;
            if let Some(before) = self.timeline.before.take() {
                self.timeline.model = before;
            }
            if curve {
                self.timeline.curves.selected = None;
            }
            self.refresh_timeline(cx);
            cx.notify();
            cx.stop_propagation();
            return;
        }
        if self.timeline.color.input.is_some()
            || self.timeline.input.is_some()
            || self.timeline.volume_input.is_some()
            || self.timeline.speed_input.is_some()
            || self.timeline.rename.is_some()
        {
            if key == "enter" {
                self.commit_inspector_input(cx);
                self.focus_handle.focus(window);
            } else if key == "escape" {
                self.timeline.input = None;
                self.timeline.color.input = None;
                self.timeline.volume_input = None;
                self.timeline.speed_input = None;
                self.timeline.input_blur = None;
                self.timeline.rename = None;
                self.focus_handle.focus(window);
                cx.notify();
            }
            return;
        }
        if self.timeline.marker_menu.is_some() {
            if key == "escape" {
                self.timeline.marker_menu = None;
                self.focus_handle.focus(window);
                cx.notify();
            }
            cx.stop_propagation();
            return;
        }
        if self.timeline.keyframe_menu.is_some() {
            if key == "escape" {
                self.timeline.keyframe_menu = None;
                self.focus_handle.focus(window);
                cx.notify();
            }
            cx.stop_propagation();
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
            Some(Command::Delete) if self.timeline.curves.open && self.curve_key().is_some() => {
                self.delete_curve_key(cx);
            }
            Some(Command::Delete) => self.delete_clips(cx),
            Some(Command::Undo) => {
                self.end_timeline_pointer(window, cx);
                self.timeline_history(false, cx);
            }
            Some(Command::Redo) => {
                self.end_timeline_pointer(window, cx);
                self.timeline_history(true, cx);
            }
            Some(Command::Play) if self.current.is_some() => {
                if self.timeline.preview {
                    self.play_timeline(window, cx);
                } else {
                    self.play_preview(window, cx);
                }
            }
            Some(Command::Settings) => self.open_settings(crate::settings::Tab::General, cx),
            Some(Command::SelectTool) if self.current.is_some() => {
                self.set_blade_tool(false, window, cx);
            }
            Some(Command::BladeTool) if self.current.is_some() => {
                self.set_blade_tool(true, window, cx);
            }
            Some(Command::SplitAtPlayhead) if self.current.is_some() => {
                let frame = self.timeline.model.frame(self.session.timeline_position);
                self.split_timeline_clips(self.timeline.selected.clone(), frame, cx);
            }
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
