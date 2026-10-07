//! 独立于项目文件的用户偏好；使用既有原子 JSON 写入，失败保留原设置。
use super::*;
use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Preferences {
    pub version: u32,
    pub live_scrub: bool,
    pub blade_requires_selection: bool,
    pub language: String,
    pub shortcuts: BTreeMap<String, Vec<String>>,
}

impl Default for Preferences {
    /// 初始偏好保持实时拖动预览与直接刀片切割，语言跟随系统并回退英文；快捷键覆盖为空。
    fn default() -> Self {
        Self {
            version: 1,
            live_scrub: true,
            blade_requires_selection: false,
            language: "auto".into(),
            shortcuts: BTreeMap::new(),
        }
    }
}

impl Preferences {
    /// 校验磁盘版本与语言索引中的稳定 ID；未知语言返回错误，快捷键语义由宿主统一注册表校验。
    fn validate(&self) -> Result<(), String> {
        if self.version != 1
            || !crate::language::LANGUAGES
                .iter()
                .any(|item| item.id == self.language)
        {
            return Err("用户设置版本或语言无效".into());
        }
        Ok(())
    }
}

impl ProjectStore {
    /// 从最近项目索引同目录读取用户偏好；文件不存在使用默认值，损坏时返回错误。
    /// 旧版尚未实现的三种语言选择在内存中迁移为英文，保留其他偏好，不主动覆写磁盘。
    pub fn load_preferences(&self) -> Result<Preferences, String> {
        let path = self.index_path.with_file_name("preferences.json");
        let bytes = match fs::read(path) {
            Ok(bytes) => bytes,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(Preferences::default()),
            Err(e) => return Err(format!("读取用户设置失败：{e}")),
        };
        let mut preferences: Preferences =
            serde_json::from_slice(&bytes).map_err(|e| format!("用户设置损坏：{e}"))?;
        if matches!(preferences.language.as_str(), "zh-TW" | "es" | "ru") {
            preferences.language = "en".into();
        }
        preferences.validate()?;
        Ok(preferences)
    }

    /// 校验并原子保存偏好，不修改任何项目；失败保留既有 JSON。
    pub fn save_preferences(&self, preferences: &Preferences) -> Result<(), String> {
        preferences.validate()?;
        atomic_json(
            &self.index_path.with_file_name("preferences.json"),
            preferences,
        )
    }
}
