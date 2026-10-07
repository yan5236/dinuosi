//! 编译内嵌的模块翻译；GPUI 全局状态只记录已应用语言，不依赖运行目录。
use gpui::{App, Global};
use std::{collections::BTreeMap, sync::OnceLock};

pub struct Locale(pub &'static str);
impl Global for Locale {}

type Messages = BTreeMap<String, String>;
type Catalogs = BTreeMap<&'static str, [Messages; 2]>;

macro_rules! catalogs {
    ($($module:literal),+ $(,)?) => {
        &[$(($module,
            include_str!(concat!("../locales/en/", $module, ".json")),
            include_str!(concat!("../locales/zh-CN/", $module, ".json"))),)+]
    };
}

pub(crate) const SOURCES: &[(&str, &str, &str)] = catalogs!(
    "common",
    "manager",
    "dialogs",
    "editor",
    "library",
    "inspector",
    "timeline",
    "settings",
    "help",
    "icons",
    "errors-project",
    "errors-media",
    "errors-timeline",
);

/// 首次查询解析所有编译内嵌的 JSON，之后借用静态缓存；资源格式错误属于构建缺陷并由测试拦截。
fn catalogs() -> &'static Catalogs {
    static CACHE: OnceLock<Catalogs> = OnceLock::new();
    CACHE.get_or_init(|| {
        SOURCES
            .iter()
            .map(|(module, en, zh)| {
                (
                    *module,
                    [
                        serde_json::from_str(en).expect("invalid English catalog"),
                        serde_json::from_str(zh).expect("invalid Chinese catalog"),
                    ],
                )
            })
            .collect()
    })
}

/// 根据已保存的偏好解析系统语言并发布全局语言；所有窗口重绘，草稿选择不会调用此函数。
pub fn apply(preference: &str, cx: &mut App) {
    let system = sys_locale::get_locale();
    cx.set_global(Locale(dinuosi_core::language::resolve(
        preference,
        system.as_deref(),
    )));
    cx.refresh_windows();
}

/// 查询指定语言与模块的消息，缺失或空译文回退英文；未知 key 原样返回以保留可读英文源文。
pub(crate) fn lookup<'a>(locale: &str, module: &str, key: &'a str) -> &'a str {
    let Some(messages) = catalogs().get(module) else {
        return key;
    };
    let index = usize::from(locale == "zh-CN");
    messages[index]
        .get(key)
        .filter(|s| !s.is_empty())
        .or_else(|| messages[0].get(key))
        .map(String::as_str)
        .unwrap_or(key)
}

/// 读取当前 GPUI 应用的已应用语言；初始化前使用英文，避免测试和启动错误依赖机器语言。
pub fn locale(cx: &App) -> &'static str {
    cx.try_global::<Locale>().map_or("en", |locale| locale.0)
}

/// 按模块读取静态界面消息；返回内嵌资源引用，不改动项目数据或用户输入。
pub fn t(module: &str, key: &'static str, cx: &App) -> &'static str {
    lookup(locale(cx), module, key)
}

/// 以命名占位符填充已翻译模板；值只做一次替换，用户名称中的花括号不会再次被解释。
pub fn f(module: &str, key: &'static str, values: &[(&str, String)], cx: &App) -> String {
    interpolate(t(module, key, cx), values)
}

/// 顺序填充模板中的占位符，未知占位符保留原文；不把替换值递归当作模板。
fn interpolate(template: &str, values: &[(&str, String)]) -> String {
    let mut output = String::new();
    let mut rest = template;
    while let Some(start) = rest.find('{') {
        let Some(end) = rest[start..].find('}').map(|n| n + start) else {
            break;
        };
        output.push_str(&rest[..start]);
        let name = &rest[start + 1..end];
        output.push_str(
            values
                .iter()
                .find(|(key, _)| *key == name)
                .map_or(&rest[start..=end], |(_, value)| value.as_str()),
        );
        rest = &rest[end + 1..];
    }
    output.push_str(rest);
    output
}

/// 匹配 Core 既有字符串错误模板并提取参数；只匹配完整消息，不翻译任意路径或用户名称。
fn capture<'a>(template: &str, message: &'a str) -> Option<Vec<(String, &'a str)>> {
    let mut values = Vec::new();
    let mut positional = 0;
    let mut pattern = template;
    let mut rest = message;
    while let Some(start) = pattern.find('{') {
        rest = rest.strip_prefix(&pattern[..start])?;
        let end = pattern[start..].find('}')? + start;
        let name = &pattern[start + 1..end];
        pattern = &pattern[end + 1..];
        let separator = pattern.split('{').next()?;
        let count = if separator.is_empty() {
            rest.len()
        } else {
            rest.find(separator)?
        };
        let name = if name.is_empty() {
            let name = positional.to_string();
            positional += 1;
            name
        } else {
            name.to_owned()
        };
        values.push((name, &rest[..count]));
        rest = &rest[count..];
    }
    (rest == pattern).then_some(values)
}

/// 在展示边界翻译既有 Core/宿主错误，保留系统诊断和参数；嵌套错误最多解析八层避免异常递归。
pub fn error(message: &str, cx: &App) -> String {
    translate_error(message, locale(cx), 0)
}

/// 按完整模板翻译一条错误；只递归错误参数，路径、文件名和项目名保持原文。
fn translate_error(message: &str, locale: &str, depth: usize) -> String {
    if depth >= 8 {
        return message.to_owned();
    }
    for module in ["errors-project", "errors-media", "errors-timeline"] {
        for (template, translated) in &catalogs()[module][usize::from(locale == "zh-CN")] {
            if let Some(values) = capture(template, message) {
                let values: Vec<_> = values
                    .iter()
                    .map(|(name, value)| {
                        let text = if matches!(name.as_str(), "e" | "error" | "rollback") {
                            translate_error(value, locale, depth + 1)
                        } else if template.contains("快捷键冲突") {
                            lookup(locale, "settings", value).to_owned()
                        } else {
                            (*value).to_owned()
                        };
                        (name.as_str(), text)
                    })
                    .collect();
                return interpolate(translated, &values);
            }
        }
    }
    for module in ["editor", "settings"] {
        if catalogs()[module][0].contains_key(message) {
            return lookup(locale, module, message).to_owned();
        }
    }
    // 导入错误以源路径开头、按行汇总；只递归分隔符右侧，保持路径及原始系统诊断。
    if message.contains('\n') {
        return message
            .lines()
            .map(|s| translate_error(s, locale, depth + 1))
            .collect::<Vec<_>>()
            .join("\n");
    }
    if let Some((path, detail)) = message.split_once('：') {
        let translated = translate_error(detail, locale, depth + 1);
        if translated == detail {
            return message.to_owned();
        }
        let separator = if locale == "zh-CN" { "：" } else { ": " };
        return format!("{path}{separator}{translated}");
    }
    message.to_owned()
}
