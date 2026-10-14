//! 曲线编辑的单素材事务规则；身份、锁定、碰撞与参数校验集中于 Core。
use crate::{
    keyframe_curve::{Easing, KeyframeCurve},
    keyframes::KeyframeKind,
    timeline::{Clip, Timeline},
};

impl Clip {
    /// 返回某个局部节点的相邻同类节点，裁剪范围外的锚点也参与曲线求值。
    pub fn curve_neighbor(
        &self,
        kind: KeyframeKind,
        frame: i64,
        incoming: bool,
    ) -> Option<&crate::keyframes::Keyframe> {
        self.keyframes
            .iter()
            .filter(|k| {
                k.kind == kind
                    && if incoming {
                        k.frame < frame
                    } else {
                        k.frame > frame
                    }
            })
            .min_by_key(|k| k.frame.abs_diff(frame))
    }

    /// 读取相对原始贝塞尔端点的切线，缺省按相邻未夹紧端点生成线性切线；无相邻节点返回 None。
    pub fn curve_handle(
        &self,
        kind: KeyframeKind,
        component: usize,
        frame: i64,
        incoming: bool,
    ) -> Option<[f64; 2]> {
        if component > usize::from(kind == KeyframeKind::Position) {
            return None;
        }
        let key = self
            .keyframes
            .iter()
            .find(|k| k.kind == kind && k.frame == frame)?;
        let neighbor = self.curve_neighbor(kind, frame, incoming)?;
        let curve = key.curves[component].unwrap_or_default();
        Some(
            if incoming {
                curve.incoming
            } else {
                curve.outgoing
            }
            .unwrap_or([
                1.0 / 3.0,
                (neighbor.curve_value(component) - key.curve_value(component)) / 3.0,
            ]),
        )
    }

    /// 按当前左右帧间距同步联动切线；优先保留指定侧，超出纵向限值时同比缩短另一侧以保持斜率。
    /// 缺少任一相邻节点或主切线时原样返回，供节点改时与手柄编辑共用，不修改片段。
    fn synced_curve(
        &self,
        kind: KeyframeKind,
        frame: i64,
        component: usize,
        mut curve: KeyframeCurve,
        incoming: bool,
    ) -> KeyframeCurve {
        if !curve.linked {
            return curve;
        }
        let primary = if incoming {
            curve.incoming
        } else {
            curve.outgoing
        };
        let incoming = if primary.is_some() {
            incoming
        } else {
            !incoming
        };
        let Some(handle) = (if incoming {
            curve.incoming
        } else {
            curve.outgoing
        }) else {
            return curve;
        };
        let Some(neighbor) = self.curve_neighbor(kind, frame, incoming) else {
            return curve;
        };
        let Some(opposite) = self.curve_neighbor(kind, frame, !incoming) else {
            return curve;
        };
        let gap = neighbor.frame.abs_diff(frame) as f64;
        let other_gap = opposite.frame.abs_diff(frame) as f64;
        let other = if incoming {
            &mut curve.outgoing
        } else {
            &mut curve.incoming
        };
        let length = other.map_or(1.0 / 3.0, |h| h[0]);
        let (min, max) = kind.range(component);
        let limit = (max - min).max(1.0) * 2.0;
        *other = Some(if handle[0] == 0.0 {
            [0.0, -handle[1]]
        } else if handle[1] == 0.0 {
            [length, 0.0]
        } else {
            let factor = length / handle[0] * (other_gap / gap);
            let max_factor = limit / handle[1].abs();
            if !factor.is_finite() {
                [handle[0] * (gap / other_gap), -handle[1]]
            } else if factor > max_factor {
                [length * (max_factor / factor), -handle[1].signum() * limit]
            } else {
                [length, -handle[1] * factor]
            }
        });
        curve
    }
}

impl Timeline {
    /// 查找可编辑节点并检查素材/轨道锁；错误不修改数据，范围外锚点只读。
    fn curve_target(
        &self,
        id: &str,
        kind: KeyframeKind,
        component: usize,
        frame: i64,
    ) -> Result<(usize, usize), String> {
        let ci = self
            .clips
            .iter()
            .position(|c| c.id == id)
            .ok_or("关键帧不存在")?;
        let c = &self.clips[ci];
        if c.locked
            || self.tracks[c.track].locked
            || !kind.accepts(c.kind)
            || component > usize::from(kind == KeyframeKind::Position)
            || frame < 0
            || frame >= c.duration
        {
            return Err("关键帧不可编辑".into());
        }
        let ki = c
            .keyframes
            .iter()
            .position(|k| k.kind == kind && k.frame == frame)
            .ok_or("关键帧不存在")?;
        Ok((ci, ki))
    }

    /// 将单个节点移动到片段局部帧并修改一个分量；同类碰撞拒绝，位置另一分量保留。
    /// 改时后按新间距同步受影响联动切线（含相邻节点及位置两分量），主动改值清除该分量的越界端点差。
    /// 参数超界、非有限或素材锁定时原子失败；调用方记录历史和持久化。
    pub fn edit_curve_node(
        &mut self,
        id: &str,
        kind: KeyframeKind,
        component: usize,
        frame: i64,
        target: i64,
        value: f64,
    ) -> Result<(), String> {
        let (ci, ki) = self.curve_target(id, kind, component, frame)?;
        let c = &mut self.clips[ci];
        let (min, max) = kind.range(component);
        if target < 0 || target >= c.duration || !value.is_finite() || !(min..=max).contains(&value)
        {
            return Err("关键帧参数无效".into());
        }
        if c.keyframes
            .iter()
            .enumerate()
            .any(|(i, k)| i != ki && k.kind == kind && k.frame == target)
        {
            return Err("该时间已有同类关键帧".into());
        }
        let mut affected = vec![target];
        if frame != target {
            for incoming in [true, false] {
                if let Some(neighbor) = c.curve_neighbor(kind, frame, incoming) {
                    affected.push(neighbor.frame);
                }
            }
        }
        c.keyframes[ki].frame = target;
        if c.keyframes[ki].values[component] != value
            && let Some(curve) = &mut c.keyframes[ki].curves[component]
        {
            curve.value_offset = 0.0;
        }
        c.keyframes[ki].values[component] = value;
        c.keyframes.sort_by_key(|k| (k.frame, k.kind));
        if frame != target {
            for incoming in [true, false] {
                if let Some(neighbor) = c.curve_neighbor(kind, target, incoming) {
                    affected.push(neighbor.frame);
                }
            }
            for index in 0..c.keyframes.len() {
                if c.keyframes[index].kind != kind || !affected.contains(&c.keyframes[index].frame)
                {
                    continue;
                }
                for component in 0..=usize::from(kind == KeyframeKind::Position) {
                    if let Some(curve) = c.keyframes[index].curves[component].filter(|c| c.linked) {
                        let frame = c.keyframes[index].frame;
                        c.keyframes[index].curves[component] =
                            Some(c.synced_curve(kind, frame, component, curve, false));
                    }
                }
            }
        }
        Ok(())
    }

    /// 对选中节点右侧区间应用常用缓动；末节点使用左侧区间，只修改本分量的相关切线。
    /// 单节点没有区间时无副作用，线性清除该区间两端切线，预设保留另一侧及另一属性。
    pub fn ease_curve(
        &mut self,
        id: &str,
        kind: KeyframeKind,
        component: usize,
        frame: i64,
        easing: Easing,
    ) -> Result<(), String> {
        let (ci, ki) = self.curve_target(id, kind, component, frame)?;
        let c = &self.clips[ci];
        let right = c.curve_neighbor(kind, frame, false);
        let Some(other) = right.or_else(|| c.curve_neighbor(kind, frame, true)) else {
            return Ok(());
        };
        let oi = c
            .keyframes
            .iter()
            .position(|k| k.kind == kind && k.frame == other.frame)
            .unwrap();
        let (ai, bi) = if right.is_some() { (ki, oi) } else { (oi, ki) };
        let handles = match easing {
            Easing::Linear => [None, None],
            Easing::EaseIn => [Some([0.42, 0.0]), Some([0.0, 0.0])],
            Easing::EaseOut => [Some([0.0, 0.0]), Some([0.42, 0.0])],
            Easing::EaseInOut => [Some([0.42, 0.0]), Some([0.42, 0.0])],
        };
        let keys = &mut self.clips[ci].keyframes;
        for (index, input, handle) in [(ai, false, handles[0]), (bi, true, handles[1])] {
            let curve = keys[index].curves[component].get_or_insert_with(KeyframeCurve::default);
            if input {
                curve.incoming = handle;
            } else {
                curve.outgoing = handle;
            }
            curve.linked = false;
            if curve.incoming.is_none() && curve.outgoing.is_none() && curve.value_offset == 0.0 {
                keys[index].curves[component] = None;
            }
        }
        Ok(())
    }

    /// 启用自由/联动切线并显式建立可拖动手柄；端点重合的零手柄展开为水平切线。
    /// 切换联动时沿用右侧斜率使两侧切线连续，已有非零切线保持原长度。
    pub fn link_curve_handles(
        &mut self,
        id: &str,
        kind: KeyframeKind,
        component: usize,
        frame: i64,
        linked: bool,
    ) -> Result<(), String> {
        let (ci, ki) = self.curve_target(id, kind, component, frame)?;
        let c = &self.clips[ci];
        let expand = |handle: Option<[f64; 2]>| {
            handle.map(|h| if h == [0.0, 0.0] { [1.0 / 3.0, 0.0] } else { h })
        };
        let curve = KeyframeCurve {
            incoming: expand(c.curve_handle(kind, component, frame, true)),
            outgoing: expand(c.curve_handle(kind, component, frame, false)),
            linked,
            value_offset: c.keyframes[ki].curves[component].map_or(0.0, |c| c.value_offset),
        };
        self.clips[ci].keyframes[ki].curves[component] = Some(curve);
        if linked {
            if let Some(handle) = curve.outgoing {
                self.edit_curve_handle(id, kind, component, frame, false, handle)?;
            } else if let Some(handle) = curve.incoming {
                self.edit_curve_handle(id, kind, component, frame, true, handle)?;
            }
        }
        Ok(())
    }

    /// 修改一侧切线；联动模式按真实帧间隔反向同步另一侧斜率，零横长退化为零横长手柄。
    /// 切线长度限相邻时间范围，参数差值有有限上界；求值允许过冲并在共享入口夹紧业务范围。
    pub fn edit_curve_handle(
        &mut self,
        id: &str,
        kind: KeyframeKind,
        component: usize,
        frame: i64,
        incoming: bool,
        handle: [f64; 2],
    ) -> Result<(), String> {
        let (ci, ki) = self.curve_target(id, kind, component, frame)?;
        let (min, max) = kind.range(component);
        let limit = (max - min).max(1.0) * 2.0;
        KeyframeCurve {
            incoming: Some(handle),
            ..Default::default()
        }
        .validate(limit)?;
        let c = &self.clips[ci];
        c.curve_neighbor(kind, frame, incoming)
            .ok_or("关键帧不存在")?;
        let mut curve = c.keyframes[ki].curves[component].unwrap_or(KeyframeCurve {
            incoming: c.curve_handle(kind, component, frame, true),
            outgoing: c.curve_handle(kind, component, frame, false),
            linked: false,
            value_offset: 0.0,
        });
        if incoming {
            curve.incoming = Some(handle);
        } else {
            curve.outgoing = Some(handle);
        }
        let curve = c.synced_curve(kind, frame, component, curve, incoming);
        self.clips[ci].keyframes[ki].curves[component] = Some(curve);
        Ok(())
    }
}
