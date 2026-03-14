//! Commands for the agent operator perspective (/api/agent/orders).

use anyhow::Result;
use clap::Subcommand;
use serde_json::json;

use crate::client::ApiClient;
use crate::credentials;

#[derive(Subcommand)]
pub enum AgentCommand {
    /// List available orders for your agent (status: CREATED)
    Available {
        #[arg(long)]
        site: Option<String>,
        /// Agent ID (defaults to agentId in credentials)
        #[arg(long)]
        agent_id: Option<String>,
    },
    /// Accept an order
    Accept {
        order_id: String,
        #[arg(long)]
        site: Option<String>,
    },
    /// Submit work for an order
    Submit {
        order_id: String,
        #[arg(long)]
        site: Option<String>,
        /// Public URL of the artifact (optional)
        #[arg(long)]
        artifact_url: Option<String>,
        /// Summary of what was done
        #[arg(long)]
        summary: Option<String>,
        /// Raw JSON metadata string (overrides --summary)
        #[arg(long)]
        metadata: Option<String>,
    },
}

pub async fn run(cmd: AgentCommand) -> Result<()> {
    match cmd {
        AgentCommand::Available { site, agent_id } => {
            let cred = credentials::resolve(site.as_deref())?;
            let aid = agent_id.or_else(|| cred.agent_id.clone()).ok_or_else(|| {
                anyhow::anyhow!(
                    "no agentId found — pass --agent-id or run `corall agents create` first"
                )
            })?;
            let client = ApiClient::from_credential(&cred).await?;
            let resp = client
                .get(&format!("/api/agent/orders/available?agentId={aid}"))
                .await?;
            println!("{}", serde_json::to_string_pretty(&resp)?);
        }

        AgentCommand::Accept { order_id, site } => {
            let cred = credentials::resolve(site.as_deref())?;
            let client = ApiClient::from_credential(&cred).await?;
            let resp = client
                .post_empty(&format!("/api/agent/orders/{order_id}/accept"))
                .await?;
            println!("{}", serde_json::to_string_pretty(&resp)?);
        }

        AgentCommand::Submit {
            order_id,
            site,
            artifact_url,
            summary,
            metadata,
        } => {
            let cred = credentials::resolve(site.as_deref())?;
            let client = ApiClient::from_credential(&cred).await?;

            let meta = if let Some(raw) = metadata {
                serde_json::from_str(&raw)?
            } else if let Some(s) = summary {
                json!({ "summary": s })
            } else {
                json!({})
            };

            let mut body = json!({ "metadata": meta });
            if let Some(url) = artifact_url {
                body["artifactUrl"] = json!(url);
            }

            let resp = client
                .post(&format!("/api/agent/orders/{order_id}/submit"), &body)
                .await?;
            println!("{}", serde_json::to_string_pretty(&resp)?);
        }
    }
    Ok(())
}
