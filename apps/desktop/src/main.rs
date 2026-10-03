#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

mod assets;
mod dialogs;
mod editor;
mod input;
mod inspector;
mod library;
mod manager;
mod media;
mod media_actions;
mod media_status;
mod playback;
mod preview;
mod preview_view;
mod sliders;
mod timeline_actions;
mod timeline_clip;
mod timeline_drag;
mod timeline_preview;
mod timeline_sound;
mod timeline_state;
mod timeline_tracks;
mod timeline_view;
mod trim_handle;
mod ui;
mod view;
mod windows;

#[cfg(test)]
mod editor_media_tests;
#[cfg(test)]
mod timeline_tests;

use dinuosi_core::ProjectStore;
use gpui::{App, Application};

/// 启动 GPUI 桌面窗口并注册编译内嵌的品牌和图标资源。
/// 用户数据位置缺失或窗口创建失败时报告错误；管理与剪辑窗口生命周期由 windows 模块负责。
fn main() {
    Application::new()
        .with_assets(assets::Assets)
        .run(|cx: &mut App| {
            let result = data_directory().and_then(|directory| {
                windows::start(
                    ProjectStore::new(directory.join("recent-projects.json")),
                    cx,
                )
            });
            if let Err(error) = result {
                eprintln!("无法创建 Dinuosi 窗口：{error}");
                cx.quit();
                std::process::exit(1);
            }
            cx.activate(true);
        });
}

/// 定位应用数据目录；允许显式 DINUOSI_DATA_DIR 隔离开发/测试数据，不把用户列表写入仓库。
/// Windows 使用 LOCALAPPDATA，其他平台使用 XDG_DATA_HOME 或用户目录；缺失时明确报错。
fn data_directory() -> Result<std::path::PathBuf, String> {
    if let Some(path) = std::env::var_os("DINUOSI_DATA_DIR") {
        return Ok(std::path::PathBuf::from(path));
    }
    let base = if cfg!(target_os = "windows") {
        std::env::var_os("LOCALAPPDATA").map(std::path::PathBuf::from)
    } else {
        std::env::var_os("XDG_DATA_HOME")
            .map(std::path::PathBuf::from)
            .or_else(|| {
                std::env::var_os("HOME").map(|p| std::path::PathBuf::from(p).join(".local/share"))
            })
    };
    base.map(|path| path.join("Dinuosi"))
        .ok_or_else(|| "无法定位用户数据目录".into())
}
