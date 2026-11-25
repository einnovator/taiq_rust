use crate::plugins::{PluginDescriptor, PluginKind};
pub fn descriptor() -> PluginDescriptor {
    PluginDescriptor {
        id: "aws_s3",
        kind: PluginKind::Connector,
        extensions: &[],
    }
}
