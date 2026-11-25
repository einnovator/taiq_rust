use crate::plugins::{PluginDescriptor, PluginKind};
pub fn descriptor() -> PluginDescriptor {
    PluginDescriptor {
        id: "brave_search",
        kind: PluginKind::Connector,
        extensions: &[],
    }
}
