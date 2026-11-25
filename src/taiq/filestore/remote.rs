use super::{FileEntry, FileStore};
use anyhow::{Context, Result, bail};
use opendal::{Operator, services};
use serde_json::Value;
use std::future::Future;
use std::path::Path;
use std::sync::LazyLock;

static RUNTIME: LazyLock<tokio::runtime::Runtime> = LazyLock::new(|| {
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("remote filestore runtime")
});

/// Cloud and WebDAV store backed by OpenDAL. The UI and agent toolsets share FileStore.
pub struct RemoteFileStore {
    operator: Operator,
    read_only: bool,
}

impl RemoteFileStore {
    pub fn from_config(config: &Value, config_path: &Path) -> Result<Self> {
        let kind = config
            .get("type")
            .and_then(Value::as_str)
            .context("filestore type is missing")?;
        let options = config.get("options").unwrap_or(config);
        let vault_values = config.get("id").and_then(Value::as_str).and_then(|id| {
            crate::credentials::CredentialStore::open(config_path)
                .ok()?
                .get(&format!("filestore:{id}"))
                .ok()
        });
        let value = |key: &str| {
            vault_values
                .as_ref()
                .and_then(|values| values.get(key))
                .or_else(|| options.get(key))
                .or_else(|| config.get(key))
                .and_then(Value::as_str)
        };
        let root = value("root").or_else(|| value("prefix")).unwrap_or("");
        let operator = match kind {
            "s3" | "seaweedfs" | "juicefs" => {
                let bucket = value("bucket").context("S3 bucket is missing")?;
                let mut builder = services::S3::default().bucket(bucket).root(root);
                if let Some(v) = value("endpoint") {
                    builder = builder.endpoint(v);
                }
                if let Some(v) = value("region") {
                    builder = builder.region(v);
                }
                if let Some(v) = value("access_key").or_else(|| value("access_key_id")) {
                    builder = builder.access_key_id(v);
                }
                if let Some(v) = value("secret_key").or_else(|| value("secret_access_key")) {
                    builder = builder.secret_access_key(v);
                }
                Operator::new(builder)?
            }
            "gcs" => {
                let bucket = value("bucket").context("GCS bucket is missing")?;
                let mut builder = services::Gcs::default().bucket(bucket).root(root);
                if let Some(v) = value("credential") {
                    builder = builder.credential(v);
                }
                if let Some(v) = value("credential_path") {
                    builder = builder.credential_path(v);
                }
                Operator::new(builder)?
            }
            "azure" | "azblob" => {
                let container = value("container").context("Azure container is missing")?;
                let endpoint = value("endpoint").context("Azure endpoint is missing")?;
                let mut builder = services::Azblob::default()
                    .container(container)
                    .endpoint(endpoint)
                    .root(root);
                if let Some(v) = value("account_name") {
                    builder = builder.account_name(v);
                }
                if let Some(v) = value("account_key") {
                    builder = builder.account_key(v);
                }
                Operator::new(builder)?
            }
            "webdav" | "rclone" if value("protocol") == Some("webdav") || kind == "webdav" => {
                let endpoint = value("url")
                    .or_else(|| value("endpoint"))
                    .context("WebDAV URL is missing")?;
                let mut builder = services::Webdav::default().endpoint(endpoint).root(root);
                if let Some(v) = value("username") {
                    builder = builder.username(v);
                }
                if let Some(v) = value("password") {
                    builder = builder.password(v);
                }
                Operator::new(builder)?
            }
            _ => bail!("NotImplemented: filestore type {kind}"),
        };
        Ok(Self {
            operator,
            read_only: config.get("read-only").and_then(Value::as_bool) == Some(true),
        })
    }

    fn path(path: &str, directory: bool) -> Result<String> {
        let path = path.replace('\\', "/");
        let path = path.trim_matches('/');
        if !path.is_empty()
            && path
                .split('/')
                .any(|part| part.is_empty() || part == "." || part == "..")
        {
            bail!("invalid filestore path: {path}");
        }
        Ok(if directory && !path.is_empty() {
            format!("{path}/")
        } else {
            path.to_owned()
        })
    }

    fn run<T: Send, F: Future<Output = Result<T>> + Send>(future: F) -> Result<T> {
        std::thread::scope(|scope| {
            scope
                .spawn(move || {
                    RUNTIME.block_on(async {
                        tokio::time::timeout(std::time::Duration::from_secs(25), future).await
                    })
                })
                .join()
        })
        .map_err(|_| anyhow::anyhow!("remote filestore task panicked"))?
        .context("remote filestore operation timed out")?
    }

    fn writable(&self) -> Result<()> {
        if self.read_only {
            bail!("this filestore is read-only");
        }
        Ok(())
    }
}

impl FileStore for RemoteFileStore {
    fn list(&self, folder: &str) -> Result<Vec<FileEntry>> {
        let path = Self::path(folder, true)?;
        Self::run(async {
            let mut listed = None;
            for attempt in 0..3 {
                match tokio::time::timeout(
                    std::time::Duration::from_secs(7),
                    self.operator.list(&path),
                )
                .await
                {
                    Ok(Ok(entries)) => {
                        listed = Some(entries);
                        break;
                    }
                    Ok(Err(error)) if attempt == 2 => return Err(error.into()),
                    Err(_) if attempt == 2 => {
                        bail!("filestore listing timed out after three attempts")
                    }
                    _ => {
                        tokio::time::sleep(std::time::Duration::from_millis(200 * (1 << attempt)))
                            .await
                    }
                }
            }
            let mut entries = listed
                .context("filestore listing failed")?
                .into_iter()
                .filter_map(|entry| {
                    let entry_path = entry.path().trim_end_matches('/');
                    if entry_path == path.trim_end_matches('/') {
                        return None;
                    }
                    let name = entry_path.rsplit('/').next()?.to_owned();
                    Some(FileEntry {
                        name,
                        path: entry_path.to_owned(),
                        is_dir: entry.metadata().is_dir(),
                    })
                })
                .collect::<Vec<_>>();
            entries.sort_by(|a, b| {
                b.is_dir
                    .cmp(&a.is_dir)
                    .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
            });
            Ok(entries)
        })
    }
    fn read_text(&self, path: &str) -> Result<String> {
        let path = Self::path(path, false)?;
        Self::run(async {
            let metadata = self.operator.stat(&path).await?;
            if metadata.content_length() > 5 * 1024 * 1024 {
                bail!("file is too large for the text editor: {path}");
            }
            let bytes = self.operator.read(&path).await?.to_vec();
            Ok(String::from_utf8(bytes)?)
        })
    }
    fn write_text(&self, path: &str, content: &str) -> Result<()> {
        self.writable()?;
        let path = Self::path(path, false)?;
        let content = content.as_bytes().to_vec();
        Self::run(async {
            self.operator.write(&path, content).await?;
            Ok(())
        })
    }
    fn create_file(&self, path: &str) -> Result<()> {
        self.writable()?;
        let path = Self::path(path, false)?;
        Self::run(async {
            if self.operator.exists(&path).await? {
                bail!("file already exists: {path}");
            }
            self.operator.write(&path, Vec::<u8>::new()).await?;
            Ok(())
        })
    }
    fn create_dir(&self, path: &str) -> Result<()> {
        self.writable()?;
        let path = Self::path(path, true)?;
        if path.is_empty() {
            bail!("folder name is required");
        }
        Self::run(async {
            self.operator.create_dir(&path).await?;
            Ok(())
        })
    }
    fn rename(&self, from: &str, to: &str) -> Result<()> {
        self.writable()?;
        let from = Self::path(from, false)?;
        let to = Self::path(to, false)?;
        Self::run(async {
            if self.operator.exists(&to).await? {
                bail!("destination exists: {to}");
            }
            if self.operator.stat(&from).await?.is_dir() {
                bail!("NotImplemented: remote folder rename");
            }
            if let Err(error) = self.operator.rename(&from, &to).await {
                if error.kind() != opendal::ErrorKind::Unsupported {
                    return Err(error.into());
                }
                if let Err(error) = self.operator.copy(&from, &to).await {
                    if error.kind() != opendal::ErrorKind::Unsupported {
                        return Err(error.into());
                    }
                    let bytes = self.operator.read(&from).await?;
                    self.operator.write(&to, bytes).await?;
                }
                self.operator.delete(&from).await?;
            }
            Ok(())
        })
    }
    fn delete(&self, path: &str) -> Result<()> {
        self.writable()?;
        let path = Self::path(path, false)?;
        if path.is_empty() {
            bail!("cannot delete store root");
        }
        Self::run(async {
            let metadata = self.operator.stat(&path).await?;
            let path = if metadata.is_dir() {
                format!("{path}/")
            } else {
                path
            };
            self.operator.delete_with(&path).recursive(true).await?;
            Ok(())
        })
    }
    fn import_file(&self, source: &Path, destination: &str) -> Result<()> {
        self.writable()?;
        let content = std::fs::read_to_string(source)?;
        self.create_file(destination)?;
        self.write_text(destination, &content)
    }
    fn export_file(&self, source: &str, destination: &Path) -> Result<()> {
        if destination.exists() {
            bail!("export destination already exists");
        }
        std::fs::write(destination, self.read_text(source)?)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remote_adapter_supports_file_lifecycle() {
        let store = RemoteFileStore {
            operator: Operator::new(services::Memory::default()).unwrap(),
            read_only: false,
        };
        store.create_dir("docs").unwrap();
        store.create_file("docs/a.txt").unwrap();
        store.write_text("docs/a.txt", "hello").unwrap();
        assert_eq!(store.read_text("docs/a.txt").unwrap(), "hello");
        assert_eq!(store.list("docs").unwrap()[0].path, "docs/a.txt");
        store.rename("docs/a.txt", "docs/b.txt").unwrap();
        store.delete("docs/b.txt").unwrap();
        assert!(store.list("docs").unwrap().is_empty());
        assert!(store.create_file("../outside").is_err());
    }
}
