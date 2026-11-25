use crate::schema::AgentDefinition;
use std::collections::HashMap;
use std::path::{Component, Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

/// Presentation data shared by both desktop renderers.
#[derive(Debug, Clone)]
pub struct AgentVisual {
    pub category: String,
    pub initials: String,
    pub avatar_path: Option<PathBuf>,
}

fn local_asset(root: &Path, value: &str) -> Option<PathBuf> {
    let bare = value.split('?').next().unwrap_or(value);
    let (directory, name) = if let Some(name) = bare.strip_prefix("/api/agent-avatars/") {
        (root.join("config/agents/avatar"), name)
    } else if let Some(name) = bare.strip_prefix("/static/") {
        (root.join("src/static"), name)
    } else {
        return None;
    };
    Path::new(name)
        .components()
        .all(|c| matches!(c, Component::Normal(_)))
        .then(|| directory.join(name))
        .filter(|path| path.is_file())
}

impl AgentVisual {
    pub fn from_agent(agent: &AgentDefinition, config_path: &Path) -> Self {
        let category = agent
            .other
            .get("category")
            .and_then(|v| v.as_str())
            .unwrap_or("Agent")
            .to_owned();
        let initials = agent
            .name
            .split_whitespace()
            .take(2)
            .filter_map(|word| word.chars().next())
            .collect::<String>()
            .to_uppercase();
        let root = config_path
            .parent()
            .and_then(Path::parent)
            .unwrap_or(Path::new("."));
        let avatar_path = ["avatar", "icon", "image"]
            .into_iter()
            .filter_map(|key| agent.other.get(key).and_then(|v| v.as_str()))
            .find_map(|value| local_asset(root, value))
            .or_else(|| local_asset(root, "/static/img/agents/scout.svg"));
        Self {
            category,
            initials,
            avatar_path,
        }
    }

    /// Cache avatar bytes for egui's in-memory image loader.
    pub fn avatar_bytes(&self) -> Option<Arc<[u8]>> {
        self.avatar_path
            .as_ref()
            .and_then(|path| cached_image_bytes(path))
            .or_else(|| cached_svg_png(include_bytes!("icons/agents.svg"), 96))
    }
}

pub fn cached_image_bytes(path: &Path) -> Option<Arc<[u8]>> {
    static CACHE: OnceLock<Mutex<HashMap<PathBuf, Arc<[u8]>>>> = OnceLock::new();
    let mut cache = CACHE.get_or_init(Default::default).lock().ok()?;
    if let Some(bytes) = cache.get(path) {
        return Some(bytes.clone());
    }
    let raw = std::fs::read(path).ok()?;
    let bytes: Arc<[u8]> = if path
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("svg"))
    {
        svg_png(&raw, 96)?.into()
    } else {
        raw.into()
    };
    cache.insert(path.to_path_buf(), bytes.clone());
    Some(bytes)
}

pub fn svg_png(data: &[u8], size: u32) -> Option<Vec<u8>> {
    let tree = resvg::usvg::Tree::from_data(data, &resvg::usvg::Options::default()).ok()?;
    let mut pixmap = resvg::tiny_skia::Pixmap::new(size, size)?;
    let scale = (size as f32 / tree.size().width()).min(size as f32 / tree.size().height());
    let transform = resvg::tiny_skia::Transform::from_scale(scale, scale);
    resvg::render(&tree, transform, &mut pixmap.as_mut());
    pixmap.encode_png().ok()
}

pub fn cached_svg_png(data: &'static [u8], size: u32) -> Option<Arc<[u8]>> {
    static CACHE: OnceLock<Mutex<HashMap<(usize, u32), Arc<[u8]>>>> = OnceLock::new();
    let mut cache = CACHE.get_or_init(Default::default).lock().ok()?;
    let key = (data.as_ptr() as usize, size);
    if let Some(bytes) = cache.get(&key) {
        return Some(bytes.clone());
    }
    let bytes: Arc<[u8]> = svg_png(data, size)?.into();
    cache.insert(key, bytes.clone());
    Some(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::load_config;
    #[test]
    fn resolves_forgeiq_agent_visuals_in_place() {
        let path =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../orqestra/forgeiq/config/config.yaml");
        let config = load_config(&path, &[]).unwrap();
        let iq = config
            .agents
            .agents
            .iter()
            .find(|a| a.id == "iq_general_agent")
            .unwrap();
        let visual = AgentVisual::from_agent(iq, &path);
        assert_eq!(visual.category, "General");
        assert!(visual.avatar_path.is_some());
        assert!(visual.avatar_bytes().is_some());
        for id in ["piper_data_analysis_agent", "atlas_workspace_agent"] {
            let agent = config
                .agents
                .agents
                .iter()
                .find(|agent| agent.id == id)
                .unwrap();
            let bytes = AgentVisual::from_agent(agent, &path)
                .avatar_bytes()
                .unwrap();
            assert!(
                bytes.starts_with(b"\x89PNG\r\n\x1a\n"),
                "{id} SVG was not rendered as PNG"
            );
        }
        let mut no_avatar = iq.clone();
        no_avatar.other.remove("avatar");
        assert!(
            AgentVisual::from_agent(&no_avatar, &path)
                .avatar_bytes()
                .is_some()
        );
    }
}
