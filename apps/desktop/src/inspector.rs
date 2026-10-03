//! 草图基本检查器：滑块、可输入数值、微调箭头与核心变换参数绑定。
use crate::{
    input::TextInput,
    timeline_state::Gesture,
    ui::{LINE, MUTED, PANEL, TEXT},
    view::Desktop,
};
use dinuosi_core::timeline::Property;
use dinuosi_icons::Icon;
use gpui::{Context, Div, MouseButton, Window, canvas, div, prelude::*, px, rgb, svg};

impl Desktop {
    /// 保持草图的“基本”页签、三组滑块和 X/Y 并排布局；没有选择时保留控件并禁用。
    pub(crate) fn inspector(&self, width: f32, cx: &mut Context<Self>) -> Div {
        let enabled = self.timeline.model.clips.iter().any(|c| {
            self.timeline.selected.contains(&c.id) && !self.timeline.model.tracks[c.track].locked
        });
        div()
            .w(px(width))
            .h_full()
            .flex_none()
            .bg(rgb(PANEL))
            .overflow_hidden()
            .child(
                div()
                    .h(px(37.0))
                    .border_b_1()
                    .border_color(rgb(LINE))
                    .pl(px(20.0))
                    .child(
                        div()
                            .w(px(28.0))
                            .pt(px(7.0))
                            .pb(px(8.0))
                            .border_b_1()
                            .border_color(rgb(TEXT))
                            .text_size(px(12.0))
                            .child("基本"),
                    ),
            )
            .child(
                div()
                    .pt(px(38.0))
                    .px(px(20.0))
                    .opacity(if enabled { 1.0 } else { 0.4 })
                    .child(self.inspector_row("不透明度", Property::Opacity, enabled, cx))
                    .child(self.inspector_row("缩放", Property::Scale, enabled, cx))
                    .child(
                        div()
                            .h(px(36.0))
                            .flex()
                            .items_center()
                            .text_size(px(12.0))
                            .gap(px(7.0))
                            .child("坐标")
                            .child("X")
                            .child(self.property_number(Property::X, enabled, cx))
                            .child(div().w(px(9.0)))
                            .child("Y")
                            .child(self.property_number(Property::Y, enabled, cx)),
                    )
                    .child(div().h(px(16.0)))
                    .child(self.inspector_row("旋转", Property::Rotation, enabled, cx)),
            )
    }
    /// 读取首个选中片段用于检查器显示，多选时写入操作会应用于全部未锁定的选中片段。
    pub(crate) fn property_value(&self, property: Property) -> f64 {
        self.timeline
            .model
            .clips
            .iter()
            .find(|c| self.timeline.selected.contains(&c.id))
            .map_or_else(
                || dinuosi_core::timeline::Transform::default().get(property),
                |c| c.transform.get(property),
            )
    }
    /// 绘制标签、滑块及数值微调控件；每个连续拖动只提交一次核心历史。
    fn inspector_row(
        &self,
        label: &'static str,
        property: Property,
        enabled: bool,
        cx: &mut Context<Self>,
    ) -> Div {
        let value = self.property_value(property);
        let (min, max) = property.range();
        let ratio = ((value - min) / (max - min)) as f32;
        let entity = cx.entity();
        let slider = div()
            .id(gpui::SharedString::from(format!(
                "inspector-slider-{property:?}"
            )))
            .debug_selector(move || format!("inspector-slider-{property:?}"))
            .flex_1()
            .min_w(px(24.0))
            .h(px(26.0))
            .relative()
            .child(
                div()
                    .absolute()
                    .left_0()
                    .right_0()
                    .top(px(12.0))
                    .h(px(2.0))
                    .bg(rgb(MUTED)),
            )
            .child(
                div()
                    .absolute()
                    .left(gpui::relative(ratio))
                    .ml(px(-5.0))
                    .top(px(7.0))
                    .size(px(11.0))
                    .rounded(px(6.0))
                    .bg(rgb(PANEL))
                    .border_1()
                    .border_color(rgb(TEXT)),
            )
            .child(
                canvas(
                    move |bounds, window, _| {
                        window.insert_hitbox(bounds, gpui::HitboxBehavior::Normal)
                    },
                    move |bounds, hitbox, window, _cx| {
                        window.on_mouse_event(
                            move |event: &gpui::MouseDownEvent, phase, window, cx| {
                                if enabled
                                    && phase == gpui::DispatchPhase::Bubble
                                    && event.button == MouseButton::Left
                                    && hitbox.is_hovered(window)
                                {
                                    entity.update(cx, |this, cx| {
                                        this.focus_handle.focus(window);
                                        this.timeline.before = Some(this.timeline.model.clone());
                                        this.timeline.gesture =
                                            Some(Gesture::Property { property, bounds });
                                        this.timeline.preview = true;
                                        this.move_timeline_pointer(event.position, cx);
                                        cx.stop_propagation();
                                    });
                                }
                            },
                        );
                    },
                )
                .absolute()
                .inset_0(),
            );
        div()
            .h(px(37.0))
            .flex()
            .items_center()
            .gap(px(14.0))
            .text_size(px(12.0))
            .child(div().w(px(63.0)).flex_none().child(label))
            .child(slider)
            .child(self.property_number(property, enabled, cx))
    }
    /// 数字框支持直接输入、Enter/失焦提交、Esc 取消及上下箭头单步调整，不添加额外说明。
    fn property_number(&self, property: Property, enabled: bool, cx: &mut Context<Self>) -> Div {
        let active = self.timeline.input.as_ref().filter(|(p, _)| *p == property);
        let suffix = match property {
            Property::Opacity | Property::Scale => "%",
            Property::Rotation => "°",
            _ => "",
        };
        div()
            .flex()
            .items_center()
            .flex_none()
            .w(px(76.0))
            .h(px(24.0))
            .child(
                div()
                    .id(gpui::SharedString::from(format!(
                        "inspector-value-{property:?}"
                    )))
                    .debug_selector(move || format!("inspector-value-{property:?}"))
                    .w(px(59.0))
                    .h(px(24.0))
                    .border_1()
                    .border_color(rgb(LINE))
                    .bg(rgb(0x212123))
                    .overflow_hidden()
                    .text_size(px(12.0))
                    .when(active.is_none(), |s| {
                        s.child(
                            div()
                                .size_full()
                                .flex()
                                .items_center()
                                .justify_center()
                                .child(format!("{:.0}{suffix}", self.property_value(property))),
                        )
                    })
                    .when_some(active, |s, (_, input)| s.child(input.clone()))
                    .on_click(cx.listener(move |this, _, window, cx| {
                        if enabled {
                            this.begin_property_input(property, window, cx);
                        }
                    })),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .w(px(17.0))
                    .children([1.0, -1.0].into_iter().map(|delta| {
                        div()
                            .id(gpui::SharedString::from(format!(
                                "inspector-step-{property:?}-{delta}"
                            )))
                            .h(px(12.0))
                            .w_full()
                            .text_size(px(9.0))
                            .flex()
                            .items_center()
                            .justify_center()
                            .border_1()
                            .border_color(rgb(LINE))
                            .child(
                                svg()
                                    .path(if delta > 0.0 {
                                        Icon::ChevronUp.path()
                                    } else {
                                        Icon::ChevronDown.path()
                                    })
                                    .size(px(10.0))
                                    .text_color(rgb(TEXT)),
                            )
                            .on_click(cx.listener(move |this, _, _, cx| {
                                if enabled {
                                    this.change_property(
                                        property,
                                        this.property_value(property) + delta,
                                        cx,
                                    );
                                }
                            }))
                    })),
            )
    }
    /// 为当前参数创建原生输入实体，保留现有 IME、选择和剪贴板行为。
    fn begin_property_input(
        &mut self,
        property: Property,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self
            .timeline
            .input
            .as_ref()
            .is_some_and(|(p, _)| *p == property)
        {
            return;
        }
        self.commit_inspector_input(cx);
        let text = format!("{:.0}", self.property_value(property));
        let input = cx.new(|cx| {
            let mut input = TextInput::new(text, cx);
            input.show_border = false;
            input.compact = true;
            input
        });
        input.read(cx).focus.focus(window);
        let focus = input.read(cx).focus.clone();
        self.timeline.input_blur = Some(cx.on_blur(&focus, window, |this, _, cx| {
            this.commit_inspector_input(cx)
        }));
        self.timeline.input = Some((property, input));
        cx.notify();
    }
    /// 提交数值输入，非法数字显示真实错误并保持原参数；轨道名称仅修改所选轨道。
    pub(crate) fn commit_inspector_input(&mut self, cx: &mut Context<Self>) {
        self.timeline.input_blur = None;
        if let Some((property, input)) = self.timeline.input.take() {
            match input
                .read(cx)
                .text
                .trim()
                .trim_end_matches(['%', '°'])
                .parse::<f64>()
            {
                Ok(value) => self.change_property(property, value, cx),
                Err(_) => self.notice = Some("请输入有效数值".into()),
            }
        }
        if let Some((index, input)) = self.timeline.rename.take() {
            let name = input.read(cx).text.trim().to_owned();
            if !name.is_empty() && name.chars().count() <= 32 {
                let before = self.timeline.model.clone();
                self.timeline.model.tracks[index].name = name;
                self.commit_timeline(before, cx);
            } else {
                self.notice = Some("轨道名称应为 1–32 个字符".into());
            }
        }
        cx.notify();
    }
    /// 激活轨道名称输入，不改变轨道身份、顺序及素材关系。
    pub(crate) fn begin_track_name(
        &mut self,
        index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.commit_inspector_input(cx);
        let text = self.timeline.model.tracks[index].name.clone();
        let input = cx.new(|cx| {
            let mut input = TextInput::new(text, cx);
            input.compact = true;
            input
        });
        input.read(cx).focus.focus(window);
        let focus = input.read(cx).focus.clone();
        self.timeline.input_blur = Some(cx.on_blur(&focus, window, |this, _, cx| {
            this.commit_inspector_input(cx)
        }));
        self.timeline.rename = Some((index, input));
        cx.notify();
    }
    /// 切换锁定/可见/静音并提交；锁定轨道同步移除其选择，隐藏轨道重新合成预览。
    pub(crate) fn toggle_track(&mut self, index: usize, field: u8, cx: &mut Context<Self>) {
        self.stop_timeline();
        let before = self.timeline.model.clone();
        let track = &mut self.timeline.model.tracks[index];
        match field {
            0 => track.locked = !track.locked,
            1 => track.visible = !track.visible,
            _ => track.muted = !track.muted,
        }
        self.timeline.selected.retain(|id| {
            self.timeline
                .model
                .clips
                .iter()
                .any(|c| &c.id == id && !self.timeline.model.tracks[c.track].locked)
        });
        self.commit_timeline(before, cx);
    }
}
