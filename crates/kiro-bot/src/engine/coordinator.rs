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
use std::sync::atomic::{
    AtomicU64,
    Ordering,
};
use std::sync::{
    Arc,
    LazyLock,
    Mutex,
    Weak,
};

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

    /// Force-acquire the lease when the previous holder is unreachable.
    /// Succeeds iff the existing lease is unowned, expired, or owned by
    /// `dead_peer`. Used by the dispatch path to recover when `forward()` to
    /// the lease holder fails — the caller assumes the peer is dead and
    /// claims the lease for itself rather than dropping the user's message.
    async fn force_acquire(&self, conversation_id: &str, dead_peer: &str) -> bool;

    /// Renew the lease this task owns.
    async fn renew(&self, conversation_id: &str) -> anyhow::Result<()>;

    /// Cadence used while a dispatch owns a lease.
    fn lease_heartbeat_interval(&self) -> std::time::Duration {
        std::time::Duration::from_secs(60)
    }

    /// Maximum time the lease remains valid without a successful renewal.
    fn lease_ttl(&self) -> std::time::Duration {
        std::time::Duration::from_secs(300)
    }

    /// Release the lease (graceful shutdown).
    async fn release(&self, conversation_id: &str) -> anyhow::Result<()>;

    /// Forward a Slack event to the peer that owns the lease.
    async fn forward(&self, peer: &str, payload: ForwardEvent) -> anyhow::Result<()>;

    /// Append one transcript turn.
    async fn append_turn(&self, conversation_id: &str, turn: Turn) -> anyhow::Result<()>;

    /// Load up to `limit` most-recent turns for the conversation, oldest first.
    async fn load_history(&self, conversation_id: &str, limit: usize) -> anyhow::Result<Vec<Turn>>;

    /// Record that this task owns the pending approval keyed by `slack_msg_ts`.
    /// Stored with a TTL so a crashed task's row eventually expires.
    ///
    /// The `pending_approvals` map in `frontend::slack` is per-process, so a
    /// reaction delivered to a peer task whose map is empty needs a way to
    /// learn which task does own the approval. Phase 3 writes this row at the
    /// moment the approval prompt is posted; the reaction handler reads it on
    /// a local miss and forwards to the owner via [`Coordinator::forward`].
    async fn register_approval(
        &self,
        slack_msg_ts: &str,
        conversation_id: &str,
        ttl: std::time::Duration,
    ) -> anyhow::Result<()>;

    /// Read back the owning task identity for a pending approval, if any.
    /// Returns `Ok(None)` when the row is missing or expired.
    async fn lookup_approval_owner(&self, slack_msg_ts: &str) -> anyhow::Result<Option<String>>;
}

type LocalLeaseKey = (usize, String);

struct LocalLeaseEntry {
    token: u64,
    lease: Weak<LeaseGuardInner>,
}

static LOCAL_LEASES: LazyLock<Mutex<HashMap<LocalLeaseKey, LocalLeaseEntry>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));
static NEXT_LEASE_TOKEN: AtomicU64 = AtomicU64::new(1);

pub(crate) struct LeaseGuard {
    inner: Arc<LeaseGuardInner>,
}

struct LeaseGuardInner {
    coordinator: Arc<dyn Coordinator>,
    conversation_id: String,
    request_id: String,
    heartbeat: tokio::task::JoinHandle<()>,
    loss_rx: tokio::sync::watch::Receiver<Option<String>>,
    registry_key: LocalLeaseKey,
    token: u64,
}

impl LeaseGuard {
    pub(crate) fn start(coordinator: Arc<dyn Coordinator>, conversation_id: String, request_id: String) -> Self {
        let coordinator_id = Arc::as_ptr(&coordinator) as *const () as usize;
        let registry_key = (coordinator_id, conversation_id.clone());
        let mut local_leases = LOCAL_LEASES.lock().unwrap();
        if let Some(inner) = local_leases.get(&registry_key).and_then(|entry| entry.lease.upgrade()) {
            tracing::debug!(
                %request_id,
                conversation_id = %conversation_id,
                "sharing locally-owned coordinator lease"
            );
            return Self { inner };
        }

        let token = NEXT_LEASE_TOKEN.fetch_add(1, Ordering::Relaxed);
        let interval = coordinator.lease_heartbeat_interval();
        let ttl = coordinator.lease_ttl();
        let heartbeat_coordinator = coordinator.clone();
        let heartbeat_conversation = conversation_id.clone();
        let heartbeat_request = request_id.clone();
        let (loss_tx, loss_rx) = tokio::sync::watch::channel(None);
        let heartbeat = tokio::spawn(async move {
            let start = tokio::time::Instant::now() + interval;
            let mut ticks = tokio::time::interval_at(start, interval);
            let mut expires_at = tokio::time::Instant::now() + ttl;
            loop {
                ticks.tick().await;
                let failure = match tokio::time::timeout(
                    std::time::Duration::from_secs(5),
                    heartbeat_coordinator.renew(&heartbeat_conversation),
                )
                .await
                {
                    Ok(Ok(())) => {
                        expires_at = tokio::time::Instant::now() + ttl;
                        tracing::debug!(
                            request_id = %heartbeat_request,
                            conversation_id = %heartbeat_conversation,
                            "coordinator lease renewed"
                        );
                        continue;
                    },
                    Ok(Err(error)) => {
                        tracing::warn!(
                            request_id = %heartbeat_request,
                            conversation_id = %heartbeat_conversation,
                            %error,
                            "coordinator lease renewal failed"
                        );
                        error.to_string()
                    },
                    Err(_) => {
                        tracing::warn!(
                            request_id = %heartbeat_request,
                            conversation_id = %heartbeat_conversation,
                            "coordinator lease renewal timed out"
                        );
                        "lease renewal timed out".to_string()
                    },
                };
                let ownership_lost = failure.contains("no longer owned") || failure.contains("not owned");
                if ownership_lost || tokio::time::Instant::now() + interval >= expires_at {
                    tracing::error!(
                        request_id = %heartbeat_request,
                        conversation_id = %heartbeat_conversation,
                        %failure,
                        "coordinator lease lost; cancelling active dispatch"
                    );
                    loss_tx.send_replace(Some(failure));
                    break;
                }
            }
        });
        let inner = Arc::new(LeaseGuardInner {
            coordinator,
            conversation_id,
            request_id,
            heartbeat,
            loss_rx,
            registry_key: registry_key.clone(),
            token,
        });
        local_leases.insert(registry_key, LocalLeaseEntry {
            token,
            lease: Arc::downgrade(&inner),
        });
        Self { inner }
    }

    pub(crate) fn subscribe_loss(&self) -> tokio::sync::watch::Receiver<Option<String>> {
        self.inner.loss_rx.clone()
    }
}

impl Drop for LeaseGuardInner {
    fn drop(&mut self) {
        self.heartbeat.abort();
        let mut local_leases = LOCAL_LEASES.lock().unwrap();
        if local_leases
            .get(&self.registry_key)
            .is_some_and(|entry| entry.token == self.token)
        {
            local_leases.remove(&self.registry_key);
        }
        drop(local_leases);

        let coordinator = self.coordinator.clone();
        let conversation_id = self.conversation_id.clone();
        let request_id = self.request_id.clone();
        tokio::spawn(async move {
            match tokio::time::timeout(std::time::Duration::from_secs(5), coordinator.release(&conversation_id)).await {
                Ok(Ok(())) => {},
                Ok(Err(error)) => tracing::warn!(
                    request_id = %request_id,
                    conversation_id = %conversation_id,
                    %error,
                    "coordinator lease release failed"
                ),
                Err(_) => tracing::warn!(
                    request_id = %request_id,
                    conversation_id = %conversation_id,
                    "coordinator lease release timed out"
                ),
            }
        });
    }
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
    /// `slack_msg_ts` → (owner_task_id, expires_at). Mirrors the production
    /// approvals table so HA tests exercise the cross-task routing path the
    /// same way `DynamoCoordinator` does.
    approvals: HashMap<String, (String, DateTime<Utc>)>,
}

#[derive(Clone)]
struct Lease {
    owner: String,
    expires_at: tokio::time::Instant,
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

    fn lease_expires_at(&self) -> tokio::time::Instant {
        tokio::time::Instant::now()
            + self
                .lease_ttl
                .to_std()
                .unwrap_or_else(|_| std::time::Duration::from_secs(0))
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
        let now = tokio::time::Instant::now();
        let expires_at = self.lease_expires_at();
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

    async fn force_acquire(&self, conversation_id: &str, dead_peer: &str) -> bool {
        let mut s = self.cluster.lock().expect("cluster state poisoned");
        let now = tokio::time::Instant::now();
        let expires_at = self.lease_expires_at();
        let claimable = match s.leases.get(conversation_id) {
            None => true,
            Some(existing) => existing.owner == dead_peer || existing.expires_at <= now,
        };
        if claimable {
            s.leases.insert(conversation_id.to_string(), Lease {
                owner: self.own_task_id.clone(),
                expires_at,
            });
        }
        claimable
    }

    async fn renew(&self, conversation_id: &str) -> anyhow::Result<()> {
        let mut s = self.cluster.lock().expect("cluster state poisoned");
        if let Some(lease) = s.leases.get_mut(conversation_id)
            && lease.owner == self.own_task_id
        {
            lease.expires_at = self.lease_expires_at();
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

    async fn register_approval(
        &self,
        slack_msg_ts: &str,
        _conversation_id: &str,
        ttl: std::time::Duration,
    ) -> anyhow::Result<()> {
        let expires_at = Utc::now() + chrono::Duration::from_std(ttl).unwrap_or_else(|_| chrono::Duration::seconds(0));
        let mut s = self.cluster.lock().expect("cluster state poisoned");
        s.approvals
            .insert(slack_msg_ts.to_string(), (self.own_task_id.clone(), expires_at));
        Ok(())
    }

    async fn lookup_approval_owner(&self, slack_msg_ts: &str) -> anyhow::Result<Option<String>> {
        let now = Utc::now();
        let s = self.cluster.lock().expect("cluster state poisoned");
        Ok(s.approvals
            .get(slack_msg_ts)
            .filter(|(_, exp)| *exp > now)
            .map(|(owner, _)| owner.clone()))
    }

    fn lease_heartbeat_interval(&self) -> std::time::Duration {
        self.lease_ttl
            .to_std()
            .unwrap_or_else(|_| std::time::Duration::from_secs(3))
            .div_f64(3.0)
            .max(std::time::Duration::from_secs(1))
    }

    fn lease_ttl(&self) -> std::time::Duration {
        self.lease_ttl
            .to_std()
            .unwrap_or_else(|_| std::time::Duration::from_secs(300))
    }
}

#[derive(Default)]
struct NoopState {
    seen_events: HashSet<String>,
    leases: HashMap<String, ()>,
    transcripts: HashMap<String, Vec<Turn>>,
    approvals: HashMap<String, DateTime<Utc>>,
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

    async fn force_acquire(&self, conversation_id: &str, _dead_peer: &str) -> bool {
        // Single-task: no peers, so claiming is always safe.
        let mut state = self.state.lock().expect("noop state poisoned");
        state.leases.insert(conversation_id.to_string(), ());
        true
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

    async fn register_approval(
        &self,
        slack_msg_ts: &str,
        _conversation_id: &str,
        ttl: std::time::Duration,
    ) -> anyhow::Result<()> {
        let expires_at = Utc::now() + chrono::Duration::from_std(ttl).unwrap_or_else(|_| chrono::Duration::seconds(0));
        let mut state = self.state.lock().expect("noop state poisoned");
        state.approvals.insert(slack_msg_ts.to_string(), expires_at);
        Ok(())
    }

    async fn lookup_approval_owner(&self, slack_msg_ts: &str) -> anyhow::Result<Option<String>> {
        // Single-task: if we have the row, we are the owner. Use a stable
        // sentinel rather than std::process::id() so test assertions are
        // deterministic — the value is only ever compared to *peer* ids by
        // the caller and Noop has no peers.
        let now = Utc::now();
        let state = self.state.lock().expect("noop state poisoned");
        Ok(state
            .approvals
            .get(slack_msg_ts)
            .filter(|exp| **exp > now)
            .map(|_| "self".to_string()))
    }
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;

    use super::*;

    struct RenewFailureCoordinator {
        releases: std::sync::atomic::AtomicUsize,
    }

    #[async_trait::async_trait]
    impl Coordinator for RenewFailureCoordinator {
        async fn dedupe_event(&self, _slack_event_id: &str) -> bool {
            true
        }

        async fn try_acquire(&self, _conversation_id: &str) -> LeaseOutcome {
            LeaseOutcome::Acquired
        }

        async fn force_acquire(&self, _conversation_id: &str, _dead_peer: &str) -> bool {
            true
        }

        async fn renew(&self, _conversation_id: &str) -> anyhow::Result<()> {
            anyhow::bail!("coordinator unavailable")
        }

        fn lease_heartbeat_interval(&self) -> std::time::Duration {
            std::time::Duration::from_secs(100)
        }

        fn lease_ttl(&self) -> std::time::Duration {
            std::time::Duration::from_secs(300)
        }

        async fn release(&self, _conversation_id: &str) -> anyhow::Result<()> {
            self.releases.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            Ok(())
        }

        async fn forward(&self, _peer: &str, _payload: ForwardEvent) -> anyhow::Result<()> {
            Ok(())
        }

        async fn append_turn(&self, _conversation_id: &str, _turn: Turn) -> anyhow::Result<()> {
            Ok(())
        }

        async fn load_history(&self, _conversation_id: &str, _limit: usize) -> anyhow::Result<Vec<Turn>> {
            Ok(Vec::new())
        }

        async fn register_approval(
            &self,
            _slack_msg_ts: &str,
            _conversation_id: &str,
            _ttl: std::time::Duration,
        ) -> anyhow::Result<()> {
            Ok(())
        }

        async fn lookup_approval_owner(&self, _slack_msg_ts: &str) -> anyhow::Result<Option<String>> {
            Ok(None)
        }
    }

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

    #[tokio::test(start_paused = true)]
    async fn lease_guard_keeps_dispatch_ownership_beyond_300_and_600_seconds() {
        let task_a = Arc::new(InMemoryClusterCoordinator::new("task-A", chrono::Duration::minutes(5)));
        let task_b = task_a.sibling("task-B");
        assert_eq!(task_a.try_acquire("convo-1").await, LeaseOutcome::Acquired);

        let guard = LeaseGuard::start(task_a, "convo-1".into(), "event-1".into());
        tokio::task::yield_now().await;
        tokio::time::advance(std::time::Duration::from_secs(301)).await;
        tokio::task::yield_now().await;
        assert!(matches!(task_b.try_acquire("convo-1").await, LeaseOutcome::Held { .. }));

        tokio::time::advance(std::time::Duration::from_secs(300)).await;
        tokio::task::yield_now().await;
        assert!(matches!(task_b.try_acquire("convo-1").await, LeaseOutcome::Held { .. }));

        drop(guard);
        tokio::task::yield_now().await;
        assert_eq!(task_b.try_acquire("convo-1").await, LeaseOutcome::Acquired);
    }

    #[tokio::test]
    async fn duplicate_local_guards_release_only_after_last_owner_drops() {
        let task_a = Arc::new(InMemoryClusterCoordinator::new("task-A", chrono::Duration::minutes(5)));
        let task_b = task_a.sibling("task-B");
        assert_eq!(task_a.try_acquire("convo-1").await, LeaseOutcome::Acquired);
        let first = LeaseGuard::start(task_a.clone(), "convo-1".into(), "event-1".into());

        assert_eq!(task_a.try_acquire("convo-1").await, LeaseOutcome::Acquired);
        let duplicate = LeaseGuard::start(task_a, "convo-1".into(), "event-2".into());
        drop(duplicate);
        tokio::task::yield_now().await;
        assert!(matches!(task_b.try_acquire("convo-1").await, LeaseOutcome::Held { .. }));

        drop(first);
        tokio::task::yield_now().await;
        assert_eq!(task_b.try_acquire("convo-1").await, LeaseOutcome::Acquired);
    }

    #[tokio::test(start_paused = true)]
    async fn lease_guard_signals_loss_before_failed_renewals_reach_ttl() {
        let coordinator = Arc::new(RenewFailureCoordinator {
            releases: std::sync::atomic::AtomicUsize::new(0),
        });
        let started = tokio::time::Instant::now();
        let guard = LeaseGuard::start(coordinator.clone(), "convo-1".into(), "event-1".into());
        let loss = guard.subscribe_loss();
        tokio::task::yield_now().await;

        tokio::time::advance(std::time::Duration::from_secs(101)).await;
        tokio::task::yield_now().await;
        assert!(loss.borrow().is_none());

        tokio::time::advance(std::time::Duration::from_secs(100)).await;
        tokio::task::yield_now().await;
        assert!(loss.borrow().is_some());
        assert!(started.elapsed() < std::time::Duration::from_secs(300));

        drop(guard);
        tokio::task::yield_now().await;
        assert_eq!(coordinator.releases.load(std::sync::atomic::Ordering::Relaxed), 1);
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

    #[tokio::test]
    async fn cluster_coordinator_routes_approvals_to_owner() {
        let task_a = InMemoryClusterCoordinator::new("task-A", chrono::Duration::minutes(5));
        let task_b = task_a.sibling("task-B");

        task_a
            .register_approval("1700000000.0001", "convo-1", std::time::Duration::from_secs(1800))
            .await
            .unwrap();

        // B's lookup must find A as the owner (this is the cross-task hop the
        // reaction handler needs to know about).
        let owner = task_b.lookup_approval_owner("1700000000.0001").await.unwrap();
        assert_eq!(owner.as_deref(), Some("task-A"));

        // Unknown msg_ts returns None.
        let missing = task_b.lookup_approval_owner("nope").await.unwrap();
        assert!(missing.is_none());
    }

    #[tokio::test]
    async fn noop_coordinator_register_lookup_round_trips() {
        let c = NoopCoordinator::new();
        c.register_approval("1700000000.0001", "convo-1", std::time::Duration::from_secs(60))
            .await
            .unwrap();
        let owner = c.lookup_approval_owner("1700000000.0001").await.unwrap();
        assert_eq!(owner.as_deref(), Some("self"));
    }
}
