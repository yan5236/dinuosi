//! 调色会话与事务：绑定片段身份，手势期间仅预览，释放复用时间线保存与历史。
use crate::{input::TextInput, timeline_state::Gesture, view::Desktop};
use dinuosi_core::{
    color::{Control, Grade},
    media::MediaKind,
};
use gpui::{AppContext, Bounds, Context, Entity, Pixels, Point, Window};

#[derive(Default)]
pub(crate) struct ColorSession {
    pub steps_open: bool,
    pub tones_open: bool,
    pub wheel_values: bool,
    pub wheel: usize,
    pub stage: usize,
    pub tool: usize,
    pub group: usize,
    pub picker: Option<(String, usize)>,
    pub pick_frame: Option<crate::color_hsl::PickFrame>,
    pub channel: usize,
    pub clipboard: Option<Grade>,
    pub input: Option<(String, usize, Control, Entity<TextInput>)>,
    pub scopes: bool,
    pub scope_image: Option<std::sync::Arc<gpui::RenderImage>>,
}
#[derive(Clone, Copy)]
pub(crate) enum ColorTarget {
    Scalar(Control),
    Wheel(usize),
    Curve(usize, usize),
}
#[derive(Clone)]
pub(crate) struct ColorDrag {
    pub id: String,
    pub stage: usize,
    pub target: ColorTarget,
    pub bounds: Bounds<Pixels>,
}
impl Desktop {
    /// 返回检查器主画面片段的调色，音频或失效选择返回 None；不改变完整选择集合。
    pub(crate) fn color_clip(&self) -> Option<&dinuosi_core::timeline::Clip> {
        self.inspector_clip().filter(|c| c.kind != MediaKind::Audio)
    }
    /// 夹紧会话步骤索引，切换到步骤较少的片段或删除步骤后读取末步。
    pub(crate) fn color_stage(&self) -> usize {
        self.color_clip().map_or(0, |c| {
            self.timeline.color.stage.min(c.color.stages.len() - 1)
        })
    }
    /// 读取当前播放头调色用于所有数值、滑块和图形；静态片段借用，动画片段使用统一 Core 求值。
    pub(crate) fn color_grade(&self) -> std::borrow::Cow<'_, Grade> {
        self.color_clip()
            .unwrap()
            .color_at(self.keyframe_frame() as f64)
    }
    /// 重置全部/单步或删除当前步骤，保持其他动画身份；沿用一次保存、失败回滚及撤销。
    pub(crate) fn reset_color(&mut self, whole: bool, remove: bool, cx: &mut Context<Self>) {
        self.commit_inspector_input(cx);
        if !self.color_enabled() {
            return;
        }
        self.stop_timeline();
        let before = self.timeline.model.clone();
        let id = self.color_clip().unwrap().id.clone();
        self.timeline
            .model
            .reset_color(&id, (!whole).then(|| self.color_stage()), remove);
        self.timeline.color.stage = self.color_stage();
        self.timeline.preview = true;
        self.commit_timeline(before, cx);
    }
    /// 判断主片段是否可编辑；锁定组内其他片段不影响主对象的单片段调色。
    pub(crate) fn color_enabled(&self) -> bool {
        self.color_clip()
            .is_some_and(|c| !self.busy && !c.locked && !self.timeline.model.tracks[c.track].locked)
    }
    /// 修改当前主片段的完整调色并提交一次；错误显示实际原因，参数不部分写入。
    pub(crate) fn edit_color(
        &mut self,
        edit: impl FnOnce(&mut Grade, usize),
        cx: &mut Context<Self>,
    ) {
        self.commit_inspector_input(cx);
        if !self.color_enabled() {
            return;
        }
        self.stop_timeline();
        let before = self.timeline.model.clone();
        let clip = self.color_clip().unwrap();
        let id = clip.id.clone();
        let mut grade = clip.color_at(self.keyframe_frame() as f64).into_owned();
        let index = self.color_stage();
        edit(&mut grade, index);
        match self.timeline.model.set_color_at(
            &[id].into(),
            &grade,
            self.keyframe_frame(),
            self.auto_keyframe(cx),
        ) {
            Ok(()) => {
                self.timeline.preview = true;
                self.commit_timeline(before, cx);
            }
            Err(e) => self.notice = Some(e),
        }
        cx.notify();
    }
    /// 开始绑定片段/步骤的根层手势，记录一次快照并暂停播放；只在未锁定画面上操作。
    pub(crate) fn begin_color_drag(
        &mut self,
        target: ColorTarget,
        bounds: Bounds<Pixels>,
        p: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.commit_inspector_input(cx);
        if !self.color_enabled() {
            return;
        }
        self.stop_timeline();
        self.focus_handle.focus(window);
        if let ColorTarget::Wheel(index) = target {
            self.timeline.color.wheel = index;
        }
        let drag = ColorDrag {
            id: self.color_clip().unwrap().id.clone(),
            stage: self.color_stage(),
            target,
            bounds,
        };
        self.timeline.before = Some(self.timeline.model.clone());
        self.timeline.gesture = Some(Gesture::Color(drag.clone()));
        self.move_color_drag(&drag, p, cx);
    }
    /// 按真实布局边界更新标量、色轮或曲线；曲线点不跨越邻点，端点只改变输出值。
    /// 目标身份失效时忽略，不把旧手势应用到新选择；中途不保存到磁盘。
    /// 标量和色轮从按下快照计算，避免整体偏移夹紧后往返丢失动画；静态曲线沿用当前模型以保留刚添加的点。
    pub(crate) fn move_color_drag(
        &mut self,
        drag: &ColorDrag,
        p: Point<Pixels>,
        cx: &mut Context<Self>,
    ) {
        if matches!(drag.target, ColorTarget::Scalar(_) | ColorTarget::Wheel(_))
            && let Some(before) = &self.timeline.before
        {
            self.timeline.model = before.clone();
        }
        let Some(c) = self.timeline.model.clips.iter().find(|c| c.id == drag.id) else {
            return;
        };
        let mut grade = c.color_at(self.keyframe_frame() as f64).into_owned();
        let Some(stage) = grade.stages.get_mut(drag.stage) else {
            return;
        };
        let x = (f32::from(p.x - drag.bounds.left()) / f32::from(drag.bounds.size.width).max(1.0))
            .clamp(0.0, 1.0) as f64;
        let y = (f32::from(p.y - drag.bounds.top()) / f32::from(drag.bounds.size.height).max(1.0))
            .clamp(0.0, 1.0) as f64;
        let result = match drag.target {
            ColorTarget::Scalar(control) => {
                let (min, max) = control.range();
                stage.set(control, min + x * (max - min))
            }
            ColorTarget::Wheel(index) => {
                let mut dx = x * 2.0 - 1.0;
                let mut dy = y * 2.0 - 1.0;
                let radius = dx.hypot(dy).max(1.0);
                dx /= radius;
                dy /= radius;
                stage.wheels[index][0] = dx * 100.0;
                stage.wheels[index][1] = dy * 100.0;
                Ok(())
            }
            ColorTarget::Curve(channel, index) => {
                let points = &mut stage.curves[channel];
                if index >= points.len() {
                    return;
                }
                if index > 0 && index + 1 < points.len() {
                    let min = points[index - 1][0] + 0.001;
                    let max = points[index + 1][0] - 0.001;
                    if min <= max {
                        points[index][0] = x.clamp(min, max);
                    }
                }
                points[index][1] = 1.0 - y;
                Ok(())
            }
        };
        if let Err(e) = result.and_then(|()| {
            self.timeline.model.set_color_at(
                &[drag.id.clone()].into(),
                &grade,
                self.keyframe_frame(),
                self.auto_keyframe(cx),
            )
        }) {
            self.notice = Some(e);
        }
        self.timeline.preview = true;
        self.refresh_timeline(cx);
    }
    /// 激活当前步骤参数的原生数值输入，绑定片段身份，支持 IME、剪贴板和失焦提交。
    pub(crate) fn begin_color_input(
        &mut self,
        control: Control,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.commit_inspector_input(cx);
        if !self.color_enabled() {
            return;
        }
        let clip = self.color_clip().unwrap();
        let index = self.color_stage();
        let id = clip.id.clone();
        let v = clip.color_at(self.keyframe_frame() as f64).stages[index].get(control);
        let input = cx.new(|cx| {
            let mut i = TextInput::new(format!("{:.*}", control.precision(), v), cx);
            i.show_border = false;
            i.compact = true;
            i
        });
        input.read(cx).focus.focus(window);
        let focus = input.read(cx).focus.clone();
        self.timeline.input_blur = Some(cx.on_blur(&focus, window, |this, _, cx| {
            this.commit_inspector_input(cx)
        }));
        self.timeline.color.input = Some((id, index, control, input));
        cx.notify();
    }
    /// 提交绑定身份的数字，非法/非有限值保留原参数并报错；选择变化后不写入另一片段。
    pub(crate) fn commit_color_input(&mut self, cx: &mut Context<Self>) {
        let Some((id, index, control, input)) = self.timeline.color.input.take() else {
            return;
        };
        let Some(c) = self.color_clip().filter(|c| c.id == id) else {
            return;
        };
        let mut grade = c.color_at(self.keyframe_frame() as f64).into_owned();
        let result = input
            .read(cx)
            .text
            .trim()
            .parse::<f64>()
            .map_err(|_| "请输入有效调色数值".to_owned())
            .and_then(|v| {
                grade
                    .stages
                    .get_mut(index)
                    .ok_or("调色步骤已改变".to_owned())?
                    .set(control, v)
            });
        if let Err(e) = result {
            self.notice = Some(e);
            cx.notify();
            return;
        }
        self.stop_timeline();
        let before = self.timeline.model.clone();
        match self.timeline.model.set_color_at(
            &[id].into(),
            &grade,
            self.keyframe_frame(),
            self.auto_keyframe(cx),
        ) {
            Ok(()) => {
                self.timeline.preview = true;
                self.commit_timeline(before, cx);
            }
            Err(e) => self.notice = Some(e),
        }
    }
    /// 将会话中复制的整套调色粘贴到选中可编辑画面；一次保存与撤销，音频/锁定片段跳过。
    pub(crate) fn paste_color(&mut self, cx: &mut Context<Self>) {
        self.commit_inspector_input(cx);
        if self.busy {
            return;
        }
        let Some(grade) = self.timeline.color.clipboard.clone() else {
            return;
        };
        self.stop_timeline();
        let before = self.timeline.model.clone();
        match self
            .timeline
            .model
            .set_color(&self.timeline.selected, &grade)
        {
            Ok(()) => {
                self.timeline.preview = true;
                self.commit_timeline(before, cx);
            }
            Err(e) => self.notice = Some(e),
        }
        cx.notify();
    }
}
