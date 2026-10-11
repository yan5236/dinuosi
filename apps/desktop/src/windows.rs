//! 项目管理与剪辑主体使用独立原生窗口；只有关闭管理窗口才退出应用。

use crate::view::Desktop;
use dinuosi_core::{ProjectEntry, ProjectStore};
use gpui::{
    App, AppContext, Bounds, Global, SharedString, TitlebarOptions, Window, WindowBounds,
    WindowDecorations, WindowHandle, WindowOptions, point, px, size,
};

struct WindowRole {
    editor: bool,
    store: ProjectStore,
}
impl Global for WindowRole {}

/// 安装关闭观察器并创建固定尺寸管理窗口；关闭最后一个剪辑窗口时重新打开管理窗口。
/// 程序切换时先创建目标窗口，源窗口随后移除，避免“最后窗口关闭”导致提前退出。
pub fn start(store: ProjectStore, cx: &mut App) -> Result<(), String> {
    cx.set_global(WindowRole {
        editor: false,
        store: store.clone(),
    });
    cx.on_window_closed(|cx| {
        if !cx.windows().is_empty() {
            return;
        }
        let state = cx.global::<WindowRole>();
        if state.editor {
            let store = state.store.clone();
            if let Err(error) = open(store, None, cx) {
                eprintln!("无法返回项目管理：{error}");
                cx.quit();
            }
        } else {
            cx.quit();
        }
    })
    .detach();
    open(store, None, cx).map(|_| ())
}

/// 返回两种窗口的系统配置；管理窗口 900×620 且不可调整，剪辑窗口保留 1440×900 与最小尺寸。
pub(crate) fn options(editor: bool, cx: &App) -> WindowOptions {
    let dimensions = if editor {
        size(px(1440.0), px(900.0))
    } else {
        size(px(900.0), px(620.0))
    };
    WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
            None, dimensions, cx,
        ))),
        window_min_size: Some(if editor {
            size(px(1040.0), px(640.0))
        } else {
            dimensions
        }),
        is_resizable: editor,
        window_decorations: cfg!(target_os = "linux").then_some(WindowDecorations::Client),
        titlebar: Some(titlebar("Dinuosi")),
        ..Default::default()
    }
}

/// 返回自绘标题栏共用的原生配置，保留系统标题、窗口拖动及可调整大小的能力。
/// macOS 系统按钮移出窗口，避免与既有灰阶控件重复；只生成配置，不操作窗口或项目。
pub(crate) fn titlebar(title: impl Into<SharedString>) -> TitlebarOptions {
    // shortcut: GPUI 0.2.2 没有独立隐藏 macOS 系统按钮的接口；上游提供该接口后替换移出窗口的方式。
    TitlebarOptions {
        title: Some(title.into()),
        appears_transparent: true,
        traffic_light_position: cfg!(target_os = "macos").then_some(point(px(-100.0), px(12.0))),
    }
}

/// 创建独立的 GPUI 原生窗口并记录其角色；失败保留源窗口和当前角色。
pub(crate) fn open(
    store: ProjectStore,
    current: Option<ProjectEntry>,
    cx: &mut App,
) -> Result<WindowHandle<Desktop>, String> {
    let editor = current.is_some();
    let handle = cx
        .open_window(options(editor, cx), |window, cx| {
            cx.new(|cx| Desktop::new(store.clone(), current, window, cx))
        })
        .map_err(|e| format!("打开窗口失败：{e}"))?;
    cx.set_global(WindowRole { editor, store });
    cx.activate(true);
    Ok(handle)
}

/// 将源窗口切换为新角色的独立窗口；仅在目标窗口成功创建后销毁源窗口。
pub(crate) fn replace(
    store: ProjectStore,
    current: Option<ProjectEntry>,
    window: &mut Window,
    cx: &mut App,
) -> Result<(), String> {
    open(store, current, cx)?;
    window.remove_window();
    Ok(())
}
