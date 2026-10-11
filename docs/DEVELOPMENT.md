# 开发指南

本文面向从源码运行、测试或打包 Dinuosi 的贡献者。软件当前的使用方式与功能范围见 [README](../README.md)。

## 环境

Windows x64 的构建与发行环境：

- Rust stable，MSVC 工具链；`rust-toolchain.toml` 声明 rustfmt 和 Clippy，Rust 依赖由 `Cargo.lock` 锁定。
- Visual Studio C++ Build Tools，包含 x64 编译工具和 Windows SDK。构建 GPUI 时需要能从 PATH 找到 `fxc.exe`。
- MSYS2，媒体构建需要 Bash、make、patch、GPG、ar 和 tar。
- Python 3，用于收集第三方许可与执行许可策略检查。

GPUI 固定为 0.2.2。Linux/macOS 原生构建、运行库准备和当前验证边界见 [跨平台支持](CROSS_PLATFORM.md)；三平台 CI 结果见 [导出平台验证](EXPORT_PLATFORM_VALIDATION.md)。上游系统依赖可参考 [GPUI 文档](https://docs.rs/gpui/0.2.2/gpui/)。

## 首次准备与运行

在已配置 MSVC x64 和 Windows SDK 的开发终端中执行。以下两个路径需替换为本机安装位置：

```powershell
$msysRoot = 'C:/msys64'
$vcVars = 'C:/Program Files (x86)/Microsoft Visual Studio/2022/BuildTools/VC/Auxiliary/Build/vcvars64.bat'
./tools/prepare-media.ps1 -Build -MsysRoot $msysRoot -VcVars $vcVars
cargo fetch --locked
cargo run -p dinuosi-desktop --locked
```

准备脚本按 `distribution/inputs.lock.json` 下载并校验固定输入，提取 FluidR3 GM 音源，从源码构建 FFmpeg，并准备锁定的 NASM/NSIS。首次准备需联网；下载缓存位于 `.local/distribution/`。

`assets/audio/FluidR3_GM.sf2` 约 141.5 MiB，编译时嵌入程序。它不进入 Git，首次编译前必须准备，否则 `include_bytes!` 会因文件缺失失败。音源说明和原始许可见 [assets/audio](../assets/audio/README.md)。

脚本将桥接动态库、配套 libav 和测试工具部署到 `target/debug`、`target/release` 及各自的 `deps` 测试程序目录。运行时已构建后，重新部署无需再次编译 FFmpeg：

```powershell
./tools/prepare-media.ps1 -MsysRoot $msysRoot
```

删除 `target/` 后需重新部署。当前脚本使用这些默认 Cargo 输出路径；自行更改 target 目录时，也需在相应程序旁部署 `runtime/ffmpeg/`。

## 媒体运行方式

Core 通过 `crates/media` 的安全接口直接调用 FFmpeg 8 的 libav 动态库，完成探测、BGRA/PCM 解码及时间线编码/封装。Desktop 负责界面绘制、播放调度与音频设备输出；应用正常媒体流程不创建 FFmpeg/ffprobe 子进程。Dinuosi 自身的用户 CLI 尚未实现。

桥接动态库及配套 libav 位于当前程序旁 `runtime/ffmpeg/`，不回退到 PATH。Windows 使用 `dinuosi_media.dll`，Linux/macOS 使用 `libdinuosi_media.so` / `libdinuosi_media.dylib`。动态库或 ABI 不匹配会返回具体错误。FFmpeg/ffprobe 可执行工具仅用于测试夹具、独立输出对照和构建许可检查；移除它们后应用仍可探测、播放及导出。

FFmpeg 固定为 8.0.3，保留禁止 GPL、nonfree、version3、自动探测和网络协议的约束，编码器与音源保持既有选择。MIT 原生桥接库动态链接可替换 libav，FFmpeg 算法代码不静态链接进 Rust 主程序；MIDI 继续由 RustySynth 与 FluidR3 合成。构建脚本同时编译桥接库，封存/打包记录桥接源码和构建脚本哈希，并随对应源码包交付。此链接边界按 [FFmpeg 官方分发要求](https://ffmpeg.org/legal.html)管理，现有公开发行审查项继续保留。

已有 FFmpeg 构建可单独重编桥接库：

```powershell
python tools/build-media-library.py --prefix .local/distribution/runtime/ffmpeg
./tools/seal-runtime.ps1 -MsysRoot $msysRoot -VcVars $vcVars
./tools/prepare-media.ps1 -MsysRoot $msysRoot
```

此步骤要求已保留锁定源码归档；源码包无法对应时封存或打包应失败。Unix 的准备脚本同时编译桥接库并记录全部原生输入；重新部署仍逐项校验哈希。导出行为与跨平台加载约定见 [EXPORT.md](EXPORT.md) 与 [跨平台支持](CROSS_PLATFORM.md)。

`.cargo/config.toml` 为 Windows MSVC 目标启用静态 C 运行库链接，使主程序无需用户另装 Visual C++ Redistributable。该文件是共享构建配置，不是本机 Cargo 缓存。

## 检查

```powershell
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
python ./tools/test-license-policy.py
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

许可收集使用本机已下载的 Cargo 依赖，并在需要时联网查询原始许可材料。打包会验证运行时、锁定源码、补丁及许可材料的一致性，输出到 `target/distribution/`：

- Windows x64 安装程序。
- 同内容便携 ZIP。
- FFmpeg、zlib、LAME 对应源码与重建脚本 ZIP。
- 第三方许可声明及产物校验清单。

安装和便携程序目录中的 `dinuosi.exe`、`runtime/ffmpeg/` 与 `licenses/` 应一同保留。当前生成的是未签名内部验收包；公开发行前的待确认事项保存在 [发行验收报告](DISTRIBUTION_VALIDATION.md#许可材料与公开发布阻断)。

`tools/verify-build-inputs.ps1` 检查缺失或变更输入时拒绝打包，`tools/verify-package.ps1` 执行实际安装、升级与卸载验收。后者会运行安装程序、创建临时当前用户安装登记及快捷方式，存在已有安装时拒绝执行；请先阅读脚本和验收范围。

## 仓库文件边界

提交应用源码、`.cargo/config.toml`、构建和测试脚本、`distribution/` 中的输入锁与补丁，以及固定回归夹具。保留第三方资源原始许可。

下载输入、运行库、构建缓存、安装包、测试项目和日志保存在忽略的 `.local/` 或 `target/` 中；大型音源由准备脚本获取。不要将这些本机产物加入版本控制。

README 的界面图片来自 2026-10-10 当前源码构建后启动的真实软件，使用独立测试项目与合成素材；对应源图为 `.local/readme-current/editor.png`，构建与截图对应证据为 `.local/readme-current/evidence.json`，仓库保留副本 `assets/readme/editor.png`。更新界面后应使用新的真实截图替换，不以概念图代替已实现功能。
