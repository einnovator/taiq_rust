use super::{CenterView, DocumentView, LeftView};
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct LayoutPreferences {
    pub left_width: f32,
    pub editor_width: f32,
    pub left_view: LeftView,
    pub center_view: CenterView,
    pub dark_mode: bool,
    pub show_reasoning: bool,
    pub document_view: DocumentView,
    pub center_tabs: Vec<CenterView>,
    pub open_documents: Vec<String>,
    pub active_document: Option<String>,
    pub floating_documents: Vec<String>,
    pub floating_bounds: HashMap<String, [f32; 4]>,
    pub selected_store: Option<String>,
    pub active_logical_root: Option<String>,
    pub workspace_folders: Vec<PathBuf>,
}

impl Default for LayoutPreferences {
    fn default() -> Self {
        Self {
            left_width: 250.0,
            editor_width: 420.0,
            left_view: LeftView::Files,
            center_view: CenterView::Chat,
            dark_mode: false,
            show_reasoning: false,
            document_view: DocumentView::Source,
            center_tabs: vec![
                CenterView::Chat,
                CenterView::Web,
                CenterView::Changes,
                CenterView::Admin,
            ],
            open_documents: Vec::new(),
            active_document: None,
            floating_documents: Vec::new(),
            floating_bounds: HashMap::new(),
            selected_store: None,
            active_logical_root: None,
            workspace_folders: Vec::new(),
        }
    }
}

pub struct PreferencesStore {
    path: PathBuf,
}

impl PreferencesStore {
    pub fn open(config_path: &Path) -> Self {
        let base = std::env::var_os("LOCALAPPDATA")
            .or_else(|| std::env::var_os("XDG_DATA_HOME"))
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|v| PathBuf::from(v).join(".local/share")))
            .unwrap_or_else(std::env::temp_dir)
            .join("taiq");
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        config_path.hash(&mut hasher);
        Self {
            path: base.join(format!("{:016x}.layout.json", hasher.finish())),
        }
    }
    pub fn load(&self) -> LayoutPreferences {
        fs::read(&self.path)
            .ok()
            .and_then(|data| serde_json::from_slice(&data).ok())
            .unwrap_or_default()
    }
    pub fn save(&self, prefs: &LayoutPreferences) -> Result<()> {
        fs::create_dir_all(self.path.parent().expect("layout path has parent"))?;
        fs::write(&self.path, serde_json::to_vec_pretty(prefs)?)?;
        Ok(())
    }
}
