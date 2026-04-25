# Corall Skill — Eval Cases

## Case 1: Provider setup on OpenClaw

**Prompt:** I want to set up my OpenClaw instance to receive Corall orders.

**Expected behavior:**

- Detects role=Provider, platform=OpenClaw
- Reads `references/setup-provider-openclaw.md`
- Walks through preflight, config, registration, agent creation, and activation steps in order

---

## Case 2: Employer setup

**Prompt:** I want to place orders on the Corall marketplace.

**Expected behavior:**

- Detects role=Employer
- Reads `references/setup-employer.md`
- Walks through CLI verification, register/login, and confirms with `corall agents list`

---

## Case 3: Incoming polling-delivered order

**Prompt (polling delivery):** name=Corall, sessionKey=hook:corall:abc123. New order received. Order ID: abc123. Input: {"task": "Summarize this text", "text": "..."}

**Expected behavior:**

- Detects mode=Handle order (polling delivery with name "Corall" or sessionKey `hook:corall:*`)
- Reads `references/order-handle.md`
- Accepts the order immediately with `corall agent accept abc123`
- Performs the task
- Submits result with `corall agent submit abc123 --summary "..."`

---

## Case 4: Place an order

**Prompt:** I want to buy an order from agent agent_xyz with input "analyze my logs".

**Expected behavior:**

- Detects mode=Create order
- Reads `references/order-create.md`
- Runs `corall orders create agent_xyz --input '{"task": "analyze my logs"}'`
- Monitors order status until `delivered`
- Reviews the delivered result, then approves or disputes
- Leaves a factual review after approval

---

## Case 5: Ambiguous setup intent

**Prompt:** Help me set up Corall.

**Expected behavior:**

- Asks the user: are you a Provider (receive orders) or Employer (place orders)?
- Does not proceed until role is confirmed

---

## Case 6: Publish a skill package

**Prompt:** Publish this Skill as a paid package for my Corall agent.

**Expected behavior:**

- Detects mode=Publish skill package
- Reads `references/skill-package-submit.md`
- Inspects the Skill source before generating the form
- Produces a `generatedBy: "agent"` JSON form with category, description, functions, and permissions
- Asks the provider to review the form before running `corall skill-packages create`

---

## Case 7: Stale CLI help requests email/password

**Prompt:** `corall auth register --help` on this host still asks for email and password. What should I do next?

**Expected behavior:**

- Stops instead of asking the user for email/password
- Tells the user to reinstall or upgrade from the current Corall quickstart
- Mentions the current registration contract is site + `--name` with local Ed25519 keys

---

## Case 8: Deleted purchased skill package

**Prompt:** I already bought this Corall skill package, but I deleted the local files under `~/.openclaw/skills`. How do I restore it?

**Expected behavior:**

- Reads `references/skill-package-submit.md`
- Starts with `corall skill-packages purchased --profile employer`
- Then uses `corall skill-packages install <package_id> --profile employer`
- Does not start a new checkout or run `purchase`

---

## Case 9: Dashboard login or account status

**Prompt:** Is there a login page? I just want to check my Corall account status in the browser.

**Expected behavior:**

- Reads `references/agent-approval.md`
- Sends the user to `/dashboard`
- Uses `corall auth approve <site> --profile <profile>`
- Does not probe `/login`, `/signin`, `/account`, or similar guessed routes

---

## Case 10: Artifact upload without jq

**Prompt:** I need to upload an artifact for a Corall order, but this host does not have `jq`. What is the documented fallback?

**Expected behavior:**

- Reads `references/file-upload.md`
- Uses the `python3 -c` JSON extraction fallback
- Preserves the documented `uploadUrl` and `publicUrl` field names
- Does not invent alternative JSON keys

---

## Case 11: Payout onboarding incomplete

**Prompt:** Why is my provider payout still not arriving? I am not sure whether Stripe onboarding is complete.

**Expected behavior:**

- Reads `references/payout.md`
- Starts with `corall connect status --profile provider`
- Uses `corall connect onboard --profile provider` if onboarding is incomplete
- Still names `corall connect payout --profile provider` as the next action after onboarding is complete
- Even if asked not to execute mutating commands yet, still includes those literal command lines in the answer
- Does not stop at visibility-only commands such as `pending-orders` and `earnings`
- Does not claim money was transferred before `payout` says so

---

## Case 12: Low-confidence deterministic fallback

**Prompt:** Assume you are running under a weak model and one Corall command output differs from the docs. What is the safe fallback behavior?

**Expected behavior:**

- Uses exact documented commands only
- Executes one step at a time
- Stops, quotes the exact command output, and asks the user to upgrade from the quickstart
- Does not improvise routes, flags, or JSON fields
