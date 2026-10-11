# 时间线视频导出

标题栏“导出”打开模态弹窗，左侧是当前时间线预览，右侧选择分辨率、整数帧率、目标视频码率、MP4/MKV、已通过试编码的编码器，并填写文件名和选择输出文件夹。右下角开始后台导出，显示进度、完成或实际失败原因；取消会等待后台清理。参数只用于本次任务，不改变项目配置。

导出包含全部时间线时长，空隙为黑色画面和静音，按现有轨道层级、可见性、源裁剪、画布坐标、缩放、旋转、透明度及关键帧合成；音频复用既有混音器，包含源音轨、独立音频、MIDI、静音、片段增益与音量关键帧。解码保持素材显示尺寸（像素比例及自动旋转），输出按所选分辨率合成，预览缓存不作为导出源。

输出帧率与项目不同时，在输出时间 `t` 使用 `floor(t * project_fps)` 取当前片段的项目源帧，画面关键帧按 `t` 插值。所选码率是编码目标，并不承诺最终文件恰好达到该平均码率。AAC 声音为 48kHz 立体声 192kbps。

导出参数支持 16–8192 的偶数宽高、1–240fps 和 100–1000000kbps；弹窗提供项目值及常用预设。实际系统编码器可能只支持其中一部分，初始化失败显示真实诊断；不会自动更换格式或编码器。参与输出的源缺失、离线或无可解码帧会报错。隐藏且静音的片段不读取其源文件。

每个任务创建唯一临时目录，先生成 PCM 再编码视频；成功且媒体处理全部通过后，以硬链接发布（不支持时用 `create_new` 安全复制）。已有同名文件一律拒绝，不覆盖源素材或此前输出。失败/取消清理本次临时数据；复制式发布期间最终路径可能短暂出现未完成文件，失败会删除本次创建的文件。取消在成功发布后到达时任务保持成功。

## 编码与许可边界

主程序仍为 MIT。H.264 是编码格式，libx264 是其具体实现；本项目不捆绑 libx264，也不启用 FFmpeg GPL/nonfree/version3。受控构建通过 `--enable-mediafoundation` 启用 FFmpeg 的 LGPL 模块，调用 Windows 已安装的 H.264 Media Foundation 编码器。主程序仅以受控子进程交互，不回退到系统 PATH。能力检测分别实际试编码 Windows 硬件 H.264、Windows 软件 H.264 与 FFmpeg 原生 MPEG-4，只有成功的实现进入选项；有可用硬件时默认选择它。未加入独立厂商编码库或复制驱动 DLL。

Microsoft 文档说明系统 H.264 MFT 的输入、参数与平台支持；FFmpeg 官方明确 LGPL/GPL 构建边界及对应源码义务。MIT 不改变第三方义务，也不自动授予编码专利的商业授权。当前公开发行门槛仍见 [发行验收报告](DISTRIBUTION_VALIDATION.md)，历史安装包验收不覆盖本次新增编码运行库。

官方资料（2026-10-11 核实）：

- [FFmpeg 许可与分发规则](https://ffmpeg.org/legal.html)
- [FFmpeg 8 Media Foundation 源码与模块许可](https://ffmpeg.org/doxygen/8.0/mfenc_8c_source.html)
- [Microsoft H.264 编码器](https://learn.microsoft.com/en-us/windows/win32/medfound/h-264-video-encoder)
- [Windows N Media Feature Pack](https://support.microsoft.com/en-us/windows/experience/platform-variants/media-feature-pack-for-windows-n)
- [Microsoft SDK 许可原文](https://github.com/microsoft/win32metadata/blob/main/licenses/sdk_license.txt)

## 验证

真实媒体回归由 `cargo test -p dinuosi-core --locked export::tests -- --ignored --nocapture` 执行，需在测试程序旁准备当前受控运行库。普通边界与 GPUI 事件回归随工作区测试执行。当前任务的具体运行结果记录于 [验证历史](VALIDATION_HISTORY.md)。
