//! 曲线面板事务、原生输入与拖动；连续预览留在内存，释放才保存一次。
use crate::{
    curve_graph::CurveDrag,
    curve_panel::{CHANNELS, CurveInput},
    input::TextInput,
    view::Desktop,
};
use dinuosi_core::{keyframe_curve::Easing, timeline::Timeline};
use gpui::{AppContext, Context, Pixels, Point, Window};

impl Desktop {
    /// 切换默认收起的独立面板；先提交已有输入和手势，再恢复工作区焦点，不写入项目数据。
    pub(crate) fn toggle_curve_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.commit_inspector_input(cx);
        self.end_timeline_pointer(window, cx);
        self.timeline.curves.open = !self.timeline.curves.open;
        self.timeline.curves.selected = None;
        self.focus_handle.focus(window);
        cx.notify();
    }

    /// 曲线离散操作共用持久化、回滚与撤销事务，真实错误显示在现有通知区。
    fn finish_curve_edit(
        &mut self,
        before: Timeline,
        result: Result<(), String>,
        cx: &mut Context<Self>,
    ) {
        if let Err(error) = result {
            self.timeline.model = before;
            self.notice = Some(error);
        } else {
            self.stop_timeline();
            self.timeline.preview = true;
            self.commit_timeline(before, cx);
        }
        cx.notify();
    }

    /// 将常用缓动交给 Core 的相邻区间规则；无有效节点或锁定时保持模型和播放头。
    pub(crate) fn apply_curve_easing(&mut self, easing: Easing, cx: &mut Context<Self>) {
        let Some(key) = self.curve_key() else {
            return;
        };
        let frame = key.frame;
        let id = self.curve_clip().unwrap().id.clone();
        let (kind, component, _) = CHANNELS[self.curve_channel()];
        let before = self.timeline.model.clone();
        let result = self
            .timeline
            .model
            .ease_curve(&id, kind, component, frame, easing);
        self.finish_curve_edit(before, result, cx);
    }

    /// 切换自由/联动切线，建立显式手柄供精调；保存一次历史，不跳转播放位置。
    pub(crate) fn apply_curve_link(&mut self, linked: bool, cx: &mut Context<Self>) {
        let Some(key) = self.curve_key() else {
            return;
        };
        let frame = key.frame;
        let id = self.curve_clip().unwrap().id.clone();
        let (kind, component, _) = CHANNELS[self.curve_channel()];
        let before = self.timeline.model.clone();
        let result = self
            .timeline
            .model
            .link_curve_handles(&id, kind, component, frame, linked);
        self.finish_curve_edit(before, result, cx);
    }

    /// 从按下快照及鼠标增量调整节点/手柄，保留按下偏移；节点限制在同类邻帧之间且整帧对齐。
    /// 只改当前属性分量，联动斜率由 Core 计算，预览实时刷新；移动期间不写磁盘或记录历史。
    pub(crate) fn move_curve_point(
        &mut self,
        p: Point<Pixels>,
        drag: CurveDrag,
        cx: &mut Context<Self>,
    ) {
        let Some(before) = self.timeline.before.as_ref() else {
            return;
        };
        let mut next = before.clone();
        let Some(c) = before.clips.iter().find(|c| c.id == drag.id) else {
            return;
        };
        let (kind, component, _) = CHANNELS[drag.channel];
        let Some(key) = c
            .keyframes
            .iter()
            .find(|k| k.kind == kind && k.frame == drag.frame)
        else {
            return;
        };
        let (x, y) = drag.view.value(drag.bounds, p);
        let (sx, sy) = drag.view.value(drag.bounds, drag.start);
        let (min, max) = kind.range(component);
        let result = if let Some(incoming) = drag.handle {
            let Some(neighbor) = c.curve_neighbor(kind, drag.frame, incoming) else {
                return;
            };
            let original = c
                .curve_handle(kind, component, drag.frame, incoming)
                .unwrap();
            let gap = (neighbor.frame - drag.frame) as f64;
            let length = (original[0] + (x - sx) / gap).clamp(0.0, 1.0);
            let limit = (max - min).max(1.0) * 2.0;
            next.edit_curve_handle(
                &drag.id,
                kind,
                component,
                drag.frame,
                incoming,
                [length, (original[1] + y - sy).clamp(-limit, limit)],
            )
        } else {
            let left = c
                .curve_neighbor(kind, drag.frame, true)
                .map_or(0, |k| (k.frame + 1).max(0));
            let right = c
                .curve_neighbor(kind, drag.frame, false)
                .map_or(c.duration - 1, |k| (k.frame - 1).min(c.duration - 1));
            let target = (drag.frame + (x - sx).round() as i64).clamp(left, right);
            let result = next.edit_curve_node(
                &drag.id,
                kind,
                component,
                drag.frame,
                target,
                (key.values[component] + y - sy).clamp(min, max),
            );
            if result.is_ok() {
                self.timeline.curves.selected = Some((drag.id.clone(), target));
            }
            result
        };
        match result {
            Ok(()) => {
                self.timeline.model = next;
                self.timeline.preview = true;
                self.refresh_timeline(cx);
            }
            Err(error) => self.notice = Some(error),
        }
    }

    /// 创建绑定片段/属性/节点身份的精确输入；时间以局部帧显示，音量以检查器一致的 dB 输入。
    pub(crate) fn begin_curve_input(
        &mut self,
        time: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self
            .timeline
            .curves
            .input
            .as_ref()
            .is_some_and(|i| i.time == time)
        {
            return;
        }
        self.commit_inspector_input(cx);
        let Some(key) = self.curve_key() else {
            return;
        };
        let frame = key.frame;
        let channel = self.curve_channel();
        let text = if time {
            frame.to_string()
        } else {
            Self::curve_value_label(channel, key.values[CHANNELS[channel].1])
        };
        let id = self.curve_clip().unwrap().id.clone();
        let input = cx.new(|cx| {
            let mut input = TextInput::new(text, cx);
            input.compact = true;
            input.show_border = false;
            input
        });
        let focus = input.read(cx).focus.clone();
        focus.focus(window);
        self.timeline.curves.blur =
            Some(cx.on_blur(&focus, window, |this, _, cx| this.commit_curve_input(cx)));
        self.timeline.curves.input = Some(CurveInput {
            time,
            id,
            frame,
            channel,
            entity: input,
        });
        cx.notify();
    }

    /// 提交精确帧/参数，失效身份忽略；非法、越界及重复帧输入保持旧数据并显示真实错误。
    /// Enter/失焦只提交一次，Y 正向输入转换回旧项目坐标；保存失败由现有事务完整回滚。
    pub(crate) fn commit_curve_input(&mut self, cx: &mut Context<Self>) {
        self.timeline.curves.blur = None;
        let Some(input) = self.timeline.curves.input.take() else {
            return;
        };
        if self.busy
            || self.curve_channel() != input.channel
            || !self.curve_clip().is_some_and(|c| c.id == input.id)
        {
            return;
        }
        let (kind, component, _) = CHANNELS[input.channel];
        let Some(key) = self.curve_key() else {
            return;
        };
        if key.frame != input.frame {
            return;
        }
        let old_value = key.values[component];
        let text = input
            .entity
            .read(cx)
            .text
            .trim()
            .trim_end_matches(['%', '°'])
            .trim_end_matches("dB")
            .trim()
            .replace('−', "-");
        let parsed = if input.time {
            text.parse::<i64>()
                .map(|frame| (frame, old_value))
                .map_err(|_| "请输入有效数值".to_owned())
        } else {
            let value = if input.channel == 5 && text == "-∞" {
                Ok(0.0)
            } else {
                text.parse::<f64>().map(|v| {
                    if input.channel == 5 {
                        10.0_f64.powf(v / 20.0)
                    } else if input.channel == 2 {
                        Self::property_coordinate(dinuosi_core::timeline::Property::Y, v)
                    } else {
                        v
                    }
                })
            };
            value
                .map(|v| (input.frame, v))
                .map_err(|_| "请输入有效数值".to_owned())
        };
        let before = self.timeline.model.clone();
        let target = parsed.as_ref().ok().map(|v| v.0);
        let result = parsed.and_then(|(frame, value)| {
            self.timeline.model.edit_curve_node(
                &input.id,
                kind,
                component,
                input.frame,
                frame,
                value,
            )
        });
        let success = result.is_ok();
        self.finish_curve_edit(before, result, cx);
        if success
            && self
                .timeline
                .model
                .clips
                .iter()
                .find(|c| c.id == input.id)
                .is_some_and(|c| {
                    c.keyframes
                        .iter()
                        .any(|k| k.kind == kind && Some(k.frame) == target)
                })
        {
            self.timeline.curves.selected = Some((input.id, target.unwrap()));
        }
    }

    /// 删除曲线面板内选中的属性节点，不删除素材或同帧其他属性；最后节点烘焙规则沿用 Core。
    pub(crate) fn delete_curve_key(&mut self, cx: &mut Context<Self>) {
        let Some(key) = self.curve_key() else {
            return;
        };
        let kind = key.kind;
        let c = self.curve_clip().unwrap();
        if self.busy || c.locked || self.timeline.model.tracks[c.track].locked {
            return;
        }
        let frame = c.start + key.frame;
        let ids = [c.id.clone()].into();
        let before = self.timeline.model.clone();
        self.timeline.model.toggle_keyframe(&ids, kind, frame);
        self.timeline.curves.selected = None;
        self.finish_curve_edit(before, Ok(()), cx);
    }
}
