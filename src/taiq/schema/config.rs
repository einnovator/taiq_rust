use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct ForgeConfig {
    pub workspace_archive_retention_days: Option<u32>,
    pub server: ServerConfig,
    pub notifications: NotificationsConfig,
    pub ui: UiConfig,
    #[serde(rename = "connector-plugins")]
    pub connector_plugins: ConnectorPluginsConfig,
    pub imports: Vec<ImportEntry>,
    pub plugins: PluginsConfig,
    pub models: Option<ModelsConfig>,
    pub auth: Option<AuthConfig>,
    pub connectors: Vec<ConnectorDefinition>,
    pub prompts: Vec<Value>,
    pub agents: AgentsConfig,
    pub memory: MemoriesConfig,
    pub runtime: RuntimesConfig,
    pub filestores: Option<FileStoresConfig>,
    #[serde(flatten)]
    pub extensions: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct ServerConfig {
    pub port: Option<u16>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct NotificationsConfig {
    pub max: Option<u32>,
    pub sound: Option<bool>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct UiConfig {
    pub chat: BTreeMap<String, Value>,
    pub themes: BTreeMap<String, Value>,
    pub branding: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct ConnectorPluginsConfig {
    pub root: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImportEntry {
    #[serde(rename = "import")]
    pub spec: ImportSpec,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImportSpec {
    pub path: String,
    #[serde(default = "all_profile")]
    pub profile: String,
}

fn all_profile() -> String {
    "all".to_owned()
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct PluginsConfig {
    pub root: Option<String>,
    pub include: Vec<PluginInclude>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginInclude {
    pub name: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub hidden: bool,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct ModelsConfig {
    pub engine: ModelEngineConfig,
    pub providers: Vec<ModelProviderConfig>,
    pub models: Vec<LlmModelConfig>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct ModelEngineConfig {
    #[serde(rename = "type")]
    pub kind: Option<String>,
    pub url: Option<String>,
    pub token: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelProviderConfig {
    pub name: String,
    pub id: Option<String>,
    pub api: Option<String>,
    pub url: Option<String>,
    pub token: Option<String>,
    pub litellm_prefix: Option<String>,
    #[serde(flatten)]
    pub other: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmModelConfig {
    pub provider: String,
    pub name: String,
    pub id: Option<String>,
    /// Select OpenAI's Responses API. If absent, the model name prefix decides.
    #[serde(default, alias = "use-responses")]
    pub use_responses: Option<bool>,
    #[serde(rename = "type")]
    pub kind: Option<String>,
    pub default: Option<bool>,
    #[serde(flatten)]
    pub other: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct AuthConfig {
    pub guess: Option<bool>,
    pub guest_ttl_seconds: Option<u32>,
    pub session_secret: Option<String>,
    pub providers: Vec<AuthProvider>,
    #[serde(flatten)]
    pub other: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthProvider {
    pub provider: String,
    pub id: Option<String>,
    pub enabled: Option<bool>,
    #[serde(flatten)]
    pub other: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct ConnectorDefinition {
    pub id: Option<String>,
    pub name: Option<String>,
    pub provider: Option<String>,
    pub proto: Option<String>,
    pub enable: Option<Value>,
    pub enabled: Option<Value>,
    pub connected: Option<Value>,
    pub options: BTreeMap<String, Value>,
    pub credentials: BTreeMap<String, Value>,
    #[serde(flatten)]
    pub other: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct AgentsConfig {
    #[serde(rename = "reasoning-file")]
    pub reasoning_file: Option<String>,
    pub skills: Vec<Value>,
    pub toolsets: Option<Value>,
    pub tools: Vec<Value>,
    pub prompts: Vec<Value>,
    pub ui: Option<Value>,
    pub agents: Vec<AgentDefinition>,
    #[serde(flatten)]
    pub other: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentDefinition {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    #[serde(default)]
    pub enabled: Option<bool>,
    #[serde(default)]
    pub hidden: Option<bool>,
    #[serde(default)]
    pub default: Option<bool>,
    pub instructions: Option<String>,
    #[serde(rename = "instructions-file", alias = "instructions_file")]
    pub instructions_file: Option<String>,
    #[serde(rename = "reasoning-file", alias = "reasoning_file")]
    pub reasoning_file: Option<String>,
    #[serde(default)]
    pub models: Vec<AgentModelConfig>,
    #[serde(default)]
    pub tools: Vec<Value>,
    #[serde(default)]
    pub prompts: Vec<Value>,
    #[serde(flatten)]
    pub other: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentModelConfig {
    pub name: String,
    pub provider: Option<String>,
    #[serde(flatten)]
    pub other: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct MemoriesConfig {
    pub root: Option<String>,
    pub memory: Vec<MemoryDefinition>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryDefinition {
    pub name: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub id: Option<String>,
    #[serde(flatten)]
    pub other: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct RuntimesConfig {
    pub root: Option<String>,
    pub runtime: Vec<RuntimeDefinition>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeDefinition {
    #[serde(rename = "type")]
    pub kind: String,
    pub id: Option<String>,
    pub name: Option<String>,
    #[serde(flatten)]
    pub other: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct FileStoresConfig {
    pub default: Option<String>,
    #[serde(flatten)]
    pub stores: BTreeMap<String, Value>,
}
