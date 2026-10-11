//! libav 动态库的安全宿主接口，不依赖 FFmpeg/ffprobe CLI 或 GUI。
mod encoder;
mod ffi;
pub use encoder::{Encoder, Encoding};
pub use ffi::library_name;
use std::{
    ffi::{CString, c_void},
    path::Path,
    ptr::NonNull,
    sync::{Arc, atomic::AtomicBool},
};

#[derive(Debug)]
pub struct Info {
    pub duration: f64,
    pub video: bool,
    pub audio: bool,
    pub size: (u32, u32),
}

/// 只读本地文件的流类型、时长和显示尺寸，不创建进程；损坏文件或动态库缺失返回诊断。
pub fn probe(path: &Path) -> Result<Info, String> {
    let api = ffi::api()?;
    let path = ffi::path_string(path)?;
    let mut info = ffi::Info::default();
    let mut error = [0; 512];
    // 安全：路径/输出/错误缓冲在同步调用期间有效且与固定 C ABI 一致。
    let result = unsafe { (api.probe)(path.as_ptr(), &mut info, error.as_mut_ptr()) };
    if result < 0 {
        return Err(ffi::error(&error));
    }
    Ok(Info {
        duration: info.duration,
        video: info.video != 0,
        audio: info.audio != 0,
        size: (info.width.max(1) as u32, info.height.max(1) as u32),
    })
}

pub struct Decoder {
    handle: NonNull<c_void>,
    api: Arc<ffi::Api>,
    _cancel: Arc<AtomicBool>,
}
// 安全：每个原生句柄完全独占，所有 API 使用 &mut self；只允许移动，不允许并发共享访问。
unsafe impl Send for Decoder {}

impl Decoder {
    /// 打开独占原生解码器，精确读取秒级区间；音频统一 PCM，视频应用内部受控滤镜。
    /// filter 不用于用户命令输入；取消句柄在会话结束前保持有效，失败释放原生资源。
    pub fn open(
        path: &Path,
        audio: bool,
        start: f64,
        end: f64,
        filter: &str,
        cancel: Arc<AtomicBool>,
    ) -> Result<Self, String> {
        if !start.is_finite() || !end.is_finite() || start < 0.0 || end <= start {
            return Err("媒体播放区间无效".into());
        }
        let api = ffi::api()?;
        let path = ffi::path_string(path)?;
        let filter = CString::new(filter).map_err(|_| "滤镜包含零字节")?;
        let mut error = [0; 512];
        // 安全：字符串在调用内有效，原生层复制滤镜，取消指针由返回会话持有的 Arc 保活。
        let handle = unsafe {
            (api.decode_open)(
                path.as_ptr(),
                i32::from(audio),
                start,
                end,
                filter.as_ptr(),
                ffi::interrupted,
                Arc::as_ptr(&cancel).cast_mut().cast(),
                error.as_mut_ptr(),
            )
        };
        Ok(Self {
            handle: NonNull::new(handle).ok_or_else(|| ffi::error(&error))?,
            api,
            _cancel: cancel,
        })
    }

    /// 将下一 BGRA 帧或 PCM 块写入给定非空缓冲；零表示正常 EOF，真实错误带原生诊断。
    /// 检查 C 长度范围与实际返回值，避免跨边界溢出；读取始终独占句柄。
    pub fn read(&mut self, bytes: &mut [u8]) -> Result<usize, String> {
        let length = i32::try_from(bytes.len())
            .ok()
            .filter(|length| *length > 0)
            .ok_or("解码缓冲长度无效")?;
        let mut error = [0; 512];
        // 安全：输出长度经过校验，独占句柄未释放；C 层写入不超过 capacity。
        let result = unsafe {
            (self.api.decode_read)(
                self.handle.as_ptr(),
                bytes.as_mut_ptr(),
                length,
                error.as_mut_ptr(),
            )
        };
        if result < 0 {
            return Err(ffi::error(&error));
        }
        if result > length {
            return Err("解码结果超过缓冲长度".into());
        }
        Ok(result as usize)
    }
}

impl Drop for Decoder {
    /// 释放原生句柄及文件；库句柄和取消 Arc 在释放调用完成前始终存活。
    fn drop(&mut self) {
        // 安全：句柄由本对象独占且仅释放一次。
        unsafe { (self.api.decode_free)(self.handle.as_ptr()) };
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    /// 验证非法区间在访问媒体库之前拒绝，路径零字节也不能穿过原生边界。
    #[test]
    fn rejects_invalid_intervals_and_paths() {
        for (start, end) in [
            (f64::NAN, 1.0),
            (-1.0, 1.0),
            (1.0, 1.0),
            (0.0, f64::INFINITY),
        ] {
            assert!(
                Decoder::open(
                    Path::new("missing"),
                    true,
                    start,
                    end,
                    "",
                    Arc::new(AtomicBool::new(false))
                )
                .is_err()
            );
        }
        assert!(ffi::path_string(Path::new("bad\0name")).is_err());
    }
}
