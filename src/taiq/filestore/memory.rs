use super::{FileEntry, FileStore};
use anyhow::{Result, bail};
use std::collections::BTreeMap;
use std::path::Path;
use std::sync::RwLock;

/// Ephemeral workspace. Paths use the same relative, slash-separated form as local stores.
#[derive(Default)]
pub struct MemoryFileStore {
    entries: RwLock<BTreeMap<String, Option<String>>>,
}

impl MemoryFileStore {
    pub fn new() -> Self {
        Self::default()
    }

    fn path(path: &str) -> Result<String> {
        let path = path.replace('\\', "/");
        let path = path.trim_matches('/');
        if path.is_empty() {
            return Ok(String::new());
        }
        if path
            .split('/')
            .any(|part| part == "." || part == ".." || part.is_empty())
        {
            bail!("invalid filestore path: {path}");
        }
        Ok(path.to_owned())
    }

    fn parent_exists(entries: &BTreeMap<String, Option<String>>, path: &str) -> bool {
        path.rsplit_once('/')
            .is_none_or(|(parent, _)| entries.get(parent) == Some(&None))
    }
}

impl FileStore for MemoryFileStore {
    fn list(&self, folder: &str) -> Result<Vec<FileEntry>> {
        let folder = Self::path(folder)?;
        let entries = self.entries.read().unwrap();
        if !folder.is_empty() && entries.get(&folder) != Some(&None) {
            bail!("not a folder: {folder}");
        }
        let mut output = entries
            .iter()
            .filter_map(|(path, content)| {
                let (parent, name) = path.rsplit_once('/').unwrap_or(("", path));
                (parent == folder).then(|| FileEntry {
                    name: name.to_owned(),
                    path: path.clone(),
                    is_dir: content.is_none(),
                })
            })
            .collect::<Vec<_>>();
        output.sort_by(|a, b| {
            b.is_dir
                .cmp(&a.is_dir)
                .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
        });
        Ok(output)
    }
    fn read_text(&self, path: &str) -> Result<String> {
        let path = Self::path(path)?;
        self.entries
            .read()
            .unwrap()
            .get(&path)
            .and_then(Clone::clone)
            .ok_or_else(|| anyhow::anyhow!("not a file: {path}"))
    }
    fn write_text(&self, path: &str, content: &str) -> Result<()> {
        let path = Self::path(path)?;
        let mut entries = self.entries.write().unwrap();
        match entries.get_mut(&path) {
            Some(Some(text)) => {
                *text = content.to_owned();
                Ok(())
            }
            _ => bail!("not a file: {path}"),
        }
    }
    fn create_file(&self, path: &str) -> Result<()> {
        self.insert(path, Some(String::new()))
    }
    fn create_dir(&self, path: &str) -> Result<()> {
        self.insert(path, None)
    }
    fn rename(&self, from: &str, to: &str) -> Result<()> {
        let from = Self::path(from)?;
        let to = Self::path(to)?;
        if from.is_empty() || to.is_empty() || to.starts_with(&format!("{from}/")) {
            bail!("invalid move");
        }
        let mut entries = self.entries.write().unwrap();
        if !entries.contains_key(&from)
            || entries.contains_key(&to)
            || !Self::parent_exists(&entries, &to)
        {
            bail!("invalid move destination");
        }
        let paths = entries
            .keys()
            .filter(|path| *path == &from || path.starts_with(&format!("{from}/")))
            .cloned()
            .collect::<Vec<_>>();
        for path in paths {
            let value = entries.remove(&path).unwrap();
            entries.insert(format!("{to}{}", &path[from.len()..]), value);
        }
        Ok(())
    }
    fn delete(&self, path: &str) -> Result<()> {
        let path = Self::path(path)?;
        if path.is_empty() {
            bail!("cannot delete store root");
        }
        let mut entries = self.entries.write().unwrap();
        if !entries.contains_key(&path) {
            bail!("path does not exist: {path}");
        }
        entries.retain(|key, _| key != &path && !key.starts_with(&format!("{path}/")));
        Ok(())
    }
    fn import_file(&self, source: &Path, destination: &str) -> Result<()> {
        let content = std::fs::read_to_string(source)?;
        self.insert(destination, Some(content))
    }
    fn export_file(&self, source: &str, destination: &Path) -> Result<()> {
        if destination.exists() {
            bail!("export destination already exists");
        }
        std::fs::write(destination, self.read_text(source)?)?;
        Ok(())
    }
}

impl MemoryFileStore {
    fn insert(&self, path: &str, content: Option<String>) -> Result<()> {
        let path = Self::path(path)?;
        if path.is_empty() {
            bail!("path is required");
        }
        let mut entries = self.entries.write().unwrap();
        if entries.contains_key(&path) || !Self::parent_exists(&entries, &path) {
            bail!("path exists or parent is missing: {path}");
        }
        entries.insert(path, content);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn manages_nested_entries() {
        let store = MemoryFileStore::new();
        store.create_dir("docs").unwrap();
        store.create_file("docs/a.md").unwrap();
        store.write_text("docs/a.md", "hello").unwrap();
        store.rename("docs", "notes").unwrap();
        assert_eq!(store.read_text("notes/a.md").unwrap(), "hello");
        assert!(store.read_text("docs/a.md").is_err());
        store.delete("notes").unwrap();
        assert!(store.list("").unwrap().is_empty());
        assert!(store.create_file("../escape").is_err());
    }
}
