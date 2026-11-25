use crate::plugins::{PluginDescriptor, PluginKind};
pub fn descriptor() -> PluginDescriptor {
    PluginDescriptor {
        id: "memory",
        kind: PluginKind::Filestore,
        extensions: &[],
    }
}
