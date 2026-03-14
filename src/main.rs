mod client;
mod commands;
mod credentials;

use anyhow::Result;
use clap::Parser;
use clap::Subcommand;
use commands::agent;
use commands::agents;
use commands::auth;
use commands::orders;
use commands::reviews;
use commands::upload;

#[derive(Parser)]
#[command(name = "corall", about = "Corall marketplace CLI", version)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Authentication and credential management
    Auth {
        #[command(subcommand)]
        cmd: auth::AuthCommand,
    },
    /// Manage agents
    Agents {
        #[command(subcommand)]
        cmd: agents::AgentsCommand,
    },
    /// Manage orders (employer perspective)
    Orders {
        #[command(subcommand)]
        cmd: orders::OrdersCommand,
    },
    /// Agent operator commands — accept and submit orders
    Agent {
        #[command(subcommand)]
        cmd: agent::AgentCommand,
    },
    /// Manage reviews
    Reviews {
        #[command(subcommand)]
        cmd: reviews::ReviewsCommand,
    },
    /// File upload helpers
    Upload {
        #[command(subcommand)]
        cmd: upload::UploadCommand,
    },
}

#[tokio::main]
async fn main() {
    if let Err(e) = run().await {
        eprintln!("{}", serde_json::json!({ "error": e.to_string() }));
        std::process::exit(1);
    }
}

async fn run() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::Auth { cmd } => auth::run(cmd).await,
        Command::Agents { cmd } => agents::run(cmd).await,
        Command::Orders { cmd } => orders::run(cmd).await,
        Command::Agent { cmd } => agent::run(cmd).await,
        Command::Reviews { cmd } => reviews::run(cmd).await,
        Command::Upload { cmd } => upload::run(cmd).await,
    }
}
