use std::collections::{
    BTreeSet,
    HashMap,
};
use std::sync::{
    Arc,
    Mutex,
};
use std::time::Duration as StdDuration;

use chrono::{
    DateTime,
    Utc,
};

use super::{
    Coordinator,
    DedupeOutcome,
    DedupeToken,
    ForwardEvent,
    LeaseAcquisition,
    LeaseToken,
    RateLimitOutcome,
    Turn,
};

const MAX_RATE_LIMIT_USERS: usize = 10_000;

#[derive(Default)]
struct InMemoryDedup {
    events: HashMap<String, DedupeToken>,
}

impl InMemoryDedup {
    fn admit(&mut self, event_id: &str, owner: &str) -> DedupeOutcome {
        if self.events.contains_key(event_id) {
            return DedupeOutcome::Duplicate;
        }
        let token = DedupeToken::new(owner);
        self.events.insert(event_id.to_string(), token.clone());
        DedupeOutcome::Accepted { token }
    }

    fn release(&mut self, event_id: &str, token: &DedupeToken) {
        if self.events.get(event_id) == Some(token) {
            self.events.remove(event_id);
        }
    }
}

struct RateWindow {
    started: DateTime<Utc>,
    expires_at: DateTime<Utc>,
    count: u32,
}

struct InMemoryRateLimits {
    windows: HashMap<String, RateWindow>,
    max_users: usize,
}

impl Default for InMemoryRateLimits {
    fn default() -> Self {
        Self {
            windows: HashMap::new(),
            max_users: MAX_RATE_LIMIT_USERS,
        }
    }
}

impl InMemoryRateLimits {
    fn admit(&mut self, user_id: &str, max_prompts: u32, window: StdDuration, now: DateTime<Utc>) -> RateLimitOutcome {
        self.windows.retain(|_, entry| entry.expires_at > now);

        let elapsed = chrono::Duration::from_std(window).unwrap_or(chrono::Duration::MAX);
        let expires_at = now.checked_add_signed(elapsed).unwrap_or(DateTime::<Utc>::MAX_UTC);

        if !self.windows.contains_key(user_id) && self.windows.len() >= self.max_users {
            let retry_after = self
                .windows
                .values()
                .filter_map(|entry| (entry.expires_at - now).to_std().ok())
                .min()
                .unwrap_or_else(|| window.max(StdDuration::from_secs(1)));
            return RateLimitOutcome::Limited { retry_after };
        }

        let entry = self.windows.entry(user_id.to_string()).or_insert(RateWindow {
            started: now,
            expires_at,
            count: 0,
        });
        if now - entry.started >= elapsed {
            entry.started = now;
            entry.expires_at = expires_at;
            entry.count = 0;
        }
        if entry.count >= max_prompts {
            let retry_after = (entry.expires_at - now).to_std().unwrap_or(StdDuration::from_secs(1));
            return RateLimitOutcome::Limited { retry_after };
        }
        entry.count += 1;
        RateLimitOutcome::Allowed
    }
}

/// In-memory implementation for single-task frontends and local development.
#[derive(Default)]
pub struct NoopCoordinator {
    state: Mutex<NoopState>,
}

/// Multi-task in-memory coordinator with shared lease and transcript state.
#[derive(Clone)]
pub struct InMemoryClusterCoordinator {
    cluster: Arc<Mutex<ClusterState>>,
    own_task_id: String,
    lease_ttl: chrono::Duration,
}

#[derive(Default)]
struct ClusterState {
    dedup: InMemoryDedup,
    leases: HashMap<String, Lease>,
    transcripts: HashMap<String, Vec<Turn>>,
    citation_resets: HashMap<String, usize>,
    approvals: HashMap<String, (String, DateTime<Utc>)>,
    rate_limits: InMemoryRateLimits,
}

#[derive(Clone)]
struct Lease {
    owner: String,
    token: LeaseToken,
    expires_at: tokio::time::Instant,
}

#[derive(Default)]
struct NoopState {
    dedup: InMemoryDedup,
    leases: HashMap<String, LeaseToken>,
    transcripts: HashMap<String, Vec<Turn>>,
    citation_resets: HashMap<String, usize>,
    approvals: HashMap<String, DateTime<Utc>>,
    rate_limits: InMemoryRateLimits,
}

fn citation_sources(
    transcripts: &HashMap<String, Vec<Turn>>,
    citation_resets: &HashMap<String, usize>,
    conversation_id: &str,
) -> Vec<String> {
    let Some(turns) = transcripts.get(conversation_id) else {
        return Vec::new();
    };
    let reset = citation_resets
        .get(conversation_id)
        .copied()
        .unwrap_or_default()
        .min(turns.len());
    let window_start = turns
        .len()
        .saturating_sub(super::CITATION_PROVENANCE_TURN_LIMIT)
        .max(reset);
    turns[window_start..]
        .iter()
        .filter(|turn| turn.role == super::TurnRole::Assistant)
        .flat_map(|turn| turn.chunk_ids.iter().cloned())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

impl InMemoryClusterCoordinator {
    pub fn new(own_task_id: impl Into<String>, lease_ttl: chrono::Duration) -> Self {
        Self {
            cluster: Arc::new(Mutex::new(ClusterState::default())),
            own_task_id: own_task_id.into(),
            lease_ttl,
        }
    }

    pub fn sibling(&self, own_task_id: impl Into<String>) -> Self {
        Self {
            cluster: self.cluster.clone(),
            own_task_id: own_task_id.into(),
            lease_ttl: self.lease_ttl,
        }
    }

    fn lease_expires_at(&self) -> tokio::time::Instant {
        tokio::time::Instant::now() + self.lease_ttl.to_std().unwrap_or_else(|_| StdDuration::from_secs(0))
    }

    fn admit_prompt_at(
        &self,
        user_id: &str,
        max_prompts: u32,
        window: StdDuration,
        now: DateTime<Utc>,
    ) -> RateLimitOutcome {
        let mut state = self.cluster.lock().expect("cluster state poisoned");
        state.rate_limits.admit(user_id, max_prompts, window, now)
    }
}

#[async_trait::async_trait]
impl Coordinator for InMemoryClusterCoordinator {
    async fn dedupe_event_outcome(&self, slack_event_id: &str) -> DedupeOutcome {
        self.cluster
            .lock()
            .expect("cluster state poisoned")
            .dedup
            .admit(slack_event_id, &self.own_task_id)
    }

    async fn release_dedup(&self, slack_event_id: &str, token: &DedupeToken) -> anyhow::Result<()> {
        self.cluster
            .lock()
            .expect("cluster state poisoned")
            .dedup
            .release(slack_event_id, token);
        Ok(())
    }

    async fn acquire_lease(&self, conversation_id: &str) -> LeaseAcquisition {
        let mut state = self.cluster.lock().expect("cluster state poisoned");
        let now = tokio::time::Instant::now();
        let expires_at = self.lease_expires_at();
        match state.leases.get(conversation_id).cloned() {
            Some(existing) if existing.owner != self.own_task_id && existing.expires_at > now => {
                LeaseAcquisition::Held { peer: existing.owner }
            },
            _ => {
                let token = LeaseToken::new(&self.own_task_id);
                state.leases.insert(conversation_id.to_string(), Lease {
                    owner: self.own_task_id.clone(),
                    token: token.clone(),
                    expires_at,
                });
                LeaseAcquisition::Acquired { token }
            },
        }
    }

    async fn renew(&self, conversation_id: &str, token: &LeaseToken) -> anyhow::Result<()> {
        let mut state = self.cluster.lock().expect("cluster state poisoned");
        if let Some(lease) = state.leases.get_mut(conversation_id)
            && lease.owner == self.own_task_id
            && lease.token == *token
        {
            lease.expires_at = self.lease_expires_at();
            return Ok(());
        }
        anyhow::bail!("renew called on lease not owned by this task")
    }

    async fn release(&self, conversation_id: &str, token: &LeaseToken) -> anyhow::Result<()> {
        let mut state = self.cluster.lock().expect("cluster state poisoned");
        if matches!(
            state.leases.get(conversation_id),
            Some(lease) if lease.owner == self.own_task_id && lease.token == *token
        ) {
            state.leases.remove(conversation_id);
        }
        Ok(())
    }

    async fn forward(&self, _peer: &str, _payload: ForwardEvent) -> anyhow::Result<()> {
        Ok(())
    }

    async fn append_turn(&self, conversation_id: &str, turn: Turn) -> anyhow::Result<()> {
        self.cluster
            .lock()
            .expect("cluster state poisoned")
            .transcripts
            .entry(conversation_id.to_string())
            .or_default()
            .push(turn);
        Ok(())
    }

    async fn load_history(&self, conversation_id: &str, limit: usize) -> anyhow::Result<Vec<Turn>> {
        let state = self.cluster.lock().expect("cluster state poisoned");
        let all = state.transcripts.get(conversation_id).cloned().unwrap_or_default();
        if all.len() <= limit {
            Ok(all)
        } else {
            Ok(all[all.len() - limit..].to_vec())
        }
    }

    async fn load_citation_sources(&self, conversation_id: &str) -> anyhow::Result<Vec<String>> {
        let state = self.cluster.lock().expect("cluster state poisoned");
        Ok(citation_sources(
            &state.transcripts,
            &state.citation_resets,
            conversation_id,
        ))
    }

    async fn reset_citation_sources(&self, conversation_id: &str) -> anyhow::Result<()> {
        let mut state = self.cluster.lock().expect("cluster state poisoned");
        let reset = state.transcripts.get(conversation_id).map_or(0, Vec::len);
        state.citation_resets.insert(conversation_id.to_string(), reset);
        Ok(())
    }

    async fn register_approval(
        &self,
        slack_msg_ts: &str,
        _conversation_id: &str,
        ttl: StdDuration,
    ) -> anyhow::Result<()> {
        let expires_at = Utc::now() + chrono::Duration::from_std(ttl).unwrap_or_else(|_| chrono::Duration::seconds(0));
        self.cluster
            .lock()
            .expect("cluster state poisoned")
            .approvals
            .insert(slack_msg_ts.to_string(), (self.own_task_id.clone(), expires_at));
        Ok(())
    }

    async fn lookup_approval_owner(&self, slack_msg_ts: &str) -> anyhow::Result<Option<String>> {
        let now = Utc::now();
        let state = self.cluster.lock().expect("cluster state poisoned");
        Ok(state
            .approvals
            .get(slack_msg_ts)
            .filter(|(_, expires_at)| *expires_at > now)
            .map(|(owner, _)| owner.clone()))
    }

    fn lease_heartbeat_interval(&self) -> StdDuration {
        self.lease_ttl
            .to_std()
            .unwrap_or_else(|_| StdDuration::from_secs(3))
            .div_f64(3.0)
            .max(StdDuration::from_secs(1))
    }

    fn lease_ttl(&self) -> StdDuration {
        self.lease_ttl.to_std().unwrap_or_else(|_| StdDuration::from_secs(300))
    }

    async fn admit_prompt(
        &self,
        user_id: &str,
        max_prompts: u32,
        window: StdDuration,
    ) -> anyhow::Result<RateLimitOutcome> {
        Ok(self.admit_prompt_at(user_id, max_prompts, window, Utc::now()))
    }
}

impl NoopCoordinator {
    pub fn new() -> Self {
        Self::default()
    }

    fn admit_prompt_at(
        &self,
        user_id: &str,
        max_prompts: u32,
        window: StdDuration,
        now: DateTime<Utc>,
    ) -> RateLimitOutcome {
        self.state
            .lock()
            .expect("noop state poisoned")
            .rate_limits
            .admit(user_id, max_prompts, window, now)
    }
}

#[async_trait::async_trait]
impl Coordinator for NoopCoordinator {
    async fn dedupe_event_outcome(&self, slack_event_id: &str) -> DedupeOutcome {
        self.state
            .lock()
            .expect("noop state poisoned")
            .dedup
            .admit(slack_event_id, "noop")
    }

    async fn release_dedup(&self, slack_event_id: &str, token: &DedupeToken) -> anyhow::Result<()> {
        self.state
            .lock()
            .expect("noop state poisoned")
            .dedup
            .release(slack_event_id, token);
        Ok(())
    }

    async fn acquire_lease(&self, conversation_id: &str) -> LeaseAcquisition {
        let mut state = self.state.lock().expect("noop state poisoned");
        let token = LeaseToken::new("noop");
        state.leases.insert(conversation_id.to_string(), token.clone());
        LeaseAcquisition::Acquired { token }
    }

    async fn renew(&self, conversation_id: &str, token: &LeaseToken) -> anyhow::Result<()> {
        let state = self.state.lock().expect("noop state poisoned");
        anyhow::ensure!(
            state.leases.get(conversation_id) == Some(token),
            "renew called on lease not owned by this task"
        );
        Ok(())
    }

    async fn release(&self, conversation_id: &str, token: &LeaseToken) -> anyhow::Result<()> {
        let mut state = self.state.lock().expect("noop state poisoned");
        if state.leases.get(conversation_id) == Some(token) {
            state.leases.remove(conversation_id);
        }
        Ok(())
    }

    async fn forward(&self, _peer: &str, _payload: ForwardEvent) -> anyhow::Result<()> {
        Ok(())
    }

    async fn append_turn(&self, conversation_id: &str, turn: Turn) -> anyhow::Result<()> {
        self.state
            .lock()
            .expect("noop state poisoned")
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
            Ok(all[all.len() - limit..].to_vec())
        }
    }

    async fn load_citation_sources(&self, conversation_id: &str) -> anyhow::Result<Vec<String>> {
        let state = self.state.lock().expect("noop state poisoned");
        Ok(citation_sources(
            &state.transcripts,
            &state.citation_resets,
            conversation_id,
        ))
    }

    async fn reset_citation_sources(&self, conversation_id: &str) -> anyhow::Result<()> {
        let mut state = self.state.lock().expect("noop state poisoned");
        let reset = state.transcripts.get(conversation_id).map_or(0, Vec::len);
        state.citation_resets.insert(conversation_id.to_string(), reset);
        Ok(())
    }

    async fn register_approval(
        &self,
        slack_msg_ts: &str,
        _conversation_id: &str,
        ttl: StdDuration,
    ) -> anyhow::Result<()> {
        let expires_at = Utc::now() + chrono::Duration::from_std(ttl).unwrap_or_else(|_| chrono::Duration::seconds(0));
        self.state
            .lock()
            .expect("noop state poisoned")
            .approvals
            .insert(slack_msg_ts.to_string(), expires_at);
        Ok(())
    }

    async fn lookup_approval_owner(&self, slack_msg_ts: &str) -> anyhow::Result<Option<String>> {
        let now = Utc::now();
        let state = self.state.lock().expect("noop state poisoned");
        Ok(state
            .approvals
            .get(slack_msg_ts)
            .filter(|expires_at| **expires_at > now)
            .map(|_| "self".to_string()))
    }

    async fn admit_prompt(
        &self,
        user_id: &str,
        max_prompts: u32,
        window: StdDuration,
    ) -> anyhow::Result<RateLimitOutcome> {
        Ok(self.admit_prompt_at(user_id, max_prompts, window, Utc::now()))
    }
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;

    use super::*;
    use crate::engine::coordinator::{
        CITATION_PROVENANCE_TURN_LIMIT,
        TurnRole,
    };

    fn at(seconds: i64) -> DateTime<Utc> {
        Utc.timestamp_opt(seconds, 0).single().unwrap()
    }

    fn turn(role: TurnRole, text: &str, seconds: i64) -> Turn {
        Turn {
            role,
            text: text.to_string(),
            ts: at(seconds),
            chunk_ids: Vec::new(),
        }
    }

    async fn acquire_token(coordinator: &dyn Coordinator, conversation_id: &str) -> LeaseToken {
        match coordinator.acquire_lease(conversation_id).await {
            LeaseAcquisition::Acquired { token } => token,
            other => panic!("expected lease acquisition, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn dedupe_event_returns_true_only_first_time() {
        let coordinator = NoopCoordinator::new();
        assert!(coordinator.dedupe_event("evt-1").await);
        assert!(!coordinator.dedupe_event("evt-1").await);
        assert!(coordinator.dedupe_event("evt-2").await);
    }

    #[tokio::test]
    async fn noop_coordinator_always_grants_lease() {
        let coordinator = NoopCoordinator::new();
        assert!(matches!(
            coordinator.acquire_lease("convo-A").await,
            LeaseAcquisition::Acquired { .. }
        ));
        assert!(matches!(
            coordinator.acquire_lease("convo-A").await,
            LeaseAcquisition::Acquired { .. }
        ));
    }

    #[tokio::test]
    async fn release_is_idempotent() {
        let coordinator = NoopCoordinator::new();
        let missing = LeaseToken::new("missing");
        coordinator.release("never-acquired", &missing).await.unwrap();
        let token = acquire_token(&coordinator, "convo-A").await;
        coordinator.release("convo-A", &token).await.unwrap();
        coordinator.release("convo-A", &token).await.unwrap();
    }

    #[tokio::test]
    async fn forward_is_a_no_op() {
        NoopCoordinator::new()
            .forward("peer-1", ForwardEvent {
                slack_event_json: serde_json::json!({"id":"x"}),
            })
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn append_and_load_round_trips_in_order() {
        let coordinator = NoopCoordinator::new();
        coordinator
            .append_turn("convo-A", turn(TurnRole::User, "hi", 1000))
            .await
            .unwrap();
        coordinator
            .append_turn("convo-A", turn(TurnRole::Assistant, "hello", 1001))
            .await
            .unwrap();
        coordinator
            .append_turn("convo-A", turn(TurnRole::User, "thanks", 1002))
            .await
            .unwrap();

        let history = coordinator.load_history("convo-A", 100).await.unwrap();
        assert_eq!(history.len(), 3);
        assert_eq!(history[0].text, "hi");
        assert_eq!(history[2].text, "thanks");
        assert!(coordinator.load_history("convo-B", 100).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn load_history_respects_limit_keeping_most_recent() {
        let coordinator = NoopCoordinator::new();
        for index in 0..5 {
            coordinator
                .append_turn("convo-A", turn(TurnRole::User, &format!("turn-{index}"), 1000 + index))
                .await
                .unwrap();
        }
        let history = coordinator.load_history("convo-A", 2).await.unwrap();
        assert_eq!(history.len(), 2);
        assert_eq!(history[0].text, "turn-3");
        assert_eq!(history[1].text, "turn-4");
    }

    #[tokio::test]
    async fn citation_sources_are_assistant_scoped_and_resettable() {
        let coordinator = NoopCoordinator::new();
        let mut assistant = turn(TurnRole::Assistant, "grounded", 1000);
        assistant.chunk_ids = vec!["docs/b.md".into(), "docs/a.md".into()];
        coordinator.append_turn("convo-A", assistant).await.unwrap();

        let mut user = turn(TurnRole::User, "untrusted", 1001);
        user.chunk_ids = vec!["docs/untrusted.md".into()];
        coordinator.append_turn("convo-A", user).await.unwrap();

        assert_eq!(coordinator.load_citation_sources("convo-A").await.unwrap(), vec![
            "docs/a.md",
            "docs/b.md"
        ]);
        assert!(coordinator.load_citation_sources("convo-B").await.unwrap().is_empty());

        coordinator.reset_citation_sources("convo-A").await.unwrap();
        assert!(coordinator.load_citation_sources("convo-A").await.unwrap().is_empty());

        let mut new_session = turn(TurnRole::Assistant, "new source", 1002);
        new_session.chunk_ids = vec!["docs/new.md".into()];
        coordinator.append_turn("convo-A", new_session).await.unwrap();
        assert_eq!(coordinator.load_citation_sources("convo-A").await.unwrap(), vec![
            "docs/new.md"
        ]);
    }

    #[tokio::test]
    async fn citation_sources_are_bounded_to_recent_transcript_rows() {
        let coordinator = NoopCoordinator::new();
        let mut old = turn(TurnRole::Assistant, "old source", 1000);
        old.chunk_ids = vec!["docs/old.md".into()];
        coordinator.append_turn("convo-A", old).await.unwrap();

        for index in 0..CITATION_PROVENANCE_TURN_LIMIT {
            coordinator
                .append_turn(
                    "convo-A",
                    turn(TurnRole::User, &format!("filler-{index}"), 1001 + index as i64),
                )
                .await
                .unwrap();
        }

        let mut recent = turn(TurnRole::Assistant, "recent source", 2000);
        recent.chunk_ids = vec!["docs/recent.md".into()];
        coordinator.append_turn("convo-A", recent).await.unwrap();

        assert_eq!(coordinator.load_citation_sources("convo-A").await.unwrap(), vec![
            "docs/recent.md"
        ]);
    }

    #[tokio::test]
    async fn cluster_coordinator_arbitrates_leases_across_two_tasks() {
        let task_a = InMemoryClusterCoordinator::new("task-A", chrono::Duration::minutes(5));
        let task_b = task_a.sibling("task-B");

        assert!(matches!(
            task_a.acquire_lease("convo-1").await,
            LeaseAcquisition::Acquired { .. }
        ));
        assert!(matches!(
            task_b.acquire_lease("convo-1").await,
            LeaseAcquisition::Held { peer } if peer == "task-A"
        ));
        assert!(matches!(
            task_b.acquire_lease("convo-2").await,
            LeaseAcquisition::Acquired { .. }
        ));
    }

    #[tokio::test]
    async fn cluster_coordinator_lets_owner_release_and_peer_acquire() {
        let task_a = InMemoryClusterCoordinator::new("task-A", chrono::Duration::minutes(5));
        let task_b = task_a.sibling("task-B");

        let token = acquire_token(&task_a, "convo-1").await;
        assert!(matches!(
            task_b.acquire_lease("convo-1").await,
            LeaseAcquisition::Held { .. }
        ));
        task_a.release("convo-1", &token).await.unwrap();
        assert!(matches!(
            task_b.acquire_lease("convo-1").await,
            LeaseAcquisition::Acquired { .. }
        ));
    }

    #[tokio::test]
    async fn delayed_release_cannot_delete_newer_same_task_lease() {
        let task_a = InMemoryClusterCoordinator::new("task-A", chrono::Duration::minutes(5));
        let task_b = task_a.sibling("task-B");

        let stale_token = acquire_token(&task_a, "convo-1").await;
        let current_token = acquire_token(&task_a, "convo-1").await;
        assert_ne!(stale_token, current_token);
        task_a.release("convo-1", &stale_token).await.unwrap();

        assert!(matches!(
            task_b.acquire_lease("convo-1").await,
            LeaseAcquisition::Held { peer } if peer == "task-A"
        ));
        task_a.renew("convo-1", &current_token).await.unwrap();
    }

    #[tokio::test]
    async fn cluster_coordinator_renew_fails_for_non_owner() {
        let task_a = InMemoryClusterCoordinator::new("task-A", chrono::Duration::minutes(5));
        let task_b = task_a.sibling("task-B");
        let token = acquire_token(&task_a, "convo-1").await;
        assert!(
            task_b
                .renew("convo-1", &token)
                .await
                .unwrap_err()
                .to_string()
                .contains("not owned")
        );
    }

    #[tokio::test]
    async fn cluster_coordinator_dedupe_is_cluster_wide() {
        let task_a = InMemoryClusterCoordinator::new("task-A", chrono::Duration::minutes(5));
        let task_b = task_a.sibling("task-B");

        assert!(task_a.dedupe_event("evt-1").await);
        assert!(!task_b.dedupe_event("evt-1").await);
    }

    #[tokio::test]
    async fn cluster_coordinator_transcripts_are_visible_to_siblings() {
        let task_a = InMemoryClusterCoordinator::new("task-A", chrono::Duration::minutes(5));
        let task_b = task_a.sibling("task-B");
        task_a
            .append_turn("convo-1", turn(TurnRole::User, "hi", 100))
            .await
            .unwrap();
        let history = task_b.load_history("convo-1", 10).await.unwrap();
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].text, "hi");
    }

    #[tokio::test]
    async fn cluster_coordinator_routes_approvals_to_owner() {
        let task_a = InMemoryClusterCoordinator::new("task-A", chrono::Duration::minutes(5));
        let task_b = task_a.sibling("task-B");

        task_a
            .register_approval("1700000000.0001", "convo-1", StdDuration::from_secs(1800))
            .await
            .unwrap();

        assert_eq!(
            task_b
                .lookup_approval_owner("1700000000.0001")
                .await
                .unwrap()
                .as_deref(),
            Some("task-A")
        );
        assert!(task_b.lookup_approval_owner("nope").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn noop_coordinator_register_lookup_round_trips() {
        let coordinator = NoopCoordinator::new();
        coordinator
            .register_approval("1700000000.0001", "convo-1", StdDuration::from_secs(60))
            .await
            .unwrap();
        assert_eq!(
            coordinator
                .lookup_approval_owner("1700000000.0001")
                .await
                .unwrap()
                .as_deref(),
            Some("self")
        );
    }

    #[tokio::test]
    async fn cluster_rate_limit_is_shared_across_tasks() {
        let task_a = InMemoryClusterCoordinator::new("task-A", chrono::Duration::minutes(5));
        let task_b = task_a.sibling("task-B");
        let window = StdDuration::from_secs(60);

        assert_eq!(
            task_a.admit_prompt("U1", 2, window).await.unwrap(),
            RateLimitOutcome::Allowed
        );
        assert_eq!(
            task_b.admit_prompt("U1", 2, window).await.unwrap(),
            RateLimitOutcome::Allowed
        );
        assert!(matches!(
            task_a.admit_prompt("U1", 2, window).await.unwrap(),
            RateLimitOutcome::Limited { .. }
        ));
        assert_eq!(
            task_b.admit_prompt("U2", 2, window).await.unwrap(),
            RateLimitOutcome::Allowed
        );
    }

    #[test]
    fn noop_rate_limits_are_bounded_and_expired_windows_are_evicted() {
        let coordinator = NoopCoordinator::new();
        coordinator.state.lock().unwrap().rate_limits.max_users = 2;
        let window = StdDuration::from_secs(60);

        assert_eq!(
            coordinator.admit_prompt_at("U1", 1, window, at(100)),
            RateLimitOutcome::Allowed
        );
        assert_eq!(
            coordinator.admit_prompt_at("U2", 1, window, at(110)),
            RateLimitOutcome::Allowed
        );
        assert!(matches!(
            coordinator.admit_prompt_at("U3", 1, window, at(120)),
            RateLimitOutcome::Limited { .. }
        ));
        assert_eq!(
            coordinator.admit_prompt_at("U3", 1, window, at(161)),
            RateLimitOutcome::Allowed
        );

        let state = coordinator.state.lock().unwrap();
        assert_eq!(state.rate_limits.windows.len(), 2);
        assert!(!state.rate_limits.windows.contains_key("U1"));
        assert!(state.rate_limits.windows.contains_key("U2"));
        assert!(state.rate_limits.windows.contains_key("U3"));
    }

    #[test]
    fn cluster_rate_limits_are_bounded_and_expired_windows_are_evicted() {
        let task_a = InMemoryClusterCoordinator::new("task-A", chrono::Duration::minutes(5));
        let task_b = task_a.sibling("task-B");
        task_a.cluster.lock().unwrap().rate_limits.max_users = 2;
        let window = StdDuration::from_secs(60);

        assert_eq!(
            task_a.admit_prompt_at("U1", 1, window, at(100)),
            RateLimitOutcome::Allowed
        );
        assert_eq!(
            task_b.admit_prompt_at("U2", 1, window, at(110)),
            RateLimitOutcome::Allowed
        );
        assert!(matches!(
            task_a.admit_prompt_at("U3", 1, window, at(120)),
            RateLimitOutcome::Limited { .. }
        ));
        assert_eq!(
            task_b.admit_prompt_at("U3", 1, window, at(161)),
            RateLimitOutcome::Allowed
        );

        let state = task_a.cluster.lock().unwrap();
        assert_eq!(state.rate_limits.windows.len(), 2);
        assert!(!state.rate_limits.windows.contains_key("U1"));
        assert!(state.rate_limits.windows.contains_key("U2"));
        assert!(state.rate_limits.windows.contains_key("U3"));
    }
}
