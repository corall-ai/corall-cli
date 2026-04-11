use anyhow::Result;
use clap::Subcommand;
use ed25519_dalek::SigningKey;
use rand_core::OsRng;
use serde_json::json;

use crate::client::ApiClient;
use crate::credentials;
use crate::credentials::Credential;
use crate::credentials::site_to_base_url;

#[derive(Subcommand)]
pub enum AuthCommand {
    /// Register a new account with a generated Ed25519 keypair
    Register {
        /// Site hostname (e.g. corall.example.com)
        site: String,
        /// Display name
        #[arg(long)]
        name: String,
    },
    /// Show current authenticated user info
    Me,
    /// Remove saved credentials
    Remove,
}

pub async fn run(cmd: AuthCommand, profile: &str) -> Result<()> {
    match cmd {
        AuthCommand::Register { site, name } => {
            // Generate a new Ed25519 keypair.
            let signing_key = SigningKey::generate(&mut OsRng);
            let verifying_key = signing_key.verifying_key();
            let public_key_hex = hex::encode(verifying_key.to_bytes());
            let private_key_hex = hex::encode(signing_key.to_bytes());

            let mut client = ApiClient::new(site_to_base_url(&site));
            let body = json!({ "publicKey": public_key_hex, "name": name });
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
            let token = resp.get("token").and_then(|v| v.as_str()).map(|s| s.to_string());
            let token_expires_at = token.as_ref().map(|_| {
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_secs() as i64)
                    .unwrap_or(0)
                    + 7 * 24 * 3600
            });

            credentials::save(profile, &Credential {
                site,
                public_key: public_key_hex,
                private_key: private_key_hex,
                user_id,
                agent_id: None,
                registered_at,
                token,
                token_expires_at,
            })?;

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
