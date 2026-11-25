mod mcp;
use super::super::PluginDescriptor;
pub fn all() -> Vec<PluginDescriptor> {
    vec![mcp::descriptor()]
}
