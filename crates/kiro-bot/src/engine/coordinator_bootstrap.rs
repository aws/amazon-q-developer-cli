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
const ENV_APPROVALS: &str = "KIRO_BOT_APPROVALS_TABLE";

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
            let approvals = std::env::var(ENV_APPROVALS).ok();
            info!(
                own_id = %own_id,
                leases = %leases,
                transcripts = %transcripts,
                dedup = %dedup,
                approvals = ?approvals,
                "coordinator: DynamoDB"
            );
            let aws_cfg = aws_config::load_defaults(aws_config::BehaviorVersion::latest()).await;
            let client = aws_sdk_dynamodb::Client::new(&aws_cfg);
            let mut coord = DynamoCoordinator::new(client, leases, transcripts, dedup, own_id);
            if let Some(t) = approvals {
                coord = coord.with_approvals_table(t);
            }
            Arc::new(coord)
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

/// Phase 2 [`Dispatcher`]: deserialize the forwarded JSON into a
/// `SlackPushEventCallback` and run it through the same
/// [`crate::frontend::slack::dispatch_event`] path Slack-delivered events
/// use. After this point the in-process `pending_approvals` lookup, the
/// approval listener, and the per-conversation worker behave exactly as if
/// Slack had delivered the event natively.
pub struct BotCoreDispatcher {
    state: Arc<crate::frontend::slack::SlackState>,
}

impl BotCoreDispatcher {
    pub fn new(state: Arc<crate::frontend::slack::SlackState>) -> Self {
        Self { state }
    }
}

#[async_trait::async_trait]
impl crate::engine::dispatch_server::Dispatcher for BotCoreDispatcher {
    async fn process_as_if_from_slack(&self, event: serde_json::Value) {
        let parsed: slack_morphism::prelude::SlackPushEventCallback = match serde_json::from_value(event.clone()) {
            Ok(v) => v,
            Err(e) => {
                warn!(error = %e, "dispatch: failed to parse forwarded Slack event JSON");
                tracing::debug!(?event, "unparsable payload");
                return;
            },
        };
        if let Err(e) = crate::frontend::slack::dispatch_event(parsed, &self.state, true).await {
            warn!(error = %e, "dispatch: forwarded event handler returned error");
        }
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
        assert_eq!(outcome, crate::engine::coordinator::LeaseOutcome::Acquired);
    }

    /// Sanity check: the JSON shape we expect from a peer's `forward` POST
    /// (`{"type":"event_callback","event":{...}}`) deserializes into the
    /// slack-morphism type the dispatcher feeds into `dispatch_event`. If
    /// slack-morphism's serde shape ever drifts from what real Slack sends,
    /// this test breaks before production does.
    #[test]
    fn parses_reaction_added_payload() {
        // Minimal `reaction_added` event_callback envelope. Field names must
        // match slack-morphism's serde. Mirrors what handle_reaction expects.
        let payload = serde_json::json!({
            "token": "test",
            "team_id": "T0",
            "api_app_id": "A0",
            "event": {
                "type": "reaction_added",
                "user": "U1",
                "reaction": "white_check_mark",
                "item": {
                    "type": "message",
                    "channel": "C1",
                    "ts": "1700000000.000100"
                },
                "item_user": "U2",
                "event_ts": "1700000000.000200"
            },
            "type": "event_callback",
            "event_id": "Ev1",
            "event_time": 1700000000,
            "authed_users": []
        });
        let parsed: Result<slack_morphism::prelude::SlackPushEventCallback, _> = serde_json::from_value(payload);
        assert!(
            parsed.is_ok(),
            "reaction_added payload must deserialize: {:?}",
            parsed.err()
        );
    }
}
