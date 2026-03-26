mod client;
mod commands;
mod credentials;

use anyhow::Result;
use clap::Parser;
use clap::Subcommand;
use commands::agent;
use commands::agents;
use commands::auth;
use commands::connect;
use commands::openclaw;
use commands::orders;
use commands::reviews;
use commands::subscriptions;
use commands::upload;

#[derive(Parser)]
#[command(name = "corall", about = "Corall marketplace CLI", version)]
struct Cli {
    /// Credential profile to use (e.g. "default", "provider", "employer")
    #[arg(long, global = true, default_value = "default")]
    profile: String,

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
    /// Stripe Connect onboarding and status
    Connect {
        #[command(subcommand)]
        cmd: connect::ConnectCommand,
    },
    /// Manage reviews
    Reviews {
        #[command(subcommand)]
        cmd: reviews::ReviewsCommand,
    },
    /// Manage subscriptions
    Subscriptions {
        #[command(subcommand)]
        cmd: subscriptions::SubscriptionsCommand,
    },
    /// File upload helpers
    Upload {
        #[command(subcommand)]
        cmd: upload::UploadCommand,
    },
    /// OpenClaw integration helpers
    Openclaw {
        #[command(subcommand)]
        cmd: openclaw::OpenclawCommand,
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
    let profile = cli.profile.as_str();
    match cli.command {
        Command::Auth { cmd } => auth::run(cmd, profile).await,
        Command::Agents { cmd } => agents::run(cmd, profile).await,
        Command::Orders { cmd } => orders::run(cmd, profile).await,
        Command::Agent { cmd } => agent::run(cmd, profile).await,
        Command::Connect { cmd } => connect::run(cmd, profile).await,
        Command::Reviews { cmd } => reviews::run(cmd, profile).await,
        Command::Subscriptions { cmd } => subscriptions::run(cmd, profile).await,
        Command::Upload { cmd } => upload::run(cmd, profile).await,
        Command::Openclaw { cmd } => openclaw::run(cmd).await,
    }
}
