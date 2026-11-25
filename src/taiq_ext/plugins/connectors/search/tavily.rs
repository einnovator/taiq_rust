use crate::plugins::{PluginDescriptor, PluginKind};
pub fn descriptor() -> PluginDescriptor {
    PluginDescriptor {
        id: "tavily",
        kind: PluginKind::Connector,
        extensions: &[],
    }
}
