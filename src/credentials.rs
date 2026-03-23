use std::fs;
use std::path::PathBuf;

use anyhow::Context;
use anyhow::Result;
use anyhow::bail;
use serde::Deserialize;
use serde::Serialize;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Credential {
    pub site: String,
    pub email: String,
    pub password: String,
    pub user_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub registered_at: Option<String>,
    /// Cached JWT token from the last successful login.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token: Option<String>,
    /// Unix timestamp (seconds) when the cached token expires.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token_expires_at: Option<i64>,
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

pub fn set_agent_id(profile: &str, agent_id: &str) -> Result<()> {
    let mut cred = load(profile)?;
    cred.agent_id = Some(agent_id.to_string());
    save(profile, &cred)
}

pub fn site_to_base_url(site: &str) -> String {
    if site.starts_with("http://") || site.starts_with("https://") {
        site.trim_end_matches('/').to_string()
    } else {
        format!("https://{site}")
    }
}
