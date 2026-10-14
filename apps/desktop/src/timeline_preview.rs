//! 核心时间线合成的异步适配，串行显示完成帧并合并后续请求，避免连续寻址饥饿。
use crate::view::Desktop;
use gpui::{AppContext, Context, Window};
use std::time::{Duration, Instant};

impl Desktop {
    /// 标记合成请求版本并请求最新画面；关闭旧关键帧和标记菜单，防止模型/帧率更新后使用过期目标。
    /// 当前解码期间合并后续请求，避免无界任务队列。
    pub(crate) fn refresh_timeline(&mut self, cx: &mut Context<Self>) {
        self.timeline.keyframe_menu = None;
        self.timeline.marker_menu = None;
        self.timeline.revision += 1;
        self.request_timeline_frame(cx);
        cx.notify();
    }
    /// 后台串行复用 Core 源帧缓存合成，主线程仅上传 BGRA；变换不重复解码未改变的源帧。
    /// 任务独占缓存并在完成后归还；按请求时项目比例裁切画布，连续寻址追赶最新位置。
    /// HSL 取样期间为显示帧保留带版本/时间身份的 CPU 像素，正常预览不额外复制。
    fn request_timeline_frame(&mut self, cx: &mut Context<Self>) {
        if !self.timeline.preview || self.timeline.rendering {
            return;
        }
        self.timeline.rendering = true;
        let mut cache = std::mem::take(&mut self.timeline.frame_cache);
        let revision = self.timeline.revision;
        let model = self.timeline.model.clone();
        let frame = model.frame(self.session.timeline_position);
        let canvas = (model.width, model.height);
        let scopes = self.timeline.color.scopes;
        let media = self
            .session
            .media
            .iter()
            .map(|m| m.source.clone())
            .collect::<Vec<_>>();
        cx.spawn(async move |this, cx| {
            let (result, cache, scopes) = cx
                .background_spawn(async move {
                    let result = cache.render(&model, frame, &media);
                    let scope = result
                        .as_ref()
                        .ok()
                        .filter(|_| scopes)
                        .and_then(|bytes| crate::color_scopes::scope_pixels(bytes, canvas));
                    (result, cache, scope)
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.timeline.rendering = false;
                this.timeline.frame_cache = cache;
                // 串行合成保证完成顺序；连续拖动时先显示刚完成的中间帧，再追赶最新位置。
                // 若只接受完全相同版本，持续移动会一直丢帧，退化成松开后才预览。
                if this.timeline.preview {
                    match result {
                        Ok(bytes) => {
                            this.timeline.color.pick_frame =
                                this.timeline.color.picker.as_ref().map(|_| {
                                    crate::color_hsl::PickFrame {
                                        pixels: bytes.clone(),
                                        size: dinuosi_core::media::preview_dimensions(canvas),
                                        frame,
                                        revision,
                                    }
                                });
                            if this.timeline.color.scopes {
                                let image = scopes.map(|bytes| {
                                    std::sync::Arc::new(gpui::RenderImage::new([
                                        image::Frame::new(
                                            image::RgbaImage::from_raw(
                                                crate::color_scopes::SIZE.0,
                                                crate::color_scopes::SIZE.1,
                                                bytes,
                                            )
                                            .unwrap(),
                                        ),
                                    ]))
                                });
                                if let Some(old) =
                                    std::mem::replace(&mut this.timeline.color.scope_image, image)
                                {
                                    this.session.retired_frames.push(old);
                                }
                            }
                            let frame = crate::media::render_frame(
                                bytes,
                                dinuosi_core::media::preview_dimensions(canvas),
                            );
                            if let Some(old) = this.timeline.frame.replace(frame) {
                                this.session.retired_frames.push(old);
                            }
                        }
                        Err(error) => {
                            this.cancel_hsl_picker();
                            this.notice = Some(error);
                        }
                    }
                    cx.notify();
                }
                if this.timeline.revision != revision {
                    this.request_timeline_frame(cx);
                }
            });
        })
        .detach();
    }
    /// 更新播放头并按宿主选项执行连续/释放寻址；帧取整和位置限制统一使用核心时间。
    pub(crate) fn seek_timeline(&mut self, seconds: f64, release: bool, cx: &mut Context<Self>) {
        self.stop_timeline();
        self.timeline.preview = true;
        self.stop_preview();
        self.session.timeline_position =
            self.timeline.model.frame(seconds) as f64 / self.timeline.model.fps as f64;
        if release || self.timeline.scrub_preview == dinuosi_core::timeline::ScrubPreview::Live {
            self.refresh_timeline(cx);
        }
        cx.notify();
    }
    /// 停止时间线设备和计时器，并使迟到的后台初始化失效；不改变当前播放头。
    pub(crate) fn stop_timeline(&mut self) {
        self.timeline.play_revision += 1;
        self.timeline.playing = false;
        self.timeline.timer = None;
        self.timeline.sound = None;
    }
    /// 在后台启动核心混音，再由设备消费时钟推进视频合成；初始化失败保留当前位置。
    pub(crate) fn play_timeline(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.cancel_hsl_picker();
        if self.timeline.playing {
            self.stop_timeline();
            cx.notify();
            return;
        }
        let end = self.timeline.model.end() as f64 / self.timeline.model.fps as f64;
        if end <= 0.0 {
            return;
        }
        self.stop_preview();
        self.stop_timeline();
        if self.session.timeline_position >= end {
            self.session.timeline_position = 0.0;
        }
        self.timeline.playing = true;
        let start = self.session.timeline_position;
        let model = self.timeline.model.clone();
        let sources = self
            .session
            .media
            .iter()
            .map(|m| m.source.clone())
            .collect::<Vec<_>>();
        let revision = self.timeline.play_revision;
        cx.spawn_in(window, async move |this, cx| {
            let result = cx
                .background_spawn(async move {
                    dinuosi_core::timeline_audio::TimelineAudio::start(&model, start, &sources)
                })
                .await;
            let _ = this.update_in(cx, |this, window, cx| {
                if !this.timeline.playing || this.timeline.play_revision != revision {
                    return;
                }
                match result
                    .and_then(|audio| crate::timeline_sound::TimelineSound::new(audio, this.volume))
                {
                    Ok(sound) => {
                        this.timeline.sound = Some(sound);
                        this.run_timeline_clock(start, end, window, cx);
                    }
                    Err(error) => {
                        this.stop_timeline();
                        this.notice = Some(error);
                    }
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }
    /// 每次设备时钟推进时请求最新合成帧；音频队列耗尽时精确停在项目终点，避免非整秒时钟舍入后永不结束。
    /// 视频渲染跟不上时合并请求，保持声音连续；尾部错误先报告，再释放设备与计时器。
    fn run_timeline_clock(
        &mut self,
        start: f64,
        end: f64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let clock = Instant::now();
        self.timeline.timer = Some(cx.spawn_in(window, async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(33))
                    .await;
                let keep = this.update(cx, |this, cx| {
                    if !this.timeline.playing {
                        return false;
                    }
                    let elapsed = this
                        .timeline
                        .sound
                        .as_ref()
                        .map_or_else(|| clock.elapsed(), |s| s.sink.get_pos());
                    let error = this
                        .timeline
                        .sound
                        .as_ref()
                        .and_then(|s| s.error.lock().unwrap().take());
                    if let Some(error) = error {
                        this.timeline.sound = None;
                        this.timeline.playing = false;
                        this.notice = Some(error);
                        cx.notify();
                        return false;
                    }
                    let finished = this.timeline.sound.as_ref().is_some_and(|s| s.sink.empty());
                    this.session.timeline_position = if finished {
                        end
                    } else {
                        (start + elapsed.as_secs_f64()).min(end)
                    };
                    this.refresh_timeline(cx);
                    this.timeline.playing = this.session.timeline_position < end;
                    if !this.timeline.playing {
                        this.timeline.sound = None;
                    }
                    this.timeline.playing
                });
                if !matches!(keep, Ok(true)) {
                    break;
                }
            }
        }));
    }
}
