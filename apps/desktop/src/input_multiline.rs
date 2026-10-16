//! TextInput 的多行绘制与命中；沿用同一 UTF-16 IME、字素选择和剪贴板实现。
use super::*;

impl TextInput {
    /// 多行输入消费 Enter，并按视觉行处理上下/Home/End；候选确认交给 IME，不生成多余换行。
    pub(super) fn multiline_key(
        &mut self,
        event: &gpui::KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let key = event.keystroke.key.as_str();
        if !matches!(key, "enter" | "up" | "down" | "home" | "end") {
            return false;
        }
        if self.marked.is_some() {
            cx.stop_propagation();
            return true;
        }
        if key == "enter" {
            self.replace(None, "\n", cx);
        } else {
            let row = self
                .lines
                .iter()
                .rposition(|(start, _)| *start <= self.caret)
                .unwrap_or(0);
            if let Some((start, line)) = self.lines.get(row) {
                let x = line.x_for_index(self.caret.saturating_sub(*start).min(line.len()));
                let target = if key == "up" {
                    row.saturating_sub(1)
                } else {
                    (row + 1).min(self.lines.len().saturating_sub(1))
                };
                self.caret = if key == "home" {
                    *start
                } else if key == "end" {
                    start + line.len()
                } else {
                    let (start, line) = &self.lines[target];
                    start + line.closest_index_for_x(x)
                };
                if !event.keystroke.modifiers.shift {
                    self.anchor = self.caret;
                }
            }
        }
        self.reveal_caret = true;
        cx.stop_propagation();
        cx.notify();
        self.focus.focus(window);
        true
    }

    /// 将窗口坐标转换为视觉行与字素边界，考虑正文框内部的垂直滚动；空布局返回零。
    pub(super) fn multiline_index(&self, position: Point<Pixels>) -> usize {
        let Some(bounds) = self.bounds else {
            return 0;
        };
        let row = ((f32::from(position.y - bounds.top() + self.scroll_y) / 20.0).max(0.0) as usize)
            .min(self.lines.len().saturating_sub(1));
        self.lines.get(row).map_or(0, |(start, line)| {
            start + line.closest_index_for_x(position.x - bounds.left())
        })
    }

    /// 返回 IME 候选窗口的插入位置；跨行范围使用首行剩余部分，避免候选框跳到编辑器外。
    pub(super) fn multiline_range_bounds(&self, range: Range<usize>) -> Option<Bounds<Pixels>> {
        let bounds = self.bounds?;
        let row = self
            .lines
            .iter()
            .rposition(|(start, _)| *start <= range.start)?;
        let (start, line) = &self.lines[row];
        let left = line.x_for_index(range.start.saturating_sub(*start).min(line.len()));
        let right = line
            .x_for_index(range.end.saturating_sub(*start).min(line.len()))
            .max(left + px(1.0));
        Some(Bounds::new(
            point(
                bounds.left() + left,
                bounds.top() + px(row as f32 * 20.0) - self.scroll_y,
            ),
            size(right - left, px(20.0)),
        ))
    }

    /// 按正文框宽度折行，所有偏移仍使用完整 UTF-8 字节位置；显式空行保留可编辑光标。
    fn shape_multiline(&self, width: Pixels, window: &mut Window) -> Vec<(usize, ShapedLine)> {
        let style = window.text_style();
        let font_size = style.font_size.to_pixels(window.rem_size());
        let shape = |text: &str| {
            window.text_system().shape_line(
                text.to_owned().into(),
                font_size,
                &[TextRun {
                    len: text.len(),
                    font: style.font(),
                    color: rgb(TEXT).into(),
                    background_color: None,
                    underline: None,
                    strikethrough: None,
                }],
                None,
            )
        };
        let mut lines = Vec::new();
        let mut offset = 0;
        for paragraph in self.text.split('\n') {
            let mut rest = paragraph;
            loop {
                let mut line = shape(rest);
                if line.width <= width || rest.is_empty() {
                    lines.push((offset, line));
                    break;
                }
                let boundary = line.closest_index_for_x(width);
                let cut = rest
                    .grapheme_indices(true)
                    .map(|(i, _)| i)
                    .rfind(|i| *i > 0 && *i <= boundary)
                    .unwrap_or_else(|| rest.graphemes(true).next().unwrap().len());
                line = shape(&rest[..cut]);
                lines.push((offset, line));
                offset += cut;
                rest = &rest[cut..];
            }
            offset += rest.len() + 1;
        }
        lines
    }

    /// 绘制可折行正文、跨行选择、组合下划线和光标；内容在固定灰阶框内裁切并跟随光标滚动。
    /// 鼠标拖选、滚轮、IME 和剪贴板真实接入，不通过显示换行符伪装多行编辑。
    pub(super) fn render_multiline(&mut self, cx: &mut Context<Self>) -> gpui::Stateful<gpui::Div> {
        let entity = cx.entity();
        let paint = entity.clone();
        div()
            .id(self.control_id)
            .debug_selector({
                let id = self.control_id;
                move || id.into()
            })
            .w_full()
            .h(px(116.0))
            .p(px(8.0))
            .rounded(px(4.0))
            .bg(rgb(PANEL))
            .border_1()
            .border_color(rgb(LINE))
            .text_size(px(14.0))
            .line_height(px(20.0))
            .overflow_hidden()
            .cursor_text()
            .track_focus(&self.focus)
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
                    this.reveal_caret = true;
                    cx.stop_propagation();
                    cx.notify();
                }),
            )
            .on_mouse_move(cx.listener(|this, event: &gpui::MouseMoveEvent, _, cx| {
                if this.selecting {
                    this.caret = this.index_at(event.position);
                    this.reveal_caret = true;
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
            .on_scroll_wheel(cx.listener(|this, event: &gpui::ScrollWheelEvent, _, cx| {
                let delta = event.delta.pixel_delta(px(20.0)).y;
                this.scroll_y = (this.scroll_y - delta).clamp(
                    px(0.0),
                    px((this.lines.len() as f32 * 20.0 - 96.0).max(0.0)),
                );
                cx.stop_propagation();
                cx.notify();
            }))
            .child(
                canvas(
                    move |bounds, window, cx| {
                        entity.read(cx).shape_multiline(bounds.size.width, window)
                    },
                    move |bounds, lines, window, cx| {
                        paint.update(cx, |this, cx| {
                            let row = lines
                                .iter()
                                .rposition(|(start, _)| *start <= this.caret)
                                .unwrap_or(0);
                            if this.focus.is_focused(window) && this.reveal_caret {
                                this.scroll_y = this
                                    .scroll_y
                                    .min(px(row as f32 * 20.0))
                                    .max(px((row + 1) as f32 * 20.0) - bounds.size.height)
                                    .max(px(0.0));
                            }
                            this.reveal_caret = false;
                            let selected = this.selection();
                            window.with_content_mask(
                                Some(gpui::ContentMask { bounds }),
                                |window| {
                                    for (i, (start, line)) in lines.iter().enumerate() {
                                        let origin = point(
                                            bounds.left(),
                                            bounds.top() + px(i as f32 * 20.0) - this.scroll_y,
                                        );
                                        if origin.y + px(20.0) < bounds.top()
                                            || origin.y > bounds.bottom()
                                        {
                                            continue;
                                        }
                                        if this.focus.is_focused(window)
                                            && selected.start < start + line.len() + 1
                                            && selected.end > *start
                                        {
                                            let left = line.x_for_index(
                                                selected
                                                    .start
                                                    .saturating_sub(*start)
                                                    .min(line.len()),
                                            );
                                            let right = line
                                                .x_for_index(
                                                    selected
                                                        .end
                                                        .saturating_sub(*start)
                                                        .min(line.len()),
                                                )
                                                .max(left + px(2.0));
                                            window.paint_quad(fill(
                                                Bounds::new(
                                                    origin + point(left, px(0.0)),
                                                    size(right - left, px(20.0)),
                                                ),
                                                rgba(0x68687480),
                                            ));
                                        }
                                        if let Err(error) = line.paint(origin, px(20.0), window, cx)
                                        {
                                            eprintln!("正文绘制失败：{error}");
                                        }
                                        if this.focus.is_focused(window) && row == i {
                                            window.paint_quad(fill(
                                                Bounds::new(
                                                    origin
                                                        + point(
                                                            line.x_for_index(
                                                                this.caret
                                                                    .saturating_sub(*start)
                                                                    .min(line.len()),
                                                            ),
                                                            px(0.0),
                                                        ),
                                                    size(px(1.0), px(20.0)),
                                                ),
                                                rgb(TEXT),
                                            ));
                                        }
                                        if let Some(marked) = &this.marked {
                                            let left = line.x_for_index(
                                                marked.start.saturating_sub(*start).min(line.len()),
                                            );
                                            let right = line.x_for_index(
                                                marked.end.saturating_sub(*start).min(line.len()),
                                            );
                                            if right > left {
                                                window.paint_quad(fill(
                                                    Bounds::new(
                                                        origin + point(left, px(19.0)),
                                                        size(right - left, px(1.0)),
                                                    ),
                                                    rgb(MUTED),
                                                ));
                                            }
                                        }
                                    }
                                },
                            );
                            window.handle_input(
                                &this.focus,
                                ElementInputHandler::new(bounds, paint.clone()),
                                cx,
                            );
                            this.lines = lines;
                            this.bounds = Some(bounds);
                        });
                    },
                )
                .size_full(),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    /// 聚焦中的长正文允许滚轮离开插入点阅读，下一次键盘输入才重新显示光标，防止每帧滚回。
    #[gpui::test]
    fn multiline_wheel_can_read_away_from_caret(cx: &mut gpui::TestAppContext) {
        let (input, cx) = cx.add_window_view(|_, cx| {
            let mut i = TextInput::new(
                (0..40)
                    .map(|n| format!("第 {n} 行文字"))
                    .collect::<Vec<_>>()
                    .join("\n"),
                cx,
            );
            i.multiline = true;
            i
        });
        input.update_in(cx, |i, window, _| i.focus.focus(window));
        cx.simulate_keystrokes("end");
        cx.run_until_parked();
        let (before, center) = input.read_with(cx, |i, _| (i.scroll_y, i.bounds.unwrap().center()));
        assert!(before > px(300.0));
        cx.simulate_event(gpui::ScrollWheelEvent {
            position: center,
            delta: gpui::ScrollDelta::Pixels(point(px(0.0), px(200.0))),
            ..Default::default()
        });
        cx.run_until_parked();
        let reading = input.read_with(cx, |i, _| i.scroll_y);
        assert!(reading < before);
        cx.run_until_parked();
        assert_eq!(input.read_with(cx, |i, _| i.scroll_y), reading);
        cx.simulate_keystrokes("end");
        cx.simulate_input("尾");
        cx.run_until_parked();
        assert!(input.read_with(cx, |i, _| i.scroll_y) > reading);
    }
}
