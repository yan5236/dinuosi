//! HSL 色组语义轨道与预览取样；吸管只读取本软件画面，不访问屏幕或改变素材选择。
use crate::{color_inspector::COLORS, tooltip::Tooltip, ui::TEXT, view::Desktop};
use dinuosi_core::{
    color::{Control, PickedHsl},
    color_render::{HUES, from_hsl, to_hsl},
};
use dinuosi_icons::Icon;
use gpui::{Bounds, Context, Div, Pixels, Point, Window, div, prelude::*, px, rgb};

pub(crate) struct PickFrame {
    pub pixels: Vec<u8>,
    pub size: (u32, u32),
    pub frame: i64,
    pub revision: u64,
}

/// 计算白平衡或 HSL 轨道的显示色；H/S/L 分别使用邻近色相、灰到彩色及暗到浅的连续渐变。
/// t 为轨道归一化位置，取样色组用其真实色相；低饱和灰阶混合保持现有品牌的克制亮度。
pub(crate) fn track_color(control: Control, picked: Option<f64>, t: f64) -> u32 {
    let t = t.clamp(0.0, 1.0);
    let hue = match control {
        Control::Hsl(g, _) => Some(HUES[g]),
        Control::PickedHsl(_) => picked,
        _ => None,
    };
    let color = if let Some(hue) = hue {
        let i = match control {
            Control::Hsl(_, i) | Control::PickedHsl(i) => i,
            _ => unreachable!(),
        };
        let rgb = from_hsl(match i {
            0 => [hue + (t - 0.5) * 120.0, 0.75, 0.5],
            1 => [hue, t, 0.5],
            _ => [hue, 0.75, t],
        });
        rgb.map(|v| (v * 255.0 * 0.75 + 0x2d as f64 * 0.25).round() as u32)
    } else {
        let (a, b) = if matches!(control, Control::Basic(2)) {
            ([77.0, 119.0, 184.0], [202.0, 173.0, 90.0])
        } else {
            ([81.0, 155.0, 118.0], [175.0, 99.0, 161.0])
        };
        std::array::from_fn(|j| (a[j] + (b[j] - a[j]) * t) as u32)
    };
    color[0] << 16 | color[1] << 8 | color[2]
}

/// 将窗口坐标映射到实际预览 BGRA 像素并返回色相；排除留白、透明、无色及损坏尺寸。
/// 原始合成固定为 640×360，按实际项目比例裁掉中心补边，与 render_frame 的显示裁剪一致。
pub(crate) fn sample_hue(
    frame: &PickFrame,
    bounds: Bounds<Pixels>,
    p: Point<Pixels>,
) -> Option<f64> {
    let (w, h) = frame.size;
    if w == 0
        || h == 0
        || w > 640
        || h > 360
        || frame.pixels.len() != 640 * 360 * 4
        || !bounds.contains(&p)
        || bounds.size.width <= px(0.0)
        || bounds.size.height <= px(0.0)
    {
        return None;
    }
    let x = ((f32::from(p.x - bounds.left()) / f32::from(bounds.size.width) * w as f32) as u32)
        .min(w - 1)
        + (640 - w) / 2;
    let y = ((f32::from(p.y - bounds.top()) / f32::from(bounds.size.height) * h as f32) as u32)
        .min(h - 1)
        + (360 - h) / 2;
    let pixel = &frame.pixels[((y * 640 + x) * 4) as usize..][..4];
    let [hue, sat, _] = to_hsl([pixel[2], pixel[1], pixel[0]].map(|v| v as f64 / 255.0));
    (pixel[3] > 0 && sat > 0.001).then_some(hue)
}

impl Desktop {
    /// 绘制吸管、可选取样色组及固定八色；新取样色组拥有独立参数，切换不会覆盖其他色组。
    pub(crate) fn color_hsl(&self, cx: &mut Context<Self>) -> Div {
        let picked = self.color_grade().stages[self.color_stage()].picked.clone();
        let group = self.hsl_group();
        let enabled = self.color_enabled()
            && self.timeline.preview
            && self
                .color_clip()
                .is_some_and(|c| c.contains_frame(self.keyframe_frame()));
        div()
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .mb(px(12.0))
                    .child(
                        Tooltip::tool(
                            "color-hsl-picker",
                            Icon::Eyedropper,
                            enabled,
                            crate::i18n::t("color", "Pick color", cx),
                        )
                        .selected(self.hsl_picking())
                        .on_click(cx.listener(
                            move |this, _, window, cx| {
                                if enabled {
                                    this.toggle_hsl_picker(window, cx);
                                }
                            },
                        )),
                    )
                    .when_some(picked, |s, picked| {
                        let c =
                            from_hsl([picked.hue, 0.75, 0.5]).map(|v| (v * 255.0).round() as u32);
                        s.child(self.hsl_swatch(8, c[0] << 16 | c[1] << 8 | c[2], group, cx))
                    })
                    .children(
                        COLORS
                            .into_iter()
                            .enumerate()
                            .map(|(i, color)| self.hsl_swatch(i, color, group, cx)),
                    ),
            )
            .children(
                ["Hue", "Saturation", "Lightness"]
                    .into_iter()
                    .enumerate()
                    .map(|(i, label)| {
                        self.color_row(
                            if group == 8 {
                                Control::PickedHsl(i)
                            } else {
                                Control::Hsl(group, i)
                            },
                            label,
                            cx,
                        )
                    }),
            )
    }
    /// 绘制有明确选中边框的色组；点击仅切换检查器，先提交数值并取消旧取样模式。
    fn hsl_swatch(
        &self,
        index: usize,
        color: u32,
        selected: usize,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<Div> {
        div()
            .id(gpui::ElementId::Name(format!("color-hsl-{index}").into()))
            .debug_selector(move || format!("color-hsl-{index}"))
            .size(px(18.0))
            .flex_none()
            .rounded(px(9.0))
            .bg(rgb(color))
            .border_2()
            .border_color(rgb(if index == selected { TEXT } else { 0x29292d }))
            .cursor_pointer()
            .on_click(cx.listener(move |this, _, _, cx| {
                this.commit_inspector_input(cx);
                this.timeline.color.group = index;
                cx.notify();
            }))
    }
    /// 解析当前色组；切换到无取样色组的片段或重置步骤后回到红色，避免访问不存在的参数。
    fn hsl_group(&self) -> usize {
        if self.timeline.color.group == 8
            && self.color_grade().stages[self.color_stage()]
                .picked
                .is_some()
        {
            8
        } else if self.timeline.color.group < 8 {
            self.timeline.color.group
        } else {
            0
        }
    }
    /// 检查吸管仍绑定原主片段、步骤和 HSL 页面；过期身份不可写入另一素材。
    pub(crate) fn hsl_picking(&self) -> bool {
        self.timeline
            .color
            .picker
            .as_ref()
            .is_some_and(|(id, stage)| {
                self.timeline.inspector_tab == crate::timeline_state::InspectorTab::Color
                    && self.timeline.color.tool == 1
                    && self.color_clip().is_some_and(|c| &c.id == id)
                    && self.color_stage() == *stage
            })
    }
    /// 启停一次性预览取样并暂停播放；只在取样期间请求/保留一帧 CPU 像素，重复点击吸管取消。
    pub(crate) fn toggle_hsl_picker(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let was_active = self.hsl_picking();
        self.commit_inspector_input(cx);
        if was_active {
            cx.notify();
            return;
        }
        if !self.color_enabled() {
            return;
        }
        let Some(clip) = self
            .color_clip()
            .filter(|c| self.timeline.preview && c.contains_frame(self.keyframe_frame()))
        else {
            return;
        };
        let id = clip.id.clone();
        self.stop_timeline();
        self.focus_handle.focus(window);
        self.timeline.color.picker = Some((id, self.color_stage()));
        self.refresh_timeline(cx);
    }
    /// 取消吸管并立即释放保留的 CPU 帧；选择、编辑、翻页或 Esc 沿用此清理入口。
    pub(crate) fn cancel_hsl_picker(&mut self) {
        self.timeline.color.picker = None;
        self.timeline.color.pick_frame = None;
    }
    /// 在有效当前帧采样并提交独立色组；取样不移动/改选片段，已有取样参数与动画保留，成功后退出。
    pub(crate) fn pick_hsl(&mut self, p: Point<Pixels>, cx: &mut Context<Self>) {
        if !self.hsl_picking() {
            self.cancel_hsl_picker();
            cx.notify();
            return;
        }
        let Some(frame) =
            self.timeline.color.pick_frame.as_ref().filter(|f| {
                f.revision == self.timeline.revision && f.frame == self.keyframe_frame()
            })
        else {
            return;
        };
        let Some(hue) = sample_hue(frame, self.timeline.preview_bounds, p) else {
            return;
        };
        self.cancel_hsl_picker();
        self.edit_color(
            |grade, stage| {
                grade.stages[stage]
                    .picked
                    .get_or_insert(PickedHsl {
                        hue,
                        values: [0.0; 3],
                    })
                    .hue = hue;
            },
            cx,
        );
        self.timeline.color.group = 8;
        cx.notify();
    }
}
