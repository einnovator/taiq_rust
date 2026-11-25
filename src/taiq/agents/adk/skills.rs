use crate::schema::{AgentDefinition, ForgeConfig};
use adk_rust::{AdkError, Tool};
use adk_tool::FunctionTool;
use anyhow::Result;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
struct SkillArgs {
    name: String,
}

fn local_roots(config: &ForgeConfig, config_path: &Path) -> Vec<PathBuf> {
    let project = config_path
        .parent()
        .and_then(Path::parent)
        .unwrap_or(config_path);
    config
        .agents
        .skills
        .iter()
        .filter(|entry| {
            entry.get("type").and_then(|v| v.as_str()) == Some("local")
                && entry.get("enabled").and_then(|v| v.as_bool()) != Some(false)
        })
        .filter_map(|entry| entry.get("root").and_then(|v| v.as_str()))
        .map(|root| {
            let root = Path::new(root);
            if root.is_absolute() {
                root.to_path_buf()
            } else {
                project.join(root)
            }
        })
        .collect()
}

pub fn local_skills(
    agent: &AgentDefinition,
    config: &ForgeConfig,
    config_path: &Path,
) -> Result<(String, Option<Arc<dyn Tool>>)> {
    let mut files = BTreeMap::new();
    for root in local_roots(config, config_path) {
        if !root.is_dir() {
            continue;
        }
        for entry in fs::read_dir(root)? {
            let entry = entry?;
            if !entry.file_type()?.is_dir() {
                continue;
            }
            let path = entry.path().join("SKILL.md");
            if path.is_file() {
                let name = entry.file_name().to_string_lossy().into_owned();
                let enabled = agent
                    .other
                    .get("skills")
                    .and_then(|value| value.as_array())
                    .and_then(|items| {
                        items
                            .iter()
                            .find(|item| item.get("id").and_then(|id| id.as_str()) == Some(&name))
                    })
                    .and_then(|item| item.get("enabled"))
                    .and_then(|value| value.as_bool())
                    .unwrap_or(true);
                if enabled {
                    files.insert(name, path);
                }
            }
        }
    }
    if files.is_empty() {
        return Ok((String::new(), None));
    }
    let summary = format!(
        "\n\nLocal skills available: {}. Use the read_skill tool to inspect a skill before following it.",
        files.keys().cloned().collect::<Vec<_>>().join(", ")
    );
    let tool = FunctionTool::new(
        "read_skill",
        "Read a configured local skill by name",
        move |_, args| {
            let files = files.clone();
            async move {
                let args: SkillArgs =
                    serde_json::from_value(args).map_err(|e| AdkError::tool(e.to_string()))?;
                let path = files
                    .get(&args.name)
                    .ok_or_else(|| AdkError::tool(format!("unknown skill: {}", args.name)))?;
                let content =
                    fs::read_to_string(path).map_err(|e| AdkError::tool(e.to_string()))?;
                Ok(json!({"name": args.name, "content": content}))
            }
        },
    )
    .with_parameters_schema::<SkillArgs>()
    .with_read_only(true);
    Ok((summary, Some(Arc::new(tool))))
}
