mod azure;
mod gcs;
mod s3;

use crate::plugins::PluginDescriptor;
pub fn all() -> Vec<PluginDescriptor> {
    vec![s3::descriptor(), gcs::descriptor(), azure::descriptor()]
}
