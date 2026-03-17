use anyhow::Result;
use clap::Subcommand;
use serde_json::Value;
use serde_json::json;

use crate::client::ApiClient;
use crate::credentials;

#[derive(Subcommand)]
pub enum OrdersCommand {
    /// List orders
    List {
        /// Filter by status (CREATED, IN_PROGRESS, SUBMITTED, COMPLETED, DISPUTED)
        #[arg(long)]
        status: Option<String>,
        /// View mode: employer, developer, or default
        #[arg(long)]
        view: Option<String>,
        #[arg(long, default_value = "1")]
        page: u64,
        #[arg(long, default_value = "20")]
        limit: u64,
    },
    /// Get a single order by ID
    Get { id: String },
    /// Create a new order for an agent
    Create {
        agent_id: String,
        /// JSON string for inputPayload
        #[arg(long)]
        input: Option<String>,
    },
    /// Approve a submitted order
    Approve { id: String },
    /// Dispute a submitted order
    Dispute { id: String },
}

pub async fn run(cmd: OrdersCommand) -> Result<()> {
    match cmd {
        OrdersCommand::List {
            status,
            view,
            page,
            limit,
        } => {
            let cred = credentials::load()?;
            let mut client = ApiClient::from_credential(&cred).await?;
            let mut params = vec![format!("page={page}"), format!("limit={limit}")];
            if let Some(s) = status {
                params.push(format!("status={s}"));
            }
            if let Some(v) = view {
                params.push(format!("view={v}"));
            }
            let path = format!("/api/orders?{}", params.join("&"));
            let resp = client.get(&path).await?;
            println!("{}", serde_json::to_string_pretty(&resp)?);
        }

        OrdersCommand::Get { id } => {
            let cred = credentials::load()?;
            let mut client = ApiClient::from_credential(&cred).await?;
            let resp = client.get(&format!("/api/orders/{id}")).await?;
            println!("{}", serde_json::to_string_pretty(&resp)?);
        }

        OrdersCommand::Create { agent_id, input } => {
            let cred = credentials::load()?;
            let mut client = ApiClient::from_credential(&cred).await?;
            let mut body = json!({ "agentId": agent_id });
            if let Some(s) = input {
                body["inputPayload"] = serde_json::from_str::<Value>(&s)?;
            }
            let resp = client.post("/api/orders", &body).await?;
            println!("{}", serde_json::to_string_pretty(&resp)?);
        }

        OrdersCommand::Approve { id } => {
            let cred = credentials::load()?;
            let mut client = ApiClient::from_credential(&cred).await?;
            let resp = client
                .post_empty(&format!("/api/orders/{id}/approve"))
                .await?;
            println!("{}", serde_json::to_string_pretty(&resp)?);
        }

        OrdersCommand::Dispute { id } => {
            let cred = credentials::load()?;
            let mut client = ApiClient::from_credential(&cred).await?;
            let resp = client
                .post_empty(&format!("/api/orders/{id}/dispute"))
                .await?;
            println!("{}", serde_json::to_string_pretty(&resp)?);
        }
    }
    Ok(())
}
