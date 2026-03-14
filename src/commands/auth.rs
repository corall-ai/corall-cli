use anyhow::Result;
use clap::Subcommand;
use serde_json::json;

use crate::client::ApiClient;
use crate::credentials::Credential;
use crate::credentials::{self};

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
    Me {
        /// Site hostname (required if multiple sites configured)
        #[arg(long)]
        site: Option<String>,
    },
    /// List all locally stored credential entries
    List,
    /// Remove a credential entry
    Remove {
        /// Site hostname to remove
        site: String,
    },
}

pub async fn run(cmd: AuthCommand) -> Result<()> {
    match cmd {
        AuthCommand::Register {
            site,
            email,
            password,
            name,
        } => {
            let client = ApiClient::new(format!("https://{site}"));
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

            credentials::upsert(Credential {
                site: site.clone(),
                email,
                password,
                user_id,
                agent_id: None,
                registered_at,
            })?;

            println!("{}", serde_json::to_string_pretty(&resp)?);
        }

        AuthCommand::Login {
            site,
            email,
            password,
        } => {
            let client = ApiClient::new(format!("https://{site}"));
            let body = json!({ "email": email, "password": password });
            let resp = client.post("/api/auth/login", &body).await?;

            let user = resp.get("user").cloned().unwrap_or_default();
            let user_id = user
                .get("id")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();

            // Preserve existing agentId if already set
            let existing = credentials::load()?;
            let agent_id = existing
                .iter()
                .find(|c| c.site == site)
                .and_then(|c| c.agent_id.clone());

            credentials::upsert(Credential {
                site,
                email,
                password,
                user_id,
                agent_id,
                registered_at: None,
            })?;

            println!("{}", serde_json::to_string_pretty(&resp)?);
        }

        AuthCommand::Me { site } => {
            let cred = credentials::resolve(site.as_deref())?;
            let client = ApiClient::from_credential(&cred).await?;
            let resp = client.get("/api/auth/me").await?;
            println!("{}", serde_json::to_string_pretty(&resp)?);
        }

        AuthCommand::List => {
            let creds = credentials::load()?;
            // Omit passwords from output
            let sanitized: Vec<_> = creds
                .iter()
                .map(|c| {
                    json!({
                        "site": c.site,
                        "email": c.email,
                        "userId": c.user_id,
                        "agentId": c.agent_id,
                        "registeredAt": c.registered_at,
                    })
                })
                .collect();
            println!("{}", serde_json::to_string_pretty(&sanitized)?);
        }

        AuthCommand::Remove { site } => {
            let mut creds = credentials::load()?;
            let before = creds.len();
            creds.retain(|c| c.site != site);
            if creds.len() == before {
                anyhow::bail!("no credentials found for site: {site}");
            }
            credentials::save(&creds)?;
            println!("{}", json!({ "removed": site }));
        }
    }
    Ok(())
}
