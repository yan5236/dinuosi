//! 全局字幕检查器：跨组列出全部字幕，列表独占剩余高度，逐句编辑和删除复用现有事务。
use crate::{
    timeline_state::InspectorTab,
    ui::{LINE, MUTED},
    view::Desktop,
};
use dinuosi_icons::Icon;
use gpui::{Context, Div, SharedString, Window, div, prelude::*, px, rgb};

impl Desktop {
    /// 仅在没有片段选择或检查器主对象属于字幕组时显示字幕页签。
    /// 多选沿用最后点击的主对象，避免编辑图片、音频或普通文本时出现无关的字幕入口。
    pub(crate) fn captions_tab_visible(&self) -> bool {
        self.timeline.selected.is_empty()
            || self
                .inspector_clip()
                .is_some_and(|c| c.text.as_ref().is_some_and(|t| t.group.is_some()))
    }

    /// 在空选择或字幕主对象上下文中，按起点和轨道列出项目全部字幕，不显示字体与效果设置。
    /// 点击整行同步播放头，点击正文进入原生输入；锁定句保留浏览能力，删除控件禁用。
    pub(crate) fn captions_inspector(&self, cx: &mut Context<Self>) -> Div {
        let mut captions: Vec<_> = self
            .timeline
            .model
            .clips
            .iter()
            .filter(|c| c.text.as_ref().is_some_and(|t| t.group.is_some()))
            .collect();
        captions.sort_by_key(|c| (c.start, c.track));
        let index = captions
            .iter()
            .position(|c| self.timeline.primary_clip.as_ref() == Some(&c.id));
        let targets = [
            ("Previous", index.and_then(|i| i.checked_sub(1))),
            (
                "Next",
                index.map_or_else(
                    || (!captions.is_empty()).then_some(0),
                    |i| (i + 1 < captions.len()).then_some(i + 1),
                ),
            ),
        ];
        div()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .p(px(12.0))
            .text_size(px(12.0))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .mb(px(8.0))
                    .flex_none()
                    .children(targets.into_iter().map(|(label, target)| {
                        let id = target.map(|i| captions[i].id.clone());
                        crate::ui::panel_button(label, false, id.is_some() && !self.busy)
                            .child(crate::i18n::t("text", label, cx))
                            .on_click(cx.listener(move |this, _, window, cx| {
                                if !this.busy
                                    && let Some(id) = &id
                                {
                                    this.select_caption(id.clone(), window, cx);
                                }
                            }))
                    }))
                    .child(div().flex_1())
                    .child(
                        crate::ui::panel_button("text-add-caption", false, !self.busy)
                            .child(crate::i18n::t("text", "Add subtitle", cx))
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.add_text_clip(true, None, window, cx)
                            })),
                    ),
            )
            .child(
                div()
                    .id("caption-list")
                    .debug_selector(|| "caption-list".into())
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .children(captions.into_iter().enumerate().map(|(i, c)| {
                        let id = c.id.clone();
                        let delete_id = id.clone();
                        let row_id = id.clone();
                        let locked = c.locked || self.timeline.model.tracks[c.track].locked;
                        let edit = self.timeline.text.body.as_ref().filter(|e| e.id == id);
                        div()
                            .id(SharedString::from(format!("caption-{id}")))
                            .debug_selector(move || format!("caption-row-{i}"))
                            .px(px(10.0))
                            .py(px(8.0))
                            .mb(px(6.0))
                            .rounded(px(4.0))
                            .border_1()
                            .border_color(rgb(LINE))
                            .bg(rgb(crate::ui::PANEL))
                            .cursor_pointer()
                            .hover(|s| s.bg(rgb(0x303033)))
                            .when(self.timeline.selected.contains(&id), |s| {
                                s.bg(rgb(0x38383c)).border_color(rgb(MUTED))
                            })
                            .on_click(cx.listener(move |this, _, window, cx| {
                                if !this.busy {
                                    this.select_caption(row_id.clone(), window, cx);
                                }
                            }))
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap(px(6.0))
                                    .child(
                                        div()
                                            .flex_1()
                                            .min_w_0()
                                            .text_size(px(11.0))
                                            .font_family(crate::ui::MONO_FONT)
                                            .text_color(rgb(MUTED))
                                            .child(format!(
                                                "{}   {} — {}",
                                                i + 1,
                                                crate::timeline_markers::timecode(
                                                    c.start,
                                                    self.timeline.model.fps
                                                ),
                                                crate::timeline_markers::timecode(
                                                    c.start + c.duration,
                                                    self.timeline.model.fps
                                                )
                                            )),
                                    )
                                    .child(
                                        crate::tooltip::Tooltip::tool(
                                            format!("caption-delete-{i}"),
                                            Icon::Trash,
                                            !locked && !self.busy,
                                            crate::i18n::t("text", "Delete subtitle", cx),
                                        )
                                        .on_click(
                                            cx.listener(move |this, _, window, cx| {
                                                this.delete_caption(&delete_id, window, cx);
                                                cx.stop_propagation();
                                            }),
                                        ),
                                    ),
                            )
                            .when_some(edit, |s, e| {
                                s.child(
                                    div()
                                        .id(SharedString::from(format!("caption-editor-{i}")))
                                        .on_click(|_, _, cx| cx.stop_propagation())
                                        .child(e.input.clone()),
                                )
                            })
                            .when(edit.is_none(), |s| {
                                s.child(
                                    div()
                                        .id(SharedString::from(format!("caption-body-{i}")))
                                        .debug_selector(move || format!("caption-body-{i}"))
                                        .min_h(px(28.0))
                                        .cursor_text()
                                        .child(c.text.as_ref().unwrap().body.clone())
                                        .on_click(cx.listener(move |this, _, window, cx| {
                                            this.edit_caption(id.clone(), window, cx);
                                            cx.stop_propagation();
                                        })),
                                )
                            })
                    })),
            )
    }

    /// 从全局列表选中一条字幕并直接编辑正文；保持字幕页签，锁定目标只定位而不创建输入实体。
    fn edit_caption(&mut self, id: String, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        self.select_caption(id, window, cx);
        self.begin_text_body(window, cx);
    }

    /// 只删除按钮绑定的字幕身份，先提交正文，保留其他选中片段；保存失败不切换目标或丢弃模型。
    /// 成功后若删除的是主对象，定位相邻字幕；无剩余选择时保留全局列表，撤销可恢复。
    pub(crate) fn delete_caption(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        self.commit_inspector_input(cx);
        let Some(c) = self
            .timeline
            .model
            .clips
            .iter()
            .find(|c| c.id == id && c.text.as_ref().is_some_and(|t| t.group.is_some()))
        else {
            return;
        };
        if c.locked || self.timeline.model.tracks[c.track].locked {
            return;
        }
        self.stop_timeline();
        self.stop_preview();
        let primary = self.timeline.primary_clip.as_deref() == Some(id);
        let mut captions: Vec<_> = self
            .timeline
            .model
            .clips
            .iter()
            .filter(|c| c.text.as_ref().is_some_and(|t| t.group.is_some()))
            .collect();
        captions.sort_by_key(|c| (c.start, c.track));
        let index = captions.iter().position(|c| c.id == id).unwrap();
        let next = captions
            .get(index + 1)
            .copied()
            .or_else(|| index.checked_sub(1).map(|i| captions[i]))
            .map(|c| c.id.clone());
        let before = self.timeline.model.clone();
        self.timeline.model.delete(&[id.to_owned()].into());
        self.commit_timeline(before, cx);
        if primary && !self.timeline.model.clips.iter().any(|c| c.id == id) {
            self.timeline.primary_clip = None;
            if let Some(next) = next {
                self.select_caption(next, window, cx);
            }
        }
        self.timeline.inspector_tab = InspectorTab::Captions;
        self.focus_handle.focus(window);
        cx.notify();
    }
}
