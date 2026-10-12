//! 项目标记点与磁吸目标；所有时间使用整数帧，和 GPUI/鼠标坐标无关。
use crate::timeline::Timeline;
use std::collections::BTreeSet;
use uuid::Uuid;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Marker {
    pub id: String,
    pub frame: i64,
    pub name: String,
    /// 无透明度的 RGB 颜色，范围 0x000000..=0xffffff。
    pub color: u32,
}

impl Timeline {
    /// 切换选中图片、视频的镜像状态；horizontal 为 true 时左右翻转，否则上下翻转。
    /// 忽略未知身份、音频、锁定片段及锁轨，不改源文件和关键帧；保存与历史由宿主提交。
    pub fn mirror_clips(&mut self, ids: &BTreeSet<String>, horizontal: bool) {
        for c in self.clips.iter_mut().filter(|c| {
            ids.contains(&c.id)
                && c.kind != crate::media::MediaKind::Audio
                && !c.locked
                && !self.tracks[c.track].locked
        }) {
            let mirror = if horizontal {
                &mut c.transform.mirror_horizontal
            } else {
                &mut c.transform.mirror_vertical
            };
            *mirror = !*mirror;
        }
    }

    /// 校验外部标记身份、时间、名称和 RGB 范围；重复身份或损坏字段返回错误，不修改项目。
    pub(crate) fn validate_markers(&self) -> Result<(), String> {
        let mut ids = BTreeSet::new();
        for m in &self.markers {
            if !ids.insert(&m.id)
                || Uuid::parse_str(&m.id).is_err()
                || !(0..=86400 * self.fps as i64).contains(&m.frame)
                || m.name.trim().is_empty()
                || m.name.chars().count() > 64
                || m.color > 0xffffff
            {
                return Err("标记点格式无效".into());
            }
        }
        Ok(())
    }

    /// 在给定项目帧新增具名标记；同帧已有标记时返回原身份，避免连续点击重复创建。
    /// 非法时间或名称返回错误且不修改状态；成功后的保存和历史由宿主提交。
    pub fn add_marker(&mut self, frame: i64, name: String) -> Result<String, String> {
        if let Some(m) = self.markers.iter().find(|m| m.frame == frame) {
            return Ok(m.id.clone());
        }
        let marker = Marker {
            id: Uuid::new_v4().to_string(),
            frame,
            name: name.trim().into(),
            color: 0xe3b85b,
        };
        self.markers.push(marker);
        if let Err(e) = self.validate_markers() {
            self.markers.pop();
            return Err(e);
        }
        Ok(self.markers.last().unwrap().id.clone())
    }

    /// 修改指定标记名称或颜色；未知身份、空白/超长名称及非法 RGB 返回错误，原标记保持不变。
    pub fn edit_marker(
        &mut self,
        id: &str,
        name: Option<String>,
        color: Option<u32>,
    ) -> Result<(), String> {
        let m = self
            .markers
            .iter_mut()
            .find(|m| m.id == id)
            .ok_or("标记点不存在")?;
        let name = name.unwrap_or_else(|| m.name.clone()).trim().to_owned();
        let color = color.unwrap_or(m.color);
        if name.is_empty() || name.chars().count() > 64 || color > 0xffffff {
            return Err("标记点名称应为 1–64 个字符，颜色必须为有效 RGB".into());
        }
        m.name = name;
        m.color = color;
        Ok(())
    }

    /// 删除指定身份标记；未知身份无操作，不移动片段和播放头，宿主负责保存/撤销。
    pub fn delete_marker(&mut self, id: &str) {
        self.markers.retain(|m| m.id != id);
    }

    /// 统一锁定或解锁选择集合，锁轨上的片段保持原状态；不清除选择，不修改素材和动画参数。
    pub fn lock_clips(&mut self, ids: &BTreeSet<String>, locked: bool) {
        for c in self
            .clips
            .iter_mut()
            .filter(|c| ids.contains(&c.id) && !self.tracks[c.track].locked)
        {
            c.locked = locked;
        }
    }

    /// 返回整组边缘到最近磁吸目标的帧修正量；edges 是拟放置首尾，exclude 排除移动组自身。
    /// 目标包含零点、播放头、全部标记及跨轨片段首尾；容差由宿主像素换算，距离相同时优先较早目标。
    /// 关闭磁吸或没有容差内目标时返回零；不改变模型，也不把任一边缘吸附到负时间。
    pub fn snap_offset(
        &self,
        edges: &[i64],
        exclude: &BTreeSet<String>,
        playhead: i64,
        tolerance: i64,
    ) -> i64 {
        if !self.snapping {
            return 0;
        }
        let minimum = edges.iter().copied().min().unwrap_or(0);
        let targets = [0, playhead]
            .into_iter()
            .chain(self.markers.iter().map(|m| m.frame))
            .chain(
                self.clips
                    .iter()
                    .filter(|c| !exclude.contains(&c.id))
                    .flat_map(|c| [c.start, c.start + c.duration]),
            );
        targets
            .flat_map(|target| edges.iter().map(move |edge| (target - edge, target)))
            .filter(|(delta, _)| delta.abs() <= tolerance.max(0) && minimum + delta >= 0)
            .min_by_key(|(delta, target)| (delta.abs(), *target))
            .map_or(0, |(delta, _)| delta)
    }
}
