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

/// 读取用户覆盖；空列表表示明确取消绑定，缺省项继承注册表默认值。
pub fn bindings(shortcut: &Shortcut, preferences: &Preferences) -> Vec<String> {
    preferences
        .shortcuts
        .get(shortcut.id)
        .cloned()
        .unwrap_or_else(|| shortcut.defaults.iter().map(|s| (*s).into()).collect())
}

/// 规范化按键，禁止占用输入导航键或系统关闭组合；不接受只有修饰键的绑定。
pub fn normalize(value: &str) -> Result<String, String> {
    let stroke = Keystroke::parse(value).map_err(|_| "快捷键格式无效".to_owned())?;
    if matches!(
        stroke.key.as_str(),
        "escape" | "tab" | "enter" | "shift" | "control" | "alt" | ""
    ) || stroke.modifiers.platform
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
        for key in bindings(item, preferences) {
            let key = normalize(&key)?;
            if let Some(label) = used.insert(key, item.label) {
                return Err(format!("“{label}”与“{}”的快捷键冲突", item.label));
            }
        }
    }
    Ok(())
}

/// 按完全相同的按键和修饰键查找动作，避免 Ctrl+Shift 等额外修饰键触发普通操作。
pub fn resolve(stroke: &Keystroke, preferences: &Preferences) -> Option<Command> {
    SHORTCUTS
        .iter()
        .find(|item| {
            bindings(item, preferences).iter().any(|key| {
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
