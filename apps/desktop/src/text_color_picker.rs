//! 文字、描边、投影和底色共用的 HSV 调色器；拖动实时预览，关闭保存一次，Escape 恢复原色。
use crate::{
    text_edit::TextField,
    ui::{LINE, PANEL, TEXT},
    view::Desktop,
};
use dinuosi_core::timeline::Timeline;
use gpui::{
    Bounds, Context, Div, MouseButton, Pixels, Point, Window, anchored, canvas, div, fill, point,
    prelude::*, px, rgb, size,
};

pub(crate) struct TextColorPicker {
    id: String,
    field: TextField,
    hsv: [f32; 3],
    before: Timeline,
    origin: Point<Pixels>,
    drag: Option<(bool, Bounds<Pixels>)>,
}

/// 将合法 RGB 转成色相、饱和度和明度；灰色没有确定色相时以红色轴作为初始位置。
fn to_hsv(color: u32) -> [f32; 3] {
    let [r, g, b] = [
        (color >> 16 & 255) as f32 / 255.0,
        (color >> 8 & 255) as f32 / 255.0,
        (color & 255) as f32 / 255.0,
    ];
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let d = max - min;
    let h = if d == 0.0 {
        0.0
    } else if max == r {
        ((g - b) / d).rem_euclid(6.0)
    } else if max == g {
        (b - r) / d + 2.0
    } else {
        (r - g) / d + 4.0
    };
    [h / 6.0, if max == 0.0 { 0.0 } else { d / max }, max]
}

/// 将单位 HSV 转回 RGB；色相首尾相接，各通道四舍五入到项目使用的八位精度。
fn to_rgb([h, s, v]: [f32; 3]) -> u32 {
    let h = h.rem_euclid(1.0) * 6.0;
    let c = v * s;
    let x = c * (1.0 - (h % 2.0 - 1.0).abs());
    let m = v - c;
    let [r, g, b] = match h as u32 {
        0 => [c, x, 0.0],
        1 => [x, c, 0.0],
        2 => [0.0, c, x],
        3 => [0.0, x, c],
        4 => [x, 0.0, c],
        _ => [c, 0.0, x],
    };
    let byte = |n: f32| ((n + m).clamp(0.0, 1.0) * 255.0).round() as u32;
    byte(r) << 16 | byte(g) << 8 | byte(b)
}

impl Desktop {
    /// 捕获颜色字段的片段身份、原模型和屏幕位置；先提交其他输入，锁定或已关闭的效果不能打开。
    pub(crate) fn begin_text_color(
        &mut self,
        field: TextField,
        origin: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.busy {
            return;
        }
        self.commit_inspector_input(cx);
        let Some(clip) = self.text_clip() else {
            return;
        };
        if clip.locked || self.timeline.model.tracks[clip.track].locked {
            return;
        }
        let Some(color) = field.color(&clip.text.as_ref().unwrap().style) else {
            return;
        };
        self.timeline.text.picker = Some(TextColorPicker {
            id: clip.id.clone(),
            field,
            hsv: to_hsv(color),
            before: self.timeline.model.clone(),
            origin,
            drag: None,
        });
        self.stop_timeline();
        self.stop_preview();
        self.focus_handle.focus(window);
        cx.notify();
    }

    /// 根据冻结的拖动区域更新颜色，指针越界时夹到合法端点；只修改绑定片段及既定字幕样式范围。
    /// 每次移动仅刷新预览，校验失败保留原模型并显示错误，保存和历史留给关闭操作统一完成。
    pub(crate) fn move_text_color(&mut self, position: Point<Pixels>, cx: &mut Context<Self>) {
        let Some(picker) = &mut self.timeline.text.picker else {
            return;
        };
        let Some((hue, bounds)) = picker.drag else {
            return;
        };
        let x = f32::from(position.x - bounds.left()) / f32::from(bounds.size.width).max(1.0);
        let y = f32::from(position.y - bounds.top()) / f32::from(bounds.size.height).max(1.0);
        if hue {
            picker.hsv[0] = x.clamp(0.0, 1.0);
        } else {
            picker.hsv[1] = x.clamp(0.0, 1.0);
            picker.hsv[2] = 1.0 - y.clamp(0.0, 1.0);
        }
        let (id, field, color) = (picker.id.clone(), picker.field, to_rgb(picker.hsv));
        match self.apply_text_field(&id, field, &format!("{color:06X}")) {
            Ok(()) => {
                self.timeline.preview = true;
                self.refresh_timeline(cx);
            }
            Err(error) => {
                self.notice = Some(error);
                cx.notify();
            }
        }
    }

    /// 鼠标释放后停止取色，调色器保持打开；再次拖动可继续调整，同一次打开只产生一条历史。
    pub(crate) fn end_text_color_drag(&mut self) {
        if let Some(picker) = &mut self.timeline.text.picker {
            picker.drag = None;
        }
    }

    /// 取出草稿后提交一次保存或恢复原模型，避免重入；保存失败由公共事务回滚，不留下临时颜色。
    pub(crate) fn finish_text_color(&mut self, cancel: bool, cx: &mut Context<Self>) {
        let Some(picker) = self.timeline.text.picker.take() else {
            return;
        };
        if cancel {
            self.timeline.model = picker.before;
            self.refresh_timeline(cx);
        } else {
            self.commit_timeline(picker.before, cx);
        }
        cx.notify();
    }

    /// 在字段附近绘制独立浮层，窗口边缘自动避让；外部点击提交、Escape 取消，不添加说明文案。
    pub(crate) fn text_color_popover(&self, cx: &mut Context<Self>) -> Div {
        let picker = self.timeline.text.picker.as_ref().unwrap();
        div()
            .absolute()
            .inset_0()
            .child(
                div()
                    .id("text-color-dismiss")
                    .absolute()
                    .inset_0()
                    .occlude()
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, _, window, cx| {
                            this.finish_text_color(false, cx);
                            this.focus_handle.focus(window);
                            cx.stop_propagation();
                        }),
                    ),
            )
            .child(
                anchored().position(picker.origin).child(
                    div()
                        .id("text-color-popover")
                        .debug_selector(|| "text-color-popover".into())
                        .w(px(224.0))
                        .p(px(10.0))
                        .bg(rgb(PANEL))
                        .border_1()
                        .border_color(rgb(LINE))
                        .rounded(px(4.0))
                        .occlude()
                        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                        .child(self.text_color_surface(false, cx))
                        .child(div().h(px(10.0)))
                        .child(self.text_color_surface(true, cx))
                        .child(
                            div()
                                .mt(px(8.0))
                                .flex()
                                .items_center()
                                .gap(px(8.0))
                                .child(
                                    div()
                                        .size(px(18.0))
                                        .bg(rgb(to_rgb(picker.hsv)))
                                        .border_1()
                                        .border_color(rgb(LINE)),
                                )
                                .child(format!("#{:06X}", to_rgb(picker.hsv))),
                        ),
                ),
            )
    }

    /// 绘制饱和度/明度平面或色相条，以本轮真实布局冻结拖动区域。
    /// 窗口级指针监听保持越界拖动和释放可用，不依赖被浮层遮挡的工作区根节点命中。
    fn text_color_surface(&self, hue: bool, cx: &mut Context<Self>) -> gpui::Stateful<Div> {
        let hsv = self.timeline.text.picker.as_ref().unwrap().hsv;
        let entity = cx.entity();
        let selector = if hue {
            "text-color-hue"
        } else {
            "text-color-plane"
        };
        div()
            .id(selector)
            .debug_selector(move || selector.into())
            .w_full()
            .h(px(if hue { 16.0 } else { 156.0 }))
            .cursor_crosshair()
            .child(
                canvas(
                    move |bounds, window, _| {
                        window.insert_hitbox(bounds, gpui::HitboxBehavior::Normal)
                    },
                    move |bounds, hitbox, window, _| {
                        let gradient = |a: gpui::Hsla, b: gpui::Hsla, angle| {
                            gpui::linear_gradient(
                                angle,
                                gpui::linear_color_stop(a, 0.0),
                                gpui::linear_color_stop(b, 1.0),
                            )
                            .color_space(gpui::ColorSpace::Srgb)
                        };
                        if hue {
                            let w = bounds.size.width / 6.0;
                            for i in 0..6 {
                                window.paint_quad(fill(
                                    Bounds::new(
                                        bounds.origin + point(w * i as f32, px(0.0)),
                                        size(w + px(0.5), bounds.size.height),
                                    ),
                                    gradient(
                                        rgb(to_rgb([i as f32 / 6.0, 1.0, 1.0])).into(),
                                        rgb(to_rgb([(i + 1) as f32 / 6.0, 1.0, 1.0])).into(),
                                        90.0,
                                    ),
                                ));
                            }
                        } else {
                            window.paint_quad(fill(
                                bounds,
                                gradient(
                                    rgb(0xffffff).into(),
                                    rgb(to_rgb([hsv[0], 1.0, 1.0])).into(),
                                    90.0,
                                ),
                            ));
                            window.paint_quad(fill(
                                bounds,
                                gradient(
                                    gpui::rgba(0x00000000).into(),
                                    rgb(0x000000).into(),
                                    180.0,
                                ),
                            ));
                        }
                        let x =
                            bounds.left() + bounds.size.width * if hue { hsv[0] } else { hsv[1] };
                        let y = if hue {
                            bounds.center().y
                        } else {
                            bounds.top() + bounds.size.height * (1.0 - hsv[2])
                        };
                        window.paint_quad(fill(
                            Bounds::new(point(x - px(3.0), y - px(3.0)), size(px(6.0), px(6.0))),
                            rgb(0x000000),
                        ));
                        window.paint_quad(fill(
                            Bounds::new(point(x - px(2.0), y - px(2.0)), size(px(4.0), px(4.0))),
                            rgb(TEXT),
                        ));
                        let move_entity = entity.clone();
                        window.on_mouse_event(move |event: &gpui::MouseMoveEvent, phase, _, cx| {
                            if phase == gpui::DispatchPhase::Bubble {
                                move_entity.update(cx, |this, cx| {
                                    if this.timeline.text.picker.as_ref().is_some_and(|p| {
                                        p.drag.is_some_and(|(active, _)| active == hue)
                                    }) {
                                        if event.pressed_button == Some(MouseButton::Left) {
                                            this.move_text_color(event.position, cx);
                                        } else {
                                            this.end_text_color_drag();
                                        }
                                        cx.stop_propagation();
                                    }
                                });
                            }
                        });
                        let up_entity = entity.clone();
                        window.on_mouse_event(move |event: &gpui::MouseUpEvent, phase, _, cx| {
                            if phase == gpui::DispatchPhase::Bubble
                                && event.button == MouseButton::Left
                            {
                                up_entity.update(cx, |this, _| this.end_text_color_drag());
                            }
                        });
                        let entity = entity.clone();
                        window.on_mouse_event(
                            move |event: &gpui::MouseDownEvent, phase, window, cx| {
                                if phase == gpui::DispatchPhase::Bubble
                                    && event.button == MouseButton::Left
                                    && hitbox.is_hovered(window)
                                {
                                    entity.update(cx, |this, cx| {
                                        if let Some(picker) = &mut this.timeline.text.picker {
                                            picker.drag = Some((hue, bounds));
                                        }
                                        this.move_text_color(event.position, cx);
                                        cx.stop_propagation();
                                    });
                                }
                            },
                        );
                    },
                )
                .size_full(),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    /// RGB 往返在项目八位精度内保持不变，覆盖灰阶、主色与非整色色相，首尾色相都应为红色。
    #[test]
    fn hsv_rgb_roundtrip_preserves_colors() {
        for r in [0, 1, 127, 254, 255] {
            for g in [0, 1, 127, 254, 255] {
                for b in [0, 1, 127, 254, 255] {
                    let color = r << 16 | g << 8 | b;
                    assert_eq!(to_rgb(to_hsv(color)), color);
                }
            }
        }
        assert_eq!(to_rgb([0.0, 1.0, 1.0]), 0xff0000);
        assert_eq!(to_rgb([1.0, 1.0, 1.0]), 0xff0000);
    }
}
