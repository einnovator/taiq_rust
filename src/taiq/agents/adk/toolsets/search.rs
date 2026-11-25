use super::catalog::ToolCatalog;
use crate::connectors::default_store;
use crate::filestore::{FileEntry, FileStore};
use crate::schema::{AgentDefinition, ForgeConfig};
use adk_rust::{AdkError, Tool};
use adk_tool::FunctionTool;
use anyhow::{Result, bail};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::path::Path;
use std::sync::Arc;

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
struct GlobArgs {
    pattern: String,
    path: Option<String>,
    limit: Option<usize>,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
struct GrepArgs {
    pattern: String,
    path: Option<String>,
    glob: Option<String>,
    output_mode: Option<String>,
    limit: Option<usize>,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
struct SearchLocalArgs {
    query: String,
}

pub fn supports(name: &str) -> bool {
    matches!(name, "glob" | "grep" | "search_local_files")
}

fn not_disabled(agent: &AgentDefinition, name: &str) -> bool {
    !agent.tools.iter().any(|entry| {
        entry.get("id").and_then(serde_json::Value::as_str) == Some(name)
            && (entry.get("enabled").and_then(serde_json::Value::as_bool) == Some(false)
                || entry.get("mode").and_then(serde_json::Value::as_str) == Some("disabled"))
    })
}

pub fn tools(
    agent: &AgentDefinition,
    config: &ForgeConfig,
    config_path: &Path,
    catalog: &ToolCatalog,
) -> Result<Vec<Arc<dyn Tool>>> {
    let names = ["glob", "grep", "search_local_files"];
    if !names
        .iter()
        .any(|name| catalog.find(name).is_some() && not_disabled(agent, name))
    {
        return Ok(Vec::new());
    }
    let store: Arc<dyn FileStore> = default_store(config, config_path)?;
    let mut result: Vec<Arc<dyn Tool>> = Vec::new();
    if catalog.find("glob").is_some() && not_disabled(agent, "glob") {
        let store = store.clone();
        result.push(Arc::new(
            FunctionTool::new("glob", "Find workspace files using a glob pattern", move |_, args| {
                let store = store.clone();
                async move {
                    let args: GlobArgs = serde_json::from_value(args).map_err(|e| AdkError::tool(e.to_string()))?;
                    let files = glob_files(store.as_ref(), &args).map_err(|e| AdkError::tool(e.to_string()))?;
                    Ok(json!({"pattern": args.pattern, "files": files, "truncated": files.len() >= args.limit.unwrap_or(100).min(500)}))
                }
            })
            .with_parameters_schema::<GlobArgs>()
            .with_read_only(true),
        ));
    }
    if catalog.find("grep").is_some() && not_disabled(agent, "grep") {
        let store = store.clone();
        result.push(Arc::new(
            FunctionTool::new(
                "grep",
                "Search UTF-8 workspace files with a regular expression",
                move |_, args| {
                    let store = store.clone();
                    async move {
                        let args: GrepArgs = serde_json::from_value(args)
                            .map_err(|e| AdkError::tool(e.to_string()))?;
                        grep_files(store.as_ref(), &args).map_err(|e| AdkError::tool(e.to_string()))
                    }
                },
            )
            .with_parameters_schema::<GrepArgs>()
            .with_read_only(true),
        ));
    }
    if catalog.find("search_local_files").is_some() && not_disabled(agent, "search_local_files") {
        let store = store.clone();
        result.push(Arc::new(
            FunctionTool::new(
                "search_local_files",
                "Search workspace file paths by name",
                move |_, args| {
                    let store = store.clone();
                    async move {
                        let args: SearchLocalArgs = serde_json::from_value(args)
                            .map_err(|e| AdkError::tool(e.to_string()))?;
                        let matches = crate::filestore::search(store.as_ref(), &args.query)
                            .map_err(|e| AdkError::tool(e.to_string()))?
                            .into_iter()
                            .map(|entry| entry.path)
                            .collect::<Vec<_>>();
                        Ok(json!({"files": matches}))
                    }
                },
            )
            .with_parameters_schema::<SearchLocalArgs>()
            .with_read_only(true),
        ));
    }
    Ok(result)
}

fn walk(store: &dyn FileStore, root: &str) -> Result<Vec<FileEntry>> {
    let mut found = Vec::new();
    let mut pending = vec![(root.to_owned(), 0usize)];
    while let Some((folder, depth)) = pending.pop() {
        if depth > 24 || found.len() >= 5000 {
            break;
        }
        for entry in store.list(&folder)? {
            if entry.is_dir {
                pending.push((entry.path.clone(), depth + 1));
            }
            found.push(entry);
            if found.len() >= 5000 {
                break;
            }
        }
    }
    Ok(found)
}

fn glob_files(store: &dyn FileStore, args: &GlobArgs) -> Result<Vec<String>> {
    let pattern = glob::Pattern::new(&args.pattern)?;
    let limit = args.limit.unwrap_or(100).clamp(1, 500);
    let matches = walk(store, args.path.as_deref().unwrap_or(""))?
        .into_iter()
        .filter(|entry| {
            !entry.is_dir && (pattern.matches(&entry.path) || pattern.matches(&entry.name))
        })
        .take(limit)
        .map(|entry| entry.path)
        .collect();
    Ok(matches)
}

fn grep_files(store: &dyn FileStore, args: &GrepArgs) -> Result<serde_json::Value> {
    let regex = regex::Regex::new(&args.pattern)?;
    let filter = args
        .glob
        .as_ref()
        .map(|pattern| glob::Pattern::new(pattern))
        .transpose()?;
    let mode = args.output_mode.as_deref().unwrap_or("files_with_matches");
    if !matches!(mode, "files_with_matches" | "content" | "count") {
        bail!("output_mode must be files_with_matches, content, or count");
    }
    let limit = args.limit.unwrap_or(100).clamp(1, 500);
    let mut matches = Vec::new();
    for entry in walk(store, args.path.as_deref().unwrap_or(""))? {
        if entry.is_dir || filter.as_ref().is_some_and(|f| !f.matches(&entry.path)) {
            continue;
        }
        let Ok(content) = store.read_text(&entry.path) else {
            continue;
        };
        let lines: Vec<_> = content
            .lines()
            .enumerate()
            .filter(|(_, line)| regex.is_match(line))
            .take(100)
            .map(|(index, line)| json!({"line": index + 1, "text": line}))
            .collect();
        if lines.is_empty() {
            continue;
        }
        matches.push(match mode {
            "count" => json!({"file": entry.path, "count": lines.len()}),
            "content" => json!({"file": entry.path, "lines": lines}),
            _ => json!(entry.path),
        });
        if matches.len() >= limit {
            break;
        }
    }
    Ok(
        json!({"pattern": args.pattern, "output_mode": mode, "matches": matches, "truncated": matches.len() >= limit}),
    )
}
