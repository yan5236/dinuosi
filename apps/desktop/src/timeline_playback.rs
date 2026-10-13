//! 倒放、定格工具的宿主适配；保存、错误回滚和撤回复用既有事务。
use crate::view::Desktop;
use dinuosi_core::media::MediaKind;
use gpui::{Context, Window};

impl Desktop {
    /// 查询选择中是否有可编辑目标；定格仅视频，倒放支持视频和音频，忙碌或锁定时禁用。
    pub(crate) fn can_change_clip_playback(&self, freeze: bool) -> bool {
        !self.busy
            && self.timeline.model.clips.iter().any(|c| {
                self.timeline.selected.contains(&c.id)
                    && !c.locked
                    && !self.timeline.model.tracks[c.track].locked
                    && if freeze {
                        c.kind == MediaKind::Video
                    } else {
                        c.kind != MediaKind::Image
                    }
            })
    }

    /// 所有选中且类型匹配的片段均启用效果时显示工具选中底色；空选择和混合状态不高亮。
    pub(crate) fn selected_playback_active(&self, freeze: bool) -> bool {
        let mut clips = self
            .timeline
            .model
            .clips
            .iter()
            .filter(|c| {
                self.timeline.selected.contains(&c.id)
                    && if freeze {
                        c.kind == MediaKind::Video
                    } else {
                        c.kind != MediaKind::Image
                    }
            })
            .peekable();
        clips.peek().is_some()
            && clips.all(|c| {
                if freeze {
                    c.freeze_frame.is_some()
                } else {
                    c.reversed
                }
            })
    }

    /// 先完成输入和手势，再切换所选片段的方向或播放头画面定格，保存成功只记录一次历史。
    /// 失败由现有事务回滚；停止旧播放并刷新时间线预览，恢复键盘焦点，不修改源素材。
    pub(crate) fn change_clip_playback(
        &mut self,
        freeze: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.can_change_clip_playback(freeze) {
            return;
        }
        self.commit_inspector_input(cx);
        self.end_timeline_pointer(window, cx);
        self.stop_timeline();
        self.stop_preview();
        let before = self.timeline.model.clone();
        if freeze {
            let frame = self.keyframe_frame();
            self.timeline
                .model
                .freeze_clips(&self.timeline.selected, frame);
        } else {
            self.timeline.model.reverse_clips(&self.timeline.selected);
        }
        self.timeline.preview = true;
        self.commit_timeline(before, cx);
        self.focus_handle.focus(window);
        cx.notify();
    }
}
