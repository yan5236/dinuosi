//! 受控运行库的真实编码能力探测。
use crate::{
    export::Encoder,
    export_process::{Process, ProcessList},
    media,
};
use std::{io::Write, process::Stdio, sync::OnceLock};

/// 返回本次应用启动内缓存的真实编码能力，避免反复打开弹窗重复启动编码进程。
/// 首次调用阻塞探测，应由调用方放入后台；运行库或驱动变更后需重启应用重新探测。
pub fn capabilities() -> Result<Vec<Encoder>, String> {
    static AVAILABLE: OnceLock<Result<Vec<Encoder>, String>> = OnceLock::new();
    AVAILABLE.get_or_init(probe).clone()
}

/// 对受控运行库逐个执行真实三帧编码，只有成功初始化并输出的实现才成为可选编码器。
/// 每个探测最多十秒，不使用系统 PATH 或 GPL 外部编码库；诊断通过错误返回。
fn probe() -> Result<Vec<Encoder>, String> {
    let mut available = Vec::new();
    let mut last_error = String::new();
    for encoder in Encoder::candidates() {
        let mut command = media::command("ffmpeg");
        command
            .args([
                "-v",
                "error",
                "-threads",
                "1",
                "-filter_threads",
                "1",
                "-f",
                "rawvideo",
                "-pixel_format",
                "bgra",
                "-video_size",
                "320x240",
                "-framerate",
                "30",
                "-i",
                "pipe:0",
                "-frames:v",
                "3",
                "-an",
                "-threads",
                "1",
                "-c:v",
                encoder.id(),
                "-pix_fmt",
                encoder.pixel_format(),
                "-b:v",
                "1000k",
            ])
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::piped());
        encoder.configure(&mut command);
        command.args(["-f", "null", "-"]);
        let processes = ProcessList::default();
        match Process::spawn(&mut command, &processes) {
            Ok(mut process) => {
                let mut stdin = process
                    .child
                    .lock()
                    .unwrap()
                    .stdin
                    .take()
                    .ok_or("无法写入编码能力检测")?;
                let writer =
                    std::thread::spawn(move || stdin.write_all(&vec![0; 320 * 240 * 4 * 3]));
                let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
                loop {
                    if process
                        .child
                        .lock()
                        .unwrap()
                        .try_wait()
                        .map_err(|e| e.to_string())?
                        .is_some()
                    {
                        break;
                    }
                    if std::time::Instant::now() >= deadline {
                        let _ = process.child.lock().unwrap().kill();
                        break;
                    }
                    std::thread::sleep(std::time::Duration::from_millis(20));
                }
                let result = process.finish();
                let written = writer
                    .join()
                    .map_err(|_| "编码探测写入线程失败".to_owned())?
                    .map_err(|e| e.to_string());
                match result.and(written) {
                    Ok(()) => available.push(encoder),
                    Err(error) => last_error = error,
                }
            }
            Err(error) => last_error = error,
        }
    }
    if available.is_empty() {
        Err(format!("没有可用的视频编码器：{last_error}"))
    } else {
        Ok(available)
    }
}
