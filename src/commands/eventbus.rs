use std::net::SocketAddr;

use anyhow::Result;
use clap::Subcommand;

use crate::eventbus::EventBusServeOptions;
use crate::eventbus::EventBusServer;

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
}

pub async fn run(cmd: EventbusCommand) -> Result<()> {
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
    }
}
