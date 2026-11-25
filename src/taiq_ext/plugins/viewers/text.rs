use super::super::{PluginDescriptor, PluginKind};

pub fn descriptor() -> PluginDescriptor {
    PluginDescriptor {
        id: "text-viewer",
        kind: PluginKind::Editor,
        extensions: &["txt", "log", "csv", "env", "ini"],
    }
}
