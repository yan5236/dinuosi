//! 与合成器一致的预览几何：640×360 解码画布、素材有效区域和绕中心变换。
use dinuosi_core::timeline::Transform;
use gpui::{Bounds, Pixels, Point, point, px, size};

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Handle {
    Move,
    Scale,
    Rotate,
}

#[derive(Clone, Copy, Debug)]
pub struct Geometry {
    pub canvas: Bounds<Pixels>,
    pub center: Point<Pixels>,
    pub half: Point<f32>,
    pub angle: f32,
}

/// 在可用预览区域内居中适配合成器的 16:9 输出，返回实际画面边界，不包括面板留白。
pub fn canvas_bounds(viewport: Bounds<Pixels>) -> Bounds<Pixels> {
    let ratio = (f32::from(viewport.size.width) / 640.0)
        .min(f32::from(viewport.size.height) / 360.0)
        .max(0.0);
    let dimensions = size(px(640.0 * ratio), px(360.0 * ratio));
    Bounds::new(
        viewport.center() - point(dimensions.width / 2.0, dimensions.height / 2.0),
        dimensions,
    )
}

impl Geometry {
    /// 从项目参数生成窗口几何；有效尺寸排除解码透明补边，与 Core blend 的缩放及旋转一致。
    pub fn new(
        canvas: Bounds<Pixels>,
        t: &Transform,
        effective: (u32, u32),
        project: (u32, u32),
    ) -> Self {
        let width = f32::from(canvas.size.width);
        let height = f32::from(canvas.size.height);
        Self {
            canvas,
            center: canvas.center()
                + point(
                    px(t.x as f32 * width / project.0.max(1) as f32),
                    px(t.y as f32 * height / project.1.max(1) as f32),
                ),
            half: point(
                width * effective.0 as f32 / 640.0 * t.scale as f32 / 200.0,
                height * effective.1 as f32 / 360.0 * t.scale as f32 / 200.0,
            ),
            angle: t.rotation.to_radians() as f32,
        }
    }

    /// 把以素材中心为原点的局部像素旋转到窗口坐标，不裁切，供边框与手柄共同使用。
    pub fn position(&self, x: f32, y: f32) -> Point<Pixels> {
        let (sin, cos) = self.angle.sin_cos();
        self.center + point(px(x * cos - y * sin), px(x * sin + y * cos))
    }

    /// 返回按顺时针排列的四个真实角点；超出画布的部分由绘制裁切而非扭曲边框。
    pub fn corners(&self) -> [Point<Pixels>; 4] {
        [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)]
            .map(|(x, y)| self.position(x * self.half.x, y * self.half.y))
    }

    /// 只保留实际处于画布内的角点，并将圆形手柄内收，保证满屏背景的手柄完整可见。
    pub fn handles(&self) -> Vec<Point<Pixels>> {
        self.corners()
            .into_iter()
            .filter(|p| self.canvas.contains(p))
            .map(|p| self.inset(p, 4.0))
            .collect()
    }

    /// 将下边中心的旋转手柄限制在画布内；下边完全不可见时不伪造远处的操作柄。
    pub fn rotation_handle(&self) -> Option<Point<Pixels>> {
        let edge = self.position(0.0, self.half.y);
        self.canvas
            .contains(&edge)
            .then(|| self.inset(self.position(0.0, self.half.y + 22.0), 10.0))
    }

    /// 把控件中心夹紧到画布内指定边距；极小画布使用中心以避免反向区间。
    fn inset(&self, p: Point<Pixels>, margin: f32) -> Point<Pixels> {
        let x = px(margin.min(f32::from(self.canvas.size.width) / 2.0));
        let y = px(margin.min(f32::from(self.canvas.size.height) / 2.0));
        point(
            p.x.clamp(self.canvas.left() + x, self.canvas.right() - x),
            p.y.clamp(self.canvas.top() + y, self.canvas.bottom() - y),
        )
    }

    /// 逆旋转命中素材矩形，排除输出画布外及零缩放区域；不会把旋转后包围盒空角选中。
    pub fn contains(&self, p: Point<Pixels>) -> bool {
        if !self.canvas.contains(&p) || self.half.x <= 0.0 || self.half.y <= 0.0 {
            return false;
        }
        let dx = f32::from(p.x - self.center.x);
        let dy = f32::from(p.y - self.center.y);
        let (sin, cos) = self.angle.sin_cos();
        (dx * cos + dy * sin).abs() <= self.half.x && (-dx * sin + dy * cos).abs() <= self.half.y
    }

    /// 优先命中旋转/缩放控件；控件命中也必须位于画布内，避免侵占预览控制条。
    pub fn handle_at(&self, p: Point<Pixels>) -> Option<Handle> {
        if !self.canvas.contains(&p) {
            return None;
        }
        if self
            .rotation_handle()
            .is_some_and(|h| distance(h, p) <= 10.0)
        {
            return Some(Handle::Rotate);
        }
        self.handles()
            .iter()
            .any(|h| distance(*h, p) <= 8.0)
            .then_some(Handle::Scale)
    }
}

/// 返回窗口点之间的欧氏距离，供手柄命中与等比例中心缩放计算使用。
pub fn distance(a: Point<Pixels>, b: Point<Pixels>) -> f32 {
    f32::from(a.x - b.x).hypot(f32::from(a.y - b.y))
}
