//! 片段级 SDR 调色契约与编辑；显示工具与渲染共享范围，不依赖桌面框架。
use crate::{media::MediaKind, timeline::Timeline};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Grade {
    pub enabled: bool,
    pub stages: Vec<Stage>,
}
impl Default for Grade {
    /// 创建启用的中性单步骤；旧项目没有调色字段时保持原像素。
    fn default() -> Self {
        Self {
            enabled: true,
            stages: vec![Stage::default()],
        }
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Stage {
    pub enabled: bool,
    /// 曝光 EV、对比度、色温、色调、饱和度、自然饱和度、高光、阴影、白色、黑色。
    pub basic: [f64; 10],
    /// 红、橙、黄、绿、青、蓝、紫、品红的色相、饱和度、亮度。
    pub hsl: [[f64; 3]; 8],
    #[serde(skip_serializing_if = "Option::is_none")]
    pub picked: Option<PickedHsl>,
    /// 总曲线及 R/G/B 的归一化输入/输出控制点，端点横坐标必须为 0 和 1。
    pub curves: [Vec<[f64; 2]>; 4],
    /// 阴影、中间调、高光的二维色偏与亮度，范围 -100..100。
    pub wheels: [[f64; 3]; 3],
    pub mask: Mask,
    pub qualifier: Qualifier,
}
impl Default for Stage {
    /// 创建全部参数为中性、曲线为恒等且局部选区关闭的步骤。
    fn default() -> Self {
        Self {
            enabled: true,
            basic: [0.0; 10],
            hsl: [[0.0; 3]; 8],
            picked: None,
            curves: std::array::from_fn(|_| vec![[0.0, 0.0], [1.0, 1.0]]),
            wheels: [[0.0; 3]; 3],
            mask: Mask::default(),
            qualifier: Qualifier::default(),
        }
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PickedHsl {
    /// 吸管取样的色相中心（0..360 度，不含 360），独立于八个固定色组。
    pub hue: f64,
    pub values: [f64; 3],
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Mask {
    pub enabled: bool,
    pub ellipse: bool,
    pub inverted: bool,
    /// 中心 X/Y、宽/高、旋转角度、羽化；坐标相对有效素材，羽化为半径比例。
    pub values: [f64; 6],
}
impl Default for Mask {
    /// 默认关闭的居中椭圆，半画面宽高，边缘羽化 10%。
    fn default() -> Self {
        Self {
            enabled: false,
            ellipse: true,
            inverted: false,
            values: [0.5, 0.5, 0.5, 0.5, 0.0, 0.1],
        }
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Qualifier {
    pub enabled: bool,
    /// 色相中心/半宽/柔化（度）、最低饱和度、最低/最高亮度。
    pub values: [f64; 6],
}
impl Default for Qualifier {
    /// 默认关闭；启用时选红色附近、排除无色区域，不限制亮度。
    fn default() -> Self {
        Self {
            enabled: false,
            values: [0.0, 30.0, 15.0, 0.1, 0.0, 1.0],
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Control {
    Basic(usize),
    Hsl(usize, usize),
    PickedHsl(usize),
    Wheel(usize, usize),
    Mask(usize),
    Qualifier(usize),
}
impl Control {
    /// 判断固定工具索引是否有效，拒绝磁盘关键帧中伪造或越界的参数身份。
    pub fn valid(self) -> bool {
        match self {
            Self::Basic(i) => i < 10,
            Self::Hsl(g, i) => g < 8 && i < 3,
            Self::PickedHsl(i) => i < 3,
            Self::Wheel(w, i) => w < 3 && i < 3,
            Self::Mask(i) | Self::Qualifier(i) => i < 6,
        }
    }
    /// 枚举全部可动画标量；曲线控制点和功能开关保持静态，不产生没有效果的动画节点。
    pub fn all() -> impl Iterator<Item = Self> {
        (0..10)
            .map(Self::Basic)
            .chain((0..8).flat_map(|g| (0..3).map(move |i| Self::Hsl(g, i))))
            .chain((0..3).flat_map(|w| (0..3).map(move |i| Self::Wheel(w, i))))
            .chain((0..6).map(Self::Mask))
            .chain((0..6).map(Self::Qualifier))
            .chain((0..3).map(Self::PickedHsl))
    }
    /// 返回合法控制范围；索引由宿主固定工具生成，未知索引不可作为外部数据入口。
    pub fn range(self) -> (f64, f64) {
        match self {
            Self::Basic(0) => (-5.0, 5.0),
            Self::Mask(0..=1) => (0.0, 1.0),
            Self::Mask(2..=3) => (0.01, 2.0),
            Self::Mask(4) => (-180.0, 180.0),
            Self::Mask(5) => (0.0, 1.0),
            Self::Qualifier(0) => (0.0, 360.0),
            Self::Qualifier(1) => (0.0, 180.0),
            Self::Qualifier(2) => (0.0, 90.0),
            Self::Qualifier(_) => (0.0, 1.0),
            _ => (-100.0, 100.0),
        }
    }
    /// 返回控件数值小数位：归一化选区为两位，曝光一位，其余整数。
    pub fn precision(self) -> usize {
        match self {
            Self::Basic(0) => 1,
            Self::Mask(0..=3 | 5) | Self::Qualifier(3..=5) => 2,
            _ => 0,
        }
    }
}
impl Stage {
    /// 判断本步骤是否有指定参数；吸管色组只在实际取样后存在，防止空色组产生无效果关键帧。
    pub fn supports(&self, c: Control) -> bool {
        c.valid() && (!matches!(c, Control::PickedHsl(_)) || self.picked.is_some())
    }
    /// 借用指定控件参数；越界索引返回 None，防止过期宿主状态引发 panic。
    fn value_mut(&mut self, c: Control) -> Option<&mut f64> {
        match c {
            Control::Basic(i) => self.basic.get_mut(i),
            Control::Hsl(g, i) => self.hsl.get_mut(g)?.get_mut(i),
            Control::PickedHsl(i) => self.picked.as_mut()?.values.get_mut(i),
            Control::Wheel(w, i) => self.wheels.get_mut(w)?.get_mut(i),
            Control::Mask(i) => self.mask.values.get_mut(i),
            Control::Qualifier(i) => self.qualifier.values.get_mut(i),
        }
    }
    /// 读取参数用于数值与滑块显示；非法索引显示零，修改路径仍拒绝该索引。
    pub fn get(&self, c: Control) -> f64 {
        match c {
            Control::Basic(i) => self.basic.get(i),
            Control::Hsl(g, i) => self.hsl.get(g).and_then(|v| v.get(i)),
            Control::PickedHsl(i) => self.picked.as_ref().and_then(|v| v.values.get(i)),
            Control::Wheel(w, i) => self.wheels.get(w).and_then(|v| v.get(i)),
            Control::Mask(i) => self.mask.values.get(i),
            Control::Qualifier(i) => self.qualifier.values.get(i),
        }
        .copied()
        .unwrap_or(0.0)
    }
    /// 设置有限参数并夹紧到业务范围；亮度选区两端保持有序，错误不产生写入。
    pub fn set(&mut self, c: Control, value: f64) -> Result<(), String> {
        if !value.is_finite() {
            return Err("调色参数必须为有限数值".into());
        }
        let (min, max) = c.range();
        let v = value.clamp(min, max);
        if matches!(c, Control::Qualifier(4)) && v > self.qualifier.values[5]
            || matches!(c, Control::Qualifier(5)) && v < self.qualifier.values[4]
        {
            return Err("颜色选区亮度范围无效".into());
        }
        self.set_value(c, value)
    }
    /// 对已验证或插值产生的有限值写入并夹紧范围；求值后统一校正选区两端，避免逐端写入误拒绝。
    pub(crate) fn set_value(&mut self, c: Control, value: f64) -> Result<(), String> {
        let (min, max) = c.range();
        let v = value.clamp(min, max);
        *self.value_mut(c).ok_or("调色参数索引无效")? = v;
        Ok(())
    }
    /// 判断当前步骤是否实际改变 RGB；选区不改变中性步骤的画面。
    pub fn neutral(&self) -> bool {
        !self.enabled
            || (self.basic == [0.0; 10]
                && self.hsl == [[0.0; 3]; 8]
                && self.picked.as_ref().is_none_or(|p| p.values == [0.0; 3])
                && self.wheels == [[0.0; 3]; 3]
                && self.curves.iter().all(|p| p.iter().all(|v| v[0] == v[1])))
    }
}
impl Grade {
    /// 判断整个处理链是否中性或旁路，用于源帧零拷贝快路径。
    pub fn neutral(&self) -> bool {
        !self.enabled || self.stages.iter().all(Stage::neutral)
    }
    /// 校验磁盘/粘贴输入，包括停用步骤；拒绝非有限数、越界、密集曲线和无限步骤。
    pub fn validate(&self) -> Result<(), String> {
        if self.stages.is_empty() || self.stages.len() > 8 {
            return Err("调色步骤数量应为 1–8".into());
        }
        for s in &self.stages {
            if s.picked
                .as_ref()
                .is_some_and(|p| !p.hue.is_finite() || !(0.0..360.0).contains(&p.hue))
            {
                return Err("取样色组色相无效".into());
            }
            for c in Control::all() {
                let v = s.get(c);
                let (min, max) = c.range();
                if !v.is_finite() || v < min || v > max {
                    return Err("调色参数范围无效".into());
                }
            }
            if s.qualifier.values[4] > s.qualifier.values[5] {
                return Err("颜色选区亮度范围无效".into());
            }
            for p in &s.curves {
                if p.len() < 2
                    || p.len() > 32
                    || p[0][0] != 0.0
                    || p[p.len() - 1][0] != 1.0
                    || p.iter()
                        .flatten()
                        .any(|v| !v.is_finite() || !(0.0..=1.0).contains(v))
                    || p.windows(2).any(|v| v[1][0] - v[0][0] < 0.001)
                {
                    return Err("调色曲线控制点无效".into());
                }
            }
        }
        Ok(())
    }
}
impl Timeline {
    /// 对选中且未锁定的图片/视频原子替换整套静态调色并清除旧调色动画；音频跳过，非法输入不产生写入。
    pub fn set_color(&mut self, ids: &BTreeSet<String>, grade: &Grade) -> Result<(), String> {
        grade.validate()?;
        for c in self.clips.iter_mut().filter(|c| {
            ids.contains(&c.id)
                && c.kind != MediaKind::Audio
                && !c.locked
                && !self.tracks[c.track].locked
        }) {
            c.color = grade.clone();
            c.keyframes
                .retain(|k| !matches!(k.kind, crate::keyframes::KeyframeKind::Color(..)));
        }
        Ok(())
    }
}
