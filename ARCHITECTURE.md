# 初始化架构

GUI 是人类接口，CLI 是脚本接口，MCP 是 AI 接口，Action 是自动化接口，Plugin 是扩展接口；它们下面共享的始终只有一个 Dinuosi Runtime。

完整目标以本地 `.ref/设计文档.md` 为准，该参考目录不进入版本控制。当前是初始化基线，尚未实现完整 Runtime。

```text
apps/desktop (GPUI + 窗口会话)
    ├── dinuosi-command → dinuosi-core
    └── dinuosi-icons
assets/brand → 编译嵌入 Desktop
```

初始化仅建立有实际代码职责的四个 package。设计文档建议的 9–12 个主要 crate 将随媒体、渲染、任务等模块实际落地逐步建立，不提前添加空壳。

## 当前边界

- Core 定义内存项目模型和稳定 UUID，不依赖 GUI，不直接序列化内部结构。
- Command 是业务入口；新建项目调用 Core 校验。它不读写磁盘，也不改动已有项目。
- Desktop 保存当前窗口的项目句柄和显示状态，只通过命令新建项目。
- Icons 不依赖 GPUI；以素材 `spec.json` 为来源，在 `OUT_DIR` 生成类型化枚举、中文标签、逻辑资源路径及原始 SVG 内容。
- GPUI 的 AssetSource 提供内嵌 SVG 与 PNG；SVG 使用 GPUI 单色渲染并由界面文字色着色，双色 Logo 使用原始 PNG 保留品牌色。

## 下一阶段

按原设计顺序建立时间线模型、编辑命令与 Undo/Redo，再接入媒体导入、预览及基础导出。引入时间线时使用有理数时间与独立对象 ID；所有修改既有项目的命令必须记录历史。

项目保存需独立定义 `ProjectFormatV1`、版本标识和迁移入口，不能把当前内部模型直接当作磁盘格式。EditorSession 和 Preferences 必须与 ProjectState 分离。

耗时媒体工作再引入 Media、Render、Task；无头 Runtime、CLI、Action、插件及 MCP 复用共同业务入口。初始化阶段尚无这些服务，不为它们预置空接口或虚假实现。

## 素材接入记录

轻线来源：`E:\轻线图标-64枚完整套装\qingxian-icons-88`。虽然目录名保留 88，当前清单为 v1.5，共 89 枚，包含 `replace-media`。原始 SVG 保留 24×24 viewBox、1.8 描边、currentColor 和圆角端点/连接。

Logo 来源：`E:\轻线图标-64枚完整套装\Dinuosi-Logo-Kit`。项目在 `assets/brand` 保留 PNG 导出文件与可编辑 SVG 原稿，界面采用主标志 PNG；本地参考素材放在不进入版本控制的 `.ref` 目录。
