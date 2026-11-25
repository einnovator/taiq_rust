mod cloud;
mod local;
mod memory;
mod remote;

use super::PluginDescriptor;
pub fn all() -> Vec<PluginDescriptor> {
    let mut plugins = vec![local::descriptor(), memory::descriptor()];
    plugins.extend(cloud::all());
    plugins.extend(remote::all());
    plugins
}
