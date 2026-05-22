//! Cross-task coordination surface for the kiro-bot runtime.
//!
//! Phase 4 introduces a `Coordinator` trait that abstracts over Slack-event
//! deduplication, per-conversation lease arbitration, peer forwarding, and
//! durable transcript storage. Two impls:
//!
//! - [`NoopCoordinator`]: in-memory, single-task. Used by the CLI/cron frontends, local
//!   development, and unit tests.
//! - `DynamoCoordinator` (Phase 4 follow-up): production multi-task.
//!
//! The trait surface here is the only piece `engine::core` interacts with;
//! the dispatch path doesn't need to know which impl is in use.
//!
//! `NoopCoordinator` is preserved because it exercises the same trait the
//! production impl will, so the engine integration is identical regardless of
//! deployment shape.

use std::collections::{
    HashMap,
    HashSet,
};
use std::sync::Mutex;

use chrono::{
    DateTime,
    Utc,
};
use serde::{
    Deserialize,
    Serialize,
};

/// User vs assistant turn in a conversation transcript.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TurnRole {
    User,
    Assistant,
}

/// One transcript entry. `ts` is when the turn was appended, not when the
/// underlying Slack event was created.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Turn {
    pub role: TurnRole,
    pub text: String,
    pub ts: DateTime<Utc>,
    /// Source paths of the retrieval chunks the assistant used to ground this
    /// turn. Empty for user turns and for assistant turns that didn't go
    /// through `search_kiro_knowledge`. Phase 6 reaction listener reads this
    /// when persisting 👍/👎 feedback so the metric Lambda can correlate bad
    /// answers back to specific docs/issues.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub chunk_ids: Vec<String>,
}

/// Result of a `try_acquire` call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LeaseOutcome {
    /// Caller now owns the conversation lease.
    Acquired,
    /// Some other task owns it; the caller should `forward` to `peer`.
    Held { peer: String },
    /// Coordinator backend is unavailable. Caller may fall back to local
    /// dispatch (best effort) or retry.
    Unavailable,
}

/// Payload handed to `forward` when a peer task owns the lease.
#[derive(Debug, Clone)]
pub struct ForwardEvent {
    pub slack_event_json: serde_json::Value,
}

/// Pluggable backend for cross-task coordination. All methods are async.
#[async_trait::async_trait]
pub trait Coordinator: Send + Sync {
    /// Returns `true` the first time we see this Slack event id; `false` if it
    /// has already been processed (or the dedup write was a no-op).
    async fn dedupe_event(&self, slack_event_id: &str) -> bool;

    /// Try to acquire the conversation lease.
    async fn try_acquire(&self, conversation_id: &str) -> LeaseOutcome;

    /// Renew the lease this task owns.
    async fn renew(&self, conversation_id: &str) -> anyhow::Result<()>;

    /// Release the lease (graceful shutdown).
    async fn release(&self, conversation_id: &str) -> anyhow::Result<()>;

    /// Forward a Slack event to the peer that owns the lease.
    async fn forward(&self, peer: &str, payload: ForwardEvent) -> anyhow::Result<()>;

    /// Append one transcript turn.
    async fn append_turn(&self, conversation_id: &str, turn: Turn) -> anyhow::Result<()>;

    /// Load up to `limit` most-recent turns for the conversation, oldest first.
    async fn load_history(&self, conversation_id: &str, limit: usize) -> anyhow::Result<Vec<Turn>>;
}

/// In-memory implementation. Single-task only (state isn't shared across
/// processes). Used by the CLI/cron frontends, `--coordinator none`, and
/// every unit test in this module.
#[derive(Default)]
pub struct NoopCoordinator {
    state: Mutex<NoopState>,
}

/// In-memory coordinator that mirrors `DynamoCoordinator`'s lease semantics —
/// at most one owner per `conversation_id` at a time, with TTL-based expiry
/// and explicit `Held { peer }` outcomes. Used by Phase 4 HA tests as a
/// drop-in stand-in for the DDB-backed impl when integration tests can't (or
/// shouldn't) reach a real DynamoDB endpoint.
pub struct InMemoryClusterCoordinator {
    /// Cluster-wide shared state. Wrap in `Arc` so multiple "tasks" can hold
    /// distinct identities while sharing one map.
    cluster: std::sync::Arc<Mutex<ClusterState>>,
    own_task_id: String,
    lease_ttl: chrono::Duration,
}

#[derive(Default)]
struct ClusterState {
    seen_events: HashSet<String>,
    leases: HashMap<String, Lease>,
    transcripts: HashMap<String, Vec<Turn>>,
}

#[derive(Clone)]
struct Lease {
    owner: String,
    expires_at: DateTime<Utc>,
}

impl InMemoryClusterCoordinator {
    pub fn new(own_task_id: impl Into<String>, lease_ttl: chrono::Duration) -> Self {
        Self {
            cluster: std::sync::Arc::new(Mutex::new(ClusterState::default())),
            own_task_id: own_task_id.into(),
            lease_ttl,
        }
    }

    /// Spawn a sibling on the same cluster — shares the lease/transcript map.
    pub fn sibling(&self, own_task_id: impl Into<String>) -> Self {
        Self {
            cluster: self.cluster.clone(),
            own_task_id: own_task_id.into(),
            lease_ttl: self.lease_ttl,
        }
    }

    fn now(&self) -> DateTime<Utc> {
        Utc::now()
    }
}

#[async_trait::async_trait]
impl Coordinator for InMemoryClusterCoordinator {
    async fn dedupe_event(&self, slack_event_id: &str) -> bool {
        let mut s = self.cluster.lock().expect("cluster state poisoned");
        s.seen_events.insert(slack_event_id.to_string())
    }

    async fn try_acquire(&self, conversation_id: &str) -> LeaseOutcome {
        let mut s = self.cluster.lock().expect("cluster state poisoned");
        let now = self.now();
        let expires_at = now + self.lease_ttl;
        match s.leases.get(conversation_id).cloned() {
            Some(existing) if existing.owner != self.own_task_id && existing.expires_at > now => {
                LeaseOutcome::Held { peer: existing.owner }
            },
            _ => {
                s.leases.insert(conversation_id.to_string(), Lease {
                    owner: self.own_task_id.clone(),
                    expires_at,
                });
                LeaseOutcome::Acquired
            },
        }
    }

    async fn renew(&self, conversation_id: &str) -> anyhow::Result<()> {
        let mut s = self.cluster.lock().expect("cluster state poisoned");
        if let Some(lease) = s.leases.get_mut(conversation_id)
            && lease.owner == self.own_task_id
        {
            lease.expires_at = self.now() + self.lease_ttl;
            return Ok(());
        }
        anyhow::bail!("renew called on lease not owned by this task")
    }

    async fn release(&self, conversation_id: &str) -> anyhow::Result<()> {
        let mut s = self.cluster.lock().expect("cluster state poisoned");
        if matches!(s.leases.get(conversation_id), Some(l) if l.owner == self.own_task_id) {
            s.leases.remove(conversation_id);
        }
        Ok(())
    }

    async fn forward(&self, _peer: &str, _payload: ForwardEvent) -> anyhow::Result<()> {
        // The cluster is in-process — nothing to do.
        Ok(())
    }

    async fn append_turn(&self, conversation_id: &str, turn: Turn) -> anyhow::Result<()> {
        let mut s = self.cluster.lock().expect("cluster state poisoned");
        s.transcripts.entry(conversation_id.to_string()).or_default().push(turn);
        Ok(())
    }

    async fn load_history(&self, conversation_id: &str, limit: usize) -> anyhow::Result<Vec<Turn>> {
        let s = self.cluster.lock().expect("cluster state poisoned");
        let all = s.transcripts.get(conversation_id).cloned().unwrap_or_default();
        if all.len() <= limit {
            Ok(all)
        } else {
            Ok(all[all.len() - limit..].to_vec())
        }
    }
}

#[derive(Default)]
struct NoopState {
    seen_events: HashSet<String>,
    leases: HashMap<String, ()>,
    transcripts: HashMap<String, Vec<Turn>>,
}

impl NoopCoordinator {
    pub fn new() -> Self {
        Self::default()
    }
}

#[async_trait::async_trait]
impl Coordinator for NoopCoordinator {
    async fn dedupe_event(&self, slack_event_id: &str) -> bool {
        let mut state = self.state.lock().expect("noop state poisoned");
        state.seen_events.insert(slack_event_id.to_string())
    }

    async fn try_acquire(&self, conversation_id: &str) -> LeaseOutcome {
        let mut state = self.state.lock().expect("noop state poisoned");
        state.leases.insert(conversation_id.to_string(), ());
        LeaseOutcome::Acquired
    }

    async fn renew(&self, _conversation_id: &str) -> anyhow::Result<()> {
        Ok(())
    }

    async fn release(&self, conversation_id: &str) -> anyhow::Result<()> {
        let mut state = self.state.lock().expect("noop state poisoned");
        state.leases.remove(conversation_id);
        Ok(())
    }

    async fn forward(&self, _peer: &str, _payload: ForwardEvent) -> anyhow::Result<()> {
        // Single-task by definition — there's nobody to forward to.
        Ok(())
    }

    async fn append_turn(&self, conversation_id: &str, turn: Turn) -> anyhow::Result<()> {
        let mut state = self.state.lock().expect("noop state poisoned");
        state
            .transcripts
            .entry(conversation_id.to_string())
            .or_default()
            .push(turn);
        Ok(())
    }

    async fn load_history(&self, conversation_id: &str, limit: usize) -> anyhow::Result<Vec<Turn>> {
        let state = self.state.lock().expect("noop state poisoned");
        let all = state.transcripts.get(conversation_id).cloned().unwrap_or_default();
        if all.len() <= limit {
            Ok(all)
        } else {
            // Most recent `limit` turns, oldest first.
            Ok(all[all.len() - limit..].to_vec())
        }
    }
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;

    use super::*;

    fn turn(role: TurnRole, text: &str, secs: i64) -> Turn {
        Turn {
            role,
            text: text.to_string(),
            ts: Utc.timestamp_opt(secs, 0).single().unwrap(),
            chunk_ids: Vec::new(),
        }
    }

    #[tokio::test]
    async fn dedupe_event_returns_true_only_first_time() {
        let c = NoopCoordinator::new();
        assert!(c.dedupe_event("evt-1").await, "first sighting must say true");
        assert!(!c.dedupe_event("evt-1").await, "second sighting must say false");
        assert!(c.dedupe_event("evt-2").await, "different id must still say true");
    }

    #[tokio::test]
    async fn noop_coordinator_always_grants_lease() {
        let c = NoopCoordinator::new();
        assert_eq!(c.try_acquire("convo-A").await, LeaseOutcome::Acquired);
        // Even on a second call within the same process — Noop is single-task.
        assert_eq!(c.try_acquire("convo-A").await, LeaseOutcome::Acquired);
    }

    #[tokio::test]
    async fn release_is_idempotent() {
        let c = NoopCoordinator::new();
        c.release("never-acquired").await.unwrap();
        c.try_acquire("convo-A").await;
        c.release("convo-A").await.unwrap();
        c.release("convo-A").await.unwrap();
    }

    #[tokio::test]
    async fn forward_is_a_no_op() {
        let c = NoopCoordinator::new();
        c.forward("peer-1", ForwardEvent {
            slack_event_json: serde_json::json!({"id":"x"}),
        })
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn append_and_load_round_trips_in_order() {
        let c = NoopCoordinator::new();
        c.append_turn("convo-A", turn(TurnRole::User, "hi", 1000))
            .await
            .unwrap();
        c.append_turn("convo-A", turn(TurnRole::Assistant, "hello", 1001))
            .await
            .unwrap();
        c.append_turn("convo-A", turn(TurnRole::User, "thanks", 1002))
            .await
            .unwrap();

        let hist = c.load_history("convo-A", 100).await.unwrap();
        assert_eq!(hist.len(), 3);
        assert_eq!(hist[0].text, "hi");
        assert_eq!(hist[2].text, "thanks");

        let other = c.load_history("convo-B", 100).await.unwrap();
        assert!(other.is_empty(), "unrelated conversation must not bleed in");
    }

    #[tokio::test]
    async fn load_history_respects_limit_keeping_most_recent() {
        let c = NoopCoordinator::new();
        for i in 0..5 {
            c.append_turn("convo-A", turn(TurnRole::User, &format!("turn-{i}"), 1000 + i))
                .await
                .unwrap();
        }
        let hist = c.load_history("convo-A", 2).await.unwrap();
        assert_eq!(hist.len(), 2);
        assert_eq!(hist[0].text, "turn-3", "oldest of the kept window first");
        assert_eq!(hist[1].text, "turn-4");
    }

    #[test]
    fn turn_round_trips_through_json() {
        let t = turn(TurnRole::Assistant, "hello", 1700000000);
        let s = serde_json::to_string(&t).unwrap();
        let back: Turn = serde_json::from_str(&s).unwrap();
        assert_eq!(t, back);
        assert!(s.contains("\"assistant\""));
    }

    #[tokio::test]
    async fn cluster_coordinator_arbitrates_leases_across_two_tasks() {
        let task_a = InMemoryClusterCoordinator::new("task-A", chrono::Duration::minutes(5));
        let task_b = task_a.sibling("task-B");

        // First attempt by A wins.
        assert_eq!(task_a.try_acquire("convo-1").await, LeaseOutcome::Acquired);

        // B sees A's lease.
        match task_b.try_acquire("convo-1").await {
            LeaseOutcome::Held { peer } => assert_eq!(peer, "task-A"),
            other => panic!("expected Held{{ peer = task-A }}, got {other:?}"),
        }

        // Different conversation: B can take it.
        assert_eq!(task_b.try_acquire("convo-2").await, LeaseOutcome::Acquired);
    }

    #[tokio::test]
    async fn cluster_coordinator_lets_owner_release_and_peer_acquire() {
        let task_a = InMemoryClusterCoordinator::new("task-A", chrono::Duration::minutes(5));
        let task_b = task_a.sibling("task-B");

        task_a.try_acquire("convo-1").await;
        assert!(matches!(task_b.try_acquire("convo-1").await, LeaseOutcome::Held { .. }));

        task_a.release("convo-1").await.unwrap();
        assert_eq!(task_b.try_acquire("convo-1").await, LeaseOutcome::Acquired);
    }

    #[tokio::test]
    async fn cluster_coordinator_renew_fails_for_non_owner() {
        let task_a = InMemoryClusterCoordinator::new("task-A", chrono::Duration::minutes(5));
        let task_b = task_a.sibling("task-B");
        task_a.try_acquire("convo-1").await;
        let err = task_b.renew("convo-1").await.unwrap_err();
        assert!(err.to_string().contains("not owned"));
    }

    #[tokio::test]
    async fn cluster_coordinator_dedupe_is_cluster_wide() {
        let task_a = InMemoryClusterCoordinator::new("task-A", chrono::Duration::minutes(5));
        let task_b = task_a.sibling("task-B");

        assert!(task_a.dedupe_event("evt-1").await, "first sighting on A → true");
        assert!(
            !task_b.dedupe_event("evt-1").await,
            "second sighting on B → false (cluster-wide)"
        );
    }

    #[tokio::test]
    async fn cluster_coordinator_transcripts_are_visible_to_siblings() {
        let task_a = InMemoryClusterCoordinator::new("task-A", chrono::Duration::minutes(5));
        let task_b = task_a.sibling("task-B");
        task_a
            .append_turn("convo-1", turn(TurnRole::User, "hi", 100))
            .await
            .unwrap();
        let hist = task_b.load_history("convo-1", 10).await.unwrap();
        assert_eq!(hist.len(), 1);
        assert_eq!(hist[0].text, "hi");
    }
}
