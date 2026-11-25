use crate::plugins::{PluginDescriptor, PluginKind};
pub fn descriptor() -> PluginDescriptor {
    PluginDescriptor {
        id: "gitlab",
        kind: PluginKind::Connector,
        extensions: &[],
    }
}
