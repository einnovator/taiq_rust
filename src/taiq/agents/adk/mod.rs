use crate::agents::{instructions, selected_model};
use crate::schema::{AgentDefinition, ForgeConfig};
use adk_rust::model::openai::{
    OpenAIReasoningEffort, OpenAIResponsesClient, OpenAIResponsesConfig,
};
use adk_rust::prelude::*;
use adk_rust::session::{CreateRequest, SessionService};
use anyhow::{Context, Result, bail};
use futures::StreamExt;
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
mod delegation;
mod router;
mod skills;
mod toolsets;

pub fn validate_toolsets(config: &ForgeConfig, config_path: &Path) -> Result<()> {
    toolsets::ToolCatalog::load(config, config_path)?;
    Ok(())
}

pub fn configured_tool_catalog(
    config: &ForgeConfig,
    config_path: &Path,
) -> Result<Vec<(String, String, String)>> {
    Ok(toolsets::ToolCatalog::load(config, config_path)?
        .declarations
        .into_iter()
        .map(|tool| {
            (
                tool.name,
                tool.toolset,
                tool.description.unwrap_or_default(),
            )
        })
        .collect())
}

pub fn supports_native_tool(name: &str) -> bool {
    matches!(
        name,
        "read_artifact"
            | "artifact_create"
            | "artifact_update"
            | "artifact_delete"
            | "glob"
            | "grep"
            | "search_local_files"
            | "list_connectors"
            | "get_connector_details"
            | "list_connector_resources_tool"
            | "read_connector_resource_tool"
            | "search_toolset"
            | "get_meta_info"
    )
}

/// Build the selected ForgeIQ agent with an ADK Rust model client.
pub fn build_agent(
    agent: &AgentDefinition,
    config: &ForgeConfig,
    config_path: &Path,
) -> Result<Arc<dyn Agent>> {
    let (configured_name, provider) = selected_model(agent, config)?;
    let mut native_tools = toolsets::native_tools(agent, config, config_path)?;
    native_tools.retain(|tool| {
        let setting = agent
            .tools
            .iter()
            .find(|entry| entry.get("id").and_then(serde_json::Value::as_str) == Some(tool.name()))
            .or_else(|| {
                config.agents.tools.iter().find(|entry| {
                    entry.get("id").and_then(serde_json::Value::as_str) == Some(tool.name())
                })
            });
        let enabled = setting
            .and_then(|entry| entry.get("enabled"))
            .and_then(serde_json::Value::as_bool)
            != Some(false);
        let allowlist = agent
            .other
            .get("tool-allowlist")
            .and_then(serde_json::Value::as_array);
        enabled
            && allowlist
                .is_none_or(|list| list.iter().any(|name| name.as_str() == Some(tool.name())))
    });
    let (skills_context, skill_tool) = skills::local_skills(agent, config, config_path)?;
    let delegate = delegation::delegation_tool(agent, config, config_path);
    let has_tools = !native_tools.is_empty() || skill_tool.is_some() || delegate.is_some();
    let build_single = |configured_name: &str, provider: &str| -> Result<Arc<dyn Llm>> {
        let model: Arc<dyn Llm> = match provider {
            "openai" => {
                let name = configured_name
                    .strip_prefix("openai/responses/")
                    .or_else(|| configured_name.strip_prefix("openai/"))
                    .unwrap_or(configured_name);
                let key = provider_key(config, config_path, provider, "OPENAI_API_KEY")?;
                let (responses, effort) = openai_route(agent, config, configured_name, has_tools)?;
                let base_url = config
                    .models
                    .as_ref()
                    .and_then(|m| m.providers.iter().find(|p| p.name == provider))
                    .and_then(|p| p.url.as_deref())
                    .filter(|url| !url.is_empty());
                if responses {
                    let mut settings = OpenAIResponsesConfig::new(&key, name);
                    if let Some(url) = base_url {
                        settings = settings.with_base_url(url);
                    }
                    if let Some(effort) = effort {
                        Arc::new(OpenAIResponsesClient::new_with_reasoning_effort(
                            settings, effort,
                        )?)
                    } else {
                        Arc::new(OpenAIResponsesClient::new(settings)?)
                    }
                } else {
                    let mut settings = adk_rust::model::openai::OpenAIConfig::new(&key, name);
                    if let Some(url) = base_url {
                        settings.base_url = Some(url.to_owned());
                    }
                    if let Some(effort) = effort {
                        Arc::new(
                            adk_rust::model::openai::OpenAIClient::new_with_reasoning_effort(
                                settings, effort,
                            )?,
                        )
                    } else {
                        Arc::new(adk_rust::model::openai::OpenAIClient::new(settings)?)
                    }
                }
            }
            "google" | "gemini" => {
                let name = configured_name
                    .strip_prefix("gemini/")
                    .or_else(|| configured_name.strip_prefix("google/"))
                    .unwrap_or(configured_name);
                let key = provider_key(config, config_path, provider, "GOOGLE_API_KEY")?;
                Arc::new(adk_rust::model::GeminiModel::new(&key, name)?)
            }
            "openrouter" => {
                let name = configured_name
                    .strip_prefix("openrouter/")
                    .unwrap_or(configured_name);
                let key = provider_key(config, config_path, provider, "OPENROUTER_API_KEY")?;
                let mut settings = adk_rust::model::openrouter::OpenRouterConfig::new(key, name);
                if let Some(url) = config
                    .models
                    .as_ref()
                    .and_then(|m| m.providers.iter().find(|p| p.name == provider))
                    .and_then(|p| p.url.as_deref())
                    .filter(|s| !s.is_empty())
                {
                    settings = settings.with_base_url(url);
                }
                Arc::new(adk_rust::model::openrouter::OpenRouterClient::new(
                    settings,
                )?)
            }
            "anthropic" => {
                let name = configured_name
                    .strip_prefix("anthropic/")
                    .unwrap_or(configured_name);
                let key = provider_key(config, config_path, provider, "ANTHROPIC_API_KEY")?;
                let mut settings = adk_rust::model::anthropic::AnthropicConfig::new(key, name);
                if let Some(url) = config
                    .models
                    .as_ref()
                    .and_then(|m| m.providers.iter().find(|p| p.name == provider))
                    .and_then(|p| p.url.as_deref())
                    .filter(|s| !s.is_empty())
                {
                    settings = settings.with_base_url(url);
                }
                Arc::new(adk_rust::model::anthropic::AnthropicClient::new(settings)?)
            }
            "ollama" => {
                let name = configured_name
                    .strip_prefix("ollama/")
                    .unwrap_or(configured_name);
                let host = config
                    .models
                    .as_ref()
                    .and_then(|m| m.providers.iter().find(|p| p.name == provider))
                    .and_then(|p| p.url.as_deref())
                    .filter(|s| !s.is_empty());
                let settings = if let Some(host) = host {
                    adk_rust::model::ollama::OllamaConfig::with_host(host, name)
                } else {
                    adk_rust::model::ollama::OllamaConfig::new(name)
                };
                Arc::new(adk_rust::model::ollama::OllamaModel::new(settings)?)
            }
            other => bail!("model provider '{other}' is not supported by this Rust build"),
        };
        Ok(model)
    };
    let mut candidates = Vec::new();
    let mut routes = vec![(configured_name.to_owned(), provider.to_owned())];
    if agent
        .models
        .first()
        .is_none_or(|model| model.name == "auto")
    {
        if let Some(models) = &config.models {
            for model in &models.models {
                if model.other.get("enabled").and_then(|value| value.as_bool()) == Some(false) {
                    continue;
                }
                let route = (model.name.clone(), model.provider.clone());
                if !routes.contains(&route) {
                    routes.push(route);
                }
            }
        }
    } else {
        for model in agent.models.iter().skip(1) {
            if model.name != "auto" {
                let route = (
                    model.name.clone(),
                    model.provider.clone().unwrap_or_else(|| "openai".into()),
                );
                if !routes.contains(&route) {
                    routes.push(route);
                }
            }
        }
    }
    let mut first_error = None;
    for (name, provider) in routes {
        match build_single(&name, &provider) {
            Ok(model) => candidates.push(model),
            Err(error) if first_error.is_none() => first_error = Some(error),
            Err(_) => {}
        }
    }
    if candidates.is_empty() {
        return Err(first_error.unwrap_or_else(|| anyhow::anyhow!("no model route is available")));
    }
    let model: Arc<dyn Llm> = if candidates.len() == 1 {
        candidates.remove(0)
    } else {
        Arc::new(router::RoutingLlm::new(candidates))
    };
    let mut instruction = instructions(agent, config_path, config)?;
    instruction.push_str(&skills_context);
    let mut builder = LlmAgentBuilder::new(&agent.id)
        .description(agent.description.as_deref().unwrap_or(&agent.name))
        .instruction(instruction)
        .model(model);
    let mut confirmations = adk_rust::ToolConfirmationPolicy::Never;
    for tool in &native_tools {
        if tool.is_read_only() {
            continue;
        }
        let setting = agent
            .tools
            .iter()
            .find(|entry| entry.get("id").and_then(serde_json::Value::as_str) == Some(tool.name()))
            .or_else(|| {
                config.agents.tools.iter().find(|entry| {
                    entry.get("id").and_then(serde_json::Value::as_str) == Some(tool.name())
                })
            });
        let autoapprove = setting
            .and_then(|entry| entry.get("autoapprove"))
            .and_then(serde_json::Value::as_bool)
            == Some(true)
            || setting
                .and_then(|entry| entry.get("mode"))
                .and_then(serde_json::Value::as_str)
                == Some("auto");
        if !autoapprove {
            confirmations = confirmations.with_tool(tool.name());
        }
    }
    builder = builder.tool_confirmation_policy(confirmations);
    for tool in native_tools {
        builder = builder.tool(tool);
    }
    if let Some(skill_tool) = skill_tool {
        builder = builder.tool(skill_tool);
    }
    if let Some(delegate) = delegate {
        builder = builder.tool(delegate);
    }
    let built = builder.build()?;
    Ok(Arc::new(built))
}

/// Choose the OpenAI endpoint from the model flag, then the ForgeIQ model name.
/// Chat Completions tool calls on newer reasoning models require `none` effort.
fn openai_route(
    agent: &AgentDefinition,
    config: &ForgeConfig,
    configured_name: &str,
    has_tools: bool,
) -> Result<(bool, Option<OpenAIReasoningEffort>)> {
    let definition = config.models.as_ref().and_then(|models| {
        models
            .models
            .iter()
            .find(|model| model.provider == "openai" && model.name == configured_name)
    });
    let override_flag = agent
        .models
        .first()
        .filter(|model| model.name == configured_name)
        .and_then(|model| model.other.get("use_responses"))
        .and_then(|value| value.as_bool());
    let responses = override_flag
        .or_else(|| definition.and_then(|model| model.use_responses))
        .unwrap_or_else(|| configured_name.starts_with("openai/responses/"));
    let name = configured_name
        .strip_prefix("openai/responses/")
        .or_else(|| configured_name.strip_prefix("openai/"))
        .unwrap_or(configured_name);
    if responses {
        let effort = definition
            .and_then(|model| model.other.get("reasoning_default"))
            .and_then(|value| value.as_str())
            .map(parse_reasoning_effort)
            .transpose()?;
        return Ok((true, effort));
    }
    let requires_none = has_tools
        && (name.starts_with("gpt-5.4")
            || name.starts_with("gpt-5.5")
            || name.starts_with("gpt-5.6")
            || name.starts_with("gpt-6"));
    if requires_none {
        let none_supported = definition
            .and_then(|model| model.other.get("reasoning_levels"))
            .and_then(|levels| levels.as_array())
            .map(|levels| levels.iter().any(|level| level.as_str() == Some("none")))
            .unwrap_or_else(|| {
                !(name.starts_with("gpt-6.2-") || name == "gpt-6-astra" || name == "gpt-6.1-sol")
            });
        if !none_supported {
            bail!(
                "OpenAI model '{name}' cannot call tools through Chat Completions with reasoning; set use_responses: true on the model"
            );
        }
        return Ok((false, Some(OpenAIReasoningEffort::None)));
    }
    Ok((false, None))
}

fn parse_reasoning_effort(value: &str) -> Result<OpenAIReasoningEffort> {
    match value {
        "none" => Ok(OpenAIReasoningEffort::None),
        "minimal" => Ok(OpenAIReasoningEffort::Minimal),
        "low" => Ok(OpenAIReasoningEffort::Low),
        "medium" => Ok(OpenAIReasoningEffort::Medium),
        "high" => Ok(OpenAIReasoningEffort::High),
        "xhigh" => Ok(OpenAIReasoningEffort::XHigh),
        "max" => Ok(OpenAIReasoningEffort::Max),
        _ => bail!("unsupported OpenAI reasoning_default '{value}'"),
    }
}

fn provider_key(
    config: &ForgeConfig,
    config_path: &Path,
    provider: &str,
    env_name: &str,
) -> Result<String> {
    std::env::var(env_name)
        .ok()
        .filter(|s| !s.is_empty())
        .or_else(|| {
            crate::credentials::CredentialStore::open(config_path)
                .ok()?
                .token_for(provider)
        })
        .or_else(|| {
            config
                .models
                .as_ref()?
                .providers
                .iter()
                .find(|p| p.name == provider)?
                .token
                .clone()
                .filter(|s| !s.is_empty())
        })
        .with_context(|| format!("{env_name} is required for provider {provider}"))
}

#[derive(Debug, Clone)]
pub struct AgentTurn {
    pub text: String,
    pub tools: Vec<String>,
}

/// Execute one prompt with a fresh session, for workflow and schedule jobs.
pub async fn ask_once(
    config: &ForgeConfig,
    config_path: &Path,
    agent_id: &str,
    prompt: &str,
) -> Result<String> {
    let turns = run_conversation(config, config_path, agent_id, &[prompt.to_owned()]).await?;
    Ok(turns
        .into_iter()
        .next()
        .map(|turn| turn.text)
        .unwrap_or_default())
}

pub async fn run_conversation(
    config: &ForgeConfig,
    config_path: &Path,
    agent_id: &str,
    prompts: &[String],
) -> Result<Vec<AgentTurn>> {
    let session = AgentSession::new(config, config_path, agent_id).await?;
    let mut turns = Vec::with_capacity(prompts.len());
    for prompt in prompts {
        turns.push(session.send(prompt).await?);
    }
    Ok(turns)
}

/// A session independent of terminal and desktop presentation. Reuse it for
/// successive prompts to retain ADK conversation context in headless workers.
pub struct AgentSession {
    runner: Runner,
    session_id: String,
}

impl AgentSession {
    pub async fn new(config: &ForgeConfig, config_path: &Path, agent_id: &str) -> Result<Self> {
        let definition = crate::agents::find_agent(config, agent_id)?;
        let agent = build_agent(definition, config, config_path)?;
        let service: Arc<dyn SessionService> = Arc::new(InMemorySessionService::new());
        let session = service
            .create(CreateRequest {
                app_name: "taiq-agent".into(),
                user_id: "worker".into(),
                session_id: None,
                state: HashMap::new(),
            })
            .await?;
        let runner = Runner::builder()
            .app_name("taiq-agent")
            .agent(agent)
            .session_service(service)
            .build()?;
        Ok(Self {
            runner,
            session_id: session.id().to_owned(),
        })
    }

    pub async fn send(&self, prompt: &str) -> Result<AgentTurn> {
        let mut stream = self
            .runner
            .run_str(
                "worker",
                &self.session_id,
                Content::new("user").with_text(prompt),
            )
            .await?;
        let mut deltas = adk_rust::EventTextDeltas::default();
        let mut turn = AgentTurn {
            text: String::new(),
            tools: Vec::new(),
        };
        while let Some(event) = stream.next().await {
            let raw = event?;
            let event = deltas.push(&raw);
            if let Some(content) = event.content() {
                for part in &content.parts {
                    match part {
                        Part::Text { text } => turn.text.push_str(text),
                        Part::FunctionCall { name, .. } => turn.tools.push(name.clone()),
                        _ => {}
                    }
                }
            }
        }
        Ok(turn)
    }
}

#[cfg(test)]
mod model_route_tests {
    use super::*;
    use crate::config::load_config_from_env;

    #[test]
    fn routes_forgeiq_responses_models_and_guards_chat_tools() {
        let path =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../orqestra/forgeiq/config/config.yaml");
        let mut config = load_config_from_env(path).unwrap();
        let agent = config
            .agents
            .agents
            .iter()
            .find(|agent| agent.default == Some(true))
            .unwrap()
            .clone();
        let name = "openai/responses/gpt-6.2-sol";
        assert_eq!(
            openai_route(&agent, &config, name, true).unwrap(),
            (true, Some(OpenAIReasoningEffort::Medium))
        );
        let model = config
            .models
            .as_mut()
            .unwrap()
            .models
            .iter_mut()
            .find(|model| model.name == name)
            .unwrap();
        model.use_responses = Some(false);
        assert!(
            openai_route(&agent, &config, name, true)
                .unwrap_err()
                .to_string()
                .contains("set use_responses: true")
        );

        let name = "openai/responses/gpt-5.6-terra";
        let model = config
            .models
            .as_mut()
            .unwrap()
            .models
            .iter_mut()
            .find(|model| model.name == name)
            .unwrap();
        model.use_responses = Some(false);
        assert_eq!(
            openai_route(&agent, &config, name, true).unwrap(),
            (false, Some(OpenAIReasoningEffort::None))
        );
    }
}
