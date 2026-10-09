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
    /// 保持草图的“基本”页签、图片、视频的变换布局；选中音频仅显示音量滑块及右侧 dB 值。
    /// 空选择显示项目概览，另一页签显示排序标记；锁定片段保留参数展示但禁用全部写入控件。
    pub(crate) fn inspector(&self, width: f32, cx: &mut Context<Self>) -> Div {
        let audio = self
            .timeline
            .model
            .clips
            .iter()
            .find(|c| self.timeline.selected.contains(&c.id))
            .is_some_and(|c| c.kind == dinuosi_core::media::MediaKind::Audio);
        let enabled = self.timeline.model.clips.iter().any(|c| {
            self.timeline.selected.contains(&c.id)
                && (c.kind == dinuosi_core::media::MediaKind::Audio) == audio
                && !c.locked
                && !self.timeline.model.tracks[c.track].locked
        });
        div()
            .w(px(width))
            .h_full()
            .flex_none()
            .bg(rgb(PANEL))
            .overflow_hidden()
            .flex()
            .flex_col()
            .child(self.inspector_tabs(self.timeline.selected.is_empty(), cx))
            .child(if self.timeline.markers_tab {
                self.markers_inspector(cx)
            } else if self.timeline.selected.is_empty() {
                self.project_inspector(cx)
            } else if audio {
                self.audio_inspector(enabled, cx)
            } else {
                div()
                    .pt(px(38.0))
                    .px(px(20.0))
                    .opacity(if enabled { 1.0 } else { 0.4 })
                    .child(self.inspector_row(
                        crate::i18n::t("inspector", "Opacity", cx),
                        Property::Opacity,
                        enabled,
                        cx,
                    ))
                    .child(self.inspector_row(
                        crate::i18n::t("inspector", "Scale", cx),
                        Property::Scale,
                        enabled,
                        cx,
                    ))
                    .child(self.position_row(width, enabled, cx))
                    .child(div().h(px(16.0)))
                    .child(self.inspector_row(
                        crate::i18n::t("inspector", "Rotation", cx),
                        Property::Rotation,
                        enabled,
                        cx,
                    ))
            })
    }
    /// 为坐标标签及 X/Y 分别保留独立布局和完整数值框，避免相邻文字被合并、挤压。
    /// 宽检查器横向排列，窄于 360px 时仅将 X/Y 成组竖排；坐标共用一个右侧关键帧控件。
    fn position_row(&self, width: f32, enabled: bool, cx: &mut Context<Self>) -> Div {
        let narrow = width < 360.0;
        div()
            .debug_selector(|| "inspector-position-row".into())
            .h(px(if narrow { 60.0 } else { 37.0 }))
            .flex()
            .items_center()
            .gap(px(6.0))
            .text_size(px(12.0))
            .child(
                div()
                    .debug_selector(|| "inspector-position-label".into())
                    .w(px(63.0))
                    .flex_none()
                    .child(crate::i18n::t("inspector", "Position", cx)),
            )
            .child(
                div()
                    .flex()
                    .flex_1()
                    .min_w_0()
                    .gap(px(6.0))
                    .when(narrow, |s| s.flex_col())
                    .children([(Property::X, "X"), (Property::Y, "Y")].into_iter().map(
                        |(property, label)| {
                            div()
                                .flex()
                                .flex_none()
                                .items_center()
                                .gap(px(6.0))
                                .child(
                                    div()
                                        .debug_selector(move || {
                                            format!("inspector-coordinate-{label}-label")
                                        })
                                        .w(px(10.0))
                                        .flex_none()
                                        .child(label),
                                )
                                .child(self.property_number(property, enabled, cx))
                        },
                    )),
            )
            .child(self.keyframe_controls(dinuosi_core::keyframes::KeyframeKind::Position, cx))
    }
    /// 按播放头帧读取首个选中画面片段的插值参数；多选写入仅应用于未锁定图片、视频。
    pub(crate) fn property_value(&self, property: Property) -> f64 {
        self.timeline
            .model
            .clips
            .iter()
            .find(|c| {
                c.kind != dinuosi_core::media::MediaKind::Audio
                    && self.timeline.selected.contains(&c.id)
            })
            .map_or_else(
                || dinuosi_core::timeline::Transform::default().get(property),
                |c| c.transform_at(self.keyframe_frame() as f64).get(property),
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
        let slider = self.inspector_slider(Some(property), ratio, enabled, cx);
        div()
            .h(px(37.0))
            .flex()
            .items_center()
            .gap(px(6.0))
            .text_size(px(12.0))
            .child(div().w(px(63.0)).flex_none().child(label))
            .child(slider)
            .child(self.property_number(property, enabled, cx))
            .child(self.keyframe_controls(
                dinuosi_core::keyframes::KeyframeKind::for_property(property),
                cx,
            ))
    }
    /// 绘制变换或音频共用的滑块；按下保存快照并捕获根节点移动，释放提交一次历史。
    /// property 为 None 时使用音频增益，ratio 为核心计算的 0..1 比例，禁用时忽略指针。
    pub(crate) fn inspector_slider(
        &self,
        property: Option<Property>,
        ratio: f32,
        enabled: bool,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<Div> {
        let entity = cx.entity();
        let selector: gpui::SharedString = property.map_or_else(
            || "inspector-volume-slider".into(),
            |p| format!("inspector-slider-{p:?}").into(),
        );
        div()
            .id(selector.clone())
            .debug_selector(move || selector.to_string())
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
                                        this.commit_inspector_input(cx);
                                        this.stop_timeline();
                                        this.focus_handle.focus(window);
                                        this.timeline.before = Some(this.timeline.model.clone());
                                        this.timeline.gesture = Some(match property {
                                            Some(property) => {
                                                Gesture::Property { property, bounds }
                                            }
                                            None => Gesture::Volume { bounds },
                                        });
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
            )
    }
    /// 数字框支持直接输入、Enter/失焦提交、Esc 取消及上下箭头单步调整。
    /// 所有参数沿用完整 59px 数值区与 17px 微调箭头，坐标布局负责适配窄窗口。
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
                            .debug_selector(move || format!("inspector-step-{property:?}-{delta}"))
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
    /// 提交画面或音量数值输入；音量支持 dB 后缀和负无穷，非法数字显示真实错误且保持原参数。
    /// 轨道名称仅修改所选轨道；输入实体先取出，避免失焦回调重复提交。
    pub(crate) fn commit_inspector_input(&mut self, cx: &mut Context<Self>) {
        self.timeline.input_blur = None;
        if let Some(input) = self.timeline.volume_input.take() {
            let text = input
                .read(cx)
                .text
                .trim()
                .trim_end_matches("dB")
                .trim()
                .replace('−', "-");
            let value = if text == "-∞" {
                Ok(f64::NEG_INFINITY)
            } else {
                text.parse::<f64>()
            };
            match value {
                Ok(db) => self.change_volume_db(db, cx),
                Err(_) => self.notice = Some("请输入有效音量".into()),
            }
        }
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
                self.timeline.model.tracks[index].custom_name = true;
                self.commit_timeline(before, cx);
            } else {
                self.notice = Some("轨道名称应为 1–32 个字符".into());
            }
        }
        cx.notify();
    }
    /// 用当前语言显示的轨道名激活输入，不改变轨道身份；提交后将名称标为用户原文。
    pub(crate) fn begin_track_name(
        &mut self,
        index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.commit_inspector_input(cx);
        let text = crate::timeline_tracks::track_name(&self.timeline.model.tracks[index], cx);
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
                .any(|c| &c.id == id && !c.locked && !self.timeline.model.tracks[c.track].locked)
        });
        self.commit_timeline(before, cx);
    }
}
