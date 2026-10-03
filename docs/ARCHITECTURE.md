# 初始化架构

GUI 是人类接口，CLI 是脚本接口，MCP 是 AI 接口，Action 是自动化接口，Plugin 是扩展接口；它们下面共享的始终只有一个 Dinuosi Runtime。

完整目标以本地 `.ref/设计文档.md` 为准，该参考目录不进入版本控制。当前是初始化基线，尚未实现完整 Runtime。

```text
apps/desktop (GPUI + 窗口会话)
    ├── dinuosi-command → dinuosi-core
    ├── dinuosi-core (媒体模型、探测、帧提取、流式解码、离线状态)
    └── dinuosi-icons
assets/brand → 编译嵌入 Desktop
```

初始化仅建立有实际代码职责的四个 package。设计文档建议的 9–12 个主要 crate 将随媒体、渲染、任务等模块实际落地逐步建立，不提前添加空壳。

## 当前边界

- Core 定义项目模型和稳定 UUID，并通过版本化存储保存元数据及源素材路径引用。`media.rs` 提供媒体模型、类型/时长探测、BGRA 帧提取和离线恢复检查；`media_stream.rs` 提供固定内存的视频帧与 PCM 流式解码及进程清理。不依赖 GPUI、Rodio 或音频设备，CLI 可直接调用这些公开接口；不直接序列化内部结构。
- Core 的 `ProjectStore::import_media` 统一批量探测、去重、逐项失败反馈和原子保存引用；`load_media` 统一重开探测、别名与离线保留。`rename_media` 保存显示别名，`remove_media` 仅移除路径引用；均不改写源素材并在索引失败时恢复项目。Desktop 文件选择器和拖放均调用同一导入接口。
- Command 将项目生命周期操作委派给 Core；本层不实现磁盘读写，也不依赖 GUI。
- Desktop 保存当前窗口的导航、素材解码结果、预览选区、音量、标尺缩放、框选和指针状态；项目及源路径引用由 Core 原子保存。导入不复制源文件，重开项目重新探测路径，失联引用保留为离线素材。
- Desktop 的 `view.rs` 负责会话与项目管理，`editor.rs` 负责剪辑区域和比例布局，`ui.rs` 提供灰阶与窗口操作。`library.rs` 管理引用式素材库，`media.rs` 只将 Core 元数据和 BGRA 像素适配为 GPUI 图像，`playback.rs` 消费 Core 解码器并通过 Rodio 输出声音、维护设备播放时钟，预览会话/绘制与指针交互各自独立；不引入 WebView，也不修改 GPUI。
- Icons 不依赖 GPUI；以素材 `spec.json` 为来源，在 `OUT_DIR` 生成类型化枚举、中文标签、逻辑资源路径及原始 SVG 内容。
- GPUI 的 AssetSource 提供内嵌 SVG 与 PNG；SVG 使用 GPUI 单色渲染并由界面文字色着色，双色 Logo 使用原始 PNG 保留品牌色。

## 下一阶段

当前已先按界面任务接入素材导入与预览，时间线已由 Core 的 `timeline.rs` 定义整数帧/帧率、稳定 UUID、轨道、片段、变换和历史；`timeline_file.rs` 将其显式映射为可选 V1 字段，旧项目缺失时创建空轨道。插入、移动、裁边、删除和参数修改都通过核心接口，桌面在一次手势释放时保存并记录一条历史。后续再实现刀片分割和导出。

项目保存需独立定义 `ProjectFormatV1`、版本标识和迁移入口，不能把当前内部模型直接当作磁盘格式。EditorSession 和 Preferences 必须与 ProjectState 分离。

Desktop 后台任务调度 Core 的媒体探测、寻址与源路径检查。Core 解码器只保留最新 BGRA 帧，通过 PCM 迭代器输出立体声 48kHz 样本，销毁时终止解码子进程；Desktop 回收 GPU 纹理并输出声音。当前解码画布固定 640×360、24fps；Core 提供计入像素比例和旋转的有效画面尺寸，Desktop 移除透明补边后再按面板比例适配，不代表最终导出质量。`midi.rs` 使用 RustySynth 与编译嵌入的 TimGM6mb 音源解析和采样合成标准 MIDI，直接接入共享 PCM 流，不经过 FFmpeg/ModPlug；256 复音、固定块缓冲，寻址在后台预渲染到目标点，成本随起点增长。共享媒体接口已可无头调用，CLI、Action、插件及 MCP 应用入口尚未实现。

## 素材接入记录

轻线来源：`E:\轻线图标-64枚完整套装\qingxian-icons-88`。虽然目录名保留 88，当前清单为 v1.5，共 89 枚，包含 `replace-media`。原始 SVG 保留 24×24 viewBox、1.8 描边、currentColor 和圆角端点/连接。

本轮从 `.ref/qingxian-library-filters-5/svg` 接入全部、音频、动效、特效、`playhead-head` 及用户提供的 `warning` 三角感叹号，视频 SVG 与原文件相同。项目清单升级到 v1.6，当时共 95 枚。播放头采用原始实心填充，其余新增图标保持 1.8 描边。

Logo 来源：`E:\轻线图标-64枚完整套装\Dinuosi-Logo-Kit`。项目在 `assets/brand` 保留 PNG 导出文件与可编辑 SVG 原稿，界面采用主标志 PNG；本地参考素材放在不进入版本控制的 `.ref` 目录。

## 时间线与检查器边界

- `timeline.rs` 的编辑、约束、矩形选择、参数限值、刻度生成和撤销/重做不依赖 GPUI。检查器坐标是 1920×1080 默认项目画布中心的像素偏移，X 向右、Y 向下，旋转为角度。
- `composite.rs` 在后台解码各可见片段，使用源入点寻址，应用透明度/缩放/平移/旋转并按轨道顺序混合；`timeline_audio.rs` 按时间线的空隙、片段位置、源选区和静音状态混合 PCM。Desktop 的 `timeline_preview.rs`、`timeline_sound.rs` 仅做任务调度、图像上传、Rodio 设备与播放时钟适配。
- `timeline_state.rs`、`timeline_actions.rs` 保存 UI 手势并调用 Core；`timeline_view.rs`、`timeline_clip.rs` 与 `timeline_tracks.rs` 绘制刻度、片段、轨道头及滚动条，`timeline_drag.rs` 绘制缩略图光标。`inspector.rs` 沿用原生 TextInput 处理参数输入。
- `trim_handle.rs` 是素材预览与时间线共用的裁剪把手；把手在片段两端向外延伸 12px，时间零点在轨道头右侧额外预留 12px，右端也保留对应空间。刻度、播放头、拖入和框选采用同一时间原点；轨道头与内容使用相同的垂直裁切区。
- 连续寻址串行消费完成的中间帧并追赶最新请求，避免每次移动都丢帧造成预览饥饿。`ScrubPreview` 默认 `Live`，保留 `OnRelease` 宿主设置接口；当前没有设置窗口，也不把它写入项目。
- 时间线元数据在释放手势时串行原子保存，索引失败回滚内存和项目；媒体探测/解码仍在后台。预览采用低分辨率软件合成，不代表最终导出分辨率或已验证的多轨实时性能。
- 最新 `.ref/轻线-状态图标4枚/qingxian-state-controls-4/svg` 中实际包含七枚 SVG，原样进入图标包：上下箭头、睁眼、闭眼、开锁、文本、低音量。清单 v1.7 共 102 枚；闭眼采用原始眼睑/睫毛造型。
