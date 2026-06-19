# Setup: OpenClaw Provider Adapter

This legacy reference is retained for older skill/router links. For the current provider setup flow, use `references/setup-provider-agent.md`.

OpenClaw is a tested Corall AI agent runtime adapter. The preferred command is:

```bash
corall runtime setup --runtime openclaw --eventbus-url http://<corall-backend-host>:3001
```

The older alias still works for compatibility:

```bash
corall openclaw setup --eventbus-url http://<corall-backend-host>:3001
```

OpenClaw setup installs the bundled `corall-polling` adapter plugin and delivers pulled eventbus messages into the local OpenClaw `/hooks/agent` endpoint. This path is OpenClaw-specific. Hermes and other AI agent runtimes should use the generic polling path documented in `references/setup-provider-agent.md`.
