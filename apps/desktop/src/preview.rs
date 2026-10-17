//! 素材预览会话和控制条；选区只影响预览播放范围，不提前创建时间线轨道。

use crate::{
    input::TextInput,
    library::Filter,
    media::{self, Media, MediaKind},
    playback::Playback,
    sliders::Pointer,
    view::Desktop,
};
use gpui::{AppContext, Bounds, Context, Entity, Pixels, RenderImage, Task, Window};
use std::{collections::HashMap, sync::Arc, time::Duration};

pub struct EditorSession {
    pub media: Vec<Media>,
    pub media_menu: Option<(crate::media_actions::LibraryTarget, gpui::Point<Pixels>)>,
    pub search: Entity<TextInput>,
    pub filter: Filter,
    pub selected: Option<usize>,
    pub frame: Option<Arc<RenderImage>>,
    pub retired_frames: Vec<Arc<RenderImage>>,
    pub strip: Vec<Arc<RenderImage>>,
    pub monitor: Option<Task<()>>,
    pub waveform_task: Option<Task<()>>,
    pub position: f64,
    pub trim_start: f64,
    pub trim_end: f64,
    pub timeline_position: f64,
    pub player: Option<Playback>,
    pub importing: bool,
    pub bounds: HashMap<Pointer, Bounds<Pixels>>,
    pub drag: Option<Pointer>,
    pub resume_after_drag: bool,
    revision: u64,
    frame_busy: bool,
    pub starting: bool,
    timer: Option<Task<()>>,
}

impl EditorSession {
    /// 初始化独立编辑会话；没有媒体时选区和播放位置均为零，搜索使用原生输入实体。
    pub fn new(cx: &mut Context<Desktop>) -> Self {
        Self {
            media: Vec::new(),
            media_menu: None,
            search: crate::library::search(cx),
            filter: Filter::All,
            selected: None,
            frame: None,
            retired_frames: Vec::new(),
            strip: Vec::new(),
            monitor: None,
            waveform_task: None,
            position: 0.0,
            trim_start: 0.0,
            trim_end: 0.0,
            timeline_position: 0.0,
            player: None,
            importing: false,
            bounds: HashMap::new(),
            drag: None,
            resume_after_drag: false,
            revision: 0,
            frame_busy: false,
            starting: false,
            timer: None,
        }
    }
    /// 返回当前选中素材的固有时长（图片默认五秒）；没有选择时返回零。
    pub fn duration(&self) -> f64 {
        self.selected
            .and_then(|i| self.media.get(i))
            .map_or(0.0, |m| m.duration)
    }
}

impl Desktop {
    /// 切换素材并复位预览选区，清除时间线选择与检查器主对象，停止旧音频和解码；不修改项目引用。
    pub(crate) fn select_media(
        &mut self,
        index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.commit_inspector_input(cx);
        self.stop_preview();
        self.timeline.preview = false;
        self.stop_timeline();
        self.timeline.selected.clear();
        self.timeline.primary_clip = None;
        let Some(media) = self.session.media.get(index) else {
            return;
        };
        self.session.selected = Some(index);
        self.session.position = 0.0;
        self.session.trim_start = 0.0;
        self.session.trim_end = media.duration;
        if let Some(old) = std::mem::replace(&mut self.session.frame, media.thumbnail.clone()) {
            self.session.retired_frames.push(old);
        }
        self.session.retired_frames.append(&mut self.session.strip);
        self.session.strip = media.thumbnail.iter().cloned().collect();
        if media.kind == MediaKind::Video && !media.offline && media.thumbnail.is_some() {
            self.load_strip(media.clone(), window, cx);
        }
        self.notice = None;
        cx.notify();
    }

    /// 后台抽取八个等间距的真实帧，绘制预览片段；切换素材后的结果丢弃，不重复伪造首帧。
    /// 仅保留当前片段帧，解码失败显示实际错误，不写入源文件或项目目录。
    fn load_strip(&self, media: Media, window: &mut Window, cx: &mut Context<Self>) {
        let path = media.path.clone();
        cx.spawn_in(window, async move |this, cx| {
            let result = cx
                .background_spawn(async move {
                    (0..8)
                        .map(|index| {
                            media::frame(
                                &media.path,
                                media.duration * index as f64 / 8.0,
                                media.preview_size,
                            )
                        })
                        .collect::<Result<Vec<_>, _>>()
                })
                .await;
            let _ = this.update_in(cx, |this, _, cx| {
                if !this
                    .session
                    .selected
                    .and_then(|i| this.session.media.get(i))
                    .is_some_and(|m| m.path == path && !m.offline)
                {
                    return;
                }
                match result {
                    Ok(strip) => {
                        this.session.retired_frames.append(&mut this.session.strip);
                        this.session.strip = strip;
                    }
                    Err(error) => this.notice = Some(error),
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// 停止预览并保留最后设备位置，撤销未完成的播放初始化；销毁播放器结束源文件读取。
    pub(crate) fn stop_preview(&mut self) {
        if let Some(player) = self.session.player.take() {
            self.session.position = player
                .position()
                .clamp(self.session.trim_start, self.session.trim_end);
        }
        self.session.revision += 1;
        self.session.starting = false;
        self.session.timer = None;
    }

    /// 启动或暂停真实素材播放；解码器在后台准备，音频设备在前台创建，迟到的解码器立即销毁。
    /// 到达选区末尾时从起点重播；切换/取消不打开设备，设备和解码错误保留原反馈。
    pub(crate) fn play_preview(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.session.player.is_some() || self.session.starting {
            self.stop_preview();
            cx.notify();
            return;
        }
        let Some(media) = self
            .session
            .selected
            .and_then(|i| self.session.media.get(i))
            .cloned()
        else {
            return;
        };
        if media.offline {
            return;
        }
        if self.session.position >= self.session.trim_end - 0.01 {
            self.session.position = self.session.trim_start;
        }
        let position = self.session.position;
        let end = self.session.trim_end;
        self.session.starting = true;
        self.session.revision += 1;
        let revision = self.session.revision;
        cx.spawn_in(window, async move |this, cx| {
            let result = cx
                .background_spawn(async move {
                    dinuosi_core::media_stream::Decoder::start(&media.source, position, end)
                })
                .await;
            let _ = this.update_in(cx, |this, window, cx| {
                if this.session.revision != revision {
                    return;
                }
                this.session.starting = false;
                match result.and_then(|decoder| Playback::start(decoder, position, this.volume)) {
                    Ok(player) => {
                        this.session.player = Some(player);
                        this.start_preview_timer(window, cx);
                    }
                    Err(error) => this.notice = Some(error),
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    /// 以约 30Hz 拉取视频帧和设备时钟；去掉解码补边并淘汰旧 GPU 帧，末尾或错误时停止。
    fn start_preview_timer(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.session.timer = Some(cx.spawn_in(window, async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(33))
                    .await;
                let keep = this.update_in(cx, |this, window, cx| {
                    let Some(player) = &this.session.player else {
                        return false;
                    };
                    let error = player.error.lock().unwrap().take();
                    let frame = player.frames.lock().unwrap().take();
                    this.session.position = player.position().min(this.session.trim_end);
                    if let Some(frame) = frame {
                        let size = this
                            .session
                            .selected
                            .and_then(|i| this.session.media.get(i))
                            .map_or((media::FRAME_WIDTH, media::FRAME_HEIGHT), |m| {
                                m.preview_size
                            });
                        this.replace_frame(media::render_frame(frame, size), window);
                    }
                    if let Some(error) = error {
                        this.notice = Some(error);
                        this.session.player = None;
                        cx.notify();
                        return false;
                    }
                    if this.session.position >= this.session.trim_end - 0.001 {
                        this.session.position = this.session.trim_end;
                        this.session.player = None;
                        cx.notify();
                        return false;
                    }
                    cx.notify();
                    true
                });
                if !matches!(keep, Ok(true)) {
                    break;
                }
            }
        }));
    }

    /// 串行后台解码寻址帧；快速拖动只保留最新请求，旧帧不会覆盖新位置或新素材。
    pub(crate) fn request_frame(&mut self, cx: &mut Context<Self>) {
        if self.session.frame_busy {
            return;
        }
        let Some(media) = self
            .session
            .selected
            .and_then(|i| self.session.media.get(i))
            .filter(|m| m.kind == MediaKind::Video && !m.offline)
            .cloned()
        else {
            return;
        };
        let position = self.session.position;
        self.session.frame_busy = true;
        cx.spawn(async move |this, cx| {
            let path = media.path.clone();
            let duration = media.duration;
            let preview_size = media.preview_size;
            let result = cx
                .background_spawn(async move {
                    media::frame(
                        &path,
                        position.min((duration - 1.0 / 24.0).max(0.0)),
                        preview_size,
                    )
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.session.frame_busy = false;
                let current = this
                    .session
                    .selected
                    .and_then(|i| this.session.media.get(i));
                let same = current.is_some_and(|m| m.path == media.path && !m.offline)
                    && (this.session.position - position).abs() < 0.001;
                if same && this.session.player.is_none() {
                    match result {
                        Ok(frame) => {
                            if let Some(old) = this.session.frame.replace(frame) {
                                this.session.retired_frames.push(old);
                            }
                        }
                        Err(error) => this.notice = Some(error),
                    }
                    cx.notify();
                } else if this.session.player.is_none() {
                    this.request_frame(cx);
                }
            });
        })
        .detach();
    }

    /// 替换动态画面并回收上一次视频帧的显卡纹理；素材缩略图仍由素材库持有。
    fn replace_frame(&mut self, frame: Arc<RenderImage>, window: &mut Window) {
        if let Some(old) = self.session.frame.replace(frame)
            && !self
                .session
                .media
                .iter()
                .any(|m| m.thumbnail.as_ref().is_some_and(|t| t.id == old.id))
        {
            let _ = window.drop_image(old);
        }
    }
}
