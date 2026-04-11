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
    /// Stored credential enables automatic re-login on 401.
    credential: Option<Credential>,
    /// Profile name used to persist refreshed tokens.
    profile: String,
}

impl ApiClient {
    pub fn new(base_url: impl Into<String>) -> Self {
        Self {
            http: Client::new(),
            base_url: base_url.into().trim_end_matches('/').to_string(),
            token: None,
            credential: None,
            profile: "default".to_string(),
        }
    }

    pub async fn from_credential(cred: &Credential, profile: &str) -> Result<Self> {
        let mut client = ApiClient {
            profile: profile.to_string(),
            ..ApiClient::new(crate::credentials::site_to_base_url(&cred.site))
        };
        client.credential = Some(cred.clone());
        if let Some(cached) = cred.cached_token() {
            client.token = Some(cached.to_string());
        } else {
            client.do_login(cred).await?;
        }
        Ok(client)
    }

    /// Performs a fresh login via Ed25519 challenge/verify, caches the token.
    async fn do_login(&mut self, cred: &Credential) -> Result<()> {
        use ed25519_dalek::Signer;

        // Step 1: request a challenge for this public key.
        let ch_resp = self
            .http
            .post(format!("{}/api/auth/challenge", self.base_url))
            .json(&serde_json::json!({ "publicKey": cred.public_key }))
            .send()
            .await
            .context("challenge request failed")?;
        let ch_body: serde_json::Value = ch_resp
            .json()
            .await
            .context("failed to parse challenge response")?;
        let challenge_hex = ch_body["challenge"]
            .as_str()
            .context("no challenge in response")?;
        let challenge_bytes = hex::decode(challenge_hex).context("invalid challenge hex")?;

        // Step 2: sign the challenge with the stored private key.
        let sk_bytes = hex::decode(&cred.private_key).context("invalid private key hex")?;
        let sk_arr: [u8; 32] = sk_bytes
            .try_into()
            .map_err(|_| anyhow::anyhow!("private key must be 32 bytes"))?;
        let signing_key = ed25519_dalek::SigningKey::from_bytes(&sk_arr);
        let signature = signing_key.sign(&challenge_bytes);
        let sig_hex = hex::encode(signature.to_bytes());

        // Step 3: verify the signature and obtain a JWT.
        let v_resp = self
            .http
            .post(format!("{}/api/auth/verify", self.base_url))
            .json(&serde_json::json!({
                "publicKey": cred.public_key,
                "signature": sig_hex,
            }))
            .send()
            .await
            .context("verify request failed")?;
        let v_body: serde_json::Value = v_resp
            .json()
            .await
            .context("failed to parse verify response")?;
        let token = v_body["token"]
            .as_str()
            .context("no token in verify response")?
            .to_string();

        let expires_at = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0)
            + 7 * 24 * 3600;
        let updated = Credential {
            token: Some(token.clone()),
            token_expires_at: Some(expires_at),
            ..cred.clone()
        };
        crate::credentials::save(&self.profile, &updated)?;
        self.token = Some(token);
        self.credential = Some(updated);
        Ok(())
    }

    /// Refreshes the token when the server returns 401.
    async fn refresh(&mut self) -> Result<()> {
        // Clone to avoid borrow conflict with &mut self in do_login.
        let cred = self
            .credential
            .clone()
            .context("cannot refresh: no credential stored")?;
        self.do_login(&cred).await
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
        let url = response.url().to_string();
        let raw = response
            .text()
            .await
            .context("failed to read response body")?;
        let body: Value = serde_json::from_str(&raw).with_context(|| {
            format!("failed to parse response as JSON (HTTP {status} from {url}): {raw}")
        })?;
        if !status.is_success() {
            let msg = body
                .get("error")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown error");
            bail!("HTTP {status}: {msg}");
        }
        Ok(body)
    }

    /// Sends a request, retrying once on 401. Returns the raw Response.
    async fn send_raw_with_refresh(
        &mut self,
        method: Method,
        path: &str,
        attach: impl Fn(RequestBuilder) -> RequestBuilder,
    ) -> Result<Response> {
        let resp = attach(self.request(method.clone(), path))
            .send()
            .await
            .context("request failed")?;

        if resp.status() == StatusCode::UNAUTHORIZED && self.credential.is_some() {
            self.refresh().await?;
            return attach(self.request(method, path))
                .send()
                .await
                .context("request failed");
        }

        Ok(resp)
    }

    pub async fn get(&mut self, path: &str) -> Result<Value> {
        let resp = self.send_raw_with_refresh(Method::GET, path, |r| r).await?;
        Self::handle(resp).await
    }

    pub async fn post<B: Serialize>(&mut self, path: &str, body: &B) -> Result<Value> {
        let bytes = serde_json::to_vec(body)?;
        let resp = self
            .send_raw_with_refresh(Method::POST, path, move |r| {
                r.header("content-type", "application/json")
                    .body(bytes.clone())
            })
            .await?;
        Self::handle(resp).await
    }

    pub async fn post_empty(&mut self, path: &str) -> Result<Value> {
        let resp = self
            .send_raw_with_refresh(Method::POST, path, |r| r.header("content-length", "0"))
            .await?;
        Self::handle(resp).await
    }

    /// Like `get` but returns the status code along with the parsed body,
    /// so callers can handle specific error codes (e.g. 402).
    pub async fn get_raw(&mut self, path: &str) -> Result<(StatusCode, Value)> {
        let resp = self.send_raw_with_refresh(Method::GET, path, |r| r).await?;
        let status = resp.status();
        let url = resp.url().to_string();
        let raw = resp.text().await.context("failed to read response body")?;
        let body: Value = serde_json::from_str(&raw).with_context(|| {
            format!("failed to parse response as JSON (HTTP {status} from {url}): {raw}")
        })?;
        Ok((status, body))
    }

    /// Like `post_empty` but returns the status code along with the parsed body.
    pub async fn post_empty_raw(&mut self, path: &str) -> Result<(StatusCode, Value)> {
        let resp = self
            .send_raw_with_refresh(Method::POST, path, |r| r.header("content-length", "0"))
            .await?;
        let status = resp.status();
        let url = resp.url().to_string();
        let raw = resp.text().await.context("failed to read response body")?;
        let body: Value = serde_json::from_str(&raw).with_context(|| {
            format!("failed to parse response as JSON (HTTP {status} from {url}): {raw}")
        })?;
        Ok((status, body))
    }

    pub async fn put<B: Serialize>(&mut self, path: &str, body: &B) -> Result<Value> {
        let bytes = serde_json::to_vec(body)?;
        let resp = self
            .send_raw_with_refresh(Method::PUT, path, move |r| {
                r.header("content-type", "application/json")
                    .body(bytes.clone())
            })
            .await?;
        Self::handle(resp).await
    }

    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    pub async fn delete(&mut self, path: &str) -> Result<StatusCode> {
        let resp = self
            .send_raw_with_refresh(Method::DELETE, path, |r| r)
            .await?;
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
