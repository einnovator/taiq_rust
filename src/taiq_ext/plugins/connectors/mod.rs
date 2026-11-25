mod configured;
mod developer;
mod local;
mod search;
mod storage;

use super::PluginDescriptor;
pub fn all() -> Vec<PluginDescriptor> {
    let mut plugins = vec![local::descriptor()];
    plugins.extend(developer::all());
    plugins.extend(configured::all());
    plugins.extend(storage::all());
    plugins.extend(search::all());
    plugins
}
