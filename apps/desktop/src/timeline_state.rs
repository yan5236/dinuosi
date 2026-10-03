//! 桌面编辑会话仅保存指针、选择与绘制缓存；编辑规则和合成均委派 Core。
use crate::{input::TextInput, view::Desktop};
use dinuosi_core::timeline::{History, Property, Timeline};
use gpui::{Bounds, Entity, Pixels, Point, RenderImage, Task};
use std::{collections::BTreeSet, sync::Arc};

pub enum Gesture {
    Marquee {
        start: Point<Pixels>,
        additive: BTreeSet<String>,
    },
    Move {
        start: Point<Pixels>,
        track: usize,
    },
    Trim {
        start: Point<Pixels>,
        id: String,
        left: bool,
    },
    Library {
        start: Point<Pixels>,
        index: usize,
    },
    Horizontal {
        start: Point<Pixels>,
        offset: f32,
    },
    Vertical {
        start: Point<Pixels>,
        offset: f32,
    },
    Property {
        property: Property,
        bounds: Bounds<Pixels>,
    },
}

#[derive(Default)]
pub struct TimelineSession {
    pub model: Timeline,
    pub history: History,
    pub selected: BTreeSet<String>,
    pub gesture: Option<Gesture>,
    pub before: Option<Timeline>,
    pub pointer: Point<Pixels>,
    pub moved: bool,
    pub bounds: Bounds<Pixels>,
    pub header: f32,
    pub ruler: f32,
    pub scroll_x: f32,
    pub scroll_y: f32,
    pub content_height: f32,
    pub preview: bool,
    pub frame: Option<Arc<RenderImage>>,
    pub revision: u64,
    pub rendering: bool,
    pub playing: bool,
    pub sound: Option<crate::timeline_sound::TimelineSound>,
    pub play_revision: u64,
    pub timer: Option<Task<()>>,
    pub input: Option<(Property, Entity<TextInput>)>,
    pub input_blur: Option<gpui::Subscription>,
    pub rename: Option<(usize, Entity<TextInput>)>,
    pub scrub_preview: dinuosi_core::timeline::ScrubPreview,
}

impl Desktop {
    /// 把对数缩放值映射到每秒像素；最大每帧 60 像素，支持逐帧刻度。
    pub(crate) fn pixels_per_second(&self) -> f32 {
        8.0 * 225.0_f32.powf((self.zoom - 0.5) / 3.5)
    }
    /// 返回时间内容总宽度，始终保留至少两屏及最后片段后的可拖放空间。
    pub(crate) fn timeline_width(&self) -> f32 {
        let visible = self.timeline_view_width();
        visible
            .max(
                (self.timeline.model.end() as f32 / self.timeline.model.fps as f32 + 10.0)
                    * self.pixels_per_second(),
            )
            .max(visible * 2.0)
    }
    /// 返回排除轨道头、两端外置把手预留和竖向滚动条后的时间宽度，最少一个像素。
    pub(crate) fn timeline_view_width(&self) -> f32 {
        (f32::from(self.timeline.bounds.size.width)
            - self.timeline.header
            - 12.0
            - crate::trim_handle::WIDTH)
            .max(1.0)
    }
    /// 将窗口横坐标换算为吸附到项目帧的时间，包含水平滚动偏移。
    pub(crate) fn timeline_frame_at(&self, x: Pixels) -> i64 {
        self.timeline.model.frame(
            ((f32::from(x - self.timeline.bounds.left()) - self.timeline.header
                + self.timeline.scroll_x)
                / self.pixels_per_second()) as f64,
        )
    }
    /// 根据实际窗口高度按草图定位轨道；三行高度为字幕 34、媒体 58、主轨 64 的比例。
    pub(crate) fn track_geometry(&self, index: usize) -> (f32, f32) {
        let height = f32::from(self.timeline.bounds.size.height).max(240.0);
        let scale = (height / 354.0).max(0.8);
        let top = [126.0, 172.0, 238.0][index] * scale - self.timeline.scroll_y;
        (top, [34.0, 54.0, 58.0][index] * scale)
    }
    /// 命中当前鼠标所在轨道；空白区不创建额外轨道，不把字幕轨道作为媒体目标。
    pub(crate) fn track_at(&self, y: Pixels) -> Option<usize> {
        let y = f32::from(y - self.timeline.bounds.top());
        (0..3).find(|i| {
            let (top, height) = self.track_geometry(*i);
            y >= top && y <= top + height
        })
    }
}
