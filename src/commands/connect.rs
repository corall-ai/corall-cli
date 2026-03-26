use anyhow::Result;
use clap::Subcommand;
use reqwest::StatusCode;

use crate::client::ApiClient;
use crate::credentials;

#[derive(Subcommand)]
pub enum ConnectCommand {
    /// Start Stripe Connect onboarding — returns a URL to complete in your browser
    Onboard,
    /// Check Stripe Connect account status (payouts enabled, onboarding status)
    Status,
    /// Transfer pending earnings from completed orders to your Stripe account
    Payout,
}

/// Print onboarding URL hint if the response is a 402 with onboardingUrl.
fn handle_onboarding_required(status: StatusCode, body: &serde_json::Value) -> bool {
    if status == StatusCode::PAYMENT_REQUIRED {
        if let Some(url) = body.get("onboardingUrl").and_then(|v| v.as_str()) {
            eprintln!(
                "Stripe Connect onboarding required. Open this URL in your browser:\n  {url}"
            );
        }
        if let Some(err) = body.get("error").and_then(|v| v.as_str()) {
            eprintln!("  {err}");
        }
        return true;
    }
    false
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
            let (status, body) = client.get_raw("/api/connect/status").await?;
            if handle_onboarding_required(status, &body) {
                return Ok(());
            }
            if !status.is_success() {
                let msg = body
                    .get("error")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown error");
                anyhow::bail!("HTTP {status}: {msg}");
            }
            println!("{}", serde_json::to_string_pretty(&body)?);
        }

        ConnectCommand::Payout => {
            let cred = credentials::load(profile)?;
            let mut client = ApiClient::from_credential(&cred, profile).await?;
            let (status, body) = client.post_empty_raw("/api/connect/payout").await?;
            if handle_onboarding_required(status, &body) {
                return Ok(());
            }
            if !status.is_success() {
                let msg = body
                    .get("error")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown error");
                anyhow::bail!("HTTP {status}: {msg}");
            }
            println!("{}", serde_json::to_string_pretty(&body)?);
        }
    }
    Ok(())
}
