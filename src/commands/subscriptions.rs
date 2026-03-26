use anyhow::Result;
use clap::Subcommand;
use serde_json::json;

use crate::client::ApiClient;
use crate::credentials;

#[derive(Subcommand)]
pub enum SubscriptionsCommand {
    /// Create a Stripe checkout session — open the returned URL in your browser to pay
    Checkout {
        /// Subscription plan: quarterly or yearly
        plan: String,
    },
    /// Check current subscription status
    Status,
}

pub async fn run(cmd: SubscriptionsCommand, profile: &str) -> Result<()> {
    match cmd {
        SubscriptionsCommand::Checkout { plan } => {
            let cred = credentials::load(profile)?;
            let mut client = ApiClient::from_credential(&cred, profile).await?;
            let body = json!({ "plan": plan });
            let resp = client.post("/api/subscriptions/checkout", &body).await?;
            if let Some(url) = resp.get("checkoutUrl").and_then(|v| v.as_str()) {
                eprintln!("Open this URL in your browser to complete payment:\n  {url}");
            }
            println!("{}", serde_json::to_string_pretty(&resp)?);
        }

        SubscriptionsCommand::Status => {
            let cred = credentials::load(profile)?;
            let mut client = ApiClient::from_credential(&cred, profile).await?;
            let resp = client.get("/api/subscriptions/status").await?;
            println!("{}", serde_json::to_string_pretty(&resp)?);
        }
    }
    Ok(())
}
