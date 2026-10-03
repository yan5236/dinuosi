//! 引用式素材库：分类和文件名搜索、外部文件拖入、项目引用持久化。

use crate::{
    input::TextInput,
    media::{self, MediaKind},
    ui::{LINE, PANEL, TEXT},
    view::Desktop,
};
use dinuosi_icons::Icon;
use gpui::{
    Context, Div, ExternalPaths, ObjectFit, PathPromptOptions, Window, div, img, prelude::*, px,
    rgb, svg,
};
use std::path::PathBuf;

#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub enum Filter {
    #[default]
    All,
    Video,
    Audio,
    Image,
    Motion,
    Effects,
}

impl Filter {
    /// 返回草图规定的分类顺序及专用 SVG，不增加素材计数或额外状态文字。
    pub fn items() -> [(Self, &'static str, Icon); 6] {
        [
            (Self::All, "全部", Icon::AllMedia),
            (Self::Video, "视频", Icon::Video),
            (Self::Audio, "音频", Icon::Audio),
            (Self::Image, "图片", Icon::Image),
            (Self::Motion, "动效", Icon::Motion),
            (Self::Effects, "特效", Icon::Effects),
        ]
    }
    /// 分类仅筛选已导入素材；动效/特效当前没有对应素材，列表保持空白。
    pub fn matches(self, kind: MediaKind) -> bool {
        self == Self::All
            || matches!(
                (self, kind),
                (Self::Video, MediaKind::Video)
                    | (Self::Audio, MediaKind::Audio)
                    | (Self::Image, MediaKind::Image)
            )
    }
}

impl Desktop {
    /// 创建左侧分类与右侧缩略图列表，接收资源管理器的多文件拖放；点击素材切换实时预览。
    pub(crate) fn library(&self, width: f32, cx: &mut Context<Self>) -> Div {
        let query = self.session.search.read(cx).text.to_lowercase();
        let tile_width = ((width - 106.0) / 3.0 - 12.0).max(72.0);
        div()
            .debug_selector(|| "素材库".into())
            .flex_none()
            .w(px(width))
            .h_full()
            .flex()
            .bg(rgb(PANEL))
            .child(
                div()
                    .w(px(84.0))
                    .flex_none()
                    .h_full()
                    .border_r_1()
                    .border_color(rgb(LINE))
                    .pt(px(8.0))
                    .children(Filter::items().into_iter().map(|(filter, label, icon)| {
                        div()
                            .id(gpui::SharedString::from(format!("filter-{}", icon.name())))
                            .debug_selector(move || format!("filter-{}", icon.name()))
                            .h(px(45.0))
                            .flex()
                            .items_center()
                            .gap(px(8.0))
                            .px(px(10.0))
                            .text_size(px(13.0))
                            .cursor_pointer()
                            .when(self.session.filter == filter, |s| s.bg(rgb(LINE)))
                            .hover(|s| s.bg(rgb(LINE)))
                            .child(svg().path(icon.path()).size(px(18.0)).text_color(rgb(TEXT)))
                            .child(label)
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.session.filter = filter;
                                cx.notify();
                            }))
                    })),
            )
            .child(
                div()
                    .id("media-drop-zone")
                    .debug_selector(|| "media-drop-zone".into())
                    .flex_1()
                    .min_w_0()
                    .h_full()
                    .flex()
                    .flex_col()
                    .p(px(10.0))
                    .on_drop(cx.listener(|this, paths: &ExternalPaths, window, cx| {
                        this.import_media(paths.paths().to_vec(), window, cx)
                    }))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(6.0))
                            .mb(px(10.0))
                            .border_1()
                            .border_color(rgb(LINE))
                            .rounded(px(6.0))
                            .pl(px(12.0))
                            .child(
                                svg()
                                    .path(Icon::Search.path())
                                    .size(px(16.0))
                                    .text_color(rgb(TEXT)),
                            )
                            .child(div().flex_1().min_w_0().child(self.session.search.clone())),
                    )
                    .child(
                        div()
                            .id("media-list")
                            .flex_1()
                            .min_h_0()
                            .overflow_y_scroll()
                            .child(
                                div().flex().flex_wrap().gap(px(10.0)).children(
                                    self.session
                                        .media
                                        .iter()
                                        .enumerate()
                                        .filter(|(_, media)| {
                                            self.session.filter.matches(media.kind)
                                                && media.name.to_lowercase().contains(&query)
                                        })
                                        .map(|(index, media)| {
                                            div()
                                                .id(("media", index))
                                                .debug_selector(move || format!("media-{index}"))
                                                .w(px(tile_width))
                                                .flex_none()
                                                .cursor_pointer()
                                                .on_mouse_down(gpui::MouseButton::Right, cx.listener(
                                                    move |this, event: &gpui::MouseDownEvent, _, cx| {
                                                        if !this.busy {
                                                            this.session.media_menu = this.session.media.get(index)
                                                                .map(|m| (m.path.clone(), event.position));
                                                            cx.notify();
                                                        }
                                                        cx.stop_propagation();
                                                    },
                                                ))
                                                .on_click(cx.listener(
                                                    move |this, _, window, cx| {
                                                        this.select_media(index, window, cx)
                                                    },
                                                ))
                                                .child(
                                                    div()
                                                        .w_full()
                                                        .h(px(tile_width * 0.62))
                                                        .relative()
                                                        .overflow_hidden()
                                                        .rounded(px(3.0))
                                                        .bg(rgb(0x242426))
                                                        .border_1()
                                                        .border_color(rgb(
                                                            if self.session.selected == Some(index)
                                                            {
                                                                TEXT
                                                            } else {
                                                                LINE
                                                            },
                                                        ))
                                                        .when_some(
                                                            if media.offline {
                                                                None
                                                            } else {
                                                                media.thumbnail.clone()
                                                            },
                                                            |s, image| {
                                                                s.child(
                                                                    img(image)
                                                                        .size_full()
                                                                        .object_fit(
                                                                            ObjectFit::Contain,
                                                                        ),
                                                                )
                                                            },
                                                        )
                                                        .when(media.offline, |s| {
                                                            s.child(
                                                                crate::media_status::offline_view(
                                                                    true,
                                                                ),
                                                            )
                                                        })
                                                        .when(
                                                            media.kind == MediaKind::Audio
                                                                && !media.offline,
                                                            |s| {
                                                                s.flex()
                                                                    .items_center()
                                                                    .justify_center()
                                                                    .child(
                                                                        svg()
                                                                            .path(
                                                                                Icon::Audio.path(),
                                                                            )
                                                                            .size(px(30.0))
                                                                            .text_color(rgb(TEXT)),
                                                                    )
                                                            },
                                                        )
                                                        .when(
                                                            media.kind != MediaKind::Image
                                                                && !media.offline,
                                                            |s| {
                                                                s.child(
                                                                    div()
                                                                        .absolute()
                                                                        .bottom(px(2.0))
                                                                        .right(px(3.0))
                                                                        .px(px(3.0))
                                                                        .bg(rgb(0x242426))
                                                                        .text_size(px(10.0))
                                                                        .child(format!(
                                                                            "{:02}:{:02}",
                                                                            media.duration as u64
                                                                                / 60,
                                                                            media.duration as u64
                                                                                % 60
                                                                        )),
                                                                )
                                                            },
                                                        ),
                                                )
                                                .child(
                                                    div()
                                                        .mt(px(4.0))
                                                        .text_size(px(11.0))
                                                        .text_ellipsis()
                                                        .child(media.name.clone()),
                                                )
                                        }),
                                ),
                            ),
                    ),
            )
    }

    /// 打开原生多选文件对话框；取消不改变项目，所有选择和拖入复用同一导入入口。
    pub(crate) fn choose_media(
        &mut self,
        _: &gpui::ClickEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.busy || self.session.importing {
            return;
        }
        let receiver = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: true,
            prompt: Some("导入素材".into()),
        });
        cx.spawn_in(window, async move |this, cx| {
            let result = receiver.await;
            let _ = this.update_in(cx, |this, window, cx| match result {
                Ok(Ok(Some(paths))) => this.import_media(paths, window, cx),
                Ok(Ok(None)) => {}
                error => {
                    this.notice = Some(format!("选择素材失败：{error:?}"));
                    cx.notify();
                }
            });
        })
        .detach();
    }

    /// 后台探测多份素材、去重并持久化源文件路径；成功素材可导入，失败文件逐一反馈。
    /// 保存失败时不向界面添加未保存的引用；不改写、复制或删除源文件。
    pub(crate) fn import_media(
        &mut self,
        paths: Vec<PathBuf>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(entry) = self.current.clone() else {
            return;
        };
        if self.busy || self.session.importing {
            return;
        }
        self.session.importing = true;
        self.busy = true;
        self.notice = None;
        let store = self.store.clone();
        cx.spawn_in(window, async move |this, cx| {
            let result = cx
                .background_spawn(async move {
                    let result = store.import_media(&entry, &paths)?;
                    let imported = result
                        .media
                        .into_iter()
                        .map(media::Media::from_source)
                        .collect::<Vec<_>>();
                    let errors = result.errors;
                    Ok::<_, String>((imported, errors))
                })
                .await;
            let _ = this.update_in(cx, |this, _, cx| {
                this.session.importing = false;
                this.busy = false;
                match result {
                    Ok((imported, errors)) => {
                        for media in imported {
                            if !this.session.media.iter().any(|m| m.path == media.path) {
                                this.session.media.push(media);
                            }
                        }
                        if !errors.is_empty() {
                            this.notice = Some(errors.join("\n"));
                        }
                    }
                    Err(error) => this.notice = Some(error),
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }
}

/// 构造可输入的文件名搜索实体，保留现有中文输入能力；文字变化只重绘素材列表。
pub fn search(cx: &mut Context<Desktop>) -> gpui::Entity<TextInput> {
    let search = cx.new(|cx| {
        let mut input = TextInput::new(String::new(), cx);
        input.placeholder = "搜索文件名";
        input.control_id = "media-search";
        input.show_border = false;
        input
    });
    cx.observe(&search, |_, _, cx| cx.notify()).detach();
    search
}
