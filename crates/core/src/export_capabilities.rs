//! 受控动态库的真实编码能力探测。
use crate::export::Encoder;
use std::sync::{
    Arc, OnceLock,
    atomic::{AtomicBool, Ordering},
};

/// 返回本次启动内缓存的真实编码能力；首次阻塞探测，应由宿主在后台调用。
/// 使用独立工作线程避免 Windows GUI 的 STA COM 环境污染 Media Foundation；变更运行库需重启。
pub fn capabilities() -> Result<Vec<Encoder>, String> {
    static AVAILABLE: OnceLock<Result<Vec<Encoder>, String>> = OnceLock::new();
    AVAILABLE
        .get_or_init(|| {
            std::thread::spawn(probe)
                .join()
                .map_err(|_| "编码能力探测线程失败".to_owned())?
        })
        .clone()
}

/// 对本平台候选逐一执行真实三帧原生编码及封装，只有完整成功的实现成为选项。
/// 每个会话十秒后设置取消标志，临时输出始终清理；不启动 FFmpeg CLI 或使用 PATH。
fn probe() -> Result<Vec<Encoder>, String> {
    let root = std::env::temp_dir().join(format!("dinuosi-codecs-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir(&root).map_err(|e| e.to_string())?;
    let mut available = Vec::new();
    let mut last_error = String::new();
    for encoder in Encoder::candidates() {
        let path = root.join("probe.mkv");
        let cancel = Arc::new(AtomicBool::new(false));
        let done = AtomicBool::new(false);
        let result = std::thread::scope(|scope| {
            scope.spawn(|| {
                let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
                while !done.load(Ordering::Relaxed) {
                    if std::time::Instant::now() >= deadline {
                        cancel.store(true, Ordering::Relaxed);
                        break;
                    }
                    std::thread::sleep(std::time::Duration::from_millis(20));
                }
            });
            let result = (|| {
                let config = dinuosi_media::Encoding {
                    size: (320, 240),
                    fps: 30,
                    bitrate_kbps: 1000,
                    encoder: encoder.id(),
                    pixel_format: encoder.pixel_format(),
                    options: encoder.options(),
                    container: "matroska",
                    duration: 0.1,
                };
                let mut native =
                    dinuosi_media::Encoder::open(&path, None, &config, cancel.clone())?;
                let pixels = [0, 0, 0, 255].repeat(320 * 240);
                for _ in 0..3 {
                    native.write(&pixels)?;
                }
                native.finish()
            })();
            done.store(true, Ordering::Relaxed);
            result
        });
        match result {
            Ok(()) => available.push(encoder),
            Err(error) => last_error = error,
        }
        let _ = std::fs::remove_file(&path);
    }
    let _ = std::fs::remove_dir(&root);
    if available.is_empty() {
        Err(format!("没有可用的视频编码器：{last_error}"))
    } else {
        Ok(available)
    }
}
