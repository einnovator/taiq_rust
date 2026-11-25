use crate::plugins::{PluginDescriptor, PluginKind};
pub fn descriptor() -> PluginDescriptor {
    PluginDescriptor {
        id: "github",
        kind: PluginKind::Connector,
        extensions: &[],
    }
}
