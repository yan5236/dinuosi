//! 固定 C ABI 与动态库加载；unsafe 仅存在于经过长度/所有权校验的边界。
use libloading::Library;
use std::{
    ffi::{CStr, CString, c_char, c_int, c_void},
    path::Path,
    sync::{
        Arc, OnceLock,
        atomic::{AtomicBool, Ordering},
    },
};

pub type Interrupt = unsafe extern "C" fn(*mut c_void) -> c_int;
#[repr(C)]
#[derive(Default)]
pub struct Info {
    pub duration: f64,
    pub video: c_int,
    pub audio: c_int,
    pub width: c_int,
    pub height: c_int,
}
#[repr(C)]
pub struct Encoding {
    pub width: c_int,
    pub height: c_int,
    pub fps: c_int,
    pub bitrate: c_int,
    pub encoder: *const c_char,
    pub pixel_format: *const c_char,
    pub options: *const c_char,
    pub container: *const c_char,
    pub duration: f64,
}
pub struct Api {
    pub probe: unsafe extern "C" fn(*const c_char, *mut Info, *mut c_char) -> c_int,
    pub decode_open: unsafe extern "C" fn(
        *const c_char,
        c_int,
        f64,
        f64,
        *const c_char,
        Interrupt,
        *mut c_void,
        *mut c_char,
    ) -> *mut c_void,
    pub decode_read: unsafe extern "C" fn(*mut c_void, *mut u8, c_int, *mut c_char) -> c_int,
    pub decode_free: unsafe extern "C" fn(*mut c_void),
    pub encode_open: unsafe extern "C" fn(
        *const c_char,
        *const c_char,
        *const Encoding,
        Interrupt,
        *mut c_void,
        *mut c_char,
    ) -> *mut c_void,
    pub encode_write: unsafe extern "C" fn(*mut c_void, *const u8, c_int, *mut c_char) -> c_int,
    pub encode_finish: unsafe extern "C" fn(*mut c_void, *mut c_char) -> c_int,
    pub encode_free: unsafe extern "C" fn(*mut c_void),
    _library: Library,
    #[cfg(unix)]
    _dependencies: Vec<Library>,
}

/// 从当前程序旁固定 runtime/ffmpeg 加载桥接库，校验 ABI 和 libav 主版本后缓存句柄。
/// Windows 使用 DLL 目录及系统目录搜索依赖，Unix 使用桥接库相对 rpath；缺失或符号不符返回错误。
pub fn api() -> Result<Arc<Api>, String> {
    static API: OnceLock<Result<Arc<Api>, String>> = OnceLock::new();
    API.get_or_init(|| {
        let exe = std::env::current_exe().map_err(|e| e.to_string())?;
        let runtime = exe
            .parent()
            .ok_or("应用程序没有父目录")?
            .join("runtime/ffmpeg");
        let path = runtime.join(library_name());
        #[cfg(unix)]
        let dependencies = load_dependencies(&runtime)?;
        // 安全：只加载应用邻接的受控本机库；符号在 ABI 校验后按同一头文件调用。
        let library = unsafe {
            #[cfg(windows)]
            {
                libloading::os::windows::Library::load_with_flags(&path, 0x00000100 | 0x00000800)
                    .map(Library::from)
            }
            #[cfg(not(windows))]
            {
                Library::new(&path)
            }
        }
        .map_err(|e| format!("无法加载媒体动态库 {}：{e}", path.display()))?;
        unsafe {
            let abi: libloading::Symbol<unsafe extern "C" fn() -> c_int> =
                library.get(b"dn_abi\0").map_err(|e| e.to_string())?;
            if abi() != 1 {
                return Err("媒体动态库 ABI 或 libav 版本不匹配".into());
            }
            macro_rules! symbol {
                ($name:literal) => {
                    *library
                        .get(concat!($name, "\0").as_bytes())
                        .map_err(|e| e.to_string())?
                };
            }
            Ok(Arc::new(Api {
                probe: symbol!("dn_probe"),
                decode_open: symbol!("dn_decode_open"),
                decode_read: symbol!("dn_decode_read"),
                decode_free: symbol!("dn_decode_free"),
                encode_open: symbol!("dn_encode_open"),
                encode_write: symbol!("dn_encode_write"),
                encode_finish: symbol!("dn_encode_finish"),
                encode_free: symbol!("dn_encode_free"),
                _library: library,
                #[cfg(unix)]
                _dependencies: dependencies,
            }))
        }
    })
    .clone()
}

/// 先按绝对路径加载配套 Unix libav 依赖，缺少相邻库时拒绝系统目录中的同名替代品。
/// 库在整个 Api 生命周期保持加载，桥接依赖通过相对 rpath 与已加载的配套库解析。
#[cfg(unix)]
fn load_dependencies(runtime: &Path) -> Result<Vec<Library>, String> {
    let mut libraries = Vec::new();
    for (name, major) in [
        ("avutil", 60),
        ("swresample", 6),
        ("swscale", 9),
        ("avcodec", 62),
        ("avformat", 62),
        ("avfilter", 11),
    ] {
        let name = if cfg!(target_os = "macos") {
            format!("lib{name}.{major}.dylib")
        } else {
            format!("lib{name}.so.{major}")
        };
        let path = runtime.join(name);
        if !path.is_file() {
            return Err(format!("媒体动态库缺失：{}", path.display()));
        }
        // 安全：仅加载应用配套的固定名称，库在 Api 生命周期内保持加载。
        let library = unsafe {
            libloading::os::unix::Library::open(Some(&path), libloading::os::unix::RTLD_NOW)
                .map(Library::from)
        }
        .map_err(|e| format!("无法加载媒体动态库 {}：{e}", path.display()))?;
        libraries.push(library);
    }
    Ok(libraries)
}

/// 返回当前平台桥接动态库的固定名称；不允许调用者指定任意库或 PATH 回退。
pub fn library_name() -> &'static str {
    if cfg!(windows) {
        "dinuosi_media.dll"
    } else if cfg!(target_os = "macos") {
        "libdinuosi_media.dylib"
    } else {
        "libdinuosi_media.so"
    }
}

/// 将路径变为 C 字符串；Unix 保留原始文件名字节，Windows 使用 FFmpeg 支持的 UTF-8。
/// 内嵌零字节或不可表示的 Windows 路径返回错误，不隐式改变用户路径。
pub fn path_string(path: &Path) -> Result<CString, String> {
    #[cfg(unix)]
    let bytes = {
        use std::os::unix::ffi::OsStrExt;
        path.as_os_str().as_bytes()
    };
    #[cfg(not(unix))]
    let bytes = path.to_str().ok_or("素材路径无法表示为 UTF-8")?.as_bytes();
    CString::new(bytes).map_err(|_| "路径包含零字节".into())
}

/// 将固定 512 字节、预先清零的原生错误缓冲转换为 Rust 诊断，保留解码失败消息前缀。
pub fn error(bytes: &[c_char; 512]) -> String {
    // 安全：缓冲在调用前清零，原生 av_strerror 始终写入终止符且容量固定。
    let message = unsafe { CStr::from_ptr(bytes.as_ptr()) }.to_string_lossy();
    format!("媒体解码失败：{message}")
}

/// 查询宿主持有的原子取消标志；opaque 指向稳定 Arc 分配，直到原生句柄释放后才失效。
pub unsafe extern "C" fn interrupted(opaque: *mut c_void) -> c_int {
    // 安全：所有创建入口只传入 Arc<AtomicBool> 的有效稳定地址。
    i32::from(unsafe { &*opaque.cast::<AtomicBool>() }.load(Ordering::Relaxed))
}
