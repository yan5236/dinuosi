//! 快捷键唯一注册表：默认值、用户覆盖、冲突校验和工作区动作解析。
use dinuosi_core::Preferences;
use gpui::Keystroke;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Command {
    New,
    Open,
    Home,
    Delete,
    Undo,
    Redo,
    Play,
    Settings,
    SelectTool,
    BladeTool,
    SplitAtPlayhead,
}

pub struct Shortcut {
    pub command: Command,
    pub id: &'static str,
    pub label: &'static str,
    pub defaults: &'static [&'static str],
}

pub const SHORTCUTS: &[Shortcut] = &[
    Shortcut {
        command: Command::New,
        id: "new",
        label: "New project",
        defaults: &["ctrl-n"],
    },
    Shortcut {
        command: Command::Open,
        id: "open",
        label: "Open project",
        defaults: &["ctrl-o"],
    },
    Shortcut {
        command: Command::Home,
        id: "home",
        label: "Project manager",
        defaults: &["ctrl-h"],
    },
    Shortcut {
        command: Command::Delete,
        id: "delete",
        label: "Delete clip",
        defaults: &["delete", "backspace"],
    },
    Shortcut {
        command: Command::Undo,
        id: "undo",
        label: "Undo",
        defaults: &["ctrl-z"],
    },
    Shortcut {
        command: Command::Redo,
        id: "redo",
        label: "Redo",
        defaults: &["ctrl-y", "ctrl-shift-z"],
    },
    Shortcut {
        command: Command::Play,
        id: "play",
        label: "Play / Pause",
        defaults: &["space"],
    },
    Shortcut {
        command: Command::Settings,
        id: "settings",
        label: "Settings",
        defaults: &["ctrl-,"],
    },
    Shortcut {
        command: Command::SelectTool,
        id: "select-tool",
        label: "Selection tool",
        defaults: &["v"],
    },
    Shortcut {
        command: Command::BladeTool,
        id: "blade-tool",
        label: "Blade tool",
        defaults: &["c"],
    },
    Shortcut {
        command: Command::SplitAtPlayhead,
        id: "split-at-playhead",
        label: "Split selected clips at playhead",
        defaults: &["ctrl-b"],
    },
];

/// 读取用户覆盖；空列表取消绑定，缺省项继承默认值，macOS 的常用操作使用 Command。
/// 项目管理沿用 Ctrl+H，避免占用 macOS 的隐藏应用组合；用户已保存的按键原样保留。
pub fn bindings(shortcut: &Shortcut, preferences: &Preferences) -> Vec<String> {
    preferences
        .shortcuts
        .get(shortcut.id)
        .cloned()
        .unwrap_or_else(|| {
            shortcut
                .defaults
                .iter()
                .map(|key| {
                    if cfg!(target_os = "macos") && shortcut.command != Command::Home {
                        key.replacen("ctrl-", "cmd-", 1)
                    } else {
                        (*key).into()
                    }
                })
                .collect()
        })
}

/// 收集参与实际匹配和冲突校验的全部绑定；macOS 仅对未覆盖操作保留旧 Ctrl 别名。
/// 与界面显示的首选 Command 绑定分开，保证保存覆盖时不会产生隐式冲突或抢占其他操作。
fn effective_bindings(shortcut: &Shortcut, preferences: &Preferences) -> Vec<String> {
    let mut keys = bindings(shortcut, preferences);
    if cfg!(target_os = "macos") && !preferences.shortcuts.contains_key(shortcut.id) {
        for key in shortcut.defaults {
            if !keys.iter().any(|existing| existing == key) {
                keys.push((*key).to_owned());
            }
        }
    }
    keys
}

/// 规范化按键，禁止占用输入导航键或系统关闭组合；不接受只有修饰键的绑定。
pub fn normalize(value: &str) -> Result<String, String> {
    let stroke = Keystroke::parse(value).map_err(|_| "快捷键格式无效".to_owned())?;
    if matches!(
        stroke.key.as_str(),
        "escape" | "tab" | "enter" | "shift" | "control" | "alt" | ""
    ) || (stroke.modifiers.platform
        && (!cfg!(target_os = "macos") || matches!(stroke.key.as_str(), "q" | "w" | "h")))
        || (stroke.modifiers.alt && stroke.key == "f4")
    {
        return Err("该按键保留用于窗口或输入操作".into());
    }
    Ok(stroke.unparse())
}

/// 校验完整有效映射，包含未覆盖的默认快捷键；冲突错误指出对应操作。
pub fn validate(preferences: &Preferences) -> Result<(), String> {
    let mut used = std::collections::BTreeMap::new();
    for item in SHORTCUTS {
        for key in effective_bindings(item, preferences) {
            let key = normalize(&key)?;
            if let Some(label) = used.insert(key, item.label) {
                return Err(format!("“{label}”与“{}”的快捷键冲突", item.label));
            }
        }
    }
    Ok(())
}

/// 精确匹配按键和修饰键，macOS 的未覆盖默认操作兼容原 Ctrl 绑定；用户覆盖不增加别名。
pub fn resolve(stroke: &Keystroke, preferences: &Preferences) -> Option<Command> {
    SHORTCUTS
        .iter()
        .find(|item| {
            effective_bindings(item, preferences).iter().any(|key| {
                Keystroke::parse(key)
                    .is_ok_and(|key| key.key == stroke.key && key.modifiers == stroke.modifiers)
            })
        })
        .map(|item| item.command)
}

/// 把存储键名转换成易读显示；多默认键保留各自完整组合。
pub fn label(keys: &[String]) -> String {
    if keys.is_empty() {
        return "Unassigned".into();
    }
    keys.iter()
        .map(|s| {
            s.split('-')
                .map(|part| match part {
                    "ctrl" => "Ctrl".into(),
                    "cmd" => "Cmd".into(),
                    "shift" => "Shift".into(),
                    "alt" => "Alt".into(),
                    "space" => "Space".into(),
                    "delete" => "Delete".into(),
                    "backspace" => "Backspace".into(),
                    _ => part.to_uppercase(),
                })
                .collect::<Vec<String>>()
                .join("+")
        })
        .collect::<Vec<_>>()
        .join(" / ")
}
