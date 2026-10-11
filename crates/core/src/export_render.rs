//! 原尺寸时间线渲染；源片段顺序解码，变换与预览使用同一个像素合成入口。
use crate::{
    composite,
    export::ExportSettings,
    export_process::{Process, ProcessList},
    media::{self, Media, MediaKind},
    timeline::{Clip, Timeline},
};
use std::{
    collections::BTreeMap,
    io::{BufReader, Read},
    process::{ChildStdout, Stdio},
};

struct Source {
    process: Process,
    reader: BufReader<ChildStdout>,
    pixels: Vec<u8>,
    size: (u32, u32),
    index: i64,
    image: bool,
}

impl Source {
    /// 从裁剪入点启动原显示比例 BGRA 顺序流，以项目帧率输出；图片只读取原尺寸一帧。
    /// 保留 FFmpeg 自动旋转并归一非方形像素，失败通过统一进程诊断返回。
    fn start(
        clip: &Clip,
        media: &Media,
        timeline: &Timeline,
        processes: &ProcessList,
    ) -> Result<Self, String> {
        let size = media.source_size;
        let mut command = media::command("ffmpeg");
        command.args(["-v", "error", "-threads", "1", "-filter_threads", "1"]);
        if media.kind == MediaKind::Video {
            command.args([
                "-ss",
                &format!("{:.9}", clip.source_in as f64 / timeline.fps as f64),
            ]);
        }
        command.arg("-i").arg(&clip.source);
        let filter = format!(
            "scale={}:{}:flags=bicubic,setsar=1,fps={}",
            size.0, size.1, timeline.fps
        );
        command.args([
            "-map", "0:v:0", "-an", "-vf", &filter, "-pix_fmt", "bgra", "-f", "rawvideo",
        ]);
        if media.kind == MediaKind::Image {
            command.args(["-frames:v", "1"]);
        }
        command
            .arg("pipe:1")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let process = Process::spawn(&mut command, processes)?;
        let reader = BufReader::new(
            process
                .child
                .lock()
                .unwrap()
                .stdout
                .take()
                .ok_or("无法读取导出画面流")?,
        );
        Ok(Self {
            process,
            reader,
            pixels: vec![0; size.0 as usize * size.1 as usize * 4],
            size,
            index: -1,
            image: media.kind == MediaKind::Image,
        })
    }

    /// 顺序读取到目标项目源帧，低帧率素材尾端重复最后有效画面，与预览尾帧语义一致。
    /// 复用原尺寸像素缓冲；EOF 时先确认进程成功并保留尾帧，部分帧或真实解码错误终止导出。
    fn read(&mut self, index: i64) -> Result<&[u8], String> {
        let target = if self.image { 0 } else { index };
        while self.index < target {
            match self.reader.read(&mut self.pixels[..1]) {
                Ok(0) => {
                    self.process.finish()?;
                    if self.index < 0 {
                        return Err("素材没有可导出的画面".into());
                    }
                    self.index = target;
                    break;
                }
                Ok(_) => self
                    .reader
                    .read_exact(&mut self.pixels[1..])
                    .map_err(|e| format!("素材画面不完整：{e}"))?,
                Err(e) => return Err(format!("读取导出画面失败：{e}")),
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
    /// 创建不持有媒体进程的渲染会话；每个活动片段按需启动，结束后立即释放。
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
        processes: &ProcessList,
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
                        Source::start(clip, source_media, timeline, processes)?,
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
        let processes = ProcessList::default();
        let first = renderer
            .render(&timeline, 0.0, &[], &settings, &processes)
            .unwrap()
            .as_ptr();
        renderer.output.fill(12);
        let frame = renderer
            .render(&timeline, 1.0, &[], &settings, &processes)
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
                .render(&timeline, 2.0, &[], &settings, &processes)
                .unwrap()
                .len(),
            64 * 128 * 4
        );
    }
}
