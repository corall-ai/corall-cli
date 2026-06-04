use std::fs;
use std::path::Component;
use std::path::Path;
use std::path::PathBuf;

use anyhow::Context;
use anyhow::Result;
use anyhow::bail;
use clap::Subcommand;
use serde_json::Value;
use serde_json::json;

use crate::client::ApiClient;
use crate::credentials;

#[derive(Subcommand)]
pub enum SkillPackagesCommand {
    /// Print the required agent-generated skill package form template
    FormTemplate,
    /// Create a paid skill package for one of your agents
    Create {
        #[arg(long)]
        agent_id: String,
        /// JSON skill payload
        #[arg(long)]
        skills: String,
        /// Price in cents
        #[arg(long)]
        price: i64,
    },
    /// List skill packages you created
    Mine,
    /// Get a single skill package by ID
    Get { id: String },
    /// Purchase a skill package through Stripe Checkout
    Purchase { id: String },
    /// List skill packages purchased by the current user
    Purchased,
    /// Install a purchased skill package into OpenClaw
    Install {
        id: String,
        /// OpenClaw directory that contains the skills/ folder
        #[arg(long)]
        openclaw_dir: Option<PathBuf>,
        /// Replace an existing local skill directory
        #[arg(long)]
        force: bool,
    },
    /// Delete one of your skill packages
    Delete { id: String },
}

pub async fn run(cmd: SkillPackagesCommand, profile: &str) -> Result<()> {
    match cmd {
        SkillPackagesCommand::FormTemplate => {
            println!(
                "{}",
                serde_json::to_string_pretty(&skill_package_form_template())?
            );
        }
        SkillPackagesCommand::Create {
            agent_id,
            skills,
            price,
        } => {
            let cred = credentials::load(profile)?;
            let mut client = ApiClient::from_credential(&cred, profile).await?;
            let body = json!({
                "agentId": agent_id,
                "skills": serde_json::from_str::<Value>(&skills)?,
                "price": price,
            });
            let resp = client.post("/api/skill-packages", &body).await?;
            println!("{}", serde_json::to_string_pretty(&resp)?);
        }
        SkillPackagesCommand::Mine => {
            let cred = credentials::load(profile)?;
            let mut client = ApiClient::from_credential(&cred, profile).await?;
            let resp = client.get("/api/skill-packages/mine").await?;
            println!("{}", serde_json::to_string_pretty(&resp)?);
        }
        SkillPackagesCommand::Get { id } => {
            let cred = credentials::load(profile)?;
            let mut client = ApiClient::from_credential(&cred, profile).await?;
            let resp = client.get(&format!("/api/skill-packages/{id}")).await?;
            println!("{}", serde_json::to_string_pretty(&resp)?);
        }
        SkillPackagesCommand::Purchase { id } => {
            let cred = credentials::load(profile)?;
            let mut client = ApiClient::from_credential(&cred, profile).await?;
            let resp = client
                .post_empty(&format!("/api/skill-packages/{id}/purchase"))
                .await?;
            println!("{}", serde_json::to_string_pretty(&resp)?);
        }
        SkillPackagesCommand::Purchased => {
            let cred = credentials::load(profile)?;
            let mut client = ApiClient::from_credential(&cred, profile).await?;
            let resp = client.get("/api/skill-packages/purchased").await?;
            println!("{}", serde_json::to_string_pretty(&resp)?);
        }
        SkillPackagesCommand::Install {
            id,
            openclaw_dir,
            force,
        } => {
            let cred = credentials::load(profile)?;
            let mut client = ApiClient::from_credential(&cred, profile).await?;
            let resp = client.get("/api/skill-packages/purchased").await?;
            let package = find_purchased_package(&resp, &id)?;
            let installed = install_skill_package(package, openclaw_dir, force)?;
            println!("{}", serde_json::to_string_pretty(&installed)?);
        }
        SkillPackagesCommand::Delete { id } => {
            let cred = credentials::load(profile)?;
            let mut client = ApiClient::from_credential(&cred, profile).await?;
            let status = client.delete(&format!("/api/skill-packages/{id}")).await?;
            println!(
                "{}",
                serde_json::to_string_pretty(&json!({
                    "deleted": status.is_success(),
                    "status": status.as_u16(),
                }))?
            );
        }
    }
    Ok(())
}

fn skill_package_form_template() -> Value {
    json!({
        "version": 1,
        "generatedBy": "agent",
        "category": {
            "primary": "Development",
            "secondary": "CLI & Terminal"
        },
        "description": {
            "summary": "A concise description of what this skill does and the problems it solves.",
            "activationTriggers": [
                "Use when the user asks for the workflow this skill enables."
            ],
            "keywords": ["keyword", "workflow", "domain"]
        },
        "functions": [
            {
                "name": "Primary function name",
                "description": "Concrete action the skill performs, including expected input and output."
            }
        ],
            "permissions": {
            "env": [
                {
                    "name": "EXAMPLE_API_KEY",
                    "required": false,
                    "sensitive": true,
                    "purpose": "Authenticate to the declared external service."
                }
            ],
            "network": [
                {
                    "domain": "api.example.com",
                    "purpose": "Call the declared external API."
                }
            ],
            "filesystem": [
                {
                    "access": "read_write",
                    "scope": "workspace",
                    "purpose": "Read inputs and write generated artifacts inside the workspace."
                }
            ],
            "tools": [
                {
                    "name": "curl",
                    "purpose": "Make documented API requests."
                }
            ],
            "install": {
                "hasInstallSteps": false,
                "manualReviewRequired": false
            },
            "persistence": {
                "requiresBackgroundService": false,
                "requiresElevatedPrivileges": false
            }
        },
        "source": {
            "name": "example-skill",
            "files": [
                {
                    "path": "SKILL.md",
                    "content": "---\nname: example-skill\ndescription: Use when the user asks for the workflow this skill enables.\n---\n# Example Skill\n"
                }
            ],
            "metadata": {
                "version": "1.0.0"
            }
        }
    })
}

fn find_purchased_package<'a>(resp: &'a Value, id: &str) -> Result<&'a Value> {
    let packages = resp
        .get("packages")
        .and_then(Value::as_array)
        .context("purchased response did not contain packages array")?;
    packages
        .iter()
        .find(|package| package.get("id").and_then(Value::as_str) == Some(id))
        .with_context(|| {
            format!(
                "skill package {id} is not purchased by this profile or payment is not complete"
            )
        })
}

fn install_skill_package(
    package: &Value,
    openclaw_dir: Option<PathBuf>,
    force: bool,
) -> Result<Value> {
    let package_id = package
        .get("id")
        .and_then(Value::as_str)
        .context("package missing id")?;
    let source = package
        .pointer("/skills/source")
        .and_then(Value::as_object)
        .context("skill package does not include installable source files; republish it with skills.source.files")?;
    let skill_name = source
        .get("name")
        .and_then(Value::as_str)
        .context("skills.source.name missing")?;
    validate_skill_name(skill_name)?;
    let files = source
        .get("files")
        .and_then(Value::as_array)
        .filter(|files| !files.is_empty())
        .context("skills.source.files must contain at least one file")?;

    let root = openclaw_dir.unwrap_or(default_openclaw_dir()?);
    let skills_dir = root.join("skills");
    let skill_dir = skills_dir.join(skill_name);

    if skill_dir.exists() {
        if !force {
            bail!(
                "skill directory already exists at {}; pass --force to replace it",
                skill_dir.display()
            );
        }
        fs::remove_dir_all(&skill_dir)
            .with_context(|| format!("failed to remove {}", skill_dir.display()))?;
    }
    fs::create_dir_all(&skill_dir)
        .with_context(|| format!("failed to create {}", skill_dir.display()))?;

    let mut written = Vec::new();
    let mut has_skill_md = false;
    for (idx, file) in files.iter().enumerate() {
        let object = file
            .as_object()
            .with_context(|| format!("skills.source.files[{idx}] must be an object"))?;
        let path = object
            .get("path")
            .and_then(Value::as_str)
            .with_context(|| format!("skills.source.files[{idx}].path missing"))?;
        if path == "SKILL.md" {
            has_skill_md = true;
        }
        let relative = safe_relative_path(path)?;
        let content = object
            .get("content")
            .and_then(Value::as_str)
            .with_context(|| format!("skills.source.files[{idx}].content must be a string"))?;
        let target = skill_dir.join(&relative);
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("failed to create {}", parent.display()))?;
        }
        fs::write(&target, content)
            .with_context(|| format!("failed to write {}", target.display()))?;
        let executable = object
            .get("executable")
            .and_then(Value::as_bool)
            .unwrap_or(false)
            || object.get("mode").and_then(Value::as_str) == Some("0755");
        set_file_permissions(&target, executable)?;
        written.push(path.to_string());
    }

    if !has_skill_md {
        bail!("skills.source.files must include SKILL.md");
    }

    let manifest_path = skill_dir.join(".corall-package.json");
    let manifest = json!({
        "packageId": package_id,
        "skillName": skill_name,
        "installedAt": unix_timestamp(),
        "purchasedAt": package.get("purchasedAt").cloned().unwrap_or(Value::Null),
        "metadata": source.get("metadata").cloned().unwrap_or(Value::Null),
        "state": source.get("state").cloned().unwrap_or(Value::Null),
    });
    fs::write(&manifest_path, serde_json::to_string_pretty(&manifest)?)
        .with_context(|| format!("failed to write {}", manifest_path.display()))?;
    written.push(".corall-package.json".to_string());

    Ok(json!({
        "installed": true,
        "packageId": package_id,
        "skillName": skill_name,
        "path": skill_dir,
        "files": written,
    }))
}

fn default_openclaw_dir() -> Result<PathBuf> {
    Ok(dirs::home_dir()
        .context("cannot determine home directory")?
        .join(".openclaw"))
}

fn validate_skill_name(name: &str) -> Result<()> {
    if name.is_empty()
        || name == "."
        || name == ".."
        || name.contains('/')
        || name.contains('\\')
        || !name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        bail!("skills.source.name must be a safe single directory name");
    }
    Ok(())
}

fn safe_relative_path(path: &str) -> Result<PathBuf> {
    let candidate = Path::new(path);
    let mut out = PathBuf::new();
    for component in candidate.components() {
        match component {
            Component::Normal(part) => out.push(part),
            _ => bail!("invalid source file path: {path}"),
        }
    }
    if out.as_os_str().is_empty() {
        bail!("invalid source file path: {path}");
    }
    Ok(out)
}

fn set_file_permissions(path: &Path, executable: bool) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = if executable { 0o755 } else { 0o644 };
        fs::set_permissions(path, fs::Permissions::from_mode(mode))
            .with_context(|| format!("failed to chmod {}", path.display()))?;
    }
    #[cfg(not(unix))]
    {
        let _ = (path, executable);
    }
    Ok(())
}

fn unix_timestamp() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn installs_skill_package_source_files() {
        let root = test_dir("install-source");
        let _ = fs::remove_dir_all(&root);
        let package = package_with_source();

        let result = install_skill_package(&package, Some(root.clone()), false).unwrap();

        assert_eq!(result["installed"], true);
        let skill_dir = root.join("skills").join("public-ip");
        assert!(skill_dir.join("SKILL.md").is_file());
        assert!(skill_dir.join("scripts").join("public-ip.sh").is_file());
        assert!(skill_dir.join(".corall-package.json").is_file());
        let manifest = fs::read_to_string(skill_dir.join(".corall-package.json")).unwrap();
        assert!(manifest.contains("pkg_123"));
        assert!(manifest.contains("persisted-state"));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn refuses_overwrite_without_force() {
        let root = test_dir("install-overwrite");
        let _ = fs::remove_dir_all(&root);
        let package = package_with_source();
        install_skill_package(&package, Some(root.clone()), false).unwrap();

        let err = install_skill_package(&package, Some(root.clone()), false).unwrap_err();
        assert!(err.to_string().contains("already exists"));

        install_skill_package(&package, Some(root.clone()), true).unwrap();
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn rejects_metadata_only_packages() {
        let package = json!({
            "id": "pkg_123",
            "skills": {
                "version": 1,
                "description": { "summary": "metadata only" }
            }
        });
        let err =
            install_skill_package(&package, Some(test_dir("missing-source")), false).unwrap_err();
        assert!(err.to_string().contains("installable source files"));
    }

    fn package_with_source() -> Value {
        json!({
            "id": "pkg_123",
            "purchasedAt": "2026-04-23T00:00:00Z",
            "skills": {
                "source": {
                    "name": "public-ip",
                    "metadata": { "version": "1.0.0" },
                    "state": { "value": "persisted-state" },
                    "files": [
                        {
                            "path": "SKILL.md",
                            "content": "---\nname: public-ip\ndescription: Detect public IP.\n---\n# Public IP\n"
                        },
                        {
                            "path": "scripts/public-ip.sh",
                            "content": "#!/usr/bin/env bash\ncurl -fsSL https://api.ipify.org\n",
                            "mode": "0755"
                        }
                    ]
                }
            }
        })
    }

    fn test_dir(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("corall-cli-{name}-{}", std::process::id()))
    }
}
