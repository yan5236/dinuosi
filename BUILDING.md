# 在新电脑上从源码运行 Dinuosi

> **Debug 和 Release 都需要单独构建并部署 FFmpeg 动态库。`cargo build` / `cargo run` 只编译 Rust 程序，不会准备媒体运行库。缺库时，即使源文件存在，项目中的媒体也可能全部显示“媒体离线”。**

以下步骤从只有仓库源码的 Windows x64 电脑开始，不需要从作者电脑复制任何文件。普通使用者拿到完整安装包或便携包时不需要安装编译工具；单独一个 `dinuosi.exe` 不是完整的软件包。

## 1. 安装构建工具

- [PowerShell 7](https://learn.microsoft.com/zh-cn/powershell/scripting/install/install-powershell-on-windows)：以下步骤与项目 CI 使用 `pwsh` 执行 UTF-8 脚本。
- [Rust](https://rustup.rs/)：安装 stable 的 `x86_64-pc-windows-msvc` 工具链。
- [Visual Studio 2022 / Build Tools](https://learn.microsoft.com/en-us/cpp/build/vscpp-step-0-installation?view=msvc-170)：选择“使用 C++ 的桌面开发”，确认包含 MSVC x64、Windows SDK 和“适用于 Windows 的 C++ CMake 工具”。本项目使用其中的 CMake、Ninja 和 MSBuild；GPUI 还需要 Windows SDK 的 `fxc.exe`。
- [Python 3](https://www.python.org/downloads/windows/)：本项目 CI 使用 3.12，安装时启用 PATH；终端中的 `python` 命令必须能运行。
- [MSYS2](https://www.msys2.org/)：记下自己的安装目录，例如 `C:/msys64`。安装后打开 MSYS2 终端，按[官方说明](https://www.msys2.org/docs/package-management/)完成更新，再安装构建脚本需要的工具：

```sh
pacman -S --needed make patch gnupg binutils tar pkgconf diffutils
```

不要把作者机器上的盘符写进自己的命令。下文的 `$msysRoot` 是你实际安装的 MSYS2 目录；MSVC 路径从当前开发终端读取。

## 2. 获取源码并打开开发终端

从 [GitHub 仓库](https://github.com/yan5236/dinuosi)下载并解压源码，或在安装 Git 后执行：

```powershell
git clone https://github.com/yan5236/dinuosi.git
```

从开始菜单打开 **x64 Native Tools Command Prompt for VS 2022**，输入 `pwsh -NoExit`，再切换到自己解压或克隆的仓库根目录（该目录应有 `Cargo.toml`、`tools/` 和 `distribution/`）。这样 PowerShell 会继承 MSVC x64 与 Windows SDK 环境。

先确认工具可用：

```powershell
Get-Command cargo, python, cl.exe, fxc.exe
python --version
```

如果有命令找不到，先补装对应组件或重新打开开发终端，不要直接跳到 Cargo 构建。使用 pyenv 等版本管理器时，也要确认 `python --version` 真正输出版本，而非提示尚未选择 Python。

## 3. 在本机下载并构建媒体运行库

在上一步的 PowerShell 中执行；MSYS2 安装位置不同就修改第一行：

```powershell
$msysRoot = 'C:/msys64'
$vcVars = Join-Path $env:VSINSTALLDIR 'VC/Auxiliary/Build/vcvars64.bat'
./tools/prepare-media.ps1 -Build -MsysRoot $msysRoot -VcVars $vcVars
```

首次运行需要联网。脚本读取仓库中已提交的 `distribution/inputs.lock.json`，自行下载并校验固定源码与资源，提取音源，编译 FFmpeg/libav 和 `dinuosi_media.dll`，然后部署到 Cargo 输出目录。不需要自行寻找、复制或安装另一份 FFmpeg。

**新克隆中没有下列文件是正常的：它们被 Git 忽略，由上面的命令在这台电脑上生成。**

| 文件或目录 | 如何得到 |
| --- | --- |
| `.local/distribution/` | 准备脚本创建，存放下载缓存、源码和媒体构建产物 |
| `assets/audio/FluidR3_GM.sf2` | 准备脚本下载、校验并提取；Rust 编译时嵌入 |
| `target/debug/runtime/ffmpeg/` | 准备脚本部署 Debug 所需动态库 |
| `target/release/runtime/ffmpeg/` | 准备脚本部署 Release 所需动态库 |

准备命令报错时先解决该错误。缺工具按第 1 步补装；下载或哈希校验失败要检查网络及输出中的具体文件，不能跳过校验。详细构建与打包约定见 [开发指南](docs/DEVELOPMENT.md)。

## 4. 编译并运行 Debug

媒体准备成功后，在同一终端执行：

```powershell
cargo run -p dinuosi-desktop --locked
```

Debug 应用目录中的媒体运行库应是 DLL：

```text
target/debug/
  dinuosi.exe
  runtime/ffmpeg/
    dinuosi_media.dll
    avcodec-62.dll
    avformat-62.dll
    avfilter-11.dll
    avutil-60.dll
    swresample-6.dll
    swscale-9.dll
    （其他配套 DLL）
```

Windows 准备脚本只向 Debug/Release 应用目录部署 DLL，并清理旧的 `ffmpeg.exe`、`ffprobe.exe`。CLI 仅保留在构建缓存与 `target/debug/deps/runtime/ffmpeg/`、`target/release/deps/runtime/ffmpeg/` 中，供测试生成素材及独立对照；应用不从这些测试目录或 PATH 借用 CLI。

## 5. 验证动态库流程确实有效

完成准备后执行已有真实回归：

```powershell
cargo test -p dinuosi-core --locked --test runtime relocated_runtime_ignores_path_and_reports_missing_files -- --ignored --nocapture
```

测试把程序和运行库移到独立目录，移除那里的 FFmpeg/ffprobe CLI，验证探测、视频帧、音频播放数据及带音频导出；再分别移除桥接 DLL 和 libav，确认媒体操作失败。它同时改变工作目录并在 PATH 放入另一份工具，检查程序没有回退借用它们。仅编译成功不能代替此验证。

## 已在这台电脑构建过，清理 target 后如何恢复

只有本机仍保留第 3 步生成的 `.local/distribution/runtime/ffmpeg/bin/` 时，才可以用快捷重新部署：

```powershell
./tools/prepare-media.ps1 -DeployOnly
cargo run -p dinuosi-desktop --locked
```

这一步不联网、不编译，也不要求 MSYS2。**全新机器或已删除 `.local/` 的机器不能用它代替第 3 步**；没有完整运行库时会明确报错。

如果缺库时已经打开软件，补齐后需保存工作并完全退出、重新启动。进程会缓存首次动态库加载结果，仅重开项目不能清除失败缓存。源文件仍在原路径时会重新探测；跨电脑移动项目后，项目内的本机素材路径还需对应实际源文件位置。

Linux/macOS 从零准备的命令与依赖见 [跨平台支持](docs/CROSS_PLATFORM.md)。
