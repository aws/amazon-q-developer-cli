//! Integration tests for the multi-task coordinator + dispatch wiring.
//!
//! These exercise [`engine::core::dispatch`] across two simulated ECS tasks
//! sharing one `InMemoryClusterCoordinator`, asserting that the user-visible
//! contract — exactly one reply per Slack event — holds across all
//! coordinator outcomes (Acquired / Held / Unavailable / forward failure).

use std::sync::{
    Arc,
    Mutex,
};
use std::time::Duration;

use anyhow::Result;
use async_trait::async_trait;
use kiro_bot::engine::acp::{
    AcpInfo,
    Work,
};
use kiro_bot::engine::coordinator::{
    Coordinator,
    ForwardEvent,
    InMemoryClusterCoordinator,
    LeaseOutcome,
};
use kiro_bot::engine::core::{
    BotCore,
    Conversation,
    DispatchEnvelope,
    Frontend,
    IncomingMessage,
    Reply,
    dispatch,
};
use kiro_bot::engine::response_policy::ResponsePolicyConfig;
use serde_json::json;
use tokio::sync::mpsc;
use tokio::time::sleep;

// ---------------------------------------------------------------------------
// Fakes: frontend that records replies, ACP loop that auto-replies.
// ---------------------------------------------------------------------------

#[derive(Default, Clone)]
struct RecorderFrontend {
    sends: Arc<Mutex<Vec<String>>>,
}

impl RecorderFrontend {
    fn new() -> Self {
        Self::default()
    }

    fn send_count(&self) -> usize {
        self.sends.lock().unwrap().len()
    }

    fn sends(&self) -> Vec<String> {
        self.sends.lock().unwrap().clone()
    }
}

#[async_trait]
impl Frontend for RecorderFrontend {
    async fn send(&self, reply: Reply) -> Result<String> {
        if let Reply::Send { text, .. } = &reply {
            self.sends.lock().unwrap().push(text.clone());
        }
        Ok("fake-msg-id".into())
    }

    async fn fetch_context(&self, _: &str, _: &str, _: Option<&str>) -> Vec<String> {
        vec![]
    }
}

/// Drain `Work::Prompt` from the BotCore's work channel and immediately
/// reply with a fixed string. Other Work variants are acknowledged with
/// empty replies so the per-action arms don't hang.
fn spawn_fake_acp(mut rx: mpsc::UnboundedReceiver<Work>) {
    tokio::spawn(async move {
        while let Some(work) = rx.recv().await {
            match work {
                Work::Prompt { reply_tx, .. } => {
                    let _ = reply_tx.send("ok".into());
                },
                Work::NewSession { reply_tx, .. } => {
                    let _ = reply_tx.send("new".into());
                },
                Work::SetMode { reply_tx, .. } => {
                    let _ = reply_tx.send("set".into());
                },
                Work::SetModel { reply_tx, .. } => {
                    let _ = reply_tx.send("set".into());
                },
                Work::Status { reply_tx, .. } => {
                    let _ = reply_tx.send("status".into());
                },
                Work::Cancel { .. } => {},
                Work::CancelAndWait { reply_tx, .. } => {
                    let _ = reply_tx.send(());
                },
                Work::Shutdown { reply_tx, .. } => {
                    let _ = reply_tx.send(());
                    break;
                },
            }
        }
    });
}

fn build_core(coordinator: Arc<dyn Coordinator>) -> BotCore {
    let (tx, rx) = mpsc::unbounded_channel();
    spawn_fake_acp(rx);
    BotCore {
        work_sender: tx,
        work_capacity: BotCore::work_capacity(64),
        inflight: Arc::new(std::sync::Mutex::new(std::collections::HashSet::new())),
        authz: None,
        response_policy: Arc::new(ResponsePolicyConfig::default_policy()),
        acp_info: Arc::new(std::sync::Mutex::new(AcpInfo::default())),
        coordinator,
    }
}

fn slack_envelope(event_id: &str) -> DispatchEnvelope {
    DispatchEnvelope {
        event_id: event_id.into(),
        raw: json!({
            "type": "event_callback",
            "event_id": event_id,
            "event": { "type": "message", "text": "hello" },
        }),
    }
}

fn incoming(text: &str, conv: Conversation, env: Option<DispatchEnvelope>) -> IncomingMessage {
    IncomingMessage {
        user: "alice".into(),
        slack_user_id: "U1".into(),
        text: text.into(),
        conversation: conv,
        reply_to: None,
        directed: true,
        context: vec![],
        envelope: env,
    }
}

/// Wait long enough for spawned dispatch tasks to send their replies, then
/// return. 200 ms is comfortably above the in-memory coordinator round-trip.
async fn drain() {
    sleep(Duration::from_millis(200)).await;
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

/// (1) Two tasks share one cluster. Same `event_id` arrives at both. Exactly
/// one reply lands on the user.
#[tokio::test]
async fn two_bots_one_cluster_dedupes_prompt() {
    let task_a = Arc::new(InMemoryClusterCoordinator::new("task-A", chrono::Duration::minutes(5)));
    let task_b = Arc::new(task_a.sibling("task-B"));

    let frontend_a = Arc::new(RecorderFrontend::new());
    let frontend_b = Arc::new(RecorderFrontend::new());

    let core_a = build_core(task_a.clone());
    let core_b = build_core(task_b.clone());

    let conv = Conversation::Channel("C123".into());
    let env = slack_envelope("EvX1");

    dispatch(
        &core_a,
        incoming("hello", conv.clone(), Some(env.clone())),
        frontend_a.clone(),
    );
    dispatch(&core_b, incoming("hello", conv, Some(env)), frontend_b.clone());

    drain().await;

    let total = frontend_a.send_count() + frontend_b.send_count();
    // One winning task: ack ("Looking into it...") + final reply ("ok") = 2.
    // Loser sends 0.
    assert_eq!(
        total,
        2,
        "expected exactly one task to reply (2 sends total: ack + final), got {total}\n  A: {:?}\n  B: {:?}",
        frontend_a.sends(),
        frontend_b.sends()
    );
}

/// (2) Same `event_id`, but the two tasks dispatch a `!help` (non-Prompt
/// action). Without the lease+dedup gate covering all action arms, both
/// tasks would post the help text. With the new dispatch: exactly one.
#[tokio::test]
async fn two_bots_one_cluster_dedupes_non_prompt_action() {
    let task_a = Arc::new(InMemoryClusterCoordinator::new("task-A", chrono::Duration::minutes(5)));
    let task_b = Arc::new(task_a.sibling("task-B"));

    let frontend_a = Arc::new(RecorderFrontend::new());
    let frontend_b = Arc::new(RecorderFrontend::new());

    let core_a = build_core(task_a.clone());
    let core_b = build_core(task_b.clone());

    let conv = Conversation::Channel("C999".into());
    let env = slack_envelope("EvX2");

    dispatch(
        &core_a,
        incoming("!help", conv.clone(), Some(env.clone())),
        frontend_a.clone(),
    );
    dispatch(&core_b, incoming("!help", conv, Some(env)), frontend_b.clone());

    drain().await;

    let total = frontend_a.send_count() + frontend_b.send_count();
    assert_eq!(
        total,
        1,
        "expected exactly one help reply, got {total}\n  A: {:?}\n  B: {:?}",
        frontend_a.sends(),
        frontend_b.sends()
    );
}

/// (3) Same conversation, three sequential events from the same Slack thread.
/// Each turn must produce exactly one reply. This catches a bug class where
/// a stale lease + missing release() makes follow-up turns silently drop.
#[tokio::test]
async fn followup_turns_each_produce_one_reply() {
    let task_a = Arc::new(InMemoryClusterCoordinator::new("task-A", chrono::Duration::minutes(5)));
    let task_b = Arc::new(task_a.sibling("task-B"));

    let frontend_a = Arc::new(RecorderFrontend::new());
    let frontend_b = Arc::new(RecorderFrontend::new());

    let core_a = build_core(task_a.clone());
    let core_b = build_core(task_b.clone());

    let conv = Conversation::Thread {
        channel: "C1".into(),
        thread_ts: "1700000000.0001".into(),
    };

    for i in 0..3 {
        let env = slack_envelope(&format!("EvFollow-{i}"));
        dispatch(
            &core_a,
            incoming("hi", conv.clone(), Some(env.clone())),
            frontend_a.clone(),
        );
        dispatch(&core_b, incoming("hi", conv.clone(), Some(env)), frontend_b.clone());
        drain().await;
    }

    let total = frontend_a.send_count() + frontend_b.send_count();
    // 3 turns × (ack + final) = 6 sends. If a follow-up turn dropped, total
    // would be < 6; if a duplicate slipped through, total would be > 6.
    assert_eq!(
        total,
        6,
        "3 follow-up turns must yield 6 sends total (ack+final per turn), got {total}\n  A: {:?}\n  B: {:?}",
        frontend_a.sends(),
        frontend_b.sends()
    );
}

// ---------------------------------------------------------------------------
// Failure-mode coordinators: wrap InMemoryClusterCoordinator and override one
// method to inject the failure under test.
// ---------------------------------------------------------------------------

struct UnavailableCoordinator {
    inner: InMemoryClusterCoordinator,
}

struct LeaseLossCoordinator {
    releases: std::sync::atomic::AtomicUsize,
}

#[async_trait]
impl Coordinator for LeaseLossCoordinator {
    async fn dedupe_event(&self, _id: &str) -> bool {
        true
    }

    async fn try_acquire(&self, _conv: &str) -> LeaseOutcome {
        LeaseOutcome::Acquired
    }

    async fn force_acquire(&self, _conv: &str, _peer: &str) -> bool {
        true
    }

    async fn renew(&self, _conv: &str) -> Result<()> {
        anyhow::bail!("coordinator unavailable")
    }

    fn lease_heartbeat_interval(&self) -> Duration {
        Duration::from_secs(10)
    }

    fn lease_ttl(&self) -> Duration {
        Duration::from_secs(30)
    }

    async fn release(&self, _conv: &str) -> Result<()> {
        self.releases.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        Ok(())
    }

    async fn forward(&self, _peer: &str, _payload: ForwardEvent) -> Result<()> {
        Ok(())
    }

    async fn append_turn(&self, _conv: &str, _turn: kiro_bot::engine::coordinator::Turn) -> Result<()> {
        Ok(())
    }

    async fn load_history(&self, _conv: &str, _limit: usize) -> Result<Vec<kiro_bot::engine::coordinator::Turn>> {
        Ok(Vec::new())
    }

    async fn register_approval(&self, _ts: &str, _conv: &str, _ttl: Duration) -> Result<()> {
        Ok(())
    }

    async fn lookup_approval_owner(&self, _ts: &str) -> Result<Option<String>> {
        Ok(None)
    }
}

#[async_trait]
impl Coordinator for UnavailableCoordinator {
    async fn dedupe_event(&self, id: &str) -> bool {
        self.inner.dedupe_event(id).await
    }

    async fn try_acquire(&self, _conv: &str) -> LeaseOutcome {
        LeaseOutcome::Unavailable
    }

    async fn force_acquire(&self, conv: &str, peer: &str) -> bool {
        self.inner.force_acquire(conv, peer).await
    }

    async fn renew(&self, conv: &str) -> Result<()> {
        self.inner.renew(conv).await
    }

    async fn release(&self, conv: &str) -> Result<()> {
        self.inner.release(conv).await
    }

    async fn forward(&self, peer: &str, payload: ForwardEvent) -> Result<()> {
        self.inner.forward(peer, payload).await
    }

    async fn append_turn(&self, conv: &str, turn: kiro_bot::engine::coordinator::Turn) -> Result<()> {
        self.inner.append_turn(conv, turn).await
    }

    async fn load_history(&self, conv: &str, limit: usize) -> Result<Vec<kiro_bot::engine::coordinator::Turn>> {
        self.inner.load_history(conv, limit).await
    }

    async fn register_approval(&self, ts: &str, conv: &str, ttl: std::time::Duration) -> Result<()> {
        self.inner.register_approval(ts, conv, ttl).await
    }

    async fn lookup_approval_owner(&self, ts: &str) -> Result<Option<String>> {
        self.inner.lookup_approval_owner(ts).await
    }
}

#[tokio::test(start_paused = true)]
async fn lease_loss_cancels_active_prompt_before_ttl_expires() {
    let coordinator = Arc::new(LeaseLossCoordinator {
        releases: std::sync::atomic::AtomicUsize::new(0),
    });
    let (work_sender, mut work_receiver) = mpsc::unbounded_channel();
    let (prompt_seen_tx, prompt_seen_rx) = tokio::sync::oneshot::channel();
    let (cancel_seen_tx, cancel_seen_rx) = tokio::sync::oneshot::channel();
    tokio::spawn(async move {
        let mut prompt_seen_tx = Some(prompt_seen_tx);
        let mut cancel_seen_tx = Some(cancel_seen_tx);
        let mut pending_reply = None;
        while let Some(work) = work_receiver.recv().await {
            match work {
                Work::Prompt { reply_tx, .. } => {
                    pending_reply = Some(reply_tx);
                    if let Some(tx) = prompt_seen_tx.take() {
                        let _ = tx.send(());
                    }
                },
                Work::CancelAndWait { reply_tx, .. } => {
                    if let Some(tx) = cancel_seen_tx.take() {
                        let _ = tx.send(());
                    }
                    let _ = reply_tx.send(());
                },
                _ => {},
            }
        }
        drop(pending_reply);
    });
    let core = BotCore {
        work_sender,
        work_capacity: BotCore::work_capacity(64),
        inflight: Arc::new(std::sync::Mutex::new(std::collections::HashSet::new())),
        authz: None,
        response_policy: Arc::new(ResponsePolicyConfig::default_policy()),
        acp_info: Arc::new(std::sync::Mutex::new(AcpInfo::default())),
        coordinator: coordinator.clone(),
    };
    let frontend = Arc::new(RecorderFrontend::new());
    dispatch(
        &core,
        incoming(
            "slow question",
            Conversation::Channel("C-lease-loss".into()),
            Some(slack_envelope("EvLeaseLoss")),
        ),
        frontend.clone(),
    );
    prompt_seen_rx.await.unwrap();

    tokio::time::advance(Duration::from_secs(11)).await;
    tokio::task::yield_now().await;
    assert_eq!(frontend.send_count(), 1, "first failed renewal must not cancel early");

    tokio::time::advance(Duration::from_secs(10)).await;
    cancel_seen_rx.await.unwrap();
    for _ in 0..5 {
        tokio::task::yield_now().await;
    }

    let sends = frontend.sends();
    assert_eq!(sends.len(), 2, "ack plus lease-loss error expected: {sends:?}");
    assert!(sends[1].contains("Coordination lease was lost"));
    assert_eq!(coordinator.releases.load(std::sync::atomic::Ordering::Relaxed), 1);
}

#[tokio::test(start_paused = true)]
async fn lease_loss_cancels_active_citation_retry() {
    let coordinator = Arc::new(LeaseLossCoordinator {
        releases: std::sync::atomic::AtomicUsize::new(0),
    });
    let (work_sender, mut work_receiver) = mpsc::unbounded_channel();
    let (retry_seen_tx, retry_seen_rx) = tokio::sync::oneshot::channel();
    let (cancel_seen_tx, cancel_seen_rx) = tokio::sync::oneshot::channel();
    tokio::spawn(async move {
        let mut prompt_count = 0;
        let mut retry_seen_tx = Some(retry_seen_tx);
        let mut cancel_seen_tx = Some(cancel_seen_tx);
        let mut pending_retry = None;
        while let Some(work) = work_receiver.recv().await {
            match work {
                Work::Prompt { reply_tx, .. } => {
                    prompt_count += 1;
                    if prompt_count == 1 {
                        let _ = reply_tx.send("Kiro can do that without a source.".into());
                    } else {
                        pending_retry = Some(reply_tx);
                        if let Some(tx) = retry_seen_tx.take() {
                            let _ = tx.send(());
                        }
                    }
                },
                Work::CancelAndWait { reply_tx, .. } => {
                    if let Some(tx) = cancel_seen_tx.take() {
                        let _ = tx.send(());
                    }
                    drop(pending_retry.take());
                    let _ = reply_tx.send(());
                },
                _ => {},
            }
        }
    });
    let core = BotCore {
        work_sender,
        work_capacity: BotCore::work_capacity(64),
        inflight: Arc::new(std::sync::Mutex::new(std::collections::HashSet::new())),
        authz: None,
        response_policy: Arc::new(ResponsePolicyConfig::default_policy()),
        acp_info: Arc::new(std::sync::Mutex::new(AcpInfo::default())),
        coordinator: coordinator.clone(),
    };
    let frontend = Arc::new(RecorderFrontend::new());
    dispatch(
        &core,
        incoming(
            "How does Kiro handle this?",
            Conversation::Channel("C-retry-lease-loss".into()),
            Some(slack_envelope("EvRetryLeaseLoss")),
        ),
        frontend.clone(),
    );
    retry_seen_rx.await.unwrap();

    tokio::time::advance(Duration::from_secs(21)).await;
    cancel_seen_rx.await.unwrap();
    for _ in 0..5 {
        tokio::task::yield_now().await;
    }

    let sends = frontend.sends();
    assert_eq!(sends.len(), 2, "ack plus lease-loss error expected: {sends:?}");
    assert!(sends[1].contains("Coordination lease was lost"));
    assert_eq!(coordinator.releases.load(std::sync::atomic::Ordering::Relaxed), 1);
}

/// (4) DDB Unavailable on every `try_acquire`. Both tasks must drop silently;
/// the user sees zero replies (operators alarm on the structured log).
#[tokio::test]
async fn unavailable_on_both_tasks_drops_silently() {
    let inner_a = InMemoryClusterCoordinator::new("task-A", chrono::Duration::minutes(5));
    let inner_b = inner_a.sibling("task-B");
    let coord_a: Arc<dyn Coordinator> = Arc::new(UnavailableCoordinator { inner: inner_a });
    let coord_b: Arc<dyn Coordinator> = Arc::new(UnavailableCoordinator { inner: inner_b });

    let frontend_a = Arc::new(RecorderFrontend::new());
    let frontend_b = Arc::new(RecorderFrontend::new());
    let core_a = build_core(coord_a);
    let core_b = build_core(coord_b);

    let conv = Conversation::Channel("C-unavail".into());
    let env = slack_envelope("EvUnavail");

    dispatch(
        &core_a,
        incoming("hello", conv.clone(), Some(env.clone())),
        frontend_a.clone(),
    );
    dispatch(&core_b, incoming("hello", conv, Some(env)), frontend_b.clone());

    drain().await;

    assert_eq!(
        frontend_a.send_count() + frontend_b.send_count(),
        0,
        "Unavailable on both tasks must drop with zero user-visible replies\n  A: {:?}\n  B: {:?}",
        frontend_a.sends(),
        frontend_b.sends()
    );
}

/// Wraps the inner coordinator and forces `forward()` to fail. `try_acquire`
/// continues to return Held{peer="task-A"} on B so we exercise the full
/// Held → forward-fail → force_acquire → process-locally recovery path.
struct ForwardFailingCoordinator {
    inner: InMemoryClusterCoordinator,
}

#[async_trait]
impl Coordinator for ForwardFailingCoordinator {
    async fn dedupe_event(&self, id: &str) -> bool {
        self.inner.dedupe_event(id).await
    }

    async fn try_acquire(&self, conv: &str) -> LeaseOutcome {
        self.inner.try_acquire(conv).await
    }

    async fn force_acquire(&self, conv: &str, peer: &str) -> bool {
        self.inner.force_acquire(conv, peer).await
    }

    async fn renew(&self, conv: &str) -> Result<()> {
        self.inner.renew(conv).await
    }

    async fn release(&self, conv: &str) -> Result<()> {
        self.inner.release(conv).await
    }

    async fn forward(&self, _peer: &str, _payload: ForwardEvent) -> Result<()> {
        anyhow::bail!("simulated peer down")
    }

    async fn append_turn(&self, conv: &str, turn: kiro_bot::engine::coordinator::Turn) -> Result<()> {
        self.inner.append_turn(conv, turn).await
    }

    async fn load_history(&self, conv: &str, limit: usize) -> Result<Vec<kiro_bot::engine::coordinator::Turn>> {
        self.inner.load_history(conv, limit).await
    }

    async fn register_approval(&self, ts: &str, conv: &str, ttl: std::time::Duration) -> Result<()> {
        self.inner.register_approval(ts, conv, ttl).await
    }

    async fn lookup_approval_owner(&self, ts: &str) -> Result<Option<String>> {
        self.inner.lookup_approval_owner(ts).await
    }
}

/// (5) Held + forward fails → non-leader force-acquires the dead peer's lease
/// and processes locally. User sees exactly one reply, even though task A
/// (the original lease holder) "died" before responding.
#[tokio::test]
async fn held_with_forward_failure_recovers_via_force_acquire() {
    let inner_a = InMemoryClusterCoordinator::new("task-A", chrono::Duration::minutes(5));
    let inner_b = inner_a.sibling("task-B");
    let coord_a: Arc<dyn Coordinator> = Arc::new(ForwardFailingCoordinator { inner: inner_a });
    let coord_b: Arc<dyn Coordinator> = Arc::new(ForwardFailingCoordinator { inner: inner_b });

    let conv_id_str = Conversation::Channel("C-held".into()).id();
    // Pre-stage: task-A holds the lease, then "dies" before responding. We
    // simulate this by acquiring on A and never releasing, then routing the
    // event only to B. B sees Held{peer=task-A}, tries to forward, fails,
    // force-acquires, and processes locally.
    assert_eq!(coord_a.try_acquire(&conv_id_str).await, LeaseOutcome::Acquired);

    let frontend_b = Arc::new(RecorderFrontend::new());
    let core_b = build_core(coord_b);

    let env = slack_envelope("EvHeld");
    dispatch(
        &core_b,
        incoming("hello", Conversation::Channel("C-held".into()), Some(env)),
        frontend_b.clone(),
    );

    drain().await;

    // After force-acquire, B processes the prompt locally → ack + final reply.
    assert_eq!(
        frontend_b.send_count(),
        2,
        "force-acquire path must produce exactly 2 sends (ack + final), got {}\n  B: {:?}",
        frontend_b.send_count(),
        frontend_b.sends()
    );
}

/// (6) The reaction dedup key is composite (ts:user:emoji). Same reaction
/// arriving twice — once at each task — must dedupe. (We exercise the
/// coordinator-level guarantee here; the slack frontend's `handle_reaction`
/// computes this same key and short-circuits via `dedupe_event`.)
#[tokio::test]
async fn reaction_dedup_key_is_cluster_wide() {
    let task_a = InMemoryClusterCoordinator::new("task-A", chrono::Duration::minutes(5));
    let task_b = task_a.sibling("task-B");

    let key = format!("rxn:{}:{}:{}", "1700000000.0001", "U1", "+1");
    assert!(task_a.dedupe_event(&key).await, "first sighting on A → true");
    assert!(
        !task_b.dedupe_event(&key).await,
        "second sighting on B → false (cluster-wide dedup)"
    );
}
