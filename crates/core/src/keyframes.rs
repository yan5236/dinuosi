//! 片段局部帧上的独立参数关键帧；曲线求值、端点保持及编辑规则不依赖界面。
use crate::{
    media::MediaKind,
    timeline::{Clip, Property, Timeline, Transform},
};
use std::collections::BTreeSet;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum KeyframeKind {
    Opacity,
    Scale,
    Position,
    Rotation,
    Volume,
    Color(usize, crate::color::Control),
}
#[derive(Clone, Debug, PartialEq)]
pub struct Keyframe {
    /// 相对于片段起点的帧；裁剪/分割保留范围外锚点以保持原动画。
    pub frame: i64,
    pub kind: KeyframeKind,
    pub values: [f64; 2],
    /// 每个分量独立保存入/出切线；缺省保持旧项目的线性插值。
    pub curves: [Option<crate::keyframe_curve::KeyframeCurve>; 2],
}
impl KeyframeKind {
    /// 返回稳定磁盘类型名；坐标用两个值，其余类型第二个值固定为零。
    pub fn name(self) -> String {
        match self {
            Self::Opacity => "opacity".into(),
            Self::Scale => "scale".into(),
            Self::Position => "position".into(),
            Self::Rotation => "rotation".into(),
            Self::Volume => "volume".into(),
            Self::Color(stage, control) => {
                use crate::color::Control as C;
                let suffix = match control {
                    C::Basic(i) => format!("basic/{i}"),
                    C::Hsl(g, i) => format!("hsl/{g}/{i}"),
                    C::PickedHsl(i) => format!("picked/{i}"),
                    C::Wheel(w, i) => format!("wheel/{w}/{i}"),
                    C::Mask(i) => format!("mask/{i}"),
                    C::Qualifier(i) => format!("qualifier/{i}"),
                };
                format!("color/{stage}/{suffix}")
            }
        }
    }
    /// 解析磁盘类型，未知类型返回错误而不丢弃项目动画。
    pub fn parse(name: &str) -> Result<Self, String> {
        match name {
            "opacity" => Ok(Self::Opacity),
            "scale" => Ok(Self::Scale),
            "position" => Ok(Self::Position),
            "rotation" => Ok(Self::Rotation),
            "volume" => Ok(Self::Volume),
            _ => crate::color_animation::parse_kind(name),
        }
    }
    /// 将检查器参数映射到动画类型；X/Y 同属于坐标，音量独立处理。
    pub fn for_property(p: Property) -> Self {
        match p {
            Property::Opacity => Self::Opacity,
            Property::Scale => Self::Scale,
            Property::X | Property::Y => Self::Position,
            Property::Rotation => Self::Rotation,
        }
    }
    /// 判断素材可编辑的类型；音频仅支持音量，画面支持几何与调色参数。
    pub fn accepts(self, kind: MediaKind) -> bool {
        (self == Self::Volume) == (kind == MediaKind::Audio)
    }
    /// 返回各值的业务范围，防止非法存储或整体调整溢出。
    pub fn range(self, component: usize) -> (f64, f64) {
        match self {
            Self::Position => Property::X.range(),
            Self::Volume if component == 0 => (0.0, 10.0),
            Self::Opacity if component == 0 => Property::Opacity.range(),
            Self::Scale if component == 0 => Property::Scale.range(),
            Self::Rotation if component == 0 => Property::Rotation.range(),
            Self::Color(_, c) if component == 0 => c.range(),
            _ => (0.0, 0.0),
        }
    }
}
impl Clip {
    /// 判断项目绝对帧是否位于此片段可见范围内；右端为开区间。
    pub fn contains_frame(&self, frame: i64) -> bool {
        frame >= self.start && frame - self.start < self.duration
    }
    /// 从静态参数读取本类型的后备值，无关键帧时使用，坐标同时读取 X/Y。
    fn base_values(&self, kind: KeyframeKind) -> [f64; 2] {
        match kind {
            KeyframeKind::Position => [self.transform.x, self.transform.y],
            KeyframeKind::Opacity => [self.transform.opacity, 0.0],
            KeyframeKind::Scale => [self.transform.scale, 0.0],
            KeyframeKind::Rotation => [self.transform.rotation, 0.0],
            KeyframeKind::Volume => [self.gain, 0.0],
            KeyframeKind::Color(stage, control) => [
                self.color.stages.get(stage).map_or(0.0, |s| s.get(control)),
                0.0,
            ],
        }
    }
    /// 设置已经校验的后备值；删除最后一帧时保留当前画面/声音。
    fn set_base(&mut self, kind: KeyframeKind, v: [f64; 2]) {
        match kind {
            KeyframeKind::Position => {
                self.transform.x = v[0];
                self.transform.y = v[1];
            }
            KeyframeKind::Opacity => self.transform.opacity = v[0],
            KeyframeKind::Scale => self.transform.scale = v[0],
            KeyframeKind::Rotation => self.transform.rotation = v[0],
            KeyframeKind::Volume => self.gain = v[0],
            KeyframeKind::Color(stage, control) => {
                if let Some(s) = self.color.stages.get_mut(stage) {
                    let _ = s.set_value(control, v[0]);
                }
            }
        }
    }
    /// 在项目绝对帧求值：同类关键帧之间逐值按贝塞尔切线插值，缺省线性，范围外保持最近端点。
    /// 支持音频的分数帧求值；源入点与其他动画类型不影响结果。
    pub fn keyframe_values(&self, kind: KeyframeKind, frame: f64) -> [f64; 2] {
        let local = frame - self.start as f64;
        let mut previous = None;
        let mut next = None;
        for k in self.keyframes.iter().filter(|k| k.kind == kind) {
            if k.frame as f64 <= local && previous.is_none_or(|p: &Keyframe| p.frame < k.frame) {
                previous = Some(k);
            }
            if k.frame as f64 >= local && next.is_none_or(|p: &Keyframe| p.frame > k.frame) {
                next = Some(k);
            }
        }
        match (previous, next) {
            (Some(a), Some(b)) if a.frame != b.frame => {
                let t = (local - a.frame as f64) / (b.frame - a.frame) as f64;
                std::array::from_fn(|i| {
                    let (min, max) = kind.range(i);
                    crate::keyframe_curve::interpolate(a, b, i, t).clamp(min, max)
                })
            }
            (Some(k), _) | (_, Some(k)) => k.values,
            _ => self.base_values(kind),
        }
    }
    /// 返回当前帧的完整画面变换，供预览合成、命中与检查器共用；镜像沿用片段静态状态。
    pub fn transform_at(&self, frame: f64) -> Transform {
        let pos = self.keyframe_values(KeyframeKind::Position, frame);
        Transform {
            opacity: self.keyframe_values(KeyframeKind::Opacity, frame)[0],
            scale: self.keyframe_values(KeyframeKind::Scale, frame)[0],
            x: pos[0],
            y: pos[1],
            rotation: self.keyframe_values(KeyframeKind::Rotation, frame)[0],
            ..self.transform.clone()
        }
    }
    /// 返回当前帧的线性增益单位音量，插值使用同一曲线；零值可安全序列化并且严格静音。
    pub fn gain_at(&self, frame: f64) -> f64 {
        self.keyframe_values(KeyframeKind::Volume, frame)[0]
    }
    /// 列出可见范围内某类或全部关键帧的项目绝对帧，同一帧多类型合并为一项。
    pub fn keyframe_frames(&self, kind: Option<KeyframeKind>) -> BTreeSet<i64> {
        self.keyframes
            .iter()
            .filter(|k| kind.is_none_or(|v| v == k.kind) && k.frame >= 0 && k.frame < self.duration)
            .map(|k| self.start + k.frame)
            .collect()
    }
    /// 校验外部动画数据的类型、帧界限、重复帧和有限参数；裁剪范围外锚点允许保留。
    pub(crate) fn validate_keyframes(&self, fps: u32) -> Result<(), String> {
        let mut keys = BTreeSet::new();
        for k in &self.keyframes {
            if !k.kind.accepts(self.kind)
                || matches!(k.kind, KeyframeKind::Color(s, c) if self.color.stages.get(s).is_none_or(|v| !v.supports(c)))
                || k.frame.unsigned_abs() > 86400 * fps as u64 * 2
                || !keys.insert((k.kind, k.frame))
            {
                return Err("关键帧类型或时间无效".into());
            }
            for (i, v) in k.values.iter().enumerate() {
                let (min, max) = k.kind.range(i);
                if !v.is_finite() || *v < min || *v > max {
                    return Err("关键帧参数无效".into());
                }
                if let Some(curve) = k.curves[i] {
                    if i == 1 && k.kind != KeyframeKind::Position {
                        return Err("关键帧曲线参数无效".into());
                    }
                    curve.validate((max - min).max(1.0) * 2.0)?;
                    if (k.curve_value(i).clamp(min, max) - v).abs() > 1e-8 {
                        return Err("关键帧曲线参数无效".into());
                    }
                }
            }
        }
        Ok(())
    }
    /// 插入或替换此局部帧的同类型值；新节点保留越界曲线几何，主动替换分量时清除其几何差。
    /// 稳定排序确保保存和导航次序确定，不修改另一位置分量的端点或切线。
    fn put_keyframe(&mut self, kind: KeyframeKind, frame: i64, values: [f64; 2]) {
        let local = frame - self.start;
        if let Some(k) = self
            .keyframes
            .iter_mut()
            .find(|k| k.kind == kind && k.frame == local)
        {
            for (i, value) in values.iter().enumerate() {
                if *value != k.values[i]
                    && let Some(curve) = &mut k.curves[i]
                {
                    curve.value_offset = 0.0;
                }
            }
            k.values = values;
        } else {
            let curves = crate::keyframe_curve::split_for_key(self, kind, local, values);
            self.keyframes.push(Keyframe {
                frame: local,
                kind,
                values,
                curves,
            });
            self.keyframes.sort_by_key(|k| (k.frame, k.kind));
        }
    }
    /// 应用单个参数编辑：已有帧直接更新，同类已有动画且自动开启时仅在实际改变后新增。
    /// 自动关闭时在帧间按差值整体调整本类型并清除该分量旧越界端点差；未启用动画时写静态值。
    pub(crate) fn edit_animated(
        &mut self,
        kind: KeyframeKind,
        frame: i64,
        component: usize,
        value: f64,
        auto: bool,
    ) {
        let mut values = self.keyframe_values(kind, frame as f64);
        if values[component] == value {
            return;
        }
        let delta = value - values[component];
        values[component] = value;
        let animated = self.keyframes.iter().any(|k| k.kind == kind);
        let exact = self
            .keyframes
            .iter()
            .any(|k| k.kind == kind && k.frame == frame - self.start);
        if animated && !self.contains_frame(frame) {
            return;
        }
        if exact || (animated && auto) {
            self.put_keyframe(kind, frame, values);
        } else if animated {
            let (min, max) = kind.range(component);
            for k in self.keyframes.iter_mut().filter(|k| k.kind == kind) {
                k.values[component] = (k.values[component] + delta).clamp(min, max);
                if let Some(curve) = &mut k.curves[component] {
                    curve.value_offset = 0.0;
                }
            }
        } else {
            self.set_base(kind, values);
        }
    }
}
impl Timeline {
    /// 在选中未锁定且覆盖当前帧的素材上添加/删除一种关键帧；混合选择仅处理匹配素材。
    /// 存在当前帧时删除，删除最后一帧烘焙当前值；不存在时记录当前插值，调用方负责保存历史。
    /// 亮度选区另一端仍有动画时保留本端原始值并校正静态范围；两端都静态后烘焙有效有序范围，不改无关参数。
    pub fn toggle_keyframe(&mut self, ids: &BTreeSet<String>, kind: KeyframeKind, frame: i64) {
        for c in self.clips.iter_mut().filter(|c| {
            ids.contains(&c.id)
                && !c.locked
                && !self.tracks[c.track].locked
                && kind.accepts(c.kind)
                && !matches!(kind, KeyframeKind::Color(s, v) if c.color.stages.get(s).is_none_or(|stage| !stage.supports(v)))
                && c.contains_frame(frame)
        }) {
            let values = c.keyframe_values(kind, frame as f64);
            let qualifier = match kind {
                KeyframeKind::Color(stage, crate::color::Control::Qualifier(i @ (4 | 5))) => {
                    Some((stage, 9 - i, c.color_at(frame as f64).stages[stage].qualifier.values))
                }
                _ => None,
            };
            if let Some(i) = c
                .keyframes
                .iter()
                .position(|k| k.kind == kind && k.frame == frame - c.start)
            {
                c.keyframes.remove(i);
                if !c.keyframes.iter().any(|k| k.kind == kind) {
                    c.set_base(kind, values);
                    if let Some((stage, other, range)) = qualifier {
                        let animated = c.keyframes.iter().any(|k| {
                            k.kind == KeyframeKind::Color(stage, crate::color::Control::Qualifier(other))
                        });
                        let base = &mut c.color.stages[stage].qualifier.values;
                        if !animated {
                            base[4] = range[4];
                            base[5] = range[5];
                        } else if base[4] > base[5] {
                            base[other] = values[0];
                        }
                    }
                }
            } else {
                c.put_keyframe(kind, frame, values);
            }
        }
    }
    /// 删除指定素材在项目绝对帧上的全部关键帧类型；时间轴合并菱形使用此入口。
    /// 未知、锁定或范围外素材忽略；复用单类型删除规则，最后一项删除时保留其静态值。
    pub fn remove_keyframes(&mut self, id: &str, frame: i64) {
        let Some(clip) = self
            .clips
            .iter()
            .find(|c| c.id == id && c.contains_frame(frame))
        else {
            return;
        };
        let kinds: BTreeSet<_> = clip
            .keyframes
            .iter()
            .filter(|k| k.frame == frame - clip.start)
            .map(|k| k.kind)
            .collect();
        let ids = [id.to_owned()].into();
        for kind in kinds {
            self.toggle_keyframe(&ids, kind, frame);
        }
    }
    /// 校验并修改选中画面参数；当前项目帧与自动添加偏好决定动画编辑模式，锁定素材跳过。
    pub fn set_property_at(
        &mut self,
        ids: &BTreeSet<String>,
        property: Property,
        value: f64,
        frame: i64,
        auto: bool,
    ) -> Result<(), String> {
        let mut checked = Transform::default();
        checked.set(property, value)?;
        for c in self.clips.iter_mut().filter(|c| {
            ids.contains(&c.id)
                && c.kind != MediaKind::Audio
                && !c.locked
                && !self.tracks[c.track].locked
        }) {
            c.edit_animated(
                KeyframeKind::for_property(property),
                frame,
                usize::from(property == Property::Y),
                checked.get(property),
                auto,
            );
        }
        Ok(())
    }
    /// 校验并修改选中音频增益；仅音量已有关键帧时自动记录，拒绝无穷与越界且无部分写入。
    pub fn set_volume_at(
        &mut self,
        ids: &BTreeSet<String>,
        gain: f64,
        frame: i64,
        auto: bool,
    ) -> Result<(), String> {
        if !gain.is_finite() || !(0.0..=10.0).contains(&gain) {
            return Err("音量参数无效".into());
        }
        for c in self.clips.iter_mut().filter(|c| {
            ids.contains(&c.id)
                && c.kind == MediaKind::Audio
                && !c.locked
                && !self.tracks[c.track].locked
        }) {
            c.edit_animated(KeyframeKind::Volume, frame, 0, gain, auto);
        }
        Ok(())
    }
}
