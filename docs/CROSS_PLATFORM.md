# 桌面跨平台支持

当前适配目标是 Windows x64、Linux x86_64 和 macOS（Apple Silicon / Intel）。平台支持通过本机 Windows 检查及 GitHub Actions 原生 runner 验证；编译、GPUI 事件测试、真实媒体回归和窗口启动各自保留证据。不能将 CI 通过等同于所有设备、发行版或 GPU 驱动均已认证。

## 运行边界

主程序和媒体工具均使用本平台原生程序。Core 从当前程序旁 `runtime/ffmpeg/` 定位 `ffmpeg` / `ffprobe`，Windows 加 `.exe`；不回退到 PATH，不在运行时下载组件。Unix 共享库以相对 rpath 加载，媒体子进程移除外部动态库搜索/注入环境变量。运行库迁移到中文或空格目录、缺失工具与缺失共享库均纳入真实测试。

H.264：Windows 使用 Media Foundation 硬件/软件；macOS 使用 VideoToolbox 硬件/软件，并保留 OpenH264 软件路径；Linux 使用 OpenH264 软件。AV1、VP9 和 MPEG-4 共用既有实现。系统后端只在本平台探测，每种实现实际编码三帧后才进入界面；硬件失败不阻止软件能力探测，不能把未探测成功的硬件计为通过。

FFmpeg 固定 8.0.3，继续禁止 GPL、nonfree、version3、网络协议及任意系统 FFmpeg。OpenH264 2.6.0 来源和 SHA-256 记录在输入锁的 `unixInputs` 中，仅用于 Unix 构建；许可与专利原文必须随对应源码和运行库保留。自行编译 OpenH264 不等于获得 Cisco 预编译二进制的专利许可承诺，公开发行仍需按现有发行约束审核。

当前阶段继续使用后台 FFmpeg 进程及 BGRA/PCM 管道。直接调用 libavformat/libavcodec 的迁移在三平台媒体基线通过后进行；该阶段需独立验证 ABI、资源所有权、线程和取消，不以仅替换链接方式判定完成。

Windows 用户数据位置保持 `%LOCALAPPDATA%/Dinuosi`；macOS 新默认是 `~/Library/Application Support/Dinuosi`，已有旧目录且新目录不存在时保留旧列表；Linux 使用绝对 `XDG_DATA_HOME` 或 `~/.local/share/Dinuosi`。`DINUOSI_DATA_DIR` 保留开发隔离用途。项目中的素材路径仍是本机引用，不自动转换为另一台电脑的路径。

字体保持 Windows 原样，macOS 使用系统字体与 Menlo，Linux 使用 Noto Sans 和 DejaVu Sans Mono；中文依赖系统 CJK 字体。macOS 默认常用快捷键使用 Command，保留旧 Ctrl 兼容和已有用户覆盖；Ctrl+H 保持项目管理用途，系统 Command+H/Q/W 保留。

## 从源码运行

Windows 沿用 [开发指南](DEVELOPMENT.md) 的 MSVC 准备和打包链路。

Linux 需 Rust stable、C/C++ 编译工具、Python 3.12+、CMake、NASM、pkg-config、GPG、patchelf，以及 GPUI/Rodio 的系统开发依赖。Ubuntu 24.04：

```sh
sudo apt-get install build-essential cmake nasm pkg-config gnupg patchelf libasound2-dev libudev-dev libssl-dev libfontconfig1-dev libwayland-dev libxkbcommon-dev libxkbcommon-x11-dev libxcb1-dev libxcb-shape0-dev libxcb-xfixes0-dev libx11-dev libxi-dev libxrandr-dev libxcursor-dev libxinerama-dev libxrender-dev libvulkan-dev mesa-vulkan-drivers fonts-noto-cjk fonts-noto-core
```

桌面需要可用的 X11/Wayland 会话和 Vulkan 驱动；文件选择器使用 GPUI 的桌面 portal，需要已安装并运行匹配桌面的 `xdg-desktop-portal` 后端；声音输出需要正常音频设备/系统服务。

macOS 需要完整 Xcode（包含 Metal 编译工具）、Command Line Tools、Rust stable、Python 3.12+，以及 `brew install cmake nasm pkg-config gnupg`。

两种 Unix 平台均执行：

```sh
python3 tools/prepare-media-unix.py
cargo run -p dinuosi-desktop --locked
```

脚本验证锁定源码 SHA-256 与 FFmpeg 官方签名，在 `.local/distribution/unix/` 原生构建，生成运行库哈希清单，部署到 Cargo debug/release 及 deps。x86_64 的汇编工具也从锁定 NASM 2.16.03 源码构建，避免 Homebrew 新版 NASM 与当前 libaom 的检测不兼容。仅重新部署可运行 `python3 tools/prepare-media-unix.py --deploy`，脚本拒绝构建输入、运行库或内嵌音源发生漂移。支持 `CARGO_TARGET_DIR`，自定义 `--target` 子目录需将运行库部署到相应程序旁。

## 验证

工作流覆盖三平台共享模块测试、全部自包含媒体/导出回归、桌面构建及 GPUI 事件/波形测试、严格 Clippy、格式、真实窗口启动。macOS 分别使用 ARM64 和 Intel runner（标签依据 [GitHub 官方 runner 列表](https://docs.github.com/en/actions/reference/runners/github-hosted-runners)）。Linux 窗口启动在 Xvfb/Openbox/Vulkan 软件驱动中进行；该结果不证明 Wayland、实机 GPU、音频设备或文件选择器已经验收。截图权限受限时保留明确状态，窗口未创建仍失败。

需要真实外部 MIDI 文件的试听检查不计入无输入 CI。源码中的所有 `#[ignore]` 只在显式执行后计为通过。签名、公证、新平台安装包、干净系统安装、设备播放、文件选择器与完整鼠标验收仍属于独立发行/实机检查。
