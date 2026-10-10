# 开发指南

本文面向从源码运行、测试或打包 Dinuosi 的贡献者。软件当前的使用方式与功能范围见 [README](../README.md)。

## 环境

当前开发与发行目标为 Windows x64：

- Rust stable，MSVC 工具链；`rust-toolchain.toml` 声明 rustfmt 和 Clippy，Rust 依赖由 `Cargo.lock` 锁定。
- Visual Studio C++ Build Tools，包含 x64 编译工具和 Windows SDK。构建 GPUI 时需要能从 PATH 找到 `fxc.exe`。
- MSYS2，媒体构建需要 Bash、make、patch、GPG、ar 和 tar。
- Python 3，用于收集第三方许可与执行许可策略检查。

GPUI 固定为 0.2.2。其他平台尚未经过本项目验证；上游系统依赖可参考 [GPUI 文档](https://docs.rs/gpui/0.2.2/gpui/)。

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

脚本将媒体工具部署到 `target/debug`、`target/release` 及各自的 `deps` 测试程序目录。运行时已构建后，重新部署无需再次编译 FFmpeg：

```powershell
./tools/prepare-media.ps1 -MsysRoot $msysRoot
```

删除 `target/` 后需重新部署。当前脚本使用这些默认 Cargo 输出路径；自行更改 target 目录时，也需在相应程序旁部署 `runtime/ffmpeg/`。

## 媒体运行方式

Core 通过后台子进程调用 `ffprobe.exe` 探测媒体，调用 `ffmpeg.exe` 解码画面和声音，通过管道读取 BGRA 帧和 PCM 样本。Desktop 负责界面绘制、播放调度与音频设备输出。Windows 上的媒体子进程不显示控制台；用户只需操作图形界面。

工具固定定位到当前程序旁的 `runtime/ffmpeg/`，并使用配套 libav DLL，不回退到 PATH。媒体工具或 DLL 缺失会报告错误。这里使用的是 FFmpeg 的命令行接口，Dinuosi 自身的用户 CLI 尚未实现。

FFmpeg 当前固定为 8.0.3，构建禁用 GPL、nonfree、version3、外部库自动探测和网络协议，仅加入 zlib 与 LGPL LAME。它作为独立工具随包部署，FFmpeg 代码不链接进 Dinuosi 主程序。MIDI 使用 RustySynth 与 FluidR3 音源合成，不经过 FFmpeg。

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
cargo test -p dinuosi-core --locked -- --ignored --nocapture
```

仅验证非 GUI 模块可运行：

```powershell
cargo test -p dinuosi-core -p dinuosi-command -p dinuosi-icons --locked
```

默认忽略的检查不能计作已通过。历史验证范围见 [历史验证记录](VALIDATION_HISTORY.md)，当前 Windows 内置运行时与安装生命周期的验收证据见 [发行验收报告](DISTRIBUTION_VALIDATION.md)。

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
