# Agent Approval and Account Status

Use this workflow when the user wants to sign in to the Corall web UI,
open the dashboard, check account status from a browser, or asks whether
there is a login/account page.

Corall has one dashboard sign-in mechanism: Agent approval. The Agent fetches a
backend challenge, signs it with its local Ed25519 account key, and receives a
one-time dashboard `loginUrl`. The backend verifies the Ed25519 signature and
sets the dashboard's HttpOnly session cookie when the browser opens that URL.
The Agent sends only the public key plus signature to Corall and must never
expose the private key, raw signature, or JWT to the user or to the dashboard.

## When the User Asks for Account Status or Login

Do not scan or guess common web routes such as `/login`, `/signin`,
`/account`, `/me`, or `/profile`. Instead:

1. Tell the user to open the Corall dashboard URL: `https://yourdomain.com/dashboard`.
2. If the dashboard is not signed in, create a signed dashboard login URL with `corall auth approve` using the profile that should own the dashboard session.
3. Give the returned `loginUrl` to the user to open in the browser.
4. After the link opens, the dashboard should finish login automatically.

For CLI-visible account status, use these commands instead of route probing:

```bash
corall auth me --profile provider
corall subscriptions status --profile provider
corall agents list --mine --profile provider
```

Use `--profile employer` for employer dashboard/account status.

## Approve a Dashboard Session

Use the profile that matches the account the dashboard should log in as:

```bash
corall auth approve https://yourdomain.com --profile employer
```

For provider dashboard access, use `--profile provider` instead.

The command fetches the dashboard approval challenge, signs it locally, and sends only the public key plus signature to Corall. If the command succeeds, open the returned `loginUrl`; the page should finish login automatically.

## Guardrails

- Do not create dashboard login URLs from polling-delivered order sessions.
- Confirm the target site before creating a login URL.
- If the user has not registered or logged in locally, run the relevant setup workflow first.
- If the link expired, run `corall auth approve` again to create a new signed dashboard login URL.

## Conservative Fallback For Weaker Models

- Do not scan routes such as `/login`, `/signin`, `/account`, or `/profile`. Give the dashboard URL and the exact `corall auth approve <site> --profile <profile>` command instead.
- If local credentials are missing or auth is broken, stop and complete the matching setup workflow before creating a login URL.
- If the login URL was already consumed or expired, run `corall auth approve` again. Do not reuse an old `loginUrl`.
- If the user did not specify whether the dashboard session should belong to the provider or employer account, ask which profile should own the browser session before creating the link.
