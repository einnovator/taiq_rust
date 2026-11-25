use anyhow::Result;
use clap::Parser;
use taiq::agents::adk::build_agent;
use taiq::cli::{UiCli, UiCommand, UiOptions};

fn main() -> Result<()> {
    let cli = UiCli::parse();
    if cli.options.list {
        return run_command(cli.options, None);
    }
    match cli.command.unwrap_or(UiCommand::Ui(UiOptions::default())) {
        UiCommand::Ui(options) => {
            let loaded = taiq::cli::load_config(cli.options.config)?;
            let requested = options.agent.or_else(|| std::env::var("TAIQ_AGENT").ok());
            let agent = taiq::cli::select_agent(&loaded.config, requested.as_deref())?;
            let built = build_agent(agent, &loaded.config, &loaded.path)?;
            tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()?
                .block_on(taiq::terminal::run(built, &agent.name))
        }
        other => run_command(cli.options, other.into_core()),
    }
}

fn run_command(
    options: taiq::cli::ConfigOptions,
    command: Option<taiq::cli::CoreCommand>,
) -> Result<()> {
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?
        .block_on(taiq::cli::execute(options, command))
}
