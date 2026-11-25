mod probe;

use crate::agents::{self, adk::AgentSession};
use crate::config::load_config_from_env;
use crate::schema::{AgentDefinition, ForgeConfig};
use anyhow::{Context, Result, bail};
use clap::{Args, Parser, Subcommand};
use serde_json::{Value, json};
use std::collections::HashSet;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use tokio::io::{AsyncBufReadExt, BufReader};

#[derive(Debug, Clone, Args)]
pub struct ConfigOptions {
    /// ForgeIQ config YAML path.
    #[arg(short = 'f', long = "config", global = true, env = "TAIQ_CONFIG")]
    pub config: Option<PathBuf>,
    /// List enabled agents (compatibility with the previous CLI).
    #[arg(long = "list", global = true)]
    pub list: bool,
}

#[derive(Debug, Clone, Args)]
pub struct TestOptions {
    /// Also probe enabled connectors.
    #[arg(short = 'c', long = "connectors")]
    pub connectors: bool,
}

#[derive(Debug, Clone, Default, Args)]
pub struct ChatOptions {
    /// Run one prompt and exit.
    #[arg(short = 'q', long = "query", conflicts_with = "input")]
    pub query: Option<String>,
    /// Read prompts from a text, JSON, or JSON Lines file.
    #[arg(short = 'i', long = "input", conflicts_with = "query")]
    pub input: Option<PathBuf>,
    /// Agent ID or name; defaults to the configured default or first enabled agent.
    #[arg(short = 'a', long = "agent", env = "TAIQ_AGENT")]
    pub agent: Option<String>,
}

#[derive(Debug, Clone, Subcommand)]
pub enum CoreCommand {
    /// Validate config, imports, agents, models, and toolset manifests.
    Check,
    /// Test enabled model providers; -c also tests enabled connectors.
    Test(TestOptions),
    /// Chat with a named or default agent.
    Chat(ChatOptions),
}

#[derive(Debug, Parser)]
#[command(version, about = "Run ForgeIQ agents without a UI")]
pub struct AgentCli {
    #[command(flatten)]
    pub options: ConfigOptions,
    #[command(subcommand)]
    pub command: Option<CoreCommand>,
}

#[cfg(any(feature = "terminal-ui", feature = "desktop-admin"))]
#[derive(Debug, Clone, Default, Args)]
pub struct UiOptions {
    /// Open with this agent selected.
    #[arg(short = 'a', long = "agent", env = "TAIQ_AGENT")]
    pub agent: Option<String>,
}

#[cfg(any(feature = "terminal-ui", feature = "desktop-admin"))]
#[derive(Debug, Clone, Subcommand)]
pub enum UiCommand {
    /// Validate config, imports, agents, models, and toolset manifests.
    Check,
    /// Test enabled model providers; -c also tests enabled connectors.
    Test(TestOptions),
    /// Chat with a named or default agent.
    Chat(ChatOptions),
    /// Start this executable's user interface.
    Ui(UiOptions),
}

#[cfg(any(feature = "terminal-ui", feature = "desktop-admin"))]
impl UiCommand {
    pub fn into_core(self) -> Option<CoreCommand> {
        match self {
            Self::Check => Some(CoreCommand::Check),
            Self::Test(options) => Some(CoreCommand::Test(options)),
            Self::Chat(options) => Some(CoreCommand::Chat(options)),
            Self::Ui(_) => None,
        }
    }
}

#[cfg(any(feature = "terminal-ui", feature = "desktop-admin"))]
#[derive(Debug, Parser)]
#[command(version, about = "Run ForgeIQ agents or start the UI")]
pub struct UiCli {
    #[command(flatten)]
    pub options: ConfigOptions,
    #[command(subcommand)]
    pub command: Option<UiCommand>,
}

pub struct LoadedConfig {
    pub path: PathBuf,
    pub config: ForgeConfig,
}

pub fn load_config(config_path: Option<PathBuf>) -> Result<LoadedConfig> {
    dotenvy::dotenv().ok();
    let path = config_path.unwrap_or_else(|| {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../orqestra/forgeiq/config/config.yaml")
    });
    let path = path
        .canonicalize()
        .with_context(|| format!("config file {}", path.display()))?;
    if let Some(project) = path.parent().and_then(Path::parent) {
        dotenvy::from_path(project.join(".env")).ok();
    }
    let config = load_config_from_env(&path)?;
    Ok(LoadedConfig { path, config })
}

#[cfg(feature = "desktop-admin")]
pub fn desktop_controller(
    config_path: Option<PathBuf>,
    requested: Option<&str>,
) -> Result<crate::ui::common::UiController> {
    let loaded = load_config(config_path)?;
    let requested = requested
        .map(str::to_owned)
        .or_else(|| std::env::var("TAIQ_AGENT").ok());
    let agent_id = requested
        .as_deref()
        .map(|name| select_agent(&loaded.config, Some(name)).map(|agent| agent.id.clone()))
        .transpose()?;
    let mut controller = crate::ui::common::UiController::new(loaded.config, &loaded.path)?;
    if let Some(agent_id) = agent_id {
        controller.dispatch(crate::ui::common::UiEvent::SelectAgent(agent_id));
    }
    Ok(controller)
}

pub fn select_agent<'a>(
    config: &'a ForgeConfig,
    requested: Option<&str>,
) -> Result<&'a AgentDefinition> {
    if let Some(name) = requested {
        return agents::find_agent(config, name);
    }
    config
        .agents
        .agents
        .iter()
        .find(|agent| agent.enabled != Some(false) && agent.default == Some(true))
        .or_else(|| {
            config
                .agents
                .agents
                .iter()
                .find(|agent| agent.enabled != Some(false))
        })
        .context("config has no enabled agent")
}

pub async fn execute(options: ConfigOptions, command: Option<CoreCommand>) -> Result<()> {
    let loaded = load_config(options.config)?;
    if options.list {
        let agents: Vec<_> = loaded.config.agents.agents.iter()
            .filter(|agent| agent.enabled != Some(false))
            .map(|agent| json!({"id":agent.id,"name":agent.name,"default":agent.default == Some(true)}))
            .collect();
        return write_line(&json!(agents));
    }
    match command.unwrap_or(CoreCommand::Chat(ChatOptions::default())) {
        CoreCommand::Check => {
            validate_config(&loaded.config, &loaded.path)?;
            write_line(
                &json!({"ok":true,"config":loaded.path,"agents":loaded.config.agents.agents.len(),"connectors":loaded.config.connectors.len(),"models":loaded.config.models.as_ref().map(|m| m.models.len()).unwrap_or(0)}),
            )
        }
        CoreCommand::Test(options) => {
            probe::run(&loaded.config, &loaded.path, options.connectors).await
        }
        CoreCommand::Chat(options) => chat(&loaded.config, &loaded.path, options).await,
    }
}

pub fn validate_config(config: &ForgeConfig, path: &Path) -> Result<()> {
    let mut ids = HashSet::new();
    let mut enabled = 0;
    for agent in &config.agents.agents {
        if agent.id.trim().is_empty() || agent.name.trim().is_empty() {
            bail!("agent ID and name are required");
        }
        if !ids.insert(agent.id.to_ascii_lowercase()) {
            bail!("duplicate agent ID: {}", agent.id);
        }
        if agent.enabled != Some(false) {
            enabled += 1;
            agents::selected_model(agent, config)
                .with_context(|| format!("model for agent {}", agent.id))?;
            agents::instructions(agent, path, config)
                .with_context(|| format!("instructions for agent {}", agent.id))?;
        }
    }
    if enabled == 0 {
        bail!("config has no enabled agents");
    }
    let mut connector_ids = HashSet::new();
    for connector in &config.connectors {
        if let Some(id) = &connector.id {
            if id.trim().is_empty() || !connector_ids.insert(id.to_ascii_lowercase()) {
                bail!("duplicate or empty connector ID: {id}");
            }
        }
    }
    crate::agents::adk::validate_toolsets(config, path)?;
    Ok(())
}

#[derive(Debug, Clone)]
struct Prompt {
    id: Option<Value>,
    text: String,
}

impl Prompt {
    fn from_value(value: &Value) -> Result<Self> {
        match value {
            Value::String(text) => Self::from_text(text),
            Value::Object(object) => {
                let text = object
                    .get("prompt")
                    .and_then(Value::as_str)
                    .context("JSON prompt requires a string 'prompt' field")?;
                let mut prompt = Self::from_text(text)?;
                prompt.id = object.get("id").cloned();
                Ok(prompt)
            }
            _ => bail!("prompt must be a string or an object with a 'prompt' field"),
        }
    }

    fn from_text(text: &str) -> Result<Self> {
        if text.trim().is_empty() {
            bail!("prompt cannot be empty");
        }
        Ok(Self {
            id: None,
            text: text.to_owned(),
        })
    }

    fn from_line(line: &str) -> Result<Self> {
        if line.trim_start().starts_with('{') {
            Self::from_value(&serde_json::from_str::<Value>(line).context("invalid JSON prompt")?)
        } else {
            Self::from_text(line)
        }
    }
}

fn prompts_from_file(path: &Path) -> Result<Vec<Prompt>> {
    let source =
        std::fs::read_to_string(path).with_context(|| format!("read input {}", path.display()))?;
    let extension = path
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    match extension.as_str() {
        "json" => {
            let value: Value = serde_json::from_str(&source)
                .with_context(|| format!("parse JSON input {}", path.display()))?;
            let items = match &value {
                Value::Array(items) => items,
                Value::Object(object) if object.contains_key("prompts") => object
                    .get("prompts")
                    .and_then(Value::as_array)
                    .context("'prompts' must be an array")?,
                _ => return Ok(vec![Prompt::from_value(&value)?]),
            };
            if items.is_empty() {
                bail!("input file contains no prompts");
            }
            items.iter().map(Prompt::from_value).collect()
        }
        "jsonl" => {
            let prompts: Vec<_> = source
                .lines()
                .filter(|line| !line.trim().is_empty())
                .map(Prompt::from_line)
                .collect::<Result<_>>()?;
            if prompts.is_empty() {
                bail!("input file contains no prompts");
            }
            Ok(prompts)
        }
        _ => Ok(vec![Prompt::from_text(&source)?]),
    }
}

async fn chat(config: &ForgeConfig, path: &Path, options: ChatOptions) -> Result<()> {
    let requested = options.agent.or_else(|| std::env::var("TAIQ_AGENT").ok());
    let agent = select_agent(config, requested.as_deref())?;
    let session = AgentSession::new(config, path, &agent.id).await?;
    if let Some(query) = options.query {
        if !send(&session, &agent.id, Prompt::from_text(&query)?).await? {
            bail!("chat request failed");
        }
        return Ok(());
    }
    if let Some(input) = options.input {
        let mut failed = 0;
        for prompt in prompts_from_file(&input)? {
            if !send(&session, &agent.id, prompt).await? {
                failed += 1;
            }
        }
        if failed > 0 {
            bail!("{failed} chat requests failed");
        }
        return Ok(());
    }
    let mut lines = BufReader::new(tokio::io::stdin()).lines();
    let mut failed = 0;
    while let Some(line) = lines.next_line().await? {
        if line.trim().is_empty() {
            continue;
        }
        match Prompt::from_line(&line) {
            Ok(prompt) => {
                if !send(&session, &agent.id, prompt).await? {
                    failed += 1;
                }
            }
            Err(error) => {
                failed += 1;
                write_line(&json!({"agent":agent.id,"error":error.to_string()}))?
            }
        }
    }
    if failed > 0 {
        bail!("{failed} chat requests failed");
    }
    Ok(())
}

async fn send(session: &AgentSession, agent_id: &str, prompt: Prompt) -> Result<bool> {
    let (output, ok) = match session.send(&prompt.text).await {
        Ok(turn) => (
            json!({"id":prompt.id,"agent":agent_id,"text":turn.text,"tools":turn.tools}),
            true,
        ),
        Err(error) => (
            json!({"id":prompt.id,"agent":agent_id,"error":format!("{error:#}")}),
            false,
        ),
    };
    write_line(&output)?;
    Ok(ok)
}

pub fn write_line(value: &Value) -> Result<()> {
    let mut output = io::stdout().lock();
    serde_json::to_writer(&mut output, value)?;
    output.write_all(b"\n")?;
    output.flush()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn selects_default_then_first_enabled_agent() {
        let path =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../orqestra/forgeiq/config/config.yaml");
        let mut config = load_config_from_env(path).unwrap();
        config
            .agents
            .agents
            .retain(|agent| agent.enabled != Some(false));
        assert!(config.agents.agents.len() >= 2);
        for agent in &mut config.agents.agents {
            agent.default = None;
        }
        let first = config.agents.agents[0].id.clone();
        let second = config.agents.agents[1].id.clone();
        assert_eq!(select_agent(&config, None).unwrap().id, first);
        config.agents.agents[1].default = Some(true);
        assert_eq!(select_agent(&config, None).unwrap().id, second);
        assert_eq!(select_agent(&config, Some(&first)).unwrap().id, first);
        config.agents.agents[1].enabled = Some(false);
        assert_eq!(select_agent(&config, None).unwrap().id, first);
    }

    #[test]
    fn reads_text_and_json_prompts() {
        let root = std::env::temp_dir().join(format!("taiq-cli-test-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let text = root.join("prompt.txt");
        let json = root.join("prompts.json");
        let jsonl = root.join("prompts.jsonl");
        std::fs::write(&text, "first\nsecond").unwrap();
        std::fs::write(&json, r#"{"prompts":["one",{"id":2,"prompt":"two"}]}"#).unwrap();
        std::fs::write(&jsonl, "{\"id\":1,\"prompt\":\"one\"}\ntwo\n").unwrap();
        assert_eq!(prompts_from_file(&text).unwrap().len(), 1);
        let prompts = prompts_from_file(&json).unwrap();
        assert_eq!(prompts.len(), 2);
        assert_eq!(prompts[1].id, Some(json!(2)));
        assert_eq!(prompts_from_file(&jsonl).unwrap().len(), 2);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn clap_parses_agent_and_ui_commands() {
        let agent = AgentCli::try_parse_from([
            "taiq-agent",
            "-f",
            "config.yaml",
            "chat",
            "-q",
            "hello",
            "-a",
            "IQ",
        ])
        .unwrap();
        assert_eq!(agent.options.config, Some(PathBuf::from("config.yaml")));
        assert!(matches!(
            agent.command,
            Some(CoreCommand::Chat(ChatOptions {
                query: Some(_),
                agent: Some(_),
                ..
            }))
        ));
        let test = AgentCli::try_parse_from(["taiq-agent", "test", "-c"]).unwrap();
        assert!(matches!(
            test.command,
            Some(CoreCommand::Test(TestOptions { connectors: true }))
        ));
        #[cfg(any(feature = "terminal-ui", feature = "desktop-admin"))]
        {
            let ui = UiCli::try_parse_from(["taiq-egui", "ui", "-a", "IQ"]).unwrap();
            assert!(matches!(
                ui.command,
                Some(UiCommand::Ui(UiOptions { agent: Some(_) }))
            ));
        }
    }
}
