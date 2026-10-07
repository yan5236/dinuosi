//! 素材右键操作：沿用项目菜单与名称表单，只修改引用及显示名称。

use crate::{
    dialogs::DialogKind,
    ui::{LINE, TEXT},
    view::Desktop,
};
use dinuosi_icons::Icon;
use gpui::{Context, Div, Window, div, prelude::*, px, rgb, svg};
use std::path::PathBuf;

impl Desktop {
    /// 在右键位置绘制改名和移除菜单，并限制在当前窗口内；通过源路径识别素材。
    /// 移除只调用核心引用保存接口，不接触源文件。
    pub(crate) fn media_actions(&self, window: &Window, cx: &mut Context<Self>) -> Div {
        let (path, position) = self.session.media_menu.as_ref().expect("菜单存在时绘制");
        let viewport = window.viewport_size();
        div()
            .absolute()
            .occlude()
            .left(px(
                f32::from(position.x).clamp(0.0, (f32::from(viewport.width) - 164.0).max(0.0))
            ))
            .top(px(
                f32::from(position.y).clamp(0.0, (f32::from(viewport.height) - 100.0).max(0.0))
            ))
            .w(px(164.0))
            .p(px(6.0))
            .bg(rgb(0x303034))
            .border_1()
            .border_color(rgb(LINE))
            .rounded(px(6.0))
            .children(
                [
                    (
                        true,
                        "media-rename",
                        crate::i18n::t("library", "Rename", cx),
                        Icon::Edit,
                    ),
                    (
                        false,
                        "media-remove",
                        crate::i18n::t("library", "Remove media", cx),
                        Icon::Trash,
                    ),
                ]
                .into_iter()
                .map(|(rename, id, label, icon)| {
                    let path = path.clone();
                    div()
                        .id(id)
                        .debug_selector(move || id.into())
                        .flex()
                        .items_center()
                        .gap(px(10.0))
                        .p(px(10.0))
                        .rounded(px(4.0))
                        .cursor_pointer()
                        .hover(|s| s.bg(rgb(LINE)))
                        .child(svg().path(icon.path()).size(px(16.0)).text_color(rgb(TEXT)))
                        .child(label)
                        .on_click(cx.listener(move |this, _, window, cx| {
                            if rename {
                                if let Some(media) =
                                    this.session.media.iter().find(|m| m.path == path)
                                {
                                    this.show_dialog(
                                        DialogKind::RenameMedia {
                                            path: path.clone(),
                                            name: media.name.clone(),
                                        },
                                        window,
                                        cx,
                                    );
                                }
                            } else {
                                this.change_media_reference(path.clone(), None, window, cx);
                            }
                        }))
                }),
            )
    }

    /// 后台保存素材别名（Some）或移除引用（None）；成功后才更新列表并修正选择下标。
    /// 失败保留表单和预览；移除当前素材会停止播放、清空选区并回收图像，不删除磁盘文件。
    pub(crate) fn change_media_reference(
        &mut self,
        path: PathBuf,
        name: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.busy {
            return;
        }
        let Some(entry) = self.current.clone() else {
            return;
        };
        self.busy = true;
        self.notice = None;
        self.session.media_menu = None;
        let store = self.store.clone();
        cx.spawn_in(window, async move |this, cx| {
            let saved_path = path.clone();
            let saved_name = name.clone();
            let result = cx
                .background_spawn(async move {
                    if let Some(name) = saved_name {
                        store.rename_media(&entry, &saved_path, &name)
                    } else {
                        store.remove_media(&entry, &saved_path)
                    }
                })
                .await;
            let _ = this.update_in(cx, |this, window, cx| {
                this.busy = false;
                match result {
                    Ok(()) => {
                        if let Some(index) = this.session.media.iter().position(|m| m.path == path)
                        {
                            if let Some(name) = name {
                                this.session.media[index].source.name = name.trim().to_owned();
                            } else {
                                if this.session.selected == Some(index) {
                                    this.stop_preview();
                                    this.session.selected = None;
                                    this.session.position = 0.0;
                                    this.session.trim_start = 0.0;
                                    this.session.trim_end = 0.0;
                                    this.session.drag = None;
                                    this.session.resume_after_drag = false;
                                    this.session.bounds.remove(&crate::sliders::Pointer::Seek);
                                    this.session
                                        .retired_frames
                                        .extend(this.session.frame.take());
                                    this.session.retired_frames.append(&mut this.session.strip);
                                } else if let Some(selected) = &mut this.session.selected
                                    && *selected > index
                                {
                                    *selected -= 1;
                                }
                                let removed = this.session.media.remove(index);
                                this.session.retired_frames.extend(removed.thumbnail);
                            }
                        }
                        this.dialog = None;
                        this.focus_handle.focus(window);
                    }
                    Err(error) => {
                        if let Some(dialog) = &mut this.dialog {
                            dialog.error = Some(error);
                        } else {
                            this.notice = Some(error);
                        }
                    }
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }
}
