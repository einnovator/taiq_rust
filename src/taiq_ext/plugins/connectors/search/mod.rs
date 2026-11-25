mod brave;
mod tavily;

use crate::plugins::PluginDescriptor;
pub fn all() -> Vec<PluginDescriptor> {
    vec![brave::descriptor(), tavily::descriptor()]
}
