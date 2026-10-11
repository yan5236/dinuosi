# Windows 内置媒体运行时验收报告

验收日期：2026-10-10。当前 Windows 10 IoT 企业版 LTSC 19044 x64 上的内部安装包和便携版通过本轮自动检查及实际媒体操作。未签名、未上传、未公开发布。公开发布仍受下述授权材料及系统验证缺口约束，不能将本报告理解为所有电脑兼容或法律风险为零。

2026-10-11 后续新增时间线导出及 Media Foundation 运行库构建，行为与验证见 [EXPORT.md](EXPORT.md)。下列安装包哈希和安装生命周期证据属于此前版本，未覆盖本次新增编码路径；新版本仍需重新打包与干净系统验证。

## 交付产物

文件位于 `target/distribution/`。安装包与 ZIP 使用同一应用目录内容；安装器另生成自己的卸载程序。集中许可文本为 `THIRD_PARTY_NOTICES.txt`，逐文件清单与校验值在包内 `package-files.json`。

| 文件 | 大小 | SHA-256 |
| --- | ---: | --- |
| Dinuosi-0.1.0-ffmpeg-8.0.3-sources.zip | 13.87 MiB | `ae9fa18c19c8773bcef77c8e26f4676a176aadb83e5e743397f6abed55d3e004` |
| Dinuosi-0.1.0-windows-x64-setup.exe | 141.94 MiB | `f5a7886c43a491581885fba341a3a35f8f588d32bb020a2958f2f237ac9244af` |
| Dinuosi-0.1.0-windows-x64.zip | 144.68 MiB | `476f29d491601a2ef26b45756d8c9b5ed188ae58a412bea8cdc7d2ac690fa533` |

## 实际变化

- 所有原有 FFmpeg/ffprobe 调用由 `crates/core/src/media.rs` 统一指向主程序旁 `runtime/ffmpeg/`。工具进程工作目录固定到运行时目录，PATH 仅保留 Windows 系统目录，防止缺少 DLL 时借用用户另一版运行库。项目格式和媒体绝对路径引用保持原状。
- 从官方 FFmpeg 8.0.3 源码构建共享 libav DLL、ffmpeg、ffprobe；禁用 GPL/nonfree/version3、自动探测和网络协议。仅加入 zlib 1.3.1 与 LGPL libmp3lame 3.100。未使用本机 GPL full_build；未加入 libx264、ModPlug 或视频导出。
- 保留 RustySynth 合成流程；编译嵌入未经修改的 FluidR3_GM.sf2，来自 Debian fluid-soundfont 3.1-5.3，沿用其 MIT 版权与许可。旧 TimGM6mb 和专属 GPL 文本从当前源码树移除，Git 历史保留。Debian GPL 打包脚本不随应用分发。
- Windows 主程序、zlib/LAME 及媒体工具静态使用 Microsoft C 运行库；依赖检查没有 VCRUNTIME140/MSVCP 外部依赖。应用使用操作系统自身的 Windows DLL，未把系统 DLL 复制进包。
- 关于窗口仅新增必要的第三方许可链接。许可文件缺失显示现有风格的实际错误；鼠标及 Enter 使用同一入口。
- NSIS 3.13 Unicode、zlib 压缩，当前用户安装，默认目录为 `%LOCALAPPDATA%/Programs/Dinuosi`；开始菜单与标准卸载登记均在当前用户范围。无开机启动、文件关联、在线组件下载。运行中升级拒绝并提示正常关闭，不结束主程序。卸载只删除包内已知文件，保留未知文件、项目和用户数据。

## 自动检查

| 检查 | 结果与具体范围 |
| --- | --- |
| 格式、严格 Clippy | `cargo fmt --all -- --check`；`cargo clippy --workspace --all-targets -- -D warnings` 通过 |
| 工作区常规测试 | 81 项通过；不会把默认忽略的媒体检查算作通过 |
| 显式 Core 媒体回归 | 11 项通过：合成缓存、音量关键帧、4 项媒体/H.264/尾帧/采样检查、MIDI、运行时路径、时间线音频及2项波形 |
| 桌面波形回归 | 1 项单独通过，覆盖真实音频导入、GPUI 点击/缩放/裁边、缓存、源入点、离线恢复和重开 |
| MIDI | 128 种 GM 乐器逐项产生有限且可听 PCM，第10 MIDI 声道打击乐通过；48kHz立体声样本数、采样区间精确相等、音色切换、波形与坏文件拒绝通过 |
| 运行时路径 | 真实复制程序到中文/空格目录，使用无关启动目录并在 PATH 放可启动的另一份工具；实际探测与解码通过；自带 ffprobe 或 avcodec DLL 缺失则失败，未回退 |
| 许可策略 | 非 GPL 双重许可分支选择、AND 组合与未知许可/例外拒绝边界通过；Windows 普通依赖闭包466个包 |
| 打包拒绝边界 | 缺少 FFmpeg 源码、缺少许可材料、运行时SHA不符、Cargo审计过期均明确失败且未生成安装程序；内嵌音源校验不符也单独验证拒绝 |
| 安装生命周期 | 实际静默安装、运行中升级拒绝且进程存活、正常关闭后升级、便携版文件一致性与启动、卸载保留未知文件/独立项目/用户数据；7项检查通过 |
| 调用环境恢复 | 验收隔离的 PATH/数据目录/代理在成功或异常后恢复；最终验证调用者 PATH 未改变 |

原有上游 `proc-macro-error2 2.0.1` 未来 Rust 兼容性警告仍存在；当前严格 Clippy、构建与测试通过。未为此扩大本轮修改范围。

## 源码对应证据

官方源码归档、官方签名/公钥、zlib/LAME原始源码、MSVC中文编译器识别与资源UTF-8补丁、zlib头文件兼容补丁、构建/重建脚本及全部输入校验值一并交付。GPG验证官方FFmpeg源码签名成功；没有把本地公钥导入的未知信任等级描述为身份认证。

实际在 `.local/distribution/reconstruction/` 的新目录提取原始归档、应用补丁，从零重建三套库及FFmpeg。最终交付运行时采用该次独立重建的exe/DLL，而不是仅比较版本文字。源码包的 `BUILDING.txt` 与 `runtime-manifest.json` 记录参数和工具链；`rebuild-runtime.ps1`拒绝非空工作目录，避免混入旧对象文件。重建成果通过实际解码、MP3、音频、寻址及波形检查。未承诺在不同机器/工具链上得到逐字节相同的二进制。

工具链：MSVC 19.44.35208、Windows SDK 10.0.22621.0、MSYS Bash 5.3.9、NASM 2.16.03；Rust 1.98.0。运行时版本及每个文件SHA见包内 `licenses/runtime-manifest.json`。

## 实际界面与音频证据

在独立 `.local/distribution/qa/` 内建立“发行验收”项目，导入12秒H.264视频、12秒660Hz WAV、图片及2秒标准钢琴MIDI。真实鼠标/键盘完成视频预览播放、音频与MIDI播放、波形显示、图片加入时间线、项目保存、正常关闭和便携版重新打开；4个素材引用及图片时间线保持。核对源文件SHA未改变，没有复制或改动正式项目。

验收程序从不同工作目录启动，进程PATH排除本机FFmpeg，设置失效HTTP/HTTPS代理；FFmpeg本身禁用网络协议，应用没有运行时下载流程。没有修改全局PATH、系统权限、防火墙或虚拟机；这不等于物理断网或干净系统验证。

播放时通过Windows实际进程信息观察到两个ffmpeg子进程，父进程为验收便携版，路径均来自其 `runtime/ffmpeg/ffmpeg.exe`。许可文件缺失实测显示中文错误，恢复后一次Enter只产生一个Notepad阅读窗口，打开的是同目录的真实许可文件。该操作验收与最终包主程序/媒体文件哈希一致。

实际WASAPI回环仅保存测量数值，不保存桌面声音录音：WAV测得660Hz、振幅0.08837；MIDI播放前RMS为0，播放时RMS为0.01917。证明音频确实进入输出设备，不把图标切换当成声音验证。

首选Computer Use运行时多次初始化失败；实际操作改用项目已有Win32 SendInput和Pillow桌面像素捕获流程。没有用程序内直接改状态代替鼠标/键盘，也没有把GPUI模拟测试当成实机操作。

## 体积与资源代价

FluidR3_GM.sf2为148,398,306字节，旧音源5,969,788字节，原始音源增大135.83MiB；完整GM覆盖按计划保留。最终主程序163,577,344字节，压缩包体积见上表。

独立MIDI回归进程在首个解码器初始化时耗时66.91ms，渲染2秒样本总耗时76.51ms；同进程寻址1秒位置耗时5.58ms，总耗时10.14ms。另一轮与打包并发的测试首次初始化为1.50秒，显示大音源载入受磁盘/CPU竞争影响；这些数值不是界面启动延迟的保证，也不是与旧音源的同条件性能对比。

实机验收进程工作集：空项目管理窗口57,020,416字节；导入4个素材并播放MIDI后403,070,976字节；观察到的峰值415,019,008字节。这包含GPUI、纹理、波形与合成数据，不能将整个差值都归因于音源。未开展长期内存泄漏或多小时压力验证。

## 许可材料与公开发布阻断

FFmpeg/zlib/LAME、FluidR3、Rust运行依赖和NSIS的相应声明随包保存。运行时文件可由用户替换兼容版本；运行时不强制校验其SHA，也没有EULA覆盖LGPL修改与替换权利。原始完整FFmpeg/LAME源码归档含各自可选模块的许可文本；本轮编译的二进制没有启用GPL-only模块。

Dinuosi 主程序采用 [MIT 许可证](../LICENSE)。下列事项仍必须在公开发布前解决：

1. 现有图标、Logo授权确认。未替资源所有者重新授权。
2. 五个旧crate发布包及已检查的对应源码提交没有独立完整许可文件：convert_case 0.4.0、hexf-parse 0.2.1、mac 0.1.1、seahash 4.1.0、simd_helpers 0.1.0。保留原始Cargo作者/许可声明、完整发布源码及未改写的SPDX标准文本；没有虚构版权年份或持有人。这是内部验收材料缺口，不是“全部许可已经法律审清”。其他未知缺口会拒绝收集/打包。
3. Microsoft静态C运行库的适用再分发条款确认；它属于Visual Studio/Build Tools授权，不能归入FFmpeg LGPL。没有捆绑Windows系统DLL。
4. 干净Windows和Windows11实际安装/运行验证。没有授权或确认可用Hyper-V测试环境，未自行变更系统权限或虚拟机。
5. 正式代码签名与正式发行审核；编解码格式涉及的专利等商业发行事项不能仅凭开源许可筛选排除。

本轮没有实现视频导出，也没有验收macOS、Linux、ARM64或承诺任意电脑兼容。

参考原始规则：[FFmpeg合法分发要求](https://www.ffmpeg.org/legal.html)、[Fluid音源版权与许可](https://metadata.ftp-master.debian.org/changelogs/main/f/fluid-soundfont/fluid-soundfont_3.1-5.3_copyright)、[NSIS许可](https://nsis.sourceforge.io/License)、[Microsoft C++运行库再分发](https://learn.microsoft.com/en-us/cpp/windows/redistributing-visual-cpp-files?view=msvc-170)。

## 本机证据位置

- 自动检查、独立源码重建、打包和安装日志：`.local/distribution/build/`。
- 安装/升级/卸载JSON：`.local/distribution/qa/package-verification.json`。
- 真正工具路径、许可键盘窗口、WASAPI输出数据：同目录的 `media-processes.json`、`license-ui.json`、`audio-loopback.json`、`midi-loopback.json`。
- 源文件SHA与验收项目：同目录 `source-hashes.json`、`projects/发行验收/`、`user-data/`。
- 桌面原始截图：同目录 `screenshots/`，包含 `final-project-reopen.png`、`final-video-playing.png`、`final-about.png`、`final-license-missing.png`、`audio-loopback.png`、`midi-playing.png`。

这些证据仅描述当前主机上实际执行的结果；不包含干净系统或Windows11证据。源码、运行时和工具大文件保留于忽略目录，未加入Git；旧历史未改写。实施前基线提交为 `001aac6`；源码实现、构建脚本和本报告一并纳入版本控制。
