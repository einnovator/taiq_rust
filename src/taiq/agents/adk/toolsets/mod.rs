mod artifact;
mod catalog;
mod connector;
mod meta;
mod search;

use crate::schema::{AgentDefinition, ForgeConfig};
use adk_rust::Tool;
use anyhow::Result;
use std::path::Path;
use std::sync::Arc;

pub use catalog::ToolCatalog;

pub fn native_tools(
    agent: &AgentDefinition,
    config: &ForgeConfig,
    config_path: &Path,
) -> Result<Vec<Arc<dyn Tool>>> {
    let catalog = ToolCatalog::load(config, config_path)?;
    let mut tools = Vec::new();
    tools.extend(artifact::tools(agent, config, config_path, &catalog)?);
    tools.extend(search::tools(agent, config, config_path, &catalog)?);
    tools.extend(connector::tools(agent, config, config_path, &catalog));
    tools.extend(meta::tools(agent, &catalog));
    Ok(tools)
}
