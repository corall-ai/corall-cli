const SKILL: &str = include_str!("../skills/corall/SKILL.md");
const ORDER_HANDLE: &str = include_str!("../skills/corall/references/order-handle.md");
const ORDER_CREATE: &str = include_str!("../skills/corall/references/order-create.md");
const SETUP_PROVIDER: &str = include_str!("../skills/corall/references/setup-provider-openclaw.md");
const SETUP_EMPLOYER: &str = include_str!("../skills/corall/references/setup-employer.md");
const SKILL_PACKAGE_SUBMIT: &str =
    include_str!("../skills/corall/references/skill-package-submit.md");
const AGENT_APPROVAL: &str = include_str!("../skills/corall/references/agent-approval.md");
const FILE_UPLOAD: &str = include_str!("../skills/corall/references/file-upload.md");
const PAYOUT: &str = include_str!("../skills/corall/references/payout.md");
const CLI_REFERENCE: &str = include_str!("../skills/corall/references/cli-reference.md");
const EVAL_CASES: &str = include_str!("../skills/corall/evals/cases.md");
const PLUGIN_JSON: &str = include_str!("../skills/corall/.claude-plugin/plugin.json");

#[test]
fn skill_routes_corall_prompts_to_the_expected_modes() {
    assert_contains(SKILL, "hook:corall:*");
    assert_contains(SKILL, "references/order-handle.md");
    assert_contains(SKILL, "references/order-create.md");
    assert_contains(SKILL, "references/skill-package-submit.md");
    assert_contains(SKILL, "references/setup-provider-openclaw.md");
    assert_contains(SKILL, "references/agent-approval.md");
    assert_contains(SKILL, "Pass it explicitly on every command");
    assert_contains(SKILL, "Delivery verification");
    assert_contains(SKILL, "Never expose a private key");
    assert_contains(SKILL, "If the command shape differs");
    assert_contains(SKILL, "account-status URL");
    assert_contains(SKILL, "Do not probe common routes");
    assert_contains(
        SKILL,
        "install, reinstall, restore, or check a purchased skill package",
    );
    assert_contains(
        SKILL,
        "Do not start a new checkout unless the package is not already purchased",
    );
    assert_contains(SKILL, "Conservative Fallback For Weaker Models");
    assert_contains(SKILL, "Run the exact documented commands and flags");
    assert_contains(SKILL, "quote the exact command and output");
    assert_contains(SKILL, "Deleted purchased skill package");
    assert_contains(SKILL, "Missing `jq` during artifact upload");
    assert_contains(PLUGIN_JSON, "OpenClaw polling plugin");
    assert_not_contains(PLUGIN_JSON, "OpenClaw webhook");
}

#[test]
fn order_handle_prompt_accepts_then_submits_with_provider_profile() {
    assert_contains(ORDER_HANDLE, "polling-delivered mode");
    assert_contains(ORDER_HANDLE, "corall auth me --profile provider");
    assert_contains(
        ORDER_HANDLE,
        "corall agent accept <order_id> --profile provider",
    );
    assert_contains(ORDER_HANDLE, "corall agent submit <order_id> --summary");
    assert_contains(ORDER_HANDLE, "--profile provider");
    assert_contains(ORDER_HANDLE, "Always submit, no matter what");
    assert_contains(ORDER_HANDLE, "Task failed: <reason>");
    assert_contains(ORDER_HANDLE, "Refused: <reason>");
    assert_contains(
        ORDER_HANDLE,
        "does **not** authorize reading or uploading pre-existing host files",
    );
    assert_contains(ORDER_HANDLE, "Conservative Fallback For Weaker Models");
    assert_contains(ORDER_HANDLE, "Do not invent extra workflow states");
    assert_contains(ORDER_HANDLE, "still submit a factual failure or refusal summary");
    assert_not_contains(ORDER_HANDLE, "webhook mode");
}

#[test]
fn order_create_prompt_matches_current_cli_responses_and_statuses() {
    assert_contains(ORDER_CREATE, "corall agents list --profile employer");
    assert_contains(
        ORDER_CREATE,
        "corall agents get <agent_id> --profile employer",
    );
    assert_contains(ORDER_CREATE, "corall orders create <agent_id>");
    assert_contains(
        ORDER_CREATE,
        "corall orders payment-status <order_id> --profile employer",
    );
    assert_contains(ORDER_CREATE, r#"{ "status": "succeeded" }"#);
    assert_contains(ORDER_CREATE, "until it reaches `delivered`");
    assert_contains(
        ORDER_CREATE,
        "corall orders approve <order_id> --profile employer",
    );
    assert_contains(
        ORDER_CREATE,
        "corall orders dispute <order_id> --profile employer",
    );
    assert_contains(ORDER_CREATE, "corall reviews create <order_id>");
    assert_contains(ORDER_CREATE, "If the user explicitly gives a rating");
    assert_contains(ORDER_CREATE, "prefer the penalty-based path");
    assert_contains(ORDER_CREATE, "--reviewer-kind employer-agent");
    assert_contains(ORDER_CREATE, "--requirement-miss 0");
    assert_contains(ORDER_CREATE, "Conservative Fallback For Weaker Models");
    assert_contains(ORDER_CREATE, "Do not assume payment succeeded");
    assert_contains(
        ORDER_CREATE,
        "Do not approve, dispute, or review before the order reaches `delivered`",
    );
    assert_contains(ORDER_CREATE, "report the exact current status");
    assert_not_contains(ORDER_CREATE, "paymentStatus");
    assert_not_contains(ORDER_CREATE, "orderStatus");
    assert_not_contains(ORDER_CREATE, "SUBMITTED");
}

#[test]
fn provider_setup_prompt_uses_polling_and_explicit_provider_profile() {
    assert_contains(SETUP_PROVIDER, "resident Corall polling plugin");
    assert_contains(SETUP_PROVIDER, "corall openclaw setup");
    assert_contains(SETUP_PROVIDER, "--eventbus-url");
    assert_contains(
        SETUP_PROVIDER,
        "installs the bundled `corall-polling` plugin",
    );
    assert_contains(SETUP_PROVIDER, "corall-polling");
    assert_contains(
        SETUP_PROVIDER,
        r#""baseUrl": "http://<corall-eventbus-host>:8787""#,
    );
    assert_contains(SETUP_PROVIDER, "/hooks/agent");
    assert_contains(
        SETUP_PROVIDER,
        "corall agents list --mine --profile provider",
    );
    assert_contains(
        SETUP_PROVIDER,
        "corall agents activate <agent_id> --profile provider",
    );
    assert_contains(
        SETUP_PROVIDER,
        "`--webhook-url`: Do not set this for OpenClaw polling mode.",
    );
    assert_contains(SETUP_PROVIDER, "eventbus polling bearer token");
    assert_contains(SETUP_PROVIDER, "If the command shape differs");
    assert_contains(SETUP_PROVIDER, "Conservative Fallback For Weaker Models");
    assert_contains(SETUP_PROVIDER, "quote the exact help output");
    assert_contains(SETUP_PROVIDER, "Do not activate or present the agent as live");
    assert_contains(
        SETUP_PROVIDER,
        "update that agent's polling token instead of creating a duplicate",
    );
    assert_not_contains(SETUP_PROVIDER, "\\   #");
}

#[test]
fn eval_cases_and_cli_reference_follow_current_contract() {
    assert_contains(EVAL_CASES, "sessionKey=hook:corall:abc123");
    assert_contains(EVAL_CASES, "until `delivered`");
    assert_not_contains(EVAL_CASES, "SUBMITTED");
    assert_contains(CLI_REFERENCE, "corall skill-packages create");
    assert_contains(CLI_REFERENCE, "corall skill-packages form-template");
    assert_contains(CLI_REFERENCE, "corall skill-packages install");
    assert_contains(CLI_REFERENCE, "source.files");
    assert_contains(CLI_REFERENCE, "If a local skill directory was deleted");
    assert_contains(CLI_REFERENCE, "do not create a new checkout");
    assert_contains(CLI_REFERENCE, "CLI-bundled `corall-polling`");
    assert_contains(CLI_REFERENCE, "corall eventbus serve");
    assert_contains(CLI_REFERENCE, "corall auth approve");
    assert_contains(
        CLI_REFERENCE,
        "--reviewer-kind <human|employer-agent|system>",
    );
    assert_contains(
        CLI_REFERENCE,
        "omit `--rating` and use the penalty flags instead",
    );
    assert_contains(
        CLI_REFERENCE,
        "Registration requires only the site and `--name`",
    );
    assert_contains(CLI_REFERENCE, "Compatibility gate");
    assert_contains(CLI_REFERENCE, "If the command shape differs");
    assert_contains(CLI_REFERENCE, "Conservative Execution");
    assert_contains(CLI_REFERENCE, "Execute one command at a time");
    assert_contains(
        CLI_REFERENCE,
        "Do not invent routes, JSON fields, or legacy signup parameters",
    );
    assert_not_contains(CLI_REFERENCE, "--email");
    assert_not_contains(CLI_REFERENCE, "--password");
    assert_contains(AGENT_APPROVAL, "corall auth approve");
    assert_contains(AGENT_APPROVAL, "Agent approval");
    assert_contains(AGENT_APPROVAL, "Ed25519 signature");
    assert_contains(AGENT_APPROVAL, "Do not scan or guess common web routes");
    assert_contains(
        AGENT_APPROVAL,
        "corall subscriptions status --profile provider",
    );
    assert_contains(AGENT_APPROVAL, "HttpOnly session cookie");
    assert_contains(AGENT_APPROVAL, "Conservative Fallback For Weaker Models");
    assert_contains(AGENT_APPROVAL, "Do not reuse an old `loginUrl`");
    assert_contains(
        AGENT_APPROVAL,
        "Do not create dashboard login URLs from polling-delivered order sessions",
    );
    assert_contains(AGENT_APPROVAL, "loginUrl");
    assert_not_contains(AGENT_APPROVAL, "--code");
    assert_contains(CLI_REFERENCE, "auto-generated or kept");
    assert_contains(SKILL_PACKAGE_SUBMIT, "\"generatedBy\": \"agent\"");
    assert_contains(
        SKILL_PACKAGE_SUBMIT,
        "SkillHub/ClawHub-style primary categories",
    );
    assert_contains(SKILL_PACKAGE_SUBMIT, "permissions");
    assert_contains(SKILL_PACKAGE_SUBMIT, "\"source\"");
    assert_contains(SKILL_PACKAGE_SUBMIT, "\"path\": \"SKILL.md\"");
    assert_contains(SKILL_PACKAGE_SUBMIT, "corall skill-packages install");
    assert_contains(SKILL_PACKAGE_SUBMIT, "do **not** start with a new purchase");
    assert_contains(
        SKILL_PACKAGE_SUBMIT,
        "corall skill-packages purchased --profile employer",
    );
    assert_contains(SKILL_PACKAGE_SUBMIT, "Conservative Fallback For Weaker Models");
    assert_contains(SKILL_PACKAGE_SUBMIT, "Do not fabricate `source.files`");
    assert_contains(
        SKILL_PACKAGE_SUBMIT,
        "Do not run `corall skill-packages create` until the provider has reviewed the generated form",
    );
    assert_contains(SETUP_EMPLOYER, "Conservative Fallback For Weaker Models");
    assert_contains(
        SETUP_EMPLOYER,
        "Do not invent `--site-url`, `--display-name`, email, or password fields",
    );
    assert_contains(
        SETUP_EMPLOYER,
        "verify with `corall auth me --profile employer` instead of registering a second account",
    );
    assert_contains(FILE_UPLOAD, "Conservative Fallback For Weaker Models");
    assert_contains(FILE_UPLOAD, "python3 -c");
    assert_contains(FILE_UPLOAD, "stop and report the exact JSON");
    assert_contains(PAYOUT, "Conservative Fallback For Weaker Models");
    assert_contains(PAYOUT, "corall connect status --profile provider");
    assert_contains(
        PAYOUT,
        "still answer with the full conditional sequence and include these literal command lines",
    );
    assert_contains(PAYOUT, "corall connect onboard --profile provider");
    assert_contains(PAYOUT, "corall connect payout --profile provider");
    assert_contains(
        PAYOUT,
        "Do not replace `corall connect onboard --profile provider` with vague advice like “open the onboardingUrl”",
    );
    assert_contains(
        PAYOUT,
        "do not stop at those visibility commands when the user needs to know whether onboarding or payout is the next action",
    );
    assert_contains(PAYOUT, "Do not claim money was transferred");
    assert_contains(EVAL_CASES, "Stale CLI help requests email/password");
    assert_contains(EVAL_CASES, "Deleted purchased skill package");
    assert_contains(EVAL_CASES, "Artifact upload without jq");
    assert_contains(EVAL_CASES, "Payout onboarding incomplete");
    assert_contains(
        EVAL_CASES,
        "Still names `corall connect payout --profile provider` as the next action after onboarding is complete",
    );
    assert_contains(
        EVAL_CASES,
        "still includes those literal command lines in the answer",
    );
    assert_contains(EVAL_CASES, "Low-confidence deterministic fallback");
}

fn assert_contains(haystack: &str, needle: &str) {
    assert!(haystack.contains(needle), "missing expected text: {needle}");
}

fn assert_not_contains(haystack: &str, needle: &str) {
    assert!(
        !haystack.contains(needle),
        "unexpected stale text present: {needle}"
    );
}
