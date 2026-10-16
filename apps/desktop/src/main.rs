#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

mod assets;
mod audio_inspector;
mod blade_cursor;
mod caption_files;
mod captions_inspector;
mod color_controls;
mod color_edit;
mod color_graph;
mod color_hsl;
mod color_inspector;
mod color_scopes;
mod color_steps;
#[cfg(test)]
mod color_tests;
mod curve_edit;
mod curve_graph;
mod curve_panel;
mod dialogs;
mod editor;
mod export_controls;
mod export_dialog;
mod export_view;
mod help;
#[cfg(test)]
mod hsl_tests;
mod i18n;
mod input;
mod inspector;
mod inspector_overview;
mod keyframes;
mod library;
mod manager;
mod marker_dialog;
mod media;
mod media_actions;
mod media_status;
mod platform;
mod playback;
mod preferences;
mod preview;
mod preview_geometry;
mod preview_player;
mod preview_transform;
mod preview_view;
mod settings;
mod settings_dropdown;
mod settings_view;
mod shortcuts;
mod sliders;
mod speed_curve;
mod speed_input;
mod speed_inspector;
mod switch;
#[cfg(test)]
mod text_close_tests;
mod text_color_picker;
mod text_edit;
mod text_effects;
mod text_fields;
#[cfg(test)]
mod text_fix_tests;
mod text_inspector;
mod text_library;
mod text_presets;
#[cfg(test)]
mod text_tests;
mod timeline_actions;
mod timeline_blade;
mod timeline_clip;
mod timeline_commit;
mod timeline_drag;
mod timeline_keyframes;
mod timeline_links;
mod timeline_markers;
mod timeline_playback;
mod timeline_preview;
mod timeline_scroll;
mod timeline_snap;
mod timeline_sound;
mod timeline_state;
mod timeline_tracks;
mod timeline_view;
mod tooltip;
mod trim_handle;
mod ui;
mod view;
mod waveform;
mod windows;

#[cfg(test)]
mod audio_track_tests;
#[cfg(test)]
mod curve_tests;
#[cfg(test)]
mod editor_media_tests;
#[cfg(test)]
mod export_tests;
#[cfg(test)]
mod i18n_tests;
#[cfg(test)]
mod keyframe_tests;
#[cfg(test)]
mod preview_player_tests;
#[cfg(test)]
mod preview_transform_tests;
#[cfg(test)]
mod settings_drag_tests;
#[cfg(test)]
mod settings_tests;
#[cfg(test)]
mod speed_tests;
#[cfg(test)]
mod timeline_blade_tests;
#[cfg(test)]
mod timeline_handle_tests;
#[cfg(test)]
mod timeline_links_tests;
#[cfg(test)]
mod timeline_playback_tests;
#[cfg(test)]
mod timeline_scroll_tests;
#[cfg(test)]
mod timeline_tests;
#[cfg(test)]
mod timeline_tools_tests;
#[cfg(test)]
mod timeline_zoom_tests;
#[cfg(test)]
mod tooltip_tests;
#[cfg(test)]
mod waveform_tests;

use dinuosi_core::ProjectStore;
use gpui::{App, Application};

/// 启动 GPUI 桌面窗口并注册编译内嵌的品牌和图标资源。
/// 用户数据位置缺失或窗口创建失败时报告错误；管理与剪辑窗口生命周期由 windows 模块负责。
fn main() {
    Application::new()
        .with_assets(assets::Assets)
        .run(|cx: &mut App| {
            let result = platform::data_directory().and_then(|directory| {
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
