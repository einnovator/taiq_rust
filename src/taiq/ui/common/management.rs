use crate::schema::{AgentDefinition, ConnectorDefinition, ForgeConfig, ModelsConfig};
use anyhow::{Result, bail};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use std::fs;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AdminSection {
    Agents,
    Connectors,
    Filestores,
    Models,
    Credentials,
    Knowledge,
    Extractors,
    Workflows,
    Schedules,
    Monitoring,
    Evaluations,
}

impl AdminSection {
    pub const ALL: [Self; 11] = [
        Self::Agents,
        Self::Connectors,
        Self::Credentials,
        Self::Models,
        Self::Filestores,
        Self::Knowledge,
        Self::Extractors,
        Self::Workflows,
        Self::Schedules,
        Self::Monitoring,
        Self::Evaluations,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Self::Agents => "Agents",
            Self::Connectors => "Connectors",
            Self::Filestores => "Filestores",
            Self::Models => "Models",
            Self::Credentials => "Credentials",
            Self::Knowledge => "Knowledge",
            Self::Extractors => "Extractors",
            Self::Workflows => "Workflows",
            Self::Schedules => "Schedules",
            Self::Monitoring => "Monitoring",
            Self::Evaluations => "Evaluations",
        }
    }
    pub fn enabled_help(self) -> &'static str {
        match self {
            Self::Agents => "Allow this agent to be selected and run from the desktop app.",
            Self::Models => "Make this model available to agent and chat model selectors.",
            Self::Connectors => "Allow this connector and its configured tools to be used.",
            Self::Filestores => "Show this configured store in the Files workspace.",
            _ => "Enable this resource in the desktop configuration.",
        }
    }
    fn editable(self) -> bool {
        !matches!(self, Self::Credentials | Self::Monitoring)
    }
}

#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(default)]
struct Overrides {
    agents: Option<Vec<AgentDefinition>>,
    connectors: Option<Vec<ConnectorDefinition>>,
    filestores: Option<crate::schema::FileStoresConfig>,
    models: Option<ModelsConfig>,
    records: BTreeMap<String, Value>,
}

pub struct ManagementStore {
    path: PathBuf,
    config_path: PathBuf,
    overrides: Overrides,
}

const UNCHANGED_SECRET: &str = "<unchanged>";
const STORE_SECRET_KEYS: &[&str] = &[
    "access_key",
    "access_key_id",
    "secret_key",
    "secret_access_key",
    "account_key",
    "credential",
    "password",
];

fn store_entries(stores: &mut crate::schema::FileStoresConfig) -> Option<&mut Vec<Value>> {
    stores.stores.get_mut("filestores")?.as_array_mut()
}

fn for_store_secrets(entry: &mut Value, mut action: impl FnMut(&str, &mut Value)) {
    for container in ["", "options"] {
        let object = if container.is_empty() {
            entry.as_object_mut()
        } else {
            entry.get_mut(container).and_then(Value::as_object_mut)
        };
        if let Some(object) = object {
            for key in STORE_SECRET_KEYS {
                if let Some(value) = object.get_mut(*key) {
                    action(key, value);
                }
            }
        }
    }
}

impl ManagementStore {
    pub fn open(config_path: &Path, config: &mut ForgeConfig) -> Result<Self> {
        let base = std::env::var_os("LOCALAPPDATA")
            .or_else(|| std::env::var_os("XDG_DATA_HOME"))
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|v| PathBuf::from(v).join(".local/share")))
            .unwrap_or_else(std::env::temp_dir)
            .join("taiq");
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        config_path.hash(&mut hasher);
        let path = base.join(format!("{:016x}.management.json", hasher.finish()));
        let overrides: Overrides = if path.exists() {
            let data = fs::read(&path)?;
            match serde_json::from_slice(&data) {
                Ok(overrides) => overrides,
                Err(error) => {
                    let backup = path.with_extension(format!(
                        "corrupt-{}.json",
                        SystemTime::now()
                            .duration_since(UNIX_EPOCH)
                            .unwrap_or_default()
                            .as_secs()
                    ));
                    fs::rename(&path, &backup)?;
                    eprintln!(
                        "Damaged desktop management state ({error}) moved to {}",
                        backup.display()
                    );
                    Overrides::default()
                }
            }
        } else {
            Overrides::default()
        };
        if let Some(agents) = &overrides.agents {
            config.agents.agents = agents.clone();
        }
        if let Some(connectors) = &overrides.connectors {
            let mut merged = connectors.clone();
            for connector in &mut merged {
                if let Some(original) = config.connectors.iter().find(|c| c.id == connector.id) {
                    connector.credentials = original.credentials.clone();
                }
            }
            config.connectors = merged;
        }
        if let Some(filestores) = &overrides.filestores {
            let mut merged = filestores.clone();
            if let (Some(entries), Some(original)) = (
                store_entries(&mut merged),
                config.filestores.as_mut().and_then(store_entries),
            ) {
                for entry in entries {
                    let id = entry.get("id").and_then(Value::as_str);
                    if let Some(old) = original
                        .iter()
                        .find(|old| old.get("id").and_then(Value::as_str) == id)
                    {
                        let mut old = old.clone();
                        let mut secrets = BTreeMap::new();
                        for_store_secrets(&mut old, |key, value| {
                            secrets.insert(key.to_owned(), value.clone());
                        });
                        for_store_secrets(entry, |key, value| {
                            if let Some(secret) = secrets.get(key) {
                                *value = secret.clone();
                            }
                        });
                        if let Some(options) =
                            entry.get_mut("options").and_then(Value::as_object_mut)
                        {
                            for (key, value) in secrets {
                                if STORE_SECRET_KEYS.contains(&key.as_str()) {
                                    options.entry(key).or_insert(value);
                                }
                            }
                        }
                    }
                }
            }
            config.filestores = Some(merged);
        }
        if let Some(models) = &overrides.models {
            let mut merged = models.clone();
            if let Some(original) = &config.models {
                merged.engine.token = original.engine.token.clone();
                for provider in &mut merged.providers {
                    provider.token = original
                        .providers
                        .iter()
                        .find(|p| p.name == provider.name)
                        .and_then(|p| p.token.clone());
                }
            }
            config.models = Some(merged);
        }
        Ok(Self {
            path,
            config_path: config_path.to_path_buf(),
            overrides,
        })
    }

    pub fn content(
        &self,
        section: AdminSection,
        config: &ForgeConfig,
        session_count: usize,
    ) -> Result<String> {
        Ok(match section {
            AdminSection::Agents => serde_yaml::to_string(&config.agents.agents)?,
            AdminSection::Filestores => {
                let mut stores = config.filestores.clone();
                if let Some(entries) = stores.as_mut().and_then(store_entries) {
                    for entry in entries {
                        for_store_secrets(entry, |_, value| {
                            if !value.is_null() {
                                *value = Value::String(UNCHANGED_SECRET.into());
                            }
                        });
                    }
                }
                serde_yaml::to_string(&stores)?
            }
            AdminSection::Connectors => {
                let mut connectors = config.connectors.clone();
                for connector in &mut connectors {
                    for value in connector.credentials.values_mut() {
                        *value = Value::String(UNCHANGED_SECRET.into());
                    }
                }
                serde_yaml::to_string(&connectors)?
            }
            AdminSection::Models => {
                let mut models = config.models.clone();
                if let Some(models) = &mut models {
                    if models.engine.token.is_some() {
                        models.engine.token = Some(UNCHANGED_SECRET.into());
                    }
                    for provider in &mut models.providers {
                        if provider.token.is_some() {
                            provider.token = Some(UNCHANGED_SECRET.into());
                        }
                    }
                }
                serde_yaml::to_string(&models)?
            }
            AdminSection::Credentials => {
                let providers = config
                    .models
                    .as_ref()
                    .map(|m| {
                        m.providers
                            .iter()
                            .map(|p| {
                                format!(
                                    "{}: {}",
                                    p.name,
                                    if p.token.as_ref().is_some_and(|v| !v.is_empty()) {
                                        "configured"
                                    } else {
                                        "environment / missing"
                                    }
                                )
                            })
                            .collect::<Vec<_>>()
                            .join("\n")
                    })
                    .unwrap_or_default();
                format!(
                    "Credentials are read from environment variables or the ForgeIQ .env file.\nSecret values are hidden.\n\n{providers}"
                )
            }
            AdminSection::Monitoring => format!(
                "Desktop sessions: {session_count}\nConfigured agents: {}\nConfigured connectors: {}\n",
                config.agents.agents.len(),
                config.connectors.len()
            ),
            other => serde_yaml::to_string(
                self.overrides
                    .records
                    .get(other.label())
                    .unwrap_or(&Value::Array(vec![])),
            )?,
        })
    }

    pub fn save(
        &mut self,
        section: AdminSection,
        draft: &str,
        config: &mut ForgeConfig,
    ) -> Result<()> {
        if !section.editable() {
            bail!("{} is read-only", section.label());
        }
        match section {
            AdminSection::Agents => {
                let agents: Vec<AgentDefinition> = serde_yaml::from_str(draft)?;
                if agents.iter().any(|a| a.id.trim().is_empty()) {
                    bail!("agent ID is required");
                }
                self.overrides.agents = Some(agents.clone());
                config.agents.agents = agents;
            }
            AdminSection::Filestores => {
                let mut stores: crate::schema::FileStoresConfig = serde_yaml::from_str(draft)?;
                let catalog = crate::connectors::store_catalog(&ForgeConfig {
                    filestores: Some(stores.clone()),
                    ..ForgeConfig::default()
                });
                let mut ids = std::collections::HashSet::new();
                if catalog.iter().any(|item| !ids.insert(item.id.clone())) {
                    bail!("filestore IDs must be unique");
                }
                if let Some(default) = &stores.default {
                    if !catalog
                        .iter()
                        .any(|item| item.id == *default && item.enabled)
                    {
                        bail!("default filestore must be enabled and present");
                    }
                }
                let mut persisted = stores.clone();
                let mut vault = crate::credentials::CredentialStore::open(&self.config_path)?;
                if let Some(entries) = store_entries(&mut stores) {
                    for entry in entries {
                        let id = entry
                            .get("id")
                            .and_then(Value::as_str)
                            .unwrap_or("")
                            .to_owned();
                        let kind = entry
                            .get("type")
                            .and_then(Value::as_str)
                            .unwrap_or("filestore")
                            .to_owned();
                        let old = config
                            .filestores
                            .as_ref()
                            .and_then(|s| s.stores.get("filestores"))
                            .and_then(Value::as_array)
                            .and_then(|items| {
                                items.iter().find(|item| {
                                    item.get("id").and_then(Value::as_str) == Some(&id)
                                })
                            })
                            .cloned();
                        let mut old_secrets = BTreeMap::new();
                        if let Some(mut old) = old {
                            for_store_secrets(&mut old, |key, value| {
                                old_secrets.insert(key.to_owned(), value.clone());
                            });
                        }
                        let mut fresh = serde_json::Map::new();
                        for_store_secrets(entry, |key, value| {
                            if value.as_str() == Some(UNCHANGED_SECRET) {
                                *value = old_secrets.get(key).cloned().unwrap_or(Value::Null);
                            } else if !value.is_null() {
                                fresh.insert(key.to_owned(), value.clone());
                            }
                        });
                        if !fresh.is_empty() {
                            let credential_id = format!("filestore:{id}");
                            let mut values = vault
                                .get(&credential_id)
                                .ok()
                                .and_then(|v| v.as_object().cloned())
                                .unwrap_or_default();
                            values.extend(fresh);
                            vault.save(crate::credentials::CredentialDraft {
                                meta: crate::credentials::CredentialMeta {
                                    id: credential_id,
                                    name: id.clone(),
                                    provider: kind,
                                    kind: "filestore".into(),
                                },
                                values: Value::Object(values),
                            })?;
                        }
                    }
                }
                if let Some(entries) = store_entries(&mut persisted) {
                    for entry in entries {
                        for_store_secrets(entry, |_, value| *value = Value::Null);
                    }
                }
                self.overrides.filestores = Some(persisted);
                config.filestores = Some(stores);
            }
            AdminSection::Connectors => {
                let mut connectors: Vec<ConnectorDefinition> = serde_yaml::from_str(draft)?;
                for connector in &mut connectors {
                    if let Some(old) = config.connectors.iter().find(|c| c.id == connector.id) {
                        for (key, value) in &mut connector.credentials {
                            if value.as_str() == Some(UNCHANGED_SECRET) {
                                if let Some(original) = old.credentials.get(key) {
                                    *value = original.clone();
                                }
                            }
                        }
                    }
                }
                let mut persisted = connectors.clone();
                for connector in &mut persisted {
                    connector.credentials.clear();
                }
                self.overrides.connectors = Some(persisted);
                config.connectors = connectors;
            }
            AdminSection::Models => {
                let mut models: Option<ModelsConfig> = serde_yaml::from_str(draft)?;
                if let (Some(models), Some(old)) = (&mut models, &config.models) {
                    if models.engine.token.as_deref() == Some(UNCHANGED_SECRET) {
                        models.engine.token = old.engine.token.clone();
                    }
                    for provider in &mut models.providers {
                        if provider.token.as_deref() == Some(UNCHANGED_SECRET) {
                            provider.token = old
                                .providers
                                .iter()
                                .find(|p| p.name == provider.name)
                                .and_then(|p| p.token.clone());
                        }
                    }
                }
                let mut persisted = models.clone();
                if let Some(persisted) = &mut persisted {
                    persisted.engine.token = None;
                    for provider in &mut persisted.providers {
                        provider.token = None;
                    }
                }
                self.overrides.models = persisted;
                config.models = models;
            }
            other => {
                let records: Value = serde_yaml::from_str(draft)?;
                if !records.is_array() {
                    bail!("{} must be a YAML list", other.label());
                }
                self.overrides.records.insert(other.label().into(), records);
            }
        }
        fs::create_dir_all(self.path.parent().expect("management path has parent"))?;
        fs::write(&self.path, serde_json::to_vec_pretty(&self.overrides)?)?;
        Ok(())
    }
}
