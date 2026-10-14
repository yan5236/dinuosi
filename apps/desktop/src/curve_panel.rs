//! 独立关键帧面板：常用预设与精确节点输入，共用时间线视口，不展开素材轨道。
use crate::{
    input::TextInput,
    ui::{LINE, MUTED, PANEL, TEXT},
    view::Desktop,
};
use dinuosi_core::{keyframe_curve::Easing, keyframes::KeyframeKind as K, timeline::Clip};
use gpui::{Context, Div, Entity, div, prelude::*, px, rgb};

pub(crate) static CHANNELS: std::sync::LazyLock<Vec<(K, usize, &'static str)>> =
    std::sync::LazyLock::new(|| {
        let mut channels = vec![
            (K::Scale, 0, "Scale"),
            (K::Position, 0, "Position X"),
            (K::Position, 1, "Position Y"),
            (K::Rotation, 0, "Rotation"),
            (K::Opacity, 0, "Opacity"),
            (K::Volume, 0, "Volume"),
        ];
        for stage in 0..8 {
            channels.extend(dinuosi_core::color::Control::all().map(|c| {
                (
                    K::Color(stage, c),
                    0,
                    crate::color_controls::control_label(c),
                )
            }));
        }
        channels
    });

#[derive(Default)]
pub(crate) struct CurveSession {
    pub open: bool,
    pub channel: usize,
    /// 绑定片段身份及局部帧，避免切换素材、撤销或分割后误写别的节点。
    pub selected: Option<(String, i64)>,
    pub input: Option<CurveInput>,
    pub blur: Option<gpui::Subscription>,
    pub bounds: gpui::Bounds<gpui::Pixels>,
}
pub(crate) struct CurveInput {
    pub time: bool,
    pub id: String,
    pub frame: i64,
    pub channel: usize,
    pub entity: Entity<TextInput>,
}

#[derive(Clone, Copy)]
enum CurveAction {
    Easing(Easing),
    Link(bool),
    Channel(usize),
    Close,
}

impl Desktop {
    /// 调色页使用检查器主画面，其他页要求唯一选择；曲线编辑不修改关联组中的其他成员。
    pub(crate) fn curve_clip(&self) -> Option<&Clip> {
        if self.timeline.inspector_tab == crate::timeline_state::InspectorTab::Color {
            return self.color_clip();
        }
        (self.timeline.selected.len() == 1)
            .then(|| self.inspector_clip())
            .flatten()
    }

    /// 音频固定音量；画面保留有效调色动画通道，否则回退至上次选择的几何属性。
    pub(crate) fn curve_channel(&self) -> usize {
        if self
            .curve_clip()
            .is_some_and(|c| c.kind == dinuosi_core::media::MediaKind::Audio)
        {
            5
        } else {
            let channel = self.timeline.curves.channel;
            if channel >= 6
                && CHANNELS.get(channel).is_some_and(|(kind, _, _)| {
                    self.curve_clip()
                        .is_some_and(|c| c.keyframes.iter().any(|k| &k.kind == kind))
                })
            {
                channel
            } else {
                channel.min(4)
            }
        }
    }

    /// 返回仍属于当前素材和属性的选中节点；过期选择不提供输入或预设目标。
    pub(crate) fn curve_key(&self) -> Option<&dinuosi_core::keyframes::Keyframe> {
        let c = self.curve_clip()?;
        let (id, frame) = self.timeline.curves.selected.as_ref()?;
        let kind = CHANNELS[self.curve_channel()].0;
        if &c.id != id {
            return None;
        }
        c.keyframes
            .iter()
            .find(|k| k.kind == kind && k.frame == *frame && k.frame >= 0 && k.frame < c.duration)
    }

    /// 曲线数值沿用检查器单位；音量显示 dB，零增益显示负无穷，其余显示百分比/像素/角度。
    /// Y 分量从旧项目坐标转换为向上为正的界面坐标。
    pub(crate) fn curve_value_label(channel: usize, value: f64) -> String {
        match channel {
            2 => format!(
                "{:.2}",
                Self::property_coordinate(dinuosi_core::timeline::Property::Y, value)
            ),
            5 if value <= 0.0 => "−∞ dB".into(),
            5 => format!("{:.2} dB", 20.0 * value.log10()),
            0 | 4 => format!("{value:.2}%"),
            3 => format!("{value:.2}°"),
            _ => format!("{value:.2}"),
        }
    }

    /// 绘制独立面板及属性列表；预设作用于选中节点后的区间（末节点使用前一区间）。
    /// 常用操作直接可见，节点被选中后才展示精确输入和自由/联动控制；最小窗口仍保留轨道空间。
    pub(crate) fn curve_panel(&self, height: f32, cx: &mut Context<Self>) -> Div {
        let channel = self.curve_channel();
        let clip = self.curve_clip();
        let selected = self.curve_key();
        let enabled = !self.busy
            && clip.is_some_and(|c| !c.locked && !self.timeline.model.tracks[c.track].locked);
        let editable = enabled && selected.is_some();
        let linked = selected
            .and_then(|k| k.curves[CHANNELS[channel].1])
            .is_some_and(|c| c.linked);
        let custom = selected
            .and_then(|k| k.curves[CHANNELS[channel].1])
            .is_some();
        div()
            .debug_selector(|| "keyframe-curve-panel".into())
            .h(px(height))
            .flex_none()
            .flex()
            .flex_col()
            .bg(rgb(PANEL))
            .border_b_1()
            .border_color(rgb(LINE))
            .child(
                div()
                    .h(px(34.0))
                    .flex_none()
                    .flex()
                    .items_center()
                    .gap(px(6.0))
                    .child(
                        div()
                            .w(px(self.timeline.header))
                            .flex_none()
                            .px(px(12.0))
                            .child(crate::i18n::t("timeline", "Keyframe curves", cx)),
                    )
                    .children(
                        [
                            (Easing::Linear, "Linear"),
                            (Easing::EaseIn, "Ease in"),
                            (Easing::EaseOut, "Ease out"),
                            (Easing::EaseInOut, "Ease in/out"),
                        ]
                        .into_iter()
                        .enumerate()
                        .map(|(index, (easing, label))| {
                            self.curve_button(
                                format!("curve-easing-{index}"),
                                label,
                                false,
                                editable,
                                CurveAction::Easing(easing),
                                cx,
                            )
                        }),
                    )
                    .when(selected.is_some(), |s| {
                        s.children([(false, "Free"), (true, "Linked")].map(|(link, label)| {
                            self.curve_button(
                                format!("curve-linked-{link}"),
                                label,
                                custom && linked == link,
                                editable,
                                CurveAction::Link(link),
                                cx,
                            )
                        }))
                        .child(div().flex_1())
                        .child(self.curve_number(true, editable, cx))
                        .child(self.curve_number(false, editable, cx))
                    })
                    .child(div().flex_1())
                    .child(self.curve_button(
                        "curve-close".into(),
                        "Close",
                        false,
                        true,
                        CurveAction::Close,
                        cx,
                    ))
                    .child(div().w(px(8.0))),
            )
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .relative()
                    .child(self.curve_graph(enabled, cx))
                    .child(
                        div()
                            .absolute()
                            .left_0()
                            .top_0()
                            .bottom_0()
                            .w(px((self.timeline.header - 65.0).max(92.0)))
                            .id("curve-properties")
                            .overflow_y_scroll()
                            .flex()
                            .flex_col()
                            .children(
                                CHANNELS
                                    .iter()
                                    .enumerate()
                                    .filter(|(_, (kind, _, _))| {
                                        clip.is_some_and(|c| {
                                            kind.accepts(c.kind)
                                                && (!matches!(kind, K::Color(..))
                                                    || c.keyframes.iter().any(|k| k.kind == *kind))
                                        })
                                    })
                                    .map(|(index, (_, _, label))| {
                                        self.curve_button(
                                            format!("curve-channel-{index}"),
                                            label,
                                            channel == index,
                                            true,
                                            CurveAction::Channel(index),
                                            cx,
                                        )
                                        .h(px(22.0))
                                    }),
                            ),
                    ),
            )
    }

    /// 生成沿用灰阶细描边的可聚焦按钮；禁用控件保留空间并拒绝键鼠操作，不新增图标资源。
    fn curve_button(
        &self,
        id: String,
        label: &'static str,
        selected: bool,
        enabled: bool,
        action: CurveAction,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<Div> {
        let debug = id.clone();
        div()
            .id(gpui::SharedString::from(id))
            .debug_selector(move || debug.clone())
            .h(px(24.0))
            .px(px(8.0))
            .flex_none()
            .flex()
            .items_center()
            .justify_center()
            .text_size(px(11.0))
            .text_color(rgb(if selected { TEXT } else { MUTED }))
            .bg(rgb(if selected { LINE } else { PANEL }))
            .opacity(if enabled { 1.0 } else { 0.35 })
            .when(enabled, |s| {
                s.focusable()
                    .tab_index(0)
                    .cursor_pointer()
                    .hover(|s| s.bg(rgb(LINE)))
                    .focus(|s| s.bg(rgb(LINE)))
            })
            .child(
                if let CurveAction::Channel(index) = action
                    && let K::Color(stage, control) = CHANNELS[index].0
                {
                    crate::color_controls::channel_label(stage, control, cx)
                } else {
                    crate::i18n::t("timeline", label, cx).to_string()
                },
            )
            .on_click(cx.listener(move |this, _, window, cx| {
                if enabled {
                    this.curve_action(action, window, cx);
                }
            }))
            .on_key_down(
                cx.listener(move |this, event: &gpui::KeyDownEvent, window, cx| {
                    if enabled
                        && !event.is_held
                        && !event.keystroke.modifiers.modified()
                        && matches!(event.keystroke.key.as_str(), "enter" | "space")
                    {
                        this.curve_action(action, window, cx);
                        cx.stop_propagation();
                    }
                }),
            )
    }

    /// 鼠标与键盘共用曲线控件路由，先完成输入/拖动事务；属性选择与收起不创建项目历史。
    fn curve_action(
        &mut self,
        action: CurveAction,
        window: &mut gpui::Window,
        cx: &mut Context<Self>,
    ) {
        self.commit_inspector_input(cx);
        self.end_timeline_pointer(window, cx);
        match action {
            CurveAction::Easing(easing) => self.apply_curve_easing(easing, cx),
            CurveAction::Link(link) => self.apply_curve_link(link, cx),
            CurveAction::Channel(index) => {
                self.timeline.curves.channel = index;
                self.timeline.curves.selected = None;
            }
            CurveAction::Close => self.toggle_curve_panel(window, cx),
        }
        self.focus_handle.focus(window);
        cx.notify();
    }

    /// 选中节点的局部帧/参数输入；复用原生 TextInput 的选择、IME、剪贴板与失焦提交。
    fn curve_number(&self, time: bool, enabled: bool, cx: &mut Context<Self>) -> Div {
        let Some(key) = self.curve_key() else {
            return div();
        };
        let channel = self.curve_channel();
        let value = if time {
            key.frame.to_string()
        } else {
            Self::curve_value_label(channel, key.values[CHANNELS[channel].1])
        };
        let active = self
            .timeline
            .curves
            .input
            .as_ref()
            .filter(|i| i.time == time);
        let id = if time { "curve-frame" } else { "curve-value" };
        div()
            .flex()
            .items_center()
            .gap(px(4.0))
            .text_size(px(11.0))
            .child(crate::i18n::t(
                "timeline",
                if time { "Frame" } else { "Value" },
                cx,
            ))
            .child(
                div()
                    .id(id)
                    .debug_selector(move || id.into())
                    .h(px(24.0))
                    .w(px(80.0))
                    .bg(rgb(crate::ui::BACKGROUND))
                    .flex()
                    .items_center()
                    .justify_center()
                    .overflow_hidden()
                    .when(active.is_none(), |s| s.child(value))
                    .when_some(active, |s, input| s.child(input.entity.clone()))
                    .when(enabled, |s| s.cursor_text().focusable().tab_index(0))
                    .on_click(cx.listener(move |this, _, window, cx| {
                        if enabled {
                            this.begin_curve_input(time, window, cx);
                        }
                    }))
                    .on_key_down(cx.listener(
                        move |this, event: &gpui::KeyDownEvent, window, cx| {
                            if enabled
                                && this.timeline.curves.input.is_none()
                                && !event.is_held
                                && !event.keystroke.modifiers.modified()
                                && matches!(event.keystroke.key.as_str(), "enter" | "space")
                            {
                                this.begin_curve_input(time, window, cx);
                                cx.stop_propagation();
                            }
                        },
                    )),
            )
    }
}
