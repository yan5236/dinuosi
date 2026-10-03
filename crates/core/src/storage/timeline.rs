//! 时间线的原子项目保存，使用与项目元数据一致的回滚约定。
use super::*;
impl ProjectStore {
    /// 读取经版本和身份校验的时间线；旧项目返回默认空轨道，不修改文件。
    pub fn load_timeline(&self, entry: &ProjectEntry) -> Result<crate::timeline::Timeline, String> {
        let project = self.open(&entry.path)?;
        if project.id().to_string() != entry.id {
            return Err("项目文件身份已改变".into());
        }
        Ok(project.timeline)
    }
    /// 验证并原子保存时间线与修改时间；索引保存失败恢复原项目，不保存 GUI 会话。
    pub fn save_timeline(
        &self,
        entry: &ProjectEntry,
        timeline: &crate::timeline::Timeline,
    ) -> Result<(), String> {
        timeline.validate()?;
        let original = self.open(&entry.path)?;
        if original.id().to_string() != entry.id {
            return Err("项目文件身份已改变".into());
        }
        let mut project = original.clone();
        project.timeline = timeline.clone();
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
}
