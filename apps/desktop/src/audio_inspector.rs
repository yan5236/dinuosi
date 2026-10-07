//! 音频片段检查器：沿用画面检查器的标签、滑块、数值框及微调布局，只提供音量参数。
use crate::{
    input::TextInput,
    ui::{LINE, TEXT},
    view::Desktop,
};
use dinuosi_core::{media::MediaKind, timeline::volume_ratio};
use dinuosi_icons::Icon;
use gpui::{Context, Div, Window, div, prelude::*, px, rgb, svg};

impl Desktop {
    /// 读取首个选中的音频片段增益，返回单位增益作为无选择时的默认值，不修改模型。
    fn audio_gain(&self) -> f64 {
        self.timeline
            .model
            .clips
            .iter()
            .find(|c| c.kind == MediaKind::Audio && self.timeline.selected.contains(&c.id))
            .map_or(1.0, |c| c.gain)
    }
    /// 格式化音量为一位小数 dB；零增益显示负无穷，小于显示精度的零值消除负号。
    fn audio_volume_text(&self) -> String {
        let gain = self.audio_gain();
        if gain == 0.0 {
            "-∞dB".into()
        } else {
            let db = 20.0 * gain.log10();
            format!("{:.1}dB", if db.abs() < 0.05 { 0.0 } else { db })
        }
    }
    /// 按现有检查器绘制左侧 63px 标签、中间滑块及右侧 76px 数值/箭头控件。
    /// 默认 0.0dB、最低 -∞dB、最高 20.0dB；锁定轨道禁用，仍只包含音量一个参数。
    pub(crate) fn audio_inspector(&self, enabled: bool, cx: &mut Context<Self>) -> Div {
        div()
            .debug_selector(|| "audio-inspector".into())
            .pt(px(38.0))
            .px(px(20.0))
            .opacity(if enabled { 1.0 } else { 0.4 })
            .child(
                div()
                    .h(px(37.0))
                    .flex()
                    .items_center()
                    .gap(px(14.0))
                    .text_size(px(12.0))
                    .text_color(rgb(TEXT))
                    .child(
                        div()
                            .debug_selector(|| "inspector-volume-label".into())
                            .w(px(63.0))
                            .flex_none()
                            .child(crate::i18n::t("inspector", "Volume", cx)),
                    )
                    .child(self.inspector_slider(
                        None,
                        volume_ratio(self.audio_gain()) as f32,
                        enabled,
                        cx,
                    ))
                    .child(self.audio_volume_number(enabled, cx)),
            )
    }
    /// 绘制与现有画面参数相同的 59px 深色数值框及 17px 上下箭头，数值居中并保留 dB 单位。
    /// 点击数值进入原生文本编辑；箭头每次调整 0.1dB，静音向上恢复 -60dB，向下越过 -60dB 静音。
    fn audio_volume_number(&self, enabled: bool, cx: &mut Context<Self>) -> Div {
        div()
            .flex()
            .items_center()
            .flex_none()
            .w(px(76.0))
            .h(px(24.0))
            .child(
                div()
                    .id("inspector-volume-value")
                    .debug_selector(|| "inspector-volume-value".into())
                    .w(px(59.0))
                    .h(px(24.0))
                    .border_1()
                    .border_color(rgb(LINE))
                    .bg(rgb(0x212123))
                    .overflow_hidden()
                    .text_size(px(12.0))
                    .when(self.timeline.volume_input.is_none(), |s| {
                        s.child(
                            div()
                                .size_full()
                                .flex()
                                .items_center()
                                .justify_center()
                                .child(self.audio_volume_text()),
                        )
                    })
                    .when_some(self.timeline.volume_input.as_ref(), |s, input| {
                        s.child(input.clone())
                    })
                    .on_click(cx.listener(move |this, _, window, cx| {
                        if enabled {
                            this.begin_volume_input(window, cx);
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
                            .id(if delta > 0.0 {
                                "inspector-volume-up"
                            } else {
                                "inspector-volume-down"
                            })
                            .debug_selector(move || {
                                if delta > 0.0 {
                                    "inspector-volume-up".into()
                                } else {
                                    "inspector-volume-down".into()
                                }
                            })
                            .h(px(12.0))
                            .w_full()
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
                                    this.commit_inspector_input(cx);
                                    let gain = this.audio_gain();
                                    let db = if gain == 0.0 {
                                        if delta > 0.0 {
                                            -60.0
                                        } else {
                                            f64::NEG_INFINITY
                                        }
                                    } else {
                                        let db = (20.0 * gain.log10() * 10.0).round() / 10.0
                                            + delta * 0.1;
                                        if db < -60.0 { f64::NEG_INFINITY } else { db }
                                    };
                                    this.change_volume_db(db, cx);
                                }
                            }))
                    })),
            )
    }
    /// 激活音量数值输入，复用原生 IME、剪贴板和检查器失焦提交；重复点击保留现有编辑内容。
    fn begin_volume_input(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.timeline.volume_input.is_some() {
            return;
        }
        self.commit_inspector_input(cx);
        let text = self.audio_volume_text().trim_end_matches("dB").to_owned();
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
        self.timeline.volume_input = Some(input);
        cx.notify();
    }
    /// 将用户输入的 dB 转为有限增益并提交一次历史；负无穷为静音，有限值夹紧至 20dB。
    /// NaN 或正无穷返回可见错误且保留原音量；仅修改选中且未锁定的音频片段。
    pub(crate) fn change_volume_db(&mut self, db: f64, cx: &mut Context<Self>) {
        if db.is_nan() || db == f64::INFINITY {
            self.notice = Some("请输入有效音量".into());
            cx.notify();
            return;
        }
        let gain = if db == f64::NEG_INFINITY {
            0.0
        } else {
            10.0_f64.powf(db.min(20.0) / 20.0)
        };
        self.stop_timeline();
        let before = self.timeline.model.clone();
        if let Err(error) = self
            .timeline
            .model
            .set_volume(&self.timeline.selected, gain)
        {
            self.notice = Some(error);
        } else {
            self.commit_timeline(before, cx);
        }
        cx.notify();
    }
}
