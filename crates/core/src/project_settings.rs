//! 项目输出参数修改，保持片段在换帧率后的秒数及共享边界。
use crate::timeline::Timeline;

impl Timeline {
    /// 原子修改分辨率、整数帧率和 kbps 码率；按绝对边界四舍五入换算时间。
    /// 低帧率若导致片段消失、源越界或重叠则拒绝整次修改，原状态保持不变。
    pub fn configure(
        &mut self,
        width: u32,
        height: u32,
        fps: u32,
        bitrate: u32,
    ) -> Result<(), String> {
        if !(1..=16384).contains(&width) || !(1..=16384).contains(&height) {
            return Err("分辨率必须在 1 至 16384 像素之间".into());
        }
        if !(1..=240).contains(&fps) || !(100..=1_000_000).contains(&bitrate) {
            return Err("帧率必须在 1 至 240 之间，码率必须在 100 至 1000000 kbps 之间".into());
        }
        self.validate()?;
        let mut next = self.clone();
        let convert = |frame: i64| (frame * fps as i64 + self.fps as i64 / 2) / self.fps as i64;
        for clip in &mut next.clips {
            let end = convert(clip.start + clip.duration);
            clip.start = convert(clip.start);
            clip.duration = end - clip.start;
            clip.source_in = convert(clip.source_in);
            clip.source_frames = convert(clip.source_frames);
        }
        next.width = width;
        next.height = height;
        next.fps = fps;
        next.bitrate_kbps = bitrate;
        next.validate()
            .map_err(|e| format!("无法应用项目设置：{e}"))?;
        *self = next;
        Ok(())
    }
}
