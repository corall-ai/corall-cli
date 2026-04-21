const SKILL: &str = include_str!("../skills/corall/SKILL.md");
const ORDER_HANDLE: &str = include_str!("../skills/corall/references/order-handle.md");
const ORDER_CREATE: &str = include_str!("../skills/corall/references/order-create.md");
const SETUP_PROVIDER: &str = include_str!("../skills/corall/references/setup-provider-openclaw.md");
const SKILL_PACKAGE_SUBMIT: &str =
    include_str!("../skills/corall/references/skill-package-submit.md");
const BROWSER_LOGIN: &str = include_str!("../skills/corall/references/browser-login.md");
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
    assert_contains(SKILL, "references/browser-login.md");
    assert_contains(SKILL, "Pass it explicitly on every command");
    assert_contains(SKILL, "Hook verification");
    assert_contains(SKILL, "Never expose a private key");
    assert_contains(PLUGIN_JSON, "OpenClaw polling plugin");
    assert_not_contains(PLUGIN_JSON, "OpenClaw webhook");
}

#[test]
fn order_handle_prompt_accepts_then_submits_with_provider_profile() {
    assert_contains(ORDER_HANDLE, "hook-triggered mode");
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
    assert_not_contains(SETUP_PROVIDER, "\\   #");
}

#[test]
fn eval_cases_and_cli_reference_follow_current_contract() {
    assert_contains(EVAL_CASES, "sessionKey=hook:corall:abc123");
    assert_contains(EVAL_CASES, "until `delivered`");
    assert_not_contains(EVAL_CASES, "SUBMITTED");
    assert_contains(CLI_REFERENCE, "corall skill-packages create");
    assert_contains(CLI_REFERENCE, "corall skill-packages form-template");
    assert_contains(CLI_REFERENCE, "CLI-bundled `corall-polling`");
    assert_contains(CLI_REFERENCE, "corall eventbus serve");
    assert_contains(CLI_REFERENCE, "corall auth browser approve");
    assert_contains(BROWSER_LOGIN, "corall auth browser approve");
    assert_contains(BROWSER_LOGIN, "HttpOnly session cookie");
    assert_contains(
        BROWSER_LOGIN,
        "Do not approve browser login codes from hook-triggered order sessions",
    );
    assert_contains(CLI_REFERENCE, "auto-generated or kept");
    assert_contains(SKILL_PACKAGE_SUBMIT, "\"generatedBy\": \"agent\"");
    assert_contains(
        SKILL_PACKAGE_SUBMIT,
        "SkillHub/ClawHub-style primary categories",
    );
    assert_contains(SKILL_PACKAGE_SUBMIT, "permissions");
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
