use crate::plugins::{PluginDescriptor, PluginKind};
pub fn descriptor() -> PluginDescriptor {
    PluginDescriptor {
        id: "gcs",
        kind: PluginKind::Filestore,
        extensions: &[],
    }
}
