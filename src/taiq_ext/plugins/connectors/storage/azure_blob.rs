use crate::plugins::{PluginDescriptor, PluginKind};
pub fn descriptor() -> PluginDescriptor {
    PluginDescriptor {
        id: "azure_blob",
        kind: PluginKind::Connector,
        extensions: &[],
    }
}
