use anyhow::Result;
use clap::Args;
use clap::Subcommand;
use clap::ValueEnum;
use serde_json::Value;
use serde_json::json;

use crate::commands::openclaw;

#[derive(Subcommand, Debug)]
pub enum RuntimeCommand {
    /// Configure Corall delivery for an AI agent runtime
    Setup(RuntimeSetupArgs),
}

#[derive(Args, Debug, Clone)]
pub struct RuntimeSetupArgs {
    /// Agent runtime adapter to configure
    #[arg(long, value_enum, default_value_t = RuntimeKind::Generic)]
    runtime: RuntimeKind,

    /// Local HTTP endpoint that should receive Corall hook payloads in generic mode
    #[arg(long)]
    hook_url: Option<String>,

    /// Local program to execute for each event in generic mode
    #[arg(long)]
    exec: Option<String>,

    /// Arguments passed to --exec in generic mode
    #[arg(long = "exec-arg")]
    exec_args: Vec<String>,

    #[command(flatten)]
    openclaw: openclaw::OpenclawSetupArgs,
}

#[derive(Clone, Debug, ValueEnum)]
enum RuntimeKind {
    Generic,
    Openclaw,
}

pub async fn run(cmd: RuntimeCommand) -> Result<()> {
    match cmd {
        RuntimeCommand::Setup(args) => {
            let result = setup_runtime(args)?;
            println!("{}", serde_json::to_string_pretty(&result)?);
        }
    }
    Ok(())
}

fn setup_runtime(args: RuntimeSetupArgs) -> Result<Value> {
    match args.runtime {
        RuntimeKind::Openclaw => openclaw::setup_openclaw(args.openclaw),
        RuntimeKind::Generic => Ok(generic_setup_result(args)),
    }
}

fn generic_setup_result(args: RuntimeSetupArgs) -> Value {
    let delivery = match (args.hook_url.as_deref(), args.exec.as_deref()) {
        (Some(hook_url), None) => json!({
            "mode": "hook",
            "hookUrl": hook_url,
        }),
        (None, Some(program)) => json!({
            "mode": "exec",
            "program": program,
            "args": args.exec_args,
        }),
        (Some(_), Some(_)) => json!({
            "mode": "invalid",
            "message": "choose either --hook-url or --exec before starting corall eventbus poll",
        }),
        (None, None) => json!({
            "mode": "pending",
            "message": "choose the AI agent runtime's local delivery target: --hook-url for HTTP or --exec for stdin JSON",
        }),
    };

    json!({
        "runtime": "generic",
        "testedAdapters": ["openclaw", "hermes"],
        "compatibility": {
            "status": "best-effort",
            "message": "Other AI agent runtimes can use Corall polling when they can accept a local HTTP hook or stdin JSON command delivery."
        },
        "eventbusUrl": args.openclaw.eventbus_url,
        "delivery": delivery,
        "nextCommand": "corall eventbus poll --base-url <eventbus-url> --profile provider --webhook-token <polling-token> --hook-url <local-hook-url>",
        "execCommand": "corall eventbus poll --base-url <eventbus-url> --profile provider --webhook-token <polling-token> --exec <program> --exec-arg <arg>",
    })
}
