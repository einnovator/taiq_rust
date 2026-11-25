use anyhow::Result;
use clap::Parser;
use taiq::cli::AgentCli;

#[tokio::main]
async fn main() -> Result<()> {
    let cli = AgentCli::parse();
    taiq::cli::execute(cli.options, cli.command).await
}
