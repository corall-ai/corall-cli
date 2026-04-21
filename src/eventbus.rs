use std::collections::BTreeMap;
use std::future::Future;
use std::net::SocketAddr;
use std::pin::Pin;
use std::sync::Arc;

use anyhow::Context;
use anyhow::Result;
use anyhow::anyhow;
use anyhow::bail;
use reqwest::Url;
use serde::Deserialize;
use serde::Serialize;
use serde_json::Value;
use serde_json::json;
use tokio::io::AsyncBufReadExt;
use tokio::io::AsyncReadExt;
use tokio::io::AsyncWriteExt;
use tokio::io::BufReader;
use tokio::net::TcpListener;
use tokio::net::TcpStream;

type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

const REGISTRATION_PREFIX: &str = "corall:eventbus:agent";
const STREAM_SUFFIX: &str = "stream";
const REGISTRATION_SUFFIX: &str = "registration";
const STREAM_START_ID: &str = "0";

#[derive(Debug, Clone)]
pub struct EventBusServeOptions {
    pub listen: SocketAddr,
    pub redis_url: String,
    pub consumer_group: String,
    pub default_wait_ms: u64,
    pub max_wait_ms: u64,
    pub default_count: usize,
    pub max_count: usize,
    pub claim_idle_ms: Option<u64>,
}

pub struct EventBusServer {
    state: Arc<AppState>,
}

impl EventBusServer {
    pub fn new(options: EventBusServeOptions) -> Result<Self> {
        let store = Arc::new(RedisEventStore::new(
            RedisConfig::from_url(&options.redis_url)?,
            options.consumer_group.clone(),
        ));
        Ok(Self {
            state: Arc::new(AppState { options, store }),
        })
    }

    #[cfg(test)]
    fn with_store(options: EventBusServeOptions, store: Arc<dyn EventStore>) -> Self {
        Self {
            state: Arc::new(AppState { options, store }),
        }
    }

    pub async fn serve(self) -> Result<()> {
        let listener = TcpListener::bind(self.state.options.listen)
            .await
            .with_context(|| format!("failed to bind {}", self.state.options.listen))?;
        self.serve_listener(listener).await
    }

    async fn serve_listener(self, listener: TcpListener) -> Result<()> {
        println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "listen": self.state.options.listen.to_string(),
                "redisUrl": redact_redis_url(&self.state.options.redis_url),
                "consumerGroup": self.state.options.consumer_group,
                "routes": {
                    "health": "/health",
                    "poll": "/v1/agents/:agent_id/events?consumerId=<id>&wait=<ms>&count=<n>",
                    "ack": "/v1/agents/:agent_id/events/:event_id/ack",
                },
            }))?
        );

        loop {
            let (socket, _) = listener
                .accept()
                .await
                .context("failed to accept connection")?;
            let state = self.state.clone();
            tokio::spawn(async move {
                if let Err(err) = handle_connection(socket, state).await {
                    eprintln!(
                        "{}",
                        serde_json::json!({ "error": format!("eventbus connection failed: {err}") })
                    );
                }
            });
        }
    }
}

struct AppState {
    options: EventBusServeOptions,
    store: Arc<dyn EventStore>,
}

trait EventStore: Send + Sync + 'static {
    fn health(&self) -> BoxFuture<'_, Result<()>>;
    fn load_registration<'a>(
        &'a self,
        agent_id: &'a str,
    ) -> BoxFuture<'a, Result<Option<AgentRegistration>>>;
    fn poll<'a>(
        &'a self,
        agent_id: &'a str,
        options: PollOptions,
    ) -> BoxFuture<'a, Result<Vec<Value>>>;
    fn ack<'a>(&'a self, agent_id: &'a str, event_id: &'a str) -> BoxFuture<'a, Result<u64>>;
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
struct AgentRegistration {
    token: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PollOptions {
    consumer_id: String,
    wait_ms: u64,
    count: usize,
    claim_idle_ms: Option<u64>,
}

#[derive(Debug, Clone, Serialize)]
struct HealthResponse {
    ok: bool,
    redis: &'static str,
}

#[derive(Debug, Clone, Serialize)]
struct PollResponse {
    #[serde(rename = "consumerId")]
    consumer_id: String,
    events: Vec<Value>,
}

#[derive(Debug, Clone, Serialize)]
struct AckResponse {
    ok: bool,
    acked: u64,
    #[serde(rename = "eventId")]
    event_id: String,
}

#[derive(Debug, Clone, Serialize)]
struct ErrorResponse<'a> {
    error: &'a str,
}

#[derive(Debug, Clone)]
struct HttpRequest {
    method: String,
    path: String,
    query: Option<String>,
    headers: BTreeMap<String, String>,
}

#[derive(Debug, Clone)]
struct HttpResponse {
    status: u16,
    headers: Vec<(String, String)>,
    body: String,
}

impl HttpResponse {
    fn json<T: Serialize>(status: u16, body: &T) -> Self {
        let body = serde_json::to_string(body)
            .unwrap_or_else(|_| "{\"error\":\"serialization failed\"}".to_owned());
        Self {
            status,
            headers: vec![("Content-Type".into(), "application/json".into())],
            body,
        }
    }

    fn unauthorized(message: &str) -> Self {
        let mut response = Self::json(401, &ErrorResponse { error: message });
        response
            .headers
            .push(("WWW-Authenticate".into(), "Bearer".into()));
        response
    }

    fn into_bytes(self) -> Vec<u8> {
        let mut output = format!(
            "HTTP/1.1 {} {}\r\nContent-Length: {}\r\nConnection: close\r\n",
            self.status,
            reason_phrase(self.status),
            self.body.len()
        )
        .into_bytes();
        for (name, value) in self.headers {
            output.extend_from_slice(name.as_bytes());
            output.extend_from_slice(b": ");
            output.extend_from_slice(value.as_bytes());
            output.extend_from_slice(b"\r\n");
        }
        output.extend_from_slice(b"\r\n");
        output.extend_from_slice(self.body.as_bytes());
        output
    }
}

#[derive(Debug, Clone)]
struct HttpError {
    status: u16,
    message: String,
}

impl HttpError {
    fn bad_request(message: impl Into<String>) -> Self {
        Self {
            status: 400,
            message: message.into(),
        }
    }

    fn unauthorized(message: impl Into<String>) -> Self {
        Self {
            status: 401,
            message: message.into(),
        }
    }

    fn method_not_allowed(message: impl Into<String>) -> Self {
        Self {
            status: 405,
            message: message.into(),
        }
    }

    fn not_found(message: impl Into<String>) -> Self {
        Self {
            status: 404,
            message: message.into(),
        }
    }

    fn service_unavailable(message: impl Into<String>) -> Self {
        Self {
            status: 503,
            message: message.into(),
        }
    }

    fn into_response(self) -> HttpResponse {
        if self.status == 401 {
            HttpResponse::unauthorized(&self.message)
        } else {
            HttpResponse::json(
                self.status,
                &ErrorResponse {
                    error: &self.message,
                },
            )
        }
    }
}

enum Route {
    Health,
    Poll { agent_id: String },
    Ack { agent_id: String, event_id: String },
}

#[derive(Debug, Clone)]
struct RedisEventStore {
    redis: RedisConfig,
    consumer_group: String,
}

impl RedisEventStore {
    fn new(redis: RedisConfig, consumer_group: String) -> Self {
        Self {
            redis,
            consumer_group,
        }
    }

    async fn health_impl(&self) -> Result<()> {
        let mut conn = self.redis.connect().await?;
        let reply = conn.execute(&["PING"]).await?;
        match reply {
            RespValue::Simple(value) if value == "PONG" => Ok(()),
            RespValue::Bulk(Some(value)) if value == b"PONG" => Ok(()),
            other => bail!("unexpected PING response: {other:?}"),
        }
    }

    async fn load_registration_impl(&self, agent_id: &str) -> Result<Option<AgentRegistration>> {
        let mut conn = self.redis.connect().await?;
        let key = registration_key(agent_id);
        match conn.execute(&["GET", key.as_str()]).await? {
            RespValue::Bulk(None) => Ok(None),
            RespValue::Bulk(Some(bytes)) => Ok(Some(
                serde_json::from_slice(&bytes).context("registration JSON is invalid")?,
            )),
            RespValue::Simple(value) => Ok(Some(
                serde_json::from_str(&value).context("registration JSON is invalid")?,
            )),
            other => bail!("unexpected GET response: {other:?}"),
        }
    }

    async fn poll_impl(&self, agent_id: &str, options: PollOptions) -> Result<Vec<Value>> {
        let stream = stream_key(agent_id);
        let mut conn = self.redis.connect().await?;
        ensure_group(&mut conn, &stream, &self.consumer_group).await?;

        let claimed = if let Some(claim_idle_ms) = options.claim_idle_ms.filter(|value| *value > 0)
        {
            match xautoclaim(
                &mut conn,
                &stream,
                &self.consumer_group,
                &options.consumer_id,
                claim_idle_ms,
                options.count,
            )
            .await
            {
                Ok(entries) => entries,
                Err(err) if redis_command_unsupported(&err) => Vec::new(),
                Err(err) => return Err(err),
            }
        } else {
            Vec::new()
        };

        let entries = if claimed.is_empty() {
            xreadgroup(
                &mut conn,
                &stream,
                &self.consumer_group,
                &options.consumer_id,
                options.count,
                options.wait_ms,
            )
            .await?
        } else {
            claimed
        };

        entries.into_iter().map(event_from_entry).collect()
    }

    async fn ack_impl(&self, agent_id: &str, event_id: &str) -> Result<u64> {
        let stream = stream_key(agent_id);
        let mut conn = self.redis.connect().await?;
        match xack(&mut conn, &stream, &self.consumer_group, event_id).await {
            Ok(acked) => Ok(acked),
            Err(err) if redis_group_missing(&err) => Ok(0),
            Err(err) => Err(err),
        }
    }
}

impl EventStore for RedisEventStore {
    fn health(&self) -> BoxFuture<'_, Result<()>> {
        Box::pin(async move { self.health_impl().await })
    }

    fn load_registration<'a>(
        &'a self,
        agent_id: &'a str,
    ) -> BoxFuture<'a, Result<Option<AgentRegistration>>> {
        Box::pin(async move { self.load_registration_impl(agent_id).await })
    }

    fn poll<'a>(
        &'a self,
        agent_id: &'a str,
        options: PollOptions,
    ) -> BoxFuture<'a, Result<Vec<Value>>> {
        Box::pin(async move { self.poll_impl(agent_id, options).await })
    }

    fn ack<'a>(&'a self, agent_id: &'a str, event_id: &'a str) -> BoxFuture<'a, Result<u64>> {
        Box::pin(async move { self.ack_impl(agent_id, event_id).await })
    }
}

#[derive(Debug, Clone)]
struct RedisConfig {
    host: String,
    port: u16,
    username: Option<String>,
    password: Option<String>,
    db: usize,
}

impl RedisConfig {
    fn from_url(raw: &str) -> Result<Self> {
        let url = Url::parse(raw).with_context(|| format!("invalid redis URL: {raw}"))?;
        match url.scheme() {
            "redis" => {}
            "rediss" => bail!("rediss:// is not supported by this scaffold"),
            scheme => bail!("unsupported redis URL scheme: {scheme}"),
        }

        let host = url
            .host_str()
            .context("redis URL is missing a host")?
            .to_owned();
        let port = url.port().unwrap_or(6379);
        let username = (!url.username().is_empty()).then(|| url.username().to_owned());
        let password = url.password().map(str::to_owned);
        if username.is_some() && password.is_none() {
            bail!("redis URL username requires a password");
        }

        let db = parse_database(url.path())?;
        Ok(Self {
            host,
            port,
            username,
            password,
            db,
        })
    }

    async fn connect(&self) -> Result<RedisConnection> {
        let stream = TcpStream::connect((self.host.as_str(), self.port))
            .await
            .with_context(|| {
                format!("failed to connect to redis at {}:{}", self.host, self.port)
            })?;
        let mut conn = RedisConnection {
            stream: BufReader::new(stream),
        };

        if let Some(password) = &self.password {
            let auth = if let Some(username) = &self.username {
                conn.execute(&["AUTH", username.as_str(), password.as_str()])
                    .await?
            } else {
                conn.execute(&["AUTH", password.as_str()]).await?
            };
            expect_ok(auth, "AUTH")?;
        }

        if self.db != 0 {
            let select = conn
                .execute(&["SELECT", &self.db.to_string()])
                .await
                .context("failed to select redis database")?;
            expect_ok(select, "SELECT")?;
        }

        Ok(conn)
    }
}

struct RedisConnection {
    stream: BufReader<TcpStream>,
}

impl RedisConnection {
    async fn execute(&mut self, args: &[&str]) -> Result<RespValue> {
        let mut buffer = format!("*{}\r\n", args.len()).into_bytes();
        for arg in args {
            buffer.extend_from_slice(format!("${}\r\n", arg.len()).as_bytes());
            buffer.extend_from_slice(arg.as_bytes());
            buffer.extend_from_slice(b"\r\n");
        }
        self.stream.get_mut().write_all(&buffer).await?;
        self.read_value().await
    }

    async fn read_value(&mut self) -> Result<RespValue> {
        let line = read_required_line(&mut self.stream).await?;
        let (prefix, payload) = line
            .split_first()
            .ok_or_else(|| anyhow!("redis response line was empty"))?;
        match prefix {
            b'+' => Ok(RespValue::Simple(String::from_utf8(payload.to_vec())?)),
            b'-' => Ok(RespValue::Error(String::from_utf8(payload.to_vec())?)),
            b':' => Ok(RespValue::Integer(
                String::from_utf8(payload.to_vec())?.parse()?,
            )),
            b'$' => {
                let len: i64 = String::from_utf8(payload.to_vec())?.parse()?;
                if len < 0 {
                    return Ok(RespValue::Bulk(None));
                }
                let mut body = vec![0; len as usize];
                self.stream.read_exact(&mut body).await?;
                let mut crlf = [0_u8; 2];
                self.stream.read_exact(&mut crlf).await?;
                if crlf != *b"\r\n" {
                    bail!("redis bulk response missing CRLF");
                }
                Ok(RespValue::Bulk(Some(body)))
            }
            b'*' => {
                let len: i64 = String::from_utf8(payload.to_vec())?.parse()?;
                if len < 0 {
                    return Ok(RespValue::Array(None));
                }
                let mut values = Vec::with_capacity(len as usize);
                for _ in 0..len {
                    values.push(Box::pin(self.read_value()).await?);
                }
                Ok(RespValue::Array(Some(values)))
            }
            other => bail!("unsupported redis RESP prefix: {}", *other as char),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum RespValue {
    Simple(String),
    Error(String),
    Integer(i64),
    Bulk(Option<Vec<u8>>),
    Array(Option<Vec<RespValue>>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct StreamEntry {
    id: String,
    fields: BTreeMap<String, String>,
}

async fn handle_connection(socket: TcpStream, state: Arc<AppState>) -> Result<()> {
    let mut reader = BufReader::new(socket);
    let request = match read_http_request(&mut reader).await? {
        Some(request) => request,
        None => return Ok(()),
    };

    let response = handle_http_request(state, request).await.into_bytes();
    let stream = reader.get_mut();
    stream.write_all(&response).await?;
    stream.shutdown().await?;
    Ok(())
}

async fn handle_http_request(state: Arc<AppState>, request: HttpRequest) -> HttpResponse {
    match dispatch_request(state, request).await {
        Ok(response) => response,
        Err(error) => error.into_response(),
    }
}

async fn dispatch_request(
    state: Arc<AppState>,
    request: HttpRequest,
) -> Result<HttpResponse, HttpError> {
    let route = parse_route(&request.path)?;
    match route {
        Route::Health => {
            if request.method != "GET" {
                return Err(HttpError::method_not_allowed("health only supports GET"));
            }
            state.store.health().await.map_err(|err| {
                HttpError::service_unavailable(format!("redis health check failed: {err}"))
            })?;
            Ok(HttpResponse::json(
                200,
                &HealthResponse {
                    ok: true,
                    redis: "ok",
                },
            ))
        }
        Route::Poll { agent_id } => {
            if request.method != "GET" {
                return Err(HttpError::method_not_allowed("poll only supports GET"));
            }
            authorize(&state, &request, &agent_id).await?;
            let options = parse_poll_options(&state.options, &request.query)?;
            let events = state
                .store
                .poll(&agent_id, options.clone())
                .await
                .map_err(|err| HttpError::service_unavailable(format!("poll failed: {err}")))?;
            Ok(HttpResponse::json(
                200,
                &PollResponse {
                    consumer_id: options.consumer_id,
                    events,
                },
            ))
        }
        Route::Ack { agent_id, event_id } => {
            if request.method != "POST" {
                return Err(HttpError::method_not_allowed("ack only supports POST"));
            }
            authorize(&state, &request, &agent_id).await?;
            let acked = state
                .store
                .ack(&agent_id, &event_id)
                .await
                .map_err(|err| HttpError::service_unavailable(format!("ack failed: {err}")))?;
            Ok(HttpResponse::json(
                200,
                &AckResponse {
                    ok: true,
                    acked,
                    event_id,
                },
            ))
        }
    }
}

async fn authorize(
    state: &AppState,
    request: &HttpRequest,
    agent_id: &str,
) -> Result<(), HttpError> {
    let provided = bearer_token(request)
        .ok_or_else(|| HttpError::unauthorized("missing Authorization: Bearer <token>"))?;
    let registration = state
        .store
        .load_registration(agent_id)
        .await
        .map_err(|err| {
            HttpError::service_unavailable(format!("registration lookup failed: {err}"))
        })?;
    let registration = registration
        .ok_or_else(|| HttpError::unauthorized("agent is not registered for eventbus"))?;
    if registration.token != provided {
        return Err(HttpError::unauthorized("invalid bearer token"));
    }
    Ok(())
}

fn parse_route(path: &str) -> Result<Route, HttpError> {
    let path = if path.len() > 1 {
        path.trim_end_matches('/')
    } else {
        path
    };
    if path == "/health" {
        return Ok(Route::Health);
    }

    let url = Url::parse(&format!("http://localhost{path}"))
        .map_err(|err| HttpError::bad_request(format!("invalid path: {err}")))?;
    let segments: Vec<_> = url
        .path_segments()
        .map(|items| items.collect())
        .unwrap_or_else(Vec::new);

    match segments.as_slice() {
        ["v1", "agents", agent_id, "events"] => Ok(Route::Poll {
            agent_id: (*agent_id).to_owned(),
        }),
        ["v1", "agents", agent_id, "events", event_id, "ack"] => Ok(Route::Ack {
            agent_id: (*agent_id).to_owned(),
            event_id: (*event_id).to_owned(),
        }),
        _ => Err(HttpError::not_found("route not found")),
    }
}

fn parse_poll_options(
    options: &EventBusServeOptions,
    query: &Option<String>,
) -> Result<PollOptions, HttpError> {
    let params = query_params(query)?;
    let consumer_id = params
        .get("consumerId")
        .filter(|value| !value.is_empty())
        .cloned()
        .ok_or_else(|| HttpError::bad_request("missing consumerId query parameter"))?;
    let wait_ms = parse_u64_param(params.get("wait"), "wait")?.unwrap_or(options.default_wait_ms);
    let count = parse_usize_param(params.get("count"), "count")?.unwrap_or(options.default_count);
    let query_claim_idle_ms = parse_u64_param(params.get("claimIdleMs"), "claimIdleMs")?;

    if count == 0 {
        return Err(HttpError::bad_request("count must be greater than zero"));
    }

    Ok(PollOptions {
        consumer_id,
        wait_ms: wait_ms.min(options.max_wait_ms),
        count: count.min(options.max_count),
        claim_idle_ms: query_claim_idle_ms.or(options.claim_idle_ms),
    })
}

fn query_params(query: &Option<String>) -> Result<BTreeMap<String, String>, HttpError> {
    let Some(query) = query else {
        return Ok(BTreeMap::new());
    };
    let url = Url::parse(&format!("http://localhost/?{query}"))
        .map_err(|err| HttpError::bad_request(format!("invalid query string: {err}")))?;
    Ok(url.query_pairs().into_owned().collect())
}

fn parse_u64_param(value: Option<&String>, name: &str) -> Result<Option<u64>, HttpError> {
    match value {
        Some(raw) => raw
            .parse()
            .map(Some)
            .map_err(|_| HttpError::bad_request(format!("{name} must be an unsigned integer"))),
        None => Ok(None),
    }
}

fn parse_usize_param(value: Option<&String>, name: &str) -> Result<Option<usize>, HttpError> {
    match value {
        Some(raw) => raw
            .parse()
            .map(Some)
            .map_err(|_| HttpError::bad_request(format!("{name} must be an unsigned integer"))),
        None => Ok(None),
    }
}

fn bearer_token(request: &HttpRequest) -> Option<String> {
    let value = request.headers.get("authorization")?;
    value
        .strip_prefix("Bearer ")
        .or_else(|| value.strip_prefix("bearer "))
        .map(str::to_owned)
}

async fn read_http_request(reader: &mut BufReader<TcpStream>) -> Result<Option<HttpRequest>> {
    let Some(request_line) = read_line(reader).await? else {
        return Ok(None);
    };
    if request_line.trim().is_empty() {
        return Ok(None);
    }

    let mut parts = request_line.split_whitespace();
    let method = parts
        .next()
        .context("missing HTTP method")?
        .to_ascii_uppercase();
    let target = parts.next().context("missing request target")?.to_owned();
    let version = parts.next().context("missing HTTP version")?;
    if version != "HTTP/1.1" && version != "HTTP/1.0" {
        bail!("unsupported HTTP version: {version}");
    }

    let (path, query) = split_target(&target)?;
    let mut headers = BTreeMap::new();
    let mut content_length = 0_usize;

    loop {
        let line = read_required_line(reader).await?;
        if line.is_empty() {
            break;
        }
        let header = String::from_utf8(line)?;
        let (name, value) = header
            .split_once(':')
            .ok_or_else(|| anyhow!("invalid HTTP header line"))?;
        let name = name.trim().to_ascii_lowercase();
        let value = value.trim().to_owned();
        if name == "content-length" {
            content_length = value.parse().context("invalid Content-Length header")?;
        }
        headers.insert(name, value);
    }

    if content_length > 0 {
        let mut discard = vec![0_u8; content_length];
        reader.read_exact(&mut discard).await?;
    }

    Ok(Some(HttpRequest {
        method,
        path,
        query,
        headers,
    }))
}

async fn read_line(reader: &mut BufReader<TcpStream>) -> Result<Option<String>> {
    let mut line = Vec::new();
    let bytes = reader.read_until(b'\n', &mut line).await?;
    if bytes == 0 {
        return Ok(None);
    }
    if !line.ends_with(b"\r\n") {
        bail!("HTTP line missing CRLF terminator");
    }
    line.truncate(line.len() - 2);
    Ok(Some(String::from_utf8(line)?))
}

async fn read_required_line(reader: &mut BufReader<TcpStream>) -> Result<Vec<u8>> {
    let mut line = Vec::new();
    let bytes = reader.read_until(b'\n', &mut line).await?;
    if bytes == 0 {
        bail!("unexpected EOF");
    }
    if !line.ends_with(b"\r\n") {
        bail!("line missing CRLF terminator");
    }
    line.truncate(line.len() - 2);
    Ok(line)
}

fn split_target(target: &str) -> Result<(String, Option<String>)> {
    if target.starts_with("http://") || target.starts_with("https://") {
        let url = Url::parse(target)?;
        return Ok((url.path().to_owned(), url.query().map(str::to_owned)));
    }
    match target.split_once('?') {
        Some((path, query)) => Ok((path.to_owned(), Some(query.to_owned()))),
        None => Ok((target.to_owned(), None)),
    }
}

fn reason_phrase(status: u16) -> &'static str {
    match status {
        200 => "OK",
        400 => "Bad Request",
        401 => "Unauthorized",
        404 => "Not Found",
        405 => "Method Not Allowed",
        503 => "Service Unavailable",
        _ => "Internal Server Error",
    }
}

fn redact_redis_url(raw: &str) -> String {
    match Url::parse(raw) {
        Ok(url) => {
            let mut display = format!("{}://{}", url.scheme(), url.host_str().unwrap_or("unknown"));
            if let Some(port) = url.port() {
                display.push(':');
                display.push_str(&port.to_string());
            }
            if !url.path().is_empty() && url.path() != "/" {
                display.push_str(url.path());
            }
            display
        }
        Err(_) => raw.to_owned(),
    }
}

fn parse_database(path: &str) -> Result<usize> {
    let trimmed = path.trim_start_matches('/');
    if trimmed.is_empty() {
        return Ok(0);
    }
    let first = trimmed
        .split('/')
        .next()
        .ok_or_else(|| anyhow!("invalid redis database path"))?;
    Ok(first.parse()?)
}

fn registration_key(agent_id: &str) -> String {
    format!("{REGISTRATION_PREFIX}:{agent_id}:{REGISTRATION_SUFFIX}")
}

fn stream_key(agent_id: &str) -> String {
    format!("{REGISTRATION_PREFIX}:{agent_id}:{STREAM_SUFFIX}")
}

async fn ensure_group(conn: &mut RedisConnection, stream: &str, group: &str) -> Result<()> {
    match conn
        .execute(&[
            "XGROUP",
            "CREATE",
            stream,
            group,
            STREAM_START_ID,
            "MKSTREAM",
        ])
        .await?
    {
        RespValue::Simple(value) if value == "OK" => Ok(()),
        RespValue::Error(message) if message.contains("BUSYGROUP") => Ok(()),
        other => bail!("unexpected XGROUP response: {other:?}"),
    }
}

async fn xreadgroup(
    conn: &mut RedisConnection,
    stream: &str,
    group: &str,
    consumer: &str,
    count: usize,
    wait_ms: u64,
) -> Result<Vec<StreamEntry>> {
    let count_str = count.to_string();
    let wait_str = wait_ms.to_string();
    let response = if wait_ms > 0 {
        conn.execute(&[
            "XREADGROUP",
            "GROUP",
            group,
            consumer,
            "COUNT",
            count_str.as_str(),
            "BLOCK",
            wait_str.as_str(),
            "STREAMS",
            stream,
            ">",
        ])
        .await?
    } else {
        conn.execute(&[
            "XREADGROUP",
            "GROUP",
            group,
            consumer,
            "COUNT",
            count_str.as_str(),
            "STREAMS",
            stream,
            ">",
        ])
        .await?
    };
    parse_xreadgroup_entries(response)
}

async fn xautoclaim(
    conn: &mut RedisConnection,
    stream: &str,
    group: &str,
    consumer: &str,
    min_idle_ms: u64,
    count: usize,
) -> Result<Vec<StreamEntry>> {
    let min_idle = min_idle_ms.to_string();
    let count = count.to_string();
    let response = conn
        .execute(&[
            "XAUTOCLAIM",
            stream,
            group,
            consumer,
            min_idle.as_str(),
            "0-0",
            "COUNT",
            count.as_str(),
        ])
        .await?;
    parse_xautoclaim_entries(response)
}

async fn xack(
    conn: &mut RedisConnection,
    stream: &str,
    group: &str,
    event_id: &str,
) -> Result<u64> {
    match conn.execute(&["XACK", stream, group, event_id]).await? {
        RespValue::Integer(value) if value >= 0 => Ok(value as u64),
        RespValue::Error(message) => bail!(message),
        other => bail!("unexpected XACK response: {other:?}"),
    }
}

fn expect_ok(value: RespValue, command: &str) -> Result<()> {
    match value {
        RespValue::Simple(reply) if reply == "OK" => Ok(()),
        RespValue::Error(message) => bail!("{command} failed: {message}"),
        other => bail!("unexpected {command} response: {other:?}"),
    }
}

fn parse_xreadgroup_entries(value: RespValue) -> Result<Vec<StreamEntry>> {
    let streams = match value {
        RespValue::Array(None) => return Ok(Vec::new()),
        RespValue::Array(Some(items)) => items,
        RespValue::Error(message) => bail!(message),
        other => bail!("unexpected XREADGROUP response: {other:?}"),
    };

    let mut entries = Vec::new();
    for stream in streams {
        let mut parts = into_array(stream, "stream entry")?.into_iter();
        let _stream_name =
            into_string(parts.next().context("missing stream name")?, "stream name")?;
        let stream_entries = into_array(
            parts.next().context("missing stream records")?,
            "stream records",
        )?;
        entries.extend(parse_entry_array(stream_entries)?);
    }
    Ok(entries)
}

fn parse_xautoclaim_entries(value: RespValue) -> Result<Vec<StreamEntry>> {
    let parts = match value {
        RespValue::Array(Some(items)) => items,
        RespValue::Array(None) => return Ok(Vec::new()),
        RespValue::Error(message) => bail!(message),
        other => bail!("unexpected XAUTOCLAIM response: {other:?}"),
    };
    if parts.len() < 2 {
        bail!("XAUTOCLAIM response was missing claimed entries");
    }
    parse_entry_array(into_array(parts[1].clone(), "claimed entries")?)
}

fn parse_entry_array(entries: Vec<RespValue>) -> Result<Vec<StreamEntry>> {
    let mut parsed = Vec::with_capacity(entries.len());
    for entry in entries {
        let mut parts = into_array(entry, "stream record")?.into_iter();
        let id = into_string(
            parts.next().context("missing stream record id")?,
            "stream record id",
        )?;
        let fields = into_array(
            parts.next().context("missing stream record fields")?,
            "stream record fields",
        )?;
        parsed.push(StreamEntry {
            id,
            fields: parse_fields(fields)?,
        });
    }
    Ok(parsed)
}

fn parse_fields(values: Vec<RespValue>) -> Result<BTreeMap<String, String>> {
    if values.len() % 2 != 0 {
        bail!("stream fields must have an even number of items");
    }

    let mut fields = BTreeMap::new();
    let mut iter = values.into_iter();
    while let Some(key) = iter.next() {
        let value = iter.next().context("missing field value")?;
        fields.insert(
            into_string(key, "field name")?,
            into_string(value, "field value")?,
        );
    }
    Ok(fields)
}

fn event_from_entry(mut entry: StreamEntry) -> Result<Value> {
    let payload = ["event", "payload", "data", "json"]
        .into_iter()
        .find_map(|field| entry.fields.remove(field));

    let mut object = match payload {
        Some(raw) => match serde_json::from_str::<Value>(&raw) {
            Ok(Value::Object(map)) => map,
            Ok(other) => {
                let mut map = serde_json::Map::new();
                map.insert("payload".into(), other);
                map
            }
            Err(err) => return Err(anyhow!("failed to parse stream event JSON: {err}")),
        },
        None => serde_json::Map::new(),
    };

    for (key, value) in entry.fields {
        object
            .entry(key)
            .or_insert_with(|| parse_field_json(&value));
    }

    if let Some(existing_id) = object.get("id").and_then(Value::as_str).map(str::to_owned) {
        if existing_id != entry.id {
            object
                .entry("eventId")
                .or_insert_with(|| Value::String(existing_id));
        }
    }
    object.insert("id".into(), Value::String(entry.id));
    Ok(Value::Object(object))
}

fn parse_field_json(value: &str) -> Value {
    serde_json::from_str(value).unwrap_or_else(|_| Value::String(value.to_owned()))
}

fn into_array(value: RespValue, context: &str) -> Result<Vec<RespValue>> {
    match value {
        RespValue::Array(Some(items)) => Ok(items),
        other => bail!("{context} was not a RESP array: {other:?}"),
    }
}

fn into_string(value: RespValue, context: &str) -> Result<String> {
    match value {
        RespValue::Simple(text) => Ok(text),
        RespValue::Bulk(Some(bytes)) => Ok(String::from_utf8(bytes)?),
        RespValue::Error(message) => bail!("{message}"),
        other => bail!("{context} was not a RESP string: {other:?}"),
    }
}

fn redis_group_missing(err: &anyhow::Error) -> bool {
    err.to_string().contains("NOGROUP")
}

fn redis_command_unsupported(err: &anyhow::Error) -> bool {
    let message = err.to_string();
    message.contains("unknown command") || message.contains("ERR unknown")
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;

    #[derive(Default)]
    struct MockStore {
        registration: Option<AgentRegistration>,
        poll_events: Vec<Value>,
        acked: u64,
        last_poll: Mutex<Option<(String, PollOptions)>>,
        last_ack: Mutex<Option<(String, String)>>,
    }

    impl EventStore for MockStore {
        fn health(&self) -> BoxFuture<'_, Result<()>> {
            Box::pin(async { Ok(()) })
        }

        fn load_registration<'a>(
            &'a self,
            _agent_id: &'a str,
        ) -> BoxFuture<'a, Result<Option<AgentRegistration>>> {
            let registration = self.registration.clone();
            Box::pin(async move { Ok(registration) })
        }

        fn poll<'a>(
            &'a self,
            agent_id: &'a str,
            options: PollOptions,
        ) -> BoxFuture<'a, Result<Vec<Value>>> {
            *self.last_poll.lock().unwrap() = Some((agent_id.to_owned(), options));
            let events = self.poll_events.clone();
            Box::pin(async move { Ok(events) })
        }

        fn ack<'a>(&'a self, agent_id: &'a str, event_id: &'a str) -> BoxFuture<'a, Result<u64>> {
            *self.last_ack.lock().unwrap() = Some((agent_id.to_owned(), event_id.to_owned()));
            let acked = self.acked;
            Box::pin(async move { Ok(acked) })
        }
    }

    fn test_options() -> EventBusServeOptions {
        EventBusServeOptions {
            listen: "127.0.0.1:8787".parse().unwrap(),
            redis_url: "redis://127.0.0.1:6379/0".into(),
            consumer_group: "corall-eventbus".into(),
            default_wait_ms: 25_000,
            max_wait_ms: 30_000,
            default_count: 50,
            max_count: 100,
            claim_idle_ms: Some(30_000),
        }
    }

    fn test_request(method: &str, target: &str, auth: Option<&str>) -> HttpRequest {
        let (path, query) = split_target(target).unwrap();
        let mut headers = BTreeMap::new();
        if let Some(token) = auth {
            headers.insert("authorization".into(), format!("Bearer {token}"));
        }
        HttpRequest {
            method: method.into(),
            path,
            query,
            headers,
        }
    }

    #[tokio::test]
    async fn poll_requires_bearer_token() {
        let store = Arc::new(MockStore {
            registration: Some(AgentRegistration {
                token: "secret".into(),
            }),
            ..Default::default()
        });
        let server = EventBusServer::with_store(test_options(), store);
        let response = handle_http_request(
            server.state.clone(),
            test_request("GET", "/v1/agents/agent-1/events?consumerId=worker-1", None),
        )
        .await;

        assert_eq!(response.status, 401);
        assert!(response.body.contains("missing Authorization"));
    }

    #[tokio::test]
    async fn poll_returns_events_and_forwards_query_options() {
        let store = Arc::new(MockStore {
            registration: Some(AgentRegistration {
                token: "secret".into(),
            }),
            poll_events: vec![json!({
                "id": "1719938100000-0",
                "type": "order.paid",
                "agentId": "agent-1",
                "orderId": "order-1",
                "hook": { "message": "paid", "name": "Corall", "sessionKey": "hook:corall:order-1", "deliver": false }
            })],
            ..Default::default()
        });
        let server = EventBusServer::with_store(test_options(), store.clone());
        let response = handle_http_request(
            server.state.clone(),
            test_request(
                "GET",
                "/v1/agents/agent-1/events?consumerId=worker-1&wait=1500&count=2&claimIdleMs=60000",
                Some("secret"),
            ),
        )
        .await;

        assert_eq!(response.status, 200);
        let body: Value = serde_json::from_str(&response.body).unwrap();
        assert_eq!(body["consumerId"], "worker-1");
        assert_eq!(body["events"].as_array().unwrap().len(), 1);

        let (agent_id, poll) = store.last_poll.lock().unwrap().clone().unwrap();
        assert_eq!(agent_id, "agent-1");
        assert_eq!(
            poll,
            PollOptions {
                consumer_id: "worker-1".into(),
                wait_ms: 1_500,
                count: 2,
                claim_idle_ms: Some(60_000),
            }
        );
    }

    #[tokio::test]
    async fn ack_uses_agent_and_event_from_route() {
        let store = Arc::new(MockStore {
            registration: Some(AgentRegistration {
                token: "secret".into(),
            }),
            acked: 1,
            ..Default::default()
        });
        let server = EventBusServer::with_store(test_options(), store.clone());
        let response = handle_http_request(
            server.state.clone(),
            test_request(
                "POST",
                "/v1/agents/agent-1/events/1719938100000-0/ack",
                Some("secret"),
            ),
        )
        .await;

        assert_eq!(response.status, 200);
        let body: Value = serde_json::from_str(&response.body).unwrap();
        assert_eq!(body["acked"], 1);
        assert_eq!(body["eventId"], "1719938100000-0");

        let (agent_id, event_id) = store.last_ack.lock().unwrap().clone().unwrap();
        assert_eq!(agent_id, "agent-1");
        assert_eq!(event_id, "1719938100000-0");
    }

    #[test]
    fn parses_xreadgroup_records() {
        let response = RespValue::Array(Some(vec![RespValue::Array(Some(vec![
            bulk("corall:eventbus:agent:agent-1:stream"),
            RespValue::Array(Some(vec![RespValue::Array(Some(vec![
                bulk("1719938100000-0"),
                RespValue::Array(Some(vec![
                    bulk("event"),
                    bulk("{\"type\":\"order.paid\",\"agentId\":\"agent-1\"}"),
                ])),
            ]))])),
        ]))]));

        let entries = parse_xreadgroup_entries(response).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].id, "1719938100000-0");
        assert_eq!(
            entries[0].fields.get("event"),
            Some(&"{\"type\":\"order.paid\",\"agentId\":\"agent-1\"}".to_owned())
        );
    }

    #[test]
    fn event_payload_uses_stream_id_for_ack() {
        let event = event_from_entry(StreamEntry {
            id: "1719938100000-0".into(),
            fields: BTreeMap::from([(
                "event".into(),
                "{\"id\":\"domain-event-1\",\"type\":\"order.paid\",\"agentId\":\"agent-1\"}"
                    .into(),
            )]),
        })
        .unwrap();

        assert_eq!(event["id"], "1719938100000-0");
        assert_eq!(event["eventId"], "domain-event-1");
    }

    #[tokio::test]
    async fn redis_contract_polls_and_acks_stream_event() {
        let Some(redis_url) = test_redis_url() else {
            eprintln!("skipping eventbus redis contract test: CORALL_TEST_REDIS_URL is unset");
            return;
        };

        let agent_id = unique_id("agent");
        let group = unique_id("group");
        let stream = stream_key(&agent_id);
        let registration = registration_key(&agent_id);
        let redis = RedisConfig::from_url(&redis_url).unwrap();
        let mut conn = redis.connect().await.unwrap();

        conn.execute(&["DEL", registration.as_str(), stream.as_str()])
            .await
            .unwrap();
        let registration_json = serde_json::json!({ "token": "secret" }).to_string();
        conn.execute(&["SET", registration.as_str(), registration_json.as_str()])
            .await
            .unwrap();
        let payload = serde_json::json!({
            "id": "domain-event-1",
            "type": "order.paid",
            "agentId": agent_id,
            "orderId": "order-1",
            "hook": {
                "message": "paid",
                "name": "Corall",
                "sessionKey": "hook:corall:order-1",
                "deliver": false
            }
        })
        .to_string();
        conn.execute(&["XADD", stream.as_str(), "*", "payload", payload.as_str()])
            .await
            .unwrap();

        let store = Arc::new(RedisEventStore::new(redis.clone(), group));
        let mut options = test_options();
        options.redis_url = redis_url;
        options.default_wait_ms = 0;
        options.max_wait_ms = 100;
        options.claim_idle_ms = None;
        let server = EventBusServer::with_store(options, store);
        let poll_target =
            format!("/v1/agents/{agent_id}/events?consumerId=worker-1&wait=0&count=1");
        let response = handle_http_request(
            server.state.clone(),
            test_request("GET", &poll_target, Some("secret")),
        )
        .await;

        assert_eq!(response.status, 200);
        let body: Value = serde_json::from_str(&response.body).unwrap();
        let event_id = body["events"][0]["id"].as_str().unwrap().to_owned();
        assert_eq!(body["events"][0]["eventId"], "domain-event-1");
        assert_eq!(
            body["events"][0]["hook"]["sessionKey"],
            "hook:corall:order-1"
        );

        let ack_target = format!("/v1/agents/{agent_id}/events/{event_id}/ack");
        let response = handle_http_request(
            server.state.clone(),
            test_request("POST", &ack_target, Some("secret")),
        )
        .await;
        assert_eq!(response.status, 200);
        let body: Value = serde_json::from_str(&response.body).unwrap();
        assert_eq!(body["acked"], 1);

        let mut cleanup = redis.connect().await.unwrap();
        cleanup
            .execute(&["DEL", registration.as_str(), stream.as_str()])
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn redis_contract_reclaims_unacked_event_after_idle() {
        let Some(redis_url) = test_redis_url() else {
            eprintln!("skipping eventbus redis reclaim test: CORALL_TEST_REDIS_URL is unset");
            return;
        };

        let agent_id = unique_id("agent_reclaim");
        let group = unique_id("group_reclaim");
        let stream = stream_key(&agent_id);
        let registration = registration_key(&agent_id);
        let redis = RedisConfig::from_url(&redis_url).unwrap();
        let mut conn = redis.connect().await.unwrap();

        conn.execute(&["DEL", registration.as_str(), stream.as_str()])
            .await
            .unwrap();
        conn.execute(&["SET", registration.as_str(), r#"{"token":"secret"}"#])
            .await
            .unwrap();
        conn.execute(&[
            "XADD",
            stream.as_str(),
            "*",
            "payload",
            r#"{"type":"order.paid","agentId":"agent_reclaim","orderId":"order-reclaim","hook":{"message":"paid","name":"Corall","sessionKey":"hook:corall:order-reclaim","deliver":false}}"#,
        ])
        .await
        .unwrap();

        let store = Arc::new(RedisEventStore::new(redis.clone(), group));
        let mut options = test_options();
        options.redis_url = redis_url;
        options.default_wait_ms = 0;
        options.max_wait_ms = 100;
        options.claim_idle_ms = None;
        let server = EventBusServer::with_store(options, store);

        let first_poll = format!("/v1/agents/{agent_id}/events?consumerId=worker-1&wait=0&count=1");
        let first_response = handle_http_request(
            server.state.clone(),
            test_request("GET", &first_poll, Some("secret")),
        )
        .await;
        assert_eq!(first_response.status, 200);
        let first_body: Value = serde_json::from_str(&first_response.body).unwrap();
        let event_id = first_body["events"][0]["id"].as_str().unwrap().to_owned();

        tokio::time::sleep(std::time::Duration::from_millis(5)).await;

        let reclaim_poll = format!(
            "/v1/agents/{agent_id}/events?consumerId=worker-2&wait=0&count=1&claimIdleMs=1"
        );
        let reclaim_response = handle_http_request(
            server.state.clone(),
            test_request("GET", &reclaim_poll, Some("secret")),
        )
        .await;
        assert_eq!(reclaim_response.status, 200);
        let reclaim_body: Value = serde_json::from_str(&reclaim_response.body).unwrap();
        assert_eq!(reclaim_body["events"][0]["id"], event_id);

        let ack_target = format!("/v1/agents/{agent_id}/events/{event_id}/ack");
        let ack_response = handle_http_request(
            server.state.clone(),
            test_request("POST", &ack_target, Some("secret")),
        )
        .await;
        assert_eq!(ack_response.status, 200);

        let mut cleanup = redis.connect().await.unwrap();
        cleanup
            .execute(&["DEL", registration.as_str(), stream.as_str()])
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn polling_http_server_integrates_with_redis_without_openclaw_or_llm() {
        let Some(redis_url) = test_redis_url() else {
            eprintln!("skipping eventbus HTTP integration test: CORALL_TEST_REDIS_URL is unset");
            return;
        };

        let agent_id = unique_id("agent_http");
        let group = unique_id("group_http");
        let stream = stream_key(&agent_id);
        let registration = registration_key(&agent_id);
        let redis = RedisConfig::from_url(&redis_url).unwrap();
        let mut conn = redis.connect().await.unwrap();

        conn.execute(&["DEL", registration.as_str(), stream.as_str()])
            .await
            .unwrap();
        conn.execute(&["SET", registration.as_str(), r#"{"token":"secret"}"#])
            .await
            .unwrap();
        conn.execute(&[
            "XADD",
            stream.as_str(),
            "*",
            "payload",
            r#"{"id":"domain-event-http","type":"order.paid","agentId":"agent_http","orderId":"order-http","hook":{"message":"paid","name":"Corall","sessionKey":"hook:corall:order-http","deliver":false}}"#,
        ])
        .await
        .unwrap();

        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let mut options = test_options();
        options.listen = addr;
        options.redis_url = redis_url;
        options.consumer_group = group;
        options.default_wait_ms = 0;
        options.max_wait_ms = 100;
        options.claim_idle_ms = None;

        let server = EventBusServer::new(options).unwrap();
        let server_task = tokio::spawn(server.serve_listener(listener));

        let poll_path =
            format!("/v1/agents/{agent_id}/events?consumerId=worker-http&wait=0&count=1");
        let poll = raw_http_json(addr, "GET", &poll_path, Some("secret")).await;
        assert_eq!(poll["status"], 200);
        let event = &poll["body"]["events"][0];
        let event_id = event["id"].as_str().unwrap();
        assert_eq!(event["eventId"], "domain-event-http");
        assert_eq!(event["hook"]["sessionKey"], "hook:corall:order-http");

        let ack_path = format!("/v1/agents/{agent_id}/events/{event_id}/ack");
        let ack = raw_http_json(addr, "POST", &ack_path, Some("secret")).await;
        assert_eq!(ack["status"], 200);
        assert_eq!(ack["body"]["acked"], 1);

        server_task.abort();
        let mut cleanup = redis.connect().await.unwrap();
        cleanup
            .execute(&["DEL", registration.as_str(), stream.as_str()])
            .await
            .unwrap();
    }

    fn bulk(value: &str) -> RespValue {
        RespValue::Bulk(Some(value.as_bytes().to_vec()))
    }

    async fn raw_http_json(
        addr: SocketAddr,
        method: &str,
        path: &str,
        bearer: Option<&str>,
    ) -> Value {
        let mut stream = TcpStream::connect(addr).await.unwrap();
        let auth = bearer
            .map(|token| format!("Authorization: Bearer {token}\r\n"))
            .unwrap_or_default();
        let request = format!(
            "{method} {path} HTTP/1.1\r\nHost: {addr}\r\n{auth}Connection: close\r\nContent-Length: 0\r\n\r\n"
        );
        stream.write_all(request.as_bytes()).await.unwrap();

        let mut raw = Vec::new();
        stream.read_to_end(&mut raw).await.unwrap();
        let response = String::from_utf8(raw).unwrap();
        let (head, body) = response.split_once("\r\n\r\n").unwrap();
        let status: u16 = head
            .lines()
            .next()
            .unwrap()
            .split_whitespace()
            .nth(1)
            .unwrap()
            .parse()
            .unwrap();
        json!({
            "status": status,
            "body": serde_json::from_str::<Value>(body).unwrap(),
        })
    }

    fn test_redis_url() -> Option<String> {
        std::env::var("CORALL_TEST_REDIS_URL").ok()
    }

    fn unique_id(prefix: &str) -> String {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        format!("{prefix}_{nanos}_{}", std::process::id())
    }
}
