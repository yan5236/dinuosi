//! 首版语言索引：集中维护稳定 ID、自称和五种语言下的译名；界面文案仍待拆分 JSON。

pub struct Language {
    pub id: &'static str,
    pub native_name: &'static str,
    names: [&'static str; 5],
}

pub const LANGUAGES: [Language; 6] = [
    Language {
        id: "auto",
        native_name: "自动选择",
        names: [
            "自动选择",
            "自動選擇",
            "Automatic",
            "Automático",
            "Автоматически",
        ],
    },
    Language {
        id: "zh-CN",
        native_name: "简体中文",
        names: [
            "简体中文",
            "簡體中文",
            "Simplified Chinese",
            "Chino simplificado",
            "Китайский (упрощённый)",
        ],
    },
    Language {
        id: "zh-TW",
        native_name: "繁體中文",
        names: [
            "繁体中文",
            "繁體中文",
            "Traditional Chinese",
            "Chino tradicional",
            "Китайский (традиционный)",
        ],
    },
    Language {
        id: "en",
        native_name: "English",
        names: ["英语", "英語", "English", "Inglés", "Английский"],
    },
    Language {
        id: "es",
        native_name: "Español",
        names: ["西班牙语", "西班牙語", "Spanish", "Español", "Испанский"],
    },
    Language {
        id: "ru",
        native_name: "Русский",
        names: ["俄语", "俄語", "Russian", "Ruso", "Русский"],
    },
];

impl Language {
    /// 返回指定界面语言中的语言名称；locale 应为解析后的索引 ID，未知值回退简体中文。
    /// 只借用索引字符串，不修改偏好，也不加载尚未拆分的界面翻译文件。
    pub fn name(&self, locale: &str) -> &'static str {
        let index = match locale {
            "zh-TW" => 1,
            "en" => 2,
            "es" => 3,
            "ru" => 4,
            _ => 0,
        };
        self.names[index]
    }
}

/// 把显式选择或系统语言标签映射到首版语言 ID；auto 使用 system_locale，缺失或不支持时回退中文。
/// 接受系统标签的地区、大小写及下划线差异；中文脚本优先于地区，不读写磁盘或系统设置。
pub fn resolve(preference: &str, system_locale: Option<&str>) -> &'static str {
    let tag = if preference == "auto" {
        system_locale.unwrap_or("zh-CN")
    } else {
        preference
    };
    let tag = tag.replace('_', "-").to_ascii_lowercase();
    let parts: Vec<_> = tag.split('-').collect();
    match parts[0] {
        "zh" => {
            if parts.contains(&"hant")
                || (!parts.contains(&"hans")
                    && parts.iter().any(|p| matches!(*p, "tw" | "hk" | "mo")))
            {
                "zh-TW"
            } else {
                "zh-CN"
            }
        }
        "en" => "en",
        "es" => "es",
        "ru" => "ru",
        _ => "zh-CN",
    }
}
