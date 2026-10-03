# Dinuosi（迪诺斯）产品 PRD v0.1

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

项目卡片显示：

- 缩略图
- 项目名称
- 创建时间
- 修改时间
- 更多操作

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
│ 当前任务                                │
└────────────────────────────────────────┘
```

---

# 6. 时间线

支持：

- 多视频轨
- 多音频轨
- 字幕轨
- Motion Composition
- 片段链接
- 片段锁定
- 时间标记
- 关键帧
- 时间线缩放
- 磁吸
- 波纹编辑

---

# 7. 时间线工具栏

当前确定：

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

关键帧不再强制放在该栏，可主要进入检查器和 Motion 系统。

---

# 8. 关键帧

Dinuosi 的关键帧不必立即绑定参数。

允许：

```text
创建关键帧
↓
暂时为空
↓
后续再写入位置 / 缩放 / 旋转 / 参数
```

关键帧可作为：

- 动画锚点
- 参数变化点
- Motion 时间节点
- Action 引用点

检查器中的具体属性应拥有独立的关键帧控制。

例如：

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
→ 覆盖 / 根据落点给出明确预览

拖入上层轨道
→ 叠加

替换素材
→ 保留当前片段结构，仅更换源素材
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

当前基础图标集已经覆盖时间线、视频、音频、窗口、剪辑等大量通用场景；例如 `razor`、`timeline`、`snap`、`keyframe`、`transition`、`subtitles`、`waveform` 等已经存在。spec

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

这些都是当前图标系统的一部分。spec

未来增加：

```text
replace-media
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

