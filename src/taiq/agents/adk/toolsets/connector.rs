use super::catalog::ToolCatalog;
use crate::connectors::remote::RemoteConnector;
use crate::schema::{AgentDefinition, ConnectorDefinition, ForgeConfig};
use adk_rust::{AdkError, Tool};
use adk_tool::FunctionTool;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use std::sync::Arc;

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
struct ConnectorArgs {
    connector_id: String,
    path: Option<String>,
    resource_id: Option<String>,
}

fn allowed(agent: &AgentDefinition, catalog: &ToolCatalog, name: &str) -> bool {
    catalog.find(name).is_some()
        && !agent.tools.iter().any(|entry| {
            entry.get("id").and_then(Value::as_str) == Some(name)
                && (entry.get("enabled").and_then(Value::as_bool) == Some(false)
                    || entry.get("mode").and_then(Value::as_str) == Some("disabled"))
        })
}

fn public_definition(item: &ConnectorDefinition) -> Value {
    json!({
        "id": item.id,
        "name": item.name,
        "provider": item.provider,
        "proto": item.proto,
        "enabled": item.enable.as_ref().and_then(Value::as_bool).unwrap_or(true),
    })
}

pub fn tools(
    agent: &AgentDefinition,
    config: &ForgeConfig,
    config_path: &Path,
    catalog: &ToolCatalog,
) -> Vec<Arc<dyn Tool>> {
    let mut result: Vec<Arc<dyn Tool>> = Vec::new();
    if allowed(agent, catalog, "list_connectors") {
        let definitions = config.connectors.clone();
        result.push(Arc::new(FunctionTool::new("list_connectors", "List configured connector metadata without secrets", move |_, _| {
            let definitions = definitions.clone();
            async move { Ok(json!({"connectors": definitions.iter().map(public_definition).collect::<Vec<_>>() })) }
        }).with_read_only(true)));
    }
    if allowed(agent, catalog, "get_connector_details") {
        let definitions = config.connectors.clone();
        result.push(Arc::new(
            FunctionTool::new(
                "get_connector_details",
                "Describe a configured connector without secrets",
                move |_, args: Value| {
                    let definitions = definitions.clone();
                    async move {
                        let args: ConnectorArgs = serde_json::from_value(args)
                            .map_err(|e| AdkError::tool(e.to_string()))?;
                        definitions
                            .iter()
                            .find(|item| item.id.as_deref() == Some(&args.connector_id))
                            .map(public_definition)
                            .ok_or_else(|| AdkError::tool("connector not found"))
                    }
                },
            )
            .with_parameters_schema::<ConnectorArgs>()
            .with_read_only(true),
        ));
    }
    if allowed(agent, catalog, "list_connector_resources_tool") {
        let definitions = config.connectors.clone();
        let config_path = PathBuf::from(config_path);
        result.push(Arc::new(
            FunctionTool::new(
                "list_connector_resources_tool",
                "List resources in a configured GitHub, GitLab, or MCP connector",
                move |_, args: Value| {
                    let definitions = definitions.clone();
                    let config_path = config_path.clone();
                    async move {
                        let args: ConnectorArgs = serde_json::from_value(args)
                            .map_err(|e| AdkError::tool(e.to_string()))?;
                        let definition = definitions
                            .iter()
                            .find(|item| item.id.as_deref() == Some(&args.connector_id))
                            .ok_or_else(|| AdkError::tool("connector not found"))?
                            .clone();
                        let connector = RemoteConnector::new(definition, &config_path)
                            .map_err(|e| AdkError::tool(e.to_string()))?;
                        connector
                            .browse(args.path.as_deref().unwrap_or(""))
                            .await
                            .map_err(|e| AdkError::tool(e.to_string()))
                    }
                },
            )
            .with_parameters_schema::<ConnectorArgs>()
            .with_read_only(true),
        ));
    }
    if allowed(agent, catalog, "read_connector_resource_tool") {
        let definitions = config.connectors.clone();
        let config_path = PathBuf::from(config_path);
        result.push(Arc::new(
            FunctionTool::new(
                "read_connector_resource_tool",
                "Read a text file from a configured developer connector",
                move |_, args: Value| {
                    let definitions = definitions.clone();
                    let config_path = config_path.clone();
                    async move {
                        let args: ConnectorArgs = serde_json::from_value(args)
                            .map_err(|e| AdkError::tool(e.to_string()))?;
                        let definition = definitions
                            .iter()
                            .find(|item| item.id.as_deref() == Some(&args.connector_id))
                            .ok_or_else(|| AdkError::tool("connector not found"))?
                            .clone();
                        let connector = RemoteConnector::new(definition, &config_path)
                            .map_err(|e| AdkError::tool(e.to_string()))?;
                        let path = args
                            .resource_id
                            .or(args.path)
                            .ok_or_else(|| AdkError::tool("resource_id is required"))?;
                        let content = connector
                            .read_text(&path)
                            .await
                            .map_err(|e| AdkError::tool(e.to_string()))?;
                        Ok(json!({"resource_id":path,"content":content}))
                    }
                },
            )
            .with_parameters_schema::<ConnectorArgs>()
            .with_read_only(true),
        ));
    }
    result
}
