mod webdav;
use crate::plugins::PluginDescriptor;
pub fn all() -> Vec<PluginDescriptor> {
    vec![webdav::descriptor()]
}
