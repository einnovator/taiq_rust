use super::super::{PluginDescriptor, PluginKind};

pub fn descriptor() -> PluginDescriptor {
    PluginDescriptor {
        id: "local-filestore",
        kind: PluginKind::Filestore,
        extensions: &[],
    }
}
