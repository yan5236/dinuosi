# 桌面跨平台支持

当前适配目标是 Windows x64、Linux x86_64 和 macOS（Apple Silicon / Intel）。平台支持通过本机 Windows 检查及 GitHub Actions 原生 runner 验证；编译、GPUI 事件测试、真实媒体回归和窗口启动各自保留证据。不能将 CI 通过等同于所有设备、发行版或 GPU 驱动均已认证。

## 运行边界

Core 通过 `dinuosi-media` 从当前程序旁 `runtime/ffmpeg/` 加载原生桥接动态库（Windows DLL、Linux SO、macOS dylib），直接调用 libavformat/libavcodec/libavfilter/libswscale 完成探测、解码、编码与封装。不依赖 FFmpeg/ffprobe CLI，不回退到 PATH，不在运行时下载组件。桥接 ABI 固定为 1 并校验 FFmpeg 8 的 libav 主版本；Unix 依赖按绝对路径预加载且以相对 rpath 解析，缺少相邻库时拒绝系统替代品。Windows DLL 搜索限制为库所在目录和系统目录。

用户可在退出程序后替换该目录中的接口兼容 libav 及配套库，保留对应平台的库名、主版本和应用所需编解码/滤镜能力。启动时不校验库文件 SHA，也不要求用户库与发行库逐字节一致；构建、缓存和打包的哈希检查仅保证发行材料与对应源码一致。动态链接与可替换性不免除 LGPL：按 [FFmpeg 官方合规要求](https://ffmpeg.org/legal.html)，发行时仍需保留声明、许可及与二进制准确对应的源码、补丁和构建说明，并尊重修改与替换权利；主程序 MIT 不改变这些义务。

FFmpeg/ffprobe 仍由源码构建，供测试生成夹具、独立探测与输出对照。真实迁移测试在中文空格路径移除两者后，要求探测、寻址、视频播放和带 AAC 的时间线导出仍成功；移除桥接库或 libav 后则必须失败。

H.264：Windows 使用 Media Foundation 硬件/软件；macOS 使用 VideoToolbox 硬件/软件，并保留 OpenH264 软件路径；Linux 使用 OpenH264 软件。AV1、VP9 和 MPEG-4 共用既有实现。系统后端只在本平台探测，每种实现实际编码三帧后才进入界面；硬件失败不阻止软件能力探测，不能把未探测成功的硬件计为通过。

FFmpeg 固定 8.0.3，继续禁止 GPL、nonfree、version3、网络协议及任意系统 FFmpeg。OpenH264 2.6.0 来源和 SHA-256 记录在输入锁的 `unixInputs` 中，仅用于 Unix 构建；许可与专利原文必须随对应源码和运行库保留。自行编译 OpenH264 不等于获得 Cisco 预编译二进制的专利许可承诺，公开发行仍需按现有发行约束审核。

Rust FFI 分别限定在 `crates/media` 的媒体边界与 `crates/cursor` 的 Windows 原生光标边界，Core/Desktop 继续禁止 unsafe。光标只在所属窗口线程创建和更新，窗口销毁或会话释放后清理回调及资源；其他平台使用 GPUI 系统十字光标。原生解码与编码上下文独占，Drop 在库与取消句柄存活期间释放资源；BGRA 和 PCM 使用有界缓冲。视频按播放时钟输出，导出复用原生顺序解码。取消监视器只设置原子标志，原生 IO 和处理边界检查取消；不并发释放上下文。系统编码器卡在驱动内部时，进程内取消不能保证强制抢占，十秒能力探测期限属于协作取消。能力探测在独立线程执行，避免 Windows GUI STA COM 环境影响 Media Foundation。

Windows 用户数据位置保持 `%LOCALAPPDATA%/Dinuosi`；macOS 新默认是 `~/Library/Application Support/Dinuosi`，已有旧目录且新目录不存在时保留旧列表；Linux 使用绝对 `XDG_DATA_HOME` 或 `~/.local/share/Dinuosi`。`DINUOSI_DATA_DIR` 保留开发隔离用途。项目中的素材路径仍是本机引用，不自动转换为另一台电脑的路径。

字体保持 Windows 原样，macOS 使用系统字体与 Menlo，Linux 使用 Noto Sans 和 DejaVu Sans Mono；中文依赖系统 CJK 字体。macOS 默认常用快捷键使用 Command，保留旧 Ctrl 兼容和已有用户覆盖；Ctrl+H 保持项目管理用途，系统 Command+H/Q/W 保留。

各窗口保留自绘灰阶控制。Linux 请求客户端装饰并调用窗口管理器拖动；GPUI 在无合成器的 X11 环境可能回退系统装饰。macOS 的 GPUI 0.2.2 没有独立隐藏系统彩色按钮的接口，暂用其公开位置选项移出窗口，保留调整窗口大小的原生样式。此限制在代码中标注，升级到提供隐藏接口的 GPUI 后替换。

素材预览与时间线播放都只把 Core 解码/混音准备放到后台。Rodio/CPAL 音频设备在拥有它的前台线程创建、持有和释放；macOS 设备句柄不可跨线程传递。迟到或取消的后台解码结果在打开音频设备之前丢弃，失败继续释放本次原生解码会话。

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
