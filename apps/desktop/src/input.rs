//! GPUI 单行输入：UTF-16 IME、字素移动、选择及剪贴板，使用现有依赖和原生 Canvas。

use crate::ui::{LINE, MUTED, PANEL, TEXT};
use gpui::{
    Bounds, ClipboardItem, Context, ElementInputHandler, EntityInputHandler, FocusHandle,
    IntoElement, MouseButton, Pixels, Point, Render, ShapedLine, TextRun, UTF16Selection, Window,
    canvas, div, fill, point, prelude::*, px, rgb, rgba, size,
};
use std::ops::Range;
use unicode_segmentation::UnicodeSegmentation;
#[path = "input_multiline.rs"]
mod multiline;

pub struct TextInput {
    pub(crate) text: String,
    pub(crate) focus: FocusHandle,
    anchor: usize,
    caret: usize,
    marked: Option<Range<usize>>,
    layout: Option<ShapedLine>,
    bounds: Option<Bounds<Pixels>>,
    scroll: Pixels,
    selecting: bool,
    pub(crate) placeholder: &'static str,
    pub(crate) placeholder_module: &'static str,
    pub(crate) control_id: &'static str,
    pub(crate) show_border: bool,
    pub(crate) compact: bool,
    pub(crate) multiline: bool,
    lines: Vec<(usize, ShapedLine)>,
    scroll_y: Pixels,
    reveal_caret: bool,
}

impl TextInput {
    /// 判断输入法组合是否仍在编辑，供弹窗防止候选确认 Enter 同时提交项目。
    pub(crate) fn is_composing(&self) -> bool {
        self.marked.is_some()
    }
    /// 初始化项目名称输入，默认全选已有名称；首次创建可立即覆盖“未命名项目”。
    pub fn new(text: String, cx: &mut Context<Self>) -> Self {
        cx.observe_global::<crate::i18n::Locale>(|_, cx| cx.notify())
            .detach();
        let caret = text.len();
        Self {
            text,
            focus: cx.focus_handle(),
            anchor: 0,
            caret,
            marked: None,
            layout: None,
            bounds: None,
            scroll: px(0.0),
            selecting: false,
            placeholder: "",
            placeholder_module: "common",
            control_id: "project-name",
            show_border: true,
            compact: false,
            multiline: false,
            lines: Vec::new(),
            scroll_y: px(0.0),
            reveal_caret: true,
        }
    }

    /// 返回按 UTF-8 字节计数的规范选择范围；反向选择不会产生逆序 Range。
    fn selection(&self) -> Range<usize> {
        self.anchor.min(self.caret)..self.anchor.max(self.caret)
    }

    /// 将系统 UTF-16 偏移转换为合法 UTF-8 边界，截断超出文本末尾的偏移。
    fn utf16_to_byte(&self, offset: usize) -> usize {
        let mut count = 0;
        for (index, ch) in self.text.char_indices() {
            if count + ch.len_utf16() > offset {
                return index;
            }
            count += ch.len_utf16();
        }
        self.text.len()
    }

    /// 将合法 UTF-8 字节偏移转换为系统 UTF-16 偏移，兼容汉字与代理对字符。
    fn to_utf16(&self, offset: usize) -> usize {
        self.text[..offset].encode_utf16().count()
    }

    /// 将系统选择范围转换为 UTF-8 字节范围；输入来自 GPUI 的 IME 协议。
    fn byte_range(&self, range: Range<usize>) -> Range<usize> {
        self.utf16_to_byte(range.start)..self.utf16_to_byte(range.end)
    }

    /// 替换 IME 指定范围、组合范围或当前选择；单行转为空格，多行保留换行并规范 CRLF。
    /// 返回插入范围，供组合文本和候选选择使用；光标移至插入末尾并触发重绘。
    fn replace(
        &mut self,
        range: Option<Range<usize>>,
        text: &str,
        cx: &mut Context<Self>,
    ) -> Range<usize> {
        let range = range
            .map(|r| self.byte_range(r))
            .or(self.marked.clone())
            .unwrap_or_else(|| self.selection());
        let text = if self.multiline {
            text.replace("\r\n", "\n")
                .replace('\r', "\n")
                .replace('\t', "    ")
        } else {
            text.replace(['\r', '\n', '\t'], " ")
        };
        self.text.replace_range(range.clone(), &text);
        self.caret = range.start + text.len();
        self.anchor = self.caret;
        self.marked = None;
        self.reveal_caret = true;
        cx.notify();
        range.start..self.caret
    }

    /// 处理公共编辑键及剪贴板，多行导航交给对应布局；删除以字素为单位，单行保留提交/取消键。
    fn key_down(
        &mut self,
        event: &gpui::KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let key = event.keystroke.key.as_str();
        if self.multiline && self.multiline_key(event, window, cx) {
            return;
        }
        let modifiers = event.keystroke.modifiers;
        let command = modifiers.control || modifiers.platform;
        let range = self.selection();
        let previous = self
            .text
            .grapheme_indices(true)
            .map(|(i, _)| i)
            .take_while(|i| *i < self.caret)
            .last()
            .unwrap_or(0);
        let next = self
            .text
            .grapheme_indices(true)
            .map(|(i, _)| i)
            .find(|i| *i > self.caret)
            .unwrap_or(self.text.len());
        match key {
            "a" if command => {
                self.anchor = 0;
                self.caret = self.text.len();
            }
            "c" | "x" if command => {
                if !range.is_empty() {
                    cx.write_to_clipboard(ClipboardItem::new_string(self.text[range].to_owned()));
                    if key == "x" {
                        self.replace(None, "", cx);
                    }
                }
            }
            "v" if command => {
                if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
                    self.replace(None, &text, cx);
                }
            }
            "backspace" | "delete" => {
                if self.marked.is_some() {
                    return;
                }
                if range.is_empty() {
                    self.anchor = if key == "backspace" { previous } else { next };
                }
                self.replace(None, "", cx);
            }
            "left" | "right" | "home" | "end" => {
                if self.marked.is_some() {
                    return;
                }
                self.caret = match key {
                    "home" => 0,
                    "end" => self.text.len(),
                    "left" if !modifiers.shift && !range.is_empty() => range.start,
                    "right" if !modifiers.shift && !range.is_empty() => range.end,
                    "left" => previous,
                    _ => next,
                };
                if !modifiers.shift {
                    self.anchor = self.caret;
                }
            }
            _ => return,
        }
        self.reveal_caret = true;
        cx.stop_propagation();
        cx.notify();
        self.focus.focus(window);
    }

    /// 将鼠标位置映射到已排版字符边界，考虑长名称的水平滚动。
    fn index_at(&self, position: Point<Pixels>) -> usize {
        if self.multiline {
            return self.multiline_index(position);
        }
        match (&self.layout, self.bounds) {
            (Some(line), Some(bounds)) => line
                .closest_index_for_x(position.x - bounds.left() + self.scroll)
                .min(self.text.len()),
            _ => 0,
        }
    }
}

impl EntityInputHandler for TextInput {
    /// 向系统返回实际文本范围，按 UTF-16 协议回传经边界校正的范围。
    fn text_for_range(
        &mut self,
        range: Range<usize>,
        actual: &mut Option<Range<usize>>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<String> {
        let range = self.byte_range(range);
        *actual = Some(self.to_utf16(range.start)..self.to_utf16(range.end));
        Some(self.text[range].to_owned())
    }

    /// 返回系统需要的 UTF-16 选择及方向，支持反向选择和中文候选定位。
    fn selected_text_range(
        &mut self,
        _: bool,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        let range = self.selection();
        Some(UTF16Selection {
            range: self.to_utf16(range.start)..self.to_utf16(range.end),
            reversed: self.caret < self.anchor,
        })
    }

    /// 返回当前 IME 组合范围；没有组合文本时返回 None。
    fn marked_text_range(&self, _: &mut Window, _: &mut Context<Self>) -> Option<Range<usize>> {
        self.marked
            .as_ref()
            .map(|r| self.to_utf16(r.start)..self.to_utf16(r.end))
    }

    /// 结束组合输入并重绘下划线；保留已输入的项目名称。
    fn unmark_text(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        self.marked = None;
        cx.notify();
    }

    /// 提交普通文本或 IME 候选，替换目标范围并清除组合标记。
    fn replace_text_in_range(
        &mut self,
        range: Option<Range<usize>>,
        text: &str,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.replace(range, text, cx);
    }

    /// 更新 IME 组合文本；候选选择偏移相对于插入文本，避免中文输入时错误移动到整行末尾。
    fn replace_and_mark_text_in_range(
        &mut self,
        range: Option<Range<usize>>,
        text: &str,
        selected: Option<Range<usize>>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let inserted = self.replace(range, text, cx);
        if !inserted.is_empty() {
            self.marked = Some(inserted.clone());
        }
        if let Some(selected) = selected {
            let base = self.to_utf16(inserted.start);
            self.anchor = self.utf16_to_byte(base + selected.start).min(inserted.end);
            self.caret = self.utf16_to_byte(base + selected.end).min(inserted.end);
        }
    }

    /// 为中文候选框返回所需字符的屏幕坐标，包含输入框水平滚动修正。
    fn bounds_for_range(
        &mut self,
        range: Range<usize>,
        _: Bounds<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        let range = self.byte_range(range);
        let bounds = self.bounds?;
        if self.multiline {
            return self.multiline_range_bounds(range);
        }
        let line = self.layout.as_ref()?;
        Some(Bounds::from_corners(
            point(
                bounds.left() + line.x_for_index(range.start) - self.scroll,
                bounds.top(),
            ),
            point(
                bounds.left() + line.x_for_index(range.end) - self.scroll,
                bounds.bottom(),
            ),
        ))
    }

    /// 向输入法返回鼠标位置的 UTF-16 字符偏移；框外位置不参与文本命中。
    fn character_index_for_point(
        &mut self,
        position: Point<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<usize> {
        self.bounds?.localize(&position)?;
        Some(self.to_utf16(self.index_at(position)))
    }
}

impl Render for TextInput {
    /// 绘制与现有灰阶一致的名称框、选择背景、组合下划线及光标；注册 GPUI 原生 IME 处理器。
    /// 鼠标支持点击、Shift 选择与拖选；长名称滚动到当前光标，不改变文本内容。
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.multiline {
            return self.render_multiline(cx).into_any_element();
        }
        let input = cx.entity();
        let paint_input = input.clone();
        div()
            .id(self.control_id)
            .debug_selector({
                let id = self.control_id;
                move || id.into()
            })
            .w_full()
            .h(px(if self.compact { 22.0 } else { 42.0 }))
            .px(px(if self.compact { 3.0 } else { 12.0 }))
            .py(px(if self.compact { 1.0 } else { 10.0 }))
            .rounded(px(6.0))
            .bg(rgb(PANEL))
            .when(self.show_border, |s| s.border_1().border_color(rgb(LINE)))
            .text_size(px(14.0))
            .line_height(px(20.0))
            .track_focus(&self.focus)
            .cursor_text()
            .overflow_hidden()
            .on_key_down(cx.listener(Self::key_down))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, event: &gpui::MouseDownEvent, window, cx| {
                    this.focus.focus(window);
                    this.caret = this.index_at(event.position);
                    if !event.modifiers.shift {
                        this.anchor = this.caret;
                    }
                    this.selecting = true;
                    cx.stop_propagation();
                    cx.notify();
                }),
            )
            .on_mouse_move(cx.listener(|this, event: &gpui::MouseMoveEvent, _, cx| {
                if this.selecting {
                    this.caret = this.index_at(event.position);
                    cx.notify();
                }
            }))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, _, _, _| this.selecting = false),
            )
            .on_mouse_up_out(
                MouseButton::Left,
                cx.listener(|this, _, _, _| this.selecting = false),
            )
            .child(
                canvas(
                    move |_, window, cx| {
                        let input = input.read(cx);
                        let empty = input.text.is_empty();
                        let text = if empty {
                            crate::i18n::t(input.placeholder_module, input.placeholder, cx)
                                .to_owned()
                        } else {
                            input.text.clone()
                        };
                        let style = window.text_style();
                        let run = TextRun {
                            len: text.len(),
                            font: style.font(),
                            color: rgb(if empty { MUTED } else { TEXT }).into(),
                            background_color: None,
                            underline: None,
                            strikethrough: None,
                        };
                        window.text_system().shape_line(
                            text.into(),
                            style.font_size.to_pixels(window.rem_size()),
                            &[run],
                            None,
                        )
                    },
                    move |bounds, line, window, cx| {
                        paint_input.update(cx, |input, cx| {
                            let caret_x = line.x_for_index(input.caret);
                            input.scroll = input
                                .scroll
                                .min(caret_x)
                                .max(caret_x - bounds.size.width + px(2.0))
                                .max(px(0.0));
                            let origin = point(bounds.left() - input.scroll, bounds.top());
                            let selected = input.selection();
                            if input.focus.is_focused(window) && !selected.is_empty() {
                                window.paint_quad(fill(
                                    Bounds::from_corners(
                                        point(
                                            origin.x + line.x_for_index(selected.start),
                                            bounds.top(),
                                        ),
                                        point(
                                            origin.x + line.x_for_index(selected.end),
                                            bounds.bottom(),
                                        ),
                                    ),
                                    rgba(0x68687480),
                                ));
                            }
                            if let Err(error) = line.paint(origin, px(20.0), window, cx) {
                                eprintln!("输入文字绘制失败：{error}");
                            }
                            if input.focus.is_focused(window) {
                                window.paint_quad(fill(
                                    Bounds::new(
                                        point(origin.x + caret_x, bounds.top()),
                                        size(px(1.0), bounds.size.height),
                                    ),
                                    rgb(TEXT),
                                ));
                            }
                            if let Some(marked) = &input.marked {
                                window.paint_quad(fill(
                                    Bounds::new(
                                        point(
                                            origin.x + line.x_for_index(marked.start),
                                            bounds.bottom() - px(1.0),
                                        ),
                                        size(
                                            line.x_for_index(marked.end)
                                                - line.x_for_index(marked.start),
                                            px(1.0),
                                        ),
                                    ),
                                    rgb(MUTED),
                                ));
                            }
                            window.handle_input(
                                &input.focus,
                                ElementInputHandler::new(bounds, paint_input.clone()),
                                cx,
                            );
                            input.layout = Some(line);
                            input.bounds = Some(bounds);
                        });
                    },
                )
                .w_full()
                .h(px(20.0)),
            )
            .into_any_element()
    }
}
