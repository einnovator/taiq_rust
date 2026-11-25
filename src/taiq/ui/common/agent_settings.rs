use crate::schema::{AgentDefinition, ForgeConfig};
use serde_json::{Map, Value, json};
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentSection {
    Agent,
    Prompts,
    Connectors,
    Tools,
    Skills,
    Schedules,
    Ui,
    Memory,
    Channels,
    Models,
    Runtime,
}

impl AgentSection {
    pub const ALL: [Self; 11] = [
        Self::Agent,
        Self::Prompts,
        Self::Connectors,
        Self::Tools,
        Self::Skills,
        Self::Schedules,
        Self::Ui,
        Self::Memory,
        Self::Channels,
        Self::Models,
        Self::Runtime,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Self::Agent => "Agent",
            Self::Prompts => "Prompts",
            Self::Connectors => "Connectors",
            Self::Tools => "Tools",
            Self::Skills => "Skills",
            Self::Schedules => "Schedules",
            Self::Ui => "UI",
            Self::Memory => "Memory",
            Self::Channels => "Channels",
            Self::Models => "Models",
            Self::Runtime => "Runtime",
        }
    }
    pub fn icon(self) -> &'static [u8] {
        match self {
            Self::Agent => include_bytes!("icons/agents.svg"),
            Self::Prompts => include_bytes!("icons/prompt.svg"),
            Self::Connectors => include_bytes!("icons/connectors.svg"),
            Self::Tools => include_bytes!("icons/settings.svg"),
            Self::Skills => include_bytes!("icons/prompt.svg"),
            Self::Schedules => include_bytes!("icons/sessions.svg"),
            Self::Ui => include_bytes!("icons/files.svg"),
            Self::Memory => include_bytes!("icons/memory.svg"),
            Self::Channels => include_bytes!("icons/chat.svg"),
            Self::Models => include_bytes!("icons/image.svg"),
            Self::Runtime => include_bytes!("icons/terminal.svg"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct AgentSettingsState {
    pub agent_id: String,
    pub section: AgentSection,
    pub draft: AgentDefinition,
    pub dirty: bool,
    pub query: String,
    pub category: String,
    pub enabled_only: bool,
    pub raw_text: String,
}

impl AgentSettingsState {
    pub fn new(agent: &AgentDefinition) -> Self {
        Self {
            agent_id: agent.id.clone(),
            section: AgentSection::Agent,
            draft: agent.clone(),
            dirty: false,
            query: String::new(),
            category: "All".into(),
            enabled_only: false,
            raw_text: String::new(),
        }
    }

    pub fn select_section(&mut self, section: AgentSection) {
        self.section = section;
        let value = match section {
            AgentSection::Prompts => {
                serde_json::to_value(&self.draft.prompts).unwrap_or(Value::Null)
            }
            AgentSection::Models => serde_json::to_value(&self.draft.models).unwrap_or(Value::Null),
            AgentSection::Connectors => self
                .draft
                .other
                .get("connectors")
                .cloned()
                .unwrap_or(json!([])),
            AgentSection::Schedules => self
                .draft
                .other
                .get("schedules")
                .cloned()
                .unwrap_or(json!([])),
            AgentSection::Channels => self
                .draft
                .other
                .get("channels")
                .cloned()
                .unwrap_or(json!([])),
            AgentSection::Runtime => self
                .draft
                .other
                .get("runtime")
                .cloned()
                .unwrap_or(json!({})),
            _ => Value::Null,
        };
        self.raw_text = serde_yaml::to_string(&value).unwrap_or_default();
    }

    pub fn apply_raw(&mut self) -> anyhow::Result<()> {
        let value: Value = serde_yaml::from_str(&self.raw_text)?;
        match self.section {
            AgentSection::Prompts => self.draft.prompts = serde_json::from_value(value)?,
            AgentSection::Models => self.draft.models = serde_json::from_value(value)?,
            AgentSection::Connectors => {
                self.draft.other.insert("connectors".into(), value);
            }
            AgentSection::Schedules => {
                self.draft.other.insert("schedules".into(), value);
            }
            AgentSection::Channels => {
                self.draft.other.insert("channels".into(), value);
            }
            AgentSection::Runtime => {
                self.draft.other.insert("runtime".into(), value);
            }
            _ => anyhow::bail!("this section uses form controls"),
        }
        self.dirty = true;
        Ok(())
    }

    pub fn set_tool(&mut self, id: &str, key: &str, enabled: bool) {
        if let Some(tool) = self
            .draft
            .tools
            .iter_mut()
            .find(|item| item.get("id").and_then(Value::as_str) == Some(id))
        {
            if let Some(object) = tool.as_object_mut() {
                object.insert(key.into(), json!(enabled));
            }
        } else {
            self.draft.tools.push(json!({"id": id, key: enabled}));
        }
        self.dirty = true;
    }

    pub fn tool_value(&self, id: &str, key: &str, default: bool) -> bool {
        self.draft
            .tools
            .iter()
            .find(|item| item.get("id").and_then(Value::as_str) == Some(id))
            .and_then(|item| item.get(key))
            .and_then(Value::as_bool)
            .unwrap_or(default)
    }

    pub fn set_skill(&mut self, id: &str, enabled: bool) {
        let skills = self
            .draft
            .other
            .entry("skills".into())
            .or_insert_with(|| json!([]));
        if !skills.is_array() {
            *skills = json!([]);
        }
        let items = skills.as_array_mut().expect("array");
        if let Some(item) = items
            .iter_mut()
            .find(|item| item.get("id").and_then(Value::as_str) == Some(id))
        {
            item["enabled"] = json!(enabled);
        } else {
            items.push(json!({"id": id, "enabled": enabled}));
        }
        self.dirty = true;
    }

    pub fn skill_enabled(&self, id: &str) -> bool {
        self.draft
            .other
            .get("skills")
            .and_then(Value::as_array)
            .and_then(|items| {
                items
                    .iter()
                    .find(|item| item.get("id").and_then(Value::as_str) == Some(id))
            })
            .and_then(|item| item.get("enabled"))
            .and_then(Value::as_bool)
            .unwrap_or(true)
    }

    pub fn field(&self, group: &str, key: &str) -> Option<&Value> {
        self.draft.other.get(group)?.get(key)
    }

    pub fn set_field(&mut self, group: &str, key: &str, value: Value) {
        let object = self
            .draft
            .other
            .entry(group.into())
            .or_insert_with(|| Value::Object(Map::new()));
        if !object.is_object() {
            *object = Value::Object(Map::new());
        }
        object
            .as_object_mut()
            .expect("object")
            .insert(key.into(), value);
        self.dirty = true;
    }

    pub fn memory_value(&self, scope: &str, key: &str, default: bool) -> bool {
        self.field("memory", scope)
            .and_then(|part| part.get(key))
            .and_then(Value::as_bool)
            .unwrap_or(default)
    }

    pub fn set_memory_value(&mut self, scope: &str, key: &str, enabled: bool) {
        let memory = self
            .draft
            .other
            .entry("memory".into())
            .or_insert_with(|| json!({}));
        if !memory.is_object() {
            *memory = json!({});
        }
        let part = memory
            .as_object_mut()
            .expect("object")
            .entry(scope)
            .or_insert_with(|| json!({}));
        if !part.is_object() {
            *part = json!({});
        }
        part.as_object_mut()
            .expect("object")
            .insert(key.into(), json!(enabled));
        self.dirty = true;
    }
}

#[derive(Debug, Clone)]
pub struct CatalogEntry {
    pub id: String,
    pub name: String,
    pub category: String,
    pub description: String,
    pub source: String,
}

fn entries(values: impl IntoIterator<Item = Value>, source: &str) -> Vec<CatalogEntry> {
    let mut result = Vec::new();
    for value in values {
        let id = value
            .as_str()
            .or_else(|| value.get("id").and_then(Value::as_str))
            .or_else(|| value.get("name").and_then(Value::as_str))
            .or_else(|| value.get("ref").and_then(Value::as_str));
        let Some(id) = id else { continue };
        if result.iter().any(|entry: &CatalogEntry| entry.id == id) {
            continue;
        }
        result.push(CatalogEntry {
            id: id.to_owned(),
            name: value
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or(id)
                .to_owned(),
            category: value
                .get("category")
                .and_then(Value::as_str)
                .unwrap_or("General")
                .to_owned(),
            description: value
                .get("description")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_owned(),
            source: source.to_owned(),
        });
    }
    result
}

pub fn tool_catalog(
    config: &ForgeConfig,
    config_path: &Path,
    settings: &AgentSettingsState,
) -> Vec<CatalogEntry> {
    let mut list = entries(
        config
            .agents
            .tools
            .iter()
            .chain(settings.draft.tools.iter())
            .cloned(),
        "Configured",
    );
    if let Ok(tools) = crate::agents::adk::configured_tool_catalog(config, config_path) {
        for (id, category, description) in tools {
            if !list.iter().any(|entry| entry.id == id) {
                list.push(CatalogEntry {
                    name: id.clone(),
                    id,
                    category,
                    description,
                    source: "Toolset".into(),
                });
            }
        }
    }
    list
}

pub fn skill_catalog(
    config: &ForgeConfig,
    config_path: &Path,
    settings: &AgentSettingsState,
) -> Vec<CatalogEntry> {
    let local = settings
        .draft
        .other
        .get("skills")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let mut list = entries(
        config.agents.skills.iter().cloned().chain(local),
        "Configured",
    );
    let project = config_path
        .parent()
        .and_then(Path::parent)
        .unwrap_or(config_path);
    for definition in &config.agents.skills {
        if definition.get("type").and_then(Value::as_str) != Some("local") {
            continue;
        }
        let Some(root) = definition.get("root").and_then(Value::as_str) else {
            continue;
        };
        let root = if Path::new(root).is_absolute() {
            Path::new(root).to_path_buf()
        } else {
            project.join(root)
        };
        if let Ok(dirs) = std::fs::read_dir(root) {
            for dir in dirs.flatten() {
                if !dir.path().join("SKILL.md").is_file() {
                    continue;
                }
                let id = dir.file_name().to_string_lossy().into_owned();
                if list.iter().any(|entry| entry.id == id) {
                    continue;
                }
                let description = std::fs::read_to_string(dir.path().join("SKILL.md"))
                    .ok()
                    .and_then(|text| {
                        text.lines()
                            .find(|line| !line.trim().is_empty() && !line.starts_with('#'))
                            .map(str::to_owned)
                    })
                    .unwrap_or_default();
                list.push(CatalogEntry {
                    id: id.clone(),
                    name: id,
                    category: "Local".into(),
                    description,
                    source: "Local".into(),
                });
            }
        }
    }
    list
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn edits_belong_to_draft_agent() {
        let agent: AgentDefinition = serde_json::from_value(json!({"id":"a","name":"A"})).unwrap();
        let mut settings = AgentSettingsState::new(&agent);
        settings.set_tool("search", "autoapprove", true);
        settings.set_memory_value("shared", "auto_store", true);
        assert_eq!(settings.agent_id, "a");
        assert!(settings.tool_value("search", "autoapprove", false));
        assert!(settings.memory_value("shared", "auto_store", false));
        assert!(settings.dirty);
    }
}
