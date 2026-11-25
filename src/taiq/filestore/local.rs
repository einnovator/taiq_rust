use super::{FileEntry, FileStore};
use anyhow::{Context, Result, bail};
use std::fs;
use std::path::{Component, Path, PathBuf};

#[derive(Debug)]
pub struct LocalFileStore {
    root: PathBuf,
    read_only: bool,
}

impl LocalFileStore {
    pub fn new(root: impl AsRef<Path>) -> Result<Self> {
        Self::new_with_mode(root, false)
    }

    pub fn new_with_mode(root: impl AsRef<Path>, read_only: bool) -> Result<Self> {
        if !read_only {
            fs::create_dir_all(root.as_ref())?;
        }
        let root = root.as_ref().canonicalize()?;
        Ok(Self { root, read_only })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    fn checked_path(&self, relative: &str) -> Result<PathBuf> {
        let path = Path::new(relative);
        if path
            .components()
            .any(|c| !matches!(c, Component::Normal(_) | Component::CurDir))
        {
            bail!("path must stay within the local file store: {relative}");
        }
        let joined = self.root.join(path);
        if joined.exists() {
            let resolved = joined.canonicalize()?;
            if !resolved.starts_with(&self.root) {
                bail!("path escapes local file store: {relative}");
            }
            Ok(resolved)
        } else {
            let parent = joined
                .parent()
                .context("file path has no parent")?
                .canonicalize()
                .with_context(|| format!("parent folder for {relative} does not exist"))?;
            if !parent.starts_with(&self.root) {
                bail!("path escapes local file store: {relative}");
            }
            Ok(parent.join(joined.file_name().context("file path has no name")?))
        }
    }
}

impl FileStore for LocalFileStore {
    fn list(&self, folder: &str) -> Result<Vec<FileEntry>> {
        let base = self.checked_path(folder)?;
        if !base.is_dir() {
            bail!("not a folder: {folder}");
        }
        let mut entries = Vec::new();
        for entry in fs::read_dir(base)? {
            let entry = entry?;
            let name = entry.file_name().to_string_lossy().into_owned();
            if matches!(
                name.as_str(),
                ".git" | ".keep" | ".system" | ".__system" | ".__trash"
            ) {
                continue;
            }
            let metadata = entry.metadata()?;
            if !(metadata.is_dir() || metadata.is_file()) {
                continue;
            }
            let path = if folder.is_empty() {
                name.clone()
            } else {
                format!("{folder}/{name}")
            };
            // Do not expose symlink targets outside the configured root.
            if self.checked_path(&path).is_err() {
                continue;
            }
            entries.push(FileEntry {
                name,
                path,
                is_dir: metadata.is_dir(),
            });
        }
        entries.sort_by(|a, b| {
            b.is_dir
                .cmp(&a.is_dir)
                .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
        });
        Ok(entries)
    }

    fn read_text(&self, path: &str) -> Result<String> {
        let target = self.checked_path(path)?;
        if !target.is_file() {
            bail!("not a file: {path}");
        }
        if target.metadata()?.len() > 5 * 1024 * 1024 {
            bail!("file is too large for the text editor: {path}");
        }
        fs::read_to_string(&target).with_context(|| format!("read {path}"))
    }

    fn write_text(&self, path: &str, content: &str) -> Result<()> {
        if self.read_only {
            bail!("this filestore is read-only");
        }
        let target = self.checked_path(path)?;
        if !target.is_file() {
            bail!("not a file: {path}");
        }
        fs::write(&target, content).with_context(|| format!("save {path}"))
    }

    fn create_file(&self, path: &str) -> Result<()> {
        if self.read_only {
            bail!("this filestore is read-only");
        }
        let target = self.checked_path(path)?;
        if target.exists() {
            bail!("file already exists: {path}");
        }
        fs::write(&target, b"").with_context(|| format!("create {path}"))
    }

    fn create_dir(&self, path: &str) -> Result<()> {
        if self.read_only {
            bail!("this filestore is read-only");
        }
        if path.is_empty() {
            bail!("folder name is required");
        }
        let target = self.checked_path(path)?;
        if target.exists() {
            bail!("folder already exists: {path}");
        }
        fs::create_dir(&target).with_context(|| format!("create folder {path}"))
    }

    fn rename(&self, from: &str, to: &str) -> Result<()> {
        if self.read_only {
            bail!("this filestore is read-only");
        }
        if from.is_empty() || to.is_empty() {
            bail!("source and destination are required");
        }
        let source = self.checked_path(from)?;
        let destination = self.checked_path(to)?;
        if !source.exists() {
            bail!("file or folder does not exist: {from}");
        }
        if destination.exists() {
            bail!("destination already exists: {to}");
        }
        if destination.starts_with(&source) {
            bail!("cannot move a folder inside itself");
        }
        fs::rename(source, destination).with_context(|| format!("move {from} to {to}"))
    }

    fn delete(&self, path: &str) -> Result<()> {
        if self.read_only {
            bail!("this filestore is read-only");
        }
        if path.is_empty() {
            bail!("cannot delete the store root");
        }
        let target = self.checked_path(path)?;
        if target.is_dir() {
            fs::remove_dir_all(target).with_context(|| format!("delete folder {path}"))
        } else {
            fs::remove_file(target).with_context(|| format!("delete file {path}"))
        }
    }

    fn import_file(&self, source: &Path, destination: &str) -> Result<()> {
        if self.read_only {
            bail!("this filestore is read-only");
        }
        if !source.is_file() {
            bail!("import source is not a file: {}", source.display());
        }
        let target = self.checked_path(destination)?;
        if target.exists() {
            bail!("destination already exists: {destination}");
        }
        fs::copy(source, target).with_context(|| format!("import {}", source.display()))?;
        Ok(())
    }

    fn export_file(&self, source: &str, destination: &Path) -> Result<()> {
        let source_path = self.checked_path(source)?;
        if !source_path.is_file() {
            bail!("export source is not a file: {source}");
        }
        if destination.exists() {
            bail!(
                "export destination already exists: {}",
                destination.display()
            );
        }
        fs::copy(source_path, destination).with_context(|| format!("export {source}"))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lists_reads_and_saves_without_escaping_root() {
        let path = std::env::temp_dir().join(format!("taiq-store-{}", std::process::id()));
        let store = LocalFileStore::new(&path).unwrap();
        store.create_file("demo.rs").unwrap();
        store.write_text("demo.rs", "fn main() {}\n").unwrap();
        assert_eq!(store.read_text("demo.rs").unwrap(), "fn main() {}\n");
        assert!(
            store
                .list("")
                .unwrap()
                .iter()
                .any(|entry| entry.path == "demo.rs")
        );
        assert!(store.read_text("../outside").is_err());
        store.create_dir("nested").unwrap();
        store.rename("demo.rs", "nested/moved.rs").unwrap();
        assert_eq!(
            store.read_text("nested/moved.rs").unwrap(),
            "fn main() {}\n"
        );
        let exported = std::env::temp_dir().join(format!("taiq-export-{}.rs", std::process::id()));
        store.export_file("nested/moved.rs", &exported).unwrap();
        store.import_file(&exported, "imported.rs").unwrap();
        assert_eq!(store.read_text("imported.rs").unwrap(), "fn main() {}\n");
        store.delete("nested").unwrap();
        assert!(store.read_text("nested/moved.rs").is_err());
        fs::remove_file(exported).unwrap();
        fs::remove_dir_all(path).unwrap();
    }
}
