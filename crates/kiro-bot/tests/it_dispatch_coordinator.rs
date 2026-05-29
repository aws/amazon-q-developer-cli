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
            }
        }
    });
}

fn build_core(coordinator: Arc<dyn Coordinator>) -> BotCore {
    let (tx, rx) = mpsc::unbounded_channel();
    spawn_fake_acp(rx);
    BotCore {
        work_sender: tx,
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
