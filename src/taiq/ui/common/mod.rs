pub mod agent_settings;
pub mod agent_visual;
mod chat;
mod desktop_terminal;
pub mod inline;
pub mod layout;
mod management;
pub mod model_picker;
mod preferences;
mod sessions;
pub mod syntax;
pub mod theme;
pub mod voice;
pub mod web;
pub use management::AdminSection;
pub use sessions::ChatSession;

use crate::admin::evaluation::{EvalSet, load_evalsets, run_evalset};
use crate::admin::jobs::{JobRunner, JobUpdate};
use crate::admin::knowledge::{Knowledge, KnowledgeStore};
use crate::admin::schedule::{Schedule, run_once as run_schedule_once};
use crate::admin::workflow::{Workflow, run_manual};
use crate::connectors::remote::RemoteConnector;
use crate::connectors::{LocalStoreDefinition, local_store_roots};
use crate::credentials::{CredentialDraft, CredentialMeta, CredentialStore};
use crate::filestore::{FileEntry, FileStore, LocalFileStore, MemoryFileStore, RemoteFileStore};
use crate::plugins::{self, PluginKind};
use crate::schema::ForgeConfig;
use agent_settings::{AgentSection, AgentSettingsState};
use anyhow::Result;
use chat::{ChatBridge, ChatUpdate};
use desktop_terminal::{TerminalSession, plain_output};
use preferences::{LayoutPreferences, PreferencesStore};
use serde::{Deserialize, Serialize};
use sessions::SessionStore;
use std::collections::VecDeque;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LeftView {
    Files,
    Agents,
    Sessions,
    Connectors,
    Settings,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CenterView {
    Chat,
    Web,
    Changes,
    Admin,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DocumentView {
    Source,
    Preview,
    Split,
}

impl LeftView {
    pub const ALL: [Self; 5] = [
        Self::Files,
        Self::Agents,
        Self::Sessions,
        Self::Connectors,
        Self::Settings,
    ];
    pub fn icon(self) -> &'static str {
        match self {
            Self::Files => "▤",
            Self::Agents => "◇",
            Self::Sessions => "◷",
            Self::Connectors => "⌘",
            Self::Settings => "⚙",
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::Files => "Files",
            Self::Agents => "Agents",
            Self::Sessions => "History",
            Self::Connectors => "Connectors",
            Self::Settings => "Settings",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TreeNode {
    pub entry: FileEntry,
    pub depth: usize,
    pub expanded: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogicalRoot {
    pub id: String,
    pub label: String,
    pub store_id: String,
    pub folder: String,
    pub read_only: bool,
}

pub fn agent_prompts(config: &ForgeConfig, agent_id: &str) -> Vec<(String, String)> {
    let Some(agent) = config
        .agents
        .agents
        .iter()
        .find(|agent| agent.id == agent_id)
    else {
        return Vec::new();
    };
    agent
        .prompts
        .iter()
        .filter_map(|choice| {
            let entry = choice
                .get("ref")
                .and_then(serde_json::Value::as_str)
                .and_then(|id| {
                    config
                        .prompts
                        .iter()
                        .chain(&config.agents.prompts)
                        .find(|prompt| {
                            prompt.get("id").and_then(serde_json::Value::as_str) == Some(id)
                        })
                })
                .unwrap_or(choice);
            let prompt = entry.get("prompt").and_then(serde_json::Value::as_str)?;
            let title = entry
                .get("title")
                .and_then(serde_json::Value::as_str)
                .or_else(|| entry.get("name").and_then(serde_json::Value::as_str))
                .unwrap_or(prompt);
            Some((title.chars().take(80).collect(), prompt.to_owned()))
        })
        .collect()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ChatRole {
    User,
    Assistant,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: ChatRole,
    pub text: String,
}

#[derive(Debug, Clone)]
pub enum ChatActivityKind {
    ToolCall {
        name: String,
        args: String,
    },
    ToolResult {
        name: String,
        result: String,
    },
    Reasoning(String),
    Approval {
        id: u64,
        name: String,
        args: String,
        decision: Option<bool>,
    },
}

#[derive(Debug, Clone)]
pub struct ChatActivity {
    pub after_message: usize,
    pub kind: ChatActivityKind,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatAttachment {
    pub name: String,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueuedChat {
    pub prompt: String,
    pub attachments: Vec<ChatAttachment>,
    pub attach_open_file: bool,
}

#[derive(Debug, Clone)]
pub struct EditorDocument {
    pub path: String,
    pub text: String,
    pub saved_text: String,
    pub plugin: &'static str,
    pub editable: bool,
}

impl EditorDocument {
    pub fn dirty(&self) -> bool {
        self.text != self.saved_text
    }

    pub fn changes(&self) -> Vec<(char, String)> {
        let old: Vec<_> = self.saved_text.lines().collect();
        let new: Vec<_> = self.text.lines().collect();
        let mut output = Vec::new();
        for index in 0..old.len().max(new.len()).min(2000) {
            match (old.get(index), new.get(index)) {
                (Some(before), Some(after)) if before == after => {
                    output.push((' ', (*after).into()))
                }
                (Some(before), Some(after)) => {
                    output.push(('-', (*before).into()));
                    output.push(('+', (*after).into()));
                }
                (Some(before), None) => output.push(('-', (*before).into())),
                (None, Some(after)) => output.push(('+', (*after).into())),
                _ => {}
            }
        }
        output
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FlashLevel {
    Info,
    Error,
}

#[derive(Debug, Clone)]
pub struct Flash {
    pub level: FlashLevel,
    pub text: String,
    pub created: Instant,
}

#[derive(Debug, Clone)]
pub struct TerminalView {
    pub open: bool,
    pub height: f32,
    pub cwd: PathBuf,
    pub shell: String,
    pub output: String,
    pub input: String,
    pub history: Vec<String>,
    pub history_position: usize,
    pub running: bool,
    pub auto_scroll: bool,
    pub settings_open: bool,
    pub buffer_lines: usize,
}

#[derive(Debug)]
pub struct UiState {
    pub left_view: LeftView,
    pub center_view: CenterView,
    pub tree: Vec<TreeNode>,
    pub expanded: HashSet<String>,
    pub editor: Option<EditorDocument>,
    pub documents: Vec<EditorDocument>,
    pub floating_documents: HashSet<String>,
    pub floating_bounds: HashMap<String, [f32; 4]>,
    pub narrow_editor: bool,
    pub narrow_left: bool,
    pub chat: Vec<ChatMessage>,
    pub chat_activity: Vec<ChatActivity>,
    pub show_reasoning: bool,
    pub chat_draft: String,
    pub attach_open_file: bool,
    pub attachments: Vec<ChatAttachment>,
    pub new_file_draft: String,
    pub agent_id: String,
    pub busy: bool,
    pub queued_chat: VecDeque<QueuedChat>,
    pub memory_enabled: bool,
    pub memory_start: usize,
    pub selected_model: String,
    pub reasoning_effort: Option<String>,
    pub flashes: Vec<Flash>,
    pub web_url: String,
    pub web_page: Option<web::WebPage>,
    pub web_loading: bool,
    pub web_history: Vec<String>,
    pub web_history_index: Option<usize>,
    pub file_search: String,
    pub search_results: Vec<TreeNode>,
    pub search_active: bool,
    pub store_id: String,
    pub active_logical_root: String,
    pub available_stores: Vec<(String, String)>,
    pub selected_path: Option<String>,
    pub sessions: Vec<ChatSession>,
    pub session_query: String,
    pub active_session_id: String,
    pub terminals: HashMap<String, TerminalView>,
    pub agent_settings: Option<AgentSettingsState>,
    pub left_width: f32,
    pub editor_width: f32,
    pub document_view: DocumentView,
    pub center_tabs: Vec<CenterView>,
    pub dark_mode: bool,
    pub admin_section: AdminSection,
    pub admin_draft: String,
    pub admin_target_id: String,
    pub admin_run_input: String,
    pub admin_output: String,
    pub admin_busy: bool,
    pub credential_draft: String,
}

impl UiState {
    fn new(config: &ForgeConfig) -> Self {
        let agent_id = config
            .agents
            .agents
            .iter()
            .find(|a| a.default == Some(true) && a.enabled != Some(false))
            .or_else(|| {
                config
                    .agents
                    .agents
                    .iter()
                    .find(|a| a.enabled != Some(false))
            })
            .map(|a| a.id.clone())
            .unwrap_or_default();
        Self {
            left_view: LeftView::Files,
            center_view: CenterView::Chat,
            tree: Vec::new(),
            expanded: HashSet::new(),
            editor: None,
            documents: Vec::new(),
            floating_documents: HashSet::new(),
            floating_bounds: HashMap::new(),
            narrow_editor: false,
            narrow_left: false,
            chat: Vec::new(),
            chat_activity: Vec::new(),
            show_reasoning: false,
            chat_draft: String::new(),
            attach_open_file: false,
            attachments: Vec::new(),
            new_file_draft: String::new(),
            agent_id,
            busy: false,
            queued_chat: VecDeque::new(),
            memory_enabled: true,
            memory_start: 0,
            selected_model: "auto".into(),
            reasoning_effort: None,
            flashes: Vec::new(),
            web_url: String::new(),
            web_page: None,
            web_loading: false,
            web_history: Vec::new(),
            web_history_index: None,
            file_search: String::new(),
            search_results: Vec::new(),
            search_active: false,
            store_id: String::new(),
            active_logical_root: "documents".into(),
            available_stores: Vec::new(),
            selected_path: None,
            sessions: Vec::new(),
            session_query: String::new(),
            active_session_id: String::new(),
            terminals: HashMap::new(),
            agent_settings: None,
            left_width: 250.0,
            editor_width: 420.0,
            document_view: DocumentView::Source,
            center_tabs: vec![
                CenterView::Chat,
                CenterView::Web,
                CenterView::Changes,
                CenterView::Admin,
            ],
            dark_mode: false,
            admin_section: AdminSection::Agents,
            admin_draft: String::new(),
            admin_target_id: String::new(),
            admin_run_input: String::new(),
            admin_output: String::new(),
            admin_busy: false,
            credential_draft: String::new(),
        }
    }
}

#[derive(Debug, Clone)]
pub enum UiEvent {
    SelectView(LeftView),
    SelectCenter(CenterView),
    ResizePanels(f32, f32),
    SetDarkMode(bool),
    SetShowReasoning(bool),
    SetDocumentView(DocumentView),
    MoveCenterTab(CenterView, CenterView),
    MoveDocument(String, String),
    ToggleDocumentFloating(String),
    SetFloatingBounds(String, [f32; 4]),
    SetNarrowEditor(bool),
    SetDocumentText(String, String),
    SaveDocument(String),
    RemoveQueuedChat(usize),
    MoveQueuedChat(usize, usize),
    ClearQueuedChat,
    RunNextQueuedChat,
    SetDefaultAgent(String),
    SetResourceEnabled(AdminSection, String, bool),
    SelectAdmin(AdminSection),
    SelectConnector(String),
    SetAdminDraft(String),
    SaveAdmin,
    SetAdminTarget(String),
    SetAdminRunInput(String),
    RunAdmin,
    CancelAdmin,
    SetCredentialDraft(String),
    SaveCredential,
    DeleteCredential(String),
    ToggleFolder(String),
    RefreshFiles,
    OpenFile(String),
    SelectEditor(String),
    SetEditorText(String),
    SaveFile,
    RevertFile,
    FormatDocument,
    CreateFile(String),
    CreateFolder(String),
    RenameFile(String, String),
    DeleteFile(String),
    ImportFile(PathBuf, String),
    ExportFile(String, PathBuf),
    SetFileSearch(String),
    SearchFiles,
    SelectPath(String),
    SelectStore(String),
    SelectLogicalRoot(String),
    AddWorkspace(PathBuf),
    SetNewFileDraft(String),
    CloseEditor,
    DiscardAndCloseEditor,
    SelectAgent(String),
    SetChatDraft(String),
    SetAttachOpenFile(bool),
    AttachFiles(Vec<PathBuf>),
    RemoveAttachment(usize),
    SendChat,
    StartVoice,
    CancelChat,
    SetMemoryEnabled(bool),
    ClearMemory,
    SelectModel(String),
    SetReasoningEffort(Option<String>),
    NewChat,
    SelectSession(String),
    SetSessionQuery(String),
    OpenAgentSettings,
    CloseAgentSettings,
    DiscardAgentSettings,
    SelectAgentSection(AgentSection),
    SetAgentSettingsQuery(String),
    SetAgentSettingsCategory(String),
    SetAgentSettingsEnabledOnly(bool),
    SetAgentTool(String, String, bool),
    SetAgentSkill(String, bool),
    SetAgentField(String, String, serde_json::Value),
    SetAgentMemory(String, String, bool),
    SetAgentRaw(String),
    ApplyAgentRaw,
    SetAgentName(String),
    SetAgentCategory(String),
    SetAgentEnabled(bool),
    SetAgentDefault(bool),
    SetAgentDescription(String),
    SaveAgentSettings,
    ToggleTerminal,
    ResizeTerminal(f32),
    SetTerminalInput(String),
    SubmitTerminal,
    InterruptTerminal,
    TerminalHistory(i32),
    ToggleTerminalAutoscroll,
    ToggleTerminalSettings,
    SetTerminalBuffer(usize),
    SetWebUrl(String),
    OpenWeb,
    WebBack,
    WebForward,
    DecideTool(u64, bool),
    Unsupported(&'static str),
}

pub struct UiController {
    pub state: UiState,
    pub config: ForgeConfig,
    pub config_path: PathBuf,
    store: Arc<dyn FileStore>,
    chat_bridge: Option<ChatBridge>,
    background_bridges: HashMap<String, ChatBridge>,
    background_activity: HashMap<String, Vec<ChatActivity>>,
    sessions: SessionStore,
    preferences: PreferencesStore,
    management: management::ManagementStore,
    local_stores: Vec<LocalStoreDefinition>,
    memory_stores: HashMap<String, Arc<dyn FileStore>>,
    remote_stores: HashMap<String, serde_json::Value>,
    jobs: Option<JobRunner>,
    credentials: CredentialStore,
    web_receiver: Option<std::sync::mpsc::Receiver<Result<web::WebPage, String>>>,
    draft_dirty_at: Option<Instant>,
    terminal_sessions: HashMap<String, TerminalSession>,
}

impl UiController {
    pub fn admin_resources(&self) -> Vec<(String, String, bool)> {
        match self.state.admin_section {
            AdminSection::Agents => self
                .config
                .agents
                .agents
                .iter()
                .map(|agent| {
                    (
                        agent.id.clone(),
                        agent.name.clone(),
                        agent.enabled != Some(false),
                    )
                })
                .collect(),
            AdminSection::Models => self
                .config
                .models
                .as_ref()
                .into_iter()
                .flat_map(|models| models.models.iter())
                .map(|model| {
                    (
                        model.id.clone().unwrap_or_else(|| model.name.clone()),
                        model.name.clone(),
                        model.other.get("enabled").and_then(|value| value.as_bool()) != Some(false),
                    )
                })
                .collect(),
            AdminSection::Connectors => self
                .config
                .connectors
                .iter()
                .filter_map(|connector| {
                    let id = connector.id.clone()?;
                    Some((
                        id.clone(),
                        connector.name.clone().unwrap_or(id),
                        connector.enabled.as_ref().and_then(|value| value.as_bool()) != Some(false)
                            && connector.enable.as_ref().and_then(|value| value.as_bool())
                                != Some(false),
                    ))
                })
                .collect(),
            AdminSection::Filestores => crate::connectors::store_catalog(&self.config)
                .into_iter()
                .map(|store| {
                    (
                        store.id,
                        format!("{} ({})", store.name, store.kind),
                        store.enabled,
                    )
                })
                .collect(),
            _ => Vec::new(),
        }
    }

    pub fn credential_summaries(&self) -> Vec<CredentialMeta> {
        self.credentials.list().to_vec()
    }

    pub fn selected_folder(&self) -> String {
        let root_folder = self
            .logical_roots()
            .into_iter()
            .find(|root| root.id == self.state.active_logical_root)
            .map(|root| root.folder)
            .unwrap_or_default();
        let Some(path) = &self.state.selected_path else {
            return root_folder;
        };
        if *path == root_folder {
            return root_folder;
        }
        if self
            .state
            .tree
            .iter()
            .any(|node| node.entry.path == *path && node.entry.is_dir)
        {
            path.clone()
        } else {
            path.rsplit_once('/')
                .map(|(parent, _)| parent.to_owned())
                .unwrap_or_default()
        }
    }

    pub fn new(config: ForgeConfig, config_path: impl AsRef<Path>) -> Result<Self> {
        let config_path = config_path.as_ref().canonicalize()?;
        let mut config = config;
        let _ = management::ManagementStore::open(&config_path, &mut config)?;
        for agent in &config.agents.agents {
            let _ = agent_visual::AgentVisual::from_agent(agent, &config_path).avatar_bytes();
        }
        if let Some(models) = &config.models {
            for provider in &models.providers {
                if let Some(path) =
                    model_picker::provider_icon_path(&config, &config_path, &provider.name)
                {
                    let _ = agent_visual::cached_image_bytes(&path);
                }
            }
        }
        let mut stores = local_store_roots(&config, &config_path)?;
        let layout = PreferencesStore::open(&config_path).load();
        for folder in &layout.workspace_folders {
            if let Ok(root) = folder.canonicalize() {
                if root.is_dir() && !stores.iter().any(|s| s.root == root) {
                    stores.push(Self::workspace_definition(root));
                }
            }
        }
        let default_id = config
            .filestores
            .as_ref()
            .and_then(|f| f.default.as_deref())
            .unwrap_or("local")
            .to_owned();
        let selected = stores
            .iter()
            .find(|s| layout.selected_store.as_deref() == Some(&s.id))
            .or_else(|| stores.iter().find(|s| s.id == default_id))
            .or_else(|| stores.first())
            .ok_or_else(|| anyhow::anyhow!("no local filestore is configured"))?;
        let store: Arc<dyn FileStore> = Arc::new(LocalFileStore::new_with_mode(
            Self::desktop_store_root(&config, selected),
            selected.read_only,
        )?);
        let mut controller = Self::with_store(config, config_path, store)?;
        controller.state.store_id = selected.id.clone();
        controller.state.active_logical_root = if selected.id == default_id {
            "documents".into()
        } else {
            format!("store:{}", selected.id)
        };
        controller.state.available_stores = stores
            .iter()
            .map(|s| (s.id.clone(), s.name.clone()))
            .collect();
        controller.local_stores = stores;
        for definition in crate::connectors::store_catalog(&controller.config)
            .into_iter()
            .filter(|item| item.enabled && item.kind == "memory")
        {
            controller
                .state
                .available_stores
                .push((definition.id.clone(), definition.name));
            controller
                .memory_stores
                .insert(definition.id, Arc::new(MemoryFileStore::new()));
        }
        if let Some(entries) = controller
            .config
            .filestores
            .as_ref()
            .and_then(|stores| stores.stores.get("filestores"))
            .and_then(|entries| entries.as_array())
        {
            for entry in entries {
                if entry.get("enabled").and_then(|v| v.as_bool()) == Some(false) {
                    continue;
                }
                let Some(kind) = entry.get("type").and_then(|v| v.as_str()) else {
                    continue;
                };
                if !matches!(
                    kind,
                    "s3" | "gcs"
                        | "azure"
                        | "azblob"
                        | "webdav"
                        | "rclone"
                        | "seaweedfs"
                        | "juicefs"
                ) {
                    continue;
                }
                let Some(id) = entry.get("id").and_then(|v| v.as_str()) else {
                    continue;
                };
                controller.state.available_stores.push((
                    id.to_owned(),
                    entry
                        .get("name")
                        .and_then(|v| v.as_str())
                        .unwrap_or(id)
                        .to_owned(),
                ));
                controller
                    .remote_stores
                    .insert(id.to_owned(), entry.clone());
            }
        }
        let desired = layout.selected_store.as_deref().unwrap_or(&default_id);
        if controller.memory_stores.contains_key(desired)
            || controller.remote_stores.contains_key(desired)
        {
            controller.apply(UiEvent::SelectStore(desired.to_owned()))?;
        }
        if let Some(root_id) = layout.active_logical_root {
            if controller
                .logical_roots()
                .iter()
                .any(|root| root.id == root_id && root.store_id == controller.state.store_id)
            {
                controller.state.active_logical_root = root_id;
                controller.refresh_tree()?;
            }
        }
        controller.save_layout()?;
        Ok(controller)
    }

    pub fn with_store(
        mut config: ForgeConfig,
        config_path: PathBuf,
        store: Arc<dyn FileStore>,
    ) -> Result<Self> {
        let management = management::ManagementStore::open(&config_path, &mut config)?;
        let credentials = CredentialStore::open(&config_path)?;
        let state = UiState::new(&config);
        let mut sessions = SessionStore::open(&config_path)?;
        let mut state = state;
        let preferences = PreferencesStore::open(&config_path);
        let layout = preferences.load();
        state.left_width = layout.left_width.clamp(170.0, 600.0);
        state.editor_width = layout.editor_width.clamp(260.0, 750.0);
        state.left_view = layout.left_view;
        state.center_view = layout.center_view;
        state.dark_mode = layout.dark_mode;
        state.show_reasoning = layout.show_reasoning;
        state.document_view = layout.document_view;
        state.floating_documents = layout.floating_documents.iter().cloned().collect();
        state.floating_bounds = layout.floating_bounds.clone();
        if layout.center_tabs.len() == 4
            && [
                CenterView::Chat,
                CenterView::Web,
                CenterView::Changes,
                CenterView::Admin,
            ]
            .iter()
            .all(|tab| layout.center_tabs.contains(tab))
        {
            state.center_tabs = layout.center_tabs;
        }
        theme::set_dark(layout.dark_mode);
        if sessions.active().is_none() {
            sessions.new_session(&state.agent_id);
            sessions.save()?;
        }
        if let Some(active) = sessions.active() {
            state.agent_id = active.agent_id.clone();
            state.selected_model = active.model_id.clone();
            state.reasoning_effort = active.reasoning_effort.clone();
            state.memory_enabled = active.memory_enabled;
            state.memory_start = active.memory_start;
            state.chat = active.messages.clone();
            state.queued_chat = active.queue.clone().into();
            state.chat_draft = active.draft.clone();
            state.attachments = active.attachments.clone();
            state.attach_open_file = active.attach_open_file;
            state.active_session_id = active.id.clone();
        }
        state.sessions = sessions.sessions().to_vec();
        state.admin_draft =
            management.content(state.admin_section, &config, state.sessions.len())?;
        let mut this = Self {
            state,
            config,
            config_path,
            store,
            chat_bridge: None,
            background_bridges: HashMap::new(),
            background_activity: HashMap::new(),
            sessions,
            preferences,
            management,
            local_stores: Vec::new(),
            memory_stores: HashMap::new(),
            remote_stores: HashMap::new(),
            jobs: None,
            credentials,
            web_receiver: None,
            draft_dirty_at: None,
            terminal_sessions: HashMap::new(),
        };
        this.refresh_tree()?;
        for path in layout.open_documents {
            let _ = this.apply(UiEvent::OpenFile(path));
        }
        if let Some(path) = layout.active_document {
            let _ = this.apply(UiEvent::SelectEditor(path));
        }
        Ok(this)
    }

    fn workspace_definition(root: PathBuf) -> LocalStoreDefinition {
        let name = root
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| root.display().to_string());
        LocalStoreDefinition {
            id: format!("workspace:{}", root.display()),
            name: format!("Workspace: {name}"),
            root,
            read_only: false,
        }
    }

    fn desktop_store_root(config: &ForgeConfig, store: &LocalStoreDefinition) -> PathBuf {
        let system = config
            .filestores
            .as_ref()
            .and_then(|stores| stores.stores.get("filestores"))
            .and_then(serde_json::Value::as_array)
            .and_then(|entries| {
                entries.iter().find(|entry| {
                    entry.get("id").and_then(serde_json::Value::as_str) == Some(store.id.as_str())
                })
            })
            .and_then(|entry| entry.get("system"))
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false);
        if !system {
            return store.root.clone();
        }
        let component = |key: &str, fallback: &str| {
            let raw = std::env::var(key).unwrap_or_else(|_| fallback.to_owned());
            let safe = raw
                .to_lowercase()
                .chars()
                .map(|ch| {
                    if ch.is_ascii_lowercase()
                        || ch.is_ascii_digit()
                        || matches!(ch, '.' | '_' | '-')
                    {
                        ch
                    } else {
                        '_'
                    }
                })
                .collect::<String>();
            let safe = safe.trim_matches(&['.', '_', '-'][..]);
            if safe.is_empty() {
                fallback.to_owned()
            } else {
                safe.to_owned()
            }
        };
        store
            .root
            .join(component("TAIQ_USER_ID", "guest"))
            .join(component("TAIQ_WORKSPACE_ID", "default"))
            .join(component("TAIQ_PROJECT_ID", "default"))
    }

    pub fn logical_roots(&self) -> Vec<LogicalRoot> {
        let default_id = self
            .config
            .filestores
            .as_ref()
            .and_then(|stores| stores.default.as_deref())
            .unwrap_or("local");
        let default_id = if self
            .state
            .available_stores
            .iter()
            .any(|(id, _)| id == default_id)
        {
            default_id.to_owned()
        } else {
            self.state.store_id.clone()
        };
        let catalog = crate::connectors::store_catalog(&self.config);
        let default_read_only = catalog
            .iter()
            .find(|store| store.id == default_id)
            .is_some_and(|store| store.read_only);
        let mut roots = vec![LogicalRoot {
            id: "documents".into(),
            label: "Documents".into(),
            store_id: default_id.clone(),
            folder: String::new(),
            read_only: default_read_only,
        }];
        for (id, label) in [("media", "Media"), ("sites", "Sites"), ("git", "Git")] {
            if id == "git"
                && (default_read_only
                    || !catalog
                        .iter()
                        .any(|store| store.id == default_id && store.kind == "local"))
            {
                continue;
            }
            roots.push(LogicalRoot {
                id: id.into(),
                label: label.into(),
                store_id: default_id.clone(),
                folder: label.into(),
                read_only: default_read_only,
            });
        }
        for (id, name) in &self.state.available_stores {
            if id == &default_id {
                continue;
            }
            let definition = catalog.iter().find(|store| &store.id == id);
            if let Some(entry) = self
                .config
                .filestores
                .as_ref()
                .and_then(|stores| stores.stores.get("filestores"))
                .and_then(serde_json::Value::as_array)
                .and_then(|entries| {
                    entries.iter().find(|entry| {
                        entry.get("id").and_then(serde_json::Value::as_str) == Some(id)
                    })
                })
            {
                if entry.get("hidden").and_then(serde_json::Value::as_bool) == Some(true)
                    || entry.get("system").and_then(serde_json::Value::as_bool) == Some(true)
                {
                    continue;
                }
            }
            roots.push(LogicalRoot {
                id: format!("store:{id}"),
                label: name.clone(),
                store_id: id.clone(),
                folder: String::new(),
                read_only: definition.is_some_and(|store| store.read_only),
            });
        }
        roots
    }

    fn terminal_workdir(&self) -> Result<PathBuf> {
        let agent = self
            .config
            .agents
            .agents
            .iter()
            .find(|agent| agent.id == self.state.agent_id)
            .ok_or_else(|| anyhow::anyhow!("select an agent before opening a terminal"))?;
        let runtime = agent
            .other
            .get("runtime")
            .and_then(|runtime| runtime.get("runtime_type"))
            .and_then(|kind| kind.as_str())
            .unwrap_or("local");
        if !matches!(runtime, "embedded" | "inmemory" | "default" | "local") {
            anyhow::bail!("NotImplemented: terminal for {runtime} agents");
        }
        self.local_stores
            .iter()
            .find(|store| store.id == self.state.store_id)
            .map(|store| Self::desktop_store_root(&self.config, store))
            .ok_or_else(|| anyhow::anyhow!("select a local workspace for the terminal"))
    }

    fn rebuild_stores(&mut self) -> Result<()> {
        let workspaces = self
            .local_stores
            .iter()
            .filter(|store| store.id.starts_with("workspace:"))
            .cloned()
            .collect::<Vec<_>>();
        self.local_stores = local_store_roots(&self.config, &self.config_path)?;
        for workspace in workspaces {
            if !self
                .local_stores
                .iter()
                .any(|store| store.id == workspace.id)
            {
                self.local_stores.push(workspace);
            }
        }
        self.remote_stores.clear();
        let mut available = self
            .local_stores
            .iter()
            .map(|store| (store.id.clone(), store.name.clone()))
            .collect::<Vec<_>>();
        let catalog = crate::connectors::store_catalog(&self.config);
        let enabled_memory = catalog
            .iter()
            .filter(|store| store.enabled && store.kind == "memory")
            .map(|store| store.id.clone())
            .collect::<HashSet<_>>();
        self.memory_stores
            .retain(|id, _| enabled_memory.contains(id));
        for store in catalog
            .iter()
            .filter(|store| store.enabled && store.kind == "memory")
        {
            self.memory_stores
                .entry(store.id.clone())
                .or_insert_with(|| Arc::new(MemoryFileStore::new()));
            available.push((store.id.clone(), store.name.clone()));
        }
        if let Some(entries) = self
            .config
            .filestores
            .as_ref()
            .and_then(|stores| stores.stores.get("filestores"))
            .and_then(serde_json::Value::as_array)
        {
            for entry in entries {
                if entry.get("enabled").and_then(serde_json::Value::as_bool) == Some(false) {
                    continue;
                }
                let Some(kind) = entry.get("type").and_then(serde_json::Value::as_str) else {
                    continue;
                };
                if !matches!(
                    kind,
                    "s3" | "gcs"
                        | "azure"
                        | "azblob"
                        | "webdav"
                        | "rclone"
                        | "seaweedfs"
                        | "juicefs"
                ) {
                    continue;
                }
                let Some(id) = entry.get("id").and_then(serde_json::Value::as_str) else {
                    continue;
                };
                let name = entry
                    .get("name")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or(id)
                    .to_owned();
                self.remote_stores.insert(id.to_owned(), entry.clone());
                available.push((id.to_owned(), name));
            }
        }
        self.state.available_stores = available;
        if !self
            .state
            .available_stores
            .iter()
            .any(|(id, _)| id == &self.state.store_id)
        {
            if let Some((id, _)) = self.state.available_stores.first() {
                self.apply(UiEvent::SelectStore(id.clone()))?;
            }
        }
        Ok(())
    }

    pub fn dispatch(&mut self, event: UiEvent) {
        if let Err(error) = self.apply(event) {
            self.flash(FlashLevel::Error, error.to_string());
        }
    }

    fn apply(&mut self, event: UiEvent) -> Result<()> {
        match event {
            UiEvent::SelectView(view) => {
                self.state.left_view = view;
                self.state.narrow_left = true;
                self.save_layout()?;
            }
            UiEvent::SelectCenter(view) => {
                self.state.center_view = view;
                self.state.narrow_editor = false;
                self.state.narrow_left = false;
                self.save_layout()?;
            }
            UiEvent::SetNarrowEditor(show) => {
                self.state.narrow_editor = show;
                self.state.narrow_left = false;
            }
            UiEvent::ResizePanels(left, editor) => {
                self.state.left_width = left.clamp(170.0, 600.0);
                self.state.editor_width = editor.clamp(260.0, 750.0);
                self.save_layout()?;
            }
            UiEvent::SetDarkMode(dark) => {
                self.state.dark_mode = dark;
                theme::set_dark(dark);
                self.save_layout()?;
            }
            UiEvent::SetShowReasoning(show) => {
                self.state.show_reasoning = show;
                self.save_layout()?;
            }
            UiEvent::SetDocumentView(view) => {
                self.state.document_view = view;
                self.save_layout()?;
            }
            UiEvent::MoveCenterTab(source, target) => {
                if source != target && self.state.center_tabs.contains(&source) {
                    self.state.center_tabs.retain(|tab| *tab != source);
                    if let Some(index) =
                        self.state.center_tabs.iter().position(|tab| *tab == target)
                    {
                        self.state.center_tabs.insert(index, source);
                        self.save_layout()?;
                    }
                }
            }
            UiEvent::MoveDocument(source, target) => {
                if source != target {
                    if let Some(index) = self.state.documents.iter().position(|d| d.path == source)
                    {
                        let doc = self.state.documents.remove(index);
                        let target_index = self
                            .state
                            .documents
                            .iter()
                            .position(|d| d.path == target)
                            .unwrap_or(self.state.documents.len());
                        self.state.documents.insert(target_index, doc);
                        self.save_layout()?;
                    }
                }
            }
            UiEvent::ToggleDocumentFloating(path) => {
                if !self.state.floating_documents.insert(path.clone()) {
                    self.state.floating_documents.remove(&path);
                }
                self.save_layout()?;
            }
            UiEvent::SetFloatingBounds(path, bounds) => {
                self.state.floating_bounds.insert(path, bounds);
                self.save_layout()?;
            }
            UiEvent::SetDocumentText(path, text) => {
                if let Some(doc) = self
                    .state
                    .documents
                    .iter_mut()
                    .find(|d| d.path == path && d.editable)
                {
                    doc.text = text.clone();
                }
                if let Some(doc) = &mut self.state.editor {
                    if doc.path == path && doc.editable {
                        doc.text = text;
                    }
                }
            }
            UiEvent::SaveDocument(path) => {
                if let Some(doc) = self
                    .state
                    .documents
                    .iter_mut()
                    .find(|d| d.path == path && d.editable)
                {
                    self.store.write_text(&path, &doc.text)?;
                    doc.saved_text = doc.text.clone();
                    if let Some(active) = &mut self.state.editor {
                        if active.path == path {
                            active.saved_text = doc.saved_text.clone();
                        }
                    }
                    self.flash(FlashLevel::Info, "File saved".into());
                }
            }
            UiEvent::RemoveQueuedChat(index) => {
                self.state.queued_chat.remove(index);
                self.persist_chat();
            }
            UiEvent::MoveQueuedChat(from, to) => {
                if from < self.state.queued_chat.len() && to < self.state.queued_chat.len() {
                    let item = self.state.queued_chat.remove(from).unwrap();
                    self.state.queued_chat.insert(to, item);
                    self.persist_chat();
                }
            }
            UiEvent::ClearQueuedChat => {
                self.state.queued_chat.clear();
                self.persist_chat();
            }
            UiEvent::RunNextQueuedChat => {
                if !self.state.busy {
                    if let Some(next) = self.state.queued_chat.pop_front() {
                        self.state.chat_draft = next.prompt;
                        self.state.attachments = next.attachments;
                        self.state.attach_open_file = next.attach_open_file;
                        if let Err(error) = self.send_chat() {
                            self.persist_chat();
                            return Err(error);
                        }
                    }
                }
            }
            UiEvent::SetDefaultAgent(id) => {
                if !self
                    .config
                    .agents
                    .agents
                    .iter()
                    .any(|agent| agent.id == id && agent.enabled != Some(false))
                {
                    anyhow::bail!("enabled agent not found: {id}");
                }
                for agent in &mut self.config.agents.agents {
                    agent.default = Some(agent.id == id);
                }
                let draft = serde_yaml::to_string(&self.config.agents.agents)?;
                self.management
                    .save(AdminSection::Agents, &draft, &mut self.config)?;
                self.flash(FlashLevel::Info, format!("Default agent: {id}"));
            }
            UiEvent::SetResourceEnabled(section, id, enabled) => {
                let mut draft = self.admin_content(section)?;
                match section {
                    AdminSection::Agents => {
                        let mut agents: Vec<crate::schema::AgentDefinition> =
                            serde_yaml::from_str(&draft)?;
                        let agent = agents
                            .iter_mut()
                            .find(|agent| agent.id == id)
                            .ok_or_else(|| anyhow::anyhow!("agent not found: {id}"))?;
                        if !enabled && agent.default == Some(true) {
                            anyhow::bail!("choose another default agent first");
                        }
                        if !enabled && self.state.agent_id == id {
                            anyhow::bail!("switch agents before disabling this one");
                        }
                        agent.enabled = Some(enabled);
                        if !agents.iter().any(|agent| agent.enabled != Some(false)) {
                            anyhow::bail!("at least one agent must remain enabled");
                        }
                        draft = serde_yaml::to_string(&agents)?;
                    }
                    AdminSection::Models => {
                        let mut models: Option<crate::schema::ModelsConfig> =
                            serde_yaml::from_str(&draft)?;
                        let models = models
                            .as_mut()
                            .ok_or_else(|| anyhow::anyhow!("models are not configured"))?;
                        let model = models
                            .models
                            .iter_mut()
                            .find(|model| model.id.as_deref() == Some(&id) || model.name == id)
                            .ok_or_else(|| anyhow::anyhow!("model not found: {id}"))?;
                        if !enabled && model.default == Some(true) {
                            anyhow::bail!("choose another default model first");
                        }
                        model
                            .other
                            .insert("enabled".into(), serde_json::Value::Bool(enabled));
                        if !models.models.iter().any(|model| {
                            model.other.get("enabled").and_then(|value| value.as_bool())
                                != Some(false)
                        }) {
                            anyhow::bail!("at least one model must remain enabled");
                        }
                        draft = serde_yaml::to_string(&models)?;
                    }
                    AdminSection::Connectors => {
                        let mut connectors: Vec<crate::schema::ConnectorDefinition> =
                            serde_yaml::from_str(&draft)?;
                        let connector = connectors
                            .iter_mut()
                            .find(|connector| connector.id.as_deref() == Some(&id))
                            .ok_or_else(|| anyhow::anyhow!("connector not found: {id}"))?;
                        connector.enabled = Some(serde_json::Value::Bool(enabled));
                        connector.enable = Some(serde_json::Value::Bool(enabled));
                        draft = serde_yaml::to_string(&connectors)?;
                    }
                    AdminSection::Filestores => {
                        let mut stores: crate::schema::FileStoresConfig =
                            serde_yaml::from_str(&draft)?;
                        if !enabled && stores.default.as_deref() == Some(&id) {
                            anyhow::bail!("choose another default filestore first");
                        }
                        if !enabled && self.state.store_id == id {
                            anyhow::bail!("switch filestores before disabling this one");
                        }
                        let entry = stores
                            .stores
                            .get_mut("filestores")
                            .and_then(serde_json::Value::as_array_mut)
                            .and_then(|entries| {
                                entries.iter_mut().find(|entry| {
                                    entry.get("id").and_then(serde_json::Value::as_str) == Some(&id)
                                })
                            })
                            .ok_or_else(|| anyhow::anyhow!("filestore not found: {id}"))?;
                        entry["enabled"] = serde_json::Value::Bool(enabled);
                        draft = serde_yaml::to_string(&stores)?;
                    }
                    _ => anyhow::bail!("this section has no resource toggle"),
                }
                self.management.save(section, &draft, &mut self.config)?;
                if section == AdminSection::Filestores {
                    self.rebuild_stores()?;
                }
                self.state.admin_draft = self.admin_content(section)?;
                self.flash(
                    FlashLevel::Info,
                    format!("{} {}", id, if enabled { "enabled" } else { "disabled" }),
                );
            }
            UiEvent::SelectAdmin(section) => {
                self.state.admin_section = section;
                self.state.center_view = CenterView::Admin;
                self.state.admin_draft = self.admin_content(section)?;
                self.save_layout()?;
            }
            UiEvent::SelectConnector(id) => {
                if !self
                    .config
                    .connectors
                    .iter()
                    .any(|c| c.id.as_deref() == Some(&id))
                {
                    anyhow::bail!("connector not found: {id}");
                }
                self.state.admin_section = AdminSection::Connectors;
                self.state.center_view = CenterView::Admin;
                self.state.admin_target_id = id;
                self.state.admin_draft = self.admin_content(AdminSection::Connectors)?;
                self.state.admin_output.clear();
                self.save_layout()?;
            }
            UiEvent::SetAdminDraft(text) => self.state.admin_draft = text,
            UiEvent::SaveAdmin => {
                if self.state.admin_section == AdminSection::Knowledge {
                    let items: Vec<Knowledge> = serde_yaml::from_str(&self.state.admin_draft)?;
                    KnowledgeStore::open(&self.config_path).replace(&items)?;
                }
                if self.state.admin_section == AdminSection::Workflows {
                    let workflows: Vec<Workflow> = serde_yaml::from_str(&self.state.admin_draft)?;
                    for workflow in &workflows {
                        workflow.validate()?;
                    }
                }
                if self.state.admin_section == AdminSection::Schedules {
                    let schedules: Vec<Schedule> = serde_yaml::from_str(&self.state.admin_draft)?;
                    for schedule in &schedules {
                        schedule.interval()?;
                    }
                }
                self.management.save(
                    self.state.admin_section,
                    &self.state.admin_draft,
                    &mut self.config,
                )?;
                if self.state.admin_section == AdminSection::Filestores {
                    self.rebuild_stores()?;
                    self.state.admin_draft = self.admin_content(AdminSection::Filestores)?;
                }
                self.flash(
                    FlashLevel::Info,
                    format!("{} saved locally", self.state.admin_section.label()),
                );
            }
            UiEvent::SetAdminTarget(id) => self.state.admin_target_id = id,
            UiEvent::SetAdminRunInput(input) => self.state.admin_run_input = input,
            UiEvent::RunAdmin => self.run_admin()?,
            UiEvent::CancelAdmin => {
                if let Some(jobs) = &mut self.jobs {
                    jobs.cancel();
                }
                self.state.admin_busy = false;
                self.state.admin_output = "Cancelled".into();
            }
            UiEvent::SetCredentialDraft(draft) => self.state.credential_draft = draft,
            UiEvent::SaveCredential => {
                let draft: CredentialDraft = serde_yaml::from_str(&self.state.credential_draft)?;
                self.credentials.save(draft)?;
                self.state.credential_draft.clear();
                self.state.admin_draft = self.admin_content(AdminSection::Credentials)?;
                self.flash(FlashLevel::Info, "Credential saved in the OS vault".into());
            }
            UiEvent::DeleteCredential(id) => {
                self.credentials.delete(&id)?;
                self.state.admin_draft = self.admin_content(AdminSection::Credentials)?;
                self.flash(FlashLevel::Info, "Credential deleted".into());
            }
            UiEvent::ToggleFolder(path) => {
                let mut ancestor = String::new();
                for component in path
                    .split('/')
                    .take(path.split('/').count().saturating_sub(1))
                {
                    if !ancestor.is_empty() {
                        ancestor.push('/');
                    }
                    ancestor.push_str(component);
                    self.state.expanded.insert(ancestor.clone());
                }
                if !self.state.expanded.insert(path.clone()) {
                    self.state.expanded.remove(&path);
                }
                self.refresh_tree()?;
            }
            UiEvent::RefreshFiles => self.refresh_tree()?,
            UiEvent::OpenFile(path) => {
                self.state.narrow_editor = true;
                self.state.selected_path = Some(path.clone());
                if let Some(existing) = self
                    .state
                    .documents
                    .iter()
                    .find(|d| d.path == path)
                    .cloned()
                {
                    self.state.editor = Some(existing);
                    self.save_layout()?;
                    return Ok(());
                }
                let Some(plugin) = plugins::for_file(&path) else {
                    self.not_implemented("file viewer for this format");
                    return Ok(());
                };
                let text = self.store.read_text(&path)?;
                self.state.editor = Some(EditorDocument {
                    path,
                    saved_text: text.clone(),
                    text,
                    plugin: plugin.id,
                    editable: plugin.kind == PluginKind::Editor,
                });
                self.sync_document();
                self.save_layout()?;
            }
            UiEvent::SelectEditor(path) => {
                self.state.editor = self
                    .state
                    .documents
                    .iter()
                    .find(|d| d.path == path)
                    .cloned();
                self.save_layout()?;
            }
            UiEvent::SetEditorText(text) => {
                if let Some(doc) = &mut self.state.editor {
                    if doc.editable {
                        doc.text = text;
                    }
                }
                self.sync_document();
            }
            UiEvent::SaveFile => {
                if let Some(doc) = &mut self.state.editor {
                    if !doc.editable {
                        self.not_implemented("save for this viewer");
                    } else {
                        if doc.path.ends_with(".ipynb") {
                            let notebook: serde_json::Value = serde_json::from_str(&doc.text)?;
                            if !notebook
                                .get("cells")
                                .is_some_and(serde_json::Value::is_array)
                            {
                                anyhow::bail!("notebook must contain a cells array");
                            }
                        }
                        self.store.write_text(&doc.path, &doc.text)?;
                        doc.saved_text = doc.text.clone();
                        self.flash(FlashLevel::Info, "File saved".into());
                        self.sync_document();
                    }
                }
            }
            UiEvent::RevertFile => {
                if let Some(doc) = &mut self.state.editor {
                    doc.text = doc.saved_text.clone();
                }
                self.sync_document();
            }
            UiEvent::FormatDocument => {
                if let Some(doc) = &mut self.state.editor {
                    if doc.path.ends_with(".json") || doc.path.ends_with(".ipynb") {
                        let value: serde_json::Value = serde_json::from_str(&doc.text)?;
                        doc.text = serde_json::to_string_pretty(&value)?;
                        self.sync_document();
                    } else {
                        self.not_implemented("formatter for this file type");
                    }
                }
            }
            UiEvent::CreateFile(path) => {
                let path = if path.contains('/') || self.selected_folder().is_empty() {
                    path
                } else {
                    format!("{}/{}", self.selected_folder(), path)
                };
                self.store.create_file(&path)?;
                self.state.new_file_draft.clear();
                self.refresh_tree()?;
                self.apply(UiEvent::OpenFile(path))?;
            }
            UiEvent::CreateFolder(path) => {
                let path = if path.contains('/') || self.selected_folder().is_empty() {
                    path
                } else {
                    format!("{}/{}", self.selected_folder(), path)
                };
                self.store.create_dir(&path)?;
                self.refresh_tree()?;
            }
            UiEvent::RenameFile(from, to) => {
                if self.state.documents.iter().any(|d| {
                    d.dirty() && (d.path == from || d.path.starts_with(&format!("{from}/")))
                }) {
                    anyhow::bail!("save or discard open changes before moving {from}");
                }
                self.store.rename(&from, &to)?;
                self.state.documents.retain(|d| d.path != from);
                if self.state.editor.as_ref().is_some_and(|d| d.path == from) {
                    self.state.editor = None;
                }
                self.refresh_tree()?;
            }
            UiEvent::DeleteFile(path) => {
                if self.state.documents.iter().any(|d| {
                    d.dirty() && (d.path == path || d.path.starts_with(&format!("{path}/")))
                }) {
                    anyhow::bail!("save or discard open changes before deleting {path}");
                }
                self.store.delete(&path)?;
                self.state
                    .documents
                    .retain(|d| d.path != path && !d.path.starts_with(&format!("{path}/")));
                if self
                    .state
                    .editor
                    .as_ref()
                    .is_some_and(|d| d.path == path || d.path.starts_with(&format!("{path}/")))
                {
                    self.state.editor = None;
                }
                self.refresh_tree()?;
            }
            UiEvent::ImportFile(source, destination) => {
                self.store.import_file(&source, &destination)?;
                self.refresh_tree()?;
            }
            UiEvent::ExportFile(source, destination) => {
                self.store.export_file(&source, &destination)?;
            }
            UiEvent::SetFileSearch(query) => {
                self.state.file_search = query;
                self.state.search_active = false;
                self.state.search_results.clear();
            }
            UiEvent::SearchFiles => {
                self.state.search_results =
                    crate::filestore::search(self.store.as_ref(), &self.state.file_search)?
                        .into_iter()
                        .map(|entry| TreeNode {
                            entry,
                            depth: 0,
                            expanded: false,
                        })
                        .collect();
                self.state.search_active = true;
            }
            UiEvent::SelectPath(path) => self.state.selected_path = Some(path),
            UiEvent::SelectStore(id) => {
                if self.state.documents.iter().any(EditorDocument::dirty) {
                    anyhow::bail!("save or discard open changes before switching filestores");
                }
                self.store = if let Some(store) = self.memory_stores.get(&id) {
                    store.clone()
                } else if let Some(definition) = self.remote_stores.get(&id) {
                    Arc::new(RemoteFileStore::from_config(definition, &self.config_path)?)
                } else {
                    let selected = self
                        .local_stores
                        .iter()
                        .find(|s| s.id == id)
                        .ok_or_else(|| anyhow::anyhow!("unknown filestore: {id}"))?;
                    Arc::new(LocalFileStore::new_with_mode(
                        Self::desktop_store_root(&self.config, selected),
                        selected.read_only,
                    )?)
                };
                self.state.store_id = id;
                let default_id = self
                    .config
                    .filestores
                    .as_ref()
                    .and_then(|stores| stores.default.as_deref())
                    .unwrap_or("local");
                self.state.active_logical_root = if self.state.store_id == default_id {
                    "documents".into()
                } else {
                    format!("store:{}", self.state.store_id)
                };
                self.state.editor = None;
                self.state.documents.clear();
                self.state.expanded.clear();
                self.state.selected_path = None;
                self.refresh_tree()?;
                self.save_layout()?;
            }
            UiEvent::SelectLogicalRoot(id) => {
                let root = self
                    .logical_roots()
                    .into_iter()
                    .find(|root| root.id == id)
                    .ok_or_else(|| anyhow::anyhow!("unknown Files root: {id}"))?;
                if self.state.store_id != root.store_id {
                    self.apply(UiEvent::SelectStore(root.store_id.clone()))?;
                }
                self.state.active_logical_root = id;
                self.state.expanded.clear();
                self.state.selected_path = if root.folder.is_empty() {
                    None
                } else {
                    Some(root.folder)
                };
                self.refresh_tree()?;
                self.save_layout()?;
            }
            UiEvent::AddWorkspace(path) => {
                if self.state.documents.iter().any(EditorDocument::dirty) {
                    anyhow::bail!("save or discard open changes before switching workspaces");
                }
                let root = path.canonicalize()?;
                if !root.is_dir() {
                    anyhow::bail!("workspace must be a folder: {}", root.display());
                }
                let definition = Self::workspace_definition(root);
                let id = definition.id.clone();
                if !self.local_stores.iter().any(|s| s.id == id) {
                    self.state
                        .available_stores
                        .push((id.clone(), definition.name.clone()));
                    self.local_stores.push(definition);
                }
                self.apply(UiEvent::SelectStore(id))?;
            }
            UiEvent::SetNewFileDraft(text) => self.state.new_file_draft = text,
            UiEvent::CloseEditor => {
                if self
                    .state
                    .editor
                    .as_ref()
                    .is_some_and(EditorDocument::dirty)
                {
                    self.flash(
                        FlashLevel::Error,
                        "Unsaved changes. Save or discard before closing.".into(),
                    );
                } else {
                    self.close_editor();
                    self.save_layout()?;
                }
            }
            UiEvent::DiscardAndCloseEditor => {
                self.close_editor();
                self.save_layout()?;
            }
            UiEvent::SelectAgent(id) => {
                if self
                    .config
                    .agents
                    .agents
                    .iter()
                    .any(|a| a.id == id && a.enabled != Some(false))
                {
                    if id != self.state.agent_id {
                        if let Some(session_id) = self
                            .sessions
                            .sessions()
                            .iter()
                            .rev()
                            .find(|session| session.agent_id == id)
                            .map(|session| session.id.clone())
                        {
                            self.apply(UiEvent::SelectSession(session_id))?;
                        } else {
                            self.persist_chat();
                            self.park_chat_bridge();
                            self.state.agent_id = id;
                            self.state.chat.clear();
                            self.state.chat_activity.clear();
                            self.state.chat_draft.clear();
                            self.state.attachments.clear();
                            self.state.attach_open_file = false;
                            self.state.busy = false;
                            self.state.queued_chat.clear();
                            self.state.selected_model = "auto".into();
                            self.state.reasoning_effort = None;
                            self.new_session()?;
                        }
                    }
                }
            }
            UiEvent::SetChatDraft(text) => {
                self.state.chat_draft = text;
                self.draft_dirty_at = Some(Instant::now());
            }
            UiEvent::SetAttachOpenFile(attach) => {
                self.state.attach_open_file = attach;
                self.draft_dirty_at = Some(Instant::now());
            }
            UiEvent::AttachFiles(paths) => {
                for path in paths {
                    let metadata = std::fs::metadata(&path)?;
                    if metadata.len() > 2 * 1024 * 1024 {
                        self.not_implemented("attachments larger than 2 MB");
                        continue;
                    }
                    match std::fs::read_to_string(&path) {
                        Ok(content) => self.state.attachments.push(ChatAttachment {
                            name: path
                                .file_name()
                                .unwrap_or_default()
                                .to_string_lossy()
                                .into_owned(),
                            content,
                        }),
                        Err(_) => self.not_implemented("binary attachment preview"),
                    }
                }
                self.draft_dirty_at = Some(Instant::now());
            }
            UiEvent::RemoveAttachment(index) => {
                if index < self.state.attachments.len() {
                    self.state.attachments.remove(index);
                    self.draft_dirty_at = Some(Instant::now());
                }
            }
            UiEvent::SendChat => self.send_chat()?,
            UiEvent::StartVoice => voice::start()?,
            UiEvent::CancelChat => {
                if let Some(bridge) = &self.chat_bridge {
                    bridge.cancel();
                }
                self.state.busy = false;
                if let Some(last) = self.state.chat.last_mut() {
                    if last.role == ChatRole::Assistant && last.text.is_empty() {
                        last.text = "[Cancelled]".into();
                    }
                }
                self.persist_chat();
            }
            UiEvent::SetMemoryEnabled(enabled) => {
                if self.state.busy {
                    anyhow::bail!("stop the current response before changing memory mode");
                }
                self.state.memory_enabled = enabled;
                self.chat_bridge = None;
                self.persist_chat();
            }
            UiEvent::ClearMemory => {
                if self.state.busy {
                    anyhow::bail!("stop the current response before clearing memory");
                }
                self.state.memory_start = self.state.chat.len();
                self.chat_bridge = None;
                self.persist_chat();
                self.flash(
                    FlashLevel::Info,
                    "Agent context cleared; chat history remains visible".into(),
                );
            }
            UiEvent::SelectModel(model) => {
                if self.state.busy {
                    anyhow::bail!("stop the current response before changing models");
                }
                self.state.selected_model = model;
                let levels = model_picker::reasoning_levels(
                    &self.config,
                    &self.state.agent_id,
                    &self.state.selected_model,
                );
                if self
                    .state
                    .reasoning_effort
                    .as_ref()
                    .is_some_and(|value| !levels.contains(value))
                {
                    self.state.reasoning_effort = None;
                }
                self.chat_bridge = None;
                self.persist_chat();
            }
            UiEvent::SetReasoningEffort(effort) => {
                if self.state.busy {
                    anyhow::bail!("stop the current response before changing reasoning");
                }
                let levels = model_picker::reasoning_levels(
                    &self.config,
                    &self.state.agent_id,
                    &self.state.selected_model,
                );
                if effort.as_ref().is_some_and(|value| !levels.contains(value)) {
                    anyhow::bail!("reasoning effort is unsupported by the effective model");
                }
                self.state.reasoning_effort = effort;
                self.chat_bridge = None;
                self.persist_chat();
            }
            UiEvent::NewChat => {
                self.persist_chat();
                self.park_chat_bridge();
                self.state.chat.clear();
                self.state.chat_activity.clear();
                self.state.chat_draft.clear();
                self.state.attachments.clear();
                self.state.attach_open_file = false;
                self.state.busy = false;
                self.state.queued_chat.clear();
                self.state.memory_start = 0;
                self.new_session()?;
            }
            UiEvent::SelectSession(id) => {
                if id == self.state.active_session_id {
                    return Ok(());
                }
                self.persist_chat();
                self.park_chat_bridge();
                if self.sessions.select(&id) {
                    if let Some(session) = self.sessions.active() {
                        self.state.agent_id = session.agent_id.clone();
                        self.state.selected_model = session.model_id.clone();
                        self.state.reasoning_effort = session.reasoning_effort.clone();
                        self.state.memory_enabled = session.memory_enabled;
                        self.state.memory_start = session.memory_start;
                        self.state.chat = session.messages.clone();
                        self.state.queued_chat = session.queue.clone().into();
                        self.state.chat_draft = session.draft.clone();
                        self.state.attachments = session.attachments.clone();
                        self.state.attach_open_file = session.attach_open_file;
                        self.state.chat_activity =
                            self.background_activity.remove(&id).unwrap_or_default();
                        self.state.active_session_id = id.clone();
                        self.chat_bridge = self.background_bridges.remove(&id);
                        self.state.busy = self
                            .chat_bridge
                            .as_ref()
                            .is_some_and(ChatBridge::is_running);
                    }
                    self.sessions.save()?;
                }
            }
            UiEvent::SetSessionQuery(query) => self.state.session_query = query,
            UiEvent::OpenAgentSettings => {
                let agent = self
                    .config
                    .agents
                    .agents
                    .iter()
                    .find(|agent| agent.id == self.state.agent_id)
                    .ok_or_else(|| anyhow::anyhow!("selected agent is unavailable"))?;
                self.state.agent_settings = Some(AgentSettingsState::new(agent));
            }
            UiEvent::CloseAgentSettings => {
                if self
                    .state
                    .agent_settings
                    .as_ref()
                    .is_some_and(|settings| settings.dirty)
                {
                    anyhow::bail!(
                        "save agent settings before closing, or discard changes from the settings toolbar"
                    );
                }
                self.state.agent_settings = None;
            }
            UiEvent::DiscardAgentSettings => self.state.agent_settings = None,
            UiEvent::SelectAgentSection(section) => {
                if let Some(settings) = &mut self.state.agent_settings {
                    settings.select_section(section);
                }
            }
            UiEvent::SetAgentSettingsQuery(query) => {
                if let Some(settings) = &mut self.state.agent_settings {
                    settings.query = query;
                }
            }
            UiEvent::SetAgentSettingsCategory(category) => {
                if let Some(settings) = &mut self.state.agent_settings {
                    settings.category = category;
                }
            }
            UiEvent::SetAgentSettingsEnabledOnly(enabled) => {
                if let Some(settings) = &mut self.state.agent_settings {
                    settings.enabled_only = enabled;
                }
            }
            UiEvent::SetAgentTool(id, field, enabled) => {
                if !crate::agents::adk::supports_native_tool(&id) {
                    self.not_implemented(&format!("Rust tool {id}"));
                    return Ok(());
                }
                if let Some(settings) = &mut self.state.agent_settings {
                    settings.set_tool(&id, &field, enabled);
                }
            }
            UiEvent::SetAgentSkill(id, enabled) => {
                if let Some(settings) = &mut self.state.agent_settings {
                    settings.set_skill(&id, enabled);
                }
            }
            UiEvent::SetAgentField(group, field, value) => {
                if let Some(settings) = &mut self.state.agent_settings {
                    settings.set_field(&group, &field, value);
                }
            }
            UiEvent::SetAgentMemory(scope, field, enabled) => {
                if let Some(settings) = &mut self.state.agent_settings {
                    settings.set_memory_value(&scope, &field, enabled);
                }
            }
            UiEvent::SetAgentRaw(text) => {
                if let Some(settings) = &mut self.state.agent_settings {
                    settings.raw_text = text;
                }
            }
            UiEvent::ApplyAgentRaw => {
                if let Some(settings) = &mut self.state.agent_settings {
                    settings.apply_raw()?;
                }
            }
            UiEvent::SetAgentName(name) => {
                if let Some(settings) = &mut self.state.agent_settings {
                    settings.draft.name = name;
                    settings.dirty = true;
                }
            }
            UiEvent::SetAgentCategory(category) => {
                if let Some(settings) = &mut self.state.agent_settings {
                    settings
                        .draft
                        .other
                        .insert("category".into(), category.into());
                    settings.dirty = true;
                }
            }
            UiEvent::SetAgentEnabled(enabled) => {
                if let Some(settings) = &mut self.state.agent_settings {
                    settings.draft.enabled = Some(enabled);
                    settings.dirty = true;
                }
            }
            UiEvent::SetAgentDefault(default) => {
                if let Some(settings) = &mut self.state.agent_settings {
                    settings.draft.default = Some(default);
                    settings.dirty = true;
                }
            }
            UiEvent::SetAgentDescription(description) => {
                if let Some(settings) = &mut self.state.agent_settings {
                    settings.draft.description = Some(description);
                    settings.dirty = true;
                }
            }
            UiEvent::SaveAgentSettings => {
                let settings = self
                    .state
                    .agent_settings
                    .as_ref()
                    .ok_or_else(|| anyhow::anyhow!("agent settings are not open"))?;
                let mut agents = self.config.agents.agents.clone();
                let target = agents
                    .iter_mut()
                    .find(|agent| agent.id == settings.agent_id)
                    .ok_or_else(|| anyhow::anyhow!("agent was removed while settings were open"))?;
                *target = settings.draft.clone();
                if settings.draft.default == Some(true) {
                    for agent in &mut agents {
                        if agent.id != settings.agent_id {
                            agent.default = Some(false);
                        }
                    }
                }
                self.management.save(
                    AdminSection::Agents,
                    &serde_yaml::to_string(&agents)?,
                    &mut self.config,
                )?;
                if let Some(settings) = &mut self.state.agent_settings {
                    settings.dirty = false;
                }
                if !self.state.busy {
                    self.chat_bridge = None;
                }
                self.flash(FlashLevel::Info, "Agent settings saved".into());
            }
            UiEvent::ToggleTerminal => {
                let id = self.state.active_session_id.clone();
                if self
                    .state
                    .terminals
                    .get(&id)
                    .is_some_and(|terminal| terminal.open)
                {
                    if let Some(terminal) = self.state.terminals.get_mut(&id) {
                        terminal.open = false;
                    }
                } else {
                    if self
                        .terminal_sessions
                        .get_mut(&id)
                        .is_some_and(|session| session.exited().unwrap_or(false))
                    {
                        self.terminal_sessions.remove(&id);
                    }
                    if !self.terminal_sessions.contains_key(&id) {
                        let cwd = self.terminal_workdir()?;
                        let session = TerminalSession::open(&cwd, 100, 24)?;
                        self.state.terminals.insert(
                            id.clone(),
                            TerminalView {
                                open: true,
                                height: 250.0,
                                cwd: session.cwd.clone(),
                                shell: session.shell.clone(),
                                output: "Terminal ready\n".into(),
                                input: String::new(),
                                history: Vec::new(),
                                history_position: 0,
                                running: true,
                                auto_scroll: true,
                                settings_open: false,
                                buffer_lines: 5000,
                            },
                        );
                        self.terminal_sessions.insert(id, session);
                    } else if let Some(terminal) = self.state.terminals.get_mut(&id) {
                        terminal.open = true;
                    }
                }
            }
            UiEvent::ResizeTerminal(height) => {
                if let Some(terminal) = self.state.terminals.get_mut(&self.state.active_session_id)
                {
                    terminal.height = height.clamp(120.0, 620.0);
                    if let Some(session) = self.terminal_sessions.get(&self.state.active_session_id)
                    {
                        let rows = (terminal.height / 16.0).round() as u16;
                        let _ = session.resize(100, rows);
                    }
                }
            }
            UiEvent::SetTerminalInput(text) => {
                if let Some(terminal) = self.state.terminals.get_mut(&self.state.active_session_id)
                {
                    terminal.input = text;
                }
            }
            UiEvent::SubmitTerminal => {
                let id = self.state.active_session_id.clone();
                if let Some(terminal) = self.state.terminals.get_mut(&id) {
                    let command = std::mem::take(&mut terminal.input);
                    if !command.trim().is_empty() {
                        let session = self
                            .terminal_sessions
                            .get_mut(&id)
                            .ok_or_else(|| anyhow::anyhow!("terminal session is closed"))?;
                        session.write_command(&command)?;
                        terminal.history.push(command);
                        terminal.history_position = terminal.history.len();
                    }
                }
            }
            UiEvent::InterruptTerminal => {
                if let Some(session) = self
                    .terminal_sessions
                    .get_mut(&self.state.active_session_id)
                {
                    session.interrupt()?;
                }
            }
            UiEvent::ToggleTerminalAutoscroll => {
                if let Some(terminal) = self.state.terminals.get_mut(&self.state.active_session_id)
                {
                    terminal.auto_scroll = !terminal.auto_scroll;
                }
            }
            UiEvent::ToggleTerminalSettings => {
                if let Some(terminal) = self.state.terminals.get_mut(&self.state.active_session_id)
                {
                    terminal.settings_open = !terminal.settings_open;
                }
            }
            UiEvent::SetTerminalBuffer(lines) => {
                if let Some(terminal) = self.state.terminals.get_mut(&self.state.active_session_id)
                {
                    terminal.buffer_lines = lines.clamp(100, 100_000);
                }
            }
            UiEvent::TerminalHistory(direction) => {
                if let Some(terminal) = self.state.terminals.get_mut(&self.state.active_session_id)
                {
                    if !terminal.history.is_empty() {
                        terminal.history_position = (terminal.history_position as i32 + direction)
                            .clamp(0, terminal.history.len() as i32)
                            as usize;
                        terminal.input = terminal
                            .history
                            .get(terminal.history_position)
                            .cloned()
                            .unwrap_or_default();
                    }
                }
            }
            UiEvent::SetWebUrl(url) => self.state.web_url = url,
            UiEvent::OpenWeb => self.load_web(true)?,
            UiEvent::WebBack => {
                if let Some(index) = self.state.web_history_index.filter(|index| *index > 0) {
                    self.state.web_history_index = Some(index - 1);
                    self.state.web_url = self.state.web_history[index - 1].clone();
                    self.load_web(false)?;
                }
            }
            UiEvent::WebForward => {
                if let Some(index) = self
                    .state
                    .web_history_index
                    .filter(|index| *index + 1 < self.state.web_history.len())
                {
                    self.state.web_history_index = Some(index + 1);
                    self.state.web_url = self.state.web_history[index + 1].clone();
                    self.load_web(false)?;
                }
            }
            UiEvent::DecideTool(id, approve) => {
                anyhow::ensure!(
                    self.chat_bridge
                        .as_ref()
                        .is_some_and(|bridge| bridge.decide(id, approve)),
                    "approval request is no longer pending"
                );
                for activity in &mut self.state.chat_activity {
                    if let ChatActivityKind::Approval {
                        id: activity_id,
                        decision,
                        ..
                    } = &mut activity.kind
                    {
                        if *activity_id == id {
                            *decision = Some(approve);
                        }
                    }
                }
            }
            UiEvent::Unsupported(feature) => self.not_implemented(feature),
        }
        Ok(())
    }

    fn load_web(&mut self, record_history: bool) -> Result<()> {
        let url = self.state.web_url.trim().to_owned();
        anyhow::ensure!(!url.is_empty(), "enter a web address");
        if record_history {
            let next = self.state.web_history_index.map_or(0, |index| index + 1);
            self.state.web_history.truncate(next);
            self.state.web_history.push(url.clone());
            self.state.web_history_index = Some(self.state.web_history.len() - 1);
        }
        let (sender, receiver) = std::sync::mpsc::channel();
        self.web_receiver = Some(receiver);
        self.state.web_loading = true;
        std::thread::spawn(move || {
            let result = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .map_err(|e| e.to_string())
                .and_then(|runtime| {
                    runtime
                        .block_on(web::fetch(&url))
                        .map_err(|e| e.to_string())
                });
            let _ = sender.send(result);
        });
        Ok(())
    }

    pub fn poll(&mut self) -> bool {
        let mut background_updates = Vec::new();
        for (id, bridge) in &self.background_bridges {
            while let Some(update) = bridge.try_recv() {
                background_updates.push((id.clone(), update));
            }
        }
        let background_changed = !background_updates.is_empty();
        for (id, update) in background_updates {
            match update {
                ChatUpdate::Text(text) => self.sessions.append_assistant_text(&id, &text),
                ChatUpdate::Error(error) => self
                    .sessions
                    .append_assistant_text(&id, &format!("\n[error] {error}")),
                ChatUpdate::ToolCall(name, args) => self
                    .background_activity
                    .entry(id.clone())
                    .or_default()
                    .push(ChatActivity {
                        after_message: self.sessions.message_count(&id).saturating_sub(1),
                        kind: ChatActivityKind::ToolCall { name, args },
                    }),
                ChatUpdate::ToolResult(name, result) => self
                    .background_activity
                    .entry(id.clone())
                    .or_default()
                    .push(ChatActivity {
                        after_message: self.sessions.message_count(&id).saturating_sub(1),
                        kind: ChatActivityKind::ToolResult { name, result },
                    }),
                ChatUpdate::Reasoning(text) => self
                    .background_activity
                    .entry(id.clone())
                    .or_default()
                    .push(ChatActivity {
                        after_message: self.sessions.message_count(&id).saturating_sub(1),
                        kind: ChatActivityKind::Reasoning(text),
                    }),
                ChatUpdate::Approval(approval, name, args) => self
                    .background_activity
                    .entry(id.clone())
                    .or_default()
                    .push(ChatActivity {
                        after_message: self.sessions.message_count(&id).saturating_sub(1),
                        kind: ChatActivityKind::Approval {
                            id: approval,
                            name,
                            args,
                            decision: None,
                        },
                    }),
                ChatUpdate::Done => {}
            }
        }
        if background_changed {
            self.state.sessions = self.sessions.sessions().to_vec();
            if let Err(error) = self.sessions.save() {
                self.flash(FlashLevel::Error, error.to_string());
            }
        }
        let mut terminal_changed = false;
        for (id, session) in &mut self.terminal_sessions {
            let output = plain_output(&session.drain());
            if !output.is_empty() {
                if let Some(terminal) = self.state.terminals.get_mut(id) {
                    terminal.output.push_str(&output);
                    let excess = terminal
                        .output
                        .lines()
                        .count()
                        .saturating_sub(terminal.buffer_lines);
                    if excess > 0 {
                        let offset = terminal
                            .output
                            .match_indices('\n')
                            .nth(excess - 1)
                            .map(|(index, _)| index + 1)
                            .unwrap_or(0);
                        terminal.output.drain(..offset);
                    }
                    if terminal.output.len() > 250_000 {
                        let trim = terminal.output.len() - 200_000;
                        let trim = terminal
                            .output
                            .char_indices()
                            .find(|(i, _)| *i >= trim)
                            .map(|(i, _)| i)
                            .unwrap_or(0);
                        terminal.output.drain(..trim);
                    }
                }
                terminal_changed = true;
            }
            if let Ok(true) = session.exited() {
                if let Some(terminal) = self.state.terminals.get_mut(id) {
                    if terminal.running {
                        terminal.running = false;
                        terminal_changed = true;
                    }
                }
            }
        }
        if self
            .draft_dirty_at
            .is_some_and(|when| when.elapsed() >= Duration::from_millis(500))
        {
            self.persist_chat();
            self.draft_dirty_at = None;
        }
        let mut web_changed = false;
        if let Some(receiver) = &self.web_receiver {
            if let Ok(result) = receiver.try_recv() {
                self.state.web_loading = false;
                match result {
                    Ok(page) => {
                        self.state.web_url = page.url.clone();
                        self.state.web_page = Some(page);
                    }
                    Err(error) => self.flash(FlashLevel::Error, error),
                }
                self.web_receiver = None;
                web_changed = true;
            }
        }
        let mut updates = Vec::new();
        if let Some(bridge) = &self.chat_bridge {
            while let Some(update) = bridge.try_recv() {
                updates.push(update);
            }
        }
        let mut changed =
            background_changed || terminal_changed || web_changed || !updates.is_empty();
        if let Some(jobs) = &self.jobs {
            if let Some(update) = jobs.try_recv() {
                changed = true;
                self.state.admin_busy = false;
                match update {
                    JobUpdate::Completed(value) => {
                        self.state.admin_output = serde_json::to_string_pretty(&value)
                            .unwrap_or_else(|_| value.to_string());
                    }
                    JobUpdate::Progress(value) => {
                        self.state.admin_busy = true;
                        self.state.admin_output = serde_json::to_string_pretty(&value)
                            .unwrap_or_else(|_| value.to_string());
                    }
                    JobUpdate::Failed(error) => {
                        self.state.admin_output = error.clone();
                        self.flash(FlashLevel::Error, error);
                    }
                }
            }
        }
        for update in updates {
            match update {
                ChatUpdate::Text(text) => {
                    if let Some(last) = self.state.chat.last_mut() {
                        last.text.push_str(&text);
                    }
                }
                ChatUpdate::ToolCall(name, args) => self.state.chat_activity.push(ChatActivity {
                    after_message: self.state.chat.len().saturating_sub(1),
                    kind: ChatActivityKind::ToolCall { name, args },
                }),
                ChatUpdate::ToolResult(name, result) => {
                    self.state.chat_activity.push(ChatActivity {
                        after_message: self.state.chat.len().saturating_sub(1),
                        kind: ChatActivityKind::ToolResult { name, result },
                    })
                }
                ChatUpdate::Reasoning(text) => self.state.chat_activity.push(ChatActivity {
                    after_message: self.state.chat.len().saturating_sub(1),
                    kind: ChatActivityKind::Reasoning(text),
                }),
                ChatUpdate::Approval(id, name, args) => {
                    self.state.chat_activity.push(ChatActivity {
                        after_message: self.state.chat.len().saturating_sub(1),
                        kind: ChatActivityKind::Approval {
                            id,
                            name,
                            args,
                            decision: None,
                        },
                    })
                }
                ChatUpdate::Error(error) => {
                    self.flash(FlashLevel::Error, error);
                    self.state.busy = false;
                }
                ChatUpdate::Done => {
                    self.state.busy = false;
                    if !self.state.memory_enabled {
                        self.chat_bridge = None;
                    }
                    self.persist_chat();
                    if let Err(error) = self.refresh_tree() {
                        self.flash(FlashLevel::Error, error.to_string());
                    }
                    if let Some(next) = self.state.queued_chat.pop_front() {
                        self.state.chat_draft = next.prompt;
                        self.state.attachments = next.attachments;
                        self.state.attach_open_file = next.attach_open_file;
                        if let Err(error) = self.send_chat() {
                            self.flash(FlashLevel::Error, error.to_string());
                        }
                    }
                }
            }
        }
        let previous = self.state.flashes.len();
        self.state
            .flashes
            .retain(|f| f.created.elapsed() < Duration::from_secs(6));
        changed || previous != self.state.flashes.len()
    }

    fn run_admin(&mut self) -> Result<()> {
        if self.state.admin_busy {
            anyhow::bail!("a desktop job is already running");
        }
        match self.state.admin_section {
            AdminSection::Schedules => {
                let schedules: Vec<Schedule> = serde_yaml::from_str(&self.state.admin_draft)?;
                let selected = if self.state.admin_target_id.trim().is_empty() {
                    schedules.first()
                } else {
                    schedules
                        .iter()
                        .find(|schedule| schedule.id == self.state.admin_target_id)
                }
                .ok_or_else(|| anyhow::anyhow!("schedule not found"))?
                .clone();
                let interval = selected.interval()?;
                let config = self.config.clone();
                let path = self.config_path.clone();
                let agent = self.state.agent_id.clone();
                let jobs = self.jobs.get_or_insert(JobRunner::new()?);
                jobs.submit_stream(move |sender| async move {
                    let mut timer = tokio::time::interval(interval);
                    timer.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
                    loop {
                        timer.tick().await;
                        let result = run_schedule_once(&selected, &config, &path, &agent).await;
                        let value = match result {
                            Ok(value) => value,
                            Err(error) => serde_json::json!({"schedule_id": selected.id, "error": format!("{error:#}")}),
                        };
                        if sender.send(JobUpdate::Progress(value)).is_err() { break; }
                    }
                    Ok(serde_json::Value::Null)
                });
                self.state.admin_busy = true;
                self.state.admin_output =
                    "Schedule started; runs while this desktop app is open".into();
            }
            AdminSection::Knowledge => {
                let query = self.state.admin_run_input.trim();
                self.state.admin_output = serde_json::to_string_pretty(
                    &KnowledgeStore::open(&self.config_path).search(query)?,
                )?;
            }
            AdminSection::Connectors => {
                let id = self.state.admin_target_id.trim();
                let connector = self
                    .config
                    .connectors
                    .iter()
                    .find(|c| c.id.as_deref() == Some(id))
                    .ok_or_else(|| anyhow::anyhow!("connector not found: {id}"))?
                    .clone();
                let path = self.state.admin_run_input.trim().to_owned();
                let runtime = RemoteConnector::new(connector, &self.config_path)?;
                let jobs = self.jobs.get_or_insert(JobRunner::new()?);
                jobs.submit(async move { runtime.browse(&path).await });
                self.state.admin_busy = true;
                self.state.admin_output = format!("Browsing {id}…");
            }
            AdminSection::Workflows => {
                let workflows: Vec<Workflow> = serde_yaml::from_str(&self.state.admin_draft)?;
                let selected = if self.state.admin_target_id.trim().is_empty() {
                    workflows.first()
                } else {
                    workflows
                        .iter()
                        .find(|workflow| workflow.id == self.state.admin_target_id)
                }
                .ok_or_else(|| anyhow::anyhow!("workflow not found"))?
                .clone();
                let input = self.state.admin_run_input.trim();
                let input: serde_json::Value = if input.is_empty() {
                    serde_json::Value::Null
                } else {
                    serde_json::from_str(input)
                        .unwrap_or_else(|_| serde_json::Value::String(input.to_owned()))
                };
                let config = self.config.clone();
                let path = self.config_path.clone();
                let jobs = self.jobs.get_or_insert(JobRunner::new()?);
                jobs.submit(async move {
                    Ok(serde_json::to_value(
                        run_manual(&selected, input, &config, &path).await?,
                    )?)
                });
                self.state.admin_busy = true;
                self.state.admin_output = "Running workflow…".into();
            }
            AdminSection::Evaluations => {
                let sets: Vec<EvalSet> = serde_yaml::from_str(&self.state.admin_draft)?;
                let selected = if self.state.admin_target_id.trim().is_empty() {
                    sets.first()
                } else {
                    sets.iter().find(|set| set.id == self.state.admin_target_id)
                }
                .ok_or_else(|| anyhow::anyhow!("evaluation set not found"))?
                .clone();
                let agent_id = self.state.agent_id.clone();
                let set_name = selected.name.clone();
                let config = self.config.clone();
                let path = self.config_path.clone();
                let jobs = self.jobs.get_or_insert(JobRunner::new()?);
                jobs.submit(async move {
                    Ok(serde_json::to_value(
                        run_evalset(&selected, &agent_id, &config, &path).await?,
                    )?)
                });
                self.state.admin_busy = true;
                self.state.admin_output =
                    format!("Running {} with {}…", set_name, self.state.agent_id);
            }
            _ => self.not_implemented("run this management section"),
        }
        Ok(())
    }

    fn admin_content(&self, section: AdminSection) -> Result<String> {
        if section == AdminSection::Knowledge {
            return Ok(serde_yaml::to_string(
                &KnowledgeStore::open(&self.config_path).list()?,
            )?);
        }
        if section == AdminSection::Credentials {
            return Ok(serde_yaml::to_string(self.credentials.list())?);
        }
        let saved = self
            .management
            .content(section, &self.config, self.state.sessions.len())?;
        if section == AdminSection::Evaluations && saved.trim() == "[]" {
            return Ok(serde_yaml::to_string(&load_evalsets(
                &self.config,
                &self.config_path,
            )?)?);
        }
        Ok(saved)
    }

    fn park_chat_bridge(&mut self) {
        if let Some(bridge) = self.chat_bridge.take() {
            if self.state.busy || bridge.is_running() {
                self.background_bridges
                    .insert(self.state.active_session_id.clone(), bridge);
                self.background_activity.insert(
                    self.state.active_session_id.clone(),
                    self.state.chat_activity.clone(),
                );
            }
        }
    }

    fn send_chat(&mut self) -> Result<()> {
        if self.state.busy {
            let prompt = self.state.chat_draft.trim().to_owned();
            if !prompt.is_empty() {
                self.state.queued_chat.push_back(QueuedChat {
                    prompt,
                    attachments: std::mem::take(&mut self.state.attachments),
                    attach_open_file: self.state.attach_open_file,
                });
                self.state.chat_draft.clear();
                self.persist_chat();
            }
            return Ok(());
        }
        let prompt = self.state.chat_draft.trim().to_owned();
        if prompt.is_empty() {
            return Ok(());
        }
        let restore_context = self.state.memory_enabled
            && self.chat_bridge.is_none()
            && self.state.chat.len() > self.state.memory_start;
        if self.chat_bridge.is_none() {
            self.chat_bridge = Some(ChatBridge::new(
                &self.config,
                &self.config_path,
                &self.state.agent_id,
                &self.state.selected_model,
                self.state.reasoning_effort.as_deref(),
            )?);
        }
        let request = if self.state.attach_open_file {
            if let Some(doc) = &self.state.editor {
                let excerpt: String = doc.text.chars().take(20_000).collect();
                format!(
                    "Open file: {}\n```\n{}\n```\n\nUser request: {}",
                    doc.path, excerpt, prompt
                )
            } else {
                self.not_implemented("attach open file without a selected file");
                prompt.clone()
            }
        } else {
            prompt.clone()
        };
        let mut request = if restore_context {
            let transcript = self
                .state
                .chat
                .iter()
                .skip(self.state.memory_start)
                .rev()
                .take(20)
                .rev()
                .map(|m| {
                    format!(
                        "{}: {}",
                        if m.role == ChatRole::User {
                            "User"
                        } else {
                            "Assistant"
                        },
                        m.text
                    )
                })
                .collect::<Vec<_>>()
                .join("\n\n");
            format!("Previous conversation:\n{transcript}\n\nCurrent request:\n{request}")
        } else {
            request
        };
        for attachment in &self.state.attachments {
            request.push_str(&format!(
                "\n\nAttached file: {}\n```\n{}\n```",
                attachment.name,
                attachment.content.chars().take(20_000).collect::<String>()
            ));
        }
        self.chat_bridge.as_ref().unwrap().submit(request);
        self.state.chat_draft.clear();
        self.state.attachments.clear();
        self.state.chat.push(ChatMessage {
            role: ChatRole::User,
            text: prompt,
        });
        self.state.chat.push(ChatMessage {
            role: ChatRole::Assistant,
            text: String::new(),
        });
        self.state.busy = true;
        self.persist_chat();
        Ok(())
    }

    fn sync_document(&mut self) {
        if let Some(doc) = &self.state.editor {
            if let Some(existing) = self.state.documents.iter_mut().find(|d| d.path == doc.path) {
                *existing = doc.clone();
            } else {
                self.state.documents.push(doc.clone());
            }
        }
    }

    fn close_editor(&mut self) {
        if let Some(doc) = self.state.editor.take() {
            self.state.documents.retain(|d| d.path != doc.path);
            self.state.floating_documents.remove(&doc.path);
        }
        self.state.editor = self.state.documents.last().cloned();
    }

    fn new_session(&mut self) -> Result<()> {
        self.state.memory_start = 0;
        let session = self.sessions.new_session(&self.state.agent_id);
        self.state.active_session_id = session.id.clone();
        self.sessions.update(
            &self.state.agent_id,
            &self.state.selected_model,
            self.state.reasoning_effort.as_deref(),
            self.state.memory_enabled,
            self.state.memory_start,
            &self.state.queued_chat,
            &self.state.chat_draft,
            &self.state.attachments,
            self.state.attach_open_file,
            &self.state.chat,
        );
        self.state.sessions = self.sessions.sessions().to_vec();
        self.sessions.save()
    }

    fn persist_chat(&mut self) {
        self.sessions.update(
            &self.state.agent_id,
            &self.state.selected_model,
            self.state.reasoning_effort.as_deref(),
            self.state.memory_enabled,
            self.state.memory_start,
            &self.state.queued_chat,
            &self.state.chat_draft,
            &self.state.attachments,
            self.state.attach_open_file,
            &self.state.chat,
        );
        self.state.sessions = self.sessions.sessions().to_vec();
        if let Err(error) = self.sessions.save() {
            self.flash(FlashLevel::Error, error.to_string());
        }
    }

    fn save_layout(&self) -> Result<()> {
        self.preferences.save(&LayoutPreferences {
            left_width: self.state.left_width,
            editor_width: self.state.editor_width,
            left_view: self.state.left_view,
            center_view: self.state.center_view,
            dark_mode: self.state.dark_mode,
            show_reasoning: self.state.show_reasoning,
            document_view: self.state.document_view,
            center_tabs: self.state.center_tabs.clone(),
            open_documents: self
                .state
                .documents
                .iter()
                .map(|d| d.path.clone())
                .collect(),
            active_document: self.state.editor.as_ref().map(|d| d.path.clone()),
            floating_documents: self.state.floating_documents.iter().cloned().collect(),
            floating_bounds: self.state.floating_bounds.clone(),
            selected_store: Some(self.state.store_id.clone()),
            active_logical_root: Some(self.state.active_logical_root.clone()),
            workspace_folders: self
                .local_stores
                .iter()
                .filter(|store| store.id.starts_with("workspace:"))
                .map(|store| store.root.clone())
                .collect(),
        })
    }

    fn refresh_tree(&mut self) -> Result<()> {
        let mut nodes = Vec::new();
        let folder = self
            .logical_roots()
            .into_iter()
            .find(|root| root.id == self.state.active_logical_root)
            .map(|root| root.folder)
            .unwrap_or_default();
        if let Err(error) = self.walk(&folder, 0, &mut nodes) {
            if folder.is_empty() {
                return Err(error);
            }
        }
        self.state.tree = nodes;
        Ok(())
    }

    fn walk(&self, folder: &str, depth: usize, nodes: &mut Vec<TreeNode>) -> Result<()> {
        if depth > 24 || nodes.len() > 5000 {
            return Ok(());
        }
        for entry in self.store.list(folder)? {
            if depth == 0
                && self.state.store_id == "samples"
                && !matches!(
                    entry.name.as_str(),
                    "docs" | "media" | "Media" | "demos" | "ml" | "numpy"
                )
            {
                continue;
            }
            if depth == 0
                && self.state.active_logical_root == "documents"
                && matches!(entry.name.as_str(), "Media" | "Sites" | "Git" | "Sandbox")
            {
                continue;
            }
            let expanded = entry.is_dir && self.state.expanded.contains(&entry.path);
            let child_path = entry.path.clone();
            nodes.push(TreeNode {
                entry,
                depth,
                expanded,
            });
            if expanded {
                self.walk(&child_path, depth + 1, nodes)?;
            }
        }
        Ok(())
    }

    fn not_implemented(&mut self, feature: &str) {
        self.flash(FlashLevel::Error, format!("NotImplemented: {feature}"));
    }
    fn flash(&mut self, level: FlashLevel, text: String) {
        self.state.flashes.push(Flash {
            level,
            text,
            created: Instant::now(),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::load_config;

    #[test]
    fn opens_and_saves_code_through_shared_events() {
        let root = std::env::temp_dir().join(format!("taiq-ui-{}", std::process::id()));
        let store: Arc<dyn FileStore> = Arc::new(LocalFileStore::new(&root).unwrap());
        store.create_file("hello.rs").unwrap();
        let config_path =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../orqestra/forgeiq/config/config.yaml");
        let config = load_config(&config_path, &[]).unwrap();
        let mut ui = UiController::with_store(config, config_path, store.clone()).unwrap();
        ui.dispatch(UiEvent::OpenFile("hello.rs".into()));
        ui.dispatch(UiEvent::SetEditorText("fn main() {}".into()));
        assert!(ui.state.editor.as_ref().unwrap().dirty());
        ui.dispatch(UiEvent::SaveFile);
        assert_eq!(store.read_text("hello.rs").unwrap(), "fn main() {}");
        ui.dispatch(UiEvent::Unsupported("example"));
        assert!(
            ui.state
                .flashes
                .last()
                .unwrap()
                .text
                .starts_with("NotImplemented")
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn logical_roots_follow_forgeiq_filestore_visibility() {
        let path =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../orqestra/forgeiq/config/config.yaml");
        let config = load_config(&path, &[]).unwrap();
        assert!(!agent_prompts(&config, "iq_general_agent").is_empty());
        let root = std::env::temp_dir().join(format!("taiq-roots-{}", std::process::id()));
        let store: Arc<dyn FileStore> = Arc::new(LocalFileStore::new(&root).unwrap());
        let mut ui = UiController::with_store(config.clone(), path.clone(), store).unwrap();
        ui.state.store_id = "local".into();
        ui.state.available_stores = crate::connectors::store_catalog(&config)
            .into_iter()
            .filter(|store| store.enabled)
            .map(|store| (store.id, store.name))
            .collect();
        let roots = ui.logical_roots();
        assert_eq!(roots[0].label, "Documents");
        assert!(roots.iter().any(|root| root.id == "media"));
        assert!(
            roots
                .iter()
                .any(|root| root.id == "store:samples" && root.read_only)
        );
        assert!(!roots.iter().any(|root| root.id == "store:local"));
        let definitions = local_store_roots(&config, &path).unwrap();
        let local = definitions
            .iter()
            .find(|store| store.id == "local")
            .unwrap();
        let samples = definitions
            .iter()
            .find(|store| store.id == "samples")
            .unwrap();
        let scoped = UiController::desktop_store_root(&config, local);
        assert_ne!(scoped, local.root);
        assert_eq!(
            scoped.parent().unwrap().parent().unwrap().parent().unwrap(),
            local.root.as_path()
        );
        assert_eq!(
            UiController::desktop_store_root(&config, samples),
            samples.root
        );
        std::fs::remove_dir_all(root).unwrap();
    }
}
