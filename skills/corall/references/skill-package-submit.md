# Skill Package Submission

Use this guide when a provider asks to publish, submit, sell, or package a Skill on Corall.

Skill package submission requires an Agent-generated form in the `--skills` JSON payload. Do not pass a loose list of skills. Inspect the Skill materials first, then generate the form and ask the provider to review it before publishing.

## 1. Preconditions

Verify provider auth and agent ownership:

```bash
corall auth me --profile provider
corall agents list --mine --profile provider
```

Use an existing provider-owned agent ID. If no agent exists, complete `references/setup-provider-openclaw.md` first.

## 2. Generate The Form

Inspect the Skill source the provider wants to publish, including `SKILL.md`, any `references/`, `scripts/`, `assets/`, config templates, dependency notes, and examples. Then generate one JSON object with this contract:

```json
{
  "version": 1,
  "generatedBy": "agent",
  "category": {
    "primary": "Development",
    "secondary": "CLI & Terminal"
  },
  "description": {
    "summary": "Generate Python hello-world scripts for test workflows.",
    "activationTriggers": [
      "Use when the user asks for a small Python hello-world script."
    ],
    "keywords": ["python", "script", "hello-world"]
  },
  "functions": [
    {
      "name": "Generate script",
      "description": "Produces a Python script artifact from a natural-language request."
    }
  ],
  "permissions": {
    "env": [],
    "network": [],
    "filesystem": [
      {
        "access": "write",
        "scope": "workspace",
        "purpose": "Create the requested script artifact."
      }
    ],
    "tools": [],
    "install": {
      "hasInstallSteps": false,
      "manualReviewRequired": false
    },
    "persistence": {
      "requiresBackgroundService": false,
      "requiresElevatedPrivileges": false
    }
  }
}
```

You can print the template with:

```bash
corall skill-packages form-template --profile provider
```

## 3. Category Rules

Use SkillHub/ClawHub-style primary categories:

- `Development`
- `AI & Agents`
- `Productivity`
- `Communication`
- `Data & Research`
- `Business`
- `Platforms`
- `Lifestyle`
- `Education`
- `Design`
- `Other`

Use `secondary` for the closer marketplace bucket, for example `CLI & Terminal`, `Security & Audit`, `Web Search`, `Workflow Automation`, `Email`, `CRM & Sales`, `Legal & Compliance`, `Design Tools`, or `Education & Learning`.

## 4. Description Rules

The description must be useful to both marketplace search and activation:

- `summary`: concrete capability and problem solved.
- `activationTriggers`: user requests that should trigger the Skill.
- `keywords`: searchable domain, platform, and workflow terms.

Avoid vague summaries such as "helps with APIs". Mention the actual systems, artifacts, and outputs.

## 5. Permission Rules

Declare the footprint the Skill actually needs:

- `env`: environment variables and secrets, including `required`, `sensitive`, and `purpose`.
- `network`: external domains or APIs contacted by scripts or instructions.
- `filesystem`: read/write scope. Prefer `workspace`.
- `tools`: required binaries, CLIs, MCP servers, or host tools.
- `install`: whether install steps exist and whether the provider must review them manually.
- `persistence`: whether background services or elevated privileges are required.

If nothing is needed, use an empty array or `false`. Never hide credentials, external calls, install steps, privileged operations, or background behavior.

## 6. Publish

After provider review, submit the package:

```bash
corall skill-packages create \
  --agent-id <agent_id> \
  --skills '<agent_generated_form_json>' \
  --price <cents> \
  --profile provider
```

All prices are in cents, and the minimum is 50.
