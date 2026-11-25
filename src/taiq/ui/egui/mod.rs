use crate::ui::common::agent_settings::{self, AgentSection};
use crate::ui::common::agent_visual::AgentVisual;
use crate::ui::common::{
    AdminSection, CenterView, ChatActivityKind, ChatRole, DocumentView, FlashLevel, LeftView,
    UiController, UiEvent,
};
use crate::ui::common::{inline, theme};
use eframe::egui::{self, Color32, RichText};

fn color(value: u32) -> Color32 {
    let value = theme::resolve(value);
    Color32::from_rgb((value >> 16) as u8, (value >> 8) as u8, value as u8)
}

fn inline_label(ui: &mut egui::Ui, source: &str) {
    let parsed = inline::parse(source);
    let mut job = egui::text::LayoutJob::default();
    let mut cursor = 0;
    let base = egui::TextStyle::Body.resolve(ui.style());
    for (range, style) in parsed.spans {
        let normal = egui::text::TextFormat {
            font_id: base.clone(),
            color: color(theme::TEXT),
            ..Default::default()
        };
        job.append(&parsed.plain[cursor..range.start], 0.0, normal.clone());
        let mut format = normal;
        match style {
            inline::InlineStyle::Strong => format.font_id.size += 1.0,
            inline::InlineStyle::Emphasis => format.italics = true,
            inline::InlineStyle::Code => {
                format.font_id = egui::TextStyle::Monospace.resolve(ui.style());
                format.background = color(theme::SURFACE_MUTED);
            }
            inline::InlineStyle::Link(_) => {
                format.color = color(theme::PRIMARY);
                format.underline = egui::Stroke::new(1.0, color(theme::PRIMARY));
            }
        }
        job.append(&parsed.plain[range.clone()], 0.0, format);
        cursor = range.end;
    }
    job.append(
        &parsed.plain[cursor..],
        0.0,
        egui::text::TextFormat {
            font_id: base,
            color: color(theme::TEXT),
            ..Default::default()
        },
    );
    ui.label(job);
}

fn install_forgeiq_theme(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    fonts.font_data.insert(
        "Inter".into(),
        egui::FontData::from_static(theme::FONT_BYTES).into(),
    );
    fonts
        .families
        .get_mut(&egui::FontFamily::Proportional)
        .unwrap()
        .insert(0, theme::FONT_FAMILY.into());
    ctx.set_fonts(fonts);
    let mut visuals = if theme::is_dark() {
        egui::Visuals::dark()
    } else {
        egui::Visuals::light()
    };
    visuals.panel_fill = color(theme::SURFACE);
    visuals.window_fill = color(theme::SURFACE);
    visuals.faint_bg_color = color(theme::SURFACE_MUTED);
    visuals.extreme_bg_color = color(theme::BG);
    visuals.selection.bg_fill = color(theme::PRIMARY_SOFT);
    visuals.selection.stroke.color = color(theme::PRIMARY);
    visuals.widgets.active.bg_fill = color(theme::PRIMARY_SOFT);
    visuals.widgets.hovered.bg_fill = color(theme::PRIMARY_SOFT);
    visuals.widgets.noninteractive.fg_stroke.color = color(theme::TEXT);
    ctx.set_theme(if theme::is_dark() {
        egui::Theme::Dark
    } else {
        egui::Theme::Light
    });
    ctx.set_visuals(visuals);
}

pub fn run(controller: UiController) -> eframe::Result {
    eframe::run_native(
        "TaIQ desktop",
        eframe::NativeOptions {
            viewport: egui::ViewportBuilder::default()
                .with_inner_size([1440.0, 850.0])
                .with_min_inner_size([900.0, 560.0]),
            ..Default::default()
        },
        Box::new(move |cc| {
            install_forgeiq_theme(&cc.egui_ctx);
            egui_extras::install_image_loaders(&cc.egui_ctx);
            Ok(Box::new(EguiApp {
                controller,
                confirm_delete: None,
                agent_query: String::new(),
                agent_cursor: 0,
                model_query: String::new(),
                model_cursor: 0,
                model_picker_open: false,
                reasoning_open: false,
            }))
        }),
    )
}

struct EguiApp {
    controller: UiController,
    confirm_delete: Option<String>,
    agent_query: String,
    agent_cursor: usize,
    model_query: String,
    model_cursor: usize,
    model_picker_open: bool,
    reasoning_open: bool,
}

fn fixed_panel(
    ui: &mut egui::Ui,
    id: &'static str,
    width: f32,
    height: f32,
    align: egui::Align,
    fill: Color32,
    contents: impl FnOnce(&mut egui::Ui),
) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(width, height), egui::Sense::hover());
    ui.painter().rect_filled(rect, 0.0, fill);
    let mut child = ui.new_child(
        egui::UiBuilder::new()
            .id_salt(id)
            .max_rect(rect)
            .layout(egui::Layout::top_down(align)),
    );
    child.set_clip_rect(rect);
    contents(&mut child);
}

impl eframe::App for EguiApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        let shortcut = |key| ctx.input(|input| input.modifiers.command && input.key_pressed(key));
        if shortcut(egui::Key::S) {
            self.controller.dispatch(UiEvent::SaveFile);
        }
        if shortcut(egui::Key::N) {
            self.controller.dispatch(UiEvent::NewChat);
        }
        if shortcut(egui::Key::W) {
            self.controller.dispatch(UiEvent::CloseEditor);
        }
        if shortcut(egui::Key::Tab) {
            let tabs = &self.controller.state.center_tabs;
            if let Some(index) = tabs
                .iter()
                .position(|tab| *tab == self.controller.state.center_view)
            {
                let backwards = ctx.input(|input| input.modifiers.shift);
                let next = if backwards {
                    (index + tabs.len() - 1) % tabs.len()
                } else {
                    (index + 1) % tabs.len()
                };
                self.controller.dispatch(UiEvent::SelectCenter(tabs[next]));
            }
        }
        self.controller.poll();
        if self.controller.state.busy
            || self.controller.state.admin_busy
            || self.controller.state.web_loading
            || !self.controller.state.flashes.is_empty()
        {
            ctx.request_repaint_after(std::time::Duration::from_millis(80));
        }
        egui::Panel::top("header")
            .frame(
                egui::Frame::new()
                    .fill(color(theme::HEADER))
                    .inner_margin(egui::Margin::symmetric(14, 9)),
            )
            .show(ui, |ui| {
                ui.set_min_height(38.0);
                ui.horizontal(|ui| {
                    ui.add(
                        egui::Image::new(egui::include_image!("../common/icons/forgeiq.svg"))
                            .fit_to_exact_size(egui::vec2(26.0, 26.0)),
                    );
                    ui.heading(RichText::new("TaIQ").color(color(theme::HEADER_TEXT)));
                    ui.separator();
                    ui.label(RichText::new("Workspace").color(color(theme::HEADER_TEXT)));
                    if ui
                        .add(
                            egui::Button::new(
                                RichText::new("Refresh files").color(color(theme::HEADER_TEXT)),
                            )
                            .fill(color(theme::HEADER)),
                        )
                        .clicked()
                    {
                        self.controller.dispatch(UiEvent::RefreshFiles);
                    }
                    let dark = self.controller.state.dark_mode;
                    if ui.button(if dark { "☀ Light" } else { "☾ Dark" }).clicked() {
                        self.controller.dispatch(UiEvent::SetDarkMode(!dark));
                        install_forgeiq_theme(&ctx);
                    }
                    if ui
                        .button(if self.controller.state.narrow_editor {
                            "Chat"
                        } else {
                            "Editor"
                        })
                        .clicked()
                    {
                        self.controller.dispatch(UiEvent::SetNarrowEditor(
                            !self.controller.state.narrow_editor,
                        ));
                    }
                    if self.controller.state.narrow_left && ui.button("Chat").clicked() {
                        self.controller
                            .dispatch(UiEvent::SelectCenter(CenterView::Chat));
                    }
                });
            });
        egui::Panel::bottom("flash").show(ui, |ui| {
            if let Some(flash) = self.controller.state.flashes.last() {
                let color = if flash.level == FlashLevel::Error {
                    color(theme::DANGER)
                } else {
                    color(0x059669)
                };
                ui.colored_label(color, &flash.text);
            } else {
                ui.label("Ready");
            }
        });
        // egui panel sizes are already in logical points. Dividing them by the
        // display scale made the workbench much too short on high-DPI screens.
        let available = ui.available_size();
        let height = available.y.max(0.0);
        let panes = crate::ui::common::layout::PaneLayout::fit(
            available.x,
            48.0,
            self.controller.state.left_width,
            self.controller.state.editor_width,
        );
        let left_width = panes.left;
        let editor_width = panes.right;
        let center_width = panes.center;
        ui.spacing_mut().item_spacing.x = 0.0;
        ui.horizontal(|ui| {
            fixed_panel(
                ui,
                "rail",
                48.0,
                height,
                egui::Align::Center,
                color(theme::SURFACE),
                |ui| {
                    ui.vertical_centered(|ui| {
                        for view in LeftView::ALL {
                            let selected = self.controller.state.left_view == view;
                            let icon = match view {
                                LeftView::Files => {
                                    egui::include_image!("../common/icons/files.svg")
                                }
                                LeftView::Agents => {
                                    egui::include_image!("../common/icons/agents.svg")
                                }
                                LeftView::Sessions => {
                                    egui::include_image!("../common/icons/sessions.svg")
                                }
                                LeftView::Connectors => {
                                    egui::include_image!("../common/icons/connectors.svg")
                                }
                                LeftView::Settings => {
                                    egui::include_image!("../common/icons/settings.svg")
                                }
                            };
                            let response = egui::Frame::new()
                                .fill(if selected {
                                    color(theme::PRIMARY_SOFT)
                                } else {
                                    color(theme::SURFACE)
                                })
                                .corner_radius(9)
                                .inner_margin(8)
                                .show(ui, |ui| {
                                    ui.add(
                                        egui::Image::new(icon)
                                            .fit_to_exact_size(egui::vec2(20.0, 20.0))
                                            .sense(egui::Sense::click()),
                                    )
                                })
                                .inner
                                .on_hover_text(view.label());
                            if response.clicked() {
                                self.controller.dispatch(UiEvent::SelectView(view));
                            }
                            ui.add_space(5.0);
                        }
                    });
                },
            );
            if panes.show_left {
                fixed_panel(
                    ui,
                    "files",
                    left_width,
                    height,
                    egui::Align::Min,
                    color(theme::SURFACE_MUTED),
                    |ui| {
                        self.left_panel(ui);
                    },
                );
                let (split, drag) =
                    ui.allocate_exact_size(egui::vec2(6.0, height), egui::Sense::drag());
                let drag = drag.on_hover_cursor(egui::CursorIcon::ResizeHorizontal);
                ui.painter().rect_filled(split, 0.0, color(theme::BORDER));
                if drag.dragged() {
                    let delta = ui.input(|i| i.pointer.delta().x);
                    self.controller.dispatch(UiEvent::ResizePanels(
                        (left_width + delta).clamp(180.0, 550.0),
                        editor_width,
                    ));
                }
            }
            fixed_panel(
                ui,
                "center",
                center_width,
                height,
                egui::Align::Min,
                color(theme::SURFACE),
                |ui| {
                    if !panes.show_left && self.controller.state.narrow_left {
                        self.left_panel(ui);
                    } else if !panes.show_right && self.controller.state.narrow_editor {
                        self.editor_panel(ui);
                    } else {
                        self.center_panel(ui);
                    }
                },
            );
            if panes.show_right {
                let (split, drag) =
                    ui.allocate_exact_size(egui::vec2(6.0, height), egui::Sense::drag());
                let drag = drag.on_hover_cursor(egui::CursorIcon::ResizeHorizontal);
                ui.painter().rect_filled(split, 0.0, color(theme::BORDER));
                if drag.dragged() {
                    let delta = ui.input(|i| i.pointer.delta().x);
                    self.controller.dispatch(UiEvent::ResizePanels(
                        left_width,
                        (editor_width - delta).clamp(250.0, 650.0),
                    ));
                }
                fixed_panel(
                    ui,
                    "editor",
                    editor_width,
                    height,
                    egui::Align::Min,
                    color(theme::SURFACE),
                    |ui| {
                        self.editor_panel(ui);
                    },
                );
            }
        });
        let floating: Vec<_> = self
            .controller
            .state
            .documents
            .iter()
            .filter(|doc| self.controller.state.floating_documents.contains(&doc.path))
            .cloned()
            .collect();
        for doc in floating {
            let mut open = true;
            let saved = self
                .controller
                .state
                .floating_bounds
                .get(&doc.path)
                .copied()
                .unwrap_or([180.0, 90.0, 680.0, 520.0]);
            let shown = egui::Window::new(&doc.path)
                .id(egui::Id::unique(("floating-document", &doc.path)))
                .open(&mut open)
                .default_pos([saved[0], saved[1]])
                .default_size([saved[2], saved[3]])
                .show(&ctx, |ui| {
                    if doc.dirty() && ui.button("Save").clicked() {
                        self.controller
                            .dispatch(UiEvent::SaveDocument(doc.path.clone()));
                    }
                    let mut text = doc.text.clone();
                    let language = std::path::Path::new(&doc.path)
                        .extension()
                        .and_then(|v| v.to_str())
                        .unwrap_or("txt")
                        .to_owned();
                    let code_theme = egui_extras::syntax_highlighting::CodeTheme::from_memory(
                        ui.ctx(),
                        ui.style(),
                    );
                    let mut layouter =
                        |ui: &egui::Ui, buf: &dyn egui::TextBuffer, wrap_width: f32| {
                            let mut job = egui_extras::syntax_highlighting::highlight(
                                ui.ctx(),
                                ui.style(),
                                &code_theme,
                                buf.as_str(),
                                &language,
                            );
                            job.wrap.max_width = wrap_width;
                            ui.fonts_mut(|fonts| fonts.layout_job(job))
                        };
                    if ui
                        .add_sized(
                            ui.available_size(),
                            egui::TextEdit::multiline(&mut text)
                                .font(egui::TextStyle::Monospace)
                                .code_editor()
                                .layouter(&mut layouter)
                                .interactive(doc.editable),
                        )
                        .changed()
                    {
                        self.controller
                            .dispatch(UiEvent::SetDocumentText(doc.path.clone(), text));
                    }
                });
            if let Some(shown) = shown {
                let rect = shown.response.rect;
                let current = [rect.min.x, rect.min.y, rect.width(), rect.height()];
                if current.iter().zip(saved).any(|(a, b)| (*a - b).abs() > 1.0) {
                    self.controller
                        .dispatch(UiEvent::SetFloatingBounds(doc.path.clone(), current));
                }
            }
            if !open {
                self.controller
                    .dispatch(UiEvent::ToggleDocumentFloating(doc.path));
            }
        }
    }
}

impl EguiApp {
    fn left_panel(&mut self, ui: &mut egui::Ui) {
        ui.heading(self.controller.state.left_view.label());
        ui.separator();
        match self.controller.state.left_view {
            LeftView::Files => {
                if ui.button("Open workspace folder…").clicked() {
                    if let Some(path) = rfd::FileDialog::new().pick_folder() {
                        self.controller.dispatch(UiEvent::AddWorkspace(path));
                    }
                }
                let mut query = self.controller.state.file_search.clone();
                if ui
                    .add(egui::TextEdit::singleline(&mut query).hint_text("⌕ Search files"))
                    .changed()
                {
                    self.controller.dispatch(UiEvent::SetFileSearch(query));
                }
                if ui.button("Search all folders").clicked() {
                    self.controller.dispatch(UiEvent::SearchFiles);
                }
                ui.horizontal(|ui| {
                    let mut draft = self.controller.state.new_file_draft.clone();
                    if ui.text_edit_singleline(&mut draft).changed() {
                        self.controller.dispatch(UiEvent::SetNewFileDraft(draft));
                    }
                    if ui.button("+").on_hover_text("Create file").clicked() {
                        let path = self.controller.state.new_file_draft.trim().to_owned();
                        if !path.is_empty() {
                            self.controller.dispatch(UiEvent::CreateFile(path));
                        }
                    }
                    if ui.button("▣").on_hover_text("Create folder").clicked() {
                        let path = self.controller.state.new_file_draft.trim().to_owned();
                        if !path.is_empty() {
                            self.controller.dispatch(UiEvent::CreateFolder(path));
                        }
                    }
                });
                ui.horizontal(|ui| {
                    if ui.button("Rename / move").clicked() {
                        if let Some(from) = self.controller.state.selected_path.clone() {
                            let to = self.controller.state.new_file_draft.trim().to_owned();
                            if !to.is_empty() {
                                self.controller.dispatch(UiEvent::RenameFile(from, to));
                            }
                        }
                    }
                    if ui.button("Delete").clicked() {
                        self.confirm_delete = self.controller.state.selected_path.clone();
                    }
                });
                ui.horizontal(|ui| {
                    if ui.button("⇧ Import").clicked() {
                        if let Some(source) = rfd::FileDialog::new().pick_file() {
                            if let Some(name) = source.file_name() {
                                let folder = self.controller.selected_folder();
                                let destination = if folder.is_empty() {
                                    name.to_string_lossy().into_owned()
                                } else {
                                    format!("{folder}/{}", name.to_string_lossy())
                                };
                                self.controller
                                    .dispatch(UiEvent::ImportFile(source, destination));
                            }
                        }
                    }
                    if ui.button("⇩ Export").clicked() {
                        if let Some(source) = self.controller.state.selected_path.clone() {
                            if let Some(destination) = rfd::FileDialog::new().save_file() {
                                self.controller
                                    .dispatch(UiEvent::ExportFile(source, destination));
                            }
                        }
                    }
                });
                if let Some(path) = self.confirm_delete.clone() {
                    ui.colored_label(Color32::LIGHT_RED, format!("Delete {path}?"));
                    ui.horizontal(|ui| {
                        if ui.button("Confirm delete").clicked() {
                            self.controller.dispatch(UiEvent::DeleteFile(path));
                            self.confirm_delete = None;
                        }
                        if ui.button("Cancel").clicked() {
                            self.confirm_delete = None;
                        }
                    });
                }
                egui::ScrollArea::vertical().show(ui, |ui| {
                    let roots = self.controller.logical_roots();
                    for root in roots
                        .iter()
                        .filter(|root| root.id == self.controller.state.active_logical_root)
                    {
                        ui.horizontal(|ui| {
                            ui.add(
                                egui::Image::new(egui::include_image!(
                                    "../common/icons/folder.svg"
                                ))
                                .fit_to_exact_size(egui::vec2(17.0, 17.0)),
                            );
                            ui.strong(format!(
                                "{}{}",
                                root.label,
                                if root.read_only { " · Read only" } else { "" }
                            ));
                        });
                    }
                    let nodes = if self.controller.state.search_active {
                        self.controller.state.search_results.clone()
                    } else {
                        self.controller.state.tree.clone()
                    };
                    for node in nodes {
                        ui.horizontal(|ui| {
                            ui.add_space(12.0 + node.depth as f32 * 12.0);
                            let icon = if node.entry.is_dir {
                                egui::include_image!("../common/icons/folder.svg")
                            } else {
                                egui::include_image!("../common/icons/file.svg")
                            };
                            ui.add(
                                egui::Image::new(icon).fit_to_exact_size(egui::vec2(16.0, 16.0)),
                            );
                            let label = if node.entry.is_dir {
                                format!(
                                    "{} {}",
                                    if node.expanded { "v" } else { ">" },
                                    node.entry.name
                                )
                            } else {
                                node.entry.name.clone()
                            };
                            if ui.button(label).clicked() {
                                self.controller
                                    .dispatch(UiEvent::SelectPath(node.entry.path.clone()));
                                if node.entry.is_dir {
                                    if self.controller.state.search_active {
                                        self.controller
                                            .dispatch(UiEvent::SetFileSearch(String::new()));
                                    }
                                    self.controller
                                        .dispatch(UiEvent::ToggleFolder(node.entry.path));
                                } else {
                                    self.controller.dispatch(UiEvent::OpenFile(node.entry.path));
                                }
                            }
                        });
                    }
                    ui.separator();
                    let active_root = self.controller.state.active_logical_root.clone();
                    for root in roots.into_iter().filter(|root| root.id != active_root) {
                        if ui
                            .button(format!(
                                "▸ {}{}",
                                root.label,
                                if root.read_only { " · Read only" } else { "" }
                            ))
                            .clicked()
                        {
                            self.controller
                                .dispatch(UiEvent::SelectLogicalRoot(root.id));
                        }
                    }
                });
            }
            LeftView::Agents => {
                let agents: Vec<_> = self
                    .controller
                    .config
                    .agents
                    .agents
                    .iter()
                    .filter(|a| a.enabled != Some(false) && a.hidden != Some(true))
                    .map(|a| {
                        (
                            a.id.clone(),
                            a.name.clone(),
                            AgentVisual::from_agent(a, &self.controller.config_path),
                            a.default == Some(true),
                        )
                    })
                    .collect();
                egui::ScrollArea::vertical().show(ui, |ui| {
                    for (id, name, visual, _is_default) in agents {
                        let row_width = ui.available_width();
                        egui::Frame::new()
                            .fill(if self.controller.state.agent_id == id {
                                color(theme::PRIMARY_SOFT)
                            } else {
                                color(theme::SURFACE)
                            })
                            .corner_radius(6)
                            .inner_margin(egui::Margin::symmetric(8, 5))
                            .show(ui, |ui| {
                                ui.set_min_width((row_width - 16.0).max(0.0));
                                ui.horizontal(|ui| {
                                    let selected = self.controller.state.agent_id == id;
                                    let avatar_clicked = if let Some(bytes) = visual.avatar_bytes()
                                    {
                                        ui.add(
                                            egui::Image::from_bytes(
                                                format!("agent-avatar:{id}.png"),
                                                bytes,
                                            )
                                            .fit_to_exact_size(egui::vec2(24.0, 24.0))
                                            .sense(egui::Sense::click()),
                                        )
                                        .clicked()
                                    } else {
                                        ui.add(
                                            egui::Button::new(&visual.initials)
                                                .min_size(egui::vec2(24.0, 24.0)),
                                        )
                                        .clicked()
                                    };
                                    let name_clicked =
                                        ui.selectable_label(selected, &name).clicked();
                                    if avatar_clicked || name_clicked {
                                        self.controller.dispatch(UiEvent::SelectAgent(id.clone()));
                                    }
                                    ui.label(
                                        RichText::new(format!("· {}", visual.category))
                                            .small()
                                            .color(color(theme::TEXT_MUTED)),
                                    );
                                });
                            });
                    }
                });
            }
            LeftView::Sessions => {
                let mut query = self.controller.state.session_query.clone();
                if ui
                    .add(egui::TextEdit::singleline(&mut query).hint_text("⌕ Search history"))
                    .changed()
                {
                    self.controller.dispatch(UiEvent::SetSessionQuery(query));
                }
                egui::ScrollArea::vertical().show(ui, |ui| {
                    let sessions: Vec<_> = self
                        .controller
                        .state
                        .sessions
                        .iter()
                        .rev()
                        .filter(|s| {
                            !s.messages.is_empty()
                                || s.id == self.controller.state.active_session_id
                        })
                        .filter(|s| {
                            self.controller.state.session_query.is_empty()
                                || s.title
                                    .to_lowercase()
                                    .contains(&self.controller.state.session_query.to_lowercase())
                                || s.messages.iter().any(|m| {
                                    m.text.to_lowercase().contains(
                                        &self.controller.state.session_query.to_lowercase(),
                                    )
                                })
                        })
                        .map(|s| (s.id.clone(), s.title.clone(), s.agent_id.clone()))
                        .collect();
                    for (id, title, agent) in sessions {
                        if ui
                            .selectable_label(
                                id == self.controller.state.active_session_id,
                                format!("{title}\n{agent}"),
                            )
                            .clicked()
                        {
                            self.controller.dispatch(UiEvent::SelectSession(id));
                            self.controller
                                .dispatch(UiEvent::SelectCenter(CenterView::Chat));
                        }
                    }
                });
            }
            LeftView::Connectors => {
                ui.label("Configured connectors");
                let connectors: Vec<_> = self
                    .controller
                    .config
                    .connectors
                    .iter()
                    .filter(|c| c.enable.as_ref().and_then(|v| v.as_bool()) != Some(false))
                    .map(|c| {
                        (
                            c.id.clone().unwrap_or_default(),
                            c.name
                                .as_deref()
                                .or(c.id.as_deref())
                                .unwrap_or("Connector")
                                .to_owned(),
                        )
                    })
                    .collect();
                for (id, label) in connectors {
                    if ui.button(label).clicked() {
                        self.controller.dispatch(UiEvent::SelectConnector(id));
                    }
                }
            }
            LeftView::Settings => {
                ui.label("Desktop management");
                for section in AdminSection::ALL {
                    let selected = self.controller.state.center_view == CenterView::Admin
                        && self.controller.state.admin_section == section;
                    if ui.selectable_label(selected, section.label()).clicked() {
                        self.controller.dispatch(UiEvent::SelectAdmin(section));
                    }
                }
                if ui.button("Members").clicked() {
                    self.controller
                        .dispatch(UiEvent::Unsupported("members management"));
                }
                if ui.button("Plugins").clicked() {
                    self.controller
                        .dispatch(UiEvent::Unsupported("plugin management"));
                }
                ui.separator();
                ui.label("Installed plugins");
                for plugin in crate::plugins::all() {
                    ui.small(plugin.id);
                }
            }
        }
    }

    fn center_panel(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            for tab in self.controller.state.center_tabs.clone() {
                let label = match tab {
                    CenterView::Chat => "Chat",
                    CenterView::Web => "Web",
                    CenterView::Changes => "Changes",
                    CenterView::Admin => "Admin",
                };
                let (_, dropped) = ui.dnd_drop_zone::<CenterView, _>(egui::Frame::new(), |ui| {
                    ui.dnd_drag_source(
                        ui.scope_id().with(format!("center-tab:{tab:?}")),
                        tab,
                        |ui| {
                            if ui
                                .selectable_label(self.controller.state.center_view == tab, label)
                                .clicked()
                            {
                                self.controller.dispatch(UiEvent::SelectCenter(tab));
                            }
                        },
                    );
                });
                if let Some(source) = dropped {
                    self.controller
                        .dispatch(UiEvent::MoveCenterTab(*source, tab));
                }
            }
        });
        ui.separator();
        match self.controller.state.center_view {
            CenterView::Chat => {
                let active_agent = self
                    .controller
                    .config
                    .agents
                    .agents
                    .iter()
                    .find(|a| a.id == self.controller.state.agent_id)
                    .map(|a| {
                        (
                            a.name.clone(),
                            AgentVisual::from_agent(a, &self.controller.config_path),
                        )
                    });
                let agent = active_agent
                    .as_ref()
                    .map(|(name, _)| name.clone())
                    .unwrap_or_else(|| "Assistant".into());
                ui.horizontal_wrapped(|ui| {
                    let sessions: Vec<_> = self
                        .controller
                        .state
                        .sessions
                        .iter()
                        .rev()
                        .filter(|session| {
                            !session.messages.is_empty()
                                || session.id == self.controller.state.active_session_id
                        })
                        .take(8)
                        .map(|session| {
                            let agent_name = self
                                .controller
                                .config
                                .agents
                                .agents
                                .iter()
                                .find(|agent| agent.id == session.agent_id)
                                .map(|agent| agent.name.as_str())
                                .unwrap_or(&session.agent_id);
                            (
                                session.id.clone(),
                                format!("{} · {}", session.title, agent_name),
                            )
                        })
                        .collect();
                    for (id, title) in sessions {
                        if ui
                            .selectable_label(id == self.controller.state.active_session_id, title)
                            .clicked()
                        {
                            self.controller.dispatch(UiEvent::SelectSession(id));
                        }
                    }
                });
                ui.horizontal(|ui| {
                    if let Some((_, visual)) = &active_agent {
                        if let Some(bytes) = visual.avatar_bytes() {
                            ui.add(
                                egui::Image::from_bytes(
                                    format!("agent-avatar:{}.png", self.controller.state.agent_id),
                                    bytes,
                                )
                                .fit_to_exact_size(egui::vec2(27.0, 27.0)),
                            );
                        } else {
                            ui.label(&visual.initials);
                        }
                        ui.heading(&agent);
                        ui.label(
                            RichText::new(format!("· {}", visual.category))
                                .color(color(theme::TEXT_MUTED)),
                        );
                    } else {
                        ui.heading(&agent);
                    }
                    let agent_options: Vec<_> = self
                        .controller
                        .config
                        .agents
                        .agents
                        .iter()
                        .filter(|a| a.enabled != Some(false) && a.hidden != Some(true))
                        .map(|a| {
                            (
                                a.id.clone(),
                                a.name.clone(),
                                AgentVisual::from_agent(a, &self.controller.config_path),
                            )
                        })
                        .collect();
                    ui.menu_button(
                        egui::Image::new(egui::include_image!("../common/icons/chevron_down.svg"))
                            .fit_to_exact_size(egui::vec2(15.0, 15.0)),
                        |ui| {
                            ui.set_min_width(260.0);
                            let response = ui.add(
                                egui::TextEdit::singleline(&mut self.agent_query)
                                    .hint_text("Search agents by name or role"),
                            );
                            response.request_focus();
                            let filtered: Vec<_> = agent_options
                                .into_iter()
                                .filter(|(_, name, visual)| {
                                    let query = self.agent_query.to_lowercase();
                                    name.to_lowercase().contains(&query)
                                        || visual.category.to_lowercase().contains(&query)
                                })
                                .collect();
                            if ui.input(|i| i.key_pressed(egui::Key::ArrowDown)) {
                                self.agent_cursor =
                                    (self.agent_cursor + 1).min(filtered.len().saturating_sub(1));
                            }
                            if ui.input(|i| i.key_pressed(egui::Key::ArrowUp)) {
                                self.agent_cursor = self.agent_cursor.saturating_sub(1);
                            }
                            let enter = ui.input(|i| i.key_pressed(egui::Key::Enter));
                            if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                                ui.close();
                            }
                            for (index, (id, name, visual)) in filtered.iter().enumerate() {
                                ui.horizontal(|ui| {
                                    if let Some(bytes) = visual.avatar_bytes() {
                                        ui.add(
                                            egui::Image::from_bytes(
                                                format!("agent-picker:{id}.png"),
                                                bytes,
                                            )
                                            .fit_to_exact_size(egui::vec2(23.0, 23.0)),
                                        );
                                    } else {
                                        ui.label(&visual.initials);
                                    }
                                    if ui
                                        .selectable_label(
                                            index == self.agent_cursor
                                                || id == &self.controller.state.agent_id,
                                            format!("{name} · {}", visual.category),
                                        )
                                        .clicked()
                                        || (enter && index == self.agent_cursor)
                                    {
                                        self.controller.dispatch(UiEvent::SelectAgent(id.clone()));
                                        self.model_picker_open = false;
                                        self.reasoning_open = false;
                                        self.agent_query.clear();
                                        self.agent_cursor = 0;
                                        ui.close();
                                    }
                                });
                            }
                        },
                    );
                    if ui
                        .add(egui::Button::image(
                            egui::Image::new(egui::include_image!("../common/icons/chat.svg"))
                                .fit_to_exact_size(egui::vec2(18.0, 18.0)),
                        ))
                        .on_hover_text("Chat")
                        .clicked()
                    {
                        if self.controller.state.agent_settings.is_some() {
                            self.controller.dispatch(UiEvent::CloseAgentSettings);
                        }
                    }
                    if ui
                        .add(egui::Button::image(
                            egui::Image::new(egui::include_image!("../common/icons/plus.svg"))
                                .fit_to_exact_size(egui::vec2(18.0, 18.0)),
                        ))
                        .on_hover_text("New chat")
                        .clicked()
                    {
                        self.controller.dispatch(UiEvent::NewChat);
                    }
                    if ui
                        .add(egui::Button::image(
                            egui::Image::new(egui::include_image!("../common/icons/sessions.svg"))
                                .fit_to_exact_size(egui::vec2(18.0, 18.0)),
                        ))
                        .on_hover_text("Sessions")
                        .clicked()
                    {
                        self.controller
                            .dispatch(UiEvent::SelectView(LeftView::Sessions));
                    }
                    ui.menu_button(
                        egui::Image::new(egui::include_image!("../common/icons/prompt.svg"))
                            .fit_to_exact_size(egui::vec2(18.0, 18.0)),
                        |ui| {
                            let prompts = crate::ui::common::agent_prompts(
                                &self.controller.config,
                                &self.controller.state.agent_id,
                            );
                            if prompts.is_empty() {
                                ui.label("No prompts for this agent");
                            }
                            for (title, prompt) in prompts {
                                if ui.button(title).clicked() {
                                    self.controller.dispatch(UiEvent::SetChatDraft(prompt));
                                    ui.close();
                                }
                            }
                        },
                    )
                    .response
                    .on_hover_text("Prompts");
                    if ui
                        .add(egui::Button::image(
                            egui::Image::new(egui::include_image!("../common/icons/settings.svg"))
                                .fit_to_exact_size(egui::vec2(18.0, 18.0)),
                        ))
                        .on_hover_text("Agent settings")
                        .clicked()
                    {
                        self.controller.dispatch(UiEvent::OpenAgentSettings);
                    }
                    if self.controller.state.busy {
                        ui.spinner();
                    }
                });
                if self.controller.state.agent_settings.is_some() {
                    self.agent_settings_body(ui);
                    return;
                }
                ui.with_layout(egui::Layout::bottom_up(egui::Align::Min), |ui| {
                    egui::Frame::new()
                        .fill(color(theme::SURFACE))
                        .stroke(egui::Stroke::new(1.0, color(theme::BORDER)))
                        .corner_radius(14)
                        .inner_margin(egui::Margin::symmetric(12, 10))
                        .show(ui, |ui| {
                            ui.with_layout(egui::Layout::top_down(egui::Align::Min), |ui| {
                            ui.horizontal_wrapped(|ui| {
                                if self.controller.state.attach_open_file {
                                    if let Some(doc) = &self.controller.state.editor {
                                        if ui.button(format!("{} ×", doc.path)).on_hover_text("Remove open file from chat context").clicked() {
                                            self.controller.dispatch(UiEvent::SetAttachOpenFile(false));
                                        }
                                    }
                                }
                                let names: Vec<_> = self
                                    .controller
                                    .state
                                    .attachments
                                    .iter()
                                    .enumerate()
                                    .map(|(i, a)| (i, a.name.clone()))
                                    .collect();
                                for (index, name) in names {
                                    if ui.button(format!("{name} ×")).on_hover_text("Remove attachment").clicked() {
                                        self.controller.dispatch(UiEvent::RemoveAttachment(index));
                                    }
                                }
                            });
                            ui.horizontal(|ui| {
                                let mut draft = self.controller.state.chat_draft.clone();
                                let response = ui.add(
                                    egui::TextEdit::multiline(&mut draft)
                                        .id(egui::Id::unique("chat-composer"))
                                        .hint_text(format!("Ask {agent}…"))
                                        .desired_rows(2)
                                        .desired_width((ui.available_width() - 88.0).max(160.0)),
                                );
                                if response.changed() { self.controller.dispatch(UiEvent::SetChatDraft(draft)); }
                                let enter = response.has_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter) && !input.modifiers.shift);
                                if ui.add(egui::Button::image(egui::Image::new(egui::include_image!("../common/icons/mic.svg"))
                                    .fit_to_exact_size(egui::vec2(18.0, 18.0))))
                                    .on_hover_text("Windows voice typing").clicked() {
                                    ui.memory_mut(|memory| memory.request_focus(egui::Id::unique("chat-composer")));
                                    self.controller.dispatch(UiEvent::StartVoice);
                                }
                                if ui.add(egui::Button::image(egui::Image::new(egui::include_image!("../common/icons/send.svg"))
                                    .fit_to_exact_size(egui::vec2(19.0, 19.0))).fill(color(theme::PRIMARY)))
                                    .on_hover_text(if self.controller.state.busy { "Queue prompt" } else { "Send prompt" }).clicked() || enter {
                                    self.controller.dispatch(UiEvent::SendChat);
                                }
                            });
                            ui.horizontal_wrapped(|ui| {
                                if ui.add(egui::Button::image(egui::Image::new(egui::include_image!("../common/icons/attach.svg"))
                                    .fit_to_exact_size(egui::vec2(17.0, 17.0))))
                                    .on_hover_text("Attach files").clicked() {
                                    if let Some(files) = rfd::FileDialog::new().pick_files() {
                                        self.controller.dispatch(UiEvent::AttachFiles(files));
                                    }
                                }
                                if ui.add_enabled(self.controller.state.editor.is_some(), egui::Button::image(egui::Image::new(egui::include_image!("../common/icons/file_context.svg"))
                                    .fit_to_exact_size(egui::vec2(17.0, 17.0))).selected(self.controller.state.attach_open_file))
                                    .on_hover_text("Use open file as context").clicked() {
                                    self.controller.dispatch(UiEvent::SetAttachOpenFile(!self.controller.state.attach_open_file));
                                }
                                let mut memory = self.controller.state.memory_enabled;
                                if ui.add(egui::Button::image(egui::Image::new(egui::include_image!("../common/icons/memory.svg"))
                                    .fit_to_exact_size(egui::vec2(17.0, 17.0))).selected(memory))
                                    .on_hover_text("Session memory").clicked() {
                                    memory = !memory;
                                    self.controller.dispatch(UiEvent::SetMemoryEnabled(memory));
                                }
                                let remembered = self.controller.state.chat.len().saturating_sub(self.controller.state.memory_start);
                                if ui.add_enabled(!self.controller.state.busy && remembered > 0, egui::Button::new(format!("{remembered} in context ×")))
                                    .on_hover_text("Clear agent context while keeping visible chat history").clicked() {
                                    self.controller.dispatch(UiEvent::ClearMemory);
                                }
                                let effective = crate::ui::common::model_picker::effective(&self.controller.config,
                                    &self.controller.state.agent_id, &self.controller.state.selected_model);
                                let model_label = effective.map(|model| model.name.as_str()).unwrap_or("Auto");
                                let provider = effective.map(|model| model.provider.as_str()).unwrap_or("?");
                                if let Some(path) = crate::ui::common::model_picker::provider_icon_path(
                                    &self.controller.config, &self.controller.config_path, provider) {
                                    if let Some(bytes) = crate::ui::common::agent_visual::cached_image_bytes(&path) {
                                        ui.add(egui::Image::from_bytes(format!("provider:{}.png", provider), bytes)
                                            .fit_to_exact_size(egui::vec2(17.0, 17.0)));
                                    }
                                }
                                let model_button = ui.button(format!("{}  {}  {}", provider.chars().next().unwrap_or('?').to_ascii_uppercase(),
                                    model_label, if self.controller.state.selected_model == "auto" { "Auto · session" } else { "Session" }));
                                if model_button.clicked() { self.model_picker_open = !self.model_picker_open; self.reasoning_open = false; }
                                if self.model_picker_open {
                                    let choices = crate::ui::common::model_picker::choices(&self.controller.config);
                                    let pos = model_button.rect.left_top() - egui::vec2(0.0, 318.0);
                                    egui::Window::new("Select model").id(egui::Id::unique("chat-model-popup"))
                                        .fixed_pos(pos).default_width(440.0).resizable(false)
                                        .title_bar(false).show(ui.ctx(), |ui| {
                                            ui.set_min_width(420.0);
                                            let search = ui.add(egui::TextEdit::singleline(&mut self.model_query)
                                                .hint_text("Search models"));
                                            search.request_focus();
                                            let filtered: Vec<_> = choices.iter().filter(|model| {
                                                let q = self.model_query.to_lowercase();
                                                model.name.to_lowercase().contains(&q) || model.provider.to_lowercase().contains(&q)
                                            }).collect();
                                            if ui.input(|i| i.key_pressed(egui::Key::ArrowDown)) {
                                                self.model_cursor = (self.model_cursor + 1).min(filtered.len());
                                            }
                                            if ui.input(|i| i.key_pressed(egui::Key::ArrowUp)) {
                                                self.model_cursor = self.model_cursor.saturating_sub(1);
                                            }
                                            let enter = ui.input(|i| i.key_pressed(egui::Key::Enter));
                                            if ui.input(|i| i.key_pressed(egui::Key::Escape)) { self.model_picker_open = false; }
                                            egui::ScrollArea::vertical().max_height(270.0).show(ui, |ui| {
                                                if ui.selectable_label(self.controller.state.selected_model == "auto" || self.model_cursor == 0,
                                                    "Auto · agent default").clicked() || (enter && self.model_cursor == 0) {
                                                    self.controller.dispatch(UiEvent::SelectModel("auto".into()));
                                                    self.model_picker_open = false;
                                                }
                                                for (index, model) in filtered.iter().enumerate() {
                                                    let selected = self.controller.state.selected_model == model.id || self.model_cursor == index + 1;
                                                    ui.horizontal(|ui| {
                                                        ui.set_min_width(ui.available_width());
                                                        if let Some(path) = crate::ui::common::model_picker::provider_icon_path(
                                                            &self.controller.config, &self.controller.config_path, &model.provider) {
                                                            if let Some(bytes) = crate::ui::common::agent_visual::cached_image_bytes(&path) {
                                                                ui.add(egui::Image::from_bytes(format!("provider:{}.png", model.provider), bytes)
                                                                    .fit_to_exact_size(egui::vec2(17.0, 17.0)));
                                                            }
                                                        }
                                                        if ui.selectable_label(selected, &model.name).clicked()
                                                            || (enter && self.model_cursor == index + 1) {
                                                            self.controller.dispatch(UiEvent::SelectModel(model.id.clone()));
                                                            self.model_picker_open = false;
                                                        }
                                                        ui.label(RichText::new(format!("· {}", model.provider)).small().color(color(theme::TEXT_MUTED)));
                                                        for capability in &model.capabilities {
                                                            ui.add(egui::Image::from_bytes(
                                                                format!("model-capability:{capability}"),
                                                                crate::ui::common::agent_visual::cached_svg_png(
                                                                    crate::ui::common::model_picker::capability_icon(capability), 32
                                                                ).expect("built-in SVG icon"),
                                                            ).fit_to_exact_size(egui::vec2(14.0, 14.0)))
                                                                .on_hover_text(capability);
                                                        }
                                                    });
                                                }
                                            });
                                        });
                                }
                                let levels = crate::ui::common::model_picker::reasoning_levels(&self.controller.config,
                                    &self.controller.state.agent_id, &self.controller.state.selected_model);
                                let reasoning_button = ui.add_enabled(!levels.is_empty(), egui::Button::image(
                                    egui::Image::new(egui::include_image!("../common/icons/reasoning.svg"))
                                        .fit_to_exact_size(egui::vec2(17.0, 17.0))))
                                    .on_hover_text("Reasoning settings");
                                if reasoning_button.clicked() { self.reasoning_open = !self.reasoning_open; self.model_picker_open = false; }
                                if self.reasoning_open && !levels.is_empty() {
                                    let pos = reasoning_button.rect.left_top() - egui::vec2(0.0, 110.0);
                                    egui::Window::new("Reasoning").id(egui::Id::unique("reasoning-popup"))
                                        .fixed_pos(pos).default_width(230.0).resizable(false).show(ui.ctx(), |ui| {
                                            let effective = crate::ui::common::model_picker::effective(&self.controller.config,
                                                &self.controller.state.agent_id, &self.controller.state.selected_model);
                                            let current = self.controller.state.reasoning_effort.as_deref()
                                                .or_else(|| effective.and_then(|model| model.other.get("reasoning_default")).and_then(|v| v.as_str()))
                                                .unwrap_or(&levels[0]);
                                            let mut index = levels.iter().position(|value| value == current).unwrap_or(0);
                                            let label = levels[index].clone();
                                            if ui.add(egui::Slider::new(&mut index, 0..=levels.len()-1).text(label)).changed() {
                                                self.controller.dispatch(UiEvent::SetReasoningEffort(Some(levels[index].clone())));
                                            }
                                            ui.small(format!("{} · session override", levels[index]));
                                            if ui.small_button("Use model default").clicked() {
                                                self.controller.dispatch(UiEvent::SetReasoningEffort(None));
                                            }
                                            if ui.input(|i| i.key_pressed(egui::Key::Escape)) { self.reasoning_open = false; }
                                        });
                                }
                                if ui.add(egui::Button::image(egui::Image::new(egui::include_image!("../common/icons/terminal.svg"))
                                    .fit_to_exact_size(egui::vec2(17.0, 17.0))))
                                    .on_hover_text("Terminal").clicked() {
                                    self.controller.dispatch(UiEvent::ToggleTerminal);
                                }
                                if self.controller.state.busy && ui.small_button("Stop").clicked() {
                                    self.controller.dispatch(UiEvent::CancelChat);
                                }
                                if !self.controller.state.queued_chat.is_empty() { ui.small(format!("{} queued", self.controller.state.queued_chat.len())); }
                            });
                            if !self.controller.state.queued_chat.is_empty() {
                                ui.collapsing(
                                    format!(
                                        "Queued prompts ({})",
                                        self.controller.state.queued_chat.len()
                                    ),
                                    |ui| {
                                        let queued: Vec<_> = self
                                            .controller
                                            .state
                                            .queued_chat
                                            .iter()
                                            .cloned()
                                            .collect();
                                        if ui.small_button("Clear queue").clicked() {
                                            self.controller.dispatch(UiEvent::ClearQueuedChat);
                                        }
                                        if !self.controller.state.busy && ui.small_button("Run next").clicked() {
                                            self.controller.dispatch(UiEvent::RunNextQueuedChat);
                                        }
                                        let queue_len = queued.len();
                                        for (index, queued) in queued.iter().enumerate() {
                                            ui.horizontal(|ui| {
                                                ui.label(format!("{}. {}{}", index + 1, queued.prompt, if queued.attachments.is_empty() { String::new() } else { format!(" ({} files)", queued.attachments.len()) }));
                                                if index > 0 && ui.small_button("↑").clicked() { self.controller.dispatch(UiEvent::MoveQueuedChat(index, index-1)); }
                                                if index + 1 < queue_len && ui.small_button("↓").clicked() { self.controller.dispatch(UiEvent::MoveQueuedChat(index, index+1)); }
                                                if ui.small_button("Remove").clicked() {
                                                    self.controller
                                                        .dispatch(UiEvent::RemoveQueuedChat(index));
                                                }
                                            });
                                        }
                                    },
                                );
                            }
                            });
                        });
                    if let Some(terminal) = self.controller.state.terminals
                        .get(&self.controller.state.active_session_id).filter(|terminal| terminal.open).cloned() {
                        let splitter = ui.allocate_response(egui::vec2(ui.available_width(), 6.0), egui::Sense::drag());
                        if splitter.dragged() {
                            self.controller.dispatch(UiEvent::ResizeTerminal(terminal.height - splitter.drag_delta().y));
                        }
                        splitter.on_hover_cursor(egui::CursorIcon::ResizeVertical);
                        egui::Frame::new().fill(Color32::from_rgb(18, 24, 36))
                            .inner_margin(egui::Margin::symmetric(10, 7)).show(ui, |ui| {
                                ui.set_min_height(terminal.height);
                                ui.set_max_height(terminal.height);
                                ui.horizontal(|ui| {
                                    ui.add(egui::Image::new(egui::include_image!("../common/icons/terminal.svg"))
                                        .fit_to_exact_size(egui::vec2(17.0, 17.0)));
                                    ui.label(RichText::new("Terminal").strong().color(Color32::WHITE));
                                    ui.label(RichText::new(terminal.cwd.display().to_string()).small().color(Color32::LIGHT_GRAY));
                                    if ui.add(egui::Button::image(egui::Image::new(egui::include_image!("../common/icons/stop.svg"))
                                        .fit_to_exact_size(egui::vec2(15.0, 15.0)))).on_hover_text("Interrupt command").clicked() {
                                        self.controller.dispatch(UiEvent::InterruptTerminal);
                                    }
                                    if ui.add(egui::Button::image(egui::Image::new(egui::include_image!("../common/icons/scroll_down.svg"))
                                        .fit_to_exact_size(egui::vec2(15.0, 15.0))).selected(terminal.auto_scroll))
                                        .on_hover_text("Toggle autoscroll").clicked() {
                                        self.controller.dispatch(UiEvent::ToggleTerminalAutoscroll);
                                    }
                                    if ui.add(egui::Button::image(egui::Image::new(egui::include_image!("../common/icons/settings.svg"))
                                        .fit_to_exact_size(egui::vec2(15.0, 15.0)))).on_hover_text("Terminal settings").clicked() {
                                        self.controller.dispatch(UiEvent::ToggleTerminalSettings);
                                    }
                                });
                                if terminal.settings_open {
                                    ui.horizontal(|ui| {
                                        let mut lines = terminal.buffer_lines;
                                        if ui.add(egui::Slider::new(&mut lines, 100..=100_000).text("Buffer lines")).changed() {
                                            self.controller.dispatch(UiEvent::SetTerminalBuffer(lines));
                                        }
                                        ui.small(format!("Persistent · PTY · {}", terminal.shell));
                                    });
                                }
                                let output_height = (terminal.height - if terminal.settings_open { 103.0 } else { 69.0 }).max(45.0);
                                egui::ScrollArea::vertical().max_height(output_height)
                                    .stick_to_bottom(terminal.auto_scroll).show(ui, |ui| {
                                        ui.label(RichText::new(&terminal.output).monospace().color(Color32::LIGHT_GRAY));
                                    });
                                ui.horizontal(|ui| {
                                    ui.label(RichText::new(">").monospace().color(Color32::LIGHT_BLUE));
                                    let mut input = terminal.input.clone();
                                    let response = ui.add(egui::TextEdit::singleline(&mut input)
                                        .id(egui::Id::unique("terminal-command"))
                                        .hint_text("Enter a command").desired_width(ui.available_width() - 35.0));
                                    if response.changed() { self.controller.dispatch(UiEvent::SetTerminalInput(input)); }
                                    if response.has_focus() {
                                        if ui.input(|i| i.key_pressed(egui::Key::ArrowUp)) { self.controller.dispatch(UiEvent::TerminalHistory(-1)); }
                                        if ui.input(|i| i.key_pressed(egui::Key::ArrowDown)) { self.controller.dispatch(UiEvent::TerminalHistory(1)); }
                                    }
                                    if (response.has_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)))
                                        || ui.small_button("Run").clicked() {
                                        self.controller.dispatch(UiEvent::SubmitTerminal);
                                    }
                                });
                            });
                    }
                    egui::ScrollArea::vertical()
                        .auto_shrink([false, false])
                        .stick_to_bottom(true)
                        .show(ui, |ui| {
                            let chat = self.controller.state.chat.clone();
                            for (index, message) in chat.iter().enumerate() {
                                let user = message.role == ChatRole::User;
                                let name = if message.role == ChatRole::User {
                                    "You"
                                } else {
                                    &agent
                                };
                                ui.label(RichText::new(name).strong().color(if user {
                                    color(theme::PRIMARY)
                                } else {
                                    color(theme::TEXT_MUTED)
                                }));
                                egui::Frame::new()
                                    .fill(if user {
                                        color(theme::PRIMARY)
                                    } else {
                                        color(0xf8fafc)
                                    })
                                    .stroke(if user {
                                        egui::Stroke::NONE
                                    } else {
                                        egui::Stroke::new(1.0, color(theme::BORDER_SOFT))
                                    })
                                    .corner_radius(14)
                                    .inner_margin(egui::Margin::symmetric(14, 12))
                                    .show(ui, |ui| {
                                        if message.text.is_empty() {
                                            ui.label("…");
                                        } else if message.role == ChatRole::Assistant {
                                            for block in crate::plugins::viewers::markdown_preview(
                                                &message.text,
                                            ) {
                                                match block {
                                            crate::plugins::viewers::PreviewBlock::Heading(
                                                level,
                                                text,
                                            ) => {
                                                ui.label(
                                                    RichText::new(text)
                                                        .size((25.0 - level as f32 * 2.0).max(14.0))
                                                        .strong(),
                                                );
                                            }
                                            crate::plugins::viewers::PreviewBlock::Paragraph(
                                                text,
                                            ) => {
                                                inline_label(ui, &text);
                                            }
                                            crate::plugins::viewers::PreviewBlock::Code(text) => {
                                                ui.code(text);
                                            }
                                            crate::plugins::viewers::PreviewBlock::ListItem(text) => { ui.label(format!("• {text}")); }
                                            crate::plugins::viewers::PreviewBlock::Quote(text) => { ui.label(RichText::new(text).italics().color(color(theme::TEXT_MUTED))); }
                                            crate::plugins::viewers::PreviewBlock::Rule => { ui.separator(); }
                                        }
                                            }
                                        } else {
                                            ui.label(
                                                RichText::new(&message.text)
                                                    .color(color(theme::HEADER_TEXT)),
                                            );
                                        }
                                    });
                                ui.add_space(12.0);
                                let activities: Vec<_> = self
                                    .controller
                                    .state
                                    .chat_activity
                                    .iter()
                                    .filter(|a| a.after_message == index)
                                    .cloned().collect();
                                for activity in &activities {
                                    match &activity.kind {
                                        ChatActivityKind::ToolCall { name, args } => {
                                            ui.collapsing(format!("⚙ Tool: {name}"), |ui| {
                                                ui.code(args);
                                            });
                                        }
                                        ChatActivityKind::ToolResult { name, result } => {
                                            ui.collapsing(format!("✓ Result: {name}"), |ui| {
                                                ui.code(result);
                                            });
                                        }
                                        ChatActivityKind::Reasoning(text)
                                            if self.controller.state.show_reasoning =>
                                        {
                                            ui.collapsing("Reasoning", |ui| {
                                                ui.label(text);
                                            });
                                        }
                                        ChatActivityKind::Approval { id, name, args, decision } => {
                                            ui.collapsing(format!("Approval requested: {name}"), |ui| {
                                                ui.code(args);
                                                if let Some(approved) = decision {
                                                    ui.label(if *approved { "Approved" } else { "Denied" });
                                                } else {
                                                    if ui.button("Approve").clicked() { self.controller.dispatch(UiEvent::DecideTool(*id, true)); }
                                                    if ui.button("Deny").clicked() { self.controller.dispatch(UiEvent::DecideTool(*id, false)); }
                                                }
                                            });
                                        }
                                        _ => {}
                                    }
                                }
                            }
                        });
                });
            }
            CenterView::Web => {
                ui.heading("Web panel");
                ui.horizontal(|ui| {
                    if ui
                        .add_enabled(
                            self.controller
                                .state
                                .web_history_index
                                .is_some_and(|i| i > 0),
                            egui::Button::new("←"),
                        )
                        .clicked()
                    {
                        self.controller.dispatch(UiEvent::WebBack);
                    }
                    if ui
                        .add_enabled(
                            self.controller
                                .state
                                .web_history_index
                                .is_some_and(|i| i + 1 < self.controller.state.web_history.len()),
                            egui::Button::new("→"),
                        )
                        .clicked()
                    {
                        self.controller.dispatch(UiEvent::WebForward);
                    }
                    let mut url = self.controller.state.web_url.clone();
                    if ui.text_edit_singleline(&mut url).changed() {
                        self.controller.dispatch(UiEvent::SetWebUrl(url));
                    }
                    if ui.button("Open").clicked() {
                        self.controller.dispatch(UiEvent::OpenWeb);
                    }
                });
                ui.separator();
                if self.controller.state.web_loading {
                    ui.spinner();
                }
                if let Some(page) = self.controller.state.web_page.clone() {
                    ui.heading(&page.title);
                    ui.small(&page.url);
                    egui::ScrollArea::vertical()
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            ui.label(&page.text);
                            if !page.links.is_empty() {
                                ui.separator();
                                ui.heading("Links");
                            }
                            for (label, url) in page.links {
                                if ui.link(label).on_hover_text(&url).clicked() {
                                    self.controller.dispatch(UiEvent::SetWebUrl(url));
                                    self.controller.dispatch(UiEvent::OpenWeb);
                                }
                            }
                        });
                } else if let Some(doc) = &self.controller.state.editor {
                    ui.label(RichText::new(format!("Local preview: {}", doc.path)).strong());
                    egui::ScrollArea::vertical().show(ui, |ui| {
                        let blocks =
                            crate::plugins::viewers::document_preview(&doc.path, &doc.text);
                        if let Some(blocks) = blocks {
                            for block in blocks {
                                match block {
                                    crate::plugins::viewers::PreviewBlock::Heading(level, text) => {
                                        ui.label(
                                            RichText::new(text)
                                                .size((29.0 - level as f32 * 3.0).max(14.0))
                                                .strong(),
                                        );
                                    }
                                    crate::plugins::viewers::PreviewBlock::Paragraph(text) => {
                                        inline_label(ui, &text);
                                    }
                                    crate::plugins::viewers::PreviewBlock::Code(text) => {
                                        ui.code(text);
                                    }
                                    crate::plugins::viewers::PreviewBlock::ListItem(text) => {
                                        ui.label(format!("• {text}"));
                                    }
                                    crate::plugins::viewers::PreviewBlock::Quote(text) => {
                                        ui.label(RichText::new(text).italics());
                                    }
                                    crate::plugins::viewers::PreviewBlock::Rule => {
                                        ui.separator();
                                    }
                                }
                                ui.add_space(5.0);
                            }
                        } else {
                            ui.label(RichText::new(&doc.text).monospace());
                        }
                    });
                } else {
                    ui.label("Open a local text file to preview it here.");
                }
            }
            CenterView::Changes => {
                ui.heading("File changes");
                if let Some(doc) = &self.controller.state.editor {
                    ui.label(&doc.path);
                    egui::ScrollArea::vertical().show(ui, |ui| {
                        for (kind, line) in doc.changes() {
                            let color = match kind {
                                '+' => Color32::LIGHT_GREEN,
                                '-' => Color32::LIGHT_RED,
                                _ => Color32::GRAY,
                            };
                            ui.colored_label(
                                color,
                                RichText::new(format!("{kind} {line}")).monospace(),
                            );
                        }
                    });
                } else {
                    ui.label("Open a file to inspect changes.");
                }
            }
            CenterView::Admin => {
                ui.heading(self.controller.state.admin_section.label());
                ui.label("Desktop overrides are saved locally. The ForgeIQ config files are not rewritten.");
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
                    if !is_default
                        && ui
                            .button(format!("Make {agent_id} the default agent"))
                            .clicked()
                    {
                        self.controller.dispatch(UiEvent::SetDefaultAgent(agent_id));
                    }
                }
                let resources = self.controller.admin_resources();
                if !resources.is_empty() {
                    if matches!(section, AdminSection::Agents | AdminSection::Models) {
                        egui::ScrollArea::vertical().show(ui, |ui| {
                            for (id, name, mut enabled) in resources {
                                let (icon, detail) = if section == AdminSection::Agents {
                                    let visual = self
                                        .controller
                                        .config
                                        .agents
                                        .agents
                                        .iter()
                                        .find(|agent| agent.id == id)
                                        .map(|agent| {
                                            AgentVisual::from_agent(
                                                agent,
                                                &self.controller.config_path,
                                            )
                                        });
                                    let detail = visual
                                        .as_ref()
                                        .map(|visual| visual.category.clone())
                                        .unwrap_or_default();
                                    (visual.and_then(|visual| visual.avatar_bytes()), detail)
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
                                    .and_then(|path| {
                                        crate::ui::common::agent_visual::cached_image_bytes(&path)
                                    })
                                    .or_else(|| {
                                        crate::ui::common::agent_visual::cached_svg_png(
                                            include_bytes!("../common/icons/file.svg"),
                                            32,
                                        )
                                    });
                                    (icon, provider)
                                };
                                let row_width = ui.available_width();
                                egui::Frame::new()
                                    .fill(color(theme::SURFACE_MUTED))
                                    .corner_radius(6)
                                    .inner_margin(egui::Margin::symmetric(10, 7))
                                    .show(ui, |ui| {
                                        ui.set_min_width((row_width - 20.0).max(0.0));
                                        ui.horizontal(|ui| {
                                            if let Some(bytes) = icon {
                                                ui.add(
                                                    egui::Image::from_bytes(
                                                        format!("admin-resource:{id}.png"),
                                                        bytes,
                                                    )
                                                    .fit_to_exact_size(egui::vec2(24.0, 24.0)),
                                                );
                                            }
                                            if ui.button(&name).clicked() {
                                                if section == AdminSection::Agents {
                                                    self.controller
                                                        .dispatch(UiEvent::SelectAgent(id.clone()));
                                                    self.controller
                                                        .dispatch(UiEvent::OpenAgentSettings);
                                                    self.controller.dispatch(
                                                        UiEvent::SelectCenter(CenterView::Chat),
                                                    );
                                                } else {
                                                    self.controller
                                                        .dispatch(UiEvent::SelectModel(id.clone()));
                                                }
                                            }
                                            if !detail.is_empty() {
                                                ui.label(
                                                    RichText::new(format!("· {detail}"))
                                                        .small()
                                                        .color(color(theme::TEXT_MUTED)),
                                                );
                                            }
                                            ui.with_layout(
                                                egui::Layout::right_to_left(egui::Align::Center),
                                                |ui| {
                                                    ui.horizontal(|ui| {
                                                        ui.spacing_mut().item_spacing.x = 2.0;
                                                        ui.label("Enabled");
                                                        if ui.checkbox(&mut enabled, "").changed() {
                                                            self.controller.dispatch(
                                                                UiEvent::SetResourceEnabled(
                                                                    section, id, enabled,
                                                                ),
                                                            );
                                                        }
                                                        ui.add(
                                                            egui::Image::new(egui::include_image!(
                                                                "../common/icons/info.svg"
                                                            ))
                                                            .fit_to_exact_size(egui::vec2(
                                                                14.0, 14.0,
                                                            )),
                                                        )
                                                        .on_hover_text(section.enabled_help());
                                                    });
                                                },
                                            );
                                        });
                                    });
                                ui.add_space(4.0);
                            }
                        });
                    } else {
                        ui.collapsing("Configured resources", |ui| {
                            egui::ScrollArea::vertical()
                                .max_height(180.0)
                                .show(ui, |ui| {
                                    for (id, name, mut enabled) in resources {
                                        ui.horizontal(|ui| {
                                            if ui.checkbox(&mut enabled, &name).changed() {
                                                self.controller.dispatch(
                                                    UiEvent::SetResourceEnabled(
                                                        section, id, enabled,
                                                    ),
                                                );
                                            }
                                            ui.add(
                                                egui::Image::new(egui::include_image!(
                                                    "../common/icons/info.svg"
                                                ))
                                                .fit_to_exact_size(egui::vec2(14.0, 14.0)),
                                            )
                                            .on_hover_text(section.enabled_help());
                                        });
                                    }
                                });
                        });
                    }
                }
                if self.controller.state.admin_section == AdminSection::Credentials {
                    ui.horizontal(|ui| {
                        ui.label("Saved credentials");
                        ui.add(egui::Image::new(egui::include_image!("../common/icons/info.svg"))
                            .fit_to_exact_size(egui::vec2(14.0, 14.0)))
                            .on_hover_text("Credential values are stored in the OS vault. The list shows metadata only.");
                    });
                    for credential in self.controller.credential_summaries() {
                        let row_width = ui.available_width();
                        egui::Frame::new()
                            .fill(color(theme::SURFACE_MUTED))
                            .corner_radius(6)
                            .inner_margin(egui::Margin::symmetric(10, 7))
                            .show(ui, |ui| {
                                ui.set_min_width((row_width - 20.0).max(0.0));
                                ui.horizontal(|ui| {
                                    ui.add(
                                        egui::Image::new(egui::include_image!(
                                            "../common/icons/key.svg"
                                        ))
                                        .fit_to_exact_size(egui::vec2(24.0, 24.0)),
                                    );
                                    ui.vertical(|ui| {
                                        ui.strong(&credential.name);
                                        ui.label(
                                            RichText::new(format!(
                                                "{} · {}",
                                                credential.provider, credential.kind
                                            ))
                                            .small()
                                            .color(color(theme::TEXT_MUTED)),
                                        );
                                    });
                                });
                            });
                        ui.add_space(4.0);
                    }
                    ui.collapsing("Credential metadata", |ui| {
                        ui.code(&self.controller.state.admin_draft);
                    });
                    ui.label("New credential YAML: id, name, provider, type, values");
                    let mut draft = self.controller.state.credential_draft.clone();
                    if ui
                        .add(
                            egui::TextEdit::multiline(&mut draft)
                                .desired_rows(8)
                                .desired_width(ui.available_width()),
                        )
                        .changed()
                    {
                        self.controller.dispatch(UiEvent::SetCredentialDraft(draft));
                    }
                    if ui.button("Save credential").clicked() {
                        self.controller.dispatch(UiEvent::SaveCredential);
                    }
                    ui.horizontal(|ui| {
                        ui.label("ID to delete");
                        let mut id = self.controller.state.admin_target_id.clone();
                        if ui.text_edit_singleline(&mut id).changed() {
                            self.controller.dispatch(UiEvent::SetAdminTarget(id));
                        }
                        if ui.button("Delete credential").clicked() {
                            self.controller.dispatch(UiEvent::DeleteCredential(
                                self.controller.state.admin_target_id.clone(),
                            ));
                        }
                    });
                    return;
                }
                if matches!(
                    self.controller.state.admin_section,
                    AdminSection::Workflows
                        | AdminSection::Evaluations
                        | AdminSection::Connectors
                        | AdminSection::Knowledge
                        | AdminSection::Schedules
                ) {
                    ui.horizontal(|ui| {
                        ui.label(match self.controller.state.admin_section {
                            AdminSection::Workflows => "Workflow ID",
                            AdminSection::Evaluations => "Evalset ID",
                            AdminSection::Knowledge => "Knowledge search",
                            AdminSection::Schedules => "Schedule ID",
                            _ => "Connector ID",
                        });
                        let mut id = self.controller.state.admin_target_id.clone();
                        if ui.text_edit_singleline(&mut id).changed() {
                            self.controller.dispatch(UiEvent::SetAdminTarget(id));
                        }
                    });
                    if matches!(
                        self.controller.state.admin_section,
                        AdminSection::Workflows
                            | AdminSection::Connectors
                            | AdminSection::Knowledge
                    ) {
                        ui.horizontal(|ui| {
                            ui.label(match self.controller.state.admin_section {
                                AdminSection::Connectors => "Path or read <file> (tools for MCP)",
                                AdminSection::Knowledge => "Search terms",
                                _ => "Input JSON",
                            });
                            let mut input = self.controller.state.admin_run_input.clone();
                            if ui.text_edit_singleline(&mut input).changed() {
                                self.controller.dispatch(UiEvent::SetAdminRunInput(input));
                            }
                        });
                    }
                    ui.horizontal(|ui| {
                        if ui
                            .add_enabled(
                                !self.controller.state.admin_busy,
                                egui::Button::new(match self.controller.state.admin_section {
                                    AdminSection::Workflows => "Run workflow",
                                    AdminSection::Evaluations => "Run evaluation",
                                    AdminSection::Knowledge => "Search knowledge",
                                    AdminSection::Schedules => "Start schedule",
                                    _ => "Browse connector",
                                }),
                            )
                            .clicked()
                        {
                            self.controller.dispatch(UiEvent::RunAdmin);
                        }
                        if self.controller.state.admin_busy && ui.button("Cancel").clicked() {
                            self.controller.dispatch(UiEvent::CancelAdmin);
                        }
                    });
                    if !self.controller.state.admin_output.is_empty() {
                        egui::ScrollArea::vertical()
                            .max_height(120.0)
                            .show(ui, |ui| {
                                ui.code(&self.controller.state.admin_output);
                            });
                    }
                }
                if !matches!(
                    self.controller.state.admin_section,
                    AdminSection::Credentials | AdminSection::Monitoring
                ) {
                    if ui.button("Save changes").clicked() {
                        self.controller.dispatch(UiEvent::SaveAdmin);
                    }
                }
                let mut draft = self.controller.state.admin_draft.clone();
                if ui
                    .add(
                        egui::TextEdit::multiline(&mut draft)
                            .font(egui::TextStyle::Monospace)
                            .desired_width(f32::INFINITY)
                            .desired_rows(22)
                            .interactive(!matches!(
                                self.controller.state.admin_section,
                                AdminSection::Credentials | AdminSection::Monitoring
                            )),
                    )
                    .changed()
                {
                    self.controller.dispatch(UiEvent::SetAdminDraft(draft));
                }
            }
        }
    }

    fn agent_settings_body(&mut self, ui: &mut egui::Ui) {
        let Some(settings) = self.controller.state.agent_settings.clone() else {
            return;
        };
        ui.horizontal(|ui| {
            ui.heading(format!(
                "{} · {}",
                settings.draft.name,
                settings.section.label()
            ));
            if settings.dirty {
                ui.colored_label(color(theme::PRIMARY), "Unsaved");
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .add(egui::Button::image(
                        egui::Image::new(egui::include_image!("../common/icons/settings.svg"))
                            .fit_to_exact_size(egui::vec2(17.0, 17.0)),
                    ))
                    .on_hover_text("Back to chat")
                    .clicked()
                {
                    self.controller.dispatch(UiEvent::CloseAgentSettings);
                }
                if ui.button("Save").clicked() {
                    self.controller.dispatch(UiEvent::SaveAgentSettings);
                }
                if settings.dirty && ui.button("Discard").clicked() {
                    self.controller.dispatch(UiEvent::DiscardAgentSettings);
                }
            });
        });
        ui.separator();
        let content_width = (ui.available_width() - 170.0).max(220.0);
        let content_height = ui.available_height().max(200.0);
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.set_min_width(152.0);
                ui.set_max_width(152.0);
                egui::ScrollArea::vertical().max_height(content_height).show(ui, |ui| {
                for section in AgentSection::ALL {
                    ui.horizontal(|ui| {
                        let bytes = crate::ui::common::agent_visual::cached_svg_png(section.icon(), 32)
                            .expect("built-in settings icon");
                        ui.add(egui::Image::from_bytes(format!("settings-section:{}", section.label()), bytes)
                            .fit_to_exact_size(egui::vec2(16.0, 16.0)));
                        if ui.add_sized([112.0, 27.0], egui::Button::new(section.label())
                            .selected(settings.section == section)).clicked() {
                            self.controller.dispatch(UiEvent::SelectAgentSection(section));
                        }
                    });
                }
                });
            });
            ui.separator();
            ui.allocate_ui_with_layout(egui::vec2(content_width, content_height), egui::Layout::top_down(egui::Align::Min), |ui| {
            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                ui.set_min_width(content_width - 14.0);
                match settings.section {
                    AgentSection::Agent => {
                        let visual = AgentVisual::from_agent(&settings.draft, &self.controller.config_path);
                        egui::Frame::new().fill(color(theme::SURFACE_MUTED)).corner_radius(8)
                            .inner_margin(egui::Margin::symmetric(12, 10)).show(ui, |ui| {
                                ui.horizontal(|ui| {
                                    if let Some(bytes) = visual.avatar_bytes() {
                                        ui.add(egui::Image::from_bytes(format!("settings-avatar:{}.png", settings.agent_id), bytes)
                                            .fit_to_exact_size(egui::vec2(46.0, 46.0)));
                                    }
                                    ui.vertical(|ui| {
                                        ui.strong(&settings.draft.name);
                                        ui.small(&visual.category);
                                        ui.small(format!("ID: {}", settings.agent_id));
                                    });
                                });
                            });
                        ui.label("Name");
                        let mut name = settings.draft.name.clone();
                        if ui.text_edit_singleline(&mut name).changed() { self.controller.dispatch(UiEvent::SetAgentName(name)); }
                        ui.label("Category");
                        let mut category = settings.draft.other.get("category").and_then(|v| v.as_str()).unwrap_or("").to_owned();
                        if ui.text_edit_singleline(&mut category).changed() { self.controller.dispatch(UiEvent::SetAgentCategory(category)); }
                        ui.horizontal(|ui| {
                            let mut enabled = settings.draft.enabled != Some(false);
                            if ui.checkbox(&mut enabled, "Enabled").changed() { self.controller.dispatch(UiEvent::SetAgentEnabled(enabled)); }
                            let mut default = settings.draft.default == Some(true);
                            if ui.checkbox(&mut default, "Default agent").changed() { self.controller.dispatch(UiEvent::SetAgentDefault(default)); }
                        });
                        ui.label("Description");
                        let mut description = settings.draft.description.clone().unwrap_or_default();
                        if ui.add(egui::TextEdit::multiline(&mut description).desired_rows(3)).changed() {
                            self.controller.dispatch(UiEvent::SetAgentDescription(description));
                        }
                        ui.small(format!("Agent ID: {}", settings.agent_id));
                    }
                    AgentSection::Tools | AgentSection::Skills => {
                        let tools = settings.section == AgentSection::Tools;
                        let entries = if tools { agent_settings::tool_catalog(&self.controller.config, &self.controller.config_path, &settings) }
                            else { agent_settings::skill_catalog(&self.controller.config, &self.controller.config_path, &settings) };
                        let categories: Vec<String> = std::iter::once("All".to_owned())
                            .chain(entries.iter().map(|entry| entry.category.clone())).collect::<std::collections::BTreeSet<_>>()
                            .into_iter().collect();
                        ui.horizontal(|ui| {
                            egui::ComboBox::from_id_salt("agent-settings-category")
                                .selected_text(if settings.category == "All" && !tools { "All categories" } else { &settings.category })
                                .show_ui(ui, |ui| {
                                    for category in &categories {
                                        if ui.selectable_label(settings.category == *category, category).clicked() {
                                            self.controller.dispatch(UiEvent::SetAgentSettingsCategory(category.clone()));
                                        }
                                    }
                                });
                            let mut query = settings.query.clone();
                            if ui.add(egui::TextEdit::singleline(&mut query)
                                .hint_text(if tools { "Search tools" } else { "Search skills" })).changed() {
                                self.controller.dispatch(UiEvent::SetAgentSettingsQuery(query));
                            }
                            if !tools && ui.selectable_label(settings.enabled_only, "Enabled").clicked() {
                                self.controller.dispatch(UiEvent::SetAgentSettingsEnabledOnly(!settings.enabled_only));
                            }
                        });
                        ui.separator();
                        let query = settings.query.to_lowercase();
                        for entry in entries {
                            if settings.category != "All" && settings.category != entry.category { continue; }
                            if !query.is_empty() && !format!("{} {} {}", entry.name, entry.category, entry.description)
                                .to_lowercase().contains(&query) { continue; }
                            if !tools && settings.enabled_only && !settings.skill_enabled(&entry.id) { continue; }
                            egui::Frame::new().fill(color(theme::SURFACE_MUTED)).corner_radius(6)
                                .inner_margin(egui::Margin::symmetric(10, 7)).show(ui, |ui| {
                                    ui.horizontal(|ui| {
                                        let icon = crate::ui::common::agent_visual::cached_svg_png(
                                            settings.section.icon(), 32
                                        ).expect("built-in settings icon");
                                        ui.add(egui::Image::from_bytes(
                                            format!("settings-resource:{}", entry.id), icon
                                        ).fit_to_exact_size(egui::vec2(18.0, 18.0)));
                                        ui.vertical(|ui| {
                                            ui.label(RichText::new(&entry.name).strong());
                                            ui.small(format!("{} · {}", entry.category, entry.source));
                                            if !entry.description.is_empty() { ui.small(&entry.description); }
                                        });
                                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                            if tools {
                                                let mut enabled = settings.tool_value(&entry.id, "enabled", true);
                                                if ui.checkbox(&mut enabled, "Enabled").changed() {
                                                    self.controller.dispatch(UiEvent::SetAgentTool(entry.id.clone(), "enabled".into(), enabled));
                                                }
                                                let mut approve = settings.tool_value(&entry.id, "autoapprove", false);
                                                if ui.checkbox(&mut approve, "Autoapprove").changed() {
                                                    self.controller.dispatch(UiEvent::SetAgentTool(entry.id.clone(), "autoapprove".into(), approve));
                                                }
                                            } else {
                                                let mut enabled = settings.skill_enabled(&entry.id);
                                                if ui.checkbox(&mut enabled, "Enabled").changed() {
                                                    self.controller.dispatch(UiEvent::SetAgentSkill(entry.id.clone(), enabled));
                                                }
                                            }
                                        });
                                    });
                                });
                            ui.add_space(3.0);
                        }
                    }
                    AgentSection::Ui => {
                        let mut enabled = settings.field("ui", "enabled").and_then(|v| v.as_bool()).unwrap_or(false);
                        if ui.checkbox(&mut enabled, "UI Enabled").changed() {
                            self.controller.dispatch(UiEvent::SetAgentField("ui".into(), "enabled".into(), enabled.into()));
                        }
                        let renderer = settings.field("ui", "renderer").and_then(|v| v.as_str()).unwrap_or("auto");
                        egui::ComboBox::from_id_salt("agent-ui-renderer").selected_text(format!("Renderer: {renderer}"))
                            .show_ui(ui, |ui| {
                                if ui.selectable_label(renderer == "auto", "Auto").clicked() {
                                    self.controller.dispatch(UiEvent::SetAgentField("ui".into(), "renderer".into(), "auto".into()));
                                }
                            });
                        for (key, label) in [("workflow_instructions", "Workflow Instructions"), ("ui_description", "UI Description")] {
                            ui.label(label);
                            let mut text = settings.field("ui", key).and_then(|v| v.as_str()).unwrap_or("").to_owned();
                            if ui.add(egui::TextEdit::multiline(&mut text).desired_rows(5).desired_width(ui.available_width())).changed() {
                                self.controller.dispatch(UiEvent::SetAgentField("ui".into(), key.into(), text.into()));
                            }
                        }
                        ui.collapsing("Advanced options", |ui| {
                            for (key, label) in [("schema", "Include schema"), ("examples", "Include examples")] {
                                let mut value = settings.field("ui", key).and_then(|v| v.as_bool()).unwrap_or(true);
                                if ui.checkbox(&mut value, label).changed() {
                                    self.controller.dispatch(UiEvent::SetAgentField("ui".into(), key.into(), value.into()));
                                }
                            }
                        });
                    }
                    AgentSection::Memory => {
                        let backends: Vec<_> = self.controller.config.memory.memory.iter()
                            .map(|memory| (memory.id.clone().unwrap_or_else(|| memory.name.clone()), memory.name.clone())).collect();
                        ui.columns(2, |panels| {
                            for (index, scope) in ["personal", "shared"].iter().enumerate() {
                                let ui = &mut panels[index];
                                ui.heading(format!("{} Memory", if index == 0 { "Personal" } else { "Shared" }));
                                let key = if index == 0 { "type" } else { "shared_type" };
                                let current = settings.field("memory", key).and_then(|v| v.as_str())
                                    .unwrap_or(if index == 0 { "adk-sqlite" } else { "" });
                                egui::ComboBox::from_id_salt(key).selected_text(if current.is_empty() { "Same backend" } else { current })
                                    .show_ui(ui, |ui| {
                                        for (id, label) in std::iter::once(("disabled".to_owned(), "Disabled".to_owned()))
                                            .chain(backends.iter().cloned()).chain(if index == 1 { Some((String::new(), "Same backend".into())) } else { None }) {
                                            if ui.selectable_label(current == id, label).clicked() {
                                                self.controller.dispatch(UiEvent::SetAgentField("memory".into(), key.into(), id.into()));
                                            }
                                        }
                                    });
                                for (field, label, default) in [("auto_retrieval", "Auto Recall", true),
                                    ("auto_store", "Auto Remember", index == 0)] {
                                    let mut value = settings.memory_value(scope, field, default);
                                    if ui.checkbox(&mut value, label).changed() {
                                        self.controller.dispatch(UiEvent::SetAgentMemory((*scope).into(), field.into(), value));
                                    }
                                }
                            }
                        });
                        ui.separator(); ui.heading("Memory Files");
                        ui.small("Memory files are managed through the configured workspace file store.");
                    }
                    _ => {
                        ui.label(format!("{} configuration (YAML)", settings.section.label()));
                        let mut text = settings.raw_text.clone();
                        if ui.add(egui::TextEdit::multiline(&mut text).code_editor().desired_rows(20)
                            .desired_width(ui.available_width())).changed() {
                            self.controller.dispatch(UiEvent::SetAgentRaw(text));
                        }
                        if ui.button("Apply section").clicked() { self.controller.dispatch(UiEvent::ApplyAgentRaw); }
                    }
                }
            });
            });
        });
    }

    fn editor_panel(&mut self, ui: &mut egui::Ui) {
        ui.horizontal_wrapped(|ui| {
            let docs: Vec<_> = self
                .controller
                .state
                .documents
                .iter()
                .map(|d| (d.path.clone(), d.dirty()))
                .collect();
            for (path, dirty) in docs {
                let selected = self
                    .controller
                    .state
                    .editor
                    .as_ref()
                    .is_some_and(|d| d.path == path);
                let name = std::path::Path::new(&path)
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy();
                let (_, dropped) = ui.dnd_drop_zone::<String, _>(egui::Frame::new(), |ui| {
                    ui.dnd_drag_source(
                        ui.scope_id().with(("document-tab", &path)),
                        path.clone(),
                        |ui| {
                            if ui
                                .selectable_label(
                                    selected,
                                    format!("{}{}", name, if dirty { " ●" } else { "" }),
                                )
                                .clicked()
                            {
                                self.controller
                                    .dispatch(UiEvent::SelectEditor(path.clone()));
                            }
                        },
                    );
                });
                if let Some(source) = dropped {
                    self.controller
                        .dispatch(UiEvent::MoveDocument((*source).clone(), path));
                }
            }
        });
        ui.separator();
        let Some(doc) = self.controller.state.editor.clone() else {
            ui.heading("Editor / Viewer");
            ui.label("Select a file from the tree.");
            return;
        };
        ui.horizontal(|ui| {
            ui.heading(&doc.path);
            if doc.dirty() {
                ui.label("●");
            }
            if ui.button("×").clicked() {
                self.controller.dispatch(UiEvent::CloseEditor);
            }
            if ui.button("↗ Float").clicked() {
                self.controller
                    .dispatch(UiEvent::ToggleDocumentFloating(doc.path.clone()));
            }
        });
        if doc.dirty() && ui.button("Discard changes and close").clicked() {
            self.controller.dispatch(UiEvent::DiscardAndCloseEditor);
        }
        ui.horizontal(|ui| {
            ui.small(doc.plugin);
            if ui
                .add_enabled(doc.editable && doc.dirty(), egui::Button::new("Save"))
                .clicked()
            {
                self.controller.dispatch(UiEvent::SaveFile);
            }
            if doc.dirty() && ui.button("Revert").clicked() {
                self.controller.dispatch(UiEvent::RevertFile);
            }
            if (doc.path.ends_with(".json") || doc.path.ends_with(".ipynb"))
                && ui.button("Format JSON").clicked()
            {
                self.controller.dispatch(UiEvent::FormatDocument);
            }
        });
        if crate::plugins::viewers::document_preview(&doc.path, &doc.text).is_some() {
            ui.horizontal(|ui| {
                for (mode, label) in [
                    (DocumentView::Source, "Source"),
                    (DocumentView::Preview, "Preview"),
                    (DocumentView::Split, "Split"),
                ] {
                    if ui
                        .selectable_label(self.controller.state.document_view == mode, label)
                        .clicked()
                    {
                        self.controller.dispatch(UiEvent::SetDocumentView(mode));
                    }
                }
            });
        }
        ui.separator();
        match self.controller.state.document_view {
            DocumentView::Source => self.editor_source(ui, &doc),
            DocumentView::Preview => self.editor_preview(ui, &doc),
            DocumentView::Split => {
                ui.columns(2, |columns| {
                    self.editor_source(&mut columns[0], &doc);
                    self.editor_preview(&mut columns[1], &doc);
                });
            }
        }
    }

    fn editor_preview(&self, ui: &mut egui::Ui, doc: &crate::ui::common::EditorDocument) {
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                if let Some(blocks) =
                    crate::plugins::viewers::document_preview(&doc.path, &doc.text)
                {
                    for block in blocks {
                        match block {
                            crate::plugins::viewers::PreviewBlock::Heading(level, text) => {
                                ui.label(
                                    RichText::new(text)
                                        .size((26.0 - level as f32 * 2.0).max(14.0))
                                        .strong(),
                                );
                            }
                            crate::plugins::viewers::PreviewBlock::Paragraph(text) => {
                                inline_label(ui, &text);
                            }
                            crate::plugins::viewers::PreviewBlock::Code(text) => {
                                ui.code(text);
                            }
                            crate::plugins::viewers::PreviewBlock::ListItem(text) => {
                                ui.label(format!("• {text}"));
                            }
                            crate::plugins::viewers::PreviewBlock::Quote(text) => {
                                ui.label(RichText::new(text).italics());
                            }
                            crate::plugins::viewers::PreviewBlock::Rule => {
                                ui.separator();
                            }
                        }
                    }
                } else {
                    ui.code(&doc.text);
                }
            });
    }

    fn editor_source(&mut self, ui: &mut egui::Ui, doc: &crate::ui::common::EditorDocument) {
        let mut text = doc.text.clone();
        let language = std::path::Path::new(&doc.path)
            .extension()
            .and_then(|v| v.to_str())
            .unwrap_or("txt")
            .to_owned();
        let theme = egui_extras::syntax_highlighting::CodeTheme::from_memory(ui.ctx(), ui.style());
        let mut layouter = |ui: &egui::Ui, buf: &dyn egui::TextBuffer, wrap_width: f32| {
            let mut job = egui_extras::syntax_highlighting::highlight(
                ui.ctx(),
                ui.style(),
                &theme,
                buf.as_str(),
                &language,
            );
            job.wrap.max_width = wrap_width;
            ui.fonts_mut(|fonts| fonts.layout_job(job))
        };
        let edit = egui::TextEdit::multiline(&mut text)
            .font(egui::TextStyle::Monospace)
            .code_editor()
            .layouter(&mut layouter)
            .desired_width(f32::INFINITY)
            .desired_rows(36)
            .interactive(doc.editable);
        if ui.add(edit).changed() {
            self.controller.dispatch(UiEvent::SetEditorText(text));
        }
    }
}
