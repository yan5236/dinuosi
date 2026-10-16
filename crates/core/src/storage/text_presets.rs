//! 用户级文字样式库，与项目及通用偏好分开，复用既有原子 JSON 写入。
use super::*;
use crate::text::TextPreset;

#[derive(Serialize, Deserialize)]
struct PresetsV1 {
    version: u32,
    presets: Vec<TextPreset>,
}

impl ProjectStore {
    /// 从用户数据目录读取样式库；不存在返回空库，损坏、重复身份/名称或超量拒绝读取。
    pub fn load_text_presets(&self) -> Result<Vec<TextPreset>, String> {
        let path = self.index_path.with_file_name("text-presets.json");
        let bytes = match fs::read(path) {
            Ok(bytes) => bytes,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(e) => return Err(format!("读取文本样式失败：{e}")),
        };
        if bytes.len() > 1024 * 1024 {
            return Err("文本样式库过大".into());
        }
        let data: PresetsV1 =
            serde_json::from_slice(&bytes).map_err(|e| format!("文本样式库损坏：{e}"))?;
        if data.version != 1 || data.presets.len() > 100 {
            return Err("文本样式库版本或数量无效".into());
        }
        let mut ids = std::collections::BTreeSet::new();
        let mut names = std::collections::BTreeSet::new();
        for preset in &data.presets {
            preset.validate()?;
            if !ids.insert(&preset.id) || !names.insert(preset.name.to_lowercase()) {
                return Err("文本样式库存在重复项".into());
            }
        }
        Ok(data.presets)
    }

    /// 保存一个命名样式，不保存正文、时间、字幕分组或位置；重名返回错误，不静默覆盖旧样式。
    /// 原子写入成功后返回完整列表，供宿主同步缓存；失败保留既有库和项目。
    pub fn save_text_preset(
        &self,
        name: &str,
        style: crate::text::TextStyle,
    ) -> Result<Vec<TextPreset>, String> {
        let preset = TextPreset {
            id: Uuid::new_v4().to_string(),
            name: name.trim().to_owned(),
            style,
        };
        preset.validate()?;
        let mut presets = self.load_text_presets()?;
        if presets.len() >= 100 {
            return Err("文本样式最多保存 100 项".into());
        }
        if presets
            .iter()
            .any(|p| p.name.to_lowercase() == preset.name.to_lowercase())
        {
            return Err("文本样式名称已存在".into());
        }
        presets.push(preset);
        atomic_json(
            &self.index_path.with_file_name("text-presets.json"),
            &PresetsV1 {
                version: 1,
                presets: presets.clone(),
            },
        )?;
        Ok(presets)
    }

    /// 按稳定身份移除用户预设，不改变任何已应用到片段的样式；未知身份返回错误，失败保留旧文件。
    pub fn remove_text_preset(&self, id: &str) -> Result<Vec<TextPreset>, String> {
        let mut presets = self.load_text_presets()?;
        let index = presets
            .iter()
            .position(|p| p.id == id)
            .ok_or("文本样式不存在")?;
        presets.remove(index);
        atomic_json(
            &self.index_path.with_file_name("text-presets.json"),
            &PresetsV1 {
                version: 1,
                presets: presets.clone(),
            },
        )?;
        Ok(presets)
    }
}
