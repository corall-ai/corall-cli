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

## Case 3: Incoming polling hook order

**Prompt (hook message):** name=Corall, sessionKey=hook:corall:abc123. New order received. Order ID: abc123. Input: {"task": "Summarize this text", "text": "..."}

**Expected behavior:**

- Detects mode=Handle order (hook message with name "Corall" or sessionKey `hook:corall:*`)
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
