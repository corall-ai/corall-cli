use anyhow::Result;
use clap::Subcommand;
use serde_json::json;

use crate::client::ApiClient;
use crate::credentials;

#[derive(Subcommand)]
pub enum ReviewsCommand {
    /// List reviews for an agent
    List {
        #[arg(long)]
        agent_id: String,
        #[arg(long)]
        site: Option<String>,
    },
    /// Create a review for a completed order
    Create {
        order_id: String,
        #[arg(long)]
        site: Option<String>,
        /// Rating from 1 to 5
        #[arg(long)]
        rating: i32,
        #[arg(long)]
        comment: Option<String>,
    },
}

pub async fn run(cmd: ReviewsCommand) -> Result<()> {
    match cmd {
        ReviewsCommand::List { agent_id, site } => {
            let cred = credentials::resolve(site.as_deref())?;
            let client = ApiClient::from_credential(&cred).await?;
            let resp = client
                .get(&format!("/api/reviews?agentId={agent_id}"))
                .await?;
            println!("{}", serde_json::to_string_pretty(&resp)?);
        }

        ReviewsCommand::Create {
            order_id,
            site,
            rating,
            comment,
        } => {
            let cred = credentials::resolve(site.as_deref())?;
            let client = ApiClient::from_credential(&cred).await?;
            let mut body = json!({ "orderId": order_id, "rating": rating });
            if let Some(c) = comment {
                body["comment"] = json!(c);
            }
            let resp = client.post("/api/reviews", &body).await?;
            println!("{}", serde_json::to_string_pretty(&resp)?);
        }
    }
    Ok(())
}
