//! 导出模态会话：参数只属于本次任务，编码与能力探测均在后台进行。
use crate::{input::TextInput, view::Desktop};
use dinuosi_core::export::{Container, Encoder, ExportSettings};
use gpui::{Context, Entity, FocusHandle, PathPromptOptions, Window, prelude::*};
use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU32, Ordering},
    },
    time::Duration,
};

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum ExportField {
    Resolution,
    Fps,
    Bitrate,
    Container,
    Encoder,
}

pub(crate) struct ExportDialog {
    pub settings: ExportSettings,
    pub encoders: Vec<Encoder>,
    pub name: Entity<TextInput>,
    pub directory: Option<PathBuf>,
    pub menu: Option<ExportField>,
    pub probing: bool,
    pub choosing: bool,
    pub running: bool,
    pub status: Option<&'static str>,
    pub error: Option<String>,
    pub cancel: Arc<AtomicBool>,
    pub progress: Arc<AtomicU32>,
    pub focus: Vec<FocusHandle>,
}

impl Drop for ExportDialog {
    /// 窗口关闭或会话销毁时通知后台终止，防止失去所属界面的编码进程继续发布文件。
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
}

impl Desktop {
    /// 打开唯一导出表单，提交当前检查器编辑并停止播放；参数以项目为默认值但不写回项目。
    /// 编码器用受控运行库实际试编码，探测失败保留弹窗并显示诊断，不借用系统 FFmpeg。
    pub(crate) fn open_export(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy || self.export_dialog.is_some() || self.current.is_none() {
            return;
        }
        self.notice = None;
        self.commit_inspector_input(cx);
        self.end_timeline_pointer(window, cx);
        if self.notice.is_some() {
            return;
        }
        self.stop_preview();
        self.stop_timeline();
        self.timeline.preview = true;
        self.refresh_timeline(cx);
        self.menu_open = false;
        self.help_open = false;
        self.project_menu = None;
        self.session.media_menu = None;
        self.timeline.marker_menu = None;
        self.timeline.keyframe_menu = None;
        self.dialog = None;
        self.timeline.marker_dialog = None;
        let name = self.current.as_ref().unwrap().name.clone();
        let name = cx.new(|cx| {
            let mut input = TextInput::new(name, cx);
            input.control_id = "export-name";
            input
        });
        let mut focus = vec![name.read(cx).focus.clone()];
        focus.extend((0..8).map(|_| cx.focus_handle()));
        focus[0].focus(window);
        self.export_dialog = Some(ExportDialog {
            settings: ExportSettings::for_timeline(&self.timeline.model, Encoder::Mpeg4),
            encoders: Vec::new(),
            name,
            directory: None,
            menu: None,
            probing: true,
            choosing: false,
            running: false,
            status: None,
            error: None,
            cancel: Arc::new(AtomicBool::new(false)),
            progress: Arc::new(AtomicU32::new(0)),
            focus,
        });
        // 以当前会话的取消句柄识别探测结果，关闭后再打开的表单不会收到旧任务的结果。
        let token = self.export_dialog.as_ref().unwrap().cancel.clone();
        cx.spawn_in(window, async move |this, cx| {
            let result = cx
                .background_spawn(async { dinuosi_core::export::capabilities() })
                .await;
            let _ = this.update_in(cx, |this, _, cx| {
                if let Some(dialog) = &mut this.export_dialog
                    && Arc::ptr_eq(&dialog.cancel, &token)
                {
                    dialog.probing = false;
                    match result {
                        Ok(encoders) if !encoders.is_empty() => {
                            dialog.settings.encoder = encoders[0];
                            dialog.encoders = encoders;
                        }
                        Ok(_) => dialog.error = Some("没有可用的视频编码器".into()),
                        Err(error) => dialog.error = Some(error),
                    }
                    cx.notify();
                }
            });
        })
        .detach();
        cx.notify();
    }

    /// 原生选择器只接受一个输出文件夹；取消不改变选择，迟到结果不会写入下一次导出会话。
    pub(crate) fn choose_export_directory(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(dialog) = &mut self.export_dialog else {
            return;
        };
        if dialog.running || dialog.choosing {
            return;
        }
        dialog.choosing = true;
        dialog.menu = None;
        let token = dialog.cancel.clone();
        let receiver = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some(crate::i18n::t("export", "Output folder", cx).into()),
        });
        cx.spawn_in(window, async move |this, cx| {
            let result = receiver.await;
            let _ = this.update_in(cx, |this, _, cx| {
                if let Some(dialog) = &mut this.export_dialog
                    && Arc::ptr_eq(&dialog.cancel, &token)
                {
                    dialog.choosing = false;
                    match result {
                        Ok(Ok(Some(paths))) => {
                            dialog.directory = paths.into_iter().next();
                            dialog.error = None;
                        }
                        Ok(Ok(None)) => {}
                        Ok(Err(error)) => dialog.error = Some(format!("选择目录失败：{error}")),
                        Err(error) => dialog.error = Some(format!("目录选择器未完成：{error}")),
                    }
                    cx.notify();
                }
            });
        })
        .detach();
        cx.notify();
    }

    /// 校验文件名与目录后对当前时间线快照编码；后台只更新原子进度，界面仅在进度改变时刷新。
    /// 核心负责临时文件、取消清理及无覆盖发布；失败保留参数，成功显示完成状态而不修改项目。
    pub(crate) fn submit_export(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(dialog) = &mut self.export_dialog else {
            return;
        };
        if dialog.running || dialog.probing || dialog.choosing || dialog.encoders.is_empty() {
            return;
        }
        let name = dialog.name.read(cx).text.trim();
        let result = export_path(name, dialog.directory.as_ref(), dialog.settings.container);
        let output = match result {
            Ok(path) => path,
            Err(error) => {
                dialog.error = Some(error);
                cx.notify();
                return;
            }
        };
        dialog.cancel.store(false, Ordering::Relaxed);
        dialog.progress.store(0, Ordering::Relaxed);
        dialog.running = true;
        dialog.menu = None;
        dialog.error = None;
        dialog.status = None;
        dialog.focus[7].focus(window);
        let cancel = dialog.cancel.clone();
        let progress = dialog.progress.clone();
        let poll_token = cancel.clone();
        let poll_progress = progress.clone();
        let settings = dialog.settings.clone();
        let timeline = self.timeline.model.clone();
        let media = self
            .session
            .media
            .iter()
            .map(|m| m.source.clone())
            .collect::<Vec<_>>();
        cx.spawn_in(window, async move |this, cx| {
            let token = cancel.clone();
            let result = cx
                .background_spawn(async move {
                    dinuosi_core::export::export(
                        &timeline,
                        &media,
                        &settings,
                        &output,
                        &cancel,
                        |value| {
                            progress.store((value * 1000.0) as u32, Ordering::Relaxed);
                        },
                    )
                })
                .await;
            let _ = this.update_in(cx, |this, _, cx| {
                if let Some(dialog) = &mut this.export_dialog
                    && Arc::ptr_eq(&dialog.cancel, &token)
                {
                    dialog.running = false;
                    match result {
                        Ok(()) => dialog.status = Some("Completed"),
                        Err(_) if token.load(Ordering::Relaxed) => {
                            dialog.status = Some("Cancelled")
                        }
                        Err(error) => dialog.error = Some(error),
                    }
                    cx.notify();
                }
            });
        })
        .detach();
        cx.spawn_in(window, async move |this, cx| {
            let mut displayed = 0;
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(100))
                    .await;
                let current = poll_progress.load(Ordering::Relaxed);
                let keep = this.update_in(cx, |this, _, cx| {
                    let keep = this
                        .export_dialog
                        .as_ref()
                        .is_some_and(|d| d.running && Arc::ptr_eq(&d.cancel, &poll_token));
                    if keep && current != displayed {
                        cx.notify();
                    }
                    keep
                });
                displayed = current;
                if !matches!(keep, Ok(true)) {
                    break;
                }
            }
        })
        .detach();
        cx.notify();
    }

    /// 运行期间请求取消并等待后台清理，空闲时关闭表单恢复工作区焦点；目录选择器打开时保持表单。
    pub(crate) fn close_export(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(dialog) = &self.export_dialog {
            if dialog.choosing {
                return;
            }
            if dialog.running {
                dialog.cancel.store(true, Ordering::Relaxed);
                cx.notify();
                return;
            }
        }
        self.export_dialog = None;
        self.focus_handle.focus(window);
        cx.notify();
    }

    /// 模态键盘路由屏蔽剪辑快捷键；Tab 循环实际控件（运行中仅取消），Escape 收起下拉或取消任务，提交只响应提交按钮。
    /// 输入法组合期间保留候选按键，避免中文文件名的确认操作触发导出。
    pub(crate) fn export_key(
        &mut self,
        event: &gpui::KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(dialog) = &mut self.export_dialog else {
            return;
        };
        if dialog.name.read(cx).is_composing() {
            return;
        }
        match event.keystroke.key.as_str() {
            "escape" if dialog.menu.is_some() => {
                dialog.menu = None;
                cx.notify();
            }
            "escape" => self.close_export(window, cx),
            "tab" if dialog.running => dialog.focus[7].focus(window),
            "tab" => {
                let current = dialog
                    .focus
                    .iter()
                    .position(|f| f.is_focused(window))
                    .unwrap_or(0);
                let step = if event.keystroke.modifiers.shift {
                    dialog.focus.len() - 1
                } else {
                    1
                };
                dialog.focus[(current + step) % dialog.focus.len()].focus(window);
            }
            "enter" | "space" if !event.is_held && !event.keystroke.modifiers.modified() => {
                if dialog.focus[8].is_focused(window) {
                    self.submit_export(window, cx);
                } else if dialog.focus[7].is_focused(window) {
                    self.close_export(window, cx);
                } else if dialog.focus[6].is_focused(window) {
                    self.choose_export_directory(window, cx);
                } else {
                    return;
                }
            }
            _ => return,
        }
        cx.stop_propagation();
    }
}

/// 将单个合法文件名与显式选择的目录组合，拒绝路径穿越、Windows 保留名与末尾点/空格。
/// 扩展名由封装格式决定，不自动覆盖已有输出；已有文件的原子保护由核心发布流程负责。
pub(crate) fn export_path(
    name: &str,
    directory: Option<&PathBuf>,
    container: Container,
) -> Result<PathBuf, String> {
    if name.is_empty()
        || name
            .chars()
            .any(|c| c.is_control() || "<>:\"/\\|?*".contains(c))
        || name.ends_with(['.', ' '])
        || name == "."
        || name == ".."
    {
        return Err("请输入有效的输出文件名".into());
    }
    let stem = name
        .split('.')
        .next()
        .unwrap_or_default()
        .to_ascii_uppercase();
    if matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || (stem.len() == 4
            && (stem.starts_with("COM") || stem.starts_with("LPT"))
            && matches!(stem.as_bytes()[3], b'1'..=b'9'))
    {
        return Err("请输入有效的输出文件名".into());
    }
    let directory = directory.ok_or("请选择输出文件夹")?;
    let extension = container.extension();
    Ok(directory.join(format!("{name}.{extension}")))
}
