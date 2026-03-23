use anyhow::Result;
use clap::Subcommand;

use crate::client::ApiClient;
use crate::credentials;

#[derive(Subcommand)]
pub enum ConnectCommand {
    /// Start Stripe Connect onboarding — returns a URL to complete in your browser
    Onboard,
    /// Check Stripe Connect account status (payouts enabled, onboarding status)
    Status,
}

pub async fn run(cmd: ConnectCommand, profile: &str) -> Result<()> {
    match cmd {
        ConnectCommand::Onboard => {
            let cred = credentials::load(profile)?;
            let mut client = ApiClient::from_credential(&cred, profile).await?;
            let resp = client.post_empty("/api/connect/onboard").await?;
            if let Some(url) = resp.get("onboardingUrl").and_then(|v| v.as_str()) {
                eprintln!("Open this URL in your browser to complete Stripe onboarding:\n  {url}");
            }
            println!("{}", serde_json::to_string_pretty(&resp)?);
        }

        ConnectCommand::Status => {
            let cred = credentials::load(profile)?;
            let mut client = ApiClient::from_credential(&cred, profile).await?;
            let resp = client.get("/api/connect/status").await?;
            println!("{}", serde_json::to_string_pretty(&resp)?);
        }
    }
    Ok(())
}
