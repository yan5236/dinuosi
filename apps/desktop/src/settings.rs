//! 独立非模态设置窗口；项目修改在原编辑会话上提交，不持有过期时间线副本。
use crate::{input::TextInput, preferences::UserPreferences, view::Desktop};
use dinuosi_core::{Preferences, ProjectStore, timeline::Timeline};
use gpui::{
    Context, Entity, EntityInputHandler, FocusHandle, KeyDownEvent, Subscription, WeakEntity,
    Window, WindowHandle, prelude::*,
};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    General,
    Shortcuts,
    Language,
    Plugins,
    Project,
}
impl Tab {
    /// 返回设置模块的稳定英文消息键，供侧边栏 ID、搜索与翻译后的主体标题复用。
    pub fn label(self) -> &'static str {
        match self {
            Self::General => "General",
            Self::Shortcuts => "Keyboard shortcuts",
            Self::Language => "Language",
            Self::Plugins => "Plugins",
            Self::Project => "Main settings",
        }
    }
}
pub const TABS: [Tab; 5] = [
    Tab::General,
    Tab::Shortcuts,
    Tab::Language,
    Tab::Plugins,
    Tab::Project,
];

pub struct Settings {
    pub(crate) owner: WeakEntity<Desktop>,
    pub(crate) store: ProjectStore,
    pub(crate) tab: Tab,
    pub(crate) search: Entity<TextInput>,
    pub(crate) focus: FocusHandle,
    pub(crate) draft: Preferences,
    pub(crate) fields: Vec<Entity<TextInput>>,
    pub(crate) project: bool,
    pub(crate) recording: Option<usize>,
    pub(crate) error: Option<String>,
    _search_watch: Subscription,
}

impl Desktop {
    /// 打开或聚焦唯一设置窗口，清除旧搜索以准确定位指定标签；工作区关闭时一并关闭。
    pub(crate) fn open_settings(&mut self, tab: Tab, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        self.menu_open = false;
        self.help_open = false;
        if let Some(handle) = self.settings_window
            && handle
                .update(cx, |settings, window, cx| {
                    settings.tab = tab;
                    settings.recording = None;
                    settings.search.update(cx, |input, cx| {
                        let end = input.text.encode_utf16().count();
                        input.replace_text_in_range(Some(0..end), "", window, cx);
                    });
                    window.activate_window();
                    cx.notify();
                })
                .is_ok()
        {
            cx.notify();
            return;
        }
        let store = self.store.clone();
        let owner = cx.weak_entity();
        let model = self.current.as_ref().map(|_| self.timeline.model.clone());
        let options = crate::help::auxiliary_options(
            crate::i18n::t("settings", "Settings", cx),
            860.0,
            600.0,
            cx,
        );
        match cx.open_window(options, |window, cx| {
            cx.new(|cx| Settings::new(owner, store, model, tab, window, cx))
        }) {
            Ok(handle) => {
                self.settings_window = Some(handle);
                close_with_owner(handle, cx);
            }
            Err(e) => self.notice = Some(format!("打开设置失败：{e}")),
        }
        cx.notify();
    }

    /// 对最新项目模型应用参数并原子保存；成功记录一次撤销，失败保留模型、历史与播放状态。
    pub(crate) fn apply_project_settings(
        &mut self,
        values: [u32; 4],
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        if self.busy
            || self.timeline.gesture.is_some()
            || self.timeline.input.is_some()
            || self.timeline.volume_input.is_some()
            || self.timeline.rename.is_some()
        {
            return Err("请先完成当前编辑操作".into());
        }
        let entry = self.current.as_ref().ok_or("当前项目已关闭")?;
        let mut next = self.timeline.model.clone();
        next.configure(values[0], values[1], values[2], values[3])?;
        if next == self.timeline.model {
            return Ok(());
        }
        self.store.save_timeline(entry, &next)?;
        self.stop_timeline();
        let before = std::mem::replace(&mut self.timeline.model, next);
        self.timeline.history.record(before, &self.timeline.model);
        self.session.timeline_position = self.timeline.model.frame(self.session.timeline_position)
            as f64
            / self.timeline.model.fps as f64;
        self.timeline.preview = true;
        self.refresh_timeline(cx);
        Ok(())
    }
}

/// 把辅助窗口寿命绑定到工作区；延迟关闭避免释放实体时重入窗口更新。
pub(crate) fn close_with_owner<T: 'static>(handle: WindowHandle<T>, cx: &mut Context<Desktop>) {
    cx.on_release(move |_, cx| {
        cx.defer(move |cx| {
            let _ = handle.update(cx, |_, window, _| window.remove_window());
        });
    })
    .detach();
}

impl Settings {
    /// 创建独立焦点、用户偏好草稿及项目参数输入，使用全局已应用语言，不重新解析草稿。
    /// 语言变化重绘窗口和独立输入实体；搜索变化仅重绘侧栏。
    pub(crate) fn new(
        owner: WeakEntity<Desktop>,
        store: ProjectStore,
        model: Option<Timeline>,
        tab: Tab,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        cx.observe_global::<crate::i18n::Locale>(|_, cx| cx.notify())
            .detach();
        let search = cx.new(|cx| {
            let mut input = TextInput::new(String::new(), cx);
            input.placeholder = "Search settings";
            input.placeholder_module = "settings";
            input.control_id = "settings-search";
            input
        });
        let watch = cx.observe(&search, |_, _, cx| cx.notify());
        let fields = model
            .as_ref()
            .map(|m| [m.width, m.height, m.fps, m.bitrate_kbps])
            .into_iter()
            .flatten()
            .zip([
                "project-width",
                "project-height",
                "project-fps",
                "project-bitrate",
            ])
            .map(|(value, id)| {
                cx.new(|cx| {
                    let mut input = TextInput::new(value.to_string(), cx);
                    input.control_id = id;
                    input
                })
            })
            .collect();
        let focus = cx.focus_handle();
        focus.focus(window);
        Self {
            owner,
            store,
            tab,
            search,
            focus,
            draft: cx.global::<UserPreferences>().0.clone(),
            fields,
            project: model.is_some(),
            recording: None,
            error: None,
            _search_watch: watch,
        }
    }

    /// 判断标签或其具体设置名是否匹配搜索；语言同时匹配索引 ID、自称及当前语言译名。
    /// 查询只读取草稿和已应用偏好，不修改分类或设置。
    pub(crate) fn matches(&self, tab: Tab, cx: &gpui::App) -> bool {
        let query = self.search.read(cx).text.trim().to_lowercase();
        let terms = match tab {
            Tab::General => "Live preview while dragging",
            Tab::Language => "Interface language Follow system",
            Tab::Plugins => "Plugins",
            Tab::Project => {
                "Project settings Main track Resolution Width Height Frame rate Bitrate"
            }
            Tab::Shortcuts => "",
        };
        query.is_empty()
            || crate::i18n::t("settings", tab.label(), cx)
                .to_lowercase()
                .contains(&query)
            || crate::i18n::lookup("zh-CN", "settings", tab.label()).contains(&query)
            || crate::i18n::t("settings", terms, cx)
                .to_lowercase()
                .contains(&query)
            || crate::i18n::lookup("zh-CN", "settings", terms).contains(&query)
            || (tab == Tab::Language
                && dinuosi_core::language::LANGUAGES.iter().any(|item| {
                    item.id.to_lowercase().contains(&query)
                        || item.native_name.to_lowercase().contains(&query)
                        || crate::i18n::t("settings", item.id, cx)
                            .to_lowercase()
                            .contains(&query)
                }))
            || (tab == Tab::Shortcuts
                && crate::shortcuts::SHORTCUTS.iter().any(|s| {
                    crate::i18n::t("settings", s.label, cx)
                        .to_lowercase()
                        .contains(&query)
                        || crate::shortcuts::label(&crate::shortcuts::bindings(s, &self.draft))
                            .to_lowercase()
                            .contains(&query)
                }))
    }

    /// 保存当前标签：项目参数提交到最新编辑模型，应用偏好在磁盘写入成功后全局生效。
    pub(crate) fn apply(&mut self, cx: &mut Context<Self>) {
        let result = if self.tab == Tab::Project {
            let values = self
                .fields
                .iter()
                .map(|v| {
                    v.read(cx)
                        .text
                        .trim()
                        .parse::<u32>()
                        .map_err(|_| "项目参数必须为正整数".to_owned())
                })
                .collect::<Result<Vec<_>, _>>();
            values.and_then(|v| {
                let values: [u32; 4] = v.try_into().map_err(|_| "没有打开项目".to_owned())?;
                self.owner
                    .update(cx, |desktop, cx| desktop.apply_project_settings(values, cx))
                    .map_err(|_| "当前项目已关闭".to_owned())?
            })
        } else {
            crate::preferences::save(&self.store, self.draft.clone(), cx)
        };
        self.error = result.err();
        self.recording = None;
        cx.notify();
    }

    /// 设置窗口处理录制、取消和焦点导航；未处理字符交给本窗口 IME，独立窗口隔离编辑器。
    pub(crate) fn key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let key = event.keystroke.key.as_str();
        if !self.focus.is_focused(window) {
            self.recording = None;
        }
        if let Some(index) = self.recording {
            if key == "escape" {
                self.recording = None;
                self.error = None;
            } else {
                let mut draft = self.draft.clone();
                let result =
                    crate::shortcuts::normalize(&event.keystroke.unparse()).and_then(|key| {
                        draft
                            .shortcuts
                            .insert(crate::shortcuts::SHORTCUTS[index].id.into(), vec![key]);
                        crate::shortcuts::validate(&draft)
                    });
                match result {
                    Ok(()) => {
                        self.draft = draft;
                        self.recording = None;
                        self.error = None;
                    }
                    Err(e) => self.error = Some(e),
                }
            }
        } else if key == "escape" {
            window.remove_window();
        } else if key == "tab" {
            if event.keystroke.modifiers.shift {
                window.focus_prev();
            } else {
                window.focus_next();
            }
        } else if key == "enter"
            && !self
                .fields
                .iter()
                .chain(std::iter::once(&self.search))
                .any(|v| v.read(cx).is_composing())
        {
            self.apply(cx);
        } else {
            return;
        }
        cx.stop_propagation();
        cx.notify();
    }
}
