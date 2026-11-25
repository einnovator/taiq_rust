use super::catalog::ToolCatalog;
use crate::schema::AgentDefinition;
use adk_rust::{AdkError, Tool};
use adk_tool::FunctionTool;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::sync::Arc;

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
struct SearchArgs {
    query: Option<String>,
    toolset: Option<String>,
}

pub fn tools(agent: &AgentDefinition, catalog: &ToolCatalog) -> Vec<Arc<dyn Tool>> {
    let mut result: Vec<Arc<dyn Tool>> = Vec::new();
    if catalog.find("search_toolset").is_some() {
        let entries = catalog.declarations.clone();
        result.push(Arc::new(
            FunctionTool::new(
                "search_toolset",
                "List and search the ForgeIQ toolsets and their Rust availability",
                move |_, args: Value| {
                    let entries = entries.clone();
                    async move {
                        let args: SearchArgs = serde_json::from_value(args)
                            .map_err(|e| AdkError::tool(e.to_string()))?;
                        let query = args.query.unwrap_or_default().to_lowercase();
                        let items: Vec<_> = entries
                            .iter()
                            .filter(|entry| {
                                args.toolset.as_deref().is_none_or(|name| entry.toolset == name)
                                    && (query.is_empty()
                                        || entry.name.to_lowercase().contains(&query)
                                        || entry.toolset.to_lowercase().contains(&query))
                            })
                            .map(|entry| {
                                json!({
                                    "name": entry.name,
                                    "toolset": entry.toolset,
                                    "description": entry.description,
                                    "available_in_rust": super::search::supports(&entry.name)
                                        || matches!(entry.name.as_str(), "search_toolset" | "get_meta_info" | "read_artifact" | "artifact_create" | "artifact_update" | "artifact_delete" | "list_connectors" | "get_connector_details" | "list_connector_resources_tool" | "read_connector_resource_tool")
                                })
                            })
                            .collect();
                        Ok(json!({"count": items.len(), "tools": items}))
                    }
                },
            )
            .with_parameters_schema::<SearchArgs>()
            .with_read_only(true),
        ));
    }
    if catalog.find("get_meta_info").is_some() {
        let agent_id = agent.id.clone();
        let count = catalog.declarations.len();
        result.push(Arc::new(
            FunctionTool::new("get_meta_info", "Describe this Rust desktop agent and its tool catalog", move |_, _| {
                let agent_id = agent_id.clone();
                async move { Ok(json!({"agent_id": agent_id, "tool_catalog_count": count, "runtime": "taiq-rust-desktop"})) }
            })
            .with_read_only(true),
        ));
    }
    result
}
