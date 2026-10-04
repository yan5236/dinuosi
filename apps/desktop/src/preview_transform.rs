//! 预览素材选择及位置/缩放/旋转操作；复用时间线选择、参数约束、保存与单次撤销。
use crate::{
    preview_geometry::{Geometry, Handle, canvas_bounds, distance},
    timeline_state::Gesture,
    view::Desktop,
};
use dinuosi_core::{
    media::MediaKind,
    timeline::{Property, Transform},
};
use gpui::{
    Bounds, ContentMask, Context, Div, MouseButton, MouseDownEvent, PathBuilder, Pixels, Point,
    Window, canvas, div, fill, point, prelude::*, px, rgb, size,
};

pub struct PreviewDrag {
    pub id: String,
    pub handle: Handle,
    pub start: Point<Pixels>,
    pub geometry: Geometry,
    pub transform: Transform,
}

pub struct PreviewShape {
    pub id: String,
    pub geometry: Geometry,
    pub selected: bool,
}

impl Desktop {
    /// 按合成器的图层顺序列出当前帧可编辑的画面；排除离线、隐藏、锁定、音频及时间范围外片段。
    /// 仅使用已加载元数据，不在绘制或鼠标事件中探测磁盘；返回顺序从最上层到背景。
    pub(crate) fn preview_shapes(&self) -> Vec<PreviewShape> {
        if !self.timeline.preview {
            return Vec::new();
        }
        let model = &self.timeline.model;
        let frame = model.frame(self.session.timeline_position);
        let mut clips: Vec<_> = model
            .clips
            .iter()
            .filter(|c| {
                let track = &model.tracks[c.track];
                track.visible
                    && !track.locked
                    && c.kind != MediaKind::Audio
                    && frame >= c.start
                    && frame < c.start + c.duration
            })
            .collect();
        clips.sort_by_key(|c| c.track);
        clips
            .into_iter()
            .filter_map(|clip| {
                let media = self
                    .session
                    .media
                    .iter()
                    .find(|m| m.path == clip.source && !m.offline)?;
                Some(PreviewShape {
                    id: clip.id.clone(),
                    geometry: Geometry::new(
                        self.timeline.preview_bounds,
                        &clip.transform,
                        media.preview_size,
                        (model.width, model.height),
                    ),
                    selected: self.timeline.selected.contains(&clip.id),
                })
            })
            .collect()
    }

    /// 在合成图上覆盖透明鼠标层；布局阶段记录实际画布，绘制阶段统一裁切选择框和控件。
    pub(crate) fn preview_transform_overlay(&self, cx: &mut Context<Self>) -> gpui::Stateful<Div> {
        let entity = cx.entity();
        let paint_entity = entity.clone();
        div()
            .id("preview-transform")
            .debug_selector(|| "preview-transform".into())
            .absolute()
            .inset_0()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(Self::begin_preview_transform),
            )
            .child(
                canvas(
                    move |bounds, _, cx| {
                        entity.update(cx, |this, _| {
                            this.timeline.preview_bounds = canvas_bounds(bounds)
                        })
                    },
                    move |_, _, window, cx| {
                        let this = paint_entity.read(cx);
                        let bounds = this.timeline.preview_bounds;
                        window.with_content_mask(Some(ContentMask { bounds }), |window| {
                            for shape in this.preview_shapes().iter().filter(|s| s.selected) {
                                paint_selection(&shape.geometry, window);
                            }
                        });
                    },
                )
                .absolute()
                .inset_0(),
            )
    }

    /// 点击预览时选择最上层命中片段，并开始相应变换；已选手柄优先，空白清空选择。
    /// 提交正在输入的检查器值，停止播放并保存按下快照；片段时间、源文件和层级均不改变。
    fn begin_preview_transform(
        &mut self,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.busy || !self.timeline.preview {
            return;
        }
        self.commit_inspector_input(cx);
        self.focus_handle.focus(window);
        let shapes = self.preview_shapes();
        let target = shapes
            .iter()
            .filter(|s| s.selected)
            .find_map(|s| s.geometry.handle_at(event.position).map(|h| (s, h)))
            .or_else(|| {
                shapes
                    .iter()
                    .find(|s| s.geometry.contains(event.position))
                    .map(|s| (s, Handle::Move))
            });
        let Some((shape, handle)) = target else {
            self.timeline.selected.clear();
            cx.notify();
            cx.stop_propagation();
            return;
        };
        let transform = self
            .timeline
            .model
            .clips
            .iter()
            .find(|c| c.id == shape.id)
            .unwrap()
            .transform
            .clone();
        self.timeline.selected = [shape.id.clone()].into();
        self.stop_timeline();
        self.stop_preview();
        self.timeline.before = Some(self.timeline.model.clone());
        self.timeline.gesture = Some(Gesture::Preview(PreviewDrag {
            id: shape.id.clone(),
            handle,
            start: event.position,
            geometry: shape.geometry,
            transform,
        }));
        self.timeline.moved = false;
        cx.notify();
        cx.stop_propagation();
    }

    /// 从按下位置计算变换，位置映射到项目像素、缩放保持比例、角度取最短有向差。
    /// 连续移动只改内存并刷新合成；使用核心参数限值，释放沿用时间线事务一次保存。
    pub(crate) fn move_preview_transform(&mut self, p: Point<Pixels>, cx: &mut Context<Self>) {
        let Some(Gesture::Preview(drag)) = &self.timeline.gesture else {
            return;
        };
        if !self.timeline.moved && distance(p, drag.start) < 2.0 {
            return;
        }
        self.timeline.moved = true;
        let mut t = drag.transform.clone();
        match drag.handle {
            Handle::Move => {
                let dx = f32::from(p.x - drag.start.x)
                    / f32::from(drag.geometry.canvas.size.width).max(1.0);
                let dy = f32::from(p.y - drag.start.y)
                    / f32::from(drag.geometry.canvas.size.height).max(1.0);
                let _ = t.set(
                    Property::X,
                    drag.transform.x + dx as f64 * self.timeline.model.width as f64,
                );
                let _ = t.set(
                    Property::Y,
                    drag.transform.y + dy as f64 * self.timeline.model.height as f64,
                );
            }
            Handle::Scale => {
                let ratio = distance(p, drag.geometry.center)
                    / distance(drag.start, drag.geometry.center).max(1.0);
                let _ = t.set(
                    Property::Scale,
                    (drag.transform.scale * ratio as f64).max(0.1),
                );
            }
            Handle::Rotate => {
                let center = drag.geometry.center;
                let start =
                    f32::from(drag.start.y - center.y).atan2(f32::from(drag.start.x - center.x));
                let end = f32::from(p.y - center.y).atan2(f32::from(p.x - center.x));
                let delta = (end - start).sin().atan2((end - start).cos()).to_degrees();
                let angle = drag.transform.rotation + delta as f64;
                let _ = t.set(
                    Property::Rotation,
                    (angle + 180.0).rem_euclid(360.0) - 180.0,
                );
            }
        }
        if let Some(clip) = self
            .timeline
            .model
            .clips
            .iter_mut()
            .find(|c| c.id == drag.id)
        {
            clip.transform = t;
        }
        self.refresh_timeline(cx);
    }
}

/// 绘制细白色旋转边框、四角缩放圆点及下方旋转柄；满屏时控件内收，整体由画布遮罩限制。
fn paint_selection(g: &Geometry, window: &mut Window) {
    let corners = g.corners();
    let mut path = PathBuilder::stroke(px(1.0));
    path.move_to(corners[0]);
    for p in &corners[1..] {
        path.line_to(*p);
    }
    path.close();
    if let Ok(path) = path.build() {
        window.paint_path(path, rgb(0xffffff));
    }
    for p in g.handles() {
        paint_dot(p, 3.5, 0xffffff, window);
    }
    if let Some(p) = g.rotation_handle() {
        let mut stem = PathBuilder::stroke(px(1.0));
        stem.move_to(g.position(0.0, g.half.y));
        stem.line_to(p);
        if let Ok(path) = stem.build() {
            window.paint_path(path, rgb(0xffffff));
        }
        paint_dot(p, 9.0, 0xffffff, window);
        paint_dot(p, 7.5, 0x262628, window);
        let mut arrow = PathBuilder::stroke(px(1.2));
        for n in 0..=16 {
            let angle = (40.0 + n as f32 * 17.5).to_radians();
            let q = p + point(px(angle.cos() * 4.5), px(angle.sin() * 4.5));
            if n == 0 {
                arrow.move_to(q);
            } else {
                arrow.line_to(q);
            }
        }
        arrow.line_to(p + point(px(0.5), px(-3.5)));
        if let Ok(path) = arrow.build() {
            window.paint_path(path, rgb(0xffffff));
        }
    }
}

/// 用圆角填充绘制固定屏幕尺寸的手柄，缩放素材时仍保持清晰且可点击。
fn paint_dot(center: Point<Pixels>, radius: f32, color: u32, window: &mut Window) {
    let mut dot = fill(
        Bounds::new(
            center - point(px(radius), px(radius)),
            size(px(radius * 2.0), px(radius * 2.0)),
        ),
        rgb(color),
    );
    dot.corner_radii = px(radius).into();
    window.paint_quad(dot);
}
