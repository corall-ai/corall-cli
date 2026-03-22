use std::env;
use std::fs;
use std::path::PathBuf;

use anyhow::Context;
use anyhow::Result;
use anyhow::bail;
use clap::Subcommand;
use rand::Rng;
use serde_json::Value;
use serde_json::json;

#[derive(Subcommand)]
pub enum OpenclawCommand {
    /// Merge Corall integration settings into ~/.openclaw/openclaw.json
    ///
    /// Sets hooks.enabled, hooks.token, hooks.allowRequestSessionKey, and
    /// hooks.allowedSessionKeyPrefixes (preserving existing entries). Also
    /// sets gateway.mode="local" and gateway.bind="lan" if not already
    /// configured. Existing keys outside these fields are left untouched.
    ///
    /// hooks.token is preserved from the existing config unless --webhook-token
    /// is supplied explicitly, so re-running setup does not rotate the token.
    Setup {
        /// Webhook token to write to hooks.token. Must match the webhookToken
        /// registered on your Corall agent. If omitted and a token is already
        /// present in the config, that token is kept unchanged. If there is no
        /// existing token, a cryptographically secure random token is generated
        /// (32 random bytes as hex, same format as OpenClaw's own auto-generated
        /// tokens) and printed in the output so you can copy it when registering
        /// the agent.
        #[arg(long)]
        webhook_token: Option<String>,

        /// Path to openclaw.json. Defaults to the standard OpenClaw location
        /// resolved via OPENCLAW_CONFIG_PATH, OPENCLAW_STATE_DIR, or
        /// ~/.openclaw/openclaw.json (with legacy path fallback).
        #[arg(long)]
        config: Option<PathBuf>,
    },
}

pub async fn run(cmd: OpenclawCommand) -> Result<()> {
    match cmd {
        OpenclawCommand::Setup {
            webhook_token,
            config,
        } => {
            let config_path = match config {
                Some(p) => p,
                None => resolve_config_path()?.ok_or_else(|| {
                    anyhow::anyhow!(
                        "OpenClaw config not found. Install OpenClaw and run it at least once\n\
                         to create the config file, then re-run this command.\n\
                         See: https://openclaw.io\n\
                         \n\
                         If your config is in a non-standard location, pass --config <path>."
                    )
                })?,
            };

            // `resolve_config_path` only returns paths that exist. When
            // --config is passed explicitly, the file might not exist yet.
            if !config_path.exists() {
                bail!(
                    "OpenClaw config not found at {}.\n\
                     Make sure OpenClaw is installed and has been run at least once.",
                    config_path.display()
                );
            }

            let raw = fs::read_to_string(&config_path)
                .with_context(|| format!("failed to read {}", config_path.display()))?;
            // OpenClaw configs are JSON5 (allow comments, trailing commas).
            let mut cfg: Value = json5::from_str(&raw)
                .with_context(|| format!("failed to parse {}", config_path.display()))?;

            if !cfg.is_object() {
                bail!("{} is not a JSON object", config_path.display());
            }

            // Resolve token: explicit arg > existing config value > newly generated.
            // A token already in the config is preserved unless --webhook-token is
            // supplied, so re-running setup does not rotate the token by accident.
            let existing_token = cfg["hooks"]["token"].as_str().map(str::to_owned);
            let (token, generated, kept) = match webhook_token {
                Some(t) => (t, false, false),
                None => match existing_token {
                    Some(t) => (t, false, true),
                    None => (generate_token(), true, false),
                },
            };

            apply_hooks(&mut cfg, &token);
            apply_gateway_defaults(&mut cfg)?;

            let content = serde_json::to_string_pretty(&cfg)?;
            fs::write(&config_path, &content)
                .with_context(|| format!("failed to write {}", config_path.display()))?;

            // Report what was written.
            //
            // `webhookToken` is included when the token was auto-generated or kept
            // from the existing config, so callers always have the token value
            // available without needing to read the config file themselves.
            // When the token was provided by the caller via --webhook-token it is
            // already known, so we omit it to avoid echoing secrets.
            let prefixes = cfg["hooks"]["allowedSessionKeyPrefixes"].clone();
            let mut result = json!({
                "configPath": config_path.display().to_string(),
                "tokenGenerated": generated,
                "tokenKept": kept,
                "applied": {
                    "hooks": {
                        "enabled": true,
                        "allowRequestSessionKey": true,
                        "allowedSessionKeyPrefixes": prefixes,
                    },
                    "gateway": {
                        "mode": cfg["gateway"]["mode"],
                        "bind": cfg["gateway"]["bind"],
                    },
                },
            });
            if generated || kept {
                result["webhookToken"] = json!(token);
            }
            println!("{}", serde_json::to_string_pretty(&result)?);
        }
    }
    Ok(())
}

/// Merge the Corall-required hooks fields into the config.
///
/// Always sets: `hooks.enabled`, `hooks.token`, `hooks.allowRequestSessionKey`.
/// For `hooks.allowedSessionKeyPrefixes`: adds `"hook:"` if not already present,
/// preserving any other existing prefixes.
fn apply_hooks(cfg: &mut Value, webhook_token: &str) {
    let obj = cfg.as_object_mut().expect("cfg is an object");
    let hooks = obj.entry("hooks").or_insert_with(|| json!({}));
    let hooks = hooks.as_object_mut().expect("hooks is an object");

    hooks.insert("enabled".into(), json!(true));
    hooks.insert("token".into(), json!(webhook_token));
    hooks.insert("allowRequestSessionKey".into(), json!(true));

    let prefixes = hooks
        .entry("allowedSessionKeyPrefixes")
        .or_insert_with(|| json!([]));
    if let Some(arr) = prefixes.as_array_mut() {
        if !arr.iter().any(|v| v.as_str() == Some("hook:")) {
            arr.push(json!("hook:"));
        }
    } else {
        // Field exists but is not an array — replace it.
        *prefixes = json!(["hook:"]);
    }
}

/// Set gateway fields required by Corall.
///
/// Both `gateway.mode` and `gateway.bind` are forced unconditionally:
/// - `mode = "local"` — the gateway must run on this machine for webhook delivery.
/// - `bind = "lan"` — binds to 0.0.0.0 so the gateway is reachable for incoming webhooks.
///
/// Fails if `gateway.tailscale.mode` is `"serve"` or `"funnel"`: OpenClaw rejects
/// the combination of `bind: "lan"` (non-loopback) with tailscale serve/funnel.
fn apply_gateway_defaults(cfg: &mut Value) -> Result<()> {
    let obj = cfg.as_object_mut().expect("cfg is an object");

    // Guard: tailscale serve/funnel requires a loopback bind, which is incompatible
    // with the lan bind that Corall needs. Bail before writing anything.
    let tailscale_mode = obj
        .get("gateway")
        .and_then(|g| g.get("tailscale"))
        .and_then(|t| t.get("mode"))
        .and_then(|m| m.as_str())
        .unwrap_or("");
    if tailscale_mode == "serve" || tailscale_mode == "funnel" {
        bail!(
            "Cannot set gateway.bind=\"lan\": your config has gateway.tailscale.mode=\"{tailscale_mode}\".\n\
             OpenClaw requires bind to resolve to loopback when tailscale serve/funnel is enabled.\n\
             Disable tailscale serve/funnel first, or set gateway.bind manually after setup."
        );
    }

    let gateway = obj.entry("gateway").or_insert_with(|| json!({}));
    let gateway = gateway.as_object_mut().expect("gateway is an object");

    gateway.insert("mode".into(), json!("local"));
    gateway.insert("bind".into(), json!("lan"));
    Ok(())
}

/// Resolve the default OpenClaw config path, following the same precedence
/// as OpenClaw itself. Returns `None` if no existing config file is found —
/// the caller is responsible for emitting an appropriate error.
///
/// 1. `OPENCLAW_CONFIG_PATH` (explicit override — must exist)
/// 2. `$OPENCLAW_STATE_DIR/openclaw.json` (must exist)
/// 3. `~/.openclaw/openclaw.json`
/// 4. Legacy state dirs: `~/.clawdbot`, `~/.moldbot`, `~/.moltbot`
fn resolve_config_path() -> Result<Option<PathBuf>> {
    // 1. Explicit config path override.
    if let Ok(p) = env::var("OPENCLAW_CONFIG_PATH") {
        return Ok(Some(expand_tilde(&p)));
    }

    // 2. State dir override.
    if let Ok(state_dir) = env::var("OPENCLAW_STATE_DIR") {
        return Ok(Some(expand_tilde(&state_dir).join("openclaw.json")));
    }

    let home = dirs::home_dir().context("cannot determine home directory")?;

    // 3. Preferred default.
    let preferred = home.join(".openclaw").join("openclaw.json");
    if preferred.exists() {
        return Ok(Some(preferred));
    }

    // 4. Legacy state dirs.
    let legacy_dirs = [".clawdbot", ".moldbot", ".moltbot"];
    let legacy_names = ["clawdbot.json", "moldbot.json", "moltbot.json"];
    for dir in &legacy_dirs {
        for name in &legacy_names {
            let p = home.join(dir).join(name);
            if p.exists() {
                return Ok(Some(p));
            }
        }
    }

    Ok(None)
}

/// Generate a cryptographically secure random token.
///
/// Uses the same format as OpenClaw: 32 random bytes encoded as a lowercase
/// hex string (64 characters). Equivalent to Node.js
/// `crypto.randomBytes(32).toString("hex")`.
fn generate_token() -> String {
    let mut bytes = [0u8; 32];
    rand::rng().fill_bytes(&mut bytes);
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn expand_tilde(path: &str) -> PathBuf {
    if let (Some(rest), Some(home)) = (path.strip_prefix("~/"), dirs::home_dir()) {
        return home.join(rest);
    }
    PathBuf::from(path)
}
