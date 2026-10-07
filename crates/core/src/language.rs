//! 可用界面语言及系统标签解析；翻译资源由 Desktop 按模块维护。
pub struct Language {
    pub id: &'static str,
    pub native_name: &'static str,
}
pub const LANGUAGES: [Language; 3] = [
    Language {
        id: "auto",
        native_name: "Automatic",
    },
    Language {
        id: "zh-CN",
        native_name: "简体中文",
    },
    Language {
        id: "en",
        native_name: "English",
    },
];
/// 把显式选择或系统语言标签解析为可用语言；auto 使用系统标签，缺失、不支持时回退英文。
/// 接受地区、大小写和下划线差异；繁体中文不属于首版支持范围，Hans 脚本优先于地区。
pub fn resolve(preference: &str, system_locale: Option<&str>) -> &'static str {
    let tag = if preference == "auto" {
        system_locale.unwrap_or("en")
    } else {
        preference
    };
    let tag = tag.replace('_', "-").to_ascii_lowercase();
    let parts: Vec<_> = tag.split('-').collect();
    if parts[0] == "zh"
        && !parts.contains(&"hant")
        && (parts.contains(&"hans") || !parts.iter().any(|p| matches!(*p, "tw" | "hk" | "mo")))
    {
        "zh-CN"
    } else {
        "en"
    }
}
