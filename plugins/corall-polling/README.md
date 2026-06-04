# Corall Polling Plugin

Native OpenClaw plugin that long-polls the Corall resident eventbus and forwards each event's `hook` payload to the local OpenClaw `/hooks/agent` endpoint. This preserves the existing Corall skill flow without requiring public inbound webhooks.

## Install

```bash
corall openclaw setup --eventbus-url http://127.0.0.1:8080
openclaw gateway restart
```

The released `corall` CLI embeds the compiled plugin files and installs them
with `openclaw plugins install --force`. For local plugin development, run
`npm ci && npm run build` before compiling the CLI so the embedded `dist/`
files are current. OpenClaw loads the compiled ESM entry at `dist/index.js`.

## Config

Add this under `plugins.entries.corall-polling` in `~/.openclaw/openclaw.json`:

```json
{
  "enabled": true,
  "config": {
    "baseUrl": "http://127.0.0.1:8080",
    "waitSeconds": 30
  }
}
```

Notes:

- `agentId` defaults to `~/.corall/credentials/provider.json` and `agentToken` defaults to `hooks.token`, so the minimal config only needs `baseUrl`.
- If you use a different Corall credential profile, set `credentialProfile`.
- Event polling uses `Authorization: Bearer <agentToken>`, so create/update the Corall agent with the same token you keep in `hooks.token` via `--webhook-token`.
- If `hookUrl` is omitted, the plugin forwards to `http://127.0.0.1:<gateway.port>/hooks/agent`.
- Local forwarding uses `hooks.token` from the active OpenClaw config, so `corall openclaw setup` still supplies the hook auth expected by the Corall skill flow.
