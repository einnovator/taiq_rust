use anyhow::Result;
use clap::Parser;
use taiq::cli::{UiCli, UiCommand, UiOptions};

fn main() -> Result<()> {
    let cli = UiCli::parse();
    if cli.options.list {
        return run_command(cli.options, None);
    }
    match cli.command.unwrap_or(UiCommand::Ui(UiOptions::default())) {
        UiCommand::Ui(options) => {
            let controller =
                taiq::cli::desktop_controller(cli.options.config, options.agent.as_deref())?;
            taiq::ui::gpui::run(controller);
            Ok(())
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
