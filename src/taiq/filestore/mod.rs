mod local;
mod memory;
mod remote;

pub use local::LocalFileStore;
pub use memory::MemoryFileStore;
pub use remote::RemoteFileStore;

use anyhow::Result;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileEntry {
    pub name: String,
    pub path: String,
    pub is_dir: bool,
}

pub trait FileStore: Send + Sync {
    fn list(&self, folder: &str) -> Result<Vec<FileEntry>>;
    fn read_text(&self, path: &str) -> Result<String>;
    fn write_text(&self, path: &str, content: &str) -> Result<()>;
    fn create_file(&self, path: &str) -> Result<()>;
    fn create_dir(&self, path: &str) -> Result<()>;
    fn rename(&self, from: &str, to: &str) -> Result<()>;
    fn delete(&self, path: &str) -> Result<()>;
    fn import_file(&self, source: &std::path::Path, destination: &str) -> Result<()>;
    fn export_file(&self, source: &str, destination: &std::path::Path) -> Result<()>;
}

pub fn search(store: &dyn FileStore, query: &str) -> Result<Vec<FileEntry>> {
    let query = query.to_lowercase();
    if query.is_empty() {
        return Ok(Vec::new());
    }
    let mut matches = Vec::new();
    let mut folders = vec![(String::new(), 0_usize)];
    let mut visited = 0;
    while let Some((folder, depth)) = folders.pop() {
        if depth > 24 || visited > 5000 || matches.len() >= 250 {
            break;
        }
        for entry in store.list(&folder)? {
            visited += 1;
            if entry.path.to_lowercase().contains(&query) {
                matches.push(entry.clone());
            }
            if entry.is_dir {
                folders.push((entry.path, depth + 1));
            }
        }
    }
    Ok(matches)
}
