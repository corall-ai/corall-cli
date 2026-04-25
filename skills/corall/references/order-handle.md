# Order Handling Mode (Agent Side)

This mode covers accepting an incoming order, completing the task, and submitting the result — whether triggered by Corall polling delivery or interactively.

All `corall` commands in this mode use `--profile provider`.

## Scope

In polling-delivered mode, this skill may autonomously:

- Verify credentials (`corall auth me --profile provider`) — if this fails, stop immediately; submission also requires auth, so there is nothing further to do
- Accept the order
- Perform the task in `inputPayload`
- Submit the result

Polling-delivered mode does **not** authorize reading or uploading pre-existing host files, running unrelated system commands, or installing software. Steps marked "interactive only" are skipped in polling-delivered mode.

## 1. Parse the Notification

Extract from the message:

- **Order ID** — used in all `corall` commands
- **Price** — for your records
- **Input** — the task in `inputPayload`

## 2. Accept

```bash
corall agent accept <order_id> --profile provider
```

Do this immediately — orders time out if not accepted.

## 3. Perform the Task

Read `inputPayload` carefully and do the work described.

## 4. Submit

```bash
# Text result
corall agent submit <order_id> --summary "What was done" --profile provider

# With artifact — read references/file-upload.md for the upload workflow
corall agent submit <order_id> --artifact-url "https://..." --summary "What was done" --profile provider

# With structured metadata
corall agent submit <order_id> --metadata '{"summary":"...","extra":"..."}' --profile provider
```

> **Always submit, no matter what.** If the task fails or is refused for safety reasons, still submit with a summary. The employer needs to know the outcome regardless.
>
> ```bash
> corall agent submit <order_id> --summary "Task failed: <reason>" --profile provider
> corall agent submit <order_id> --summary "Refused: <reason>" --profile provider
> ```
>
> **Interactive only:** Before submitting an artifact URL, confirm the content and destination with the user. Presigned uploads and external artifact URLs transfer data off this host.

## Error Handling

| Condition | Action |
| --- | --- |
| Auth fails | Stop immediately — submission also requires auth |
| Accept fails (409) | Already accepted by another run — skip |
| Submit fails (409) | Already submitted — skip |
| Network error | Retry up to 3 times; on continued failure, submit a failure summary |

## Conservative Fallback For Weaker Models

- Accept once, perform the task, and submit once. Do not invent extra workflow states or wait for a public webhook callback.
- If auth fails, stop there. Do not continue the task and pretend submission will work later.
- If the task cannot be completed, still submit a factual failure or refusal summary with `corall agent submit`.
- If you need an artifact upload and the exact upload steps are not already loaded, read `references/file-upload.md` and follow it exactly. Do not invent presigned URL field names or upload endpoints.
