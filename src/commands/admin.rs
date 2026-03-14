use anyhow::Result;
use clap::Subcommand;
use serde_json::json;

use crate::client::ApiClient;
use crate::credentials;

#[derive(Subcommand)]
pub enum AdminCommand {
    /// Show admin dashboard (all users, agents, orders, disputes)
    Dashboard {
        #[arg(long)]
        site: Option<String>,
    },
    /// Suspend a user or agent
    Suspend {
        #[arg(long)]
        site: Option<String>,
        /// "user" or "agent"
        kind: String,
        id: String,
    },
    /// Activate a suspended user or agent
    Activate {
        #[arg(long)]
        site: Option<String>,
        /// "user" or "agent"
        kind: String,
        id: String,
    },
    /// Resolve a disputed order (refunds employer)
    ResolveDispute {
        order_id: String,
        #[arg(long)]
        site: Option<String>,
    },
    /// List all payout ledger entries
    Payouts {
        #[arg(long)]
        site: Option<String>,
    },
    /// Create a payout entry
    Payout {
        user_id: String,
        amount: f64,
        #[arg(long)]
        site: Option<String>,
    },
}

pub async fn run(cmd: AdminCommand) -> Result<()> {
    match cmd {
        AdminCommand::Dashboard { site } => {
            let cred = credentials::resolve(site.as_deref())?;
            let client = ApiClient::from_credential(&cred).await?;
            let resp = client.get("/api/admin").await?;
            println!("{}", serde_json::to_string_pretty(&resp)?);
        }

        AdminCommand::Suspend { site, kind, id } => {
            let cred = credentials::resolve(site.as_deref())?;
            let client = ApiClient::from_credential(&cred).await?;
            let action = format!("suspend_{kind}");
            let resp = client
                .put("/api/admin", &json!({ "action": action, "id": id }))
                .await?;
            println!("{}", serde_json::to_string_pretty(&resp)?);
        }

        AdminCommand::Activate { site, kind, id } => {
            let cred = credentials::resolve(site.as_deref())?;
            let client = ApiClient::from_credential(&cred).await?;
            let action = format!("activate_{kind}");
            let resp = client
                .put("/api/admin", &json!({ "action": action, "id": id }))
                .await?;
            println!("{}", serde_json::to_string_pretty(&resp)?);
        }

        AdminCommand::ResolveDispute { order_id, site } => {
            let cred = credentials::resolve(site.as_deref())?;
            let client = ApiClient::from_credential(&cred).await?;
            let resp = client
                .put(
                    "/api/admin",
                    &json!({ "action": "resolve_dispute", "id": order_id }),
                )
                .await?;
            println!("{}", serde_json::to_string_pretty(&resp)?);
        }

        AdminCommand::Payouts { site } => {
            let cred = credentials::resolve(site.as_deref())?;
            let client = ApiClient::from_credential(&cred).await?;
            let resp = client.get("/api/admin/payouts").await?;
            println!("{}", serde_json::to_string_pretty(&resp)?);
        }

        AdminCommand::Payout {
            user_id,
            amount,
            site,
        } => {
            let cred = credentials::resolve(site.as_deref())?;
            let client = ApiClient::from_credential(&cred).await?;
            let resp = client
                .post(
                    "/api/admin/payouts",
                    &json!({ "userId": user_id, "amount": amount }),
                )
                .await?;
            println!("{}", serde_json::to_string_pretty(&resp)?);
        }
    }
    Ok(())
}
