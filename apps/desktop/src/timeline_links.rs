//! 关联工具复用已有保存、错误回滚及单次撤销事务，不增加界面入口。
use crate::view::Desktop;
use gpui::{Context, Window};

impl Desktop {
    /// 按当前选择和整组锁定状态决定关联/解除按钮是否可用；活动手势期间禁止改变组关系。
    pub(crate) fn can_link_clips(&self, unlink: bool) -> bool {
        !self.busy
            && self.timeline.gesture.is_none()
            && self
                .timeline
                .model
                .can_link_clips(&self.timeline.selected, unlink)
    }

    /// 关联或解除选择涉及的完整组，先提交检查器输入，再保存并记录一次历史。
    /// 保存失败复用事务回滚；成功关联后选中完整组，解除后保留原选择，源文件和片段位置不变。
    pub(crate) fn link_selected_clips(
        &mut self,
        unlink: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.can_link_clips(unlink) {
            return;
        }
        self.commit_inspector_input(cx);
        let before = self.timeline.model.clone();
        if self
            .timeline
            .model
            .link_clips(&self.timeline.selected, unlink)
        {
            self.commit_timeline(before, cx);
            if !unlink {
                self.timeline.selected = self.timeline.model.linked_ids(&self.timeline.selected);
            }
        }
        self.focus_handle.focus(window);
        cx.notify();
    }
}
