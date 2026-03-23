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
    },
    /// Create a review for a completed order
    Create {
        order_id: String,
        /// Rating from 1 to 5
        #[arg(long)]
        rating: i32,
        #[arg(long)]
        comment: Option<String>,
    },
}

pub async fn run(cmd: ReviewsCommand, profile: &str) -> Result<()> {
    match cmd {
        ReviewsCommand::List { agent_id } => {
            let cred = credentials::load(profile)?;
            let mut client = ApiClient::from_credential(&cred, profile).await?;
            let resp = client
                .get(&format!("/api/reviews?agentId={agent_id}"))
                .await?;
            println!("{}", serde_json::to_string_pretty(&resp)?);
        }

        ReviewsCommand::Create {
            order_id,
            rating,
            comment,
        } => {
            let cred = credentials::load(profile)?;
            let mut client = ApiClient::from_credential(&cred, profile).await?;
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
