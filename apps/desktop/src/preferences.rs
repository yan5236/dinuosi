//! GPUI 全局偏好适配；存储由 Core 负责，运行时只在保存成功后发布新值。
use dinuosi_core::{Preferences, ProjectStore};
use gpui::{App, Global};

#[derive(Default)]
pub struct UserPreferences(pub Preferences);
impl Global for UserPreferences {}

/// 首个工作区载入并校验偏好，后续窗口共享同一份值；损坏文件返回错误且不覆盖磁盘。
pub fn initialize(store: &ProjectStore, cx: &mut App) -> Result<(), String> {
    if cx.has_global::<UserPreferences>() {
        return Ok(());
    }
    cx.set_global(UserPreferences::default());
    let data = store.load_preferences()?;
    crate::shortcuts::validate(&data)?;
    cx.set_global(UserPreferences(data));
    Ok(())
}

/// 验证快捷键并原子写入用户偏好；失败时内存与磁盘旧设置均保持不变。
pub fn save(store: &ProjectStore, data: Preferences, cx: &mut App) -> Result<(), String> {
    crate::shortcuts::validate(&data)?;
    store.save_preferences(&data)?;
    cx.set_global(UserPreferences(data));
    Ok(())
}
