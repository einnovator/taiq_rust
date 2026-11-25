mod github;
mod gitlab;
use super::super::PluginDescriptor;
pub fn all() -> Vec<PluginDescriptor> {
    vec![github::descriptor(), gitlab::descriptor()]
}
