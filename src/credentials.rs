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
    pub agent_id: Option<String>,
    pub registered_at: Option<String>,
}

fn credentials_path() -> Result<PathBuf> {
    let home = dirs::home_dir().context("cannot determine home directory")?;
    Ok(home.join(".corall").join("credentials.json"))
}

pub fn load() -> Result<Vec<Credential>> {
    let path = credentials_path()?;
    if !path.exists() {
        return Ok(vec![]);
    }
    let content =
        fs::read_to_string(&path).with_context(|| format!("failed to read {}", path.display()))?;
    let creds: Vec<Credential> = serde_json::from_str(&content)
        .with_context(|| format!("failed to parse {}", path.display()))?;
    Ok(creds)
}

pub fn save(creds: &[Credential]) -> Result<()> {
    let path = credentials_path()?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let content = serde_json::to_string_pretty(creds)?;
    fs::write(&path, content).with_context(|| format!("failed to write {}", path.display()))?;
    // chmod 600
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600))?;
    }
    Ok(())
}

pub fn resolve(site: Option<&str>) -> Result<Credential> {
    let creds = load()?;
    if creds.is_empty() {
        bail!("no credentials found — run `corall auth register <site>` first");
    }
    match site {
        Some(s) => creds
            .into_iter()
            .find(|c| c.site == s)
            .with_context(|| format!("no credentials found for site: {s}")),
        None if creds.len() == 1 => Ok(creds.into_iter().next().unwrap()),
        None => bail!(
            "multiple sites found — specify one with --site: {}",
            creds
                .iter()
                .map(|c| c.site.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        ),
    }
}

pub fn site_to_base_url(site: &str) -> String {
    if site.starts_with("http://") || site.starts_with("https://") {
        site.trim_end_matches('/').to_string()
    } else {
        format!("https://{site}")
    }
}

pub fn upsert(cred: Credential) -> Result<()> {
    let mut creds = load()?;
    if let Some(existing) = creds.iter_mut().find(|c| c.site == cred.site) {
        *existing = cred;
    } else {
        creds.push(cred);
    }
    save(&creds)
}

pub fn set_agent_id(site: &str, agent_id: &str) -> Result<()> {
    let mut creds = load()?;
    let entry = creds
        .iter_mut()
        .find(|c| c.site == site)
        .with_context(|| format!("no credentials found for site: {site}"))?;
    entry.agent_id = Some(agent_id.to_string());
    save(&creds)
}
