//! 调色标量复用片段关键帧与贝塞尔求值；步骤身份、替换与动画编辑集中在领域层。
use crate::{
    color::{Control, Grade},
    keyframes::KeyframeKind as K,
    media::MediaKind,
    timeline::{Clip, Timeline},
};
use std::{borrow::Cow, collections::BTreeSet};

/// 解析稳定调色关键帧身份并严格核对段数、步骤和参数索引；未知类型返回原有错误。
pub(crate) fn parse_kind(name: &str) -> Result<K, String> {
    let parts: Vec<_> = name.split('/').collect();
    let parse = |i: usize| parts.get(i).and_then(|v| v.parse::<usize>().ok());
    let kind = (|| {
        if parts.first()? != &"color" {
            return None;
        }
        let stage = parse(1)?;
        let i = parse(3)?;
        let c = match (parts.get(2).copied()?, parts.len()) {
            ("basic", 4) => Control::Basic(i),
            ("hsl", 5) => Control::Hsl(i, parse(4)?),
            ("picked", 4) => Control::PickedHsl(i),
            ("wheel", 5) => Control::Wheel(i, parse(4)?),
            ("mask", 4) => Control::Mask(i),
            ("qualifier", 4) => Control::Qualifier(i),
            _ => return None,
        };
        (stage < 8 && c.valid()).then_some(K::Color(stage, c))
    })();
    kind.ok_or_else(|| "关键帧类型无效".into())
}

impl Clip {
    /// 求项目绝对帧的整套调色；静态片段直接借用，动画片段只覆盖有关键帧的参数。
    /// 每个标量沿用贝塞尔、端点保持和业务范围；亮度选区交叉时按相同顺序归一化。
    pub fn color_at(&self, frame: f64) -> Cow<'_, Grade> {
        let kinds: BTreeSet<_> = self
            .keyframes
            .iter()
            .filter_map(|k| match k.kind {
                K::Color(s, c) => Some((s, c)),
                _ => None,
            })
            .collect();
        if kinds.is_empty() {
            return Cow::Borrowed(&self.color);
        }
        let mut grade = self.color.clone();
        for (s, c) in kinds {
            if let Some(stage) = grade.stages.get_mut(s) {
                let _ = stage.set_value(c, self.keyframe_values(K::Color(s, c), frame)[0]);
            }
        }
        for stage in &mut grade.stages {
            if stage.qualifier.values[4] > stage.qualifier.values[5] {
                stage.qualifier.values.swap(4, 5);
            }
        }
        Cow::Owned(grade)
    }
}

impl Timeline {
    /// 在当前项目帧编辑调色标量，复用自动加帧或整体平移规则；开关和颜色曲线保持静态。
    /// 外部输入先完整验证；允许追加一步并保留旧动画，减少步骤须走删除接口，锁定和音频片段跳过。
    /// 与当前求值相同的输入不改后备值，避免无变化拖动产生额外保存或撤销历史。
    pub fn set_color_at(
        &mut self,
        ids: &BTreeSet<String>,
        grade: &Grade,
        frame: i64,
        auto: bool,
    ) -> Result<(), String> {
        grade.validate()?;
        if self.clips.iter().any(|c| {
            ids.contains(&c.id)
                && c.kind != MediaKind::Audio
                && !c.locked
                && !self.tracks[c.track].locked
                && (grade.stages.len() < c.color.stages.len()
                    || grade.stages.len() > c.color.stages.len() + 1)
        }) {
            return Err("调色步骤数量已改变".into());
        }
        for c in self.clips.iter_mut().filter(|c| {
            ids.contains(&c.id)
                && c.kind != MediaKind::Audio
                && !c.locked
                && !self.tracks[c.track].locked
        }) {
            if grade.stages.len() > c.color.stages.len() {
                c.color.stages.push(grade.stages.last().unwrap().clone());
            }
            let current = c.color_at(frame as f64).into_owned();
            if current == *grade {
                continue;
            }
            for (s, stage) in grade.stages.iter().enumerate() {
                if stage.picked.is_none() {
                    c.keyframes.retain(
                        |k| !matches!(k.kind, K::Color(i, Control::PickedHsl(_)) if i == s),
                    );
                } else if c.color.stages[s].picked.is_none() {
                    c.color.stages[s].picked = stage.picked.clone();
                }
                for control in Control::all().filter(|v| current.stages[s].get(*v) != stage.get(*v))
                {
                    c.edit_animated(K::Color(s, control), frame, 0, stage.get(control), auto);
                }
                c.color.stages[s] = stage.clone();
            }
            c.color.enabled = grade.enabled;
        }
        Ok(())
    }

    /// 删除单步或全部调色动画；删除单步时同步后续步骤身份，重置时只清除目标步骤。
    /// 不触碰其他关键帧类型、音频或锁定片段；返回后仍保留至少一个步骤。
    pub fn reset_color(&mut self, id: &str, stage: Option<usize>, remove: bool) {
        let Some(c) = self.clips.iter_mut().find(|c| {
            c.id == id && c.kind != MediaKind::Audio && !c.locked && !self.tracks[c.track].locked
        }) else {
            return;
        };
        if let Some(s) = stage {
            if s >= c.color.stages.len() || (remove && c.color.stages.len() == 1) {
                return;
            }
            c.keyframes
                .retain(|k| !matches!(k.kind, K::Color(i, _) if i == s));
            if remove {
                c.color.stages.remove(s);
                for k in &mut c.keyframes {
                    if let K::Color(i, control) = k.kind
                        && i > s
                    {
                        k.kind = K::Color(i - 1, control);
                    }
                }
            } else {
                c.color.stages[s] = Default::default();
            }
        } else {
            c.color = Grade::default();
            c.keyframes.retain(|k| !matches!(k.kind, K::Color(..)));
        }
    }
}
