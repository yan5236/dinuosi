//! 素材、片段和轨道拖动：根层浮块随鼠标移动，落点规则与释放提交共用核心模型。
use crate::{
    timeline_state::{DropTarget, Gesture},
    ui::{LINE, TEXT},
    view::Desktop,
};
use dinuosi_core::timeline::{Timeline, TrackKind};
use dinuosi_icons::Icon;
use gpui::{
    Context, Div, MouseDownEvent, ObjectFit, Pixels, Point, Window, canvas, div, img, prelude::*,
    px, rgb, svg,
};
impl Desktop {
    /// 从轨道名称开始整体拖动；主轨固定在底部，自由轨道的片段和状态随轨道一起移动。
    /// 保存按下快照供释放时单次持久化和撤销，锁定轨道和输入状态不启动手势。
    pub(crate) fn begin_track_drag(
        &mut self,
        index: usize,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.busy
            || self.timeline.rename.is_some()
            || index == self.timeline.model.main_track()
            || self.timeline.model.tracks[index].locked
        {
            return;
        }
        self.commit_inspector_input(cx);
        self.stop_timeline();
        self.focus_handle.focus(window);
        self.timeline.before = Some(self.timeline.model.clone());
        self.timeline.gesture = Some(Gesture::Track {
            start: event.position,
            index,
        });
        self.timeline.pointer = event.position;
        self.timeline.moved = false;
        cx.stop_propagation();
        cx.notify();
    }

    /// 从手势快照计算最终片段落点，插轨会先重映射原始索引，再由核心处理冲突。
    /// 片段间剩余空隙足够时保留后续位置，不足才后移；返回候选模型供提示和释放共用，不修改状态。
    /// 窗口外、字幕、锁轨、类型不匹配或非法跨轨组返回 None。
    pub(crate) fn clip_drop_model(
        &self,
        before: &Timeline,
        start: Point<Pixels>,
        origin: usize,
    ) -> Option<Timeline> {
        if self.timeline.selected.is_empty()
            || f32::from(self.timeline.pointer.x - self.timeline.bounds.left())
                < self.timeline.header
        {
            return None;
        }
        let target = self.timeline_drop_target(self.timeline.pointer)?;
        let mut next = before.clone();
        let delta = self.clip_move_delta(before, start);
        let frame = before
            .clips
            .iter()
            .find(|c| self.timeline.selected.contains(&c.id))?
            .start
            + delta;
        if let DropTarget::Track(track) = target
            && self.timeline.selected.len() == 1
            && let Some(frame) =
                self.timeline_clip_insertion(before, track, frame, &self.timeline.selected)
        {
            next.move_clip_ripple(self.timeline.selected.first()?, track, frame)
                .ok()?;
            return Some(next);
        }
        let (target, origin) = match target {
            DropTarget::Track(index) => (index, origin),
            DropTarget::Insert(index) => {
                next.create_track(
                    index,
                    TrackKind::for_media(
                        before
                            .clips
                            .iter()
                            .find(|c| self.timeline.selected.contains(&c.id))?
                            .kind,
                    ),
                )
                .ok()?;
                (index, origin + usize::from(origin >= index))
            }
        };
        next.move_clips(
            &self.timeline.selected,
            delta,
            target as isize - origin as isize,
        )
        .ok()?;
        Some(next)
    }

    /// 把拟放置首帧换算为核心片段间插入落点；移动保留按下偏移，并应用统一磁吸，排除正在移动的片段。
    /// 库素材传空集合，已有单片段移动传当前选择；其他位置保持普通拖放/冲突分轨规则。
    pub(crate) fn timeline_clip_insertion(
        &self,
        model: &Timeline,
        track: usize,
        frame: i64,
        exclude: &std::collections::BTreeSet<String>,
    ) -> Option<i64> {
        if model
            .tracks
            .get(track)
            .is_none_or(|t| t.locked || t.kind == TrackKind::Subtitle)
        {
            return None;
        }
        model.insertion_frame(track, frame, 0, exclude)
    }

    /// 在根层绘制库素材缩略图、保留按下偏移的二维片段浮块或轨道名称；不受轨道内容裁切。
    /// 仅超过拖动阈值时显示，库素材拖动隐藏系统光标；释放后浮块和光标请求自动移除。
    pub(crate) fn library_drag_overlay(&self) -> Option<Div> {
        if !self.timeline.moved {
            return None;
        }
        if let Some(Gesture::Move { start, .. }) = self.timeline.gesture.as_ref() {
            let start = self.timeline_gesture_start(*start);
            let before = self.timeline.before.as_ref()?;
            return Some(
                div().absolute().inset_0().children(
                    before
                        .clips
                        .iter()
                        .filter(|c| self.timeline.selected.contains(&c.id))
                        .map(|clip| {
                            let (top, h) = self.track_geometry(clip.track);
                            let x = clip.start as f32 / before.fps as f32
                                * self.pixels_per_second()
                                - self.timeline.scroll_x;
                            let image = self
                                .session
                                .media
                                .iter()
                                .find(|m| m.path == clip.source)
                                .and_then(|m| m.thumbnail.clone());
                            div()
                                .debug_selector(|| "timeline-drag-ghost".into())
                                .absolute()
                                .left(
                                    self.timeline.bounds.left()
                                        + px(self.timeline.header + x)
                                        + self.timeline.pointer.x
                                        - start.x,
                                )
                                .top(
                                    self.timeline.bounds.top()
                                        + px(top + 4.0)
                                        + self.timeline.pointer.y
                                        - start.y,
                                )
                                .w(px(clip.duration as f32 / before.fps as f32
                                    * self.pixels_per_second()))
                                .h(px(h - 8.0))
                                .rounded(px(4.0))
                                .overflow_hidden()
                                .border_1()
                                .border_color(rgb(TEXT))
                                .bg(rgb(LINE))
                                .opacity(0.85)
                                .when_some(image, |s, image| {
                                    s.child(img(image).size_full().object_fit(ObjectFit::Cover))
                                })
                                .child(
                                    div()
                                        .absolute()
                                        .left(px(7.0))
                                        .top(px(3.0))
                                        .text_size(px(11.0))
                                        .child(clip.name.clone()),
                                )
                        }),
                ),
            );
        }
        if let Some(Gesture::Track { index, .. }) = self.timeline.gesture.as_ref() {
            return Some(
                div()
                    .absolute()
                    .left(self.timeline.pointer.x - px(35.0))
                    .top(self.timeline.pointer.y - px(18.0))
                    .px(px(12.0))
                    .py(px(8.0))
                    .bg(rgb(LINE))
                    .border_1()
                    .border_color(rgb(TEXT))
                    .child(self.timeline.model.tracks[*index].name.clone()),
            );
        }
        let Gesture::Library { index, .. } = self.timeline.gesture.as_ref()? else {
            return None;
        };
        let media = self.session.media.get(*index)?;
        Some(
            div()
                .absolute()
                .left(self.timeline.pointer.x - px(35.0))
                .top(self.timeline.pointer.y - px(22.0))
                .w(px(100.0))
                .h(px(62.0))
                .rounded(px(4.0))
                .overflow_hidden()
                .border_1()
                .border_color(rgb(TEXT))
                .bg(rgb(LINE))
                .opacity(0.85)
                .when_some(media.thumbnail.clone(), |s, image| {
                    s.child(img(image).size_full().object_fit(ObjectFit::Contain))
                })
                .when(media.thumbnail.is_none(), |s| {
                    s.flex().items_center().justify_center().child(
                        svg()
                            .path(Icon::Audio.path())
                            .size(px(28.0))
                            .text_color(rgb(TEXT)),
                    )
                })
                .child(
                    canvas(
                        |_, _, _| (),
                        |_, _, window, _| window.set_window_cursor_style(gpui::CursorStyle::None),
                    )
                    .absolute()
                    .inset_0(),
                ),
        )
    }
}
