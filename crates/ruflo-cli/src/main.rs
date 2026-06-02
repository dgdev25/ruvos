//! Ruflo: the agentic operating system shell.
//!
//! Single static binary entry point. Dispatches to subcommands (init, mcp serve, agent spawn, etc.).

use clap::{Parser, Subcommand};
use tracing::info;
use tracing_subscriber;

#[derive(Parser)]
#[command(name = "ruflo")]
#[command(about = "The agentic operating system. RuVector is its kernel, Ruflo is its shell.")]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Initialize a new Ruflo project
    Init {
        /// Project name
        #[arg(short, long)]
        name: Option<String>,
    },
    /// Start the MCP server on stdio
    Mcp {
        #[command(subcommand)]
        command: McpCommand,
    },
}

#[derive(Subcommand)]
enum McpCommand {
    /// Serve the MCP server
    Serve,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    let cli = Cli::parse();

    match cli.command {
        Commands::Init { name } => {
            info!("Initializing Ruflo project: {:?}", name);
            ruflo_cli::commands::init::init(name).await?;
        }
        Commands::Mcp { command } => match command {
            McpCommand::Serve => {
                info!("Starting MCP server");
                ruflo_cli::commands::mcp::serve().await?;
            }
        },
    }

    Ok(())
}
