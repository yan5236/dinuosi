//! 桌面编辑会话仅保存指针、选择与绘制缓存；编辑规则和合成均委派 Core。
use crate::{input::TextInput, view::Desktop};
use dinuosi_core::timeline::{History, Property, Timeline};
use gpui::{Bounds, Entity, Pixels, Point, RenderImage, Task};
use std::{collections::BTreeSet, sync::Arc};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DropTarget {
    Track(usize),
    Insert(usize),
}

pub enum Gesture {
    Preview(crate::preview_transform::PreviewDrag),
    Track {
        start: Point<Pixels>,
        index: usize,
    },
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
    Volume {
        bounds: Bounds<Pixels>,
    },
    Property {
        property: Property,
        bounds: Bounds<Pixels>,
    },
}

#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub enum InspectorTab {
    #[default]
    Basic,
    Markers,
    Speed,
}

#[derive(Default)]
pub struct TimelineSession {
    pub model: Timeline,
    pub history: History,
    pub selected: BTreeSet<String>,
    pub keyframe_menu: Option<crate::timeline_keyframes::KeyframeMenu>,
    pub blade: bool,
    pub inspector_tab: InspectorTab,
    pub marker_menu: Option<crate::timeline_markers::MarkerMenu>,
    pub marker_dialog: Option<crate::marker_dialog::MarkerDialog>,
    pub blade_pointer: Option<Point<Pixels>>,
    pub gesture: Option<Gesture>,
    pub before: Option<Timeline>,
    pub pointer: Point<Pixels>,
    pub moved: bool,
    pub gesture_scroll: Point<f32>,
    pub scroll_task: Option<Task<()>>,
    pub bounds: Bounds<Pixels>,
    pub header: f32,
    pub ruler: f32,
    pub scroll_x: f32,
    pub scroll_y: f32,
    pub content_height: f32,
    pub preview: bool,
    pub preview_bounds: Bounds<Pixels>,
    pub frame: Option<Arc<RenderImage>>,
    pub revision: u64,
    pub rendering: bool,
    pub frame_cache: dinuosi_core::composite::FrameCache,
    pub playing: bool,
    pub sound: Option<crate::timeline_sound::TimelineSound>,
    pub play_revision: u64,
    pub timer: Option<Task<()>>,
    pub input: Option<(Property, Entity<TextInput>)>,
    pub volume_input: Option<Entity<TextInput>>,
    pub input_blur: Option<gpui::Subscription>,
    pub rename: Option<(usize, Entity<TextInput>)>,
    pub scrub_preview: dinuosi_core::timeline::ScrubPreview,
}

impl Desktop {
    /// 最小档按所有轨道最晚结束时间适配可见宽度，并留出 15% 尾部空白；空项目显示一分钟。
    /// 中间档对数插值到原有每秒 1800 像素上限；编辑手势使用按下快照，避免裁边时比例漂移。
    pub(crate) fn pixels_per_second(&self) -> f32 {
        let model = self
            .timeline
            .before
            .as_ref()
            .unwrap_or(&self.timeline.model);
        let duration = if model.end() > 0 {
            model.end() as f32 / model.fps as f32 * 1.15
        } else {
            60.0
        };
        let minimum = (self.timeline_view_width() / duration).min(1800.0);
        minimum * (1800.0 / minimum).powf(((self.zoom - 0.5) / 3.5).clamp(0.0, 1.0))
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
            .max(self.timeline.scroll_x + visible)
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
    /// 将按下位置按手势期间的滚动差换回当前窗口坐标；框选和片段位移始终锚定内容。
    pub(crate) fn timeline_gesture_start(&self, start: Point<Pixels>) -> Point<Pixels> {
        start
            - gpui::point(
                gpui::px(self.timeline.scroll_x - self.timeline.gesture_scroll.x),
                gpui::px(self.timeline.scroll_y - self.timeline.gesture_scroll.y),
            )
    }
    /// 按当前轨道数计算行位置；主轨保持下方，轨道增多后内容自然扩展，不预设三行。
    /// 返回面板内坐标及行高，包含当前竖向滚动偏移。
    pub(crate) fn track_geometry(&self, index: usize) -> (f32, f32) {
        let scale = (f32::from(self.timeline.bounds.size.height).max(240.0) / 354.0).max(0.8);
        let heights: Vec<_> = self
            .timeline
            .model
            .tracks
            .iter()
            .map(|t| {
                if t.kind == dinuosi_core::timeline::TrackKind::Subtitle {
                    34.0 * scale
                } else {
                    58.0 * scale
                }
            })
            .collect();
        let above: f32 = heights
            .iter()
            .take(heights.len().saturating_sub(1))
            .map(|h| h + 12.0)
            .sum();
        let first = (238.0 * scale - above).max(70.0);
        let top = first + heights.iter().take(index).map(|h| h + 12.0).sum::<f32>();
        (top - self.timeline.scroll_y, heights[index])
    }
    /// 返回实际轨道底部后的内容高度；单轨没有多余滚动范围，增删轨道后重新夹紧偏移。
    pub(crate) fn timeline_content_height(&self) -> f32 {
        let (top, height) = self.track_geometry(self.timeline.model.tracks.len() - 1);
        (top + self.timeline.scroll_y + height + 24.0 - 38.0).max(1.0)
    }
    /// 命中实际轨道行；字幕是否接收素材由核心类型约束决定。
    pub(crate) fn track_at(&self, y: Pixels) -> Option<usize> {
        let y = f32::from(y - self.timeline.bounds.top());
        (0..self.timeline.model.tracks.len()).find(|i| {
            let (top, height) = self.track_geometry(*i);
            y >= top + 6.0 && y <= top + height - 6.0
        })
    }
    /// 返回拖放目标：行内部放入现有轨道，上方空白和行间边界显示插轨线。
    /// 面板外或标尺/底部滚动条上没有有效落点；空白只在现有轨道上方创建新轨道。
    pub(crate) fn timeline_drop_target(&self, point: Point<Pixels>) -> Option<DropTarget> {
        if !self.timeline.bounds.contains(&point) {
            return None;
        }
        let y = f32::from(point.y - self.timeline.bounds.top());
        if y <= 38.0 || y >= f32::from(self.timeline.bounds.size.height) - 12.0 {
            return None;
        }
        if let Some(index) = self.track_at(point.y) {
            return Some(DropTarget::Track(index));
        }
        (0..self.timeline.model.tracks.len()).find_map(|i| {
            let (top, _) = self.track_geometry(i);
            (y <= top + 6.0).then_some(DropTarget::Insert(i))
        })
    }
}
