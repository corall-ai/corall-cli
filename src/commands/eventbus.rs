use std::net::SocketAddr;

use anyhow::Result;
use clap::Subcommand;

use crate::eventbus::EventBusServeOptions;
use crate::eventbus::EventBusServer;
use crate::eventbus_poller::EventbusPollOptions;

#[derive(Subcommand, Debug)]
pub enum EventbusCommand {
    /// Start the Redis-backed HTTP polling service for agent event delivery
    Serve {
        /// Address for the HTTP server to bind
        #[arg(long, default_value = "127.0.0.1:8787")]
        listen: SocketAddr,

        /// Redis connection URL used for registrations and agent streams
        #[arg(long, default_value = "redis://127.0.0.1:6379/0")]
        redis_url: String,

        /// Redis consumer group name used for per-agent streams
        #[arg(long, default_value = "corall-eventbus")]
        consumer_group: String,

        /// Default long-poll wait in milliseconds
        #[arg(long, default_value_t = 25_000)]
        default_wait_ms: u64,

        /// Maximum allowed long-poll wait in milliseconds
        #[arg(long, default_value_t = 30_000)]
        max_wait_ms: u64,

        /// Default number of events returned per poll
        #[arg(long, default_value_t = 50)]
        default_count: usize,

        /// Maximum number of events returned per poll
        #[arg(long, default_value_t = 100)]
        max_count: usize,

        /// Default idle threshold for reclaiming unacked messages; 0 disables reclaim
        #[arg(long, default_value_t = 30_000)]
        claim_idle_ms: u64,
    },
    /// Long-poll agent order events from the Corall eventbus and deliver them locally
    Poll {
        /// Corall eventbus base URL. Falls back to CORALL_EVENTBUS_URL.
        #[arg(long)]
        base_url: Option<String>,

        /// Agent ID. Defaults to agentId stored in the active credential profile.
        #[arg(long)]
        agent_id: Option<String>,

        /// Eventbus polling bearer token. Falls back to CORALL_WEBHOOK_TOKEN or
        /// pollingToken stored in the active credential profile.
        #[arg(long, alias = "agent-token")]
        webhook_token: Option<String>,

        /// Consumer ID used for the eventbus stream group.
        #[arg(long)]
        consumer_id: Option<String>,

        /// Long-poll wait in milliseconds.
        #[arg(long, default_value_t = 30_000)]
        wait_ms: u64,

        /// HTTP timeout for each poll request. Defaults to waitMs + 15000.
        #[arg(long)]
        request_timeout_ms: Option<u64>,

        /// Timeout for local delivery and ack requests.
        #[arg(long, default_value_t = 10_000)]
        ack_timeout_ms: u64,

        /// Delay after an empty poll result.
        #[arg(long, default_value_t = 1_000)]
        idle_delay_ms: u64,

        /// Initial error backoff after a failed poll cycle.
        #[arg(long, default_value_t = 2_000)]
        error_backoff_ms: u64,

        /// Maximum error backoff after repeated failures.
        #[arg(long, default_value_t = 30_000)]
        max_error_backoff_ms: u64,

        /// Deduplication window for already-forwarded events.
        #[arg(long, default_value_t = 600_000)]
        recent_event_ttl_ms: u64,

        /// Local HTTP endpoint that should receive the hook payload.
        #[arg(long)]
        hook_url: Option<String>,

        /// Optional bearer token for the local hook endpoint.
        #[arg(long)]
        hook_token: Option<String>,

        /// Local program to execute for each event. The JSON event envelope is
        /// written to stdin. Use repeated --exec-arg values for arguments.
        #[arg(long)]
        exec: Option<String>,

        /// Arguments passed to --exec.
        #[arg(long = "exec-arg")]
        exec_args: Vec<String>,
    },
}

pub async fn run(cmd: EventbusCommand, profile: &str) -> Result<()> {
    match cmd {
        EventbusCommand::Serve {
            listen,
            redis_url,
            consumer_group,
            default_wait_ms,
            max_wait_ms,
            default_count,
            max_count,
            claim_idle_ms,
        } => {
            let server = EventBusServer::new(EventBusServeOptions {
                listen,
                redis_url,
                consumer_group,
                default_wait_ms,
                max_wait_ms,
                default_count,
                max_count,
                claim_idle_ms: (claim_idle_ms > 0).then_some(claim_idle_ms),
            })?;
            server.serve().await
        }
        EventbusCommand::Poll {
            base_url,
            agent_id,
            webhook_token,
            consumer_id,
            wait_ms,
            request_timeout_ms,
            ack_timeout_ms,
            idle_delay_ms,
            error_backoff_ms,
            max_error_backoff_ms,
            recent_event_ttl_ms,
            hook_url,
            hook_token,
            exec,
            exec_args,
        } => {
            crate::eventbus_poller::run(
                EventbusPollOptions {
                    base_url,
                    agent_id,
                    webhook_token,
                    consumer_id,
                    wait_ms,
                    request_timeout_ms,
                    ack_timeout_ms,
                    idle_delay_ms,
                    error_backoff_ms,
                    max_error_backoff_ms,
                    recent_event_ttl_ms,
                    hook_url,
                    hook_token,
                    exec,
                    exec_args,
                },
                profile,
            )
            .await
        }
    }
}
