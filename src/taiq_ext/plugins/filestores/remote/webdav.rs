use crate::plugins::{PluginDescriptor, PluginKind};
pub fn descriptor() -> PluginDescriptor {
    PluginDescriptor {
        id: "webdav",
        kind: PluginKind::Filestore,
        extensions: &[],
    }
}
