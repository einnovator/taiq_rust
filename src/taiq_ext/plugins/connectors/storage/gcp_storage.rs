use crate::plugins::{PluginDescriptor, PluginKind};
pub fn descriptor() -> PluginDescriptor {
    PluginDescriptor {
        id: "gcp_storage",
        kind: PluginKind::Connector,
        extensions: &[],
    }
}
