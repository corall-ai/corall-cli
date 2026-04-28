use std::fs;
use std::path::PathBuf;

use anyhow::Context;
use anyhow::Result;
use anyhow::bail;
use ring::rand::SystemRandom;
use ring::signature::Ed25519KeyPair;
use ring::signature::KeyPair;
use serde::Deserialize;
use serde::Serialize;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Credential {
    pub site: String,
    pub user: CredentialUser,
    pub private_key_pkcs8: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub polling_token: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub registered_at: Option<String>,
    /// Cached JWT token from the last successful login.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token: Option<String>,
    /// Unix timestamp (seconds) when the cached token expires.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token_expires_at: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CredentialUser {
    pub id: String,
    pub public_key: String,
}

pub struct GeneratedKey {
    pub private_key_pkcs8: String,
    pub public_key: String,
}

impl Credential {
    /// Returns the cached token if it is still valid (with a 5-minute buffer).
    pub fn cached_token(&self) -> Option<&str> {
        let token = self.token.as_deref()?;
        let expires_at = self.token_expires_at?;
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
        if expires_at > now + 300 {
            Some(token)
        } else {
            None
        }
    }
}

pub fn generate_key() -> Result<GeneratedKey> {
    let rng = SystemRandom::new();
    let private_key = Ed25519KeyPair::generate_pkcs8(&rng)
        .map_err(|_| anyhow::anyhow!("failed to generate Ed25519 keypair"))?;
    let key_pair = Ed25519KeyPair::from_pkcs8(private_key.as_ref())
        .map_err(|_| anyhow::anyhow!("failed to read generated Ed25519 keypair"))?;

    Ok(GeneratedKey {
        private_key_pkcs8: hex::encode(private_key.as_ref()),
        public_key: hex::encode(key_pair.public_key().as_ref()),
    })
}

pub fn sign_challenge(private_key_pkcs8: &str, challenge: &str) -> Result<String> {
    let private_key = hex::decode(private_key_pkcs8).context("invalid privateKeyPkcs8 hex")?;
    let challenge = hex::decode(challenge).context("invalid challenge hex")?;
    let key_pair = Ed25519KeyPair::from_pkcs8(&private_key)
        .map_err(|_| anyhow::anyhow!("invalid Ed25519 private key"))?;
    Ok(hex::encode(key_pair.sign(&challenge).as_ref()))
}

pub fn remove(profile: &str) -> Result<bool> {
    let path = credentials_path(profile)?;
    if path.exists() {
        fs::remove_file(&path)?;
        Ok(true)
    } else {
        Ok(false)
    }
}

/// Returns the path for a named profile: ~/.corall/credentials/<profile>.json
fn credentials_path(profile: &str) -> Result<PathBuf> {
    let home = dirs::home_dir().context("cannot determine home directory")?;
    Ok(home
        .join(".corall")
        .join("credentials")
        .join(format!("{profile}.json")))
}

pub fn load(profile: &str) -> Result<Credential> {
    let path = credentials_path(profile)?;
    if !path.exists() {
        bail!(
            "no credentials found for profile '{profile}' — run `corall auth login <site> --profile {profile}` first"
        );
    }
    let content =
        fs::read_to_string(&path).with_context(|| format!("failed to read {}", path.display()))?;
    serde_json::from_str(&content).with_context(|| format!("failed to parse {}", path.display()))
}

pub fn save(profile: &str, cred: &Credential) -> Result<()> {
    let path = credentials_path(profile)?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let content = serde_json::to_string_pretty(cred)?;
    fs::write(&path, content).with_context(|| format!("failed to write {}", path.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600))?;
    }
    Ok(())
}

pub fn update_agent_registration(
    profile: &str,
    agent_id: Option<&str>,
    polling_token: Option<&str>,
) -> Result<()> {
    let mut cred = load(profile)?;
    if let Some(agent_id) = agent_id {
        cred.agent_id = Some(agent_id.to_string());
    }
    if let Some(polling_token) = polling_token {
        cred.polling_token = Some(polling_token.to_string());
    }
    save(profile, &cred)
}

pub fn site_to_base_url(site: &str) -> String {
    if site.starts_with("http://") || site.starts_with("https://") {
        site.trim_end_matches('/').to_string()
    } else {
        format!("https://{site}")
    }
}

#[cfg(test)]
mod tests {
    use ring::signature;
    use serde_json::json;

    use super::*;

    #[test]
    fn generated_key_signs_challenge() {
        let key = generate_key().unwrap();
        let challenge = hex::encode(b"challenge");
        let signature_hex = sign_challenge(&key.private_key_pkcs8, &challenge).unwrap();
        let signature = hex::decode(signature_hex).unwrap();
        signature::UnparsedPublicKey::new(
            &signature::ED25519,
            hex::decode(key.public_key).unwrap(),
        )
        .verify(b"challenge", &signature)
        .unwrap();
    }

    #[test]
    fn credential_serializes_expected_schema() {
        let credential = Credential {
            site: "http://corall.test".to_string(),
            user: CredentialUser {
                id: "user-1".to_string(),
                public_key: "a".repeat(64),
            },
            private_key_pkcs8: "b".repeat(64),
            agent_id: Some("agent-1".to_string()),
            polling_token: Some("polling-token".to_string()),
            registered_at: Some("2026-04-20T00:00:00Z".to_string()),
            token: Some("token".to_string()),
            token_expires_at: Some(1_776_000_000),
        };

        assert_eq!(
            serde_json::to_value(credential).unwrap(),
            json!({
                "site": "http://corall.test",
                "user": {
                    "id": "user-1",
                    "publicKey": "a".repeat(64),
                },
                "privateKeyPkcs8": "b".repeat(64),
                "agentId": "agent-1",
                "pollingToken": "polling-token",
                "registeredAt": "2026-04-20T00:00:00Z",
                "token": "token",
                "tokenExpiresAt": 1776000000,
            })
        );
    }
}
