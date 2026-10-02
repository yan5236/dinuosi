//! 不依赖 GUI 的项目领域模型；磁盘格式与迁移将在项目存储阶段独立定义。

use uuid::Uuid;

/// 项目身份与名称；不包含编辑会话、用户设置或磁盘序列化契约。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Project {
    id: Uuid,
    name: String,
}

impl Project {
    /// 创建独立的内存项目，去除名称首尾空白并生成稳定 UUID。
    /// 名称为空或只有空白时返回错误；不创建文件或修改源素材。
    pub fn new(name: &str) -> Result<Self, &'static str> {
        let name = name.trim();
        if name.is_empty() {
            return Err("项目名称不能为空");
        }
        Ok(Self {
            id: Uuid::new_v4(),
            name: name.to_owned(),
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
}
