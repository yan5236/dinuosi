# Dinuosi · 迪诺斯

本地优先的开源视频剪辑软件，采用 Rust + GPUI。PRD、设计文档和界面参考图位于本地 `.ref/`，不进入版本控制；提交中的模块边界见 [ARCHITECTURE.md](ARCHITECTURE.md)。

## 开发

Windows 开发环境需 Rust stable（MSVC）、Visual Studio C++ Build Tools 和 Windows SDK。GPUI 固定为 0.2.2；应用依赖锁定在 Cargo.lock 中。其他平台的系统依赖参见 [GPUI 文档](https://docs.rs/gpui/0.2.2/gpui/)。

```powershell
cargo run -p dinuosi-desktop
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

首次构建需要联网下载依赖；图标和界面 Logo 已嵌入程序，运行不依赖原始素材目录。仅验证非 GUI 模块可运行 `cargo test -p dinuosi-core -p dinuosi-command -p dinuosi-icons`。

## 初始化范围

- `apps/desktop`：GPUI 启动页、内存中新建项目、PRD 所规定的空编辑工作区。
- `crates/core`：项目身份、名称校验；不依赖 GPUI。
- `crates/command`：共享的新建项目命令入口，可独立于 GUI 执行。
- `crates/icons`：轻线 v1.5 的 89 枚原始 SVG、清单、说明，以及构建生成的 `Icon` 枚举。
- `assets/brand`：Logo PNG 导出文件与可编辑 SVG 原稿。

项目管理页支持点击按钮、Enter 或空格新建项目；Ctrl+N 新建，Ctrl+H 或剪辑页 Logo 返回项目管理，同一会话内可重新打开项目。当前项目仅保存在内存，关闭窗口后不会保留。导入、保存、设置、时间线编辑、预览、导出尚未实现，对应入口按需提示或置灰。没有引入 FFmpeg、AI 或插件运行时。

桌面采用 GPUI 自绘标题栏、中性灰背景和细描边胶囊按钮；布局依据两张本地草图，视觉约定与当前能力见 [PRD.md](PRD.md)。

## 架构与后续开发

GUI 负责窗口、布局和会话；项目业务经 Command 进入 Core。依赖边界及扩展顺序见 [ARCHITECTURE.md](ARCHITECTURE.md)。图标按 `crates/icons/spec.json` 生成，修改清单或 SVG 后 Cargo 自动重新构建，不应手工维护生成文件。

软件及素材的公开发布许可证待确认，当前所有 package 均禁止发布到 crates.io；没有为用户素材擅自指定许可证。

## 初始化验证（2026-10-02）

Windows / Rust 1.98.0：格式检查、Clippy 全工作区全目标严格检查、3 项测试、桌面构建均通过。全部 89 枚图标及完整 Logo Kit 复制前后哈希一致，图标 XML/viewBox 校验通过。

程序从独立 `.local` 工作目录启动后持续运行 5 秒，未立即退出。此检查只证明启动存活，不代表界面显示或点击已验证。Computer Use 工具因沙箱初始化错误无法启动，实际窗口截图、鼠标点击与键盘交互尚未验证。

上游间接依赖 `proc-macro-error2 2.0.1` 仍有未来 Rust 兼容性警告，当前构建和测试通过。

## 本轮界面验证（2026-10-02）

- 格式检查、全工作区全目标 Clippy 严格检查、5 项测试通过；包含 GPUI 原生布局与点击回归，在 1440×900、1040×640 下验证窗口按钮位置、三栏分界、新建、返回和标尺缩放。
- 已实际启动 Windows 应用。根据用户提供的运行截图定位并修复窗口图标不显示：GPUI 0.2.2 的 `Svg` 必须显式设置颜色，不能只依赖父容器文字色。
- 沙箱初始化故障已修复。Computer Use 的截图仍超时，已改用 Windows 窗口坐标配合桌面像素抓取，取得两页真实截图；截图保存在忽略目录 `.local/`。
- 修复标题栏不能拖动：根节点的 GPUI 自动聚焦会阻止 Windows 默认鼠标行为，现将快捷键焦点放在零尺寸节点。两页拖动均经鼠标实测及窗口坐标确认；管理页最小化、关闭，剪辑页最大化后最小化及还原均通过。窗口按钮由 Windows 命中区域执行，保留自绘外观。
- GPUI 回归新增标题栏和窗口按钮不得阻止系统默认鼠标行为的断言，保留两种尺寸下的布局、新建、返回和缩放检查。
- 测试链接阶段出现 MSVC 创建导入库的 `linker stdout` 提示；测试成功，未将其视为截图或窗口交互证据。
