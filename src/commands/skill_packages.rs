use anyhow::Result;
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
        }
    })
}
