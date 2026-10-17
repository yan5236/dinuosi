//! 素材剪贴板操作复用时间线持久化及单次撤销，不复制或删除源文件。
use crate::{shortcuts::Command, view::Desktop};
use gpui::Context;

impl Desktop {
    /// 按真实选择、锁定和历史判断菜单可用性；活动手势或后台任务期间禁止执行。
    /// 复制可读取锁定素材，剪切要求整组选区可编辑；删除沿用现有跳过锁定成员语义。
    pub(crate) fn can_edit_command(&self, command: Command) -> bool {
        if self.busy || self.current.is_none() || self.timeline.gesture.is_some() {
            return false;
        }
        let selected: Vec<_> = self
            .timeline
            .model
            .clips
            .iter()
            .filter(|c| self.timeline.selected.contains(&c.id))
            .collect();
        let editable = |c: &&dinuosi_core::timeline::Clip| {
            !c.locked && !self.timeline.model.tracks[c.track].locked
        };
        match command {
            Command::Undo => self.timeline.history.can_step(false),
            Command::Redo => self.timeline.history.can_step(true),
            Command::Copy => !selected.is_empty(),
            Command::Cut => !selected.is_empty() && selected.iter().all(editable),
            Command::Delete => selected.iter().any(editable),
            Command::Paste => self.timeline.clipboard.is_some(),
            _ => false,
        }
    }

    /// 复制当前明确选择；剪切先保存删除，成功才替换剪贴板，失败保持旧剪贴板和项目。
    /// 先提交正文及检查器草稿，复制包含最新参数；复制本身不产生撤销历史。
    pub(crate) fn copy_timeline_clips(&mut self, cut: bool, cx: &mut Context<Self>) {
        let command = if cut { Command::Cut } else { Command::Copy };
        if !self.can_edit_command(command) || !self.commit_text_input(cx) {
            return;
        }
        self.commit_inspector_input(cx);
        let Some(copy) = self.timeline.model.copy_clips(&self.timeline.selected) else {
            return;
        };
        if cut {
            self.stop_timeline();
            let before = self.timeline.model.clone();
            self.timeline.model.delete(&self.timeline.selected);
            if !self.commit_timeline(before, cx) {
                return;
            }
        }
        self.timeline.clipboard = Some(copy);
        cx.notify();
    }

    /// 将剪贴板在播放头处粘贴为一次保存/撤销；成功选中新片段，失败保持原选择及原模型并显示错误。
    /// 重复粘贴读取同一快照，每次生成独立身份，源文件始终按路径引用。
    pub(crate) fn paste_timeline_clips(&mut self, cx: &mut Context<Self>) {
        if !self.can_edit_command(Command::Paste) || !self.commit_text_input(cx) {
            return;
        }
        self.commit_inspector_input(cx);
        self.stop_timeline();
        let copy = self.timeline.clipboard.as_ref().unwrap();
        let before = self.timeline.model.clone();
        let frame = self.timeline.model.frame(self.session.timeline_position);
        match self.timeline.model.paste_clips(copy, frame) {
            Ok(ids) => {
                if self.commit_timeline(before, cx) {
                    self.timeline.primary_clip = ids.iter().next().cloned();
                    self.timeline.selected = ids;
                    self.timeline.preview = true;
                    self.refresh_timeline(cx);
                }
            }
            Err(error) => self.notice = Some(error),
        }
        cx.notify();
    }
}
