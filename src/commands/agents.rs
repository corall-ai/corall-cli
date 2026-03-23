use anyhow::Result;
use clap::Subcommand;
use serde_json::Value;
use serde_json::json;

use crate::client::ApiClient;
use crate::credentials;

#[derive(Subcommand)]
pub enum AgentsCommand {
    /// List agents
    List {
        #[arg(long)]
        search: Option<String>,
        #[arg(long)]
        tag: Option<String>,
        #[arg(long)]
        min_price: Option<f64>,
        #[arg(long)]
        max_price: Option<f64>,
        #[arg(long)]
        sort_by: Option<String>,
        #[arg(long, default_value = "1")]
        page: u64,
        #[arg(long, default_value = "20")]
        limit: u64,
        /// Only show your own agents
        #[arg(long)]
        mine: bool,
        #[arg(long)]
        provider_id: Option<String>,
    },
    /// Get a single agent by ID
    Get { id: String },
    /// Create a new agent
    Create {
        #[arg(long)]
        name: String,
        #[arg(long)]
        description: Option<String>,
        #[arg(long, value_delimiter = ',')]
        tags: Vec<String>,
        #[arg(long)]
        price: Option<f64>,
        #[arg(long)]
        delivery_time: Option<i32>,
        #[arg(long)]
        webhook_url: Option<String>,
        #[arg(long)]
        webhook_token: Option<String>,
        /// JSON string for inputSchema
        #[arg(long)]
        input_schema: Option<String>,
        /// JSON string for outputSchema
        #[arg(long)]
        output_schema: Option<String>,
    },
    /// Update an agent
    Update {
        id: String,
        #[arg(long)]
        name: Option<String>,
        #[arg(long)]
        description: Option<String>,
        #[arg(long, value_delimiter = ',')]
        tags: Option<Vec<String>>,
        #[arg(long)]
        price: Option<f64>,
        #[arg(long)]
        delivery_time: Option<i32>,
        #[arg(long)]
        status: Option<String>,
        #[arg(long)]
        webhook_url: Option<String>,
        #[arg(long)]
        webhook_token: Option<String>,
    },
    /// Delete an agent
    Delete { id: String },
    /// Activate a draft agent
    Activate { id: String },
}

pub async fn run(cmd: AgentsCommand, profile: &str) -> Result<()> {
    match cmd {
        AgentsCommand::List {
            search,
            tag,
            min_price,
            max_price,
            sort_by,
            page,
            limit,
            mine,
            provider_id,
        } => {
            let cred = credentials::load(profile)?;
            let mut client = ApiClient::from_credential(&cred, profile).await?;
            let mut params = vec![format!("page={page}"), format!("limit={limit}")];
            if let Some(s) = search {
                params.push(format!("search={s}"));
            }
            if let Some(t) = tag {
                params.push(format!("tag={t}"));
            }
            if let Some(p) = min_price {
                params.push(format!("minPrice={p}"));
            }
            if let Some(p) = max_price {
                params.push(format!("maxPrice={p}"));
            }
            if let Some(s) = sort_by {
                params.push(format!("sortBy={s}"));
            }
            if mine {
                params.push("mine=true".to_string());
            }
            if let Some(d) = provider_id {
                params.push(format!("providerId={d}"));
            }
            let path = format!("/api/agents?{}", params.join("&"));
            let resp = client.get(&path).await?;
            println!("{}", serde_json::to_string_pretty(&resp)?);
        }

        AgentsCommand::Get { id } => {
            let cred = credentials::load(profile)?;
            let mut client = ApiClient::from_credential(&cred, profile).await?;
            let resp = client.get(&format!("/api/agents/{id}")).await?;
            println!("{}", serde_json::to_string_pretty(&resp)?);
        }

        AgentsCommand::Create {
            name,
            description,
            tags,
            price,
            delivery_time,
            webhook_url,
            webhook_token,
            input_schema,
            output_schema,
        } => {
            let cred = credentials::load(profile)?;
            let mut client = ApiClient::from_credential(&cred, profile).await?;

            let mut body = json!({ "name": name });
            if let Some(v) = description {
                body["description"] = json!(v);
            }
            if !tags.is_empty() {
                body["tags"] = json!(tags);
            }
            if let Some(v) = price {
                body["price"] = json!(v);
            }
            if let Some(v) = delivery_time {
                body["deliveryTime"] = json!(v);
            }
            if let Some(v) = webhook_url {
                body["webhookUrl"] = json!(v);
            }
            if let Some(v) = webhook_token {
                body["webhookToken"] = json!(v);
            }
            if let Some(s) = input_schema {
                body["inputSchema"] = serde_json::from_str::<Value>(&s)?;
            }
            if let Some(s) = output_schema {
                body["outputSchema"] = serde_json::from_str::<Value>(&s)?;
            }

            let resp = client.post("/api/agents", &body).await?;

            // Auto-save agentId to credentials
            if let Some(agent_id) = resp
                .get("agent")
                .and_then(|a| a.get("id"))
                .and_then(|v| v.as_str())
            {
                credentials::set_agent_id(profile, agent_id)?;
            }

            println!("{}", serde_json::to_string_pretty(&resp)?);
        }

        AgentsCommand::Update {
            id,
            name,
            description,
            tags,
            price,
            delivery_time,
            status,
            webhook_url,
            webhook_token,
        } => {
            let cred = credentials::load(profile)?;
            let mut client = ApiClient::from_credential(&cred, profile).await?;

            let mut body = json!({});
            if let Some(v) = name {
                body["name"] = json!(v);
            }
            if let Some(v) = description {
                body["description"] = json!(v);
            }
            if let Some(v) = tags {
                body["tags"] = json!(v);
            }
            if let Some(v) = price {
                body["price"] = json!(v);
            }
            if let Some(v) = delivery_time {
                body["deliveryTime"] = json!(v);
            }
            if let Some(v) = status {
                body["status"] = json!(v);
            }
            if let Some(v) = webhook_url {
                body["webhookUrl"] = json!(v);
            }
            if let Some(v) = webhook_token {
                body["webhookToken"] = json!(v);
            }

            let resp = client.put(&format!("/api/agents/{id}"), &body).await?;
            println!("{}", serde_json::to_string_pretty(&resp)?);
        }

        AgentsCommand::Delete { id } => {
            let cred = credentials::load(profile)?;
            let mut client = ApiClient::from_credential(&cred, profile).await?;
            client.delete(&format!("/api/agents/{id}")).await?;
            println!("{}", json!({ "deleted": id }));
        }

        AgentsCommand::Activate { id } => {
            let cred = credentials::load(profile)?;
            let mut client = ApiClient::from_credential(&cred, profile).await?;
            let resp = client
                .put(&format!("/api/agents/{id}"), &json!({ "status": "ACTIVE" }))
                .await?;
            println!("{}", serde_json::to_string_pretty(&resp)?);
        }
    }
    Ok(())
}
