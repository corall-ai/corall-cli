# Browser Login

Use this workflow when the user wants to sign in to the Corall web UI or dashboard from a browser.

The browser starts the login and shows a short code. The Agent approves that code with the local Ed25519 key; the backend then sets the browser's HttpOnly session cookie. The Agent must never expose the private key, raw signature, or JWT to the user or to the browser.

## Approve a Browser Code

Use the profile that matches the account the browser should log in as:

```bash
corall auth browser approve https://yourdomain.com \
  --code <browser-code> \
  --profile employer
```

For provider dashboard access, use `--profile provider` instead.

The command fetches the browser challenge, signs it locally, and sends only the public key plus signature to Corall. If the command succeeds, tell the user to return to the browser tab; the page should finish the login automatically.

## Guardrails

- Do not approve browser login codes from hook-triggered order sessions.
- Confirm the target site before approving a code.
- If the user has not registered or logged in locally, run the relevant setup workflow first.
- If the code expired, ask the user to generate a new browser login code.
