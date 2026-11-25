use crate::plugins::{PluginDescriptor, PluginKind};
pub fn descriptor() -> PluginDescriptor {
    PluginDescriptor {
        id: "s3",
        kind: PluginKind::Filestore,
        extensions: &[],
    }
}
