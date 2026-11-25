use super::build_agent;
use crate::schema::{AgentDefinition, ForgeConfig};
use adk_rust::prelude::*;
use adk_rust::session::{CreateRequest, SessionService};
use adk_tool::FunctionTool;
use futures::StreamExt;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
struct DelegateArgs {
    agent_id: String,
    request: String,
}

pub fn delegation_tool(
    agent: &AgentDefinition,
    config: &ForgeConfig,
    path: &Path,
) -> Option<Arc<dyn Tool>> {
    let category = agent
        .other
        .get("category")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    if category != "Manager" {
        return None;
    }
    let available: Vec<_> = config
        .agents
        .agents
        .iter()
        .filter(|a| a.id != agent.id && a.enabled != Some(false))
        .map(|a| format!("{} ({})", a.id, a.description.as_deref().unwrap_or(&a.name)))
        .collect();
    let description = format!(
        "Ask another configured agent to handle a focused task. Available agents: {}",
        available.join("; ")
    );
    let config = config.clone();
    let path = path.to_path_buf();
    let own_id = agent.id.clone();
    Some(Arc::new(
        FunctionTool::new("ask_agent", description, move |_, args| {
            let config = config.clone();
            let path = path.clone();
            let own_id = own_id.clone();
            async move {
                let args: DelegateArgs =
                    serde_json::from_value(args).map_err(|e| AdkError::tool(e.to_string()))?;
                let target = config
                    .agents
                    .agents
                    .iter()
                    .find(|a| a.id == args.agent_id && a.enabled != Some(false) && a.id != own_id)
                    .ok_or_else(|| {
                        AdkError::tool(format!("unknown delegate: {}", args.agent_id))
                    })?;
                let agent = build_agent(target, &config, &path)
                    .map_err(|e| AdkError::tool(e.to_string()))?;
                let service: Arc<dyn SessionService> = Arc::new(InMemorySessionService::new());
                let session = service
                    .create(CreateRequest {
                        app_name: "taiq-delegate".into(),
                        user_id: "desktop".into(),
                        session_id: None,
                        state: HashMap::new(),
                    })
                    .await?;
                let runner = Runner::builder()
                    .app_name("taiq-delegate")
                    .agent(agent)
                    .session_service(service)
                    .build()?;
                let mut stream = runner
                    .run_str(
                        "desktop",
                        session.id(),
                        Content::new("user").with_text(&args.request),
                    )
                    .await?;
                let mut deltas = adk_rust::EventTextDeltas::default();
                let mut answer = String::new();
                while let Some(event) = stream.next().await {
                    let event = event?;
                    let event = deltas.push(&event);
                    if let Some(content) = event.content() {
                        for part in &content.parts {
                            if let Part::Text { text } = part {
                                answer.push_str(text);
                            }
                        }
                    }
                }
                Ok(json!({"agent_id": args.agent_id, "answer": answer}))
            }
        })
        .with_parameters_schema::<DelegateArgs>(),
    ))
}
