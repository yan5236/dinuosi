//! 不依赖 GUI 的项目领域模型、媒体处理及版本化项目存储。

pub mod composite;
pub mod language;
pub mod media;
pub mod media_stream;
mod midi;
mod project_settings;
mod storage;
pub mod timeline;
pub mod timeline_audio;
mod timeline_edit;
mod timeline_file;
pub mod waveform;
pub use storage::preferences::Preferences;
pub use storage::{MediaImport, ProjectEntry, ProjectStore};

use uuid::Uuid;

/// 项目身份与名称；不包含编辑会话、用户设置或磁盘序列化契约。
#[derive(Debug, Clone, PartialEq)]
pub struct Project {
    id: Uuid,
    name: String,
    created_at: u64,
    modified_at: u64,
    media_paths: Vec<std::path::PathBuf>,
    media_names: std::collections::BTreeMap<std::path::PathBuf, String>,
    timeline: timeline::Timeline,
}

impl Project {
    /// 创建独立的内存项目，去除名称首尾空白并生成稳定 UUID。
    /// 名称为空、过长或不是合法跨平台目录名时返回错误；不创建文件或修改源素材。
    pub fn new(name: &str) -> Result<Self, &'static str> {
        let name = validate_name(name)?;
        let now = timestamp();
        Ok(Self {
            id: Uuid::new_v4(),
            name: name.to_owned(),
            created_at: now,
            modified_at: now,
            media_paths: Vec::new(),
            media_names: Default::default(),
            timeline: Default::default(),
        })
    }

    /// 返回项目的稳定身份；调用方不能通过此方法更改项目 ID。
    pub fn id(&self) -> Uuid {
        self.id
    }

    /// 借用经过校验的项目名称；不会复制字符串或修改项目。
    pub fn name(&self) -> &str {
        &self.name
    }

    /// 返回 UTC Unix 秒级创建时间；与窗口会话和文件系统修改时间无关。
    pub fn created_at(&self) -> u64 {
        self.created_at
    }

    /// 返回最近一次项目内容保存的 UTC Unix 秒级时间；打开项目不会更改该值。
    pub fn modified_at(&self) -> u64 {
        self.modified_at
    }
}

/// 校验跨平台可用的项目目录名称并去除首尾空白；拒绝路径、控制字符和 Windows 保留名称。
/// 返回借用的规范名称；失败不会创建目录或修改已有项目。
fn validate_name(name: &str) -> Result<&str, &'static str> {
    let name = name.trim();
    if name.is_empty() {
        return Err("项目名称不能为空");
    }
    if name.chars().count() > 120
        || name
            .chars()
            .any(|c| c.is_control() || "<>:\"/\\|?*".contains(c))
        || name.ends_with('.')
    {
        return Err("项目名称最多 120 个字符，不能包含路径或特殊文件名字符");
    }
    let stem = name
        .split('.')
        .next()
        .unwrap_or_default()
        .to_ascii_uppercase();
    if matches!(
        stem.as_str(),
        "CON" | "PRN" | "AUX" | "NUL" | "CONIN$" | "CONOUT$"
    ) || ["COM", "LPT"].iter().any(|prefix| {
        stem.strip_prefix(prefix).is_some_and(|suffix| {
            matches!(
                suffix,
                "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9" | "¹" | "²" | "³"
            )
        })
    }) {
        return Err("项目名称不能使用系统保留文件名");
    }
    Ok(name)
}

/// 获取 UTC Unix 秒；系统时钟早于纪元时返回零，不使项目创建崩溃。
fn timestamp() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
