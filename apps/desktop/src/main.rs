#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

mod assets;
mod editor;
mod ui;
mod view;

use gpui::{
    App, AppContext, Application, Bounds, TitlebarOptions, WindowBounds, WindowOptions, px, size,
};
use view::Desktop;

/// 启动 GPUI 桌面窗口并注册编译内嵌的品牌和图标资源。
/// 窗口创建失败时向标准错误报告并退出；关闭最后一个窗口时结束程序。
fn main() {
    Application::new()
        .with_assets(assets::Assets)
        .run(|cx: &mut App| {
            cx.on_window_closed(|cx| {
                if cx.windows().is_empty() {
                    cx.quit();
                }
            })
            .detach();
            let bounds = Bounds::centered(None, size(px(1440.0), px(900.0)), cx);
            let result = cx.open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(bounds)),
                    window_min_size: Some(size(px(1040.0), px(640.0))),
                    titlebar: Some(TitlebarOptions {
                        title: Some("Dinuosi".into()),
                        appears_transparent: true,
                        ..Default::default()
                    }),
                    ..Default::default()
                },
                |window, cx| cx.new(|cx| Desktop::new(window, cx)),
            );
            if let Err(error) = result {
                eprintln!("无法创建 Dinuosi 窗口：{error}");
                cx.quit();
                std::process::exit(1);
            }
            cx.activate(true);
        });
}
