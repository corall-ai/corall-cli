use anyhow::Context;
use anyhow::Result;
use anyhow::bail;
use reqwest::Client;
use reqwest::Method;
use reqwest::RequestBuilder;
use reqwest::Response;
use reqwest::StatusCode;
use serde::Serialize;
use serde_json::Value;

use crate::credentials::Credential;

pub struct ApiClient {
    http: Client,
    base_url: String,
    token: Option<String>,
}

impl ApiClient {
    pub fn new(base_url: impl Into<String>) -> Self {
        Self {
            http: Client::new(),
            base_url: base_url.into().trim_end_matches('/').to_string(),
            token: None,
        }
    }

    pub async fn from_credential(cred: &Credential) -> Result<Self> {
        let mut client = ApiClient::new(crate::credentials::site_to_base_url(&cred.site));
        let token = client.login(&cred.email, &cred.password).await?;
        client.token = Some(token);
        Ok(client)
    }

    fn request(&self, method: Method, path: &str) -> RequestBuilder {
        let url = format!("{}{}", self.base_url, path);
        let mut req = self.http.request(method, url);
        if let Some(token) = &self.token {
            req = req.bearer_auth(token);
        }
        req
    }

    async fn handle(response: Response) -> Result<Value> {
        let status = response.status();
        let body: Value = response
            .json()
            .await
            .context("failed to parse response as JSON")?;
        if !status.is_success() {
            let msg = body
                .get("error")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown error");
            bail!("HTTP {status}: {msg}");
        }
        Ok(body)
    }

    pub async fn login(&self, email: &str, password: &str) -> Result<String> {
        let resp = self
            .request(Method::POST, "/api/auth/login")
            .json(&serde_json::json!({ "email": email, "password": password }))
            .send()
            .await
            .context("request failed")?;
        let body = Self::handle(resp).await?;
        body.get("token")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .context("no token in login response")
    }

    pub async fn get(&self, path: &str) -> Result<Value> {
        let resp = self
            .request(Method::GET, path)
            .send()
            .await
            .context("request failed")?;
        Self::handle(resp).await
    }

    pub async fn post<B: Serialize>(&self, path: &str, body: &B) -> Result<Value> {
        let resp = self
            .request(Method::POST, path)
            .json(body)
            .send()
            .await
            .context("request failed")?;
        Self::handle(resp).await
    }

    pub async fn post_empty(&self, path: &str) -> Result<Value> {
        let resp = self
            .request(Method::POST, path)
            .header("content-length", "0")
            .send()
            .await
            .context("request failed")?;
        Self::handle(resp).await
    }

    pub async fn put<B: Serialize>(&self, path: &str, body: &B) -> Result<Value> {
        let resp = self
            .request(Method::PUT, path)
            .json(body)
            .send()
            .await
            .context("request failed")?;
        Self::handle(resp).await
    }

    pub async fn delete(&self, path: &str) -> Result<StatusCode> {
        let resp = self
            .request(Method::DELETE, path)
            .send()
            .await
            .context("request failed")?;
        let status = resp.status();
        if !status.is_success() {
            let body: Value = resp.json().await.unwrap_or(Value::Null);
            let msg = body
                .get("error")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown error");
            bail!("HTTP {status}: {msg}");
        }
        Ok(status)
    }
}
