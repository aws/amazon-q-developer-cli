//! Pick a [`Coordinator`] impl at startup based on environment.
//!
//! The Fargate runtime stack injects `KIRO_BOT_*` env vars naming the four
//! DynamoDB tables and the dispatch port. When all of `KIRO_BOT_LEASES_TABLE`,
//! `KIRO_BOT_TRANSCRIPTS_TABLE`, and `KIRO_BOT_DEDUP_TABLE` are set we wire up
//! a [`DynamoCoordinator`]; otherwise we keep the in-memory
//! [`NoopCoordinator`] so local CLI runs (`cmd_chat`) and unit tests are
//! unaffected.
//!
//! Phase 1 only *constructs* the coordinator; the engine integration
//! (`dedupe_event`, `try_acquire`, `forward`) lands in Phase 3.

use std::sync::Arc;

use tracing::{
    info,
    warn,
};

use crate::engine::coordinator::{
    Coordinator,
    NoopCoordinator,
};
use crate::engine::dynamo_coordinator::DynamoCoordinator;
use crate::engine::task_metadata;

const ENV_LEASES: &str = "KIRO_BOT_LEASES_TABLE";
const ENV_TRANSCRIPTS: &str = "KIRO_BOT_TRANSCRIPTS_TABLE";
const ENV_DEDUP: &str = "KIRO_BOT_DEDUP_TABLE";

/// Bind a [`Coordinator`] for the current process. Always succeeds — on
/// misconfiguration we log and fall back to [`NoopCoordinator`] so the bot
/// keeps running in a degraded (single-task) mode.
pub async fn build_coordinator() -> Arc<dyn Coordinator> {
    let leases = std::env::var(ENV_LEASES).ok();
    let transcripts = std::env::var(ENV_TRANSCRIPTS).ok();
    let dedup = std::env::var(ENV_DEDUP).ok();

    match (leases, transcripts, dedup) {
        (Some(leases), Some(transcripts), Some(dedup)) => {
            let own_id = task_metadata::resolve_self_id().await;
            info!(
                own_id = %own_id,
                leases = %leases,
                transcripts = %transcripts,
                dedup = %dedup,
                "coordinator: DynamoDB"
            );
            let aws_cfg = aws_config::load_defaults(aws_config::BehaviorVersion::latest()).await;
            let client = aws_sdk_dynamodb::Client::new(&aws_cfg);
            Arc::new(DynamoCoordinator::new(client, leases, transcripts, dedup, own_id))
        },
        _ => {
            // Treat partial env as misconfiguration — most often local dev.
            // Logging at info (not warn) since it's the common case for
            // `kiro-bot chat`.
            info!("coordinator: in-memory (Noop) — set KIRO_BOT_*_TABLE env vars to use DynamoDB");
            Arc::new(NoopCoordinator::new())
        },
    }
}

/// Stub [`Dispatcher`] for Phase 1: log the inbound payload and 200 it.
/// Phase 2 swaps in a real implementation that re-enters the Slack event
/// pipeline.
pub struct LoggingDispatcher;

#[async_trait::async_trait]
impl crate::engine::dispatch_server::Dispatcher for LoggingDispatcher {
    async fn process_as_if_from_slack(&self, event: serde_json::Value) {
        let kind = event
            .get("type")
            .and_then(|v| v.as_str())
            .unwrap_or("<missing-type>");
        // Trace the full payload at debug; emit a single info line per event
        // so we can grep for forwards in production.
        warn!(
            event_type = %kind,
            "dispatch /dispatch received but Phase 2 handler not wired yet — payload dropped"
        );
        tracing::debug!(?event, "forwarded event payload");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// SAFETY: each test removes the three table env vars before running so
    /// they don't observe state from the runtime environment. They run
    /// serially within a single test binary, so the unsafe `remove_var`
    /// calls are race-free in practice.
    fn clear_env() {
        unsafe {
            std::env::remove_var(ENV_LEASES);
            std::env::remove_var(ENV_TRANSCRIPTS);
            std::env::remove_var(ENV_DEDUP);
        }
    }

    #[tokio::test]
    async fn falls_back_to_noop_when_env_missing() {
        clear_env();
        let coord = build_coordinator().await;
        // Noop always returns Acquired.
        let outcome = coord.try_acquire("convo-test").await;
        assert_eq!(
            outcome,
            crate::engine::coordinator::LeaseOutcome::Acquired
        );
    }
}
