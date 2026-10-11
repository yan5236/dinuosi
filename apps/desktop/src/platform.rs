//! 桌面数据目录的系统约定；测试通过显式环境映射验证，不修改进程全局环境。
use std::{ffi::OsString, path::PathBuf};

/// 定位独立应用数据目录，保留开发覆盖以及已有 macOS 旧目录，避免升级后丢失项目列表。
/// Windows 使用 LOCALAPPDATA，macOS 使用 Application Support，Linux 遵循 XDG；缺少基目录返回错误。
pub fn data_directory() -> Result<PathBuf, String> {
    let path = directory(std::env::consts::OS, |key| std::env::var_os(key))?;
    if cfg!(target_os = "macos") && std::env::var_os("DINUOSI_DATA_DIR").is_none() {
        let legacy = directory("linux", |key| std::env::var_os(key))?;
        if legacy.join("recent-projects.json").is_file() && !path.exists() {
            return Ok(legacy);
        }
    }
    Ok(path)
}

/// 按给定平台及环境查询计算路径；忽略空值和相对 XDG 路径，避免依赖启动工作目录。
/// 返回尚未创建的目录，缺少用户主目录或 Windows 数据目录时保持既有错误合同。
fn directory(os: &str, env: impl Fn(&str) -> Option<OsString>) -> Result<PathBuf, String> {
    let read = |key| {
        env(key)
            .filter(|value| !value.is_empty())
            .map(PathBuf::from)
    };
    if let Some(path) = read("DINUOSI_DATA_DIR") {
        return Ok(path);
    }
    let base = match os {
        "windows" => read("LOCALAPPDATA"),
        "macos" => read("HOME").map(|home| home.join("Library/Application Support")),
        _ => read("XDG_DATA_HOME")
            .filter(|path| path.is_absolute())
            .or_else(|| read("HOME").map(|home| home.join(".local/share"))),
    };
    base.map(|path| path.join("Dinuosi"))
        .ok_or_else(|| "无法定位用户数据目录".into())
}

/// 用合成环境检查三平台路径、显式覆盖及无用户目录失败，不创建文件或影响真实用户数据。
#[test]
fn data_locations_follow_platform_conventions() {
    let env = |key: &str| match key {
        "HOME" => Some(OsString::from("/users/test")),
        "LOCALAPPDATA" => Some(OsString::from("/users/test/local")),
        "XDG_DATA_HOME" => Some(OsString::from("relative")),
        _ => None,
    };
    assert_eq!(
        directory("windows", env).unwrap(),
        PathBuf::from("/users/test/local/Dinuosi")
    );
    assert_eq!(
        directory("macos", env).unwrap(),
        PathBuf::from("/users/test/Library/Application Support/Dinuosi")
    );
    assert_eq!(
        directory("linux", env).unwrap(),
        PathBuf::from("/users/test/.local/share/Dinuosi")
    );
    assert!(directory("linux", |_| None).is_err());
    assert_eq!(
        directory("macos", |_| Some(OsString::from("override"))).unwrap(),
        PathBuf::from("override")
    );
}
