//! Cross-task coordination contracts and runtime implementations.

use std::sync::atomic::{
    AtomicU64,
    Ordering,
};
use std::time::Duration as StdDuration;

use chrono::{
    DateTime,
    Utc,
};
use serde::{
    Deserialize,
    Serialize,
};

mod in_memory;
mod lease_manager;

pub use in_memory::{
    InMemoryClusterCoordinator,
    NoopCoordinator,
};
pub use lease_manager::LeaseManager;
pub(crate) use lease_manager::{
    LeaseGuard,
    ManagedLeaseAcquisition,
};

/// User vs assistant turn in a conversation transcript.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TurnRole {
    User,
    Assistant,
}

/// One transcript entry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Turn {
    pub role: TurnRole,
    pub text: String,
    pub ts: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub chunk_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DedupeOutcome {
    Accepted { token: DedupeToken },
    Duplicate,
    Unavailable,
}

#[derive(Clone, PartialEq, Eq)]
pub struct DedupeToken(String);

impl DedupeToken {
    pub fn new(owner: &str) -> Self {
        static NEXT_TOKEN: AtomicU64 = AtomicU64::new(1);
        Self(format!("{owner}:{}", NEXT_TOKEN.fetch_add(1, Ordering::Relaxed)))
    }

    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Debug for DedupeToken {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.debug_tuple("DedupeToken").field(&self.0).finish()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LeaseAcquisition {
    Acquired { token: LeaseToken },
    Held { peer: String },
    Unavailable,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RateLimitOutcome {
    Allowed,
    Limited { retry_after: StdDuration },
}

#[derive(Clone, PartialEq, Eq)]
pub struct LeaseToken(String);

impl LeaseToken {
    pub fn new(owner: &str) -> Self {
        static NEXT_TOKEN: AtomicU64 = AtomicU64::new(1);
        Self(format!("{owner}:{}", NEXT_TOKEN.fetch_add(1, Ordering::Relaxed)))
    }

    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }

    pub(crate) fn from_stored(value: String) -> Self {
        Self(value)
    }
}

impl std::fmt::Debug for LeaseToken {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.debug_tuple("LeaseToken").field(&self.0).finish()
    }
}

#[derive(Debug, Clone)]
pub struct ForwardEvent {
    pub slack_event_json: serde_json::Value,
}

#[async_trait::async_trait]
pub trait Coordinator: Send + Sync {
    async fn dedupe_event_outcome(&self, slack_event_id: &str) -> DedupeOutcome;

    async fn dedupe_event(&self, slack_event_id: &str) -> bool {
        matches!(
            self.dedupe_event_outcome(slack_event_id).await,
            DedupeOutcome::Accepted { .. }
        )
    }

    async fn release_dedup(&self, slack_event_id: &str, token: &DedupeToken) -> anyhow::Result<()>;

    async fn acquire_lease(&self, conversation_id: &str) -> LeaseAcquisition;

    async fn renew(&self, conversation_id: &str, token: &LeaseToken) -> anyhow::Result<()>;

    fn lease_heartbeat_interval(&self) -> StdDuration {
        StdDuration::from_secs(60)
    }

    fn lease_ttl(&self) -> StdDuration {
        StdDuration::from_secs(300)
    }

    async fn release(&self, conversation_id: &str, token: &LeaseToken) -> anyhow::Result<()>;

    async fn forward(&self, peer: &str, payload: ForwardEvent) -> anyhow::Result<()>;

    async fn append_turn(&self, conversation_id: &str, turn: Turn) -> anyhow::Result<()>;

    async fn load_history(&self, conversation_id: &str, limit: usize) -> anyhow::Result<Vec<Turn>>;

    async fn register_approval(
        &self,
        slack_msg_ts: &str,
        conversation_id: &str,
        ttl: StdDuration,
    ) -> anyhow::Result<()>;

    async fn lookup_approval_owner(&self, slack_msg_ts: &str) -> anyhow::Result<Option<String>>;

    async fn admit_prompt(
        &self,
        _user_id: &str,
        _max_prompts: u32,
        _window: StdDuration,
    ) -> anyhow::Result<RateLimitOutcome> {
        Ok(RateLimitOutcome::Allowed)
    }
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;

    use super::*;

    #[test]
    fn turn_round_trips_through_json() {
        let turn = Turn {
            role: TurnRole::Assistant,
            text: "hello".to_string(),
            ts: Utc.timestamp_opt(1_700_000_000, 0).single().unwrap(),
            chunk_ids: Vec::new(),
        };
        let serialized = serde_json::to_string(&turn).unwrap();
        let deserialized: Turn = serde_json::from_str(&serialized).unwrap();
        assert_eq!(turn, deserialized);
        assert!(serialized.contains("\"assistant\""));
    }
}
