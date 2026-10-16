//! 本地 SRT 交换；原生文件对话框只选路径，解析/写入在后台，导入提交为一次可撤销事务。
use crate::{timeline_state::InspectorTab, view::Desktop};
use gpui::{AppContext, Context, PathPromptOptions, Window};
use std::{
    io::{Read, Write},
    path::{Path, PathBuf},
};

impl Desktop {
    /// 选择一个 UTF-8 SRT 文件；取消不修改项目，错误沿用实际失败提示，不导入其他媒体格式。
    pub(crate) fn choose_caption_file(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        self.commit_inspector_input(cx);
        // 原生对话框返回时不把确认键再次交给仍聚焦的导入按钮。
        self.focus_handle.focus(window);
        let receiver = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some(crate::i18n::t("text", "Import SRT", cx).into()),
        });
        cx.spawn_in(window, async move |this, cx| {
            let result = receiver.await;
            let _ = this.update_in(cx, |this, _, cx| match result {
                Ok(Ok(Some(paths))) => {
                    if let Some(path) = paths.into_iter().next() {
                        this.import_caption_file(path, cx);
                    }
                }
                Ok(Ok(None)) => {}
                error => {
                    this.notice = Some(format!("选择字幕文件失败：{error:?}"));
                    cx.notify();
                }
            });
        })
        .detach();
    }
    /// 在后台有界读取并原子解析字幕模型，成功后定位第一句并保存一次历史；失败保持原项目。
    /// 忙碌状态阻止并发编辑，文件不复制到项目，正文与时序成为项目内可编辑内容。
    pub(crate) fn import_caption_file(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        if self.busy || self.current.is_none() {
            return;
        }
        self.commit_inspector_input(cx);
        self.stop_timeline();
        self.stop_preview();
        let before = self.timeline.model.clone();
        let mut next = before.clone();
        self.busy = true;
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_spawn(async move {
                    let source = read_srt(&path)?;
                    let ids = next.import_srt(&source)?;
                    Ok::<_, String>((next, ids))
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.busy = false;
                match result {
                    Ok((next, ids)) => {
                        this.timeline.model = next;
                        if let Some(id) = ids.first() {
                            this.timeline.selected = [id.clone()].into();
                            this.timeline.primary_clip = Some(id.clone());
                            let c = this
                                .timeline
                                .model
                                .clips
                                .iter()
                                .find(|c| c.id == *id)
                                .unwrap();
                            this.session.timeline_position =
                                c.start as f64 / this.timeline.model.fps as f64;
                        }
                        this.timeline.preview = true;
                        this.timeline.inspector_tab = InspectorTab::Basic;
                        this.timeline.scroll_y = 0.0;
                        this.commit_timeline(before, cx);
                    }
                    Err(error) => this.notice = Some(error),
                }
                cx.notify();
            });
        })
        .detach();
    }
    /// 导出当前选中字幕所属组，使用系统另存为对话框；只交换正文/时间，不修改项目或已有文件。
    pub(crate) fn choose_caption_export(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        self.commit_inspector_input(cx);
        let Some(group) = self
            .text_clip()
            .and_then(|c| c.text.as_ref())
            .and_then(|t| t.group.as_ref())
        else {
            return;
        };
        let content = match self.timeline.model.export_srt(group) {
            Ok(s) => s,
            Err(error) => {
                self.notice = Some(error);
                cx.notify();
                return;
            }
        };
        let Some(entry) = &self.current else {
            return;
        };
        let name = format!("{}.srt", entry.name);
        // 原生保存确认后，焦点回到编辑器，避免 Enter 再次触发导出。
        self.focus_handle.focus(window);
        let receiver =
            cx.prompt_for_new_path(entry.path.parent().unwrap_or(Path::new(".")), Some(&name));
        cx.spawn_in(window, async move |this, cx| match receiver.await {
            Ok(Ok(Some(mut path))) => {
                if path.extension().is_none() {
                    path.set_extension("srt");
                }
                let result = cx
                    .background_spawn(async move { write_srt(&path, &content) })
                    .await;
                let _ = this.update(cx, |this, cx| {
                    if let Err(error) = result {
                        this.notice = Some(error);
                    }
                    cx.notify();
                });
            }
            Ok(Ok(None)) => {}
            error => {
                let _ = this.update(cx, |this, cx| {
                    this.notice = Some(format!("选择字幕输出失败：{error:?}"));
                    cx.notify();
                });
            }
        })
        .detach();
    }
}

/// 按上限读取 UTF-8 SRT，不一次读入任意大的文件；扩展名、编码和超量均给出明确错误。
fn read_srt(path: &Path) -> Result<String, String> {
    if !path
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("srt"))
    {
        return Err("请选择 SRT 字幕文件".into());
    }
    let mut bytes = Vec::new();
    std::fs::File::open(path)
        .map_err(|e| format!("无法读取字幕：{e}"))?
        .take(4 * 1024 * 1024 + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| format!("无法读取字幕：{e}"))?;
    if bytes.len() > 4 * 1024 * 1024 {
        return Err("字幕文件超过 4MiB".into());
    }
    String::from_utf8(bytes).map_err(|_| "字幕文件必须使用 UTF-8 编码".into())
}

/// 以新文件写入 UTF-8 字幕并落盘；已有路径绝不覆盖，失败仅清理本次新建的不完整文件。
pub(crate) fn write_srt(path: &Path, content: &str) -> Result<(), String> {
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|e| format!("无法创建字幕文件：{e}"))?;
    if let Err(error) = file
        .write_all(content.as_bytes())
        .and_then(|_| file.sync_all())
    {
        drop(file);
        let _ = std::fs::remove_file(path);
        return Err(format!("字幕写入失败：{error}"));
    }
    Ok(())
}
