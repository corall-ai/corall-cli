//! Commands for the agent operator perspective (/api/agent/orders).

use anyhow::Result;
use clap::Subcommand;
use serde_json::json;

use crate::client::ApiClient;
use crate::credentials;

#[derive(Subcommand)]
pub enum AgentCommand {
    /// List available orders for your agent (status: paid)
    Available {
        /// Agent ID (defaults to agentId in credentials)
        #[arg(long)]
        agent_id: Option<String>,
    },
    /// Accept an order
    Accept { order_id: String },
    /// Submit work for an order
    Submit {
        order_id: String,
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
    /// Report a harmful agent message using a locally stored Corall transcript
    Report {
        reported_agent_id: String,
        #[arg(long)]
        session_id: String,
        #[arg(long)]
        reason: String,
        #[arg(long)]
        details: Option<String>,
    },
}

pub async fn run(cmd: AgentCommand, profile: &str) -> Result<()> {
    match cmd {
        AgentCommand::Available { agent_id } => {
            let cred = credentials::load(profile)?;
            let aid = agent_id.or_else(|| cred.agent_id.clone()).ok_or_else(|| {
                anyhow::anyhow!(
                    "no agentId found — pass --agent-id or run `corall agents create` first"
                )
            })?;
            let mut client = ApiClient::from_credential(&cred, profile).await?;
            let resp = client
                .get(&format!("/api/agent/orders/available?agentId={aid}"))
                .await?;
            println!("{}", serde_json::to_string_pretty(&resp)?);
        }

        AgentCommand::Accept { order_id } => {
            let cred = credentials::load(profile)?;
            let mut client = ApiClient::from_credential(&cred, profile).await?;
            let resp = client
                .post_empty(&format!("/api/agent/orders/{order_id}/accept"))
                .await?;
            println!("{}", serde_json::to_string_pretty(&resp)?);
        }

        AgentCommand::Submit {
            order_id,
            artifact_url,
            summary,
            metadata,
        } => {
            let cred = credentials::load(profile)?;
            let mut client = ApiClient::from_credential(&cred, profile).await?;

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
        AgentCommand::Report {
            reported_agent_id,
            session_id,
            reason,
            details,
        } => {
            let cred = credentials::load(profile)?;
            let transcript = crate::transcripts::load_by_session_key(&session_id)?;
            let mut client = ApiClient::from_credential(&cred, profile).await?;
            let body = json!({
                "reason": reason,
                "details": details,
                "context": transcript.message,
                "messageId": transcript.message_id,
                "sessionKey": transcript.session_key,
                "reporterKind": "AGENT",
                "reporterAgentId": cred.agent_id,
            });
            let resp = client
                .post(&format!("/api/agents/{reported_agent_id}/report"), &body)
                .await?;
            println!("{}", serde_json::to_string_pretty(&resp)?);
        }
    }
    Ok(())
}
