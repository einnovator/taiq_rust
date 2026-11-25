use crate::schema::ForgeConfig;
use anyhow::{Context, Result, bail};
use serde::Deserialize;
use serde_json::{Map, Value};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

/// Load ForgeIQ's multi-document YAML config, including active imports and profiles.
pub fn load_config(path: impl AsRef<Path>, profiles: &[&str]) -> Result<ForgeConfig> {
    let source = path
        .as_ref()
        .canonicalize()
        .with_context(|| format!("config file {}", path.as_ref().display()))?;
    let root = source
        .parent()
        .and_then(Path::parent)
        .context("config must be below a project directory")?;
    let mut loader = Loader {
        profiles: profiles.iter().map(|s| (*s).to_owned()).collect(),
        loaded: HashSet::new(),
        stack: Vec::new(),
        dotenv: read_dotenv(&root.join(".env"))?,
    };
    let mut payload = Value::Object(Map::new());
    loader.load_file(&source, &mut payload)?;
    expand_env(&mut payload, &loader.dotenv)?;
    qualify_model_names(&mut payload);
    serde_json::from_value(payload)
        .with_context(|| format!("invalid ForgeIQ config from {}", source.display()))
}

/// Load with profiles from FORGEIQ_PROFILES (comma-separated).
pub fn load_config_from_env(path: impl AsRef<Path>) -> Result<ForgeConfig> {
    let profiles = std::env::var("FORGEIQ_PROFILES").unwrap_or_default();
    let names: Vec<_> = profiles
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .collect();
    load_config(path, &names)
}

struct Loader {
    profiles: HashSet<String>,
    loaded: HashSet<PathBuf>,
    stack: Vec<PathBuf>,
    dotenv: HashMap<String, String>,
}

impl Loader {
    fn load_file(&mut self, path: &Path, target: &mut Value) -> Result<()> {
        let path = path
            .canonicalize()
            .with_context(|| format!("import {}", path.display()))?;
        if self.stack.contains(&path) {
            bail!("configuration import cycle at {}", path.display());
        }
        if self.loaded.contains(&path) {
            return Ok(());
        }
        self.stack.push(path.clone());
        let result = self.load_documents(&path, target);
        self.stack.pop();
        if result.is_ok() {
            self.loaded.insert(path);
        }
        result
    }

    fn load_documents(&mut self, path: &Path, target: &mut Value) -> Result<()> {
        let source =
            fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
        for document in serde_yaml::Deserializer::from_str(&source) {
            let yaml = serde_yaml::Value::deserialize(document)
                .with_context(|| format!("parse YAML {}", path.display()))?;
            let mut value = serde_json::to_value(yaml)?;
            if !filter_profiles(&mut value, &self.profiles)? {
                continue;
            }
            let Some(object) = value.as_object_mut() else {
                bail!("{}: config document must be a mapping", path.display());
            };
            let extends = object.remove("extends");
            if let Some(parent) = extends
                .as_ref()
                .and_then(|v| v.get("path"))
                .and_then(Value::as_str)
            {
                let parent = expand_string(parent, &self.dotenv)?;
                self.load_file(&path.parent().unwrap().join(parent), target)?;
            }
            let extend_providers = extends.is_some()
                || object
                    .get_mut("auth")
                    .and_then(Value::as_object_mut)
                    .and_then(|v| v.remove("extends"))
                    .is_some();
            let imports = object
                .get("imports")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            for entry in imports {
                let Some(spec) = entry.get("import") else {
                    continue;
                };
                let profile = spec.get("profile").and_then(Value::as_str).unwrap_or("all");
                if profile != "all" && !self.profiles.contains(profile) {
                    continue;
                }
                let Some(raw_path) = spec.get("path").and_then(Value::as_str) else {
                    continue;
                };
                let imported = path
                    .parent()
                    .unwrap()
                    .join(expand_string(raw_path, &self.dotenv)?);
                if !imported.is_file() {
                    bail!(
                        "{}: imported file does not exist: {}",
                        path.display(),
                        imported.display()
                    );
                }
                // Agent definition files and other content imports are not config documents.
                if contains_config(&imported)? {
                    self.load_file(&imported, target)?;
                }
            }
            // ForgeIQ records resolved import paths in the resulting model.
            if let Some(entries) = object.get_mut("imports").and_then(Value::as_array_mut) {
                for entry in entries {
                    if let Some(spec) = entry.get_mut("import").and_then(Value::as_object_mut) {
                        if let Some(raw) = spec.get("path").and_then(Value::as_str) {
                            let resolved = path
                                .parent()
                                .unwrap()
                                .join(expand_string(raw, &self.dotenv)?);
                            spec.insert(
                                "path".into(),
                                Value::String(resolved.to_string_lossy().into_owned()),
                            );
                        }
                    }
                }
            }
            merge(target, value, &[], extend_providers);
        }
        Ok(())
    }
}

fn contains_config(path: &Path) -> Result<bool> {
    let source = fs::read_to_string(path)?;
    const KEYS: &[&str] = &[
        "ui",
        "server",
        "notifications",
        "workspace_archive_retention_days",
        "filestores",
        "auth",
        "connectors",
        "connector-plugins",
        "models",
        "prompts",
        "agents",
        "memory",
        "runtime",
        "plugins",
        "imports",
    ];
    for document in serde_yaml::Deserializer::from_str(&source) {
        let value = serde_yaml::Value::deserialize(document)?;
        if let Some(map) = value.as_mapping() {
            if map
                .keys()
                .filter_map(serde_yaml::Value::as_str)
                .any(|key| KEYS.contains(&key))
            {
                return Ok(true);
            }
        }
    }
    Ok(false)
}

fn filter_profiles(value: &mut Value, profiles: &HashSet<String>) -> Result<bool> {
    match value {
        Value::Object(map) => {
            // The profile on an import or extends spec is a selector for the import/extension.
            let selected =
                if map.contains_key("import") || map.contains_key("extends") && map.len() == 1 {
                    true
                } else {
                    let names = map
                        .get("profile")
                        .into_iter()
                        .chain(map.get("profiles"))
                        .flat_map(|v| {
                            if let Some(items) = v.as_array() {
                                items.iter().collect::<Vec<_>>()
                            } else {
                                vec![v]
                            }
                        })
                        .filter_map(Value::as_str)
                        .collect::<Vec<_>>();
                    names.is_empty() || names.iter().any(|name| profiles.contains(*name))
                };
            if !selected {
                return Ok(false);
            }
            map.remove("profile");
            map.remove("profiles");
            let keys: Vec<_> = map.keys().cloned().collect();
            for key in keys {
                let child = map.get_mut(&key).unwrap();
                if key == "import" || key == "extends" {
                    continue;
                }
                if !filter_profiles(child, profiles)? {
                    map.remove(&key);
                }
            }
        }
        Value::Array(items) => {
            let mut kept = Vec::with_capacity(items.len());
            for mut item in std::mem::take(items) {
                if filter_profiles(&mut item, profiles)? {
                    kept.push(item);
                }
            }
            *items = kept;
        }
        _ => {}
    }
    Ok(true)
}

fn merge(target: &mut Value, incoming: Value, path: &[String], extend_providers: bool) {
    match (target, incoming) {
        (Value::Object(dst), Value::Object(src)) => {
            for (key, item) in src {
                let mut child_path = path.to_vec();
                child_path.push(key.clone());
                if let Some(existing) = dst.get_mut(&key) {
                    merge(existing, item, &child_path, extend_providers);
                } else {
                    dst.insert(key, item);
                }
            }
        }
        (Value::Array(dst), Value::Array(src)) => {
            let keyed = path == ["filestores", "filestores"]
                || extend_providers && (path == ["auth", "providers"] || path == ["connectors"]);
            for item in src {
                let identity = if keyed {
                    item.get("id").or_else(|| {
                        if path == ["auth", "providers"] {
                            item.get("provider")
                        } else {
                            None
                        }
                    })
                } else {
                    None
                };
                if let Some(index) = identity.and_then(|id| {
                    dst.iter()
                        .position(|old| old.get("id").or_else(|| old.get("provider")) == Some(id))
                }) {
                    merge(&mut dst[index], item, path, extend_providers);
                } else {
                    dst.push(item);
                }
            }
        }
        (dst, src) => *dst = src,
    }
}

fn read_dotenv(path: &Path) -> Result<HashMap<String, String>> {
    if !path.is_file() {
        return Ok(HashMap::new());
    }
    Ok(dotenvy::from_path_iter(path)?
        .filter_map(Result::ok)
        .collect())
}

fn expand_env(value: &mut Value, dotenv: &HashMap<String, String>) -> Result<()> {
    match value {
        Value::String(s) => *s = expand_string(s, dotenv)?,
        Value::Array(items) => {
            for item in items {
                expand_env(item, dotenv)?;
            }
        }
        Value::Object(map) => {
            for item in map.values_mut() {
                expand_env(item, dotenv)?;
            }
        }
        _ => {}
    }
    Ok(())
}

fn expand_string(input: &str, dotenv: &HashMap<String, String>) -> Result<String> {
    let mut out = String::new();
    let mut rest = input;
    while let Some(start) = rest.find("${") {
        out.push_str(&rest[..start]);
        let after = &rest[start + 2..];
        let Some(end) = after.find('}') else {
            bail!("unterminated environment reference in {input}");
        };
        let (name, default) = after[..end]
            .split_once(':')
            .map_or((&after[..end], None), |(n, d)| (n, Some(d)));
        let value = std::env::var(name)
            .ok()
            .or_else(|| dotenv.get(name).cloned())
            .or_else(|| default.map(str::to_owned))
            .with_context(|| format!("environment variable {name} is required"))?;
        out.push_str(&value);
        rest = &after[end + 1..];
    }
    out.push_str(rest);
    Ok(out)
}

fn qualify_model_names(payload: &mut Value) {
    let Some(catalog) = payload.get_mut("models").and_then(Value::as_object_mut) else {
        return;
    };
    let providers: HashMap<String, (Option<String>, Option<String>)> = catalog
        .get("providers")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|p| {
            Some((
                p.get("name")?.as_str()?.to_owned(),
                (
                    p.get("api").and_then(Value::as_str).map(str::to_owned),
                    p.get("litellm_prefix")
                        .and_then(Value::as_str)
                        .map(str::to_owned),
                ),
            ))
        })
        .collect();
    if let Some(models) = catalog.get_mut("models").and_then(Value::as_array_mut) {
        for model in models {
            let Some(provider) = model
                .get("provider")
                .and_then(Value::as_str)
                .and_then(|name| providers.get(name))
            else {
                continue;
            };
            if provider.0.as_deref() == Some("openai") {
                continue;
            }
            if let (Some(prefix), Some(name)) =
                (&provider.1, model.get("name").and_then(Value::as_str))
            {
                if !name.starts_with(&format!("{prefix}/")) {
                    let qualified = format!("{prefix}/{name}");
                    model["name"] = Value::String(qualified);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loads_existing_forgeiq_config_in_place() {
        let path =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../orqestra/forgeiq/config/config.yaml");
        let config = load_config(&path, &[]).unwrap();
        assert_eq!(config.server.port, Some(8280));
        assert!(
            config
                .models
                .as_ref()
                .is_some_and(|m| !m.providers.is_empty() && !m.models.is_empty())
        );
        assert!(!config.connectors.is_empty());
        assert!(!config.runtime.runtime.is_empty());
        assert!(!config.memory.memory.is_empty());
        assert!(!config.agents.agents.is_empty());
        assert!(config.filestores.is_some());
    }

    #[test]
    fn local_dev_profile_merges_overlay() {
        let path =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../orqestra/forgeiq/config/config.yaml");
        let config = load_config(path, &["local-dev"]).unwrap();
        assert!(
            config
                .auth
                .unwrap()
                .providers
                .iter()
                .any(|p| p.id.as_deref() == Some("local-dev"))
        );
    }
}
