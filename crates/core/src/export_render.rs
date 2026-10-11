//! 原尺寸时间线渲染；源片段顺序解码，变换与预览使用同一个像素合成入口。
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
}

impl Source {
    /// 从裁剪入点创建原生 BGRA 顺序解码，计入旋转/像素比例并按项目帧率输出。
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
        let offset = if media.kind == MediaKind::Video {
            clip.source_in as f64 / timeline.fps as f64
        } else {
            0.0
        };
        let filter = format!(
            "scale={}:{}:flags=bicubic,setsar=1,format=bgra,fps={}",
            size.0, size.1, timeline.fps
        );
        let decoder =
            dinuosi_media::Decoder::open(&clip.source, false, offset, 1e12, &filter, cancel)?;
        Ok(Self {
            decoder,
            pixels: vec![0; size.0 as usize * size.1 as usize * 4],
            size,
            index: -1,
            image: media.kind == MediaKind::Image,
        })
    }

    /// 顺序读取到目标项目源帧，低帧率素材尾端重复最后有效画面，与预览尾帧语义一致。
    /// 复用原尺寸像素缓冲；正常 EOF 保留尾帧，部分帧或真实原生解码错误终止导出。
    fn read(&mut self, index: i64) -> Result<&[u8], String> {
        let target = if self.image { 0 } else { index };
        while self.index < target {
            let count = self.decoder.read(&mut self.pixels)?;
            if count == 0 {
                if self.index < 0 {
                    return Err("素材没有可导出的画面".into());
                }
                self.index = target;
                break;
            }
            if count != self.pixels.len() {
                return Err("素材画面不完整".into());
            }
            self.index += 1;
        }
        Ok(&self.pixels)
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

    /// 在指定项目帧合成完整导出画面，保持轨道顺序、隐藏、入点和所有画面关键帧。
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
                let pixels = source.read((frame - clip.start as f64).floor() as i64)?;
                composite::blend_pixels(
                    output,
                    (settings.width, settings.height),
                    pixels,
                    size,
                    &clip.transform_at(frame),
                    (timeline.width, timeline.height),
                    composite::fitted_size(
                        source_media.source_size,
                        (timeline.width, timeline.height),
                    ),
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
