# Dinuosi（迪诺斯）产品 PRD v0.1

本文记录产品目标与已确认的行为约定，不把规划能力视为已实现功能。当前实现核对日期：2026-10-12；可用功能见 [README](../README.md)，实现边界见 [架构说明](ARCHITECTURE.md)，导出行为见 [EXPORT.md](EXPORT.md)。

| 范围 | 当前实现 | 后续规划 |
| --- | --- | --- |
| 项目与剪辑 | 项目管理、引用式素材库、画面/音频多轨、拖放、裁边、刀片、多选、撤销/重做、磁吸、标记、片段锁定 | 字幕编辑、Motion Composition、片段链接及其他未接入的剪辑操作 |
| 参数与关键帧 | 位置、缩放、旋转、透明度、音量及其线性关键帧；创建时必须绑定参数并记录数值 | Graph Editor、更多参数与插值方式 |
| 预览与导出 | 视频/音频/MIDI 预览、波形、全屏观看、多轨 MP4/MKV 导出、进度与取消 | Proxy、Optimized Media、复杂处理链预览缓存、统一任务队列 |
| 自动化与扩展 | 共享 Core 接口及项目生命周期 Command 层 | Action、插件运行时、用户 CLI、MCP、官方 Skill、AI 功能 |

下文第 10–25 节描述后续产品目标；已有预览缓存和导出后台任务不代表这些规划系统已完整实现。已存在图标、分类或禁用控件也不代表对应功能可用。

## 1. 产品概述

**产品名称：** Dinuosi（迪诺斯）

**产品形态：** 桌面端开源视频剪辑软件

**核心定位：**

> 一款本地优先、开源、可扩展，同时兼顾传统精确剪辑与一键智能处理的视频创作工具。

Dinuosi 不以“专业功能必须复杂”为前提，也不以“新手友好就必须黑盒化”为前提。

产品应同时服务两类操作方式：

- 新手：通过一键处理、预设、Action 快速完成工作。
- 进阶用户：保留时间线、关键帧、参数、Motion、插件、CLI 等完整控制能力。

---

# 2. 产品原则

### 2.1 本地优先

基础剪辑不能依赖互联网。

断网后仍应正常使用：

- 项目管理
- 素材导入
- 时间线剪辑
- 字幕编辑
- 关键帧
- 音频处理
- Motion
- 导出
- 已下载模型的 AI 功能
- Action
- 插件

需要联网的能力必须明确标注。

本地模型允许：

> 慢，但不限次数。

---

### 2.2 导出不设“最后一道会员墙”

原则：

> 用户成功使用并写入项目的本地功能，不能到导出阶段才突然要求付费。

如果未来存在收费能力，应在使用前明确说明。

可能收费的合理范围主要是：

- 云端模型推理
- 云渲染
- 云同步
- 团队协作
- 托管素材
- 第三方付费服务

而不是：

- 导出
- 基础字幕
- 色度抠图修边
- 基础降噪
- 时间线编辑

---

# 3. 目标用户

主要面向：

1. 刚接触视频剪辑的新手
2. 普通内容创作者
3. 科技、游戏、知识类视频创作者
4. 希望使用开源剪辑工具的用户
5. 希望自动化批处理视频的高级用户
6. 希望由 AI Agent 操作剪辑器的开发者

---

# 4. 核心功能

## 4.1 项目管理

启动页提供：

- 新建项目
- 导入项目
- 设置
- 最近项目

项目列表显示：

- 项目名称
- 创建时间
- 修改时间

项目操作通过已有更多菜单提供。列表不显示缩略图、项目数量或额外状态文本。

项目文件必须支持版本迁移。

---

# 5. 剪辑主界面

主要区域：

```text
┌────────────────────────────────────────┐
│ 菜单 / 导出                             │
├──────────┬────────────────┬────────────┤
│ 素材库   │ 实时预览        │ 检查器     │
│          │                │            │
├──────────┴────────────────┴────────────┤
│ 时间线工具栏                            │
├────────────────────────────────────────┤
│ 时间线                                  │
├────────────────────────────────────────┤
│ 状态栏                                  │
└────────────────────────────────────────┘
```

标题栏仅保留既有菜单与导出入口，导出按钮左侧不显示项目名称。统一任务状态展示属于第 12 节规划。

---

# 6. 时间线

当前支持：

- 主轨与多画面轨（视频、图片）
- 多音频轨
- 片段锁定
- 时间标记
- 关键帧
- 时间线缩放
- 磁吸

主轨按顺序放置；其他画面/音频轨可自由定位，同轨片段不重叠。音频进入独立音频轨，不放入画面轨。

字幕编辑、Motion Composition、片段链接及完整波纹编辑属于后续规划；字幕轨道类型已预留，尚不能接收可编辑字幕。

---

# 7. 时间线工具栏

当前确定：

以下为工具栏布局与功能目标。当前已接入选择、刀片分割、磁吸、标记、片段锁定、删除和缩放；撤销/重做可通过快捷键操作。镜像、倒放、定格、变速、链接、替换素材、波纹删除及尚未接入的工具栏按钮保持禁用，不计作可用功能。

### 基础编辑

- 选择
- 分割

### 历史操作

- 撤销
- 重做 / 还原

### 时间线辅助

- 磁吸
- 标记点
- 锁定位置

### 片段变换

- 左右镜像
- 上下镜像
- 倒放
- 定格
- 变速

### 片段关系

- 链接
- 取消链接

### 素材操作

- 替换素材

### 删除

- 波纹删除
- 删除

右侧单独提供：

- 时间线放大
- 时间线缩小
- 缩放滑杆

参数关键帧通过检查器操作；时间线上显示已有关键帧并提供定位、删除。Motion 的关键帧界面属于后续规划。

---

# 8. 关键帧

Dinuosi 不允许创建空关键帧。每个关键帧从创建时起就必须属于一个素材的具体参数类型，并保存该参数在指定帧的有效数值。

创建流程：

```text
选中素材，在检查器选择具体参数
↓
将播放头定位到素材范围内
↓
点击该参数的关键帧菱形
↓
记录当前位置的参数值（已有动画时记录当前插值）
```

空心菱形表示当前帧没有该参数的关键帧，实心菱形表示已有；空心状态不是已创建的空关键帧。手动记录当前值不要求先改变数值，也不会生成待填充节点。只用于时间定位的节点使用项目标记，与参数关键帧分开。

当前规则：

- 画面支持位置、缩放、旋转、透明度；音频支持音量。位置 X/Y 共用一个两分量关键帧，其余类型独立。
- 同素材、同类型、同一帧只有一个关键帧；同帧可记录不同类型。检查器仅在唯一选中且播放头位于素材范围内时提供对应控制，锁定素材不可编辑。
- 当前为线性插值，首尾之外保持端点。两帧以上提供同类型前后导航。
- 自动添加默认开启；仅同类型已有动画且数值实际改变时自动增加关键帧。尚未启用动画的参数仍修改静态值。
- 当前已有关键帧时修改该帧；关闭自动添加后的帧间编辑整体调整同类型动画。删除最后一个关键帧时保留当前值为静态参数。
- 时间线菱形合并同帧的不同类型；删除该菱形会删除该素材该帧的全部类型。保存与撤销复用既有编辑事务。

画面检查器示例（音频检查器对应音量）：

```text
位置      ◇
缩放      ◇
旋转      ◇
透明度    ◇
```

---

# 9. 素材放置逻辑

Dinuosi 不强制要求新手理解：

- Insert Edit
- Overwrite Edit
- Replace Edit

优先通过拖拽语义完成。

例如：

```text
拖入两个片段之间
→ 插入
→ 后方片段后移

拖入已有轨道片段区域
→ 根据落点给出预览，冲突时放入上层轨道，不覆盖已有素材

拖入上层轨道
→ 叠加

替换素材
→ 规划为保留当前片段结构，仅更换源素材；当前入口未接入
```

专业模式以后可提供：

- 插入编辑
- 覆盖编辑
- 替换编辑

但不必长期占用基础工具栏。

---

# 10. Proxy 代理媒体

4K、高码率、H.265、AV1 等素材可以生成代理文件。

原则：

```text
编辑 / 预览
→ Proxy

最终导出
→ Original
```

代理不得改变：

- 时长
- 帧映射
- 时间码关系
- 音视频同步

支持：

- 自动代理
- 手动代理
- 不生成
- 代理质量选择

并区分：

### Proxy

主要降低剪辑负载。

### Optimized Media

主要把素材转成更适合随机访问和剪辑的编码。

---

# 11. 预览缓存

代理解决：

> 源素材本身太重。

预览缓存解决：

> 特效、Motion、AI 处理链太重。

例如：

```text
Motion + Blur + Glow + Tracking
↓
Render Preview Cache
```

之后直接播放缓存结果。

---

# 12. 任务队列

所有耗时操作均进入统一任务系统。

例如：

- 字幕识别
- 双语翻译
- 音频分离
- 降噪
- 超分
- 插帧
- 物体追踪
- 代理生成
- 预览缓存
- 导出

用户开始任务后：

> 弹窗可以关闭，继续正常剪辑。

左下角显示简洁状态：

```text
正在处理 2 · 等待 3 · 47%
```

点击展开：

```text
字幕识别      78%
音频分离      34%
超分          等待
插帧          等待
```

支持：

- 暂停
- 取消
- 重试
- 调整优先级
- 同时任务数
- 自动资源调度

默认：

> 剪辑预览优先于后台 AI。

---

# 13. 智能能力

计划包括：

### 字幕

- 自动字幕
- 字幕编辑
- 双语字幕
- 本地翻译
- 智能划重点
- 标记无效片段
- 一键删除口水词

### 音频

- 人声 / 伴奏分离
- 多 Stem 分离
- 音频降噪
- 响度标准化
- 淡入淡出

### 视频

- 画面超分
- 分辨率提升
- AI 插帧
- 去闪烁
- 去噪
- 画面增强
- 动态模糊
- 物体追踪
- 色度抠图

---

# 14. 色度抠图

基础能力应包括：

- 色彩取样
- 阈值
- 相似度
- 边缘羽化
- 边缘收缩 / 扩张
- Despill / 边缘清除
- 遮罩预览

这些能力属于基础合成工具，不应依赖云端。

---

# 15. Action

Action 是 Dinuosi 的核心特色之一。

用户可以：

- 手动编写 YAML
- 录制操作
- 导入他人 Action
- 分享 Action
- 一键执行

例如：

```yaml
name: 口播快速处理

steps:
  - action: audio.denoise
    strength: 0.65

  - action: subtitle.transcribe

  - action: speech.remove_fillers

  - action: audio.normalize
    target_lufs: -14
```

录制的不是：

```text
点击 (123, 456)
```

而是：

```text
clip.split
audio.denoise
subtitle.transcribe
```

Action 本质：

> Command API 的声明式工作流。

---

# 16. Motion / MG

Dinuosi 不采用：

> 一个剪辑软件 + 一个独立动效软件

的产品结构。

而是：

> 同一个项目中的 Edit 与 Motion 工作区。

例如：

```text
Edit | Motion | Audio | Subtitle | Color
```

Motion Composition 在时间线上表现为普通片段。

双击进入 Motion 工作区。

---

# 17. Motion 基础能力

支持：

- Text
- Shape
- Image
- Video
- Group
- Composition
- Camera
- Light
- Path
- Mask

可动画参数包括：

- Position
- Scale
- Rotation
- Anchor
- Opacity
- Shape 属性
- Stroke
- Fill
- Path
- Blur
- Glow
- Effect 参数

---

# 18. Graph Editor

支持：

- Linear
- Ease In
- Ease Out
- Ease In Out
- Bezier
- Hold
- Spring

普通用户使用 Preset。

高级用户编辑曲线。

---

# 19. Motion Template

Motion 模板可以暴露参数：

```yaml
parameters:
  title:
    type: string

  value:
    type: number

  image:
    type: media

  accent:
    type: color
```

用户只修改参数，不需要进入复杂图层。

---

# 20. 图表系统

针对科技测评、知识视频重点支持：

- Bar
- Line
- Pie
- Scatter
- Radar

支持：

- CSV
- JSON
- Excel 粘贴
- 数据绑定
- 数字动画
- 柱形增长
- 折线绘制
- Stagger Animation

---

# 21. Auto Layout

Motion 支持类似 UI 设计工具的：

- Horizontal Stack
- Vertical Stack
- Gap
- Padding
- Align
- Constraint

从而降低绝对坐标布局带来的复杂度。

---

# 22. 插件系统

插件可以扩展：

- Effect
- Motion
- AI Model
- Exporter
- Importer
- Action
- Tool
- Asset Source

插件必须声明权限：

```yaml
permissions:
  - project.read
  - project.write
  - filesystem.cache

network: false
```

默认原则：

> 插件不得绕过 Dinuosi 核心直接破坏源素材。

---

# 23. CLI

无 GUI 也能操作 Dinuosi。

例如：

```bash
dinuosi project open demo.dinuosi

dinuosi clip split \
  --clip clip_42 \
  --time 12.5

dinuosi action run clean-talk.yaml

dinuosi export \
  --preset h264-1080p
```

---

# 24. MCP

Dinuosi 提供 MCP Server。

AI 可调用：

```text
get_project
get_timeline
list_clips
split_clip
delete_clip
replace_media
set_speed
transcribe
translate_subtitles
run_action
render
get_task_status
```

耗时操作返回：

```json
{
  "task_id": "task_123",
  "status": "queued"
}
```

---

# 25. Skill

官方提供 Dinuosi Skill。

Skill 负责教 Agent：

- 如何剪辑
- 如何组织节奏
- 如何使用字幕
- 如何选择 Motion
- 如何制作图表
- 如何避免滥用动画
- 如何调用 MCP
- 如何生成 Action

Skill 是 AI 的行为指导，而不是剪辑引擎的一部分。

---

# 26. 图标系统

Dinuosi 使用自有图标库。

当前图标清单为 v1.9，共 104 枚，来源为版本控制内的 [spec.json](../crates/icons/spec.json)。它覆盖时间线、视频、音频、窗口、剪辑等场景；例如 `razor`、`timeline`、`snap`、`keyframe`、`transition`、`subtitles`、`waveform` 等已经存在。图标资源与功能是否实现分别判断。

v1.4 又补充了：

- 倒放
- 定格
- 左右镜像
- 上下镜像
- 左右旋转
- Undo
- Redo
- 参数重置
- 关联片段
- 解除关联
- 波纹删除

这些都是当前图标系统的一部分。后续已加入 `replace-media`、分类图标、`keyframe-filled` 和仅用于鼠标的 `razor-cursor`。

未来增加：

```text
proxy
task-queue
action
action-record
transcribe
bilingual-subtitle
audio-separate
denoise
upscale
frame-interpolation
chroma-key
object-tracking
motion-composition
graph-editor
precompose
plugin
mcp
```

---

