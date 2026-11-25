use crate::plugins::{PluginDescriptor, PluginKind};
pub fn descriptor() -> PluginDescriptor {
    PluginDescriptor {
        id: "filesystem-connector",
        kind: PluginKind::Connector,
        extensions: &[],
    }
}
