use crate::ui::common::agent_settings::{self, AgentSection};
use crate::ui::common::agent_visual::{self, AgentVisual};
use crate::ui::common::{
    AdminSection, CenterView, ChatActivityKind, ChatRole, DocumentView, LeftView, UiController,
    UiEvent,
};
use crate::ui::common::{inline, syntax, theme};
use gpui::{
    App, Bounds, Context, DispatchPhase, Entity, Focusable, FontStyle, HighlightStyle, IntoElement,
    KeyDownEvent, MouseButton, ObjectFit, QuitMode, Render, ScrollHandle, StyledText, Subscription,
    Task, Window, WindowBounds, WindowOptions, div, img, prelude::*, px, rgb, size,
};
use gpui_ce_elements::editable_text::{
    EditableTextState, StringStorage, TextChanged,
    actions::{DEFAULT_INPUT_CONTEXT, default_bindings},
    editable_text, text_input,
};
use std::borrow::Cow;
use std::time::Duration;

fn bitmap_icon(data: &'static [u8], size: f32) -> gpui::AnyElement {
    agent_visual::cached_svg_png(data, 48)
        .map(|bytes| {
            img(std::sync::Arc::new(gpui::Image::from_bytes(
                gpui::ImageFormat::Png,
                bytes.to_vec(),
            )))
            .w(px(size))
            .h(px(size))
            .object_fit(ObjectFit::Contain)
            .into_any_element()
        })
        .unwrap_or_else(|| div().w(px(size)).h(px(size)).into_any_element())
}

fn bitmap_asset(path: &std::path::Path, size: f32) -> gpui::AnyElement {
    if path
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("svg"))
    {
        if let Some(bytes) = agent_visual::cached_image_bytes(path) {
            return img(std::sync::Arc::new(gpui::Image::from_bytes(
                gpui::ImageFormat::Png,
                bytes.to_vec(),
            )))
            .w(px(size))
            .h(px(size))
            .object_fit(ObjectFit::Contain)
            .into_any_element();
        }
        return bitmap_icon(include_bytes!("../common/icons/file.svg"), size);
    }
    img(path.to_path_buf())
        .w(px(size))
        .h(px(size))
        .object_fit(ObjectFit::Contain)
        .into_any_element()
}

fn agent_avatar(visual: &AgentVisual, size: f32) -> gpui::AnyElement {
    if let Some(path) = &visual.avatar_path {
        if path
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("svg"))
            && agent_visual::cached_image_bytes(path).is_none()
        {
            return bitmap_icon(include_bytes!("../common/icons/agents.svg"), size);
        }
        return bitmap_asset(path, size);
    }
    if let Some(bytes) = visual.avatar_bytes() {
        return img(std::sync::Arc::new(gpui::Image::from_bytes(
            gpui::ImageFormat::Png,
            bytes.to_vec(),
        )))
        .w(px(size))
        .h(px(size))
        .object_fit(ObjectFit::Contain)
        .into_any_element();
    }
    div()
        .w(px(size))
        .h(px(size))
        .child(visual.initials.clone())
        .into_any_element()
}

struct InfoTooltip {
    text: &'static str,
}

impl Render for InfoTooltip {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .max_w(px(300.))
            .p_2()
            .rounded_md()
            .border_1()
            .border_color(rgb(theme::resolve(theme::BORDER)))
            .bg(rgb(theme::resolve(theme::SURFACE)))
            .child(self.text)
    }
}

fn info_icon(id: String, help: &'static str) -> impl IntoElement {
    div()
        .id(id)
        .w(px(16.))
        .h(px(16.))
        .child(bitmap_icon(include_bytes!("../common/icons/info.svg"), 15.))
        .tooltip(move |_, cx| cx.new(|_| InfoTooltip { text: help }).into())
}

fn highlighted_code(text: String) -> StyledText {
    let highlights = syntax::tokens(&text)
        .into_iter()
        .map(|(range, kind)| {
            let color = match kind {
                syntax::TokenKind::Keyword => 0x7c3aed,
                syntax::TokenKind::String => 0x059669,
                syntax::TokenKind::Comment => 0x64748b,
                syntax::TokenKind::Number => 0xd97706,
            };
            (
                range,
                HighlightStyle {
                    color: Some(gpui::rgb_to_hsla(rgb(color))),
                    ..Default::default()
                },
            )
        })
        .collect::<Vec<_>>();
    StyledText::new(text).with_highlights(highlights)
}

fn styled_inline(text: String) -> StyledText {
    let parsed = inline::parse(&text);
    let highlights = parsed
        .spans
        .into_iter()
        .map(|(range, kind)| {
            let style = match kind {
                inline::InlineStyle::Strong => HighlightStyle {
                    font_weight: Some(gpui::FontWeight::SEMIBOLD),
                    ..Default::default()
                },
                inline::InlineStyle::Emphasis => HighlightStyle {
                    font_style: Some(FontStyle::Italic),
                    ..Default::default()
                },
                inline::InlineStyle::Code => HighlightStyle {
                    background_color: Some(gpui::rgb_to_hsla(rgb(theme::resolve(
                        theme::SURFACE_MUTED,
                    )))),
                    ..Default::default()
                },
                inline::InlineStyle::Link(_) => HighlightStyle {
                    color: Some(gpui::rgb_to_hsla(rgb(theme::resolve(theme::PRIMARY)))),
                    ..Default::default()
                },
            };
            (range, style)
        })
        .collect::<Vec<_>>();
    StyledText::new(parsed.plain).with_highlights(highlights)
}

pub fn run(controller: UiController) {
    gpui_platform::application()
        .with_quit_mode(QuitMode::LastWindowClosed)
        .run(move |cx: &mut App| {
            cx.text_system()
                .add_fonts(vec![Cow::Borrowed(theme::FONT_BYTES)])
                .expect("load bundled Inter font");
            cx.bind_keys(default_bindings().as_keybindings(Some(DEFAULT_INPUT_CONTEXT)));
            let mut controller = Some(controller);
            let bounds = Bounds::centered(None, size(px(1440.), px(850.)), cx);
            cx.open_window(
                WindowOptions::default().window_bounds(WindowBounds::Windowed(bounds)),
                move |window, cx| {
                    window.set_window_title("TaIQ desktop");
                    cx.new(|cx| DesktopView::new(controller.take().unwrap(), window, cx))
                },
            )
            .expect("open taiq window");
            cx.activate(true);
        });
}

struct DesktopView {
    controller: UiController,
    chat_input: Entity<EditableTextState>,
    terminal_input: Entity<EditableTextState>,
    terminal_scroll: ScrollHandle,
    agent_search_input: Entity<EditableTextState>,
    model_search_input: Entity<EditableTextState>,
    settings_search_input: Entity<EditableTextState>,
    settings_name_input: Entity<EditableTextState>,
    settings_category_input: Entity<EditableTextState>,
    settings_description_input: Entity<EditableTextState>,
    settings_workflow_input: Entity<EditableTextState>,
    settings_ui_description_input: Entity<EditableTextState>,
    settings_raw_input: Entity<EditableTextState>,
    file_input: Entity<EditableTextState>,
    search_input: Entity<EditableTextState>,
    session_search_input: Entity<EditableTextState>,
    web_input: Entity<EditableTextState>,
    editor_input: Entity<EditableTextState>,
    admin_input: Entity<EditableTextState>,
    admin_target_input: Entity<EditableTextState>,
    admin_run_input: Entity<EditableTextState>,
    credential_input: Entity<EditableTextState>,
    _subscriptions: Vec<Subscription>,
    _poll_task: Task<()>,
    left_width: f32,
    editor_width: f32,
    dragging: Option<(bool, f32)>,
    dragging_tab: Option<CenterView>,
    dragging_document: Option<String>,
    dragging_float: Option<(String, bool, f32, f32)>,
    confirm_delete: Option<String>,
    model_picker_open: bool,
    model_query: String,
    model_cursor: usize,
    agent_picker_open: bool,
    agent_query: String,
    agent_cursor: usize,
    reasoning_open: bool,
    prompts_open: bool,
    settings_categories_open: bool,
    dragging_terminal: Option<f32>,
}

impl DesktopView {
    fn sync_agent_settings_inputs(&mut self, cx: &mut Context<Self>) {
        let Some(settings) = self.controller.state.agent_settings.clone() else {
            return;
        };
        self.settings_name_input
            .update(cx, |input, cx| input.emplace(&settings.draft.name, cx));
        self.settings_category_input.update(cx, |input, cx| {
            input.emplace(
                settings
                    .draft
                    .other
                    .get("category")
                    .and_then(|v| v.as_str())
                    .unwrap_or(""),
                cx,
            )
        });
        self.settings_description_input.update(cx, |input, cx| {
            input.emplace(settings.draft.description.as_deref().unwrap_or(""), cx)
        });
        self.settings_workflow_input.update(cx, |input, cx| {
            input.emplace(
                settings
                    .field("ui", "workflow_instructions")
                    .and_then(|v| v.as_str())
                    .unwrap_or(""),
                cx,
            )
        });
        self.settings_ui_description_input.update(cx, |input, cx| {
            input.emplace(
                settings
                    .field("ui", "ui_description")
                    .and_then(|v| v.as_str())
                    .unwrap_or(""),
                cx,
            )
        });
        self.settings_raw_input
            .update(cx, |input, cx| input.emplace(&settings.raw_text, cx));
        self.settings_search_input
            .update(cx, |input, cx| input.emplace(&settings.query, cx));
        if let Some(state) = &mut self.controller.state.agent_settings {
            state.dirty = settings.dirty;
        }
    }

    fn new(controller: UiController, _window: &mut Window, cx: &mut Context<Self>) -> Self {
        let chat_input = cx.new(|cx| EditableTextState::new(StringStorage::default(), cx));
        let terminal_input = cx.new(|cx| EditableTextState::new(StringStorage::default(), cx));
        let agent_search_input = cx.new(|cx| EditableTextState::new(StringStorage::default(), cx));
        let model_search_input = cx.new(|cx| EditableTextState::new(StringStorage::default(), cx));
        let settings_search_input =
            cx.new(|cx| EditableTextState::new(StringStorage::default(), cx));
        let settings_name_input = cx.new(|cx| EditableTextState::new(StringStorage::default(), cx));
        let settings_category_input =
            cx.new(|cx| EditableTextState::new(StringStorage::default(), cx));
        let settings_description_input =
            cx.new(|cx| EditableTextState::new(StringStorage::default(), cx));
        let settings_workflow_input =
            cx.new(|cx| EditableTextState::new(StringStorage::default(), cx));
        let settings_ui_description_input =
            cx.new(|cx| EditableTextState::new(StringStorage::default(), cx));
        let settings_raw_input = cx.new(|cx| EditableTextState::new(StringStorage::default(), cx));
        let file_input = cx.new(|cx| EditableTextState::new(StringStorage::default(), cx));
        let search_input = cx.new(|cx| EditableTextState::new(StringStorage::default(), cx));
        let session_search_input =
            cx.new(|cx| EditableTextState::new(StringStorage::default(), cx));
        let web_input = cx.new(|cx| EditableTextState::new(StringStorage::default(), cx));
        let editor_input = cx.new(|cx| EditableTextState::new(StringStorage::default(), cx));
        let admin_input = cx.new(|cx| EditableTextState::new(StringStorage::default(), cx));
        let admin_target_input = cx.new(|cx| EditableTextState::new(StringStorage::default(), cx));
        let admin_run_input = cx.new(|cx| EditableTextState::new(StringStorage::default(), cx));
        let credential_input = cx.new(|cx| EditableTextState::new(StringStorage::default(), cx));
        let mut subscriptions = Vec::new();
        subscriptions.push(
            cx.subscribe(&chat_input, |this, input, _: &TextChanged, cx| {
                this.controller
                    .dispatch(UiEvent::SetChatDraft(input.read(cx).as_str().to_owned()));
                cx.notify();
            }),
        );
        subscriptions.push(
            cx.subscribe(&terminal_input, |this, input, _: &TextChanged, cx| {
                this.controller.dispatch(UiEvent::SetTerminalInput(
                    input.read(cx).as_str().to_owned(),
                ));
                cx.notify();
            }),
        );
        subscriptions.push(cx.subscribe(
            &settings_search_input,
            |this, input, _: &TextChanged, cx| {
                this.controller.dispatch(UiEvent::SetAgentSettingsQuery(
                    input.read(cx).as_str().to_owned(),
                ));
                cx.notify();
            },
        ));
        subscriptions.push(cx.subscribe(
            &agent_search_input,
            |this, input, _: &TextChanged, cx| {
                this.agent_query = input.read(cx).as_str().to_owned();
                this.agent_cursor = 0;
                cx.notify();
            },
        ));
        subscriptions.push(cx.subscribe(
            &model_search_input,
            |this, input, _: &TextChanged, cx| {
                this.model_query = input.read(cx).as_str().to_owned();
                this.model_cursor = 0;
                cx.notify();
            },
        ));
        subscriptions.push(cx.subscribe(
            &settings_name_input,
            |this, input, _: &TextChanged, cx| {
                this.controller
                    .dispatch(UiEvent::SetAgentName(input.read(cx).as_str().to_owned()));
                cx.notify();
            },
        ));
        subscriptions.push(cx.subscribe(
            &settings_description_input,
            |this, input, _: &TextChanged, cx| {
                this.controller.dispatch(UiEvent::SetAgentDescription(
                    input.read(cx).as_str().to_owned(),
                ));
                cx.notify();
            },
        ));
        subscriptions.push(cx.subscribe(
            &settings_category_input,
            |this, input, _: &TextChanged, cx| {
                this.controller.dispatch(UiEvent::SetAgentCategory(
                    input.read(cx).as_str().to_owned(),
                ));
                cx.notify();
            },
        ));
        subscriptions.push(cx.subscribe(
            &settings_workflow_input,
            |this, input, _: &TextChanged, cx| {
                this.controller.dispatch(UiEvent::SetAgentField(
                    "ui".into(),
                    "workflow_instructions".into(),
                    input.read(cx).as_str().to_owned().into(),
                ));
                cx.notify();
            },
        ));
        subscriptions.push(cx.subscribe(
            &settings_ui_description_input,
            |this, input, _: &TextChanged, cx| {
                this.controller.dispatch(UiEvent::SetAgentField(
                    "ui".into(),
                    "ui_description".into(),
                    input.read(cx).as_str().to_owned().into(),
                ));
                cx.notify();
            },
        ));
        subscriptions.push(cx.subscribe(
            &settings_raw_input,
            |this, input, _: &TextChanged, cx| {
                this.controller
                    .dispatch(UiEvent::SetAgentRaw(input.read(cx).as_str().to_owned()));
                cx.notify();
            },
        ));
        subscriptions.push(
            cx.subscribe(&file_input, |this, input, _: &TextChanged, cx| {
                this.controller
                    .dispatch(UiEvent::SetNewFileDraft(input.read(cx).as_str().to_owned()));
                cx.notify();
            }),
        );
        subscriptions.push(
            cx.subscribe(&search_input, |this, input, _: &TextChanged, cx| {
                this.controller
                    .dispatch(UiEvent::SetFileSearch(input.read(cx).as_str().to_owned()));
                cx.notify();
            }),
        );
        subscriptions.push(cx.subscribe(
            &session_search_input,
            |this, input, _: &TextChanged, cx| {
                this.controller
                    .dispatch(UiEvent::SetSessionQuery(input.read(cx).as_str().to_owned()));
                cx.notify();
            },
        ));
        subscriptions.push(
            cx.subscribe(&web_input, |this, input, _: &TextChanged, cx| {
                this.controller
                    .dispatch(UiEvent::SetWebUrl(input.read(cx).as_str().to_owned()));
                cx.notify();
            }),
        );
        subscriptions.push(
            cx.subscribe(&editor_input, |this, input, _: &TextChanged, cx| {
                this.controller
                    .dispatch(UiEvent::SetEditorText(input.read(cx).as_str().to_owned()));
                cx.notify();
            }),
        );
        subscriptions.push(
            cx.subscribe(&admin_input, |this, input, _: &TextChanged, cx| {
                this.controller
                    .dispatch(UiEvent::SetAdminDraft(input.read(cx).as_str().to_owned()));
                cx.notify();
            }),
        );
        subscriptions.push(cx.subscribe(
            &admin_target_input,
            |this, input, _: &TextChanged, cx| {
                this.controller
                    .dispatch(UiEvent::SetAdminTarget(input.read(cx).as_str().to_owned()));
                cx.notify();
            },
        ));
        subscriptions.push(
            cx.subscribe(&admin_run_input, |this, input, _: &TextChanged, cx| {
                this.controller.dispatch(UiEvent::SetAdminRunInput(
                    input.read(cx).as_str().to_owned(),
                ));
                cx.notify();
            }),
        );
        subscriptions.push(
            cx.subscribe(&credential_input, |this, input, _: &TextChanged, cx| {
                this.controller.dispatch(UiEvent::SetCredentialDraft(
                    input.read(cx).as_str().to_owned(),
                ));
                cx.notify();
            }),
        );
        admin_input.update(cx, |input, cx| {
            input.emplace(&controller.state.admin_draft, cx)
        });
        chat_input.update(cx, |input, cx| {
            input.emplace(&controller.state.chat_draft, cx)
        });
        let poll_task = cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(100))
                    .await;
                if this
                    .update(cx, |view, cx| {
                        if view.controller.poll() {
                            if view
                                .controller
                                .state
                                .terminals
                                .get(&view.controller.state.active_session_id)
                                .is_some_and(|terminal| terminal.open && terminal.auto_scroll)
                            {
                                view.terminal_scroll.scroll_to_bottom();
                            }
                            cx.notify();
                        }
                    })
                    .is_err()
                {
                    break;
                }
            }
        });
        let left_width = controller.state.left_width;
        let editor_width = controller.state.editor_width;
        Self {
            controller,
            chat_input,
            terminal_input,
            terminal_scroll: ScrollHandle::new(),
            agent_search_input,
            model_search_input,
            settings_search_input,
            settings_name_input,
            settings_category_input,
            settings_description_input,
            settings_workflow_input,
            settings_ui_description_input,
            settings_raw_input,
            file_input,
            search_input,
            session_search_input,
            web_input,
            editor_input,
            admin_input,
            admin_target_input,
            admin_run_input,
            credential_input,
            _subscriptions: subscriptions,
            _poll_task: poll_task,
            left_width,
            editor_width,
            dragging: None,
            dragging_tab: None,
            dragging_document: None,
            dragging_float: None,
            confirm_delete: None,
            model_picker_open: false,
            model_query: String::new(),
            model_cursor: 0,
            agent_picker_open: false,
            agent_query: String::new(),
            agent_cursor: 0,
            reasoning_open: false,
            prompts_open: false,
            settings_categories_open: false,
            dragging_terminal: None,
        }
    }

    fn enabled_switch(
        &self,
        id: String,
        enabled: bool,
        event: UiEvent,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        div()
            .id(id)
            .flex()
            .items_center()
            .gap_1()
            .cursor_pointer()
            .child("Enabled")
            .child(
                div()
                    .w(px(40.))
                    .h(px(24.))
                    .p_1()
                    .rounded_lg()
                    .bg(rgb(theme::resolve(if enabled {
                        theme::PRIMARY
                    } else {
                        theme::BORDER
                    })))
                    .flex()
                    .items_center()
                    .child(
                        div()
                            .w(px(16.))
                            .h(px(16.))
                            .rounded_lg()
                            .bg(rgb(theme::resolve(theme::SURFACE)))
                            .when(enabled, |knob| knob.ml_auto()),
                    ),
            )
            .on_click(cx.listener(move |this, _, _, cx| {
                this.controller.dispatch(event.clone());
                cx.notify();
            }))
    }

    fn button(
        &self,
        id: String,
        label: String,
        event: UiEvent,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let selected = matches!(&event, UiEvent::SelectCenter(view) if *view == self.controller.state.center_view);
        div()
            .id(id)
            .px_2()
            .py_1()
            .rounded_md()
            .cursor_pointer()
            .bg(rgb(theme::resolve(if selected {
                theme::PRIMARY_SOFT
            } else {
                theme::SURFACE
            })))
            .text_color(rgb(theme::resolve(if selected {
                theme::PRIMARY
            } else {
                theme::TEXT
            })))
            .hover(|s| s.bg(rgb(theme::resolve(theme::PRIMARY_SOFT))))
            .child(label)
            .on_click(cx.listener(move |this, _, _, cx| {
                this.controller.dispatch(event.clone());
                if matches!(
                    &event,
                    UiEvent::OpenAgentSettings | UiEvent::SelectAgentSection(_)
                ) {
                    this.sync_agent_settings_inputs(cx);
                }
                if matches!(&event, UiEvent::WebBack | UiEvent::WebForward) {
                    let url = this.controller.state.web_url.clone();
                    this.web_input
                        .update(cx, |input, cx| input.emplace(&url, cx));
                }
                if matches!(&event, UiEvent::SelectAdmin(_)) {
                    this.admin_input.update(cx, |input, cx| {
                        input.emplace(&this.controller.state.admin_draft, cx)
                    });
                }
                if matches!(&event, UiEvent::SaveCredential)
                    && this.controller.state.credential_draft.is_empty()
                {
                    this.credential_input
                        .update(cx, |input, cx| input.emplace("", cx));
                }
                if matches!(&event, UiEvent::RevertFile | UiEvent::FormatDocument) {
                    if let Some(doc) = &this.controller.state.editor {
                        this.editor_input
                            .update(cx, |input, cx| input.emplace(&doc.text, cx));
                    }
                }
                if matches!(
                    &event,
                    UiEvent::CloseEditor | UiEvent::DiscardAndCloseEditor | UiEvent::SelectStore(_)
                ) {
                    let text = this
                        .controller
                        .state
                        .editor
                        .as_ref()
                        .map(|d| d.text.as_str())
                        .unwrap_or("");
                    this.editor_input
                        .update(cx, |input, cx| input.emplace(text, cx));
                }
                if matches!(&event, UiEvent::NewChat | UiEvent::SelectSession(_)) {
                    let draft = this.controller.state.chat_draft.clone();
                    this.chat_input
                        .update(cx, |input, cx| input.emplace(&draft, cx));
                    let command = this
                        .controller
                        .state
                        .terminals
                        .get(&this.controller.state.active_session_id)
                        .map(|terminal| terminal.input.as_str())
                        .unwrap_or("")
                        .to_owned();
                    this.terminal_input
                        .update(cx, |input, cx| input.emplace(&command, cx));
                }
                if matches!(&event, UiEvent::SetAgentSettingsCategory(_)) {
                    this.settings_categories_open = false;
                }
                cx.notify();
            }))
    }

    fn icon_action(
        &self,
        id: String,
        icon: &'static [u8],
        label: &'static str,
        event: UiEvent,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let active = matches!(
            &event,
            UiEvent::SetMemoryEnabled(false)
                | UiEvent::SetShowReasoning(false)
                | UiEvent::SetAttachOpenFile(false)
        );
        let tone = if active {
            theme::PRIMARY
        } else {
            theme::TEXT_MUTED
        };
        div()
            .id(id)
            .flex()
            .items_center()
            .gap_1()
            .p_1()
            .rounded_md()
            .cursor_pointer()
            .text_color(rgb(theme::resolve(tone)))
            .hover(|style| style.bg(rgb(theme::resolve(theme::PRIMARY_SOFT))))
            .child(bitmap_icon(icon, 18.))
            .child(div().text_xs().child(label))
            .on_click(cx.listener(move |this, _, _, cx| {
                this.controller.dispatch(event.clone());
                cx.notify();
            }))
    }

    fn toolbar_action(
        &self,
        id: &'static str,
        icon: &'static [u8],
        event: UiEvent,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        div()
            .id(id)
            .w(px(29.))
            .h(px(29.))
            .flex()
            .items_center()
            .justify_center()
            .rounded_md()
            .cursor_pointer()
            .hover(|style| style.bg(rgb(theme::resolve(theme::PRIMARY_SOFT))))
            .child(bitmap_icon(icon, 18.))
            .on_click(cx.listener(move |this, _, _, cx| {
                this.controller.dispatch(event.clone());
                if matches!(&event, UiEvent::NewChat) {
                    let draft = this.controller.state.chat_draft.clone();
                    this.chat_input
                        .update(cx, |input, cx| input.emplace(&draft, cx));
                }
                cx.notify();
            }))
    }

    fn open_file(&mut self, path: String, cx: &mut Context<Self>) {
        self.controller.dispatch(UiEvent::OpenFile(path));
        if let Some(doc) = &self.controller.state.editor {
            self.editor_input
                .update(cx, |input, cx| input.emplace(&doc.text, cx));
        }
        cx.notify();
    }

    fn left_panel(&self, width: f32, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let mut panel = div()
            .id("left-panel")
            .w(px(width))
            .flex_shrink_0()
            .h_full()
            .min_h(px(0.))
            .flex()
            .flex_col()
            .gap_2()
            .p_3()
            .overflow_y_scroll()
            .bg(rgb(theme::resolve(theme::SURFACE)))
            .child(self.controller.state.left_view.label());
        match self.controller.state.left_view {
            LeftView::Files => {
                panel = panel.child(
                    div()
                        .id("open-workspace")
                        .cursor_pointer()
                        .child("Open workspace folder…")
                        .on_click(cx.listener(|this, _, _, cx| {
                            if let Some(path) = rfd::FileDialog::new().pick_folder() {
                                this.controller.dispatch(UiEvent::AddWorkspace(path));
                                cx.notify();
                            }
                        })),
                );
                panel = panel.child("Search files").child(
                    text_input("file-search")
                        .state(self.search_input.downgrade())
                        .w_full()
                        .p_1()
                        .border_1()
                        .border_color(rgb(theme::resolve(theme::BORDER))),
                );
                panel = panel.child(self.button(
                    "search-all".into(),
                    "Search all folders".into(),
                    UiEvent::SearchFiles,
                    cx,
                ));
                panel = panel.child("New file or folder").child(
                    div()
                        .flex()
                        .gap_1()
                        .child(
                            text_input("new-file")
                                .state(self.file_input.downgrade())
                                .w(px(165.))
                                .p_1()
                                .border_1()
                                .border_color(rgb(theme::resolve(theme::BORDER))),
                        )
                        .child(
                            div()
                                .id("create-file")
                                .cursor_pointer()
                                .child("+")
                                .on_click(cx.listener(|this, _, _, cx| {
                                    let path =
                                        this.controller.state.new_file_draft.trim().to_owned();
                                    if !path.is_empty() {
                                        this.controller.dispatch(UiEvent::CreateFile(path));
                                        this.file_input
                                            .update(cx, |input, cx| input.emplace("", cx));
                                        if let Some(doc) = &this.controller.state.editor {
                                            this.editor_input.update(cx, |input, cx| {
                                                input.emplace(&doc.text, cx)
                                            });
                                        }
                                    }
                                    cx.notify();
                                })),
                        ),
                );
                panel = panel.child(
                    div()
                        .flex()
                        .gap_2()
                        .child(
                            div()
                                .id("new-folder")
                                .cursor_pointer()
                                .child("▣ Folder")
                                .on_click(cx.listener(|this, _, _, cx| {
                                    let path =
                                        this.controller.state.new_file_draft.trim().to_owned();
                                    if !path.is_empty() {
                                        this.controller.dispatch(UiEvent::CreateFolder(path));
                                    }
                                    cx.notify();
                                })),
                        )
                        .child(
                            div()
                                .id("rename-file")
                                .cursor_pointer()
                                .child("Rename")
                                .on_click(cx.listener(|this, _, _, cx| {
                                    if let Some(from) = this.controller.state.selected_path.clone()
                                    {
                                        let to =
                                            this.controller.state.new_file_draft.trim().to_owned();
                                        if !to.is_empty() {
                                            this.controller.dispatch(UiEvent::RenameFile(from, to));
                                        }
                                    }
                                    cx.notify();
                                })),
                        )
                        .child(
                            div()
                                .id("delete-file")
                                .cursor_pointer()
                                .child("Delete")
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.confirm_delete =
                                        this.controller.state.selected_path.clone();
                                    cx.notify();
                                })),
                        ),
                );
                if let Some(path) = &self.confirm_delete {
                    panel = panel
                        .child(format!("Delete {path}?"))
                        .child(
                            div()
                                .id("confirm-delete")
                                .cursor_pointer()
                                .child("Confirm delete")
                                .on_click(cx.listener(|this, _, _, cx| {
                                    if let Some(path) = this.confirm_delete.take() {
                                        this.controller.dispatch(UiEvent::DeleteFile(path));
                                    }
                                    cx.notify();
                                })),
                        )
                        .child(
                            div()
                                .id("cancel-delete")
                                .cursor_pointer()
                                .child("Cancel")
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.confirm_delete = None;
                                    cx.notify();
                                })),
                        );
                }
                panel = panel.child(
                    div()
                        .flex()
                        .gap_2()
                        .child(
                            div()
                                .id("import-file")
                                .cursor_pointer()
                                .child("⇧ Import")
                                .on_click(cx.listener(|this, _, _, cx| {
                                    if let Some(source) = rfd::FileDialog::new().pick_file() {
                                        if let Some(name) = source.file_name() {
                                            let folder = this.controller.selected_folder();
                                            let destination = if folder.is_empty() {
                                                name.to_string_lossy().into_owned()
                                            } else {
                                                format!("{folder}/{}", name.to_string_lossy())
                                            };
                                            this.controller
                                                .dispatch(UiEvent::ImportFile(source, destination));
                                        }
                                    }
                                    cx.notify();
                                })),
                        )
                        .child(
                            div()
                                .id("export-file")
                                .cursor_pointer()
                                .child("⇩ Export")
                                .on_click(cx.listener(|this, _, _, cx| {
                                    if let Some(source) =
                                        this.controller.state.selected_path.clone()
                                    {
                                        if let Some(destination) =
                                            rfd::FileDialog::new().save_file()
                                        {
                                            this.controller
                                                .dispatch(UiEvent::ExportFile(source, destination));
                                        }
                                    }
                                    cx.notify();
                                })),
                        ),
                );
                let roots = self.controller.logical_roots();
                let mut rows: Vec<gpui::AnyElement> = Vec::new();
                if let Some(root) = roots
                    .iter()
                    .find(|root| root.id == self.controller.state.active_logical_root)
                {
                    rows.push(
                        div()
                            .flex()
                            .items_center()
                            .gap_1()
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .child(bitmap_icon(
                                include_bytes!("../common/icons/folder.svg"),
                                17.,
                            ))
                            .child(root.label.clone())
                            .into_any_element(),
                    );
                }
                let visible = if self.controller.state.search_active {
                    self.controller.state.search_results.clone()
                } else {
                    self.controller.state.tree.clone()
                };
                for node in visible {
                    let label = if node.entry.is_dir {
                        format!(
                            "{} {}",
                            if node.expanded { "v" } else { ">" },
                            node.entry.name
                        )
                    } else {
                        node.entry.name.clone()
                    };
                    let icon: &'static [u8] = if node.entry.is_dir {
                        include_bytes!("../common/icons/folder.svg")
                    } else {
                        include_bytes!("../common/icons/file.svg")
                    };
                    let path = node.entry.path.clone();
                    let is_dir = node.entry.is_dir;
                    rows.push(
                        div()
                            .id(format!("file:{path}"))
                            .flex()
                            .items_center()
                            .gap_1()
                            .px_1()
                            .pl(px(4.0 + node.depth as f32 * 12.0))
                            .py_1()
                            .cursor_pointer()
                            .hover(|s| s.bg(rgb(theme::resolve(theme::PRIMARY_SOFT))))
                            .child(bitmap_icon(icon, 16.))
                            .child(label)
                            .on_click(cx.listener(move |this, _, _, cx| {
                                if is_dir {
                                    this.controller.dispatch(UiEvent::SelectPath(path.clone()));
                                    if this.controller.state.search_active {
                                        this.controller
                                            .dispatch(UiEvent::SetFileSearch(String::new()));
                                        this.search_input
                                            .update(cx, |input, cx| input.emplace("", cx));
                                    }
                                    this.controller
                                        .dispatch(UiEvent::ToggleFolder(path.clone()));
                                    cx.notify();
                                } else {
                                    this.open_file(path.clone(), cx);
                                }
                            }))
                            .into_any_element(),
                    );
                }
                for root in roots
                    .into_iter()
                    .filter(|root| root.id != self.controller.state.active_logical_root)
                {
                    rows.push(
                        self.button(
                            format!("logical-root:{}", root.id),
                            format!(
                                "{}{}",
                                root.label,
                                if root.read_only { " · Read only" } else { "" }
                            ),
                            UiEvent::SelectLogicalRoot(root.id),
                            cx,
                        )
                        .into_any_element(),
                    );
                }
                panel = panel.child(
                    div()
                        .id("file-list")
                        .flex_1()
                        .overflow_y_scroll()
                        .children(rows),
                );
            }
            LeftView::Agents => {
                for agent in self
                    .controller
                    .config
                    .agents
                    .agents
                    .iter()
                    .filter(|a| a.enabled != Some(false) && a.hidden != Some(true))
                {
                    let visual = AgentVisual::from_agent(agent, &self.controller.config_path);
                    let avatar = agent_avatar(&visual, 25.);
                    let agent_id = agent.id.clone();
                    panel = panel.child(
                        div()
                            .id(format!("agent-row:{agent_id}"))
                            .w_full()
                            .flex()
                            .items_center()
                            .gap_2()
                            .p_2()
                            .rounded_md()
                            .cursor_pointer()
                            .bg(rgb(theme::resolve(
                                if self.controller.state.agent_id == agent_id {
                                    theme::PRIMARY_SOFT
                                } else {
                                    theme::SURFACE
                                },
                            )))
                            .hover(|s| s.bg(rgb(theme::resolve(theme::PRIMARY_SOFT))))
                            .child(avatar)
                            .child(
                                div()
                                    .min_w(px(0.))
                                    .flex()
                                    .items_center()
                                    .gap_1()
                                    .child(agent.name.clone())
                                    .child(
                                        div()
                                            .text_color(rgb(theme::resolve(theme::TEXT_MUTED)))
                                            .text_xs()
                                            .child(format!("· {}", visual.category)),
                                    ),
                            )
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.controller
                                    .dispatch(UiEvent::SelectAgent(agent_id.clone()));
                                let draft = this.controller.state.chat_draft.clone();
                                this.chat_input
                                    .update(cx, |input, cx| input.emplace(&draft, cx));
                                cx.notify();
                            })),
                    );
                }
                panel = panel.child("Model");
                panel = panel.child(
                    div()
                        .id("model:auto")
                        .w_full()
                        .flex()
                        .items_center()
                        .gap_2()
                        .p_2()
                        .rounded_md()
                        .cursor_pointer()
                        .child(bitmap_icon(
                            include_bytes!("../common/icons/agents.svg"),
                            18.,
                        ))
                        .child("Auto · agent default")
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.controller
                                .dispatch(UiEvent::SelectModel("auto".into()));
                            cx.notify();
                        })),
                );
                if let Some(models) = &self.controller.config.models {
                    for model in &models.models {
                        let id = model.id.clone().unwrap_or_else(|| model.name.clone());
                        let icon = crate::ui::common::model_picker::provider_icon_path(
                            &self.controller.config,
                            &self.controller.config_path,
                            &model.provider,
                        )
                        .map(|path| bitmap_asset(&path, 18.))
                        .unwrap_or_else(|| {
                            bitmap_icon(include_bytes!("../common/icons/file.svg"), 18.)
                        });
                        panel = panel.child(
                            div()
                                .id(format!("model:{id}"))
                                .w_full()
                                .flex()
                                .items_center()
                                .gap_2()
                                .p_2()
                                .rounded_md()
                                .cursor_pointer()
                                .bg(rgb(theme::resolve(
                                    if self.controller.state.selected_model == id {
                                        theme::PRIMARY_SOFT
                                    } else {
                                        theme::SURFACE
                                    },
                                )))
                                .child(icon)
                                .child(model.name.clone())
                                .child(
                                    div()
                                        .text_xs()
                                        .text_color(rgb(theme::resolve(theme::TEXT_MUTED)))
                                        .child(format!("· {}", model.provider)),
                                )
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.controller.dispatch(UiEvent::SelectModel(id.clone()));
                                    cx.notify();
                                })),
                        );
                    }
                }
            }
            LeftView::Sessions => {
                panel = panel.child(
                    text_input("session-search")
                        .state(self.session_search_input.downgrade())
                        .placeholder("⌕ Search history")
                        .w_full()
                        .p_1()
                        .border_1()
                        .border_color(rgb(theme::resolve(theme::BORDER))),
                );
                let query = self.controller.state.session_query.to_lowercase();
                for session in self.controller.state.sessions.iter().rev() {
                    if session.messages.is_empty()
                        && session.id != self.controller.state.active_session_id
                    {
                        continue;
                    }
                    if !query.is_empty()
                        && !session.title.to_lowercase().contains(&query)
                        && !session
                            .messages
                            .iter()
                            .any(|m| m.text.to_lowercase().contains(&query))
                    {
                        continue;
                    }
                    panel = panel.child(self.button(
                        format!("history:{}", session.id),
                        format!("{}\n{}", session.title, session.agent_id),
                        UiEvent::SelectSession(session.id.clone()),
                        cx,
                    ));
                }
            }
            LeftView::Connectors => {
                panel = panel.child("Configured connectors");
                for connector in &self.controller.config.connectors {
                    let label = connector
                        .name
                        .as_deref()
                        .or(connector.id.as_deref())
                        .unwrap_or("Connector");
                    panel = panel.child(self.button(
                        format!("connector:{label}"),
                        label.to_owned(),
                        UiEvent::SelectConnector(connector.id.clone().unwrap_or_default()),
                        cx,
                    ));
                }
            }
            LeftView::Settings => {
                for section in AdminSection::ALL {
                    panel = panel.child(self.button(
                        format!("admin:{}", section.label()),
                        section.label().into(),
                        UiEvent::SelectAdmin(section),
                        cx,
                    ));
                }
                panel = panel.child(self.button(
                    "admin:members".into(),
                    "Members".into(),
                    UiEvent::Unsupported("members management"),
                    cx,
                ));
                panel = panel.child(self.button(
                    "admin:plugins".into(),
                    "Plugins".into(),
                    UiEvent::Unsupported("plugin management"),
                    cx,
                ));
                panel = panel.child("Installed plugins");
                for plugin in crate::plugins::all() {
                    panel = panel.child(plugin.id);
                }
            }
        }
        panel
    }

    fn agent_settings_panel(&self, width: f32, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let settings = self
            .controller
            .state
            .agent_settings
            .as_ref()
            .expect("settings open");
        let mut navigation = div()
            .id("agent-settings-navigation")
            .w(px(156.))
            .flex_shrink_0()
            .min_h(px(0.))
            .overflow_y_scroll()
            .flex()
            .flex_col()
            .gap_1()
            .p_2()
            .border_1()
            .border_color(rgb(theme::resolve(theme::BORDER)))
            .bg(rgb(theme::resolve(theme::SURFACE)));
        for section in AgentSection::ALL {
            navigation = navigation.child(
                div()
                    .id(format!("agent-section:{}", section.label()))
                    .p_2()
                    .rounded_md()
                    .cursor_pointer()
                    .bg(rgb(theme::resolve(if settings.section == section {
                        theme::PRIMARY_SOFT
                    } else {
                        theme::SURFACE
                    })))
                    .text_color(rgb(theme::resolve(if settings.section == section {
                        theme::PRIMARY
                    } else {
                        theme::TEXT
                    })))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(bitmap_icon(section.icon(), 17.))
                            .child(section.label()),
                    )
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.controller
                            .dispatch(UiEvent::SelectAgentSection(section));
                        this.sync_agent_settings_inputs(cx);
                        cx.notify();
                    })),
            );
        }
        let mut content = div()
            .id("agent-settings-content")
            .flex_1()
            .min_w(px(0.))
            .min_h(px(0.))
            .overflow_y_scroll()
            .flex()
            .flex_col()
            .gap_2()
            .p_3();
        match settings.section {
            AgentSection::Agent => {
                let visual = AgentVisual::from_agent(&settings.draft, &self.controller.config_path);
                content = content
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_3()
                            .p_3()
                            .rounded_md()
                            .bg(rgb(theme::resolve(theme::SURFACE_MUTED)))
                            .child(agent_avatar(&visual, 48.))
                            .child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .child(settings.draft.name.clone())
                                    .child(
                                        div()
                                            .text_xs()
                                            .text_color(rgb(theme::resolve(theme::TEXT_MUTED)))
                                            .child(visual.category),
                                    ),
                            ),
                    )
                    .child("Name")
                    .child(
                        text_input("agent-name")
                            .state(self.settings_name_input.downgrade())
                            .w_full()
                            .p_2()
                            .border_1()
                            .border_color(rgb(theme::resolve(theme::BORDER))),
                    )
                    .child("Category")
                    .child(
                        text_input("agent-category")
                            .state(self.settings_category_input.downgrade())
                            .w_full()
                            .p_2()
                            .border_1()
                            .border_color(rgb(theme::resolve(theme::BORDER))),
                    )
                    .child(
                        div()
                            .flex()
                            .gap_2()
                            .child(self.button(
                                "agent-enabled".into(),
                                format!("Enabled: {}", settings.draft.enabled != Some(false)),
                                UiEvent::SetAgentEnabled(settings.draft.enabled == Some(false)),
                                cx,
                            ))
                            .child(self.button(
                                "agent-default".into(),
                                format!("Default: {}", settings.draft.default == Some(true)),
                                UiEvent::SetAgentDefault(settings.draft.default != Some(true)),
                                cx,
                            )),
                    )
                    .child("Description")
                    .child(
                        editable_text("agent-description")
                            .state(self.settings_description_input.downgrade())
                            .w_full()
                            .h(px(85.))
                            .p_2()
                            .border_1()
                            .border_color(rgb(theme::resolve(theme::BORDER))),
                    )
                    .child(format!("Agent ID: {}", settings.agent_id));
            }
            AgentSection::Tools | AgentSection::Skills => {
                let tools = settings.section == AgentSection::Tools;
                content = content.child(
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .child(
                            text_input("agent-settings-search")
                                .state(self.settings_search_input.downgrade())
                                .placeholder(if tools {
                                    "Search tools"
                                } else {
                                    "Search skills"
                                })
                                .flex_1()
                                .min_w(px(0.))
                                .p_2()
                                .border_1()
                                .border_color(rgb(theme::resolve(theme::BORDER))),
                        )
                        .child(
                            div()
                                .id("agent-category-filter")
                                .p_2()
                                .rounded_md()
                                .cursor_pointer()
                                .border_1()
                                .border_color(rgb(theme::resolve(theme::BORDER)))
                                .child(
                                    div()
                                        .flex()
                                        .items_center()
                                        .gap_1()
                                        .child(settings.category.clone())
                                        .child(bitmap_icon(
                                            include_bytes!("../common/icons/chevron_down.svg"),
                                            13.,
                                        )),
                                )
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.settings_categories_open = !this.settings_categories_open;
                                    cx.notify();
                                })),
                        ),
                );
                if !tools {
                    content = content.child(
                        self.button(
                            "skills-enabled-only".into(),
                            if settings.enabled_only {
                                "Enabled only ✓"
                            } else {
                                "All / Enabled"
                            }
                            .into(),
                            UiEvent::SetAgentSettingsEnabledOnly(!settings.enabled_only),
                            cx,
                        ),
                    );
                }
                let entries = if tools {
                    agent_settings::tool_catalog(
                        &self.controller.config,
                        &self.controller.config_path,
                        settings,
                    )
                } else {
                    agent_settings::skill_catalog(
                        &self.controller.config,
                        &self.controller.config_path,
                        settings,
                    )
                };
                let mut categories = std::collections::BTreeSet::new();
                for entry in &entries {
                    categories.insert(entry.category.clone());
                }
                let mut categories_row = div().flex().flex_wrap().gap_1();
                categories_row = categories_row.child(self.button(
                    "category:all".into(),
                    "All".into(),
                    UiEvent::SetAgentSettingsCategory("All".into()),
                    cx,
                ));
                for category in categories {
                    categories_row = categories_row.child(self.button(
                        format!("category:{category}"),
                        category.clone(),
                        UiEvent::SetAgentSettingsCategory(category),
                        cx,
                    ));
                }
                if self.settings_categories_open {
                    content = content.child(categories_row);
                }
                let query = settings.query.to_lowercase();
                for entry in entries {
                    if settings.category != "All" && entry.category != settings.category {
                        continue;
                    }
                    if !query.is_empty()
                        && !format!("{} {} {}", entry.name, entry.category, entry.description)
                            .to_lowercase()
                            .contains(&query)
                    {
                        continue;
                    }
                    if !tools && settings.enabled_only && !settings.skill_enabled(&entry.id) {
                        continue;
                    }
                    let mut row = div()
                        .p_2()
                        .rounded_md()
                        .bg(rgb(theme::resolve(theme::SURFACE_MUTED)))
                        .flex()
                        .items_center()
                        .gap_2()
                        .child(bitmap_icon(
                            if tools {
                                include_bytes!("../common/icons/settings.svg")
                            } else {
                                include_bytes!("../common/icons/prompt.svg")
                            },
                            20.,
                        ))
                        .child(
                            div()
                                .flex_1()
                                .min_w(px(0.))
                                .flex()
                                .flex_col()
                                .child(entry.name.clone())
                                .child(
                                    div()
                                        .text_xs()
                                        .text_color(rgb(theme::resolve(theme::TEXT_MUTED)))
                                        .child(format!(
                                            "{} · {}  {}",
                                            entry.category, entry.source, entry.description
                                        )),
                                ),
                        );
                    if tools {
                        let enabled = settings.tool_value(&entry.id, "enabled", true);
                        let approve = settings.tool_value(&entry.id, "autoapprove", false);
                        row = row.child(
                            div()
                                .flex()
                                .flex_col()
                                .gap_1()
                                .child(self.button(
                                    format!("tool-enabled:{}", entry.id),
                                    format!("Enabled {}", if enabled { "✓" } else { "○" }),
                                    UiEvent::SetAgentTool(
                                        entry.id.clone(),
                                        "enabled".into(),
                                        !enabled,
                                    ),
                                    cx,
                                ))
                                .child(self.button(
                                    format!("tool-approve:{}", entry.id),
                                    format!("Autoapprove {}", if approve { "✓" } else { "○" }),
                                    UiEvent::SetAgentTool(
                                        entry.id.clone(),
                                        "autoapprove".into(),
                                        !approve,
                                    ),
                                    cx,
                                )),
                        );
                    } else {
                        let enabled = settings.skill_enabled(&entry.id);
                        row = row.child(self.button(
                            format!("skill-enabled:{}", entry.id),
                            format!("Enabled {}", if enabled { "✓" } else { "○" }),
                            UiEvent::SetAgentSkill(entry.id.clone(), !enabled),
                            cx,
                        ));
                    }
                    content = content.child(row);
                }
            }
            AgentSection::Ui => {
                let enabled = settings
                    .field("ui", "enabled")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                content = content
                    .child(self.button(
                        "agent-ui-enabled".into(),
                        format!("UI Enabled {}", if enabled { "✓" } else { "○" }),
                        UiEvent::SetAgentField("ui".into(), "enabled".into(), (!enabled).into()),
                        cx,
                    ))
                    .child("Renderer: Auto")
                    .child("Workflow Instructions")
                    .child(
                        editable_text("workflow-instructions")
                            .state(self.settings_workflow_input.downgrade())
                            .w_full()
                            .h(px(100.))
                            .p_2()
                            .border_1()
                            .border_color(rgb(theme::resolve(theme::BORDER))),
                    )
                    .child("UI Description")
                    .child(
                        editable_text("ui-description")
                            .state(self.settings_ui_description_input.downgrade())
                            .w_full()
                            .h(px(100.))
                            .p_2()
                            .border_1()
                            .border_color(rgb(theme::resolve(theme::BORDER))),
                    )
                    .child("Advanced options");
                for (key, label) in [
                    ("schema", "Include schema"),
                    ("examples", "Include examples"),
                ] {
                    let value = settings
                        .field("ui", key)
                        .and_then(|v| v.as_bool())
                        .unwrap_or(true);
                    content = content.child(self.button(
                        format!("ui-{key}"),
                        format!("{label} {}", if value { "✓" } else { "○" }),
                        UiEvent::SetAgentField("ui".into(), key.into(), (!value).into()),
                        cx,
                    ));
                }
            }
            AgentSection::Memory => {
                let mut panels = div().flex().flex_wrap().gap_2();
                for (scope, title, default_store) in [
                    ("personal", "Personal Memory", true),
                    ("shared", "Shared Memory", false),
                ] {
                    let key = if scope == "personal" {
                        "type"
                    } else {
                        "shared_type"
                    };
                    let current = settings
                        .field("memory", key)
                        .and_then(|v| v.as_str())
                        .unwrap_or(if scope == "personal" {
                            "adk-sqlite"
                        } else {
                            ""
                        });
                    let mut card = div()
                        .min_w(px(((width - 190.) / 2.).max(150.)))
                        .flex_1()
                        .p_2()
                        .rounded_md()
                        .bg(rgb(theme::resolve(theme::SURFACE_MUTED)))
                        .flex()
                        .flex_col()
                        .gap_1()
                        .child(title)
                        .child(format!(
                            "Backend: {}",
                            if current.is_empty() {
                                "Same backend"
                            } else {
                                current
                            }
                        ));
                    for memory in &self.controller.config.memory.memory {
                        let id = memory.id.as_deref().unwrap_or(&memory.name);
                        card = card.child(self.button(
                            format!("{scope}-backend-{id}"),
                            memory.name.clone(),
                            UiEvent::SetAgentField(
                                "memory".into(),
                                key.into(),
                                id.to_owned().into(),
                            ),
                            cx,
                        ));
                    }
                    if scope == "shared" {
                        card = card.child(self.button(
                            "same-backend".into(),
                            "Same backend".into(),
                            UiEvent::SetAgentField("memory".into(), key.into(), "".into()),
                            cx,
                        ));
                    }
                    for (field, label, default) in [
                        ("auto_retrieval", "Auto Recall", true),
                        ("auto_store", "Auto Remember", default_store),
                    ] {
                        let value = settings.memory_value(scope, field, default);
                        card = card.child(self.button(
                            format!("{scope}-{field}"),
                            format!("{label} {}", if value { "✓" } else { "○" }),
                            UiEvent::SetAgentMemory(scope.into(), field.into(), !value),
                            cx,
                        ));
                    }
                    panels = panels.child(card);
                }
                content = content
                    .child(panels)
                    .child("Memory Files")
                    .child("Memory files are available in the selected workspace file store.");
            }
            _ => {
                content = content
                    .child(format!("{} configuration (YAML)", settings.section.label()))
                    .child(
                        editable_text("agent-section-yaml")
                            .state(self.settings_raw_input.downgrade())
                            .w_full()
                            .h(px(320.))
                            .p_2()
                            .font_family("Consolas")
                            .border_1()
                            .border_color(rgb(theme::resolve(theme::BORDER))),
                    )
                    .child(self.button(
                        "apply-agent-section".into(),
                        "Apply section".into(),
                        UiEvent::ApplyAgentRaw,
                        cx,
                    ));
            }
        }
        div()
            .flex_1()
            .min_h(px(0.))
            .flex()
            .flex_col()
            .gap_2()
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(format!(
                        "{} · {}",
                        settings.draft.name,
                        settings.section.label()
                    ))
                    .when(settings.dirty, |row| row.child("Unsaved"))
                    .child(div().flex_1())
                    .child(self.button(
                        "save-agent-settings".into(),
                        "Save".into(),
                        UiEvent::SaveAgentSettings,
                        cx,
                    ))
                    .child(self.button(
                        "close-agent-settings".into(),
                        "Close".into(),
                        UiEvent::CloseAgentSettings,
                        cx,
                    ))
                    .when(settings.dirty, |row| {
                        row.child(self.button(
                            "discard-agent-settings".into(),
                            "Discard".into(),
                            UiEvent::DiscardAgentSettings,
                            cx,
                        ))
                    }),
            )
            .child(
                div()
                    .flex_1()
                    .min_h(px(0.))
                    .flex()
                    .gap_2()
                    .child(navigation)
                    .child(content),
            )
    }

    fn center_panel(&self, width: f32, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let mut panel = div()
            .w(px(width))
            .flex_shrink_0()
            .min_w(px(0.))
            .h_full()
            .min_h(px(0.))
            .flex()
            .flex_col()
            .gap_2()
            .p_4()
            .bg(rgb(theme::resolve(theme::SURFACE)));
        let mut tabs = div().flex().gap_1();
        for tab in self.controller.state.center_tabs.clone() {
            let label = match tab {
                CenterView::Chat => "Chat",
                CenterView::Web => "Web",
                CenterView::Changes => "Changes",
                CenterView::Admin => "Admin",
            };
            tabs = tabs.child(
                div()
                    .id(format!("center-tab:{tab:?}"))
                    .px_2()
                    .py_1()
                    .cursor_pointer()
                    .bg(rgb(theme::resolve(
                        if self.controller.state.center_view == tab {
                            theme::PRIMARY_SOFT
                        } else {
                            theme::SURFACE
                        },
                    )))
                    .child(label)
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _, _, cx| {
                            this.dragging_tab = Some(tab);
                            cx.notify();
                        }),
                    )
                    .on_mouse_up(
                        MouseButton::Left,
                        cx.listener(move |this, _, _, cx| {
                            if let Some(source) = this.dragging_tab.take() {
                                this.controller
                                    .dispatch(UiEvent::MoveCenterTab(source, tab));
                            }
                            this.controller.dispatch(UiEvent::SelectCenter(tab));
                            cx.notify();
                        }),
                    ),
            );
        }
        panel = panel.child(tabs);
        if self.controller.state.center_view == CenterView::Chat {
            let mut tabs = div().flex().gap_1().overflow_hidden();
            for session in self
                .controller
                .state
                .sessions
                .iter()
                .rev()
                .filter(|session| {
                    !session.messages.is_empty()
                        || session.id == self.controller.state.active_session_id
                })
                .take(6)
            {
                let active = session.id == self.controller.state.active_session_id;
                let agent_name = self
                    .controller
                    .config
                    .agents
                    .agents
                    .iter()
                    .find(|agent| agent.id == session.agent_id)
                    .map(|agent| agent.name.as_str())
                    .unwrap_or(&session.agent_id);
                tabs = tabs.child(self.button(
                    format!("session:{}", session.id),
                    format!(
                        "{}{} · {}",
                        if active { "● " } else { "" },
                        session.title,
                        agent_name
                    ),
                    UiEvent::SelectSession(session.id.clone()),
                    cx,
                ));
            }
            panel = panel.child(tabs);
        }
        match self.controller.state.center_view {
            CenterView::Chat => {
                if let Some(agent) = self
                    .controller
                    .config
                    .agents
                    .agents
                    .iter()
                    .find(|a| a.id == self.controller.state.agent_id)
                {
                    let visual = AgentVisual::from_agent(agent, &self.controller.config_path);
                    let avatar = agent_avatar(&visual, 27.);
                    panel = panel.child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .pb_2()
                            .child(avatar)
                            .child(
                                div()
                                    .font_weight(gpui::FontWeight::SEMIBOLD)
                                    .child(agent.name.clone()),
                            )
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(rgb(theme::resolve(theme::TEXT_MUTED)))
                                    .child(format!("· {}", visual.category)),
                            )
                            .child(
                                div()
                                    .id("choose-chat-agent")
                                    .p_1()
                                    .cursor_pointer()
                                    .child(bitmap_icon(
                                        include_bytes!("../common/icons/chevron_down.svg"),
                                        14.,
                                    ))
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.agent_picker_open = !this.agent_picker_open;
                                        if this.agent_picker_open {
                                            this.agent_search_input
                                                .read(cx)
                                                .focus_handle(cx)
                                                .focus(window, cx);
                                        }
                                        cx.notify();
                                    })),
                            )
                            .child(self.toolbar_action(
                                "agent-chat",
                                include_bytes!("../common/icons/chat.svg"),
                                UiEvent::CloseAgentSettings,
                                cx,
                            ))
                            .child(self.toolbar_action(
                                "agent-new-chat",
                                include_bytes!("../common/icons/plus.svg"),
                                UiEvent::NewChat,
                                cx,
                            ))
                            .child(self.toolbar_action(
                                "agent-sessions",
                                include_bytes!("../common/icons/sessions.svg"),
                                UiEvent::SelectView(LeftView::Sessions),
                                cx,
                            ))
                            .child(
                                div()
                                    .id("agent-prompts")
                                    .w(px(29.))
                                    .h(px(29.))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .rounded_md()
                                    .cursor_pointer()
                                    .hover(|style| {
                                        style.bg(rgb(theme::resolve(theme::PRIMARY_SOFT)))
                                    })
                                    .child(bitmap_icon(
                                        include_bytes!("../common/icons/prompt.svg"),
                                        18.,
                                    ))
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.prompts_open = !this.prompts_open;
                                        cx.notify();
                                    })),
                            )
                            .child(self.toolbar_action(
                                "agent-settings",
                                include_bytes!("../common/icons/settings.svg"),
                                UiEvent::OpenAgentSettings,
                                cx,
                            )),
                    );
                }
                if self.prompts_open {
                    let mut prompts = div()
                        .id("agent-prompts-list")
                        .w(px(340.))
                        .max_h(px(230.))
                        .overflow_y_scroll()
                        .p_2()
                        .rounded_md()
                        .border_1()
                        .border_color(rgb(theme::resolve(theme::BORDER)))
                        .bg(rgb(theme::resolve(theme::SURFACE)))
                        .flex()
                        .flex_col()
                        .gap_1();
                    let choices = crate::ui::common::agent_prompts(
                        &self.controller.config,
                        &self.controller.state.agent_id,
                    );
                    if choices.is_empty() {
                        prompts = prompts.child("No prompts for this agent");
                    }
                    for (index, (title, prompt)) in choices.into_iter().enumerate() {
                        prompts = prompts.child(
                            div()
                                .id(format!("agent-prompt:{index}"))
                                .p_1()
                                .cursor_pointer()
                                .hover(|style| style.bg(rgb(theme::resolve(theme::PRIMARY_SOFT))))
                                .child(title)
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.controller
                                        .dispatch(UiEvent::SetChatDraft(prompt.clone()));
                                    this.chat_input
                                        .update(cx, |input, cx| input.emplace(&prompt, cx));
                                    this.prompts_open = false;
                                    cx.notify();
                                })),
                        );
                    }
                    panel = panel.child(prompts);
                }
                if self.agent_picker_open {
                    let mut picker = div()
                        .id("chat-agent-picker")
                        .w(px(320.))
                        .max_h(px(260.))
                        .overflow_y_scroll()
                        .p_2()
                        .rounded_md()
                        .border_1()
                        .border_color(rgb(theme::resolve(theme::BORDER)))
                        .bg(rgb(theme::resolve(theme::SURFACE_MUTED)))
                        .flex()
                        .flex_col()
                        .gap_1()
                        .child(
                            text_input("agent-picker-search")
                                .state(self.agent_search_input.downgrade())
                                .placeholder("Search agents by name or role")
                                .w_full()
                                .p_2(),
                        );
                    for (index, agent) in self
                        .controller
                        .config
                        .agents
                        .agents
                        .iter()
                        .filter(|agent| agent.enabled != Some(false) && agent.hidden != Some(true))
                        .filter(|agent| {
                            let category = agent
                                .other
                                .get("category")
                                .and_then(|value| value.as_str())
                                .unwrap_or("");
                            let query = self.agent_query.to_lowercase();
                            agent.name.to_lowercase().contains(&query)
                                || category.to_lowercase().contains(&query)
                        })
                        .enumerate()
                    {
                        let visual = AgentVisual::from_agent(agent, &self.controller.config_path);
                        let avatar = agent_avatar(&visual, 24.);
                        let id = agent.id.clone();
                        picker = picker.child(
                            div()
                                .id(format!("agent-option:{id}"))
                                .w_full()
                                .flex()
                                .items_center()
                                .gap_2()
                                .p_1()
                                .cursor_pointer()
                                .rounded_md()
                                .bg(rgb(theme::resolve(
                                    if id == self.controller.state.agent_id
                                        || index == self.agent_cursor
                                    {
                                        theme::PRIMARY_SOFT
                                    } else {
                                        theme::SURFACE
                                    },
                                )))
                                .child(avatar)
                                .child(format!("{} · {}", agent.name, visual.category))
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.controller.dispatch(UiEvent::SelectAgent(id.clone()));
                                    this.agent_picker_open = false;
                                    this.model_picker_open = false;
                                    this.reasoning_open = false;
                                    let draft = this.controller.state.chat_draft.clone();
                                    this.chat_input
                                        .update(cx, |input, cx| input.emplace(&draft, cx));
                                    cx.notify();
                                })),
                        );
                    }
                    panel = panel.child(picker);
                }
                if self.controller.state.agent_settings.is_some() {
                    return panel.child(self.agent_settings_panel(width, cx));
                }
                let mut messages = div()
                    .id("chat-messages")
                    .flex_1()
                    .min_h(px(0.))
                    .overflow_y_scroll()
                    .flex()
                    .flex_col()
                    .gap_3();
                for (index, message) in self.controller.state.chat.iter().enumerate() {
                    let user = message.role == ChatRole::User;
                    let role = if user { "You" } else { "Assistant" };
                    let mut bubble = div()
                        .id(format!("message:{index}"))
                        .max_w(px(width * 0.86))
                        .p_3()
                        .rounded_lg()
                        .bg(rgb(theme::resolve(if user {
                            theme::PRIMARY
                        } else {
                            theme::SURFACE_MUTED
                        })))
                        .text_color(rgb(theme::resolve(if user {
                            theme::HEADER_TEXT
                        } else {
                            theme::TEXT
                        })))
                        .border_1()
                        .border_color(rgb(theme::resolve(if user {
                            theme::PRIMARY
                        } else {
                            theme::BORDER_SOFT
                        })))
                        .flex()
                        .flex_col()
                        .gap_1()
                        .child(
                            div()
                                .text_xs()
                                .font_weight(gpui::FontWeight::SEMIBOLD)
                                .child(role),
                        );
                    if message.text.is_empty() {
                        bubble = bubble.child("…");
                    } else if message.role == ChatRole::Assistant {
                        for block in crate::plugins::viewers::markdown_preview(&message.text) {
                            bubble = match block {
                                crate::plugins::viewers::PreviewBlock::Heading(_, text) => {
                                    bubble.child(div().text_xl().child(text))
                                }
                                crate::plugins::viewers::PreviewBlock::Paragraph(text) => {
                                    bubble.child(styled_inline(text))
                                }
                                crate::plugins::viewers::PreviewBlock::Code(text) => bubble.child(
                                    div()
                                        .p_2()
                                        .bg(rgb(theme::resolve(theme::SURFACE)))
                                        .font_family("Consolas")
                                        .child(highlighted_code(text)),
                                ),
                                crate::plugins::viewers::PreviewBlock::ListItem(text) => {
                                    bubble.child(format!("• {text}"))
                                }
                                crate::plugins::viewers::PreviewBlock::Quote(text) => bubble.child(
                                    div()
                                        .text_color(rgb(theme::resolve(theme::TEXT_MUTED)))
                                        .child(text),
                                ),
                                crate::plugins::viewers::PreviewBlock::Rule => bubble
                                    .child(div().h(px(1.)).bg(rgb(theme::resolve(theme::BORDER)))),
                            };
                        }
                    } else {
                        bubble = bubble.child(message.text.clone());
                    }
                    let row = if user {
                        div().flex().justify_end()
                    } else {
                        div().flex()
                    };
                    messages = messages.child(row.child(bubble));
                    for (activity_index, activity) in self
                        .controller
                        .state
                        .chat_activity
                        .iter()
                        .enumerate()
                        .filter(|(_, a)| a.after_message == index)
                    {
                        let (label, detail) = match &activity.kind {
                            ChatActivityKind::ToolCall { name, args } => {
                                (format!("⚙ Tool: {name}"), args.clone())
                            }
                            ChatActivityKind::ToolResult { name, result } => {
                                (format!("✓ Result: {name}"), result.clone())
                            }
                            ChatActivityKind::Reasoning(text)
                                if self.controller.state.show_reasoning =>
                            {
                                ("Reasoning".to_owned(), text.clone())
                            }
                            ChatActivityKind::Approval { name, args, .. } => {
                                (format!("Approval requested: {name}"), args.clone())
                            }
                            _ => continue,
                        };
                        let mut card = div()
                            .id(format!("activity:{index}:{activity_index}"))
                            .p_2()
                            .rounded_md()
                            .bg(rgb(theme::resolve(theme::SURFACE_MUTED)))
                            .child(label)
                            .child(detail);
                        if let ChatActivityKind::Approval { id, decision, .. } = &activity.kind {
                            if let Some(approved) = decision {
                                card = card.child(if *approved { "Approved" } else { "Denied" });
                            } else {
                                card = card
                                    .child(self.button(
                                        format!("approve:{id}"),
                                        "Approve".into(),
                                        UiEvent::DecideTool(*id, true),
                                        cx,
                                    ))
                                    .child(self.button(
                                        format!("deny:{id}"),
                                        "Deny".into(),
                                        UiEvent::DecideTool(*id, false),
                                        cx,
                                    ));
                            }
                        }
                        messages = messages.child(card);
                    }
                }
                panel = panel.child(messages);
                if let Some(terminal) = self
                    .controller
                    .state
                    .terminals
                    .get(&self.controller.state.active_session_id)
                    .filter(|terminal| terminal.open)
                    .cloned()
                {
                    let output = terminal
                        .output
                        .chars()
                        .rev()
                        .take(30_000)
                        .collect::<String>()
                        .chars()
                        .rev()
                        .collect::<String>();
                    panel = panel.child(
                        div()
                            .id("terminal-splitter")
                            .h(px(6.))
                            .w_full()
                            .cursor_with(|_, _| Some(gpui::CursorStyle::ResizeUpDown))
                            .bg(rgb(theme::resolve(theme::BORDER)))
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(|this, event: &gpui::MouseDownEvent, _, cx| {
                                    this.dragging_terminal = Some(f32::from(event.position.y));
                                    cx.notify();
                                }),
                            ),
                    );
                    panel = panel.child(
                        div()
                            .id("chat-terminal")
                            .h(px(terminal.height))
                            .min_h(px(120.))
                            .flex_shrink_0()
                            .flex()
                            .flex_col()
                            .gap_1()
                            .p_2()
                            .bg(rgb(0x121824))
                            .text_color(rgb(0xd1d5db))
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap_2()
                                    .child(bitmap_icon(
                                        include_bytes!("../common/icons/terminal.svg"),
                                        18.,
                                    ))
                                    .child("Terminal")
                                    .child(
                                        div()
                                            .text_xs()
                                            .text_color(rgb(0x94a3b8))
                                            .child(terminal.cwd.display().to_string()),
                                    )
                                    .child(div().ml_auto().child(self.toolbar_action(
                                        "terminal-stop",
                                        include_bytes!("../common/icons/stop.svg"),
                                        UiEvent::InterruptTerminal,
                                        cx,
                                    )))
                                    .child(self.toolbar_action(
                                        "terminal-autoscroll",
                                        include_bytes!("../common/icons/scroll_down.svg"),
                                        UiEvent::ToggleTerminalAutoscroll,
                                        cx,
                                    ))
                                    .child(self.toolbar_action(
                                        "terminal-settings",
                                        include_bytes!("../common/icons/settings.svg"),
                                        UiEvent::ToggleTerminalSettings,
                                        cx,
                                    )),
                            )
                            .when(terminal.settings_open, |view| {
                                view.child(
                                    div()
                                        .flex()
                                        .items_center()
                                        .gap_1()
                                        .child(format!(
                                            "Persistent · PTY · {} · buffer {} lines",
                                            terminal.shell, terminal.buffer_lines
                                        ))
                                        .child(self.button(
                                            "terminal-buffer-1000".into(),
                                            "1k".into(),
                                            UiEvent::SetTerminalBuffer(1000),
                                            cx,
                                        ))
                                        .child(self.button(
                                            "terminal-buffer-5000".into(),
                                            "5k".into(),
                                            UiEvent::SetTerminalBuffer(5000),
                                            cx,
                                        ))
                                        .child(self.button(
                                            "terminal-buffer-10000".into(),
                                            "10k".into(),
                                            UiEvent::SetTerminalBuffer(10000),
                                            cx,
                                        )),
                                )
                            })
                            .child(
                                div()
                                    .id("terminal-output")
                                    .flex_1()
                                    .min_h(px(0.))
                                    .overflow_y_scroll()
                                    .track_scroll(&self.terminal_scroll)
                                    .font_family("Consolas")
                                    .text_sm()
                                    .child(output),
                            )
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap_1()
                                    .child("❯")
                                    .child(
                                        text_input("terminal-command")
                                            .state(self.terminal_input.downgrade())
                                            .placeholder("Enter a command")
                                            .flex_1()
                                            .min_w(px(0.))
                                            .p_1()
                                            .bg(rgb(0x1e293b)),
                                    )
                                    .child(
                                        div()
                                            .id("terminal-submit")
                                            .cursor_pointer()
                                            .child("↵")
                                            .on_click(cx.listener(|this, _, _, cx| {
                                                this.controller.dispatch(UiEvent::SubmitTerminal);
                                                this.terminal_input
                                                    .update(cx, |input, cx| input.emplace("", cx));
                                                cx.notify();
                                            })),
                                    ),
                            ),
                    );
                }
                for (index, queued) in self.controller.state.queued_chat.iter().enumerate() {
                    panel = panel.child(
                        div()
                            .flex()
                            .gap_2()
                            .child(format!(
                                "{}. {} ({} files)",
                                index + 1,
                                queued.prompt,
                                queued.attachments.len()
                            ))
                            .child(self.button(
                                format!("queue-up:{index}"),
                                "↑".into(),
                                UiEvent::MoveQueuedChat(index, index.saturating_sub(1)),
                                cx,
                            ))
                            .child(self.button(
                                format!("queue-down:{index}"),
                                "↓".into(),
                                UiEvent::MoveQueuedChat(index, index + 1),
                                cx,
                            ))
                            .child(self.button(
                                format!("remove-queue:{index}"),
                                "Remove".into(),
                                UiEvent::RemoveQueuedChat(index),
                                cx,
                            )),
                    );
                }
                if !self.controller.state.queued_chat.is_empty() {
                    panel = panel.child(self.button(
                        "clear-queue".into(),
                        "Clear queue".into(),
                        UiEvent::ClearQueuedChat,
                        cx,
                    ));
                    if !self.controller.state.busy {
                        panel = panel.child(self.button(
                            "run-next-queue".into(),
                            "Run next".into(),
                            UiEvent::RunNextQueuedChat,
                            cx,
                        ));
                    }
                }
                let mut chips = div().flex().flex_wrap().gap_1();
                if self.controller.state.attach_open_file {
                    if let Some(doc) = &self.controller.state.editor {
                        chips = chips.child(self.button(
                            "context-open-file".into(),
                            format!("{} ×", doc.path),
                            UiEvent::SetAttachOpenFile(false),
                            cx,
                        ));
                    }
                }
                for (index, attachment) in self.controller.state.attachments.iter().enumerate() {
                    chips = chips.child(self.button(
                        format!("attachment:{index}"),
                        format!("{} ×", attachment.name),
                        UiEvent::RemoveAttachment(index),
                        cx,
                    ));
                }
                let models = crate::ui::common::model_picker::choices(&self.controller.config);
                let effective = crate::ui::common::model_picker::effective(
                    &self.controller.config,
                    &self.controller.state.agent_id,
                    &self.controller.state.selected_model,
                );
                let model_label = effective
                    .map(|model| format!("{} · {}", model.provider, model.name))
                    .unwrap_or_else(|| "Auto".into());
                let model_icon = effective
                    .and_then(|model| {
                        crate::ui::common::model_picker::provider_icon_path(
                            &self.controller.config,
                            &self.controller.config_path,
                            &model.provider,
                        )
                    })
                    .map(|path| bitmap_asset(&path, 17.))
                    .unwrap_or_else(|| div().w(px(0.)).into_any_element());
                let mut model_options = div()
                    .id("chat-model-options")
                    .flex()
                    .flex_col()
                    .gap_1()
                    .p_2()
                    .max_h(px(250.))
                    .overflow_y_scroll()
                    .rounded_md()
                    .bg(rgb(theme::resolve(theme::SURFACE_MUTED)))
                    .child(
                        text_input("chat-model-search")
                            .state(self.model_search_input.downgrade())
                            .placeholder("Search models")
                            .w_full()
                            .p_2(),
                    );
                let mut filtered = vec![(
                    "auto".to_owned(),
                    "Auto · agent default".to_owned(),
                    String::new(),
                    Vec::<String>::new(),
                )];
                filtered.extend(
                    models
                        .iter()
                        .filter(|model| {
                            let query = self.model_query.to_lowercase();
                            model.name.to_lowercase().contains(&query)
                                || model.provider.to_lowercase().contains(&query)
                        })
                        .map(|model| {
                            (
                                model.id.clone(),
                                format!("{}  {}  {}", model.provider, model.name, ""),
                                model.provider.clone(),
                                model.capabilities.clone(),
                            )
                        }),
                );
                for (index, (id, label, provider, capabilities)) in filtered.into_iter().enumerate()
                {
                    let icon = crate::ui::common::model_picker::provider_icon_path(
                        &self.controller.config,
                        &self.controller.config_path,
                        &provider,
                    )
                    .map(|path| bitmap_asset(&path, 17.))
                    .unwrap_or_else(|| div().w(px(0.)).into_any_element());
                    let mut row = div()
                        .id(format!("chat-model-option:{id}"))
                        .w_full()
                        .p_1()
                        .rounded_md()
                        .cursor_pointer()
                        .bg(rgb(theme::resolve(
                            if id == self.controller.state.selected_model
                                || index == self.model_cursor
                            {
                                theme::PRIMARY_SOFT
                            } else {
                                theme::SURFACE_MUTED
                            },
                        )))
                        .hover(|s| s.bg(rgb(theme::resolve(theme::PRIMARY_SOFT))))
                        .flex()
                        .items_center()
                        .gap_1()
                        .child(icon)
                        .child(label);
                    for capability in capabilities {
                        row = row.child(bitmap_icon(
                            crate::ui::common::model_picker::capability_icon(&capability),
                            13.,
                        ));
                    }
                    model_options =
                        model_options.child(row.on_click(cx.listener(move |this, _, _, cx| {
                            this.controller.dispatch(UiEvent::SelectModel(id.clone()));
                            this.model_picker_open = false;
                            cx.notify();
                        })));
                }
                let mut selectors = div()
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .gap_2()
                    .child(
                        div()
                            .id("attach-files")
                            .flex()
                            .items_center()
                            .gap_1()
                            .p_1()
                            .rounded_md()
                            .cursor_pointer()
                            .text_color(rgb(theme::resolve(theme::TEXT_MUTED)))
                            .child(bitmap_icon(
                                include_bytes!("../common/icons/attach.svg"),
                                18.,
                            ))
                            .child(div().text_xs().child("Attach"))
                            .on_click(cx.listener(|this, _, _, cx| {
                                if let Some(files) = rfd::FileDialog::new().pick_files() {
                                    this.controller.dispatch(UiEvent::AttachFiles(files));
                                }
                                cx.notify();
                            })),
                    )
                    .child(self.icon_action(
                        "file-context".into(),
                        include_bytes!("../common/icons/file_context.svg"),
                        "Open file",
                        UiEvent::SetAttachOpenFile(!self.controller.state.attach_open_file),
                        cx,
                    ))
                    .child(self.icon_action(
                        "memory-mode".into(),
                        include_bytes!("../common/icons/memory.svg"),
                        "Memory",
                        UiEvent::SetMemoryEnabled(!self.controller.state.memory_enabled),
                        cx,
                    ))
                    .child(
                        div()
                            .id("chat-model")
                            .p_1()
                            .rounded_md()
                            .cursor_pointer()
                            .flex()
                            .items_center()
                            .gap_1()
                            .child(model_icon)
                            .child(format!("Model: {model_label}"))
                            .child(bitmap_icon(
                                include_bytes!("../common/icons/chevron_down.svg"),
                                12.,
                            ))
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.model_picker_open = !this.model_picker_open;
                                this.reasoning_open = false;
                                if this.model_picker_open {
                                    this.model_search_input
                                        .read(cx)
                                        .focus_handle(cx)
                                        .focus(window, cx);
                                }
                                cx.notify();
                            })),
                    )
                    .child(
                        div()
                            .id("reasoning-settings")
                            .p_1()
                            .cursor_pointer()
                            .child(bitmap_icon(
                                include_bytes!("../common/icons/reasoning.svg"),
                                18.,
                            ))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.reasoning_open = !this.reasoning_open;
                                this.model_picker_open = false;
                                cx.notify();
                            })),
                    )
                    .child(self.icon_action(
                        "terminal".into(),
                        include_bytes!("../common/icons/terminal.svg"),
                        "Terminal",
                        UiEvent::ToggleTerminal,
                        cx,
                    ));
                let remembered = self
                    .controller
                    .state
                    .chat
                    .len()
                    .saturating_sub(self.controller.state.memory_start);
                if remembered > 0 && !self.controller.state.busy {
                    selectors = selectors.child(self.button(
                        "clear-memory".into(),
                        format!("{remembered} in context ×"),
                        UiEvent::ClearMemory,
                        cx,
                    ));
                }
                if self.controller.state.busy {
                    selectors = selectors.child(self.button(
                        "stop-chat".into(),
                        "Stop".into(),
                        UiEvent::CancelChat,
                        cx,
                    ));
                }
                if self.model_picker_open {
                    panel = panel.child(model_options);
                }
                if self.reasoning_open {
                    let levels = crate::ui::common::model_picker::reasoning_levels(
                        &self.controller.config,
                        &self.controller.state.agent_id,
                        &self.controller.state.selected_model,
                    );
                    let mut popover = div()
                        .p_2()
                        .rounded_md()
                        .bg(rgb(theme::resolve(theme::SURFACE_MUTED)))
                        .flex()
                        .flex_col()
                        .gap_1()
                        .child("Reasoning");
                    if levels.is_empty() {
                        popover = popover.child("No reasoning levels for this model");
                    } else {
                        let mut track = div().flex().gap_1();
                        for level in levels {
                            let selected =
                                self.controller.state.reasoning_effort.as_deref() == Some(&level);
                            track = track.child(self.button(
                                format!("reasoning:{level}"),
                                format!("{} {level}", if selected { "●" } else { "○" }),
                                UiEvent::SetReasoningEffort(Some(level)),
                                cx,
                            ));
                        }
                        popover = popover.child(track).child(self.button(
                            "reasoning-default".into(),
                            "Model default".into(),
                            UiEvent::SetReasoningEffort(None),
                            cx,
                        ));
                    }
                    panel = panel.child(popover);
                }
                panel = panel.child(
                    div()
                        .p_2()
                        .rounded_lg()
                        .border_1()
                        .border_color(rgb(theme::resolve(theme::BORDER)))
                        .bg(rgb(theme::resolve(theme::SURFACE)))
                        .flex()
                        .flex_col()
                        .gap_2()
                        .child(chips)
                        .child(
                            div()
                                .flex()
                                .items_end()
                                .gap_2()
                                .child(
                                    editable_text("chat-draft")
                                        .state(self.chat_input.downgrade())
                                        .placeholder("Ask this agent…")
                                        .h(px(64.))
                                        .flex_1()
                                        .min_w(px(0.))
                                        .p_2()
                                        .border_1()
                                        .border_color(rgb(theme::resolve(theme::BORDER))),
                                )
                                .child(
                                    div()
                                        .id("voice-input")
                                        .p_2()
                                        .rounded_md()
                                        .cursor_pointer()
                                        .child(bitmap_icon(
                                            include_bytes!("../common/icons/mic.svg"),
                                            20.,
                                        ))
                                        .on_click(cx.listener(|this, _, window, cx| {
                                            this.chat_input
                                                .read(cx)
                                                .focus_handle(cx)
                                                .focus(window, cx);
                                            this.controller.dispatch(UiEvent::StartVoice);
                                            cx.notify();
                                        })),
                                )
                                .child(
                                    div()
                                        .id("send-chat")
                                        .p_2()
                                        .rounded_md()
                                        .cursor_pointer()
                                        .bg(rgb(theme::resolve(theme::PRIMARY)))
                                        .child(bitmap_icon(
                                            include_bytes!("../common/icons/send.svg"),
                                            20.,
                                        ))
                                        .on_click(cx.listener(|this, _, _, cx| {
                                            this.controller.dispatch(UiEvent::SendChat);
                                            if this.controller.state.chat_draft.is_empty() {
                                                this.chat_input
                                                    .update(cx, |input, cx| input.emplace("", cx));
                                            }
                                            cx.notify();
                                        })),
                                ),
                        )
                        .child(selectors),
                );
            }
            CenterView::Web => {
                panel = panel
                    .child("Web panel")
                    .child(
                        div()
                            .flex()
                            .gap_1()
                            .child(self.button("web-back".into(), "←".into(), UiEvent::WebBack, cx))
                            .child(self.button(
                                "web-forward".into(),
                                "→".into(),
                                UiEvent::WebForward,
                                cx,
                            )),
                    )
                    .child(
                        text_input("web-url")
                            .state(self.web_input.downgrade())
                            .placeholder("https://...")
                            .p_2()
                            .border_1()
                            .border_color(rgb(theme::resolve(theme::BORDER))),
                    )
                    .child(self.button("open-web".into(), "Open".into(), UiEvent::OpenWeb, cx));
                if self.controller.state.web_loading {
                    panel = panel.child("Loading…");
                }
                if let Some(page) = &self.controller.state.web_page {
                    let mut preview = div()
                        .id("web-document")
                        .flex_1()
                        .min_h(px(0.))
                        .overflow_y_scroll()
                        .flex()
                        .flex_col()
                        .gap_2()
                        .child(div().text_2xl().child(page.title.clone()))
                        .child(
                            div()
                                .text_xs()
                                .text_color(rgb(theme::resolve(theme::TEXT_MUTED)))
                                .child(page.url.clone()),
                        )
                        .child(page.text.clone());
                    for (label, url) in &page.links {
                        let url = url.clone();
                        preview = preview.child(
                            div()
                                .id(format!("web-link:{url}"))
                                .cursor_pointer()
                                .text_color(rgb(theme::resolve(theme::PRIMARY)))
                                .child(label.clone())
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.controller.dispatch(UiEvent::SetWebUrl(url.clone()));
                                    this.web_input
                                        .update(cx, |input, cx| input.emplace(&url, cx));
                                    this.controller.dispatch(UiEvent::OpenWeb);
                                    cx.notify();
                                })),
                        );
                    }
                    panel = panel.child(preview);
                } else if let Some(doc) = &self.controller.state.editor {
                    let mut preview = div()
                        .id("web-preview")
                        .flex_1()
                        .overflow_y_scroll()
                        .flex()
                        .flex_col()
                        .gap_2();
                    let blocks = crate::plugins::viewers::document_preview(&doc.path, &doc.text);
                    if let Some(blocks) = blocks {
                        for block in blocks {
                            preview = match block {
                                crate::plugins::viewers::PreviewBlock::Heading(_, text) => {
                                    preview.child(div().text_2xl().child(text))
                                }
                                crate::plugins::viewers::PreviewBlock::Paragraph(text) => {
                                    preview.child(styled_inline(text))
                                }
                                crate::plugins::viewers::PreviewBlock::Code(text) => preview.child(
                                    div()
                                        .p_2()
                                        .bg(rgb(theme::resolve(theme::SURFACE_MUTED)))
                                        .child(highlighted_code(text)),
                                ),
                                crate::plugins::viewers::PreviewBlock::ListItem(text) => {
                                    preview.child(format!("• {text}"))
                                }
                                crate::plugins::viewers::PreviewBlock::Quote(text) => preview
                                    .child(
                                        div()
                                            .text_color(rgb(theme::resolve(theme::TEXT_MUTED)))
                                            .child(text),
                                    ),
                                crate::plugins::viewers::PreviewBlock::Rule => preview
                                    .child(div().h(px(1.)).bg(rgb(theme::resolve(theme::BORDER)))),
                            };
                        }
                    } else {
                        preview = preview.child(doc.text.clone());
                    }
                    panel = panel
                        .child(format!("Local preview: {}", doc.path))
                        .child(preview);
                } else {
                    panel = panel.child("Open a local text file to preview it here.");
                }
            }
            CenterView::Changes => {
                let mut changes = div()
                    .id("file-changes")
                    .flex_1()
                    .min_h(px(0.))
                    .overflow_y_scroll()
                    .flex()
                    .flex_col()
                    .gap_1();
                if let Some(doc) = &self.controller.state.editor {
                    changes = changes.child(doc.path.clone());
                    for (kind, line) in doc.changes() {
                        changes = changes.child(
                            div()
                                .text_color(rgb(match kind {
                                    '+' => 0x059669,
                                    '-' => theme::DANGER,
                                    _ => theme::TEXT_MUTED,
                                }))
                                .child(format!("{kind} {line}")),
                        );
                    }
                } else {
                    changes = changes.child("Open a file to inspect changes.");
                }
                panel = panel.child(changes);
            }
            CenterView::Admin => {
                panel = panel
                    .child(self.controller.state.admin_section.label())
                    .child("Desktop overrides are stored locally.");
                let section = self.controller.state.admin_section;
                if section == AdminSection::Agents {
                    let agent_id = self.controller.state.agent_id.clone();
                    let is_default = self
                        .controller
                        .config
                        .agents
                        .agents
                        .iter()
                        .any(|agent| agent.id == agent_id && agent.default == Some(true));
                    if !is_default {
                        panel = panel.child(self.button(
                            "default-agent".into(),
                            format!("Make {agent_id} the default agent"),
                            UiEvent::SetDefaultAgent(agent_id),
                            cx,
                        ));
                    }
                }
                for (id, name, enabled) in self.controller.admin_resources() {
                    if matches!(section, AdminSection::Agents | AdminSection::Models) {
                        let (icon, detail) = if section == AdminSection::Agents {
                            let visual = self
                                .controller
                                .config
                                .agents
                                .agents
                                .iter()
                                .find(|agent| agent.id == id)
                                .map(|agent| {
                                    AgentVisual::from_agent(agent, &self.controller.config_path)
                                });
                            let detail = visual
                                .as_ref()
                                .map(|visual| visual.category.clone())
                                .unwrap_or_default();
                            (
                                visual
                                    .map(|visual| agent_avatar(&visual, 24.))
                                    .unwrap_or_else(|| {
                                        bitmap_icon(
                                            include_bytes!("../common/icons/agents.svg"),
                                            24.,
                                        )
                                    }),
                                detail,
                            )
                        } else {
                            let provider = self
                                .controller
                                .config
                                .models
                                .as_ref()
                                .and_then(|models| {
                                    models.models.iter().find(|model| {
                                        model.id.as_deref() == Some(id.as_str())
                                            || model.name == name
                                    })
                                })
                                .map(|model| model.provider.clone())
                                .unwrap_or_default();
                            let icon = crate::ui::common::model_picker::provider_icon_path(
                                &self.controller.config,
                                &self.controller.config_path,
                                &provider,
                            )
                            .map(|path| bitmap_asset(&path, 24.))
                            .unwrap_or_else(|| {
                                bitmap_icon(include_bytes!("../common/icons/file.svg"), 24.)
                            });
                            (icon, provider)
                        };
                        let open_id = id.clone();
                        panel = panel.child(
                            div()
                                .id(format!("resource-row:{id}"))
                                .w_full()
                                .flex()
                                .items_center()
                                .gap_2()
                                .p_2()
                                .rounded_md()
                                .bg(rgb(theme::resolve(theme::SURFACE_MUTED)))
                                .child(icon)
                                .child(
                                    div()
                                        .id(format!("resource-open:{id}"))
                                        .cursor_pointer()
                                        .child(name)
                                        .on_click(cx.listener(move |this, _, _, cx| {
                                            if section == AdminSection::Agents {
                                                this.controller.dispatch(UiEvent::SelectAgent(
                                                    open_id.clone(),
                                                ));
                                                this.controller
                                                    .dispatch(UiEvent::OpenAgentSettings);
                                                this.controller.dispatch(UiEvent::SelectCenter(
                                                    CenterView::Chat,
                                                ));
                                            } else {
                                                this.controller.dispatch(UiEvent::SelectModel(
                                                    open_id.clone(),
                                                ));
                                            }
                                            cx.notify();
                                        })),
                                )
                                .child(
                                    div()
                                        .text_xs()
                                        .text_color(rgb(theme::resolve(theme::TEXT_MUTED)))
                                        .child(format!("· {detail}")),
                                )
                                .child(div().flex_1())
                                .child(self.enabled_switch(
                                    format!("resource-toggle:{id}"),
                                    enabled,
                                    UiEvent::SetResourceEnabled(section, id.clone(), !enabled),
                                    cx,
                                ))
                                .child(info_icon(
                                    format!("resource-info:{id}"),
                                    section.enabled_help(),
                                )),
                        );
                    } else {
                        panel = panel.child(
                            div()
                                .id(format!("resource-row:{id}"))
                                .w_full()
                                .flex()
                                .items_center()
                                .gap_2()
                                .p_2()
                                .rounded_md()
                                .bg(rgb(theme::resolve(theme::SURFACE_MUTED)))
                                .child(name)
                                .child(div().flex_1())
                                .child(self.enabled_switch(
                                    format!("resource-toggle:{id}"),
                                    enabled,
                                    UiEvent::SetResourceEnabled(section, id.clone(), !enabled),
                                    cx,
                                ))
                                .child(info_icon(
                                    format!("resource-info:{id}"),
                                    section.enabled_help(),
                                )),
                        );
                    }
                }
                if self.controller.state.admin_section == AdminSection::Credentials {
                    let mut credential_panel = panel
                        .child(div().flex().items_center().gap_1()
                            .child("Saved credentials")
                            .child(info_icon("credential-vault-info".into(),
                                "Credential values are stored in the OS vault. The list shows metadata only.")));
                    for credential in self.controller.credential_summaries() {
                        credential_panel = credential_panel.child(
                            div()
                                .id(format!("credential-row:{}", credential.id))
                                .w_full()
                                .flex()
                                .items_center()
                                .gap_2()
                                .p_2()
                                .rounded_md()
                                .bg(rgb(theme::resolve(theme::SURFACE_MUTED)))
                                .child(bitmap_icon(include_bytes!("../common/icons/key.svg"), 24.))
                                .child(
                                    div().flex().flex_col().child(credential.name).child(
                                        div()
                                            .text_xs()
                                            .text_color(rgb(theme::resolve(theme::TEXT_MUTED)))
                                            .child(format!(
                                                "{} · {}",
                                                credential.provider, credential.kind
                                            )),
                                    ),
                                ),
                        );
                    }
                    return credential_panel
                        .child("Credential YAML: id, name, provider, type, values")
                        .child(
                            editable_text("credential-draft")
                                .state(self.credential_input.downgrade())
                                .w_full()
                                .h(px(150.))
                                .p_2()
                                .border_1()
                                .border_color(rgb(theme::resolve(theme::BORDER))),
                        )
                        .child(self.button(
                            "save-credential".into(),
                            "Save credential".into(),
                            UiEvent::SaveCredential,
                            cx,
                        ))
                        .child("ID to delete")
                        .child(
                            text_input("credential-delete-id")
                                .state(self.admin_target_input.downgrade())
                                .w_full()
                                .p_1()
                                .border_1()
                                .border_color(rgb(theme::resolve(theme::BORDER))),
                        )
                        .child(self.button(
                            "delete-credential".into(),
                            "Delete credential".into(),
                            UiEvent::DeleteCredential(
                                self.controller.state.admin_target_id.clone(),
                            ),
                            cx,
                        ));
                }
                if matches!(
                    self.controller.state.admin_section,
                    AdminSection::Workflows
                        | AdminSection::Evaluations
                        | AdminSection::Connectors
                        | AdminSection::Knowledge
                        | AdminSection::Schedules
                ) {
                    panel = panel
                        .child(match self.controller.state.admin_section {
                            AdminSection::Workflows => "Workflow ID (blank selects first)",
                            AdminSection::Evaluations => "Evalset ID (blank selects first)",
                            AdminSection::Knowledge => "Knowledge search",
                            AdminSection::Schedules => "Schedule ID (blank selects first)",
                            _ => "Connector ID",
                        })
                        .child(
                            text_input("admin-target")
                                .state(self.admin_target_input.downgrade())
                                .w_full()
                                .p_1()
                                .border_1()
                                .border_color(rgb(theme::resolve(theme::BORDER))),
                        );
                    if matches!(
                        self.controller.state.admin_section,
                        AdminSection::Workflows
                            | AdminSection::Connectors
                            | AdminSection::Knowledge
                    ) {
                        panel = panel
                            .child(match self.controller.state.admin_section {
                                AdminSection::Connectors => "Path or read <file> (tools for MCP)",
                                AdminSection::Knowledge => "Search terms",
                                _ => "Input JSON",
                            })
                            .child(
                                text_input("admin-run-input")
                                    .state(self.admin_run_input.downgrade())
                                    .w_full()
                                    .p_1()
                                    .border_1()
                                    .border_color(rgb(theme::resolve(theme::BORDER))),
                            );
                    }
                    panel = panel
                        .child(
                            self.button(
                                "run-admin".into(),
                                match self.controller.state.admin_section {
                                    AdminSection::Workflows => "Run workflow",
                                    AdminSection::Evaluations => "Run evaluation",
                                    AdminSection::Knowledge => "Search knowledge",
                                    AdminSection::Schedules => "Start schedule",
                                    _ => "Browse connector",
                                }
                                .into(),
                                UiEvent::RunAdmin,
                                cx,
                            ),
                        )
                        .child(self.controller.state.admin_output.clone());
                    if self.controller.state.admin_busy {
                        panel = panel.child(self.button(
                            "cancel-admin".into(),
                            "Cancel".into(),
                            UiEvent::CancelAdmin,
                            cx,
                        ));
                    }
                }
                if matches!(
                    self.controller.state.admin_section,
                    AdminSection::Credentials | AdminSection::Monitoring
                ) {
                    panel = panel.child(self.controller.state.admin_draft.clone());
                } else {
                    panel = panel
                        .child(self.button(
                            "save-admin".into(),
                            "Save changes".into(),
                            UiEvent::SaveAdmin,
                            cx,
                        ))
                        .child(
                            editable_text("admin-editor")
                                .state(self.admin_input.downgrade())
                                .w_full()
                                .flex_1()
                                .p_2()
                                .border_1()
                                .border_color(rgb(theme::resolve(theme::BORDER)))
                                .overflow_y_scroll(),
                        );
                }
            }
        }
        panel
    }

    fn editor_panel(&self, width: f32, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let mut panel = div()
            .w(px(width))
            .flex_shrink_0()
            .h_full()
            .min_h(px(0.))
            .flex()
            .flex_col()
            .gap_2()
            .p_3()
            .bg(rgb(theme::resolve(theme::SURFACE)));
        let mut tabs = div().flex().gap_1().overflow_hidden();
        for doc in &self.controller.state.documents {
            let path = doc.path.clone();
            let drag_path = path.clone();
            let drop_path = path.clone();
            let active = self
                .controller
                .state
                .editor
                .as_ref()
                .is_some_and(|d| d.path == path);
            let label = format!(
                "{}{}{}",
                if active { "● " } else { "" },
                std::path::Path::new(&path)
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy(),
                if doc.dirty() { " *" } else { "" }
            );
            tabs = tabs.child(
                div()
                    .id(format!("editor-tab:{path}"))
                    .px_2()
                    .py_1()
                    .cursor_pointer()
                    .bg(rgb(theme::resolve(if active {
                        theme::PRIMARY_SOFT
                    } else {
                        theme::SURFACE_MUTED
                    })))
                    .child(label)
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _, _, cx| {
                            this.dragging_document = Some(drag_path.clone());
                            cx.notify();
                        }),
                    )
                    .on_mouse_up(
                        MouseButton::Left,
                        cx.listener(move |this, _, _, cx| {
                            if let Some(source) = this.dragging_document.take() {
                                this.controller
                                    .dispatch(UiEvent::MoveDocument(source, drop_path.clone()));
                            }
                            cx.notify();
                        }),
                    )
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.controller
                            .dispatch(UiEvent::SelectEditor(path.clone()));
                        if let Some(doc) = &this.controller.state.editor {
                            this.editor_input
                                .update(cx, |input, cx| input.emplace(&doc.text, cx));
                        }
                        cx.notify();
                    })),
            );
        }
        panel = panel.child(tabs);
        if let Some(doc) = &self.controller.state.editor {
            panel = panel.child(doc.path.clone()).child(doc.plugin);
            panel = panel.child(self.button(
                "close-file".into(),
                "× Close".into(),
                UiEvent::CloseEditor,
                cx,
            ));
            panel = panel.child(
                self.button(
                    "float-file".into(),
                    if self.controller.state.floating_documents.contains(&doc.path) {
                        "Dock"
                    } else {
                        "Float"
                    }
                    .into(),
                    UiEvent::ToggleDocumentFloating(doc.path.clone()),
                    cx,
                ),
            );
            if doc.dirty() {
                panel = panel.child(self.button(
                    "discard-file".into(),
                    "Discard changes".into(),
                    UiEvent::DiscardAndCloseEditor,
                    cx,
                ));
            }
            if doc.editable {
                panel = panel
                    .child(self.button("save-file".into(), "Save".into(), UiEvent::SaveFile, cx))
                    .child(self.button(
                        "revert-file".into(),
                        "Revert".into(),
                        UiEvent::RevertFile,
                        cx,
                    ));
                if doc.path.ends_with(".json") || doc.path.ends_with(".ipynb") {
                    panel = panel.child(self.button(
                        "format-json".into(),
                        "Format JSON".into(),
                        UiEvent::FormatDocument,
                        cx,
                    ));
                }
            }
            let preview_blocks = crate::plugins::viewers::document_preview(&doc.path, &doc.text);
            if preview_blocks.is_some() {
                panel = panel.child(
                    div()
                        .flex()
                        .gap_1()
                        .child(self.button(
                            "doc-source".into(),
                            "Source".into(),
                            UiEvent::SetDocumentView(DocumentView::Source),
                            cx,
                        ))
                        .child(self.button(
                            "doc-preview".into(),
                            "Preview".into(),
                            UiEvent::SetDocumentView(DocumentView::Preview),
                            cx,
                        ))
                        .child(self.button(
                            "doc-split".into(),
                            "Split".into(),
                            UiEvent::SetDocumentView(DocumentView::Split),
                            cx,
                        )),
                );
            }
            let mode = if preview_blocks.is_none() {
                DocumentView::Source
            } else {
                self.controller.state.document_view
            };
            let mut body = div().flex().flex_1().min_h(px(0.)).gap_1();
            if mode != DocumentView::Preview {
                if doc.editable {
                    body = body.child(
                        editable_text("code-editor")
                            .state(self.editor_input.downgrade())
                            .font_family("Consolas")
                            .w_full()
                            .flex_1()
                            .p_2()
                            .border_1()
                            .border_color(rgb(theme::resolve(theme::BORDER)))
                            .overflow_y_scroll(),
                    );
                } else {
                    body = body.child(
                        div()
                            .id("viewer")
                            .flex_1()
                            .font_family("Consolas")
                            .overflow_y_scroll()
                            .child(doc.text.clone()),
                    );
                }
            }
            if mode != DocumentView::Source {
                let mut preview = div()
                    .id("editor-preview")
                    .flex_1()
                    .min_w(px(0.))
                    .overflow_y_scroll()
                    .flex()
                    .flex_col()
                    .gap_2();
                if let Some(blocks) = preview_blocks {
                    for block in blocks {
                        preview = match block {
                            crate::plugins::viewers::PreviewBlock::Heading(_, text) => {
                                preview.child(div().text_xl().child(text))
                            }
                            crate::plugins::viewers::PreviewBlock::Paragraph(text) => {
                                preview.child(styled_inline(text))
                            }
                            crate::plugins::viewers::PreviewBlock::Code(text) => preview.child(
                                div()
                                    .p_2()
                                    .font_family("Consolas")
                                    .bg(rgb(theme::resolve(theme::SURFACE_MUTED)))
                                    .child(highlighted_code(text)),
                            ),
                            crate::plugins::viewers::PreviewBlock::ListItem(text) => {
                                preview.child(format!("• {text}"))
                            }
                            crate::plugins::viewers::PreviewBlock::Quote(text) => preview.child(
                                div()
                                    .text_color(rgb(theme::resolve(theme::TEXT_MUTED)))
                                    .child(text),
                            ),
                            crate::plugins::viewers::PreviewBlock::Rule => preview
                                .child(div().h(px(1.)).bg(rgb(theme::resolve(theme::BORDER)))),
                        };
                    }
                }
                body = body.child(preview);
            }
            panel = panel.child(body);
        } else {
            panel = panel
                .child("Editor / Viewer")
                .child("Select a file from the tree.");
        }
        panel
    }
}

impl Render for DesktopView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // GPUI reports the content area in logical pixels already.
        let content_width = f32::from(window.viewport_size().width);
        let content_height = f32::from(window.viewport_size().height);
        let panes = crate::ui::common::layout::PaneLayout::fit(
            content_width,
            64.0,
            self.left_width,
            self.editor_width,
        );
        let center_width = panes.center;
        let mut rail = div()
            .w(px(64.))
            .flex_shrink_0()
            .h_full()
            .flex()
            .flex_col()
            .gap_2()
            .p_2()
            .bg(rgb(theme::resolve(theme::SURFACE)));
        for view in LeftView::ALL {
            let icon: &'static [u8] = match view {
                LeftView::Files => include_bytes!("../common/icons/files.svg"),
                LeftView::Agents => include_bytes!("../common/icons/agents.svg"),
                LeftView::Sessions => include_bytes!("../common/icons/sessions.svg"),
                LeftView::Connectors => include_bytes!("../common/icons/connectors.svg"),
                LeftView::Settings => include_bytes!("../common/icons/settings.svg"),
            };
            rail = rail.child(
                div()
                    .id(format!("nav:{view:?}"))
                    .flex()
                    .flex_col()
                    .items_center()
                    .gap_1()
                    .p_2()
                    .rounded_lg()
                    .cursor_pointer()
                    .bg(rgb(theme::resolve(
                        if self.controller.state.left_view == view {
                            theme::PRIMARY_SOFT
                        } else {
                            theme::SURFACE
                        },
                    )))
                    .hover(|s| s.bg(rgb(theme::resolve(theme::PRIMARY_SOFT))))
                    .child(bitmap_icon(icon, 20.))
                    .child(
                        div()
                            .text_xs()
                            .text_color(rgb(theme::resolve(theme::TEXT_MUTED)))
                            .child(view.label()),
                    )
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.controller.dispatch(UiEvent::SelectView(view));
                        cx.notify();
                    })),
            );
        }
        let weak = cx.entity().downgrade();
        let weak_up = weak.clone();
        let weak_left = weak.clone();
        let weak_right = weak.clone();
        let weak_float = weak.clone();
        let floating = self
            .controller
            .state
            .documents
            .iter()
            .filter(|doc| self.controller.state.floating_documents.contains(&doc.path))
            .enumerate()
            .map(|(index, doc)| {
                let bounds = self
                    .controller
                    .state
                    .floating_bounds
                    .get(&doc.path)
                    .copied()
                    .unwrap_or([
                        180. + index as f32 * 28.,
                        90. + index as f32 * 28.,
                        620.,
                        460.,
                    ]);
                let drag_path = doc.path.clone();
                let resize_path = doc.path.clone();
                let weak_drag = weak_float.clone();
                let weak_resize = weak_float.clone();
                div()
                    .id(format!("floating:{}", doc.path))
                    .absolute()
                    .top(px(bounds[1]))
                    .left(px(bounds[0]))
                    .w(px(bounds[2]))
                    .h(px(bounds[3]))
                    .flex()
                    .flex_col()
                    .border_1()
                    .border_color(rgb(theme::resolve(theme::BORDER)))
                    .bg(rgb(theme::resolve(theme::SURFACE)))
                    .child(
                        div()
                            .id(format!("floating-title:{}", doc.path))
                            .flex()
                            .justify_between()
                            .p_2()
                            .cursor_pointer()
                            .on_mouse_down(
                                MouseButton::Left,
                                move |event: &gpui::MouseDownEvent, _, cx| {
                                    let _ = weak_drag.update(cx, |view, _| {
                                        view.dragging_float = Some((
                                            drag_path.clone(),
                                            false,
                                            f32::from(event.position.x),
                                            f32::from(event.position.y),
                                        ));
                                    });
                                },
                            )
                            .child(doc.path.clone())
                            .child(self.button(
                                format!("dock:{}", doc.path),
                                "Dock".into(),
                                UiEvent::ToggleDocumentFloating(doc.path.clone()),
                                cx,
                            )),
                    )
                    .child(
                        div()
                            .id(format!("floating-content:{}", doc.path))
                            .flex_1()
                            .min_h(px(0.))
                            .overflow_y_scroll()
                            .p_3()
                            .font_family("Consolas")
                            .child(highlighted_code(doc.text.chars().take(40_000).collect())),
                    )
                    .child(
                        div()
                            .id(format!("floating-resize:{}", doc.path))
                            .h(px(10.))
                            .cursor_col_resize()
                            .on_mouse_down(
                                MouseButton::Left,
                                move |event: &gpui::MouseDownEvent, _, cx| {
                                    let _ = weak_resize.update(cx, |view, _| {
                                        view.dragging_float = Some((
                                            resize_path.clone(),
                                            true,
                                            f32::from(event.position.x),
                                            f32::from(event.position.y),
                                        ));
                                    });
                                },
                            ),
                    )
            })
            .collect::<Vec<_>>();
        div()
            .w(px(content_width))
            .h(px(content_height))
            .relative()
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                if event.keystroke.key == "escape" {
                    if this.agent_picker_open || this.model_picker_open || this.reasoning_open {
                        this.agent_picker_open = false;
                        this.model_picker_open = false;
                        this.reasoning_open = false;
                        cx.stop_propagation();
                        cx.notify();
                        return;
                    }
                }
                if this.agent_picker_open
                    && this
                        .agent_search_input
                        .read(cx)
                        .focus_handle(cx)
                        .is_focused(window)
                {
                    let matches: Vec<_> = this
                        .controller
                        .config
                        .agents
                        .agents
                        .iter()
                        .filter(|agent| agent.enabled != Some(false) && agent.hidden != Some(true))
                        .filter(|agent| {
                            let query = this.agent_query.to_lowercase();
                            let role = agent
                                .other
                                .get("category")
                                .and_then(|value| value.as_str())
                                .unwrap_or("");
                            agent.name.to_lowercase().contains(&query)
                                || role.to_lowercase().contains(&query)
                        })
                        .map(|agent| agent.id.clone())
                        .collect();
                    match event.keystroke.key.as_str() {
                        "down" => {
                            this.agent_cursor =
                                (this.agent_cursor + 1).min(matches.len().saturating_sub(1))
                        }
                        "up" => this.agent_cursor = this.agent_cursor.saturating_sub(1),
                        "enter" => {
                            if let Some(id) = matches.get(this.agent_cursor) {
                                this.controller.dispatch(UiEvent::SelectAgent(id.clone()));
                                this.agent_picker_open = false;
                                let draft = this.controller.state.chat_draft.clone();
                                this.model_picker_open = false;
                                this.reasoning_open = false;
                                this.chat_input
                                    .update(cx, |input, cx| input.emplace(&draft, cx));
                            }
                        }
                        _ => return,
                    }
                    cx.stop_propagation();
                    cx.notify();
                    return;
                }
                if this.model_picker_open
                    && this
                        .model_search_input
                        .read(cx)
                        .focus_handle(cx)
                        .is_focused(window)
                {
                    let mut matches = vec!["auto".to_owned()];
                    matches.extend(
                        crate::ui::common::model_picker::choices(&this.controller.config)
                            .into_iter()
                            .filter(|model| {
                                let query = this.model_query.to_lowercase();
                                model.name.to_lowercase().contains(&query)
                                    || model.provider.to_lowercase().contains(&query)
                            })
                            .map(|model| model.id),
                    );
                    match event.keystroke.key.as_str() {
                        "down" => {
                            this.model_cursor =
                                (this.model_cursor + 1).min(matches.len().saturating_sub(1))
                        }
                        "up" => this.model_cursor = this.model_cursor.saturating_sub(1),
                        "enter" => {
                            if let Some(id) = matches.get(this.model_cursor) {
                                this.controller.dispatch(UiEvent::SelectModel(id.clone()));
                                this.model_picker_open = false;
                            }
                        }
                        _ => return,
                    }
                    cx.stop_propagation();
                    cx.notify();
                    return;
                }
                if event.keystroke.key == "enter"
                    && this
                        .terminal_input
                        .read(cx)
                        .focus_handle(cx)
                        .is_focused(window)
                    && !event.is_held
                {
                    this.controller.dispatch(UiEvent::SubmitTerminal);
                    this.terminal_input
                        .update(cx, |input, cx| input.emplace("", cx));
                    cx.stop_propagation();
                    cx.notify();
                    return;
                }
                if this
                    .terminal_input
                    .read(cx)
                    .focus_handle(cx)
                    .is_focused(window)
                    && matches!(event.keystroke.key.as_str(), "up" | "down")
                {
                    this.controller.dispatch(UiEvent::TerminalHistory(
                        if event.keystroke.key == "up" { -1 } else { 1 },
                    ));
                    let input = this
                        .controller
                        .state
                        .terminals
                        .get(&this.controller.state.active_session_id)
                        .map(|terminal| terminal.input.clone())
                        .unwrap_or_default();
                    this.terminal_input
                        .update(cx, |editor, cx| editor.emplace(&input, cx));
                    cx.stop_propagation();
                    cx.notify();
                    return;
                }
                if event.keystroke.key == "enter"
                    && !event.keystroke.modifiers.shift
                    && !event.is_held
                    && this.chat_input.read(cx).focus_handle(cx).is_focused(window)
                {
                    this.controller.dispatch(UiEvent::SendChat);
                    if this.controller.state.chat_draft.is_empty() {
                        this.chat_input
                            .update(cx, |input, cx| input.emplace("", cx));
                    }
                    cx.stop_propagation();
                    cx.notify();
                    return;
                }
                if !event.keystroke.modifiers.control || event.is_held {
                    return;
                }
                let action = match event.keystroke.key.as_str() {
                    "s" => Some(UiEvent::SaveFile),
                    "n" => Some(UiEvent::NewChat),
                    "w" => Some(UiEvent::CloseEditor),
                    "tab" => {
                        let tabs = &this.controller.state.center_tabs;
                        let index = tabs
                            .iter()
                            .position(|tab| *tab == this.controller.state.center_view)
                            .unwrap_or(0);
                        let next = if event.keystroke.modifiers.shift {
                            (index + tabs.len() - 1) % tabs.len()
                        } else {
                            (index + 1) % tabs.len()
                        };
                        Some(UiEvent::SelectCenter(tabs[next]))
                    }
                    _ => None,
                };
                if let Some(action) = action {
                    this.controller.dispatch(action);
                    cx.stop_propagation();
                    cx.notify();
                }
            }))
            .on_mouse_move_all(move |event, phase, _, _, cx| {
                if phase != DispatchPhase::Bubble || event.pressed_button != Some(MouseButton::Left)
                {
                    return;
                }
                let _ = weak.update(cx, |view, cx| {
                    if let Some((left, previous)) = view.dragging {
                        let position = f32::from(event.position.x);
                        let delta = position - previous;
                        if left {
                            view.left_width = (view.left_width + delta).clamp(170.0, 600.0);
                        } else {
                            view.editor_width = (view.editor_width - delta).clamp(260.0, 750.0);
                        }
                        view.dragging = Some((left, position));
                        cx.notify();
                    }
                    if let Some(previous) = view.dragging_terminal {
                        let position = f32::from(event.position.y);
                        let id = view.controller.state.active_session_id.clone();
                        if let Some(terminal) = view.controller.state.terminals.get(&id) {
                            view.controller.dispatch(UiEvent::ResizeTerminal(
                                terminal.height + previous - position,
                            ));
                        }
                        view.dragging_terminal = Some(position);
                        cx.notify();
                    }
                    if let Some((path, resize, previous_x, previous_y)) =
                        view.dragging_float.clone()
                    {
                        let x = f32::from(event.position.x);
                        let y = f32::from(event.position.y);
                        let bounds = view
                            .controller
                            .state
                            .floating_bounds
                            .entry(path.clone())
                            .or_insert([180., 90., 620., 460.]);
                        if resize {
                            bounds[2] = (bounds[2] + x - previous_x).max(240.);
                            bounds[3] = (bounds[3] + y - previous_y).max(180.);
                        } else {
                            bounds[0] = (bounds[0] + x - previous_x).max(0.);
                            bounds[1] = (bounds[1] + y - previous_y).max(0.);
                        }
                        view.dragging_float = Some((path, resize, x, y));
                        cx.notify();
                    }
                });
            })
            .on_mouse_up_all(move |_, phase, _, _, cx| {
                if phase == DispatchPhase::Bubble {
                    let _ = weak_up.update(cx, |view, _| {
                        view.dragging_tab = None;
                        view.dragging_document = None;
                        view.dragging_terminal = None;
                        if view.dragging.take().is_some() {
                            view.controller.dispatch(UiEvent::ResizePanels(
                                view.left_width,
                                view.editor_width,
                            ));
                        }
                        if let Some((path, _, _, _)) = view.dragging_float.take() {
                            if let Some(bounds) =
                                view.controller.state.floating_bounds.get(&path).copied()
                            {
                                view.controller
                                    .dispatch(UiEvent::SetFloatingBounds(path, bounds));
                            }
                        }
                    });
                }
            })
            .flex()
            .flex_col()
            .font_family(theme::FONT_FAMILY)
            .text_color(rgb(theme::resolve(theme::TEXT)))
            .bg(rgb(theme::resolve(theme::BG)))
            .child(
                div()
                    .h(px(56.))
                    .px_4()
                    .flex()
                    .items_center()
                    .gap_2()
                    .bg(rgb(theme::resolve(theme::HEADER)))
                    .text_color(rgb(theme::resolve(theme::HEADER_TEXT)))
                    .child(bitmap_icon(theme::BRAND_ICON, 28.))
                    .child(div().font_weight(gpui::FontWeight::SEMIBOLD).child("TaIQ"))
                    .child(div().text_color(rgb(0x94a3b8)).child("/  Workspace"))
                    .when(self.controller.state.narrow_left, |header| {
                        header.child(self.button(
                            "narrow-chat".into(),
                            "Chat".into(),
                            UiEvent::SelectCenter(CenterView::Chat),
                            cx,
                        ))
                    })
                    .child(
                        div()
                            .id("narrow-editor-toggle")
                            .cursor_pointer()
                            .child(if self.controller.state.narrow_editor {
                                "Chat"
                            } else {
                                "Editor"
                            })
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.controller.dispatch(UiEvent::SetNarrowEditor(
                                    !this.controller.state.narrow_editor,
                                ));
                                cx.notify();
                            })),
                    )
                    .child(
                        div()
                            .id("theme-toggle")
                            .ml_auto()
                            .cursor_pointer()
                            .child(if self.controller.state.dark_mode {
                                "☀ Light"
                            } else {
                                "☾ Dark"
                            })
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.controller.dispatch(UiEvent::SetDarkMode(
                                    !this.controller.state.dark_mode,
                                ));
                                cx.notify();
                            })),
                    ),
            )
            .child(
                div()
                    .flex_1()
                    .min_h(px(0.))
                    .flex()
                    .overflow_hidden()
                    .child(rail)
                    .when(panes.show_left, |body| {
                        body.child(self.left_panel(panes.left, cx))
                    })
                    .when(panes.show_left, |body| {
                        body.child(
                            div()
                                .id("left-splitter")
                                .w(px(6.))
                                .h_full()
                                .bg(rgb(theme::resolve(theme::BORDER)))
                                .cursor_col_resize()
                                .on_mouse_down(
                                    MouseButton::Left,
                                    move |event: &gpui::MouseDownEvent, _, cx| {
                                        let _ = weak_left.update(cx, |this, _| {
                                            this.dragging =
                                                Some((true, f32::from(event.position.x)));
                                        });
                                    },
                                ),
                        )
                    })
                    .when(
                        (panes.show_right || !self.controller.state.narrow_editor)
                            && (panes.show_left || !self.controller.state.narrow_left),
                        |body| body.child(self.center_panel(center_width, cx)),
                    )
                    .when(
                        !panes.show_right
                            && self.controller.state.narrow_editor
                            && (panes.show_left || !self.controller.state.narrow_left),
                        |body| body.child(self.editor_panel(center_width, cx)),
                    )
                    .when(
                        !panes.show_left && self.controller.state.narrow_left,
                        |body| body.child(self.left_panel(center_width, cx)),
                    )
                    .when(panes.show_right, |body| {
                        body.child(
                            div()
                                .id("right-splitter")
                                .w(px(6.))
                                .h_full()
                                .bg(rgb(theme::resolve(theme::BORDER)))
                                .cursor_col_resize()
                                .on_mouse_down(
                                    MouseButton::Left,
                                    move |event: &gpui::MouseDownEvent, _, cx| {
                                        let _ = weak_right.update(cx, |this, _| {
                                            this.dragging =
                                                Some((false, f32::from(event.position.x)));
                                        });
                                    },
                                ),
                        )
                    })
                    .when(panes.show_right, |body| {
                        body.child(self.editor_panel(panes.right, cx))
                    }),
            )
            .child(
                div()
                    .h(px(28.))
                    .px_3()
                    .bg(rgb(theme::resolve(theme::SURFACE_MUTED)))
                    .text_color(rgb(theme::resolve(theme::TEXT_MUTED)))
                    .child(
                        self.controller
                            .state
                            .flashes
                            .last()
                            .map(|f| f.text.clone())
                            .unwrap_or_else(|| "Ready".into()),
                    ),
            )
            .children(floating)
    }
}
