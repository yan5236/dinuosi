//! GUI 与未来无头入口共享的命令层；业务校验由核心模型执行。

use dinuosi_core::{Project, ProjectEntry, ProjectStore};
use std::path::PathBuf;

/// 项目生命周期命令；目录与文件操作统一由 Core 执行，不依赖窗口或 GPUI。
pub enum ProjectCommand {
    Create {
        name: String,
        parent: PathBuf,
    },
    Open {
        path: PathBuf,
    },
    Rename {
        entry: ProjectEntry,
        name: String,
    },
    Delete {
        entry: ProjectEntry,
        delete_folder: bool,
    },
}

/// 项目管理命令结果；打开或修改返回持久化摘要，移除返回明确的完成标记。
pub enum ProjectOutcome {
    Entry(ProjectEntry),
    Deleted,
}

/// 执行磁盘项目命令并传播核心错误；成功前不报告创建、重命名或删除完成。
/// 重命名属于管理元数据操作，时间线编辑历史仍由未来编辑命令负责。
pub fn execute_project(
    store: &ProjectStore,
    command: ProjectCommand,
) -> Result<ProjectOutcome, String> {
    match command {
        ProjectCommand::Create { name, parent } => {
            store.create(&name, &parent).map(ProjectOutcome::Entry)
        }
        ProjectCommand::Open { path } => store.import(&path).map(ProjectOutcome::Entry),
        ProjectCommand::Rename { entry, name } => {
            store.rename(&entry, &name).map(ProjectOutcome::Entry)
        }
        ProjectCommand::Delete {
            entry,
            delete_folder,
        } => store
            .delete(&entry, delete_folder)
            .map(|()| ProjectOutcome::Deleted),
    }
}

/// 初始化阶段提供的新建项目命令；时间线编辑命令随实际功能加入。
#[derive(Debug, Clone)]
pub enum EditCommand {
    CreateProject { name: String },
}

/// 执行新建项目命令并返回独立的内存项目；校验失败时原样返回核心错误。
/// 不依赖 GPUI，不读写磁盘，也不修改已有项目；后续编辑命令必须加入历史记录。
pub fn execute(command: EditCommand) -> Result<Project, &'static str> {
    match command {
        EditCommand::CreateProject { name } => Project::new(&name),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 验证无 GUI 的命令入口能创建唯一项目、规范名称并拒绝空白输入。
    #[test]
    fn create_project_validates_name_and_assigns_independent_identity() {
        let first = execute(EditCommand::CreateProject {
            name: "  迪诺斯项目  ".into(),
        })
        .unwrap();
        let second = execute(EditCommand::CreateProject {
            name: "迪诺斯项目".into(),
        })
        .unwrap();
        assert_eq!(first.name(), "迪诺斯项目");
        assert!(!first.id().is_nil());
        assert_ne!(first.id(), second.id());
        assert_eq!(
            execute(EditCommand::CreateProject {
                name: " \t\n".into()
            }),
            Err("项目名称不能为空")
        );
    }
}
