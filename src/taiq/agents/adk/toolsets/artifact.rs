use super::catalog::ToolCatalog;
use crate::connectors::default_store;
use crate::filestore::FileStore;
use crate::schema::{AgentDefinition, ForgeConfig};
use adk_rust::{AdkError, Tool};
use adk_tool::FunctionTool;
use anyhow::Result;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::path::Path;
use std::sync::Arc;

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
struct ReadArgs {
    resource_id: String,
    offset: Option<usize>,
    limit: Option<usize>,
}
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
struct CreateArgs {
    path: String,
    content: String,
}
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
struct UpdateArgs {
    resource_id: String,
    content: String,
}
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
struct DeleteArgs {
    resource_id: String,
}

fn permitted(agent: &AgentDefinition, config: &ForgeConfig, name: &str, write: bool) -> bool {
    let alias = if name == "artifact_create" {
        "create_artifact"
    } else {
        name
    };
    let setting = agent.tools.iter().find(|entry| {
        matches!(entry.get("id").and_then(Value::as_str), Some(id) if id == name || id == alias)
    }).or_else(|| config.agents.tools.iter().find(|entry| {
        matches!(entry.get("id").and_then(Value::as_str), Some(id) if id == name || id == alias)
    }));
    match setting {
        Some(entry)
            if entry.get("enabled").and_then(Value::as_bool) == Some(false)
                || entry.get("mode").and_then(Value::as_str) == Some("disabled") =>
        {
            false
        }
        Some(_) if write => true,
        Some(_) => true,
        None => !write,
    }
}

pub fn tools(
    agent: &AgentDefinition,
    config: &ForgeConfig,
    config_path: &Path,
    catalog: &ToolCatalog,
) -> Result<Vec<Arc<dyn Tool>>> {
    let names = [
        "read_artifact",
        "artifact_create",
        "artifact_update",
        "artifact_delete",
    ];
    if !names.iter().any(|name| {
        catalog.find(name).is_some() && permitted(agent, config, name, *name != "read_artifact")
    }) {
        return Ok(Vec::new());
    }
    let store: Arc<dyn FileStore> = default_store(config, config_path)?;
    let mut tools: Vec<Arc<dyn Tool>> = Vec::new();
    if catalog.find("read_artifact").is_some() && permitted(agent, config, "read_artifact", false) {
        let store = store.clone();
        tools.push(Arc::new(FunctionTool::new("read_artifact", "Read a local text artifact by path with optional 1-based line range", move |_, args| {
            let store = store.clone();
            async move {
                let args: ReadArgs = serde_json::from_value(args).map_err(|e| AdkError::tool(e.to_string()))?;
                let content = store.read_text(&args.resource_id).map_err(|e| AdkError::tool(e.to_string()))?;
                let lines: Vec<_> = content.lines().collect();
                let offset = args.offset.unwrap_or(1);
                if offset == 0 { return Err(AdkError::tool("offset must be 1-based")); }
                let limit = args.limit.unwrap_or(300).clamp(1, 1000);
                let text = lines.iter().skip(offset - 1).take(limit).enumerate()
                    .map(|(i,line)| format!("{:>6}\t{line}", offset + i)).collect::<Vec<_>>().join("\n");
                Ok(json!({"resource_id": args.resource_id, "content": text, "total_lines": lines.len(), "next_offset": if offset - 1 + limit < lines.len() { Some(offset + limit) } else { None }}))
            }
        }).with_parameters_schema::<ReadArgs>().with_read_only(true)));
    }
    if catalog.find("artifact_create").is_some()
        && permitted(agent, config, "artifact_create", true)
    {
        let store = store.clone();
        tools.push(Arc::new(
            FunctionTool::new(
                "artifact_create",
                "Create a local text artifact",
                move |_, args| {
                    let store = store.clone();
                    async move {
                        let args: CreateArgs = serde_json::from_value(args)
                            .map_err(|e| AdkError::tool(e.to_string()))?;
                        store
                            .create_file(&args.path)
                            .map_err(|e| AdkError::tool(e.to_string()))?;
                        store
                            .write_text(&args.path, &args.content)
                            .map_err(|e| AdkError::tool(e.to_string()))?;
                        Ok(json!({"resource_id": args.path, "created": true}))
                    }
                },
            )
            .with_parameters_schema::<CreateArgs>(),
        ));
    }
    if catalog.find("artifact_update").is_some()
        && permitted(agent, config, "artifact_update", true)
    {
        let store = store.clone();
        tools.push(Arc::new(
            FunctionTool::new(
                "artifact_update",
                "Replace the content of an existing local text artifact",
                move |_, args| {
                    let store = store.clone();
                    async move {
                        let args: UpdateArgs = serde_json::from_value(args)
                            .map_err(|e| AdkError::tool(e.to_string()))?;
                        store
                            .write_text(&args.resource_id, &args.content)
                            .map_err(|e| AdkError::tool(e.to_string()))?;
                        Ok(json!({"resource_id": args.resource_id, "updated": true}))
                    }
                },
            )
            .with_parameters_schema::<UpdateArgs>(),
        ));
    }
    if catalog.find("artifact_delete").is_some()
        && permitted(agent, config, "artifact_delete", true)
    {
        let store = store.clone();
        tools.push(Arc::new(
            FunctionTool::new(
                "artifact_delete",
                "Delete a local artifact by path",
                move |_, args| {
                    let store = store.clone();
                    async move {
                        let args: DeleteArgs = serde_json::from_value(args)
                            .map_err(|e| AdkError::tool(e.to_string()))?;
                        store
                            .delete(&args.resource_id)
                            .map_err(|e| AdkError::tool(e.to_string()))?;
                        Ok(json!({"resource_id": args.resource_id, "deleted": true}))
                    }
                },
            )
            .with_parameters_schema::<DeleteArgs>(),
        ));
    }
    Ok(tools)
}
