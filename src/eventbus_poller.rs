use std::collections::HashMap;
use std::env;
use std::io::Write;
use std::process::Command;
use std::process::Stdio;
use std::time::Duration;
use std::time::Instant;

use anyhow::Context;
use anyhow::Result;
use anyhow::bail;
use reqwest::Client;
use reqwest::Url;
use serde::Deserialize;
use serde::Serialize;
use serde_json::Value;
use serde_json::json;
use tokio::time::sleep;

use crate::credentials;

#[derive(Debug, Clone)]
pub struct EventbusPollOptions {
    pub base_url: Option<String>,
    pub agent_id: Option<String>,
    pub webhook_token: Option<String>,
    pub consumer_id: Option<String>,
    pub wait_ms: u64,
    pub request_timeout_ms: Option<u64>,
    pub ack_timeout_ms: u64,
    pub idle_delay_ms: u64,
    pub error_backoff_ms: u64,
    pub max_error_backoff_ms: u64,
    pub recent_event_ttl_ms: u64,
    pub hook_url: Option<String>,
    pub hook_token: Option<String>,
    pub exec: Option<String>,
    pub exec_args: Vec<String>,
}

#[derive(Debug, Clone)]
struct ResolvedPollOptions {
    base_url: String,
    agent_id: String,
    webhook_token: String,
    consumer_id: String,
    wait_ms: u64,
    request_timeout_ms: u64,
    ack_timeout_ms: u64,
    idle_delay_ms: u64,
    error_backoff_ms: u64,
    max_error_backoff_ms: u64,
    recent_event_ttl_ms: u64,
    delivery: DeliveryMode,
}

#[derive(Debug, Clone)]
enum DeliveryMode {
    Hook {
        hook_url: String,
        hook_token: Option<String>,
    },
    Exec {
        program: String,
        args: Vec<String>,
    },
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
struct HookPayload {
    message: String,
    name: String,
    #[serde(rename = "sessionKey")]
    session_key: String,
    deliver: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
struct PollingEvent {
    id: String,
    #[serde(rename = "dedupeId")]
    dedupe_id: String,
    hook: HookPayload,
}

pub async fn run(options: EventbusPollOptions, profile: &str) -> Result<()> {
    let resolved = resolve_options(options, profile)?;
    let client = Client::new();
    let mut recent_events = HashMap::<String, Instant>::new();
    let mut backoff_ms = resolved.error_backoff_ms.max(1);

    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "mode": "eventbus-poll",
            "baseUrl": resolved.base_url,
            "agentId": resolved.agent_id,
            "consumerId": resolved.consumer_id,
            "waitMs": resolved.wait_ms,
            "delivery": delivery_summary(&resolved.delivery),
        }))?
    );

    loop {
        prune_recent_events(&mut recent_events, resolved.recent_event_ttl_ms);

        match poll_events(&client, &resolved).await {
            Ok(events) if events.is_empty() => {
                backoff_ms = resolved.error_backoff_ms.max(1);
                sleep(Duration::from_millis(resolved.idle_delay_ms)).await;
            }
            Ok(events) => {
                for event in events {
                    handle_event(&client, &resolved, &event, &mut recent_events).await?;
                }
                backoff_ms = resolved.error_backoff_ms.max(1);
            }
            Err(err) => {
                eprintln!(
                    "{}",
                    json!({
                        "warning": format!("eventbus poll cycle failed: {err}"),
                        "backoffMs": backoff_ms,
                    })
                );
                sleep(Duration::from_millis(backoff_ms)).await;
                backoff_ms = (backoff_ms.saturating_mul(2))
                    .max(resolved.error_backoff_ms.max(1))
                    .min(
                        resolved
                            .max_error_backoff_ms
                            .max(resolved.error_backoff_ms.max(1)),
                    );
            }
        }
    }
}

fn resolve_options(options: EventbusPollOptions, profile: &str) -> Result<ResolvedPollOptions> {
    let base_url = options
        .base_url
        .or_else(|| env::var("CORALL_EVENTBUS_URL").ok())
        .map(|value| value.trim_end_matches('/').to_string())
        .filter(|value| !value.is_empty())
        .context("eventbus base URL is required: pass --base-url or set CORALL_EVENTBUS_URL")?;

    let cred = credentials::load(profile).ok();
    let agent_id = options
        .agent_id
        .or_else(|| cred.as_ref().and_then(|cred| cred.agent_id.clone()))
        .context(
            "no agentId found — pass --agent-id or create/update an agent with this profile first",
        )?;

    let webhook_token = options
        .webhook_token
        .or_else(|| env::var("CORALL_WEBHOOK_TOKEN").ok())
        .or_else(|| cred.as_ref().and_then(|cred| cred.polling_token.clone()))
        .filter(|value| !value.trim().is_empty())
        .context(
            "polling token is required: pass --webhook-token, set CORALL_WEBHOOK_TOKEN, or create/update the agent with --webhook-token first",
        )?;

    let delivery = match (
        options.hook_url.map(|value| value.trim().to_string()),
        options.exec.map(|value| value.trim().to_string()),
    ) {
        (Some(hook_url), None) if !hook_url.is_empty() => DeliveryMode::Hook {
            hook_url,
            hook_token: options.hook_token.filter(|value| !value.trim().is_empty()),
        },
        (None, Some(program)) if !program.is_empty() => DeliveryMode::Exec {
            program,
            args: options.exec_args,
        },
        (Some(_), Some(_)) => {
            bail!("choose exactly one local delivery target: either --hook-url or --exec")
        }
        _ => bail!("missing local delivery target: pass either --hook-url or --exec"),
    };

    let consumer_id = options
        .consumer_id
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| format!("corall-cli-poll:{agent_id}:{}", std::process::id()));

    Ok(ResolvedPollOptions {
        base_url,
        agent_id,
        webhook_token,
        consumer_id,
        wait_ms: options.wait_ms,
        request_timeout_ms: options
            .request_timeout_ms
            .unwrap_or(options.wait_ms.saturating_add(15_000))
            .max(options.wait_ms.saturating_add(1_000)),
        ack_timeout_ms: options.ack_timeout_ms,
        idle_delay_ms: options.idle_delay_ms,
        error_backoff_ms: options.error_backoff_ms.max(1),
        max_error_backoff_ms: options
            .max_error_backoff_ms
            .max(options.error_backoff_ms.max(1)),
        recent_event_ttl_ms: options.recent_event_ttl_ms.max(1),
        delivery,
    })
}

fn delivery_summary(delivery: &DeliveryMode) -> Value {
    match delivery {
        DeliveryMode::Hook { hook_url, .. } => json!({
            "mode": "hook",
            "hookUrl": hook_url,
        }),
        DeliveryMode::Exec { program, args } => json!({
            "mode": "exec",
            "program": program,
            "args": args,
        }),
    }
}

fn prune_recent_events(recent_events: &mut HashMap<String, Instant>, ttl_ms: u64) {
    let ttl = Duration::from_millis(ttl_ms);
    recent_events.retain(|_, seen_at| seen_at.elapsed() <= ttl);
}

async fn poll_events(client: &Client, config: &ResolvedPollOptions) -> Result<Vec<PollingEvent>> {
    let mut url = agent_events_url(config, None)?;
    url.query_pairs_mut()
        .append_pair("consumerId", &config.consumer_id)
        .append_pair("wait", &config.wait_ms.to_string());

    let response = client
        .get(url.clone())
        .bearer_auth(&config.webhook_token)
        .header("accept", "application/json")
        .timeout(Duration::from_millis(config.request_timeout_ms))
        .send()
        .await
        .with_context(|| format!("failed to poll {url}"))?;

    let status = response.status();
    let body = response
        .text()
        .await
        .context("failed to read eventbus poll response body")?;
    if !status.is_success() {
        bail!("eventbus poll returned HTTP {status}: {body}");
    }

    let value: Value = if body.trim().is_empty() {
        Value::Null
    } else {
        serde_json::from_str(&body).context("eventbus poll response was not valid JSON")?
    };

    Ok(extract_events(&value)
        .into_iter()
        .filter_map(normalize_event)
        .collect())
}

async fn ack_event(client: &Client, config: &ResolvedPollOptions, event_id: &str) -> Result<()> {
    let url = agent_events_url(config, Some(event_id))?;

    let response = client
        .post(url.clone())
        .bearer_auth(&config.webhook_token)
        .header("accept", "application/json")
        .timeout(Duration::from_millis(config.ack_timeout_ms))
        .send()
        .await
        .with_context(|| format!("failed to ack {url}"))?;

    let status = response.status();
    let body = response
        .text()
        .await
        .context("failed to read eventbus ack response body")?;
    if !status.is_success() {
        bail!("eventbus ack returned HTTP {status}: {body}");
    }
    Ok(())
}

fn agent_events_url(config: &ResolvedPollOptions, event_id: Option<&str>) -> Result<Url> {
    let mut url = Url::parse(&config.base_url)?;
    {
        let mut segments = url
            .path_segments_mut()
            .map_err(|_| anyhow::anyhow!("base URL cannot be used for path segments"))?;
        segments.extend(["v1", "agents", &config.agent_id, "events"]);
        if let Some(event_id) = event_id {
            segments.extend([event_id, "ack"]);
        }
    }
    Ok(url)
}

async fn handle_event(
    client: &Client,
    config: &ResolvedPollOptions,
    event: &PollingEvent,
    recent_events: &mut HashMap<String, Instant>,
) -> Result<()> {
    let already_forwarded = recent_events.contains_key(&event.dedupe_id);
    if !already_forwarded {
        deliver_event(client, config, event).await?;
        recent_events.insert(event.dedupe_id.clone(), Instant::now());
    }
    ack_event(client, config, &event.id).await
}

async fn deliver_event(
    client: &Client,
    config: &ResolvedPollOptions,
    event: &PollingEvent,
) -> Result<()> {
    match &config.delivery {
        DeliveryMode::Hook {
            hook_url,
            hook_token,
        } => deliver_hook(client, hook_url, hook_token.as_deref(), &event.hook, config).await,
        DeliveryMode::Exec { program, args } => deliver_exec(program, args, config, event),
    }
}

async fn deliver_hook(
    client: &Client,
    hook_url: &str,
    hook_token: Option<&str>,
    hook: &HookPayload,
    config: &ResolvedPollOptions,
) -> Result<()> {
    let mut request = client
        .post(hook_url)
        .header("content-type", "application/json")
        .header("accept", "application/json")
        .timeout(Duration::from_millis(config.ack_timeout_ms))
        .json(hook);
    if let Some(hook_token) = hook_token {
        request = request.bearer_auth(hook_token);
    }

    let response = request
        .send()
        .await
        .with_context(|| format!("failed to deliver hook to {hook_url}"))?;

    let status = response.status();
    let body = response
        .text()
        .await
        .context("failed to read local hook response body")?;
    if !status.is_success() {
        bail!("local hook returned HTTP {status}: {body}");
    }
    Ok(())
}

fn deliver_exec(
    program: &str,
    args: &[String],
    config: &ResolvedPollOptions,
    event: &PollingEvent,
) -> Result<()> {
    let payload = serde_json::to_vec(&json!({
        "id": event.id,
        "dedupeId": event.dedupe_id,
        "hook": event.hook,
    }))?;

    let mut child = Command::new(program)
        .args(args)
        .env("CORALL_AGENT_ID", &config.agent_id)
        .env("CORALL_EVENT_ID", &event.id)
        .env("CORALL_EVENT_DEDUPE_ID", &event.dedupe_id)
        .env("CORALL_HOOK_NAME", &event.hook.name)
        .env("CORALL_HOOK_MESSAGE", &event.hook.message)
        .env("CORALL_HOOK_SESSION_KEY", &event.hook.session_key)
        .env("CORALL_HOOK_DELIVER", event.hook.deliver.to_string())
        .stdin(Stdio::piped())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .spawn()
        .with_context(|| format!("failed to start local command `{program}`"))?;

    if let Some(stdin) = child.stdin.as_mut() {
        stdin
            .write_all(&payload)
            .context("failed to write event payload to local command stdin")?;
    } else {
        bail!("local command `{program}` did not expose stdin");
    }

    let status = child
        .wait()
        .with_context(|| format!("failed to wait for local command `{program}`"))?;
    if !status.success() {
        bail!("local command `{program}` exited with status {status}");
    }

    Ok(())
}

fn extract_events(payload: &Value) -> Vec<&Value> {
    if let Some(events) = payload.as_array() {
        return events.iter().collect();
    }

    let Some(object) = payload.as_object() else {
        return Vec::new();
    };

    if let Some(events) = object.get("events").and_then(Value::as_array) {
        return events.iter().collect();
    }

    if let Some(event) = object.get("event") {
        return vec![event];
    }

    if object.get("hook").is_some() {
        return vec![payload];
    }

    Vec::new()
}

fn normalize_event(value: &Value) -> Option<PollingEvent> {
    let object = value.as_object()?;
    let id = first_string(object, &["id", "streamId", "stream_id"])?;
    let hook = serde_json::from_value::<HookPayload>(object.get("hook")?.clone()).ok()?;
    let dedupe_id = first_string(object, &["eventId", "event_id", "dedupeId", "dedupe_id"])
        .unwrap_or_else(|| hook.session_key.clone());

    Some(PollingEvent {
        id,
        dedupe_id,
        hook,
    })
}

fn first_string(object: &serde_json::Map<String, Value>, keys: &[&str]) -> Option<String> {
    keys.iter().find_map(|key| {
        object
            .get(*key)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_owned)
    })
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;
    use std::time::SystemTime;

    use serde_json::json;

    use super::*;

    #[test]
    fn normalize_event_accepts_eventbus_shapes() {
        let event = normalize_event(&json!({
            "streamId": "stream-1",
            "eventId": "order.paid:1",
            "hook": {
                "message": "paid",
                "name": "Corall",
                "sessionKey": "hook:corall:1",
                "deliver": false
            }
        }))
        .expect("event should normalize");

        assert_eq!(event.id, "stream-1");
        assert_eq!(event.dedupe_id, "order.paid:1");
        assert_eq!(event.hook.session_key, "hook:corall:1");
    }

    #[test]
    fn normalize_event_falls_back_to_session_key_for_dedupe_id() {
        let event = normalize_event(&json!({
            "id": "stream-2",
            "hook": {
                "message": "paid",
                "name": "Corall",
                "sessionKey": "hook:corall:2",
                "deliver": false
            }
        }))
        .expect("event should normalize");

        assert_eq!(event.id, "stream-2");
        assert_eq!(event.dedupe_id, "hook:corall:2");
    }

    #[test]
    fn extract_events_accepts_single_event_shape() {
        let payload = json!({
            "event": {
                "id": "stream-3",
                "hook": {
                    "message": "paid",
                    "name": "Corall",
                    "sessionKey": "hook:corall:3",
                    "deliver": false
                }
            }
        });

        let events = extract_events(&payload);
        assert_eq!(events.len(), 1);
        assert_eq!(events[0]["id"], "stream-3");
    }

    #[test]
    fn resolve_options_rejects_missing_or_conflicting_delivery_targets() {
        let base = EventbusPollOptions {
            base_url: Some("http://127.0.0.1:3001".to_string()),
            agent_id: Some("agent-1".to_string()),
            webhook_token: Some("token".to_string()),
            consumer_id: None,
            wait_ms: 30_000,
            request_timeout_ms: None,
            ack_timeout_ms: 10_000,
            idle_delay_ms: 1_000,
            error_backoff_ms: 2_000,
            max_error_backoff_ms: 30_000,
            recent_event_ttl_ms: 600_000,
            hook_url: None,
            hook_token: None,
            exec: None,
            exec_args: Vec::new(),
        };

        let missing = resolve_options(base.clone(), "provider").unwrap_err();
        assert!(
            missing
                .to_string()
                .contains("missing local delivery target: pass either --hook-url or --exec")
        );

        let conflicting = resolve_options(
            EventbusPollOptions {
                hook_url: Some("http://127.0.0.1:9000/hooks".to_string()),
                exec: Some("python3".to_string()),
                ..base
            },
            "provider",
        )
        .unwrap_err();
        assert!(
            conflicting
                .to_string()
                .contains("choose exactly one local delivery target")
        );
    }

    #[test]
    fn resolve_options_uses_explicit_exec_delivery_and_defaults_consumer_id() {
        let resolved = resolve_options(
            EventbusPollOptions {
                base_url: Some("http://127.0.0.1:3001/".to_string()),
                agent_id: Some("agent-1".to_string()),
                webhook_token: Some("token".to_string()),
                consumer_id: None,
                wait_ms: 30_000,
                request_timeout_ms: None,
                ack_timeout_ms: 10_000,
                idle_delay_ms: 1_000,
                error_backoff_ms: 2_000,
                max_error_backoff_ms: 30_000,
                recent_event_ttl_ms: 600_000,
                hook_url: None,
                hook_token: None,
                exec: Some("python3".to_string()),
                exec_args: vec!["worker.py".to_string()],
            },
            "provider",
        )
        .expect("options should resolve");

        assert_eq!(resolved.base_url, "http://127.0.0.1:3001");
        assert_eq!(resolved.agent_id, "agent-1");
        assert_eq!(resolved.webhook_token, "token");
        assert!(
            resolved.consumer_id.starts_with("corall-cli-poll:agent-1:"),
            "unexpected consumer id: {}",
            resolved.consumer_id
        );
        assert_eq!(resolved.request_timeout_ms, 45_000);
        match resolved.delivery {
            DeliveryMode::Exec { program, args } => {
                assert_eq!(program, "python3");
                assert_eq!(args, vec!["worker.py".to_string()]);
            }
            DeliveryMode::Hook { .. } => panic!("expected exec delivery"),
        }
    }

    #[test]
    fn deliver_exec_writes_event_payload_to_stdin_and_exports_env() {
        let out_path = unique_temp_file("corall-poller-event");
        let env_path = unique_temp_file("corall-poller-env");
        let config = ResolvedPollOptions {
            base_url: "http://127.0.0.1:8787".to_string(),
            agent_id: "agent-1".to_string(),
            webhook_token: "token".to_string(),
            consumer_id: "consumer".to_string(),
            wait_ms: 30_000,
            request_timeout_ms: 45_000,
            ack_timeout_ms: 10_000,
            idle_delay_ms: 10,
            error_backoff_ms: 10,
            max_error_backoff_ms: 100,
            recent_event_ttl_ms: 60_000,
            delivery: DeliveryMode::Exec {
                program: "python3".to_string(),
                args: vec![
                    "-c".to_string(),
                    format!(
                        "import json, os, pathlib, sys; \
                         pathlib.Path(r\"{}\").write_bytes(sys.stdin.buffer.read()); \
                         pathlib.Path(r\"{}\").write_text(json.dumps({{\
                             'CORALL_AGENT_ID': os.environ.get('CORALL_AGENT_ID'), \
                             'CORALL_EVENT_ID': os.environ.get('CORALL_EVENT_ID'), \
                             'CORALL_EVENT_DEDUPE_ID': os.environ.get('CORALL_EVENT_DEDUPE_ID'), \
                             'CORALL_HOOK_NAME': os.environ.get('CORALL_HOOK_NAME'), \
                             'CORALL_HOOK_MESSAGE': os.environ.get('CORALL_HOOK_MESSAGE'), \
                             'CORALL_HOOK_SESSION_KEY': os.environ.get('CORALL_HOOK_SESSION_KEY'), \
                             'CORALL_HOOK_DELIVER': os.environ.get('CORALL_HOOK_DELIVER'), \
                         }}))",
                        out_path.display(),
                        env_path.display()
                    ),
                ],
            },
        };
        let event = PollingEvent {
            id: "stream-1".to_string(),
            dedupe_id: "order-1".to_string(),
            hook: HookPayload {
                message: "paid".to_string(),
                name: "Corall".to_string(),
                session_key: "hook:corall:1".to_string(),
                deliver: false,
            },
        };

        deliver_exec(
            "python3",
            match &config.delivery {
                DeliveryMode::Exec { args, .. } => args,
                DeliveryMode::Hook { .. } => unreachable!(),
            },
            &config,
            &event,
        )
        .expect("exec delivery should succeed");

        let written = fs::read_to_string(&out_path).expect("payload should be written");
        let value: Value = serde_json::from_str(&written).expect("payload should be valid JSON");
        assert_eq!(value["id"], "stream-1");
        assert_eq!(value["dedupeId"], "order-1");
        assert_eq!(value["hook"]["sessionKey"], "hook:corall:1");

        let env_json = fs::read_to_string(&env_path).expect("env snapshot should be written");
        let env_value: Value =
            serde_json::from_str(&env_json).expect("env snapshot should be valid JSON");
        assert_eq!(env_value["CORALL_AGENT_ID"], "agent-1");
        assert_eq!(env_value["CORALL_EVENT_ID"], "stream-1");
        assert_eq!(env_value["CORALL_EVENT_DEDUPE_ID"], "order-1");
        assert_eq!(env_value["CORALL_HOOK_NAME"], "Corall");
        assert_eq!(env_value["CORALL_HOOK_MESSAGE"], "paid");
        assert_eq!(env_value["CORALL_HOOK_SESSION_KEY"], "hook:corall:1");
        assert_eq!(env_value["CORALL_HOOK_DELIVER"], "false");

        let _ = fs::remove_file(&out_path);
        let _ = fs::remove_file(&env_path);
    }

    fn unique_temp_file(prefix: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .expect("clock should be valid")
            .as_nanos();
        std::env::temp_dir().join(format!("{prefix}-{nanos}.json"))
    }
}
