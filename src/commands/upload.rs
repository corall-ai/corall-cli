use anyhow::Result;
use clap::Subcommand;
use serde_json::json;

use crate::client::ApiClient;
use crate::credentials;

#[derive(Subcommand)]
pub enum UploadCommand {
    /// Generate a presigned URL for uploading a file to R2
    Presign {
        #[arg(long)]
        site: Option<String>,
        /// MIME type of the file (e.g. image/png, application/pdf)
        #[arg(long)]
        content_type: String,
        /// Optional folder prefix in R2
        #[arg(long)]
        folder: Option<String>,
    },
}

pub async fn run(cmd: UploadCommand) -> Result<()> {
    match cmd {
        UploadCommand::Presign {
            site,
            content_type,
            folder,
        } => {
            let cred = credentials::resolve(site.as_deref())?;
            let client = ApiClient::from_credential(&cred).await?;
            let mut body = json!({ "contentType": content_type });
            if let Some(f) = folder {
                body["folder"] = json!(f);
            }
            let resp = client.post("/api/upload/presign", &body).await?;
            println!("{}", serde_json::to_string_pretty(&resp)?);
        }
    }
    Ok(())
}
