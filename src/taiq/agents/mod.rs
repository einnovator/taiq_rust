pub mod adk;

use crate::schema::{AgentDefinition, ForgeConfig};
use anyhow::{Context, Result, bail};
use std::fs;
use std::path::{Path, PathBuf};

pub fn find_agent<'a>(config: &'a ForgeConfig, name: &str) -> Result<&'a AgentDefinition> {
    config
        .agents
        .agents
        .iter()
        .find(|agent| {
            agent.enabled != Some(false)
                && (agent.id.eq_ignore_ascii_case(name) || agent.name.eq_ignore_ascii_case(name))
        })
        .with_context(|| format!("agent '{name}' not found; use --list to see available agents"))
}

pub fn instructions(
    agent: &AgentDefinition,
    config_path: &Path,
    config: &ForgeConfig,
) -> Result<String> {
    let root = config_path.parent().context("config path has no parent")?;
    let mut text = if let Some(inline) = &agent.instructions {
        inline.clone()
    } else if let Some(file) = &agent.instructions_file {
        let path = resolve_instruction(root, file)?;
        fs::read_to_string(&path).with_context(|| {
            format!(
                "read instructions for {} from {}",
                agent.name,
                path.display()
            )
        })?
    } else {
        "You are a helpful assistant.".to_owned()
    };
    let reasoning = agent
        .reasoning_file
        .as_deref()
        .or(config.agents.reasoning_file.as_deref());
    if let Some(file) = reasoning.filter(|s| !s.is_empty()) {
        let path = resolve_instruction(root, file)?;
        text.push_str("\n\n");
        text.push_str(
            &fs::read_to_string(&path)
                .with_context(|| format!("read reasoning file {}", path.display()))?,
        );
    }
    Ok(text)
}

fn resolve_instruction(root: &Path, file: &str) -> Result<PathBuf> {
    let candidate = Path::new(file);
    if candidate.is_absolute() {
        return Ok(candidate.to_path_buf());
    }
    let options = [
        root.join(file),
        root.join("agents").join(file),
        root.join("agents").join("instructions").join(file),
    ];
    options.into_iter().find(|p| p.is_file()).with_context(|| {
        format!(
            "instruction file '{file}' not found under {}",
            root.display()
        )
    })
}

pub fn selected_model<'a>(
    agent: &'a AgentDefinition,
    config: &'a ForgeConfig,
) -> Result<(&'a str, &'a str)> {
    if let Some(model) = agent.models.first() {
        if model.name != "auto" {
            return Ok((&model.name, model.provider.as_deref().unwrap_or("openai")));
        }
    }
    let models = config
        .models
        .as_ref()
        .context("config has no models section")?;
    let selected = models
        .models
        .iter()
        .find(|m| {
            m.default == Some(true)
                && m.other.get("enabled").and_then(|v| v.as_bool()) != Some(false)
        })
        .or_else(|| {
            models
                .models
                .iter()
                .find(|m| m.other.get("enabled").and_then(|v| v.as_bool()) != Some(false))
        })
        .context("config has no enabled model")?;
    if selected.name.is_empty() {
        bail!("selected model has no name");
    }
    Ok((&selected.name, &selected.provider))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::load_config;

    #[test]
    fn resolves_named_agent_and_instructions() {
        let path =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../orqestra/forgeiq/config/config.yaml");
        let config = load_config(&path, &[]).unwrap();
        let agent = find_agent(&config, "IQ").unwrap();
        assert_eq!(agent.id, "iq_general_agent");
        assert!(!instructions(agent, &path, &config).unwrap().is_empty());
        assert!(!selected_model(agent, &config).unwrap().0.is_empty());
    }
}
