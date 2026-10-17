//! 原尺寸时间线渲染；正放顺序解码，倒放/定格按源帧寻址，变换与预览共用像素合成。
use crate::{
    composite,
    export::ExportSettings,
    export_cancel::{CancellationList, register},
    media::{Media, MediaKind},
    timeline::{Clip, Timeline},
};
use std::{
    collections::BTreeMap,
    sync::{Arc, atomic::AtomicBool},
};

struct Source {
    decoder: dinuosi_media::Decoder,
    pixels: Vec<u8>,
    size: (u32, u32),
    index: i64,
    image: bool,
    path: std::path::PathBuf,
    filter: String,
    fps: u32,
    cancel: Arc<AtomicBool>,
}

impl Source {
    /// 从片段首帧对应源位置创建原生 BGRA 解码，计入倒放、定格、旋转及项目帧率。
    /// 图片仅消费首帧，原生上下文独占；取消句柄登记到本次导出，失败不写源文件。
    fn start(
        clip: &Clip,
        media: &Media,
        timeline: &Timeline,
        sessions: &CancellationList,
    ) -> Result<Self, String> {
        let size = media.source_size;
        let cancel = Arc::new(AtomicBool::new(false));
        register(sessions, &cancel);
        let first = if media.kind == MediaKind::Video {
            clip.source_frame(clip.start)
        } else {
            0
        };
        let filter = format!(
            "scale={}:{}:flags=bicubic,setsar=1,format=bgra,fps={}",
            size.0, size.1, timeline.fps
        );
        let decoder = dinuosi_media::Decoder::open(
            &clip.source,
            false,
            first as f64 / timeline.fps as f64,
            1e12,
            &filter,
            cancel.clone(),
        )?;
        Ok(Self {
            decoder,
            pixels: vec![0; size.0 as usize * size.1 as usize * 4],
            size,
            index: first - 1,
            image: media.kind == MediaKind::Image,
            path: clip.source.clone(),
            filter,
            fps: timeline.fps,
            cancel,
        })
    }

    /// 读取绝对源帧；倒放时重新寻址，正放继续顺序解码，定格与重复输出帧复用当前像素。
    /// 复用原尺寸缓冲，低帧率源 EOF 保留尾帧；部分帧、取消及真实解码错误终止导出。
    fn read(&mut self, index: i64) -> Result<&[u8], String> {
        let target = if self.image { 0 } else { index };
        if target < self.index {
            // shortcut: 倒放逐帧寻址可能较慢，需要提升长视频导出速度时改用有内存上限的分块缓存。
            self.seek(target)?;
        }
        let mut decoded = false;
        let mut fallback = false;
        while self.index < target {
            let count = self.decoder.read(&mut self.pixels)?;
            if count == 0 {
                if !decoded && !fallback && !self.image && target > 0 {
                    self.seek((target - self.fps as i64).max(0))?;
                    fallback = true;
                    continue;
                }
                if !decoded {
                    return Err("素材没有可导出的画面".into());
                }
                self.index = target;
                break;
            }
            if count != self.pixels.len() {
                return Err("素材画面不完整".into());
            }
            self.index += 1;
            decoded = true;
        }
        Ok(&self.pixels)
    }

    /// 重新打开指定源帧的原生解码器，复用滤镜、取消句柄和像素缓冲；打开失败不返回旧画面。
    /// 尾帧无输出时由调用方向前一秒顺序解码，避免低帧率素材定格或倒放首帧变黑。
    fn seek(&mut self, frame: i64) -> Result<(), String> {
        self.decoder = dinuosi_media::Decoder::open(
            &self.path,
            false,
            frame as f64 / self.fps as f64,
            1e12,
            &self.filter,
            self.cancel.clone(),
        )?;
        self.index = frame - 1;
        Ok(())
    }
}

pub struct Renderer {
    sources: BTreeMap<String, Source>,
    output: Vec<u8>,
}

impl Renderer {
    /// 创建不持有原生解码句柄的渲染会话；每个活动片段按需打开，结束后立即释放。
    pub fn new() -> Self {
        Self {
            sources: BTreeMap::new(),
            output: Vec::new(),
        }
    }

    /// 在指定项目帧先应用共享调色再合成导出画面，保持轨道顺序、隐藏、选区方向、定格源帧及画面关键帧。
    /// 按项目素材适配规则计算覆盖尺寸，与预览一致；输出比例不同时居中适配整个项目画布。
    /// 复用输出缓冲并重置为黑色不透明底，返回切片在下一次渲染前有效；失败返回错误，不静默跳过。
    pub fn render(
        &mut self,
        timeline: &Timeline,
        frame: f64,
        media: &[Media],
        settings: &ExportSettings,
        sessions: &CancellationList,
    ) -> Result<&[u8], String> {
        self.sources.retain(|id, _| {
            timeline.clips.iter().any(|clip| {
                clip.id == *id
                    && frame >= clip.start as f64
                    && frame < (clip.start + clip.duration) as f64
            })
        });
        self.output
            .resize(settings.width as usize * settings.height as usize * 4, 0);
        let output = &mut self.output;
        for pixel in output.as_chunks_mut::<4>().0 {
            pixel.copy_from_slice(&[0, 0, 0, 255]);
        }
        for track in (0..timeline.tracks.len())
            .rev()
            .filter(|&track| timeline.tracks[track].visible)
        {
            for clip in timeline.clips.iter().filter(|clip| {
                clip.track == track
                    && clip.kind != MediaKind::Audio
                    && frame >= clip.start as f64
                    && frame < (clip.start + clip.duration) as f64
            }) {
                if clip.kind == MediaKind::Text {
                    crate::text_render::blend(
                        output,
                        (settings.width, settings.height),
                        clip,
                        timeline,
                        frame,
                    )?;
                    continue;
                }
                let source_media = media
                    .iter()
                    .find(|m| m.path == clip.source)
                    .ok_or("导出素材信息缺失")?;
                if !self.sources.contains_key(&clip.id) {
                    self.sources.insert(
                        clip.id.clone(),
                        Source::start(clip, source_media, timeline, sessions)?,
                    );
                }
                let source = self.sources.get_mut(&clip.id).unwrap();
                let size = source.size;
                let pixels = source.read(clip.source_frame(frame.floor() as i64))?;
                let graded = crate::color_render::apply(pixels, size, size, &clip.color_at(frame))?;
                composite::blend_pixels(
                    output,
                    (settings.width, settings.height),
                    &graded,
                    size,
                    &clip.transform_at(frame),
                    (timeline.width, timeline.height),
                    timeline
                        .media_fit
                        .size(source_media.source_size, (timeline.width, timeline.height)),
                )?;
            }
        }
        Ok(output)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{export::Encoder, timeline::Timeline};
    /// 同一输出尺寸下文字预览与导出逐字节一致，含中文斜体、多行、位置与旋转；无需媒体源。
    #[test]
    fn text_preview_and_export_use_identical_pixels() {
        let mut timeline = Timeline {
            width: 640,
            height: 360,
            ..Default::default()
        };
        let mut text = crate::text::Text::new("专业文本\nTitle 01".into(), (640, 360), false);
        text.style.italic = true;
        text.style.size = 32.0;
        text.style.outline = 2;
        text.style.shadow = Some(Default::default());
        text.style.background = Some(Default::default());
        timeline.add_text(0, 90, text).unwrap();
        timeline.clips[0].transform.x = 33.0;
        timeline.clips[0].transform.rotation = 12.0;
        let expected = crate::composite::render(&timeline, 0, &[]).unwrap();
        let mut renderer = Renderer::new();
        let settings = ExportSettings::for_timeline(&timeline, Encoder::Mpeg4);
        let frame = renderer
            .render(&timeline, 0.0, &[], &settings, &CancellationList::default())
            .unwrap();
        assert_eq!(frame, expected);
        timeline.tracks[0].visible = false;
        assert!(
            renderer
                .render(&timeline, 0.0, &[], &settings, &CancellationList::default())
                .unwrap()
                .as_chunks::<4>()
                .0
                .iter()
                .all(|p| *p == [0, 0, 0, 255])
        );
    }
    /// 连续帧复用同一输出分配且清除旧颜色；改变输出尺寸时长度与黑色底保持正确。
    #[test]
    fn renderer_reuses_and_clears_output_between_frames() {
        let mut renderer = Renderer::new();
        let timeline = Timeline::default();
        let mut settings = ExportSettings::for_timeline(&timeline, Encoder::Av1);
        settings.width = 128;
        settings.height = 128;
        let sessions = CancellationList::default();
        let first = renderer
            .render(&timeline, 0.0, &[], &settings, &sessions)
            .unwrap()
            .as_ptr();
        renderer.output.fill(12);
        let frame = renderer
            .render(&timeline, 1.0, &[], &settings, &sessions)
            .unwrap();
        assert_eq!(first, frame.as_ptr());
        assert!(
            frame
                .as_chunks::<4>()
                .0
                .iter()
                .all(|p| *p == [0, 0, 0, 255])
        );
        settings.width = 64;
        assert_eq!(
            renderer
                .render(&timeline, 2.0, &[], &settings, &sessions)
                .unwrap()
                .len(),
            64 * 128 * 4
        );
    }
}
