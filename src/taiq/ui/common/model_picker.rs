use crate::schema::{ForgeConfig, LlmModelConfig};
use serde_json::Value;
use std::path::{Component, Path, PathBuf};

#[derive(Debug, Clone)]
pub struct ModelChoice {
    pub id: String,
    pub name: String,
    pub provider: String,
    pub capabilities: Vec<String>,
    pub reasoning_levels: Vec<String>,
    pub reasoning_default: Option<String>,
}

impl ModelChoice {
    fn from_config(model: &LlmModelConfig) -> Self {
        let strings = |key: &str| {
            model
                .other
                .get(key)
                .and_then(Value::as_array)
                .map(|items| {
                    items
                        .iter()
                        .filter_map(Value::as_str)
                        .map(str::to_owned)
                        .collect()
                })
                .unwrap_or_default()
        };
        Self {
            id: model.id.clone().unwrap_or_else(|| model.name.clone()),
            name: model.name.clone(),
            provider: model.provider.clone(),
            capabilities: if model.other.contains_key("capabilities") {
                strings("capabilities")
            } else {
                strings("media")
            },
            reasoning_levels: strings("reasoning_levels"),
            reasoning_default: model
                .other
                .get("reasoning_default")
                .and_then(Value::as_str)
                .map(str::to_owned),
        }
    }
}

pub fn choices(config: &ForgeConfig) -> Vec<ModelChoice> {
    config
        .models
        .as_ref()
        .map(|models| models.models.iter().map(ModelChoice::from_config).collect())
        .unwrap_or_default()
}

pub fn capability_icon(capability: &str) -> &'static [u8] {
    match capability {
        "vision" | "image" | "images" => include_bytes!("icons/image.svg"),
        "audio" | "speech" => include_bytes!("icons/audio.svg"),
        "text" | "chat" | "code" => include_bytes!("icons/text.svg"),
        _ => include_bytes!("icons/file.svg"),
    }
}

pub fn effective<'a>(
    config: &'a ForgeConfig,
    agent_id: &str,
    selected: &str,
) -> Option<&'a LlmModelConfig> {
    let models = &config.models.as_ref()?.models;
    if selected != "auto" {
        return models
            .iter()
            .find(|model| model.id.as_deref() == Some(selected) || model.name == selected);
    }
    let agent = config
        .agents
        .agents
        .iter()
        .find(|agent| agent.id == agent_id)?;
    agent
        .models
        .iter()
        .find_map(|choice| {
            models.iter().find(|model| {
                model.name == choice.name
                    && choice
                        .provider
                        .as_deref()
                        .is_none_or(|provider| provider == model.provider)
            })
        })
        .or_else(|| models.iter().find(|model| model.default == Some(true)))
        .or_else(|| models.first())
}

pub fn reasoning_levels(config: &ForgeConfig, agent_id: &str, selected: &str) -> Vec<String> {
    let Some(model) = effective(config, agent_id, selected) else {
        return Vec::new();
    };
    if model.provider != "openai" {
        return Vec::new();
    }
    ModelChoice::from_config(model).reasoning_levels
}

pub fn provider_icon_path(
    config: &ForgeConfig,
    config_path: &Path,
    provider: &str,
) -> Option<PathBuf> {
    let icon = config
        .models
        .as_ref()?
        .providers
        .iter()
        .find(|item| item.name == provider)?
        .other
        .get("icon")?
        .as_str()?
        .strip_prefix("/static/")?;
    if !Path::new(icon)
        .components()
        .all(|component| matches!(component, Component::Normal(_)))
    {
        return None;
    }
    let project = config_path.parent()?.parent()?;
    let path = project.join("src/static").join(icon);
    path.is_file().then_some(path)
}
