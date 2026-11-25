use crate::plugins::{PluginDescriptor, PluginKind};
pub fn descriptor() -> PluginDescriptor {
    PluginDescriptor {
        id: "azure",
        kind: PluginKind::Filestore,
        extensions: &[],
    }
}
