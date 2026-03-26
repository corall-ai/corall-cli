use anyhow::Result;
use clap::Subcommand;
use serde_json::json;

use crate::client::ApiClient;
use crate::credentials;
use crate::credentials::Credential;
use crate::credentials::site_to_base_url;

#[derive(Subcommand)]
pub enum AuthCommand {
    /// Register a new account and save credentials
    Register {
        /// Site hostname (e.g. corall.example.com)
        site: String,
        /// Email address
        #[arg(long)]
        email: String,
        /// Password (min 6 characters)
        #[arg(long)]
        password: String,
        /// Display name
        #[arg(long)]
        name: String,
    },
    /// Login to an existing account (refreshes local credentials)
    Login {
        /// Site hostname
        site: String,
        /// Email address
        #[arg(long)]
        email: String,
        /// Password
        #[arg(long)]
        password: String,
    },
    /// Show current authenticated user info
    Me,
    /// Remove saved credentials
    Remove,
}

pub async fn run(cmd: AuthCommand, profile: &str) -> Result<()> {
    match cmd {
        AuthCommand::Register {
            site,
            email,
            password,
            name,
        } => {
            let mut client = ApiClient::new(site_to_base_url(&site));
            let body = json!({ "email": email, "password": password, "name": name });
            let resp = client.post("/api/auth/register", &body).await?;

            let user = resp.get("user").cloned().unwrap_or_default();
            let user_id = user
                .get("id")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let registered_at = user
                .get("createdAt")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());

            credentials::save(
                profile,
                &Credential {
                    site,
                    email,
                    password,
                    user_id,
                    agent_id: None,
                    registered_at,
                    token: None,
                    token_expires_at: None,
                },
            )?;

            println!("{}", serde_json::to_string_pretty(&resp)?);
        }

        AuthCommand::Login {
            site,
            email,
            password,
        } => {
            let mut client = ApiClient::new(site_to_base_url(&site));
            let body = json!({ "email": email, "password": password });
            let resp = client.post("/api/auth/login", &body).await?;

            let user = resp.get("user").cloned().unwrap_or_default();
            let user_id = user
                .get("id")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();

            // Preserve existing agentId if already set for this profile, site, and email.
            let agent_id = credentials::load(profile)
                .ok()
                .filter(|c| c.site == site && c.email == email)
                .and_then(|c| c.agent_id);

            credentials::save(
                profile,
                &Credential {
                    site,
                    email,
                    password,
                    user_id,
                    agent_id,
                    registered_at: None,
                    token: None,
                    token_expires_at: None,
                },
            )?;

            println!("{}", serde_json::to_string_pretty(&resp)?);
        }

        AuthCommand::Me => {
            let cred = credentials::load(profile)?;
            let mut client = ApiClient::from_credential(&cred, profile).await?;
            let resp = client.get("/api/auth/me").await?;
            println!("{}", serde_json::to_string_pretty(&resp)?);
        }

        AuthCommand::Remove => {
            let removed = credentials::remove(profile)?;
            println!("{}", json!({ "removed": removed }));
        }
    }
    Ok(())
}
