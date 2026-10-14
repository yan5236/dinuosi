轻线 · 完整界面图标集 / Qingxian Interface Icons
应用现行清单见 spec.json：v1.10，共 105 枚，新增 24px 轻线取色器 eyedropper.svg。
下文保留 v1.5 原始素材包的交付说明。
版本 1.5 · 89 枚完整版 · 2026-10-02

一、这是什么
89 枚用于网站、软件、视频剪辑和桌面窗口界面的原生 SVG 图标。
延续前版 88 枚，新增“替换素材”，并优化“重置参数”的内部间距。
统一 24 × 24 网格、1.8 单位描边、圆角端点和连接。
透明背景、currentColor；无位图和外部资源依赖。
本次两枚图标已用 Inkscape 1.4.4 命令行矢量编辑、导出并完成视觉检查。

二、文件说明
svg/                                  89 个独立 SVG
qingxian-master-88.svg                 Inkscape 可编辑分层母版
qingxian-preview-88.png                全套 89 枚总览
qingxian-preview-editing-extra-12.png  v1.4 新增 12 枚预览（已同步重置参数）
icons-sprite.svg                       全套 89 个 symbol 精灵图
demo.html                             可直接离线打开的示例
spec.json                             名称、规格及加入版本
qingxian-preview-replace-reset.png     本次两枚图标的深浅色与尺寸预览
check_icons.py                        独立图标、精灵与母版一致性检查

目录名与母版、总览文件名中的 88 为兼容历史路径保留；实际数量为 89。

三、图标清单
原有 76 枚：
home                首页    user                我的
star                收藏    search              搜索
settings            设置    menu                菜单
close               关闭    chevron-left        上一项
chevron-right       下一项    bell                通知
mail                消息    folder              文件夹
file                文档    download            下载
upload              上传    edit                编辑
trash               删除    check               完成
plus                新增    heart               喜欢
calendar            日历    lock                安全
link                链接    more                更多
image               图片    camera              相机
play                播放    pause               暂停
volume              音量    microphone          麦克风
share               分享    copy                复制
refresh             刷新    filter              筛选
help                帮助    info                信息
logout              退出    cart                购物车
pin                 定位    globe               全球
razor               刀片切割    select              选择工具
trim-start          裁剪入点    trim-end            裁剪出点
timeline            时间线    track               轨道
snap                磁吸对齐    keyframe            关键帧
transition          转场    subtitles           字幕
waveform            音频波形    export-video        导出视频
import-media        导入素材    video               视频片段
record              录制    stop                停止
skip-back           上一剪辑点    skip-forward        下一剪辑点
speed               播放速度    crop                画面裁切
marker              时间标记    mute                静音
zoom-in             放大时间线    zoom-out            缩小时间线
minimize            最小化    maximize            最大化
restore             还原窗口    fullscreen          进入全屏
exit-fullscreen     退出全屏    sidebar-left        左侧边栏
sidebar-right       右侧边栏    panel-bottom        底部面板
split-horizontal    左右分栏    split-vertical      上下分栏
pin-window          窗口置顶    picture-in-picture  画中画

v1.4 新增 12 枚：
reverse-playback     倒放
freeze-frame         定格
mirror-horizontal    左右镜像
mirror-vertical      上下镜像
rotate-left          向左旋转
rotate-right         向右旋转
undo                 撤销
redo                 重做
reset-parameters     重置参数
link-clips           关联片段
unlink-clips         解除关联
ripple-delete        波纹删除

本次新增 1 枚（v1.5）：
replace-media        替换素材

含义与使用建议：
replace-media（替换素材）：用新素材替换选中的素材；双向替换箭头、单个目标片段与两侧时间线表达素材替换，省去缩略图和轨道分隔细节。
reset-parameters 在 v1.5 中缩短箭头头部并下移调节线，拉开内部间距。
reverse-playback（倒放）：反向播放素材，图形为回转箭头和反向播放三角。
freeze-frame（定格）：从当前帧生成静帧，图形为画框和冰晶符号。
它们与 skip-back（上一剪辑点）、pause（暂停）是不同操作。
mirror-horizontal / mirror-vertical 分别是左右/上下镜像，虚线是对称轴。
rotate-left / rotate-right 为向左/向右旋转；具体旋转角度由软件实现决定。
undo（撤销）：撤回上一步操作。
redo（重做）：重新应用刚才撤销的操作。
reset-parameters（重置参数）：将当前参数恢复为默认值，和重做不同。
restore（还原窗口）：仍保留为窗口尺寸还原；只有两个叠放空心方框。
maximize（最大化）：仍保留单个空心方框，无标题栏细节。
link-clips / unlink-clips 为片段关联/解除关联。
ripple-delete（波纹删除）：删除选中的片段并闭合其后间隙。
这些 SVG 仅提供图形，具体编辑逻辑需在软件中实现。
NEW 标记仅存在于总览与示例中，不出现在独立图标内。

四、最简单的使用方式
以普通图片使用：
<img src="./svg/home.svg" width="24" height="24" alt="首页">

注意：用 img 引用外部 SVG 时，图标不能继承网页的 CSS color，
默认显示为黑色。需要动态换色时，使用下方的内联 SVG 或精灵图。

五、随主题变色（推荐）
将对应 SVG 文件的完整内容复制到 HTML 中，并给 SVG 添加
class="icon"。CSS：

.icon {
  width: 24px;
  height: 24px;
  color: #4965ee;
  vertical-align: middle;
}

SVG 中的 currentColor 会继承 color，适用于悬停、选中态和深色主题。
改变 width/height 可整体缩放；48px 显示时，描边也随之等比例放大。
不要给 SVG 添加 fill: currentColor，会填满原本为空心的图标。

六、精灵图使用
在通过 HTTP/HTTPS 运行、并与精灵图同源的网页中：

<svg width="24" height="24" style="color:#4965ee" aria-hidden="true">
  <use href="./icons-sprite.svg#icon-home"></use>
</svg>

symbol ID 为 icon- 加文件名，如 icon-star、icon-chevron-right。
直接通过 file:// 打开网页时，浏览器可能限制外部精灵图加载。
demo.html 已内嵌精灵图，因此可离线直接打开。

七、无障碍建议
纯装饰图标：在 SVG 上设置 aria-hidden="true"。
单独传递信息的图标：设置 role="img" 和 aria-label="首页"。
只有图标的按钮：在 button 上设置 aria-label="收藏"。
已有可见按钮文字时，让图标保持 aria-hidden="true" 即可。

八、在 Inkscape 中编辑
打开 qingxian-master-88.svg，在“图层与对象”面板中选择
“04 · 可编辑图标”。每个图标为单独命名的组。
母版保留路径、圆形、矩形等可编辑矢量对象，未转为位图。
可用“节点工具”编辑路径，用“填充和描边”调整样式。
需要用于产品时，优先编辑 svg/ 中对应的 24 × 24 文件并保留页面尺寸。
母版中的图标以 2.4 倍展示；网站接入请使用独立图标文件。
图标默认以 24px 使用；轨道、字幕等细节较多的图标建议不小于 20px。
母版标签使用 Noto Sans CJK SC；未安装时会回退到系统中文字体。
图标本身不依赖字体。PNG 预览不受字体差异影响。

九、检查记录
89 个 SVG 均通过 XML 和 24 × 24 viewBox 检查；精灵、离线示例、母版中的矢量内容一致。
全部使用 currentColor、1.8 描边、round 端点和连接。
历史 v1.4 记录：原 88 枚曾完成 24、48、240px 导出检查，共 264 次。
本次 v1.5：仅替换素材和重置参数重新完成 24、48、240px 导出，共 6 次。
两枚图标均保留透明安全边距，无边缘裁切；已检查深浅色及实际 24px 可读性。
已重新导出母版总览和进阶剪辑 12 枚预览；图标无内嵌位图和外部链接。
执行一致性检查：python check_icons.py
