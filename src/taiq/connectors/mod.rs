use crate::filestore::{FileEntry, FileStore};
use crate::filestore::{LocalFileStore, MemoryFileStore, RemoteFileStore};
pub mod remote;
use crate::schema::ForgeConfig;
use anyhow::{Context, Result, bail};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::{Mutex, OnceLock};

pub trait Connector: Send + Sync {
    fn id(&self) -> &str;
    fn list(&self, path: &str) -> Result<Vec<FileEntry>>;
    fn read_text(&self, path: &str) -> Result<String>;
}

pub struct LocalConnector {
    store: Arc<dyn FileStore>,
}

#[derive(Debug, Clone)]
pub struct LocalStoreDefinition {
    pub id: String,
    pub name: String,
    pub root: PathBuf,
    pub read_only: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoreDefinition {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub enabled: bool,
    pub read_only: bool,
}

/// Full configured catalog, including stores unavailable to this build.
pub fn store_catalog(config: &ForgeConfig) -> Vec<StoreDefinition> {
    config
        .filestores
        .as_ref()
        .and_then(|stores| stores.stores.get("filestores"))
        .and_then(|entries| entries.as_array())
        .into_iter()
        .flatten()
        .filter_map(|entry| {
            let id = entry.get("id")?.as_str()?.to_owned();
            Some(StoreDefinition {
                name: entry
                    .get("name")
                    .and_then(|v| v.as_str())
                    .unwrap_or(&id)
                    .to_owned(),
                kind: entry
                    .get("type")
                    .and_then(|v| v.as_str())
                    .unwrap_or("local")
                    .to_owned(),
                enabled: entry.get("enabled").and_then(|v| v.as_bool()) != Some(false),
                read_only: entry
                    .get("read-only")
                    .or_else(|| entry.get("read_only"))
                    .and_then(|v| v.as_bool())
                    == Some(true),
                id,
            })
        })
        .collect()
}

/// Open the configured default workspace for headless agent tools.
pub fn default_store(config: &ForgeConfig, config_path: &Path) -> Result<Arc<dyn FileStore>> {
    let default = config
        .filestores
        .as_ref()
        .and_then(|stores| stores.default.as_deref())
        .unwrap_or("local");
    if let Some(local) = local_store_roots(config, config_path)?
        .into_iter()
        .find(|store| store.id == default)
    {
        return Ok(Arc::new(LocalFileStore::new_with_mode(
            local.root,
            local.read_only,
        )?));
    }
    let entry = config
        .filestores
        .as_ref()
        .and_then(|stores| stores.stores.get("filestores"))
        .and_then(serde_json::Value::as_array)
        .and_then(|entries| {
            entries
                .iter()
                .find(|entry| entry.get("id").and_then(serde_json::Value::as_str) == Some(default))
        })
        .with_context(|| format!("default filestore '{default}' is not configured"))?;
    if entry.get("enabled").and_then(serde_json::Value::as_bool) == Some(false) {
        bail!("default filestore '{default}' is disabled");
    }
    if entry.get("type").and_then(serde_json::Value::as_str) == Some("memory") {
        static STORES: OnceLock<Mutex<std::collections::HashMap<String, Arc<MemoryFileStore>>>> =
            OnceLock::new();
        let key = format!("{}:{default}", config_path.display());
        return Ok(STORES
            .get_or_init(Default::default)
            .lock()
            .unwrap()
            .entry(key)
            .or_insert_with(|| Arc::new(MemoryFileStore::new()))
            .clone());
    }
    Ok(Arc::new(RemoteFileStore::from_config(entry, config_path)?))
}

pub fn local_store_roots(
    config: &ForgeConfig,
    config_path: &Path,
) -> Result<Vec<LocalStoreDefinition>> {
    let project = config_path
        .parent()
        .and_then(Path::parent)
        .context("config must be under project/config")?;
    let stores = config
        .filestores
        .as_ref()
        .context("filestores are not configured")?;
    let mut definitions = Vec::new();
    if let Some(entries) = stores.stores.get("filestores").and_then(|v| v.as_array()) {
        for entry in entries {
            if entry.get("type").and_then(|v| v.as_str()) != Some("local")
                || entry.get("enabled").and_then(|v| v.as_bool()) == Some(false)
            {
                continue;
            }
            let Some(id) = entry.get("id").and_then(|v| v.as_str()) else {
                continue;
            };
            let root = entry
                .get("root")
                .and_then(|v| v.as_str())
                .unwrap_or("data/artifacts");
            let root = Path::new(root);
            definitions.push(LocalStoreDefinition {
                id: id.to_owned(),
                name: entry
                    .get("name")
                    .and_then(|v| v.as_str())
                    .unwrap_or(id)
                    .to_owned(),
                root: if root.is_absolute() {
                    root.to_path_buf()
                } else {
                    project.join(root)
                },
                read_only: entry
                    .get("read-only")
                    .or_else(|| entry.get("read_only"))
                    .and_then(|v| v.as_bool())
                    == Some(true),
            });
        }
    }
    if definitions.is_empty() {
        if let Some(local) = stores.stores.get("local") {
            let root = local
                .get("root")
                .and_then(|v| v.as_str())
                .unwrap_or("data/artifacts");
            let root = Path::new(root);
            definitions.push(LocalStoreDefinition {
                id: "local".into(),
                name: "Local".into(),
                root: if root.is_absolute() {
                    root.to_path_buf()
                } else {
                    project.join(root)
                },
                read_only: false,
            });
        }
    }
    Ok(definitions)
}

impl LocalConnector {
    pub fn new(store: Arc<dyn FileStore>) -> Self {
        Self { store }
    }
}

impl Connector for LocalConnector {
    fn id(&self) -> &str {
        "local"
    }
    fn list(&self, path: &str) -> Result<Vec<FileEntry>> {
        self.store.list(path)
    }
    fn read_text(&self, path: &str) -> Result<String> {
        self.store.read_text(path)
    }
}

pub fn local_store_root(config: &ForgeConfig, config_path: &Path) -> Result<PathBuf> {
    let project = config_path
        .parent()
        .and_then(Path::parent)
        .context("config must be under project/config")?;
    let stores = config
        .filestores
        .as_ref()
        .context("filestores are not configured")?;
    let local = stores
        .stores
        .get("local")
        .or_else(|| {
            stores
                .stores
                .get("filestores")
                .and_then(|v| v.as_array())
                .and_then(|items| {
                    items
                        .iter()
                        .find(|v| v.get("id").and_then(|v| v.as_str()) == Some("local"))
                })
        })
        .context("local filestore is not configured")?;
    if local.get("enabled").and_then(|v| v.as_bool()) == Some(false) {
        bail!("local filestore is disabled");
    }
    let root = local
        .get("root")
        .and_then(|v| v.as_str())
        .unwrap_or("data/artifacts");
    let root_path = Path::new(root);
    Ok(if root_path.is_absolute() {
        root_path.to_path_buf()
    } else {
        project.join(root_path)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::load_config;

    #[test]
    fn finds_existing_forgeiq_local_store() {
        let path =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../orqestra/forgeiq/config/config.yaml");
        let config = load_config(&path, &[]).unwrap();
        assert_eq!(
            local_store_root(&config, &path)
                .unwrap()
                .file_name()
                .unwrap(),
            "artifacts"
        );
        let stores = local_store_roots(&config, &path).unwrap();
        assert!(
            stores
                .iter()
                .any(|store| store.id == "local" && !store.read_only)
        );
        assert!(
            stores
                .iter()
                .any(|store| store.id == "samples" && store.read_only)
        );
        let catalog = store_catalog(&config);
        assert!(
            catalog
                .iter()
                .any(|store| store.id == "aws" && store.kind == "s3")
        );
    }
}
