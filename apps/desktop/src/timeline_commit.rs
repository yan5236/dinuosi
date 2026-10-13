//! 时间线持久化、删除和历史事务；指针适配与工具共用同一保存及回滚路径。
use crate::view::Desktop;
use dinuosi_core::timeline::Timeline;
use gpui::Context;

impl Desktop {
    /// 提交前清理空非主轨并同步保存元数据；主轨保留，轨道清理与编辑共用一条撤销历史。
    /// 失败回滚本次编辑并显示实际错误，清除不存在的片段选择，成功刷新轨道几何和预览。
    pub(crate) fn commit_timeline(&mut self, before: Timeline, cx: &mut Context<Self>) {
        self.timeline.model.remove_empty_tracks();
        self.timeline.rename = None;
        if before == self.timeline.model {
            return;
        }
        let result = self
            .current
            .as_ref()
            .ok_or_else(|| "没有打开项目".to_owned())
            .and_then(|entry| self.store.save_timeline(entry, &self.timeline.model));
        if let Err(error) = result {
            self.timeline.model = before;
            self.notice = Some(error);
        } else {
            self.timeline.history.record(before, &self.timeline.model);
        }
        self.timeline
            .selected
            .retain(|id| self.timeline.model.clips.iter().any(|c| &c.id == id));
        self.refresh_timeline(cx);
    }
    /// 删除未锁定的选中片段，持久化成功后可撤销；不更改任何素材文件。
    pub(crate) fn delete_clips(&mut self, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        self.stop_timeline();
        let before = self.timeline.model.clone();
        self.timeline.model.delete(&self.timeline.selected);
        self.commit_timeline(before, cx);
        self.timeline
            .selected
            .retain(|id| self.timeline.model.clips.iter().any(|c| &c.id == id));
        cx.notify();
    }
    /// 撤销/重做由核心快照交换执行，保存失败时反向交换恢复历史和当前模型；成功后清理失效选择，让检查器显示正确项目资料。
    pub(crate) fn timeline_history(&mut self, redo: bool, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        self.stop_timeline();
        let before = self.timeline.model.clone();
        self.timeline.history.step(&mut self.timeline.model, redo);
        if self.timeline.model != before
            && let Some(entry) = &self.current
        {
            if let Err(error) = self.store.save_timeline(entry, &self.timeline.model) {
                self.timeline.history.step(&mut self.timeline.model, !redo);
                self.notice = Some(error);
            }
            self.timeline
                .selected
                .retain(|id| self.timeline.model.clips.iter().any(|c| &c.id == id));
            self.refresh_timeline(cx);
        }
    }
}
