//! 用户命名文字样式：原子保存、真实样式缩略图和跨项目复用，不保存正文或时间。
use crate::{
    input::TextInput,
    ui::{LINE, MUTED, TEXT},
    view::Desktop,
};
use dinuosi_core::text::{Text, TextPreset, TextStyle};
use gpui::{
    Context, Div, Entity, ObjectFit, RenderImage, SharedString, Window, div, img, prelude::*, px,
    rgb,
};
use std::sync::Arc;

pub(crate) struct PresetItem {
    pub preset: TextPreset,
    pub image: Arc<RenderImage>,
}
pub(crate) struct PresetDraft {
    pub input: Entity<TextInput>,
    pub style: TextStyle,
}

impl Desktop {
    /// 首次打开文本分类时读取用户样式库；损坏时只显示真实错误，不用空库覆盖用户数据。
    pub(crate) fn load_text_presets(&mut self, cx: &mut Context<Self>) {
        if self.timeline.text.presets_loaded {
            return;
        }
        match self.store.load_text_presets().and_then(previews) {
            Ok(items) => {
                self.timeline.text.presets = items;
                self.timeline.text.presets_loaded = true;
            }
            Err(error) => self.notice = Some(error),
        }
        cx.notify();
    }
    /// 捕获当前完整样式并显示命名输入；之后切换片段不会把其他片段的样式误存到这个草稿。
    pub(crate) fn begin_text_preset(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        self.commit_inspector_input(cx);
        let Some(style) = self
            .text_clip()
            .and_then(|c| c.text.as_ref())
            .map(|t| t.style.clone())
        else {
            return;
        };
        self.load_text_presets(cx);
        let name = format!(
            "{} {}",
            crate::i18n::t("text", "Style", cx),
            self.timeline.text.presets.len() + 1
        );
        let input = cx.new(|cx| {
            let mut i = TextInput::new(name, cx);
            i.compact = true;
            i.control_id = "text-preset-name";
            i
        });
        input.read(cx).focus.focus(window);
        self.timeline.text.preset_draft = Some(PresetDraft { input, style });
        cx.notify();
    }
    /// 将命名草稿写入用户样式库；保存成功才发布新列表，失败保留草稿以便修正，不更改项目正文。
    pub(crate) fn save_text_preset(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(draft) = &self.timeline.text.preset_draft else {
            return;
        };
        if draft.input.read(cx).is_composing() {
            return;
        }
        let name = draft.input.read(cx).text.clone();
        match self
            .store
            .save_text_preset(&name, draft.style.clone())
            .and_then(previews)
        {
            Ok(items) => {
                self.timeline.text.presets = items;
                self.timeline.text.presets_loaded = true;
                self.timeline.text.preset_draft = None;
                self.focus_handle.focus(window);
            }
            Err(error) => self.notice = Some(error),
        }
        cx.notify();
    }
    /// 对选中文字应用完整样式，保留正文、位置和时间；无文字选择时直接创建带此样式的普通文本。
    pub(crate) fn use_text_preset(
        &mut self,
        id: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(style) = self
            .timeline
            .text
            .presets
            .iter()
            .find(|p| p.preset.id == id)
            .map(|p| p.preset.style.clone())
        else {
            return;
        };
        if self.text_clip().is_some() {
            self.change_text_style(|s| *s = style, cx);
        } else {
            self.add_text_clip(false, Some(style), window, cx);
        }
    }
    /// 删除库中的独立样式，已应用片段继续保存各自副本；失败不更新显示列表。
    pub(crate) fn remove_text_preset(&mut self, id: &str, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        match self.store.remove_text_preset(id).and_then(previews) {
            Ok(items) => self.timeline.text.presets = items,
            Err(error) => self.notice = Some(error),
        }
        cx.notify();
    }
    /// 在检查器提供单一“保存样式”入口；命名时显示输入及保存/取消，不新增说明性文案。
    pub(crate) fn text_preset_save(&self, enabled: bool, cx: &mut Context<Self>) -> Div {
        div()
            .mt(px(4.0))
            .flex()
            .flex_col()
            .items_end()
            .when_some(self.timeline.text.preset_draft.as_ref(), |s, draft| {
                s.child(draft.input.clone()).child(
                    div()
                        .mt(px(6.0))
                        .flex()
                        .gap(px(10.0))
                        .child(
                            div()
                                .id("text-preset-save-confirm")
                                .cursor_pointer()
                                .child(crate::i18n::t("text", "Save", cx))
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.save_text_preset(window, cx)
                                })),
                        )
                        .child(
                            div()
                                .id("text-preset-save-cancel")
                                .cursor_pointer()
                                .text_color(rgb(MUTED))
                                .child(crate::i18n::t("text", "Cancel", cx))
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.timeline.text.preset_draft = None;
                                    this.focus_handle.focus(window);
                                    cx.notify();
                                })),
                        ),
                )
            })
            .when(self.timeline.text.preset_draft.is_none(), |s| {
                s.child(
                    crate::ui::panel_button("text-save-style", false, enabled)
                        .focusable()
                        .tab_index(0)
                        .border_1()
                        .border_color(rgb(LINE))
                        .child(crate::i18n::t("text", "Save style", cx))
                        .on_click(cx.listener(move |this, _, window, cx| {
                            if enabled {
                                this.begin_text_preset(window, cx);
                            }
                        }))
                        .on_key_down(cx.listener(
                            move |this, e: &gpui::KeyDownEvent, window, cx| {
                                if enabled
                                    && !e.is_held
                                    && matches!(e.keystroke.key.as_str(), "enter" | "space")
                                {
                                    this.begin_text_preset(window, cx);
                                    cx.stop_propagation();
                                }
                            },
                        )),
                )
            })
    }
    /// 在左侧文本区绘制真实字体/效果缩略图和用户名称，点击取用，删除按钮不触发应用。
    pub(crate) fn text_preset_library(&self, cx: &mut Context<Self>) -> Div {
        div()
            .mt(px(18.0))
            .when(!self.timeline.text.presets.is_empty(), |s| {
                s.child(div().mb(px(10.0)).text_size(px(12.0)).child(crate::i18n::t(
                    "text",
                    "My styles",
                    cx,
                )))
            })
            .child(div().flex().flex_wrap().gap(px(12.0)).children(
                self.timeline.text.presets.iter().map(|p| {
                    let id = p.preset.id.clone();
                    let remove = id.clone();
                    let keyboard = id.clone();
                    div()
                        .id(SharedString::from(format!("text-preset-{id}")))
                        .debug_selector({
                            let id = id.clone();
                            move || format!("text-preset-{id}")
                        })
                        .focusable()
                        .tab_index(0)
                        .w(px(130.0))
                        .border_1()
                        .border_color(rgb(LINE))
                        .rounded(px(4.0))
                        .cursor_pointer()
                        .focus(|s| s.border_color(rgb(TEXT)))
                        .child(
                            div()
                                .h(px(76.0))
                                .bg(rgb(if p.preset.style.color < 0x666666 {
                                    0xcccccc
                                } else {
                                    0x242426
                                }))
                                .child(
                                    img(p.image.clone())
                                        .size_full()
                                        .object_fit(ObjectFit::Contain),
                                ),
                        )
                        .child(
                            div()
                                .h(px(32.0))
                                .flex()
                                .items_center()
                                .gap(px(4.0))
                                .px(px(6.0))
                                .child(
                                    div()
                                        .flex_1()
                                        .min_w_0()
                                        .text_size(px(12.0))
                                        .text_ellipsis()
                                        .child(p.preset.name.clone()),
                                )
                                .child(
                                    crate::ui::tool(
                                        SharedString::from(format!("remove-style-{remove}")),
                                        dinuosi_icons::Icon::Trash,
                                        true,
                                    )
                                    .on_click(cx.listener(
                                        move |this, _, _, cx| {
                                            cx.stop_propagation();
                                            this.remove_text_preset(&remove, cx);
                                        },
                                    )),
                                ),
                        )
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.use_text_preset(&id, window, cx)
                        }))
                        .on_key_down(cx.listener(
                            move |this, e: &gpui::KeyDownEvent, window, cx| {
                                if !e.is_held
                                    && matches!(e.keystroke.key.as_str(), "enter" | "space")
                                {
                                    this.use_text_preset(&keyboard, window, cx);
                                    cx.stop_propagation();
                                }
                            },
                        ))
                }),
            ))
    }
}

/// 用统一渲染器生成小尺寸样式样张，仅归一化字号和宽度；避免每次界面重绘重新排版整套预设。
fn previews(presets: Vec<TextPreset>) -> Result<Vec<PresetItem>, String> {
    presets
        .into_iter()
        .map(|preset| {
            let mut text = Text::new("Aa 文字".into(), (260, 150), false);
            text.style = preset.style.clone();
            text.style.size = 32.0;
            text.style.width = 220.0;
            let raster = dinuosi_core::text_render::raster(&text, (260, 150))?;
            let buffer =
                image::RgbaImage::from_raw(raster.size.0, raster.size.1, raster.pixels.clone())
                    .ok_or("样式缩略图尺寸无效")?;
            Ok(PresetItem {
                preset,
                image: Arc::new(RenderImage::new([image::Frame::new(buffer)])),
            })
        })
        .collect()
}
