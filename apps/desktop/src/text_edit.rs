//! 文字编辑会话：正文实时预览，失焦一次保存/撤销；样式与时间输入始终绑定片段身份。
pub(crate) use crate::text_fields::TextField;
use crate::{input::TextInput, timeline_state::InspectorTab, view::Desktop};
use dinuosi_core::{
    text::{Text, TextStyle},
    timeline::{Clip, Timeline},
};
use gpui::{AppContext, Context, Entity, Subscription, Window};

pub(crate) struct BodyEdit {
    pub id: String,
    pub input: Entity<TextInput>,
    pub before: Timeline,
    _observe: Subscription,
    _blur: Subscription,
}

pub(crate) struct FieldEdit {
    pub id: String,
    pub field: TextField,
    pub input: Entity<TextInput>,
    _blur: Subscription,
}

pub(crate) struct TextSession {
    pub body: Option<BodyEdit>,
    pub field: Option<FieldEdit>,
    pub apply_group: bool,
    pub font_menu: bool,
    pub picker: Option<crate::text_color_picker::TextColorPicker>,
    pub fonts: Vec<String>,
    pub presets: Vec<crate::text_presets::PresetItem>,
    pub presets_loaded: bool,
    pub preset_draft: Option<crate::text_presets::PresetDraft>,
}

impl Default for TextSession {
    /// 字幕样式默认应用本组，普通文本忽略此开关；不预先扫描字体或创建输入实体。
    fn default() -> Self {
        Self {
            body: None,
            field: None,
            apply_group: true,
            font_menu: false,
            picker: None,
            fonts: Vec::new(),
            presets: Vec::new(),
            presets_loaded: false,
            preset_draft: None,
        }
    }
}

impl Desktop {
    /// 读取当前检查器的生成文字，其他媒体返回 None，不改变时间线选择集合。
    pub(crate) fn text_clip(&self) -> Option<&Clip> {
        self.inspector_clip().filter(|c| c.text.is_some())
    }

    /// 在播放头新增普通文字或手动字幕，自动建轨并选中；本组新增沿用当前字幕的样式与变换。
    /// 已有片段不会覆盖或后移；创建成功后立即聚焦正文，失败恢复新增前模型并显示错误。
    pub(crate) fn add_text_clip(
        &mut self,
        subtitle: bool,
        preset: Option<TextStyle>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.busy {
            return;
        }
        self.commit_inspector_input(cx);
        self.stop_timeline();
        self.stop_preview();
        let model = &self.timeline.model;
        let mut text = Text::new(
            crate::i18n::t("text", if subtitle { "Subtitle" } else { "Text" }, cx).into(),
            (model.width, model.height),
            subtitle,
        );
        let inherited = self
            .text_clip()
            .filter(|c| subtitle && c.text.as_ref().unwrap().group.is_some())
            .cloned();
        if let Some(c) = &inherited {
            let t = c.text.as_ref().unwrap();
            text.group = t.group.clone();
            text.style = t.style.clone();
        }
        if let Some(style) = preset {
            text.style = style;
        }
        let start = model.frame(self.session.timeline_position);
        let before = model.clone();
        match self
            .timeline
            .model
            .add_text(start, self.timeline.model.fps as i64 * 3, text)
        {
            Ok(id) => {
                if let Some(c) = inherited {
                    self.timeline
                        .model
                        .clips
                        .iter_mut()
                        .find(|c| c.id == id)
                        .unwrap()
                        .transform = c.transform;
                }
                self.timeline.selected = [id.clone()].into();
                self.timeline.primary_clip = Some(id);
                if !subtitle || self.timeline.inspector_tab != InspectorTab::Captions {
                    self.timeline.inspector_tab = InspectorTab::Basic;
                }
                self.timeline.preview = true;
                self.timeline.scroll_y = 0.0;
                self.commit_timeline(before, cx);
                self.begin_text_body(window, cx);
            }
            Err(error) => {
                self.timeline.model = before;
                self.notice = Some(error);
            }
        }
        cx.notify();
    }

    /// 创建多行正文实体并订阅已提交的文本变化；预览/片段标签同步，IME 组合不提前持久化。
    /// 每次聚焦保存一个历史快照，锁定文字只展示正文，不创建可编辑实体。
    pub(crate) fn begin_text_body(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(c) = self.text_clip().cloned() else {
            return;
        };
        if self.busy || c.locked || self.timeline.model.tracks[c.track].locked {
            return;
        }
        if self
            .timeline
            .text
            .body
            .as_ref()
            .is_some_and(|e| e.id == c.id)
        {
            return;
        }
        self.commit_inspector_input(cx);
        let input = cx.new(|cx| {
            let mut input = TextInput::new(c.text.unwrap().body, cx);
            input.multiline = true;
            input.control_id = "text-body";
            input
        });
        input.read(cx).focus.focus(window);
        let id = c.id.clone();
        let observe = cx.observe(&input, move |this, input, cx| {
            let input = input.read(cx);
            if input.is_composing() {
                return;
            }
            let body = input.text.clone();
            if this
                .timeline
                .model
                .clips
                .iter()
                .find(|c| c.id == id)
                .and_then(|c| c.text.as_ref())
                .is_some_and(|t| t.body == body)
            {
                return;
            }
            if this.timeline.model.set_text_body(&id, body).is_ok() {
                this.refresh_timeline(cx);
            }
        });
        let focus = input.read(cx).focus.clone();
        let blur = cx.on_blur(&focus, window, |this, _, cx| {
            this.commit_text_input(cx);
        });
        self.timeline.text.body = Some(BodyEdit {
            id: c.id,
            input,
            before: self.timeline.model.clone(),
            _observe: observe,
            _blur: blur,
        });
        cx.notify();
    }

    /// 开启绑定当前文字的字体/数值/颜色/时间输入；失焦、Enter 都经过同一验证和单次保存。
    pub(crate) fn begin_text_field(
        &mut self,
        field: TextField,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(c) = self.text_clip().cloned() else {
            return;
        };
        if self.busy || c.locked || self.timeline.model.tracks[c.track].locked {
            return;
        }
        self.commit_inspector_input(cx);
        let value = field.value(&c, self.timeline.model.fps);
        let input = cx.new(|cx| {
            let mut i = TextInput::new(value, cx);
            i.compact = true;
            i.control_id = field.id();
            i
        });
        input.read(cx).focus.focus(window);
        let focus = input.read(cx).focus.clone();
        let blur = cx.on_blur(&focus, window, |this, _, cx| {
            this.commit_text_input(cx);
        });
        self.timeline.text.field = Some(FieldEdit {
            id: c.id,
            field,
            input,
            _blur: blur,
        });
        cx.notify();
    }

    /// 先结束调色草稿再提交正文与参数输入；取出实体避免失焦重入，失败恢复原模型并保留输入供重试。
    /// 正文编辑期间的所有实时预览只记录一次撤销；样式传播只覆盖明确的本组字幕。
    /// 全部提交成功返回 true；校验或保存失败返回 false，并通知界面显示实际错误。
    pub(crate) fn commit_text_input(&mut self, cx: &mut Context<Self>) -> bool {
        self.finish_text_color(false, cx);
        if let Some(edit) = self.timeline.text.body.take() {
            if let Err(error) = self
                .timeline
                .model
                .set_text_body(&edit.id, edit.input.read(cx).text.clone())
            {
                self.timeline.model = edit.before.clone();
                self.notice = Some(error);
                self.timeline.text.body = Some(edit);
                self.refresh_timeline(cx);
                cx.notify();
                return false;
            } else if !self.commit_timeline(edit.before.clone(), cx) {
                self.timeline.text.body = Some(edit);
                cx.notify();
                return false;
            }
        }
        if let Some(edit) = self.timeline.text.field.take() {
            let before = self.timeline.model.clone();
            match self.apply_text_field(&edit.id, edit.field, &edit.input.read(cx).text.clone()) {
                Ok(()) => {
                    if !self.commit_timeline(before, cx) {
                        self.timeline.text.field = Some(edit);
                        cx.notify();
                        return false;
                    }
                }
                Err(error) => {
                    self.timeline.model = before;
                    self.notice = Some(error);
                    self.timeline.text.field = Some(edit);
                    cx.notify();
                    return false;
                }
            }
        }
        self.timeline.text.font_menu = false;
        true
    }

    /// Escape 取消本次调色、正文或参数草稿，恢复进入编辑前快照；不触碰更早的已保存编辑。
    pub(crate) fn cancel_text_input(&mut self, cx: &mut Context<Self>) {
        self.finish_text_color(true, cx);
        if let Some(edit) = self.timeline.text.body.take() {
            self.timeline.model = edit.before;
            self.refresh_timeline(cx);
        }
        self.timeline.text.field = None;
        self.timeline.text.font_menu = false;
        cx.notify();
    }

    /// 对当前文字执行一个样式操作并提交单次历史；作用范围由字幕的明确勾选项控制。
    pub(crate) fn change_text_style(
        &mut self,
        change: impl FnOnce(&mut TextStyle),
        cx: &mut Context<Self>,
    ) {
        if self.busy {
            return;
        }
        self.commit_inspector_input(cx);
        let Some(c) = self.text_clip() else {
            return;
        };
        let id = c.id.clone();
        let mut style = c.text.as_ref().unwrap().style.clone();
        change(&mut style);
        let before = self.timeline.model.clone();
        match self
            .timeline
            .model
            .set_text_style(&id, style, self.timeline.text.apply_group)
        {
            Ok(()) => {
                self.commit_timeline(before, cx);
            }
            Err(error) => self.notice = Some(error),
        }
        cx.notify();
    }

    /// 从字幕列表选中指定句并把播放头移到起点；原正文先提交，检查器与画布共用同一主对象。
    pub(crate) fn select_caption(
        &mut self,
        id: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.commit_inspector_input(cx);
        self.stop_timeline();
        self.stop_preview();
        if let Some(c) = self.timeline.model.clips.iter().find(|c| c.id == id) {
            self.timeline.curves.selected = None;
            self.session.timeline_position = c.start as f64 / self.timeline.model.fps as f64;
            self.timeline.selected = [id.clone()].into();
            self.timeline.primary_clip = Some(id);
            self.timeline.preview = true;
            self.focus_handle.focus(window);
            self.refresh_timeline(cx);
        }
        cx.notify();
    }
}
