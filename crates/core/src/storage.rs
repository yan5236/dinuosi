//! 项目文件与最近项目索引的磁盘边界；内部 Project 不直接参与序列化。

use crate::{Project, timestamp, validate_name};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::{self, Write},
    path::{Path, PathBuf},
};
use uuid::Uuid;

const PROJECT_FILE: &str = "project.dinuosi";
const OWNER_FILE: &str = ".dinuosi-owner";

/// 项目管理的持久化摘要；路径指向项目文件，丢失的项目仍保留可移除的记录。
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ProjectEntry {
    pub id: String,
    pub name: String,
    pub path: PathBuf,
    pub created_at: u64,
    pub modified_at: u64,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ProjectFormatV1 {
    format: String,
    version: u32,
    id: String,
    name: String,
    created_at: u64,
    modified_at: u64,
}

#[derive(Serialize, Deserialize)]
struct RecentProjectsV1 {
    version: u32,
    projects: Vec<ProjectEntry>,
}

/// 独立于 GUI 的项目存储；索引位置由宿主指定，所有失败均返回可显示的错误。
// ponytail: 当前宿主单窗口串行写入；需要多个进程同时管理同一索引时再加入跨进程文件锁。
#[derive(Clone)]
pub struct ProjectStore {
    index_path: PathBuf,
}

impl ProjectStore {
    /// 创建存储句柄，不读写磁盘；调用者提供用户数据目录中的索引文件绝对路径。
    pub fn new(index_path: PathBuf) -> Self {
        Self { index_path }
    }

    /// 读取最近项目；首次运行返回空列表，损坏或未知版本索引返回错误，绝不静默覆盖。
    pub fn list(&self) -> Result<Vec<ProjectEntry>, String> {
        let bytes = match fs::read(&self.index_path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(error) => return Err(format!("读取项目列表失败：{error}")),
        };
        let index: RecentProjectsV1 =
            serde_json::from_slice(&bytes).map_err(|e| format!("项目列表损坏：{e}"))?;
        if index.version != 1 {
            return Err("不支持此项目列表版本".into());
        }
        Ok(index.projects)
    }

    /// 在用户选择的父目录下创建独占项目子目录并持久化项目和索引。
    /// 同名目录已存在时拒绝覆盖；索引写入失败时只清理本次新建的目录。
    pub fn create(&self, name: &str, parent: &Path) -> Result<ProjectEntry, String> {
        let project = Project::new(name).map_err(str::to_owned)?;
        let mut entries = self.list()?;
        let parent = fs::canonicalize(parent).map_err(|e| format!("项目目录不可用：{e}"))?;
        let folder = parent.join(project.name());
        fs::create_dir(&folder)
            .map_err(|e| format!("无法创建项目目录（同名目录不会覆盖）：{e}"))?;
        let path = folder.join(PROJECT_FILE);
        let result = (|| {
            atomic_json(&path, &ProjectFormatV1::from_project(&project))?;
            fs::write(folder.join(OWNER_FILE), project.id().to_string())
                .map_err(|e| e.to_string())?;
            let entry = ProjectEntry::from_project(&project, path);
            entries.insert(0, entry.clone());
            self.write_index(entries)?;
            Ok(entry)
        })();
        if result.is_err() {
            // 仅删除刚刚通过 create_dir 独占创建的目录，不清理父目录或任何既有同名目录。
            if let Err(cleanup) = fs::remove_dir_all(&folder) {
                return Err(format!(
                    "创建失败，且无法清理新建目录 {}：{cleanup}",
                    folder.display()
                ));
            }
        }
        result
    }

    /// 校验项目格式、版本、UUID、名称及时间后转换为领域模型；未来版本不会被当成 V1 保存。
    pub fn open(&self, path: &Path) -> Result<Project, String> {
        let bytes = fs::read(path).map_err(|e| format!("无法打开项目 {}：{e}", path.display()))?;
        let header: serde_json::Value =
            serde_json::from_slice(&bytes).map_err(|e| format!("项目文件损坏：{e}"))?;
        if header.get("format").and_then(|v| v.as_str()) != Some("dinuosi-project")
            || header.get("version").and_then(|v| v.as_u64()) != Some(1)
        {
            return Err("不支持此项目格式或版本".into());
        }
        let file: ProjectFormatV1 =
            serde_json::from_value(header).map_err(|e| format!("项目数据不完整：{e}"))?;
        let name = validate_name(&file.name).map_err(str::to_owned)?.to_owned();
        let id = Uuid::parse_str(&file.id).map_err(|_| "项目 ID 无效")?;
        if id.is_nil() || file.modified_at < file.created_at {
            return Err("项目身份或时间无效".into());
        }
        Ok(Project {
            id,
            name,
            created_at: file.created_at,
            modified_at: file.modified_at,
        })
    }

    /// 打开外部项目文件并登记最近项目；同一路径只保留一项，不复制或移动源文件。
    pub fn import(&self, path: &Path) -> Result<ProjectEntry, String> {
        let path = fs::canonicalize(path).map_err(|e| format!("项目文件不可用：{e}"))?;
        let project = self.open(&path)?;
        let entry = ProjectEntry::from_project(&project, path);
        let mut entries = self.list()?;
        entries.retain(|item| item.path != entry.path);
        entries.insert(0, entry.clone());
        self.write_index(entries)?;
        Ok(entry)
    }

    /// 重命名项目的显示名称并保存项目和索引；保留目录与 UUID，以免破坏媒体相对路径。
    /// 索引更新失败时恢复原项目文件，并明确报告恢复失败。
    pub fn rename(&self, entry: &ProjectEntry, name: &str) -> Result<ProjectEntry, String> {
        let name = validate_name(name).map_err(str::to_owned)?;
        let mut entries = self.list()?;
        let index = entries
            .iter()
            .position(|e| e.path == entry.path && e.id == entry.id)
            .ok_or("项目已不在列表中")?;
        let original = self.open(&entry.path)?;
        if original.id().to_string() != entry.id {
            return Err("项目文件身份已改变，操作已取消".into());
        }
        let mut project = original.clone();
        project.name = name.to_owned();
        project.modified_at = timestamp().max(project.modified_at);
        atomic_json(&entry.path, &ProjectFormatV1::from_project(&project))?;
        let renamed = ProjectEntry::from_project(&project, entry.path.clone());
        entries[index] = renamed.clone();
        if let Err(error) = self.write_index(entries) {
            atomic_json(&entry.path, &ProjectFormatV1::from_project(&original))
                .map_err(|rollback| format!("{error}；恢复项目失败：{rollback}"))?;
            return Err(error);
        }
        Ok(renamed)
    }

    /// 判断是否可以删除整份项目目录；只允许带匹配所有权标记的本软件项目目录。
    /// 拒绝链接、磁盘根、索引所在目录及其祖先，防止导入项目误删用户的共享素材目录。
    pub fn can_delete_folder(&self, entry: &ProjectEntry) -> bool {
        let Some(folder) = entry.path.parent() else {
            return false;
        };
        let Ok(metadata) = fs::symlink_metadata(folder) else {
            return false;
        };
        let Ok(canonical) = fs::canonicalize(folder) else {
            return false;
        };
        let Ok(index_parent) = fs::canonicalize(self.index_path.parent().unwrap_or(Path::new("")))
        else {
            return false;
        };
        let Ok(owner) = fs::read_to_string(folder.join(OWNER_FILE)) else {
            return false;
        };
        entry
            .path
            .file_name()
            .is_some_and(|name| name == PROJECT_FILE)
            && metadata.is_dir()
            && !metadata.file_type().is_symlink()
            && canonical.parent().is_some()
            && !index_parent.starts_with(&canonical)
            && owner == entry.id
            && self
                .open(&entry.path)
                .is_ok_and(|p| p.id().to_string() == entry.id)
    }

    /// 默认只从最近列表移除项目；显式勾选时删除已验证所有权的项目目录。
    /// 删除前先把目录暂移到同级隔离目录；索引保存失败会恢复目录，不删除外部引用的媒体。
    pub fn delete(&self, entry: &ProjectEntry, delete_folder: bool) -> Result<(), String> {
        let mut entries = self.list()?;
        if !entries
            .iter()
            .any(|e| e.path == entry.path && e.id == entry.id)
        {
            return Err("项目已不在列表中".into());
        }
        entries.retain(|e| e.path != entry.path);
        if !delete_folder {
            return self.write_index(entries);
        }
        if !self.can_delete_folder(entry) {
            return Err("此目录无法安全删除，请仅从列表移除项目".into());
        }
        let folder = entry.path.parent().ok_or("项目目录无效")?;
        let staged = folder.with_file_name(format!(".dinuosi-delete-{}", Uuid::new_v4()));
        fs::rename(folder, &staged).map_err(|e| format!("无法移除项目目录：{e}"))?;
        if let Err(error) = self.write_index(entries) {
            fs::rename(&staged, folder).map_err(|e| format!("{error}；目录恢复失败：{e}"))?;
            return Err(error);
        }
        fs::remove_dir_all(&staged)
            .map_err(|e| format!("已从列表移除，但目录清理未完成 {}：{e}", staged.display()))
    }

    /// 原子替换版本化最近项目索引；失败保留原文件，不把损坏索引转换成空列表。
    fn write_index(&self, projects: Vec<ProjectEntry>) -> Result<(), String> {
        atomic_json(
            &self.index_path,
            &RecentProjectsV1 {
                version: 1,
                projects,
            },
        )
    }
}

impl ProjectFormatV1 {
    /// 从领域模型显式构造 V1 磁盘契约，不持久化 GUI 会话状态。
    fn from_project(project: &Project) -> Self {
        Self {
            format: "dinuosi-project".into(),
            version: 1,
            id: project.id().to_string(),
            name: project.name().to_owned(),
            created_at: project.created_at(),
            modified_at: project.modified_at(),
        }
    }
}

impl ProjectEntry {
    /// 从已校验项目生成最近列表摘要；路径由存储入口确定，不从项目 JSON 信任外部路径。
    fn from_project(project: &Project, path: PathBuf) -> Self {
        Self {
            id: project.id().to_string(),
            name: project.name().to_owned(),
            path,
            created_at: project.created_at(),
            modified_at: project.modified_at(),
        }
    }
}

/// 在同一目录写入独占临时文件，刷盘后原子替换目标 JSON；失败删除临时文件并保留原目标。
/// 不先截断既有文件，进程中断时最多留下不参与加载的临时文件。
fn atomic_json(path: &Path, value: &impl Serialize) -> Result<(), String> {
    let bytes = serde_json::to_vec_pretty(value).map_err(|e| e.to_string())?;
    let parent = path.parent().ok_or("保存路径缺少父目录")?;
    fs::create_dir_all(parent).map_err(|e| format!("无法创建数据目录：{e}"))?;
    let temporary = parent.join(format!(".{}.tmp", Uuid::new_v4()));
    let result = (|| -> io::Result<()> {
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)?;
        file.write_all(&bytes)?;
        file.sync_all()?;
        drop(file);
        fs::rename(&temporary, path)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result.map_err(|e| format!("保存 {} 失败：{e}", path.display()))
}
