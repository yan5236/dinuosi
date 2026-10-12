# 开发指南

本文面向从源码运行、测试或打包 Dinuosi 的贡献者。软件当前的使用方式与功能范围见 [README](../README.md)。

> **先准备媒体运行时，再运行 Debug 或 Release。`cargo build/run` 不会自动编译或部署 FFmpeg；缺少程序旁的 `runtime/ffmpeg/` 会导致已有媒体全部显示离线。首次构建、清理 `target/` 后重新部署及所需 DLL 见根目录 [BUILDING.md](../BUILDING.md)。补齐运行库后，已启动的进程需要完全退出再重启。**

## 环境

Windows x64 的构建与发行环境：

- Rust stable，MSVC 工具链；`rust-toolchain.toml` 声明 rustfmt 和 Clippy，Rust 依赖由 `Cargo.lock` 锁定。
- PowerShell 7，沿用 CI 的 `pwsh` 执行准备及检查脚本。
- Visual Studio 2022 C++ Build Tools，包含 x64 编译工具、Windows SDK 和 C++ CMake 工具（CMake/Ninja/MSBuild）。构建 GPUI 时需要能从 PATH 找到 `fxc.exe`。
- MSYS2，媒体构建需要 Bash、make、patch、GPG、ar、tar、pkg-config 和 diff。
- Python 3，用于收集第三方许可与执行许可策略检查。

GPUI 固定为 0.2.2。Linux/macOS 原生构建、运行库准备和当前验证边界见 [跨平台支持](CROSS_PLATFORM.md)；三平台 CI 结果见 [导出平台验证](EXPORT_PLATFORM_VALIDATION.md)。上游系统依赖可参考 [GPUI 文档](https://docs.rs/gpui/0.2.2/gpui/)。

## 首次准备与运行

全新电脑按照根目录 [BUILDING.md](../BUILDING.md) 从安装工具、获取源码开始执行。仓库不包含 `.local/`、`target/` 和大型音源，也不要求从其他机器复制它们；首次运行必须使用 `prepare-media.ps1 -Build` 在本机生成。

准备脚本按 `distribution/inputs.lock.json` 下载并校验固定输入，提取 FluidR3 GM 音源，从源码构建 FFmpeg，并准备锁定的 NASM/NSIS。首次准备需联网；下载缓存位于 `.local/distribution/`。

`assets/audio/FluidR3_GM.sf2` 约 141.5 MiB，编译时嵌入程序。它不进入 Git，首次编译前必须准备，否则 `include_bytes!` 会因文件缺失失败。音源说明和原始许可见 [assets/audio](../assets/audio/README.md)。

Windows 脚本将桥接动态库和配套 libav 部署到 `target/debug`、`target/release` 及各自的 `deps` 测试程序目录；FFmpeg/ffprobe CLI 只部署到 `deps`，应用目录中的旧 CLI 会清理。只有本机仍保留已构建运行库时，重新部署才无需再次编译 FFmpeg：

```powershell
./tools/prepare-media.ps1 -DeployOnly
```

删除 `target/` 后需重新部署。当前脚本使用这些默认 Cargo 输出路径；自行更改 target 目录时，也需在相应程序旁部署 `runtime/ffmpeg/`。

`-DeployOnly` 在复制前检查桥接库、必要 libav 和测试工具是否完整，并按已有许可审计生成第三方声明；同时部署主程序 `LICENSE` 和 `licenses/` 到四个默认程序目录。它不下载、不重新提取音源，也不要求 MSYS2。许可材料缺失或 Cargo 输入变化时，先执行 `python tools/collect-licenses.py`，再重新部署。它不能与 `-Build` 同时使用；首次完整准备仍使用 BUILDING.md 中的 `-Build` 命令，并会获取锁定 Rust 依赖和收集许可。

## 媒体运行方式

Core 通过 `crates/media` 的安全接口直接调用 FFmpeg 8 的 libav 动态库，完成探测、BGRA/PCM 解码及时间线编码/封装。Desktop 负责界面绘制、播放调度与音频设备输出；应用正常媒体流程不创建 FFmpeg/ffprobe 子进程。Dinuosi 自身的用户 CLI 尚未实现。

桥接动态库及配套 libav 位于当前程序旁 `runtime/ffmpeg/`，不回退到 PATH。Windows 使用 `dinuosi_media.dll`，Linux/macOS 使用 `libdinuosi_media.so` / `libdinuosi_media.dylib`。动态库或 ABI 不匹配会返回具体错误。FFmpeg/ffprobe 可执行工具仅用于测试夹具、独立输出对照和构建许可检查；移除它们后应用仍可探测、播放及导出。

FFmpeg 固定为 8.0.3，保留禁止 GPL、nonfree、version3、自动探测和网络协议的约束，编码器与音源保持既有选择。MIT 原生桥接库动态链接可替换 libav，FFmpeg 算法代码不静态链接进 Rust 主程序；MIDI 继续由 RustySynth 与 FluidR3 合成。构建脚本同时编译桥接库，封存/打包记录桥接源码和构建脚本哈希，并随对应源码包交付。此链接边界按 [FFmpeg 官方分发要求](https://ffmpeg.org/legal.html)管理，现有公开发行审查项继续保留。

已有 FFmpeg 构建可单独重编桥接库：

```powershell
python tools/build-media-library.py --prefix .local/distribution/runtime/ffmpeg
./tools/seal-runtime.ps1 -MsysRoot $msysRoot -VcVars $vcVars
./tools/prepare-media.ps1 -DeployOnly
```

此步骤要求已保留锁定源码归档；源码包无法对应时封存或打包应失败。Unix 的准备脚本同时编译桥接库并记录全部原生输入；重新部署仍逐项校验哈希。导出行为与跨平台加载约定见 [EXPORT.md](EXPORT.md) 与 [跨平台支持](CROSS_PLATFORM.md)。

`.cargo/config.toml` 为 Windows MSVC 目标启用静态 C 运行库链接，使主程序无需用户另装 Visual C++ Redistributable。该文件是共享构建配置，不是本机 Cargo 缓存。

开发构建保留调试信息，同时优化 Core、MIDI 合成及 GPUI/Desktop 的实时路径。窗口平移会触发 GPUI 内容刷新，波形使用按设备像素和源时间缓存的纹理；刀片使用平台光标，避免将光标移动速度绑定到主界面绘制速度。

Windows Debug/Release 构建通过 Desktop 的 `build.rs` 将 `assets/brand/dinuosi.ico` 嵌入 EXE 的编号 1 图标资源；GPUI 0.2.2 的原生窗口、任务栏和资源管理器共用该图标，无需程序旁放置图片。资源编译失败会终止构建。ICO 来自已确认的 `assets/brand/png/dinuosi-app-icon-1024.png`，含 16/20/24/32/40/48/64/128/256px 透明帧；品牌图更新后用 `python tools/prepare-app-icon.py`（需 Pillow）重新生成并提交 ICO。非 Windows 构建跳过资源编译。

构建后运行 `python tools/verify-windows-icon.py` 验证 Debug，或追加 `target/release/dinuosi.exe` 验证 Release。检查会逐帧比较 EXE 资源与 ICO，并以隔离数据目录启动真实窗口，确认系统窗口类图标与 EXE 品牌图一致；证据保存在 `.local/windows-icon/`。

## 检查

```powershell
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
python ./tools/test-license-policy.py
./tools/test-media-deployment.ps1
```

常规测试默认忽略需要真实媒体工具的回归。完成媒体准备后，显式执行 Core 的这些检查：

```powershell
cargo test -p dinuosi-core --locked -- --ignored --nocapture --skip real_midi_preview_samples
```

`real_midi_preview_samples` 另需 `DINUOSI_VERIFY_MIDI` 指向实际 MIDI 素材；可选 `DINUOSI_VERIFY_OUTPUT` 指向已存在的试听输出目录。未指定素材时不要将该外部输入检查计作通过。

仅验证非 GUI 模块可运行：

```powershell
cargo test -p dinuosi-core -p dinuosi-command -p dinuosi-icons --locked
```

默认忽略的检查不能计作已通过。历史验证范围见 [历史验证记录](VALIDATION_HISTORY.md)，当前 Windows 内置运行时与安装生命周期的验收证据见 [发行验收报告](DISTRIBUTION_VALIDATION.md)。

导出 CI 的 Windows 首次运行从源码编译，日志实时显示并留档；FFmpeg 使用 `DINUOSI_BUILD_JOBS=2`，本机不指定时仍为单路。成功构建后缓存配套运行库、内嵌音源和运行库清单；键包含输入锁、补丁与构建脚本，命中后逐文件校验哈希，不使用模糊匹配或跨平台缓存。源码/构建配置变化需要重新构建。

## 生成安装包与便携版

先完成媒体准备，再执行：

```powershell
cargo build -p dinuosi-desktop --release --locked
python ./tools/collect-licenses.py
./tools/package.ps1
```

许可收集使用本机已下载的 Cargo 依赖，并在需要时联网查询原始许可材料。开发部署与发行打包共用 `tools/write-license-notices.ps1`，声明开头包含主程序 MIT 信息及完整原文，随后保留原生库和 Rust 依赖许可；关于窗口的“第三方许可证”打开程序旁的 `licenses/THIRD_PARTY_NOTICES.txt`，无需先生成安装包。打包会验证运行时、锁定源码、补丁及许可材料的一致性，输出到 `target/distribution/`：

- Windows x64 安装程序。
- 同内容便携 ZIP。
- FFmpeg、zlib、LAME、libaom、libvpx 及媒体桥接对应源码与重建脚本 ZIP。
- 第三方许可声明及产物校验清单。

安装器提供 English 与简体中文选择，保存到当前用户安装登记；再次交互安装可改选，卸载沿用安装语言。目录页、进度页、卸载确认、运行占用提示与卸载快捷方式均按所选语言显示。实现沿用 [NSIS Modern UI 2 的语言选择与登记流程](https://nsis.sourceforge.io/Docs/Modern%20UI%202/Readme.html#language-selection-dialog)。安装器语言不改写应用自身的语言偏好。

安装器与卸载器共用现有 `assets/brand/dinuosi.ico`，窗口及语言选择图标使用该品牌资源；向导底部品牌文字为 `Dinuosi`。

安装详情默认收起，按钮可反复展开与收起日志；该切换只影响进度页列表，不改变安装器窗口状态。安装结束保留“下一步”，进入“安装完成”页；“创建桌面快捷方式”和“立即运行 Dinuosi”默认勾选，仅点击“完成”后执行。开始菜单快捷方式仍随安装创建；静默安装跳过完成页，不创建桌面快捷方式或自动运行。卸载确认隐藏目录标签和路径框，实际卸载仍使用安装登记目录及原文件清单；只有 `DesktopShortcutCreated` 登记表明桌面快捷方式由本安装器创建时才删除，未创建时保留用户的同名文件。

打包时 `tools/build-installer-ui.py` 使用已有 MSVC x86 工具及锁定 NSIS 3.13 SDK 构建进度页按钮扩展。NSIS 安装器自身为 x86，不改变应用 x64；该扩展只嵌入安装器并在其临时目录运行，便携版和应用运行库不包含它。完成页复用已有 nsDialogs 与品牌页首，不引入额外向导图片。

`python tools/verify-installer-ui.py` 执行真实原生控件回归，覆盖两种语言的四种完成选项组合、详情反复切换、点击完成前不触发副作用、卸载隐藏目录及未知文件/用户数据保留。它拒绝已有安装或桌面/开始菜单快捷方式，使用 `.local/installer-flow/<随机ID>/` 中的独占安装与隔离数据；截图需要既有 Pillow，测试结果及截图保存在该目录。

应用包只包含 `dinuosi.exe`、主程序 MIT、`runtime/ffmpeg/` 中的桥接及配套 DLL、`licenses/` 与 `package-files.json`；音源、界面翻译和品牌图标已编译内嵌。FFmpeg/ffprobe CLI、导入库、头文件与构建工具留在开发缓存，不进入安装包或便携 ZIP。原生媒体源码及重建材料通过对应源码 ZIP 另行交付。

已有运行库但封存清单早于桥接库修改时，先按上文重编 `build-media-library.py` 并运行 `seal-runtime.ps1`，再重新收集许可和打包。封存不接受缺少桥接或必要 libav 的目录，打包不接受遗漏必要 DLL 的清单；Cargo 输入变化后不能复用旧许可审计。

安装和便携程序目录中的 `dinuosi.exe`、`runtime/ffmpeg/` 与 `licenses/` 应一同保留。当前生成的是未签名内部验收包；公开发行前的待确认事项保存在 [发行验收报告](DISTRIBUTION_VALIDATION.md#许可材料与公开发布阻断)。

`tools/verify-build-inputs.ps1` 检查缺失或变更输入时拒绝打包，`tools/verify-package.ps1` 执行实际安装、升级与卸载验收。后者会运行安装程序、创建临时当前用户安装登记及快捷方式，存在已有安装时拒绝执行；请先阅读脚本和验收范围。分别使用 `-Language English` 与 `-Language SimpChinese` 验证语言登记和本地化快捷方式；结果分别写入 `.local/distribution/qa/package-verification-English.json` 与 `package-verification-SimpChinese.json`，并检查安装/便携内容含完整 DLL 和许可且不含测试 CLI。

## 仓库文件边界

提交应用源码、`.cargo/config.toml`、构建和测试脚本、`distribution/` 中的输入锁与补丁，以及固定回归夹具。保留第三方资源原始许可。

下载输入、运行库、构建缓存、安装包、测试项目和日志保存在忽略的 `.local/` 或 `target/` 中；大型音源由准备脚本获取。不要将这些本机产物加入版本控制。

README 的界面图片来自 2026-10-10 当前源码构建后启动的真实软件，使用独立测试项目与合成素材；对应源图为 `.local/readme-current/editor.png`，构建与截图对应证据为 `.local/readme-current/evidence.json`，仓库保留副本 `assets/readme/editor.png`。更新界面后应使用新的真实截图替换，不以概念图代替已实现功能。
