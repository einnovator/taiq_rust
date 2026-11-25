use crate::plugins::{PluginDescriptor, PluginKind};
pub fn descriptor() -> PluginDescriptor {
    PluginDescriptor {
        id: "mcp-http",
        kind: PluginKind::Connector,
        extensions: &[],
    }
}
