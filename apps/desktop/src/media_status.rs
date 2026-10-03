//! 媒体离线监测与统一占位：保留引用，检测源文件消失和原路径恢复。

use crate::{media::Media, view::Desktop};
use dinuosi_icons::Icon;
use gpui::{Context, Div, Window, div, prelude::*, px, rgb, svg};
use std::time::Duration;

/// 检查源文件是否仍可访问；离线素材回到原路径时重新探测，其他文件不重复解码。
/// 保留失联文件的路径和已知信息，不扫描其他目录或把同名文件自动重连。
pub(crate) fn check_sources(items: Vec<Media>) -> Vec<Media> {
    items
        .into_iter()
        .map(|item| {
            let checked = dinuosi_core::media::check_sources(vec![item.source.clone()]).remove(0);
            if item.offline != checked.offline {
                Media::from_source(checked)
            } else {
                item
            }
        })
        .collect()
}

/// 构造暗红色离线占位；缩略图与预览复用同一图标和文字，仅按空间调整大小。
pub(crate) fn offline_view(compact: bool) -> Div {
    div()
        .size_full()
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .gap(px(if compact { 3.0 } else { 10.0 }))
        .bg(rgb(0x512e32))
        .text_color(rgb(0xe0c7c9))
        .text_size(px(if compact { 11.0 } else { 14.0 }))
        .child(
            svg()
                .path(Icon::Warning.path())
                .size(px(if compact { 22.0 } else { 48.0 }))
                .text_color(rgb(0xe0c7c9)),
        )
        .child("媒体离线")
}

impl Desktop {
    /// 每秒在后台检查引用可用性；选中素材失联时立即停止声音，并同步更新两处占位。
    /// 会话销毁时任务取消；原文件恢复后重新探测并恢复预览，不更改项目保存的引用。
    pub(crate) fn watch_media(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.current.is_none() {
            return;
        }
        self.session.monitor = Some(cx.spawn_in(window, async move |this, cx| {
            loop {
                cx.background_executor().timer(Duration::from_secs(1)).await;
                let Ok(items) = this.update_in(cx, |this, _, _| this.session.media.clone()) else {
                    break;
                };
                let checked = cx
                    .background_spawn(async move { check_sources(items) })
                    .await;
                let result = this.update_in(cx, |this, window, cx| {
                    this.apply_media_status(checked, window, cx);
                });
                if result.is_err() {
                    break;
                }
            }
        }));
    }

    /// 将后台可用性结果按路径应用到当前列表；只刷新变化项，已选择素材同时更新预览。
    /// 状态刷新保留当前别名，避免旧监测结果覆盖刚保存的改名；重新选择会停止旧播放器。
    pub(crate) fn apply_media_status(
        &mut self,
        checked: Vec<Media>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let mut selected = None;
        let mut changed = false;
        for mut item in checked {
            if let Some((index, current)) = self
                .session
                .media
                .iter_mut()
                .enumerate()
                .find(|(_, m)| m.path == item.path)
                && current.offline != item.offline
            {
                item.source.name = current.name.clone();
                *current = item;
                changed = true;
                if self.session.selected == Some(index) {
                    selected = Some(index);
                }
            }
        }
        if let Some(index) = selected {
            self.select_media(index, window, cx);
        }
        if changed {
            cx.notify();
        }
    }
}
