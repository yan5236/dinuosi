//! 项目文件与最近项目索引的磁盘边界；内部 Project 不直接参与序列化。

use crate::{
    Project,
    media::{self, Media},
    timestamp, validate_name,
};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::{self, Write},
    path::{Path, PathBuf},
};
use uuid::Uuid;

const PROJECT_FILE: &str = "project.dinuosi";
const OWNER_FILE: &str = ".dinuosi-owner";
mod timeline;

/// 项目管理的持久化摘要；路径指向项目文件，丢失的项目仍保留可移除的记录。
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ProjectEntry {
    pub id: String,
    pub name: String,
    pub path: PathBuf,
    pub created_at: u64,
    pub modified_at: u64,
}

/// 批量导入结果：有效媒体已持久化，单个无效文件的错误独立返回给宿主呈现。
pub struct MediaImport {
    pub media: Vec<Media>,
    pub errors: Vec<String>,
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
    #[serde(default)]
    media_paths: Vec<PathBuf>,
    #[serde(default)]
    media_names: std::collections::BTreeMap<PathBuf, String>,
    #[serde(default)]
    timeline: Option<crate::timeline_file::TimelineV1>,
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
    /// 重开项目引用并探测素材，失联或无法读取的文件保留为离线项，不删除项目引用。
    /// 项目身份/格式错误直接返回；同步媒体读取由 GUI 后台任务或 CLI 自行调度。
    pub fn load_media(&self, entry: &ProjectEntry) -> Result<Vec<Media>, String> {
        let project = self.open(&entry.path)?;
        if project.id().to_string() != entry.id {
            return Err("项目文件身份已改变".into());
        }
        Ok(project
            .media_paths
            .into_iter()
            .map(|path| {
                let mut item = media::probe(&path).unwrap_or_else(|_| media::offline(path));
                if let Some(name) = project.media_names.get(&item.path) {
                    item.name = name.clone();
                }
                item
            })
            .collect())
    }

    /// 批量探测、规范化去重并保存源路径引用；坏文件逐一反馈，有效文件仍可导入。
    /// 项目/索引写入失败返回错误且不报告成功，绝不复制、删除或改写源媒体。
    pub fn import_media(
        &self,
        entry: &ProjectEntry,
        paths: &[PathBuf],
    ) -> Result<MediaImport, String> {
        let mut result = MediaImport {
            media: Vec::new(),
            errors: Vec::new(),
        };
        for path in paths {
            match media::probe(path) {
                Ok(item) => {
                    if !result
                        .media
                        .iter()
                        .any(|existing| existing.path == item.path)
                    {
                        result.media.push(item);
                    }
                }
                Err(error) => result.errors.push(format!("{}：{error}", path.display())),
            }
        }
        if !result.media.is_empty() {
            self.add_media_paths(
                entry,
                &result
                    .media
                    .iter()
                    .map(|m| m.path.clone())
                    .collect::<Vec<_>>(),
            )?;
            let project = self.open(&entry.path)?;
            for item in &mut result.media {
                if let Some(name) = project.media_names.get(&item.path) {
                    item.name = name.clone();
                }
            }
        }
        Ok(result)
    }

    /// 读取项目中保存的源素材路径；旧 V1 项目默认没有素材，不复制、打开或解码源文件。
    pub fn media_paths(&self, entry: &ProjectEntry) -> Result<Vec<PathBuf>, String> {
        let project = self.open(&entry.path)?;
        if project.id().to_string() != entry.id {
            return Err("项目文件身份已改变".into());
        }
        Ok(project.media_paths)
    }

    /// 合并经过媒体探测的源文件引用，规范化路径并去重，原子保存项目及修改时间。
    /// 文件不存在、项目身份改变或索引保存失败时返回错误；失败恢复项目，绝不复制或删除素材。
    pub fn add_media_paths(&self, entry: &ProjectEntry, paths: &[PathBuf]) -> Result<(), String> {
        let original = self.open(&entry.path)?;
        if original.id().to_string() != entry.id {
            return Err("项目文件身份已改变".into());
        }
        let mut project = original.clone();
        for path in paths {
            let path = fs::canonicalize(path).map_err(|e| format!("素材不可用：{e}"))?;
            if !path.is_file() {
                return Err("只能导入素材文件".into());
            }
            if !project.media_paths.contains(&path) {
                project.media_paths.push(path);
            }
        }
        project.modified_at = timestamp().max(project.modified_at);
        let mut entries = self.list()?;
        let item = entries
            .iter_mut()
            .find(|e| e.id == entry.id && e.path == entry.path)
            .ok_or("项目已不在列表中")?;
        item.modified_at = project.modified_at;
        atomic_json(&entry.path, &ProjectFormatV1::from_project(&project))?;
        if let Err(error) = self.write_index(entries) {
            atomic_json(&entry.path, &ProjectFormatV1::from_project(&original))
                .map_err(|rollback| format!("{error}；恢复项目失败：{rollback}"))?;
            return Err(error);
        }
        Ok(())
    }

    /// 更改素材库显示名称；路径不变，也不重命名磁盘文件，离线引用同样支持改名。
    /// 名称须为非空、无控制字符且不超过 120 字符；保存失败保留原引用并返回错误。
    pub fn rename_media(
        &self,
        entry: &ProjectEntry,
        path: &Path,
        name: &str,
    ) -> Result<(), String> {
        let name = name.trim();
        if name.is_empty() || name.chars().count() > 120 || name.chars().any(char::is_control) {
            return Err("素材名称须为 1–120 个字符，不能包含控制字符".into());
        }
        self.update_media_reference(entry, path, Some(name))
    }

    /// 仅移除项目内的素材引用及别名；不读取、移动或删除源文件，允许移除离线素材。
    /// 项目身份不符、引用不存在或保存失败时返回错误，不报告成功。
    pub fn remove_media(&self, entry: &ProjectEntry, path: &Path) -> Result<(), String> {
        self.update_media_reference(entry, path, None)
    }

    /// 按已保存路径更新别名或移除引用，原子保存项目及索引；索引失败时恢复原项目。
    /// 不规范化源路径，以保证源文件离线时仍可管理已有引用。
    fn update_media_reference(
        &self,
        entry: &ProjectEntry,
        path: &Path,
        name: Option<&str>,
    ) -> Result<(), String> {
        let original = self.open(&entry.path)?;
        if original.id().to_string() != entry.id {
            return Err("项目文件身份已改变".into());
        }
        if !original.media_paths.iter().any(|p| p == path) {
            return Err("素材已不在素材库中".into());
        }
        let mut project = original.clone();
        if let Some(name) = name {
            project.media_names.insert(path.to_owned(), name.to_owned());
        } else {
            project.media_paths.retain(|p| p != path);
            project.media_names.remove(path);
        }
        project.modified_at = timestamp().max(project.modified_at);
        let mut entries = self.list()?;
        let item = entries
            .iter_mut()
            .find(|e| e.id == entry.id && e.path == entry.path)
            .ok_or("项目已不在列表中")?;
        item.modified_at = project.modified_at;
        atomic_json(&entry.path, &ProjectFormatV1::from_project(&project))?;
        if let Err(error) = self.write_index(entries) {
            atomic_json(&entry.path, &ProjectFormatV1::from_project(&original))
                .map_err(|rollback| format!("{error}；恢复项目失败：{rollback}"))?;
            return Err(error);
        }
        Ok(())
    }

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
            media_paths: file.media_paths,
            media_names: file.media_names,
            timeline: file
                .timeline
                .map(|v| v.into_timeline())
                .transpose()?
                .unwrap_or_default(),
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
            media_paths: project.media_paths.clone(),
            media_names: project.media_names.clone(),
            timeline: Some(crate::timeline_file::TimelineV1::from_timeline(
                &project.timeline,
            )),
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
