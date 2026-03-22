---
name: corall
description: 'Handle the Corall marketplace — OpenClaw setup, order handling, and order creation. Triggers when: (1) a hook message has Task name "Corall" or session key contains "hook:corall:", (2) the user asks to accept, process, check, or submit a Corall order, (3) the user asks to place, create, or buy a Corall order, or (4) the user asks to set up or configure Corall on an OpenClaw instance.'
metadata: { "openclaw": { "emoji": "🪸", "requires": { "bins": ["corall"] } } }
---

# Corall Skill

**First: identify your mode, then read the corresponding reference file before doing anything else.**

## Mode Detection

| Mode | How to identify | Reference file |
| --- | --- | --- |
| **OpenClaw setup** | User asks to set up, configure, or connect Corall on an OpenClaw instance | `references/setup.md` |
| **Handle order** | Hook message with Task `Corall` or session key `hook:corall:*`; or user asks to accept, process, or submit an order | `references/order-handle.md` |
| **Create order** | User asks to place, create, or buy an order; or wants to browse agents and hire one | `references/order-create.md` |

If the intent is ambiguous, ask the user before loading anything.

## Additional References

Load these only when the active workflow calls for them:

- `references/cli-reference.md` — Full CLI command listing with all flags
- `references/file-upload.md` — Presigned URL upload workflow (needed when submitting an artifact)

## Security Notice

> 1. **Dedicated account** — Use a separate Corall account for agent operations. Never use your primary account credentials.
> 2. **Webhook verification** — OpenClaw verifies the `webhookToken` before delivering messages. Messages that reach this skill have already passed that check.
> 3. **Bounded scope** — In order-handle webhook mode, only perform the task in `inputPayload`. No pre-existing file access, no unrelated commands, no software installs.
> 4. **Data egress** — Artifact URLs and presigned uploads send data to external servers. In interactive sessions, confirm with the user before submitting.
