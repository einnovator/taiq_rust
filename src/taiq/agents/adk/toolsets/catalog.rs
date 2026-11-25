use crate::schema::ForgeConfig;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ToolDeclaration {
    pub name: String,
    pub function: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(skip)]
    pub toolset: String,
}

#[derive(Debug, Deserialize)]
struct Manifest {
    name: String,
    #[serde(default)]
    tools: Vec<ToolDeclaration>,
}

#[derive(Debug, Clone, Default)]
pub struct ToolCatalog {
    pub declarations: Vec<ToolDeclaration>,
}

impl ToolCatalog {
    pub fn load(config: &ForgeConfig, config_path: &Path) -> Result<Self> {
        let project = config_path
            .parent()
            .and_then(Path::parent)
            .context("config path must be under project/config")?;
        let options = config.agents.toolsets.as_ref();
        let root = options
            .and_then(|value| value.get("root"))
            .and_then(Value::as_str)
            .unwrap_or("plugins/toolsets");
        let path = if Path::new(root).is_absolute() {
            PathBuf::from(root)
        } else {
            project.join(root)
        };
        if !path.exists() {
            return Ok(Self::default());
        }
        let include = options.and_then(|value| value.get("include"));
        let permitted = |name: &str| match include {
            Some(Value::String(s)) if s != "*" => s.split(',').any(|part| part.trim() == name),
            Some(Value::Array(items)) => items.iter().any(|item| item.as_str() == Some(name)),
            _ => true,
        };
        let mut declarations = Vec::new();
        for entry in std::fs::read_dir(&path)? {
            let entry = entry?;
            if !entry.file_type()?.is_dir() {
                continue;
            }
            let manifest_path = entry.path().join("manifest.yaml");
            if !manifest_path.is_file() {
                continue;
            }
            let manifest: Manifest = serde_yaml::from_slice(&std::fs::read(&manifest_path)?)
                .with_context(|| format!("parse {}", manifest_path.display()))?;
            if !permitted(&manifest.name) {
                continue;
            }
            for mut tool in manifest.tools {
                tool.toolset = manifest.name.clone();
                declarations.push(tool);
            }
        }
        declarations.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(Self { declarations })
    }

    pub fn find(&self, name: &str) -> Option<&ToolDeclaration> {
        self.declarations.iter().find(|tool| tool.name == name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn reads_forgeiq_manifests_in_place() {
        let path =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../orqestra/forgeiq/config/config.yaml");
        if !path.exists() {
            return;
        }
        let config = crate::config::load_config_from_env(&path).unwrap();
        let catalog = ToolCatalog::load(&config, &path).unwrap();
        assert_eq!(catalog.find("glob").unwrap().toolset, "search");
        assert_eq!(catalog.find("artifact_create").unwrap().toolset, "artifact");
    }
}
