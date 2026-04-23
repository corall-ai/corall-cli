use anyhow::Result;
use clap::Subcommand;
use serde_json::json;

use crate::client::ApiClient;
use crate::credentials;
use crate::credentials::Credential;
use crate::credentials::CredentialUser;
use crate::credentials::site_to_base_url;

#[derive(Subcommand)]
pub enum AuthCommand {
    /// Register a new account and save credentials
    Register {
        /// Site hostname (e.g. corall.example.com)
        site: String,
        /// Legacy option accepted for compatibility; public-key auth does not use it.
        #[arg(long, hide = true)]
        email: Option<String>,
        /// Legacy option accepted for compatibility; public-key auth does not use it.
        #[arg(long, hide = true)]
        password: Option<String>,
        /// Display name
        #[arg(long)]
        name: String,
    },
    /// Login to an existing account (refreshes local credentials)
    Login {
        /// Site hostname
        site: String,
        /// Legacy option accepted for compatibility; public-key auth does not use it.
        #[arg(long, hide = true)]
        email: Option<String>,
        /// Legacy option accepted for compatibility; public-key auth does not use it.
        #[arg(long, hide = true)]
        password: Option<String>,
    },
    /// Approve a dashboard session with the local Ed25519 key
    Approve {
        /// Site hostname
        site: String,
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
            email: _,
            password: _,
            name,
        } => {
            let key = credentials::generate_key()?;
            let mut client = ApiClient::new(site_to_base_url(&site));
            let body = json!({ "publicKey": &key.public_key, "name": name });
            let resp = client.post("/api/auth/register", &body).await?;

            let user = resp.get("user").cloned().unwrap_or_default();
            let user_id = user
                .get("id")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let public_key = user
                .get("publicKey")
                .and_then(|v| v.as_str())
                .unwrap_or(&key.public_key)
                .to_string();
            let registered_at = user
                .get("createdAt")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            let token = resp
                .get("token")
                .and_then(|v| v.as_str())
                .map(str::to_owned);
            let token_expires_at = token.as_ref().map(|_| token_expiry_timestamp());

            credentials::save(
                profile,
                &Credential {
                    site,
                    user: CredentialUser {
                        id: user_id,
                        public_key,
                    },
                    private_key_pkcs8: key.private_key_pkcs8,
                    agent_id: None,
                    registered_at,
                    token,
                    token_expires_at,
                },
            )?;

            println!("{}", serde_json::to_string_pretty(&resp)?);
        }

        AuthCommand::Login {
            site,
            email: _,
            password: _,
        } => {
            let mut cred = credentials::load(profile)?;
            if cred.site != site {
                anyhow::bail!(
                    "credentials for profile '{profile}' belong to '{}', not '{site}'",
                    cred.site
                );
            }

            let client = ApiClient::new(site_to_base_url(&site));
            let token = client.login_with_key(&cred).await?;
            cred.token = Some(token);
            cred.token_expires_at = Some(token_expiry_timestamp());
            credentials::save(profile, &cred)?;

            let mut client = ApiClient::from_credential(&cred, profile).await?;
            let resp = client.get("/api/auth/me").await?;
            println!("{}", serde_json::to_string_pretty(&resp)?);
        }

        AuthCommand::Approve { site } => approve_dashboard_session(&site, profile).await?,

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

async fn approve_dashboard_session(site: &str, profile: &str) -> Result<()> {
    let cred = credentials::load(profile)?;
    if cred.site != site {
        anyhow::bail!(
            "credentials for profile '{profile}' belong to '{}', not '{site}'",
            cred.site
        );
    }

    let client = ApiClient::new(site_to_base_url(site));
    let resp = client.approve_agent_approval(&cred).await?;
    println!("{}", serde_json::to_string_pretty(&resp)?);
    Ok(())
}

fn token_expiry_timestamp() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
        + 7 * 24 * 3600
}
