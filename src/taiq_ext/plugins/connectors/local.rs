use super::super::{PluginDescriptor, PluginKind};

pub fn descriptor() -> PluginDescriptor {
    PluginDescriptor {
        id: "local-files",
        kind: PluginKind::Connector,
        extensions: &[],
    }
}
