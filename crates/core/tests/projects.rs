use dinuosi_core::ProjectStore;
use std::{fs, path::PathBuf};
use uuid::Uuid;

struct Workspace(PathBuf);
impl Workspace {
    /// 创建隔离临时目录供真实磁盘回归使用，名称带随机 UUID，不复用用户目录。
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!("dinuosi-test-{}", Uuid::new_v4()));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
    /// 在隔离目录中创建独立用户数据索引句柄；不触碰实际 LOCALAPPDATA。
    fn store(&self) -> ProjectStore {
        ProjectStore::new(self.0.join("data/recent.json"))
    }
}
impl Drop for Workspace {
    /// 清理本测试独占的临时目录，避免测试失败时遗留项目文件。
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// 用真实磁盘覆盖创建、重新实例化存储、改名、仅移除、导入和删除目录；外部素材必须保留。
#[test]
fn persistent_project_lifecycle() {
    let workspace = Workspace::new();
    let store = workspace.store();
    let entry = store.create("  测试项目  ", &workspace.0).unwrap();
    let first = store.open(&entry.path).unwrap();
    assert_eq!(first.name(), "测试项目");
    let restarted = workspace.store();
    assert_eq!(restarted.list().unwrap()[0].id, first.id().to_string());
    assert_eq!(restarted.open(&entry.path).unwrap(), first);
    let renamed = restarted.rename(&entry, "重命名项目").unwrap();
    assert_eq!(renamed.id, entry.id);
    assert_eq!(renamed.path, entry.path);
    assert_eq!(restarted.open(&entry.path).unwrap().name(), "重命名项目");
    assert_eq!(workspace.store().list().unwrap()[0].name, "重命名项目");
    restarted.delete(&renamed, false).unwrap();
    assert!(entry.path.exists());
    assert!(workspace.store().list().unwrap().is_empty());
    let imported = store.import(&entry.path).unwrap();
    store.import(&entry.path).unwrap();
    assert_eq!(store.list().unwrap().len(), 1);
    let external = workspace.0.join("external-video.mp4");
    fs::write(&external, b"external media").unwrap();
    fs::write(
        entry.path.parent().unwrap().join("inside.txt"),
        b"project asset",
    )
    .unwrap();
    store.delete(&imported, true).unwrap();
    assert!(!entry.path.parent().unwrap().exists());
    assert!(external.exists());
    assert!(store.list().unwrap().is_empty());
}

/// 验证名称信任边界、同名目录保护、损坏/未来格式拒绝、无所有权目录保护和丢失项目移除。
#[test]
fn invalid_inputs_and_unowned_folders_are_safe() {
    let workspace = Workspace::new();
    let store = workspace.store();
    for name in [
        "",
        " \t ",
        "../escape",
        "CON",
        "COM1",
        "LPT¹",
        "a/b",
        "abc.",
        "line\nname",
    ] {
        assert!(
            store.create(name, &workspace.0).is_err(),
            "接受了无效名称：{name}"
        );
    }
    let entry = store.create("existing", &workspace.0).unwrap();
    let original = fs::read(&entry.path).unwrap();
    assert!(store.create("existing", &workspace.0).is_err());
    assert_eq!(fs::read(&entry.path).unwrap(), original);
    fs::remove_file(entry.path.parent().unwrap().join(".dinuosi-owner")).unwrap();
    assert!(!store.can_delete_folder(&entry));
    assert!(store.delete(&entry, true).is_err());
    assert!(entry.path.exists());
    assert_eq!(store.list().unwrap().len(), 1);
    fs::write(&entry.path, br#"{"format":"dinuosi-project","version":99}"#).unwrap();
    assert!(store.open(&entry.path).unwrap_err().contains("版本"));
    assert!(store.rename(&entry, "future").is_err());
    fs::write(&entry.path, b"bad json").unwrap();
    assert!(store.open(&entry.path).is_err());
    fs::remove_file(&entry.path).unwrap();
    store.delete(&entry, false).unwrap();
    assert!(store.list().unwrap().is_empty());
}

/// 验证损坏索引不会被重置，索引写入失败时新建清理、重命名与删除能保留既有项目。
#[test]
fn index_failures_preserve_existing_data() {
    let workspace = Workspace::new();
    let store = workspace.store();
    let index = workspace.0.join("data/recent.json");
    fs::create_dir_all(index.parent().unwrap()).unwrap();
    fs::write(&index, b"broken index").unwrap();
    assert!(store.create("not-created", &workspace.0).is_err());
    assert!(!workspace.0.join("not-created").exists());
    assert_eq!(fs::read(&index).unwrap(), b"broken index");
    fs::remove_file(&index).unwrap();
    let entry = store.create("persist", &workspace.0).unwrap();
    let before = fs::read(&entry.path).unwrap();
    // 文件只读使 Windows 的原子替换失败；Unix 对目录内 rename 不受目标只读影响。
    #[cfg(target_os = "windows")]
    {
        let original_permissions = fs::metadata(&index).unwrap().permissions();
        let mut permissions = original_permissions.clone();
        permissions.set_readonly(true);
        fs::set_permissions(&index, permissions.clone()).unwrap();
        assert!(store.create("rollback-create", &workspace.0).is_err());
        assert!(!workspace.0.join("rollback-create").exists());
        assert!(store.rename(&entry, "rollback-rename").is_err());
        assert_eq!(fs::read(&entry.path).unwrap(), before);
        assert!(store.delete(&entry, true).is_err());
        assert!(entry.path.exists());
        fs::set_permissions(&index, original_permissions).unwrap();
    }
    assert_eq!(store.open(&entry.path).unwrap().name(), "persist");
    assert_eq!(fs::read(&entry.path).unwrap(), before);
}
