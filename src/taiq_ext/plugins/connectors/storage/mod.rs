mod aws_s3;
mod azure_blob;
mod filesystem;
mod gcp_storage;
use super::super::PluginDescriptor;
pub fn all() -> Vec<PluginDescriptor> {
    vec![
        filesystem::descriptor(),
        aws_s3::descriptor(),
        azure_blob::descriptor(),
        gcp_storage::descriptor(),
    ]
}
