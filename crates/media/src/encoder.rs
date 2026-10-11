//! 编码和封装的独占安全边界；输出仍由 Core 临时文件发布机制保护。
use crate::ffi;
use std::{
    ffi::{CString, c_void},
    path::Path,
    ptr::NonNull,
    sync::{Arc, atomic::AtomicBool},
};

pub struct Encoding<'a> {
    pub size: (u32, u32),
    pub fps: u32,
    pub bitrate_kbps: u32,
    pub encoder: &'a str,
    pub pixel_format: &'a str,
    pub options: &'a str,
    pub container: &'a str,
    pub duration: f64,
}
pub struct Encoder {
    handle: NonNull<c_void>,
    api: Arc<ffi::Api>,
    _cancel: Arc<AtomicBool>,
    length: usize,
}

impl Encoder {
    /// 为 Core 的专属临时输出创建编码器，audio 为已混合 PCM 或 None；不会发布最终用户文件。
    /// 校验尺寸、帧率、码率、时长及字符串，所有指针在创建调用有效且原生层保留自己的状态。
    pub fn open(
        path: &Path,
        audio: Option<&Path>,
        config: &Encoding<'_>,
        cancel: Arc<AtomicBool>,
    ) -> Result<Self, String> {
        let (width, height) = config.size;
        if !(16..=8192).contains(&width)
            || !(16..=8192).contains(&height)
            || !width.is_multiple_of(2)
            || !height.is_multiple_of(2)
            || !(1..=240).contains(&config.fps)
            || !(100..=1_000_000).contains(&config.bitrate_kbps)
            || !config.duration.is_finite()
            || config.duration <= 0.0
        {
            return Err("导出参数无效".into());
        }
        let api = ffi::api()?;
        let path = ffi::path_string(path)?;
        let audio = audio.map(ffi::path_string).transpose()?;
        let strings = [
            config.encoder,
            config.pixel_format,
            config.options,
            config.container,
        ]
        .map(CString::new);
        let [encoder, pixel_format, options, container] = strings;
        let (encoder, pixel_format, options, container) = (
            encoder.map_err(|e| e.to_string())?,
            pixel_format.map_err(|e| e.to_string())?,
            options.map_err(|e| e.to_string())?,
            container.map_err(|e| e.to_string())?,
        );
        let native = ffi::Encoding {
            width: width as i32,
            height: height as i32,
            fps: config.fps as i32,
            bitrate: (config.bitrate_kbps * 1000) as i32,
            encoder: encoder.as_ptr(),
            pixel_format: pixel_format.as_ptr(),
            options: options.as_ptr(),
            container: container.as_ptr(),
            duration: config.duration,
        };
        let mut error = [0; 512];
        // 安全：字符串和配置存活至同步调用结束；取消 Arc 在会话内保活，句柄完全独占。
        let handle = unsafe {
            (api.encode_open)(
                path.as_ptr(),
                audio.as_ref().map_or(std::ptr::null(), |s| s.as_ptr()),
                &native,
                ffi::interrupted,
                Arc::as_ptr(&cancel).cast_mut().cast(),
                error.as_mut_ptr(),
            )
        };
        Ok(Self {
            handle: NonNull::new(handle).ok_or_else(|| ffi::error(&error))?,
            api,
            _cancel: cancel,
            length: width as usize * height as usize * 4,
        })
    }

    /// 写入与创建尺寸严格匹配的 BGRA 帧；原生编码器复制像素，不保留宿主切片指针。
    pub fn write(&mut self, pixels: &[u8]) -> Result<(), String> {
        if pixels.len() != self.length {
            return Err("编码画面长度不匹配".into());
        }
        let mut error = [0; 512];
        // 安全：尺寸已验证，slice 存活至调用结束，句柄只能由当前对象独占访问。
        let result = unsafe {
            (self.api.encode_write)(
                self.handle.as_ptr(),
                pixels.as_ptr(),
                pixels.len() as i32,
                error.as_mut_ptr(),
            )
        };
        if result < 0 {
            Err(ffi::error(&error))
        } else {
            Ok(())
        }
    }

    /// 排空编码包并完成封装；消费会话使调用方不能重复写入或完成，错误由 Core 清理临时文件。
    pub fn finish(self) -> Result<(), String> {
        let mut error = [0; 512];
        // 安全：句柄在 Drop 前有效，finish 只调用一次，随后由 Drop 释放。
        let result = unsafe { (self.api.encode_finish)(self.handle.as_ptr(), error.as_mut_ptr()) };
        if result < 0 {
            Err(ffi::error(&error))
        } else {
            Ok(())
        }
    }
}
impl Drop for Encoder {
    /// 释放编码句柄及打开的临时文件；正常、取消和失败出口执行同一清理路径。
    fn drop(&mut self) {
        // 安全：本对象独占该原生句柄，库在释放完成前由 Arc 保持加载。
        unsafe { (self.api.encode_free)(self.handle.as_ptr()) };
    }
}
