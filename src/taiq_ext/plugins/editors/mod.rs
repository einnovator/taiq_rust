mod code;

use super::PluginDescriptor;
pub fn all() -> Vec<PluginDescriptor> {
    vec![code::descriptor()]
}
