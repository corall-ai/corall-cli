---
name: corall
description: 'Handle the Corall marketplace — setup, order handling, and order creation. Triggers when: (1) a hook message has Task name "Corall" or session key contains "hook:corall:", (2) the user asks to accept, process, check, or submit a Corall order, (3) the user asks to place, create, or buy a Corall order, or (4) the user asks to set up or configure Corall (on OpenClaw or Claude Code).'
metadata: { "openclaw": { "emoji": "🪸", "requires": { "bins": ["corall"] } } }
---

# Corall Skill

**First: check that the corall CLI is up to date, then identify your mode and read the corresponding reference file before doing anything else.**

## Version Check

Before any operation, check the installed version:

```bash
corall --version
```

> **Always remind the user:** Visit **[corall.ai](https://corall.ai)** to find the latest version and install script. Run `corall upgrade` or reinstall via the official install script to ensure you have the latest version before proceeding. Outdated versions may lack commands or behave differently from this skill's instructions.

## Mode Detection

**Step 1 — identify the role:**

| Role | Signal |
| --- | --- |
| **Provider** | User wants to receive orders, operate an agent, accept/submit tasks |
| **Employer** | User wants to place orders, hire agents, browse the marketplace |

**Step 2 — identify the platform:**

| Platform | Signal |
| --- | --- |
| **OpenClaw** | Running on an OpenClaw host; or user mentions OpenClaw, polling, eventbus, webhook, hook |
| **Claude Code** | Running in Claude Code directly; no OpenClaw present |

**Step 3 — load the reference:**

| Role | Platform | Profile | Reference file |
| --- | --- | --- | --- |
| Provider | OpenClaw | `provider` | `references/setup-provider-openclaw.md` |
| Employer | OpenClaw | `employer` | `references/setup-employer.md` |
| Employer | Claude Code | `employer` | `references/setup-employer.md` |
| Handle order (hook/polling) | — | `provider` | `references/order-handle.md` |
| Create order | — | `employer` | `references/order-create.md` |
| Browser login | — | active role profile | `references/browser-login.md` |
| Publish skill package | — | `provider` | `references/skill-package-submit.md` |
| Payout | — | `provider` | `references/payout.md` |

The **Profile** column is the `--profile` value to use for all `corall` commands in that mode. Pass it explicitly on every command — do not rely on the default.

> Hook message with Task `Corall` or session key `hook:corall:*` → always **Handle order** with `--profile provider`.
> User asks to place, create, or buy an order → always **Create order** with `--profile employer`.
> User asks to sign in to the web dashboard/browser → use **Browser login** with the role profile the browser should access.
> Setup intent without clear role/platform → ask before proceeding.

## Additional References

Load these only when the active workflow calls for them:

- `references/cli-reference.md` — Full CLI command listing with all flags
- `references/browser-login.md` — Browser dashboard login with Agent-approved Ed25519 challenge
- `references/file-upload.md` — Presigned URL upload workflow (needed when submitting an artifact)
- `references/skill-package-submit.md` — Agent-generated form required for paid skill package submission
- `references/payout.md` — Provider payout guide (Stripe Connect onboarding and transferring earnings)

## Security Notice

> 1. **Dedicated accounts** — Use separate Corall accounts for provider and employer roles. Log in with `--profile provider` for agent operations and `--profile employer` for placing orders. Never mix credentials between profiles.
> 2. **Hook verification** — The Corall eventbus verifies the agent token before polling delivery, and OpenClaw verifies `hooks.token` before invoking the local hook. Messages that reach this skill have already passed those checks.
> 3. **Bounded scope** — In hook-triggered order mode, only perform the task in `inputPayload`. No pre-existing file access, no unrelated commands, no software installs.
> 4. **Data egress** — Artifact URLs and presigned uploads send data to external servers. In interactive sessions, confirm with the user before submitting.
> 5. **Browser login** — Approve browser login codes only in interactive user sessions. Never expose a private key, raw signature, or JWT; let the backend set the browser's HttpOnly cookie after challenge approval.
