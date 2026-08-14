//! Integration tests for the multi-task coordinator + dispatch wiring.
//!
//! These exercise [`engine::core::dispatch`] across two simulated ECS tasks
//! sharing one `InMemoryClusterCoordinator`, asserting that the user-visible
//! contract — exactly one reply per Slack event — holds across all
//! coordinator outcomes (Acquired / Held / Unavailable / forward failure).

use std::sync::atomic::{
    AtomicBool,
    AtomicUsize,
    Ordering,
};
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
    DedupeOutcome,
    DedupeToken,
    ForwardEvent,
    InMemoryClusterCoordinator,
    LeaseAcquisition,
    LeaseManager,
    LeaseToken,
    RateLimitOutcome,
};
use kiro_bot::engine::core::{
    BotCore,
    Conversation,
    DispatchRouting,
    dispatch,
    dispatch_with_receipt,
};
use kiro_bot::engine::dispatch_server::{
    DISPATCH_TOKEN_HEADER,
    DispatchState,
    Dispatcher,
    router,
};
use kiro_bot::engine::response_policy::ResponsePolicyConfig;
use serde_json::json;
use tokio::sync::{
    Notify,
    mpsc,
};

#[path = "it_dispatch_coordinator/support.rs"]
mod support;

use support::*;

struct ParsingDispatcher;

#[async_trait]
impl Dispatcher for ParsingDispatcher {
    async fn process_as_if_from_slack(&self, event: serde_json::Value) -> Result<()> {
        serde_json::from_value::<slack_morphism::prelude::SlackPushEventCallback>(event)?;
        Ok(())
    }
}

struct RoutingFailureDispatcher;

#[async_trait]
impl Dispatcher for RoutingFailureDispatcher {
    async fn process_as_if_from_slack(&self, _event: serde_json::Value) -> Result<()> {
        anyhow::bail!("receiver routing failed")
    }
}

#[derive(Default)]
struct BlockingAdmissionDispatcher {
    started: Notify,
    admit: Notify,
}

#[async_trait]
impl Dispatcher for BlockingAdmissionDispatcher {
    async fn process_as_if_from_slack(&self, _event: serde_json::Value) -> Result<()> {
        self.started.notify_one();
        self.admit.notified().await;
        Ok(())
    }
}

async fn serve_dispatcher(dispatcher: Arc<dyn Dispatcher>) -> std::net::SocketAddr {
    let app = router(DispatchState {
        dispatcher,
        token: Arc::new("test-dispatch-token".into()),
    });
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    address
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn dispatch_endpoint_returns_non_success_for_parse_failure() {
    let address = serve_dispatcher(Arc::new(ParsingDispatcher)).await;
    let response = reqwest::Client::new()
        .post(format!("http://{address}/dispatch"))
        .header(DISPATCH_TOKEN_HEADER, "test-dispatch-token")
        .json(&json!({"type": "event_callback"}))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), 503);
}

#[tokio::test]
async fn dispatch_endpoint_returns_non_success_for_routing_failure() {
    let address = serve_dispatcher(Arc::new(RoutingFailureDispatcher)).await;
    let response = reqwest::Client::new()
        .post(format!("http://{address}/dispatch"))
        .header(DISPATCH_TOKEN_HEADER, "test-dispatch-token")
        .json(&json!({"type": "event_callback"}))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), 503);
}

#[tokio::test]
async fn dispatch_endpoint_acknowledges_only_after_receiver_admission() {
    let dispatcher = Arc::new(BlockingAdmissionDispatcher::default());
    let address = serve_dispatcher(dispatcher.clone()).await;
    let request = tokio::spawn(async move {
        reqwest::Client::new()
            .post(format!("http://{address}/dispatch"))
            .header(DISPATCH_TOKEN_HEADER, "test-dispatch-token")
            .json(&json!({"type": "event_callback"}))
            .send()
            .await
            .unwrap()
    });

    dispatcher.started.notified().await;
    tokio::task::yield_now().await;
    assert!(!request.is_finished(), "peer acknowledgement must wait for admission");

    dispatcher.admit.notify_one();
    let response = request.await.unwrap();
    assert_eq!(response.status(), 200);
}

#[tokio::test]
async fn command_receipt_resolves_before_slow_reply_delivery() {
    let coordinator = Arc::new(kiro_bot::engine::coordinator::NoopCoordinator::new());
    let core = build_core(coordinator);
    let frontend = Arc::new(BlockingInitialProgressFrontend::default());

    let receipt = dispatch_with_receipt(
        &core,
        incoming(
            "!help",
            Conversation::Channel("C-command-latency".into()),
            Some(slack_envelope("EvCommandLatency")),
        ),
        frontend.clone(),
    );
    let routing = tokio::time::timeout(Duration::from_millis(100), receipt)
        .await
        .expect("command routing must stay well within the socket acknowledgement budget")
        .unwrap()
        .unwrap();
    assert_eq!(routing, DispatchRouting::Local);

    frontend.progress_started.notified().await;
    frontend.continue_progress.notify_one();
}

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

    let receipt_a = dispatch_with_receipt(
        &core_a,
        incoming("hello", conv.clone(), Some(env.clone())),
        frontend_a.clone(),
    );
    let receipt_b = dispatch_with_receipt(&core_b, incoming("hello", conv, Some(env)), frontend_b.clone());
    let (routing_a, routing_b) = tokio::join!(receipt_a, receipt_b);
    routing_a.unwrap().unwrap();
    routing_b.unwrap().unwrap();
    wait_for_total_send_count(&[frontend_a.as_ref(), frontend_b.as_ref()], 2).await;

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

#[tokio::test]
async fn prompt_stream_targets_source_thread_and_requester_workspace() {
    let core = build_core(Arc::new(kiro_bot::engine::coordinator::NoopCoordinator::new()));
    let frontend = Arc::new(RecorderFrontend::new());
    let mut message = incoming(
        "hello",
        Conversation::Channel("C-stream-target".into()),
        Some(slack_envelope("EvStreamTarget")),
    );
    message.slack_user_id = "U-requester".into();
    message.slack_team_id = "T-workspace".into();
    message.source_message_id = Some("170.1".into());

    dispatch_with_receipt(&core, message, frontend.clone())
        .await
        .unwrap()
        .unwrap();
    frontend.wait_for_send_count(2).await;

    let mut followup = incoming(
        "follow up",
        Conversation::Thread {
            channel: "C-stream-target".into(),
            thread_ts: "170.1".into(),
        },
        Some(slack_envelope("EvStreamFollowup")),
    );
    followup.slack_user_id = "U-requester".into();
    followup.slack_team_id = "T-workspace".into();
    followup.source_message_id = Some("170.2".into());
    followup.reply_to = Some("170.1".into());
    dispatch_with_receipt(&core, followup, frontend.clone())
        .await
        .unwrap()
        .unwrap();
    frontend.wait_for_send_count(4).await;

    assert_eq!(frontend.stream_targets(), vec![
        ("170.1".into(), "U-requester".into(), "T-workspace".into()),
        ("170.1".into(), "U-requester".into(), "T-workspace".into()),
    ]);
}

#[tokio::test]
async fn duplicate_events_do_not_prepare_prompt_inputs() {
    let task_a = Arc::new(InMemoryClusterCoordinator::new("task-A", chrono::Duration::minutes(5)));
    let task_b = Arc::new(task_a.sibling("task-B"));
    let core_a = build_core(task_a);
    let core_b = build_core(task_b);
    let count = Arc::new(AtomicUsize::new(0));
    let event = slack_envelope("EvLazy");
    let frontend_a = Arc::new(RecorderFrontend::new());
    let frontend_b = Arc::new(RecorderFrontend::new());

    let receipt_a = dispatch_with_receipt(
        &core_a,
        incoming_with_preparation(
            "inspect the attachment",
            Conversation::Channel("C-lazy".into()),
            event.clone(),
            count.clone(),
        ),
        frontend_a.clone(),
    );
    let receipt_b = dispatch_with_receipt(
        &core_b,
        incoming_with_preparation(
            "inspect the attachment",
            Conversation::Channel("C-lazy".into()),
            event,
            count.clone(),
        ),
        frontend_b.clone(),
    );
    let (routing_a, routing_b) = tokio::join!(receipt_a, receipt_b);
    routing_a.unwrap().unwrap();
    routing_b.unwrap().unwrap();
    wait_for_total_send_count(&[frontend_a.as_ref(), frontend_b.as_ref()], 2).await;
    assert_eq!(count.load(Ordering::Relaxed), 1);
}

#[tokio::test]
async fn forwarded_duplicates_are_deduped_on_the_receiving_task() {
    let coordinator = Arc::new(kiro_bot::engine::coordinator::NoopCoordinator::new());
    let core = build_core(coordinator);
    let frontend = Arc::new(RecorderFrontend::new());
    let conversation = Conversation::Channel("C-forwarded".into());
    let mut envelope = slack_envelope("EvForwarded");
    envelope.forwarded = true;

    let first = dispatch_with_receipt(
        &core,
        incoming("hello", conversation.clone(), Some(envelope.clone())),
        frontend.clone(),
    )
    .await
    .unwrap()
    .unwrap();
    let duplicate = dispatch_with_receipt(&core, incoming("hello", conversation, Some(envelope)), frontend.clone())
        .await
        .unwrap()
        .unwrap();

    assert_eq!(first, DispatchRouting::Local);
    assert_eq!(duplicate, DispatchRouting::Duplicate);
    frontend.wait_for_send_count(2).await;
    assert_eq!(frontend.send_count(), 2, "only one forwarded event may execute");
}

#[tokio::test]
async fn commands_do_not_consume_or_block_prompt_quota() {
    let mut core = build_core(Arc::new(kiro_bot::engine::coordinator::NoopCoordinator::new()));
    core.rate_limit = kiro_bot::config::RateLimitConfig {
        max_prompts: 1,
        window_secs: 60,
    };
    let frontend = Arc::new(RecorderFrontend::new());
    let conversation = Conversation::Channel("C-quota".into());

    let help_receipt = dispatch_with_receipt(
        &core,
        incoming("!help", conversation.clone(), Some(slack_envelope("EvHelp"))),
        frontend.clone(),
    );
    let cancel_receipt = dispatch_with_receipt(
        &core,
        incoming("!cancel", conversation.clone(), Some(slack_envelope("EvCancel"))),
        frontend.clone(),
    );
    let (help_routing, cancel_routing) = tokio::join!(help_receipt, cancel_receipt);
    help_routing.unwrap().unwrap();
    cancel_routing.unwrap().unwrap();
    frontend.wait_for_send_count(2).await;

    dispatch_with_receipt(
        &core,
        incoming("first prompt", conversation.clone(), Some(slack_envelope("EvPrompt1"))),
        frontend.clone(),
    )
    .await
    .unwrap()
    .unwrap();
    frontend.wait_for_send_count(4).await;
    wait_for_dispatch_idle(&core, &conversation.id()).await;

    dispatch_with_receipt(
        &core,
        incoming("second prompt", conversation, Some(slack_envelope("EvPrompt2"))),
        frontend.clone(),
    )
    .await
    .unwrap()
    .unwrap();
    frontend.wait_for_send_count(5).await;

    let sends = frontend.sends();
    assert!(sends.iter().any(|text| text == "progress"));
    assert!(
        sends
            .iter()
            .any(|text| text.contains("You're sending requests too quickly"))
    );
}

#[tokio::test]
async fn accepted_prompts_persist_feedback_ready_transcript_turns() {
    let coordinator = Arc::new(kiro_bot::engine::coordinator::NoopCoordinator::new());
    let core = build_core_with_reply(
        coordinator.clone(),
        "Grounded answer.\n\nSources: `crates/kiro-bot/src/engine/core.rs:100-120`",
    );

    dispatch_with_receipt(
        &core,
        incoming(
            "How does dispatch work?",
            Conversation::Channel("C-transcript".into()),
            Some(slack_envelope("EvTranscript")),
        ),
        Arc::new(RecorderFrontend::new()),
    )
    .await
    .unwrap()
    .unwrap();

    let turns = wait_for_history_len(coordinator.as_ref(), "channel:C-transcript", 2).await;
    assert_eq!(turns.len(), 2);
    assert_eq!(turns[0].role, kiro_bot::engine::coordinator::TurnRole::User);
    assert_eq!(turns[0].text, "How does dispatch work?");
    assert_eq!(turns[1].chunk_ids, vec!["crates/kiro-bot/src/engine/core.rs:100-120"]);
}

#[tokio::test]
async fn failed_final_delivery_does_not_persist_an_unseen_assistant_turn() {
    let coordinator = Arc::new(kiro_bot::engine::coordinator::NoopCoordinator::new());
    let core = build_core_with_reply(coordinator.clone(), "Answer the user never received.");

    let frontend = Arc::new(FailingFinalFrontend::default());
    dispatch_with_receipt(
        &core,
        incoming(
            "Will this be delivered?",
            Conversation::Channel("C-delivery-failure".into()),
            Some(slack_envelope("EvDeliveryFailure")),
        ),
        frontend.clone(),
    )
    .await
    .unwrap()
    .unwrap();
    frontend.wait_for_send_count(2).await;

    let turns = coordinator
        .load_history("channel:C-delivery-failure", 10)
        .await
        .unwrap();
    assert_eq!(turns.len(), 1);
    assert_eq!(turns[0].role, kiro_bot::engine::coordinator::TurnRole::User);
}

#[tokio::test]
async fn queue_saturation_rejects_without_side_effects_and_retry_is_accepted() {
    let coordinator = Arc::new(kiro_bot::engine::coordinator::NoopCoordinator::new());
    let mut core = build_core(coordinator.clone());
    core.work_capacity = BotCore::work_capacity(1);
    let frontend = Arc::new(RecorderFrontend::new());
    let conversation = Conversation::Channel("C-queue-saturation".into());
    let envelope = slack_envelope("EvQueueSaturation");
    let capacity = core.work_capacity.clone().acquire_owned().await.unwrap();

    let error = dispatch_with_receipt(
        &core,
        incoming("hello", conversation.clone(), Some(envelope.clone())),
        frontend.clone(),
    )
    .await
    .unwrap()
    .unwrap_err();
    assert!(error.contains("capacity"));
    assert_eq!(frontend.send_count(), 0);
    assert!(
        coordinator
            .load_history(&conversation.id(), 10)
            .await
            .unwrap()
            .is_empty()
    );

    drop(capacity);
    let retry = dispatch_with_receipt(
        &core,
        incoming("hello", conversation.clone(), Some(envelope)),
        frontend.clone(),
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(retry, DispatchRouting::Local);
    let turns = wait_for_history_len(coordinator.as_ref(), &conversation.id(), 2).await;

    assert_eq!(frontend.send_count(), 2);
    assert_eq!(turns.len(), 2);
}

#[tokio::test]
async fn receiver_closure_at_admission_boundary_is_retryable_without_side_effects() {
    let coordinator = Arc::new(BlockingRateAdmissionCoordinator {
        inner: InMemoryClusterCoordinator::new("task-A", chrono::Duration::minutes(5)),
        started: Notify::new(),
        release: Notify::new(),
    });
    let (work_sender, work_receiver) = mpsc::unbounded_channel();
    let core = BotCore {
        work_sender,
        work_capacity: BotCore::work_capacity(1),
        inflight: Arc::new(std::sync::Mutex::new(std::collections::HashSet::new())),
        authz: None,
        response_policy: Arc::new(ResponsePolicyConfig::default_policy()),
        acp_info: Arc::new(std::sync::Mutex::new(AcpInfo::default())),
        coordinator: coordinator.clone(),
        lease_manager: LeaseManager::new(coordinator.clone()),
        rate_limit: kiro_bot::config::RateLimitConfig::default(),
    };
    let frontend = Arc::new(RecorderFrontend::new());
    let preparation_count = Arc::new(AtomicUsize::new(0));
    let conversation = Conversation::Channel("C-receiver-boundary".into());
    let envelope = slack_envelope("EvReceiverBoundary");

    let receipt = dispatch_with_receipt(
        &core,
        incoming_with_preparation(
            "inspect the attachment",
            conversation.clone(),
            envelope,
            preparation_count.clone(),
        ),
        frontend.clone(),
    );
    coordinator.started.notified().await;
    drop(work_receiver);
    coordinator.release.notify_one();

    let error = receipt.await.unwrap().unwrap_err();
    assert!(error.contains("ACP runtime is unavailable"));
    assert_eq!(frontend.send_count(), 0);
    assert_eq!(preparation_count.load(Ordering::Relaxed), 0);
    assert!(
        coordinator
            .load_history(&conversation.id(), 10)
            .await
            .unwrap()
            .is_empty()
    );
    assert!(matches!(
        coordinator.dedupe_event_outcome("EvReceiverBoundary").await,
        DedupeOutcome::Accepted { .. }
    ));
}

#[tokio::test]
async fn initial_progress_failure_keeps_accepted_work_and_dedup() {
    let coordinator = Arc::new(kiro_bot::engine::coordinator::NoopCoordinator::new());
    let core = build_core(coordinator.clone());
    let frontend = Arc::new(FailingInitialProgressFrontend::default());
    let conversation = Conversation::Channel("C-progress-failure".into());
    let envelope = slack_envelope("EvProgressFailure");

    let routing = dispatch_with_receipt(
        &core,
        incoming("hello", conversation.clone(), Some(envelope.clone())),
        frontend.clone(),
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(routing, DispatchRouting::Local);
    tokio::time::timeout(Duration::from_secs(1), frontend.direct_send_seen.notified())
        .await
        .expect("accepted work must fall back to a direct reply");

    {
        let sends = frontend.direct_sends.lock().unwrap();
        assert_eq!(sends.len(), 1);
        assert!(sends[0].contains("ok"));
    }
    let turns = wait_for_history_len(coordinator.as_ref(), &conversation.id(), 2).await;
    assert_eq!(turns.len(), 2);

    let duplicate = dispatch_with_receipt(
        &core,
        incoming("hello", conversation, Some(envelope)),
        Arc::new(RecorderFrontend::new()),
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(duplicate, DispatchRouting::Duplicate);
}

#[tokio::test]
async fn user_append_failure_does_not_cancel_accepted_work() {
    let coordinator = Arc::new(FailFirstUserAppendCoordinator {
        inner: InMemoryClusterCoordinator::new("task-A", chrono::Duration::minutes(5)),
        fail_next_user_append: AtomicBool::new(true),
    });
    let core = build_core(coordinator.clone());
    let frontend = Arc::new(RecorderFrontend::new());
    let conversation = Conversation::Channel("C-user-append-failure".into());
    let envelope = slack_envelope("EvUserAppendFailure");

    let routing = dispatch_with_receipt(
        &core,
        incoming("hello", conversation.clone(), Some(envelope.clone())),
        frontend.clone(),
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(routing, DispatchRouting::Local);

    let turns = wait_for_history_len(coordinator.as_ref(), &conversation.id(), 1).await;
    frontend.wait_for_send_count(2).await;
    assert_eq!(turns.len(), 1);
    assert_eq!(turns[0].role, kiro_bot::engine::coordinator::TurnRole::Assistant);

    let duplicate = dispatch_with_receipt(
        &core,
        incoming("hello", conversation, Some(envelope)),
        Arc::new(RecorderFrontend::new()),
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(duplicate, DispatchRouting::Duplicate);
}

#[tokio::test]
async fn callback_expiry_after_admission_does_not_cancel_work() {
    let coordinator = Arc::new(kiro_bot::engine::coordinator::NoopCoordinator::new());
    let core = build_core(coordinator.clone());
    let blocking_frontend = Arc::new(BlockingInitialProgressFrontend::default());
    let conversation = Conversation::Channel("C-closed-prompt-receipt".into());
    let envelope = slack_envelope("EvClosedPromptReceipt");

    let receipt = dispatch_with_receipt(
        &core,
        incoming("hello", conversation.clone(), Some(envelope.clone())),
        blocking_frontend.clone(),
    );
    blocking_frontend.progress_started.notified().await;
    drop(receipt);
    blocking_frontend.continue_progress.notify_one();
    blocking_frontend.progress_deleted.notified().await;
    wait_for_history_len(coordinator.as_ref(), &conversation.id(), 2).await;

    let duplicate = dispatch_with_receipt(
        &core,
        incoming("hello", conversation, Some(envelope)),
        Arc::new(RecorderFrontend::new()),
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(duplicate, DispatchRouting::Duplicate);
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

    let receipt_a = dispatch_with_receipt(
        &core_a,
        incoming("!help", conv.clone(), Some(env.clone())),
        frontend_a.clone(),
    );
    let receipt_b = dispatch_with_receipt(&core_b, incoming("!help", conv, Some(env)), frontend_b.clone());
    let (routing_a, routing_b) = tokio::join!(receipt_a, receipt_b);
    routing_a.unwrap().unwrap();
    routing_b.unwrap().unwrap();
    wait_for_total_send_count(&[frontend_a.as_ref(), frontend_b.as_ref()], 1).await;

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
        let receipt_a = dispatch_with_receipt(
            &core_a,
            incoming("hi", conv.clone(), Some(env.clone())),
            frontend_a.clone(),
        );
        let receipt_b = dispatch_with_receipt(&core_b, incoming("hi", conv.clone(), Some(env)), frontend_b.clone());
        let (routing_a, routing_b) = tokio::join!(receipt_a, receipt_b);
        routing_a.unwrap().unwrap();
        routing_b.unwrap().unwrap();
        wait_for_history_len(task_a.as_ref(), &conv.id(), (i + 1) * 2).await;
        wait_for_total_send_count(&[frontend_a.as_ref(), frontend_b.as_ref()], (i + 1) * 2).await;
        wait_for_dispatch_idle(&core_a, &conv.id()).await;
        wait_for_dispatch_idle(&core_b, &conv.id()).await;
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

#[derive(Clone, Copy)]
enum UnavailableOperation {
    Lease,
    Dedupe,
}

struct UnavailableCoordinator {
    inner: InMemoryClusterCoordinator,
    operation: UnavailableOperation,
}

struct DelayedAcquisitionCoordinator {
    inner: InMemoryClusterCoordinator,
    acquisition_started: Notify,
    continue_acquisition: Notify,
    acquisition_calls: AtomicUsize,
    acquired_token: Mutex<Option<LeaseToken>>,
    released_token: Mutex<Option<LeaseToken>>,
}

struct LeaseLossCoordinator {
    releases: std::sync::atomic::AtomicUsize,
}

struct FailFirstUserAppendCoordinator {
    inner: InMemoryClusterCoordinator,
    fail_next_user_append: AtomicBool,
}

struct BlockingRateAdmissionCoordinator {
    inner: InMemoryClusterCoordinator,
    started: Notify,
    release: Notify,
}

#[async_trait]
impl Coordinator for BlockingRateAdmissionCoordinator {
    async fn dedupe_event_outcome(&self, id: &str) -> DedupeOutcome {
        self.inner.dedupe_event_outcome(id).await
    }

    async fn release_dedup(&self, id: &str, token: &DedupeToken) -> Result<()> {
        self.inner.release_dedup(id, token).await
    }

    async fn acquire_lease(&self, conversation: &str) -> LeaseAcquisition {
        self.inner.acquire_lease(conversation).await
    }

    async fn renew(&self, conversation: &str, token: &LeaseToken) -> Result<()> {
        self.inner.renew(conversation, token).await
    }

    async fn release(&self, conversation: &str, token: &LeaseToken) -> Result<()> {
        self.inner.release(conversation, token).await
    }

    async fn forward(&self, peer: &str, payload: ForwardEvent) -> Result<()> {
        self.inner.forward(peer, payload).await
    }

    async fn append_turn(&self, conversation: &str, turn: kiro_bot::engine::coordinator::Turn) -> Result<()> {
        self.inner.append_turn(conversation, turn).await
    }

    async fn load_history(&self, conversation: &str, limit: usize) -> Result<Vec<kiro_bot::engine::coordinator::Turn>> {
        self.inner.load_history(conversation, limit).await
    }

    async fn register_approval(&self, ts: &str, conversation: &str, ttl: Duration) -> Result<()> {
        self.inner.register_approval(ts, conversation, ttl).await
    }

    async fn lookup_approval_owner(&self, ts: &str) -> Result<Option<String>> {
        self.inner.lookup_approval_owner(ts).await
    }

    async fn admit_prompt(&self, _user_id: &str, _max_prompts: u32, _window: Duration) -> Result<RateLimitOutcome> {
        self.started.notify_one();
        self.release.notified().await;
        Ok(RateLimitOutcome::Allowed)
    }
}

#[async_trait]
impl Coordinator for FailFirstUserAppendCoordinator {
    async fn dedupe_event_outcome(&self, id: &str) -> DedupeOutcome {
        self.inner.dedupe_event_outcome(id).await
    }

    async fn release_dedup(&self, id: &str, token: &DedupeToken) -> Result<()> {
        self.inner.release_dedup(id, token).await
    }

    async fn acquire_lease(&self, conversation: &str) -> LeaseAcquisition {
        self.inner.acquire_lease(conversation).await
    }

    async fn renew(&self, conversation: &str, token: &LeaseToken) -> Result<()> {
        self.inner.renew(conversation, token).await
    }

    async fn release(&self, conversation: &str, token: &LeaseToken) -> Result<()> {
        self.inner.release(conversation, token).await
    }

    async fn forward(&self, peer: &str, payload: ForwardEvent) -> Result<()> {
        self.inner.forward(peer, payload).await
    }

    async fn append_turn(&self, conversation: &str, turn: kiro_bot::engine::coordinator::Turn) -> Result<()> {
        if turn.role == kiro_bot::engine::coordinator::TurnRole::User
            && self.fail_next_user_append.swap(false, Ordering::Relaxed)
        {
            anyhow::bail!("simulated user transcript failure");
        }
        self.inner.append_turn(conversation, turn).await
    }

    async fn load_history(&self, conversation: &str, limit: usize) -> Result<Vec<kiro_bot::engine::coordinator::Turn>> {
        self.inner.load_history(conversation, limit).await
    }

    async fn register_approval(&self, ts: &str, conversation: &str, ttl: Duration) -> Result<()> {
        self.inner.register_approval(ts, conversation, ttl).await
    }

    async fn lookup_approval_owner(&self, ts: &str) -> Result<Option<String>> {
        self.inner.lookup_approval_owner(ts).await
    }
}

#[async_trait]
impl Coordinator for LeaseLossCoordinator {
    async fn dedupe_event_outcome(&self, _id: &str) -> DedupeOutcome {
        DedupeOutcome::Accepted {
            token: DedupeToken::new("lease-loss"),
        }
    }

    async fn release_dedup(&self, _id: &str, _token: &DedupeToken) -> Result<()> {
        Ok(())
    }

    async fn acquire_lease(&self, _conv: &str) -> LeaseAcquisition {
        LeaseAcquisition::Acquired {
            token: LeaseToken::new("lease-loss"),
        }
    }

    async fn renew(&self, _conv: &str, _token: &LeaseToken) -> Result<()> {
        anyhow::bail!("coordinator unavailable")
    }

    fn lease_heartbeat_interval(&self) -> Duration {
        Duration::from_secs(10)
    }

    fn lease_ttl(&self) -> Duration {
        Duration::from_secs(30)
    }

    async fn release(&self, _conv: &str, _token: &LeaseToken) -> Result<()> {
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
    async fn dedupe_event_outcome(&self, id: &str) -> DedupeOutcome {
        match self.operation {
            UnavailableOperation::Dedupe => DedupeOutcome::Unavailable,
            UnavailableOperation::Lease => self.inner.dedupe_event_outcome(id).await,
        }
    }

    async fn release_dedup(&self, id: &str, token: &DedupeToken) -> Result<()> {
        self.inner.release_dedup(id, token).await
    }

    async fn acquire_lease(&self, conv: &str) -> LeaseAcquisition {
        match self.operation {
            UnavailableOperation::Lease => LeaseAcquisition::Unavailable,
            UnavailableOperation::Dedupe => self.inner.acquire_lease(conv).await,
        }
    }

    async fn renew(&self, conv: &str, token: &LeaseToken) -> Result<()> {
        self.inner.renew(conv, token).await
    }

    async fn release(&self, conv: &str, token: &LeaseToken) -> Result<()> {
        self.inner.release(conv, token).await
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

#[async_trait]
impl Coordinator for DelayedAcquisitionCoordinator {
    async fn dedupe_event_outcome(&self, id: &str) -> DedupeOutcome {
        self.inner.dedupe_event_outcome(id).await
    }

    async fn release_dedup(&self, id: &str, token: &DedupeToken) -> Result<()> {
        self.inner.release_dedup(id, token).await
    }

    async fn acquire_lease(&self, conv: &str) -> LeaseAcquisition {
        self.acquisition_calls.fetch_add(1, Ordering::Relaxed);
        self.acquisition_started.notify_one();
        self.continue_acquisition.notified().await;
        let acquisition = self.inner.acquire_lease(conv).await;
        if let LeaseAcquisition::Acquired { token } = &acquisition {
            *self.acquired_token.lock().unwrap() = Some(token.clone());
        }
        acquisition
    }

    async fn renew(&self, conv: &str, token: &LeaseToken) -> Result<()> {
        self.inner.renew(conv, token).await
    }

    async fn release(&self, conv: &str, token: &LeaseToken) -> Result<()> {
        *self.released_token.lock().unwrap() = Some(token.clone());
        self.inner.release(conv, token).await
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

    async fn register_approval(&self, ts: &str, conv: &str, ttl: Duration) -> Result<()> {
        self.inner.register_approval(ts, conv, ttl).await
    }

    async fn lookup_approval_owner(&self, ts: &str) -> Result<Option<String>> {
        self.inner.lookup_approval_owner(ts).await
    }
}

#[tokio::test]
async fn closed_receipt_finishes_acquisition_and_releases_exact_token() {
    let inner = InMemoryClusterCoordinator::new("task-A", chrono::Duration::minutes(5));
    let peer = inner.sibling("task-B");
    let coordinator = Arc::new(DelayedAcquisitionCoordinator {
        inner,
        acquisition_started: Notify::new(),
        continue_acquisition: Notify::new(),
        acquisition_calls: AtomicUsize::new(0),
        acquired_token: Mutex::new(None),
        released_token: Mutex::new(None),
    });
    let core = build_core(coordinator.clone());
    let frontend = Arc::new(RecorderFrontend::new());

    let receipt = dispatch_with_receipt(
        &core,
        incoming(
            "hello",
            Conversation::Channel("C-closed-receipt".into()),
            Some(slack_envelope("EvClosedReceipt")),
        ),
        frontend.clone(),
    );
    coordinator.acquisition_started.notified().await;
    drop(receipt);
    coordinator.continue_acquisition.notify_one();

    tokio::time::timeout(Duration::from_secs(1), async {
        loop {
            if coordinator.released_token.lock().unwrap().is_some() {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("closed receipt should release a completed acquisition");

    assert_eq!(
        *coordinator.acquired_token.lock().unwrap(),
        *coordinator.released_token.lock().unwrap(),
        "the release must use the exact acquired generation token"
    );
    assert!(matches!(
        peer.acquire_lease("channel:C-closed-receipt").await,
        LeaseAcquisition::Acquired { .. }
    ));
    assert_eq!(frontend.send_count(), 0);
}

#[tokio::test]
async fn forwarded_cancel_shares_owner_lease_and_confirms_once() {
    let inner = InMemoryClusterCoordinator::new("task-A", chrono::Duration::minutes(5));
    let peer = inner.sibling("task-B");
    let coordinator = Arc::new(DelayedAcquisitionCoordinator {
        inner,
        acquisition_started: Notify::new(),
        continue_acquisition: Notify::new(),
        acquisition_calls: AtomicUsize::new(0),
        acquired_token: Mutex::new(None),
        released_token: Mutex::new(None),
    });

    let (work_sender, mut work_receiver) = mpsc::unbounded_channel();
    let (prompt_seen_tx, prompt_seen_rx) = tokio::sync::oneshot::channel();
    let (cancel_seen_tx, cancel_seen_rx) = tokio::sync::oneshot::channel();
    tokio::spawn(async move {
        let mut prompt_seen_tx = Some(prompt_seen_tx);
        let mut cancel_seen_tx = Some(cancel_seen_tx);
        let mut pending_prompt = None;
        while let Some(work) = work_receiver.recv().await {
            match work {
                Work::Prompt { input, reply_tx, .. } => {
                    accept_prompt_input(input).await;
                    pending_prompt = Some(reply_tx);
                    if let Some(tx) = prompt_seen_tx.take() {
                        let _ = tx.send(());
                    }
                },
                Work::Cancel { .. } => {
                    drop(pending_prompt.take());
                    if let Some(tx) = cancel_seen_tx.take() {
                        let _ = tx.send(());
                    }
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
        lease_manager: LeaseManager::new(coordinator.clone()),
        rate_limit: kiro_bot::config::RateLimitConfig::default(),
    };
    let frontend = Arc::new(BlockingCancelFrontend::default());
    let conversation = Conversation::Channel("C-forwarded-cancel".into());

    let prompt_receipt = dispatch_with_receipt(
        &core,
        incoming(
            "slow question",
            conversation.clone(),
            Some(slack_envelope("EvSlowPrompt")),
        ),
        frontend.clone(),
    );
    coordinator.acquisition_started.notified().await;
    coordinator.continue_acquisition.notify_one();
    assert_eq!(prompt_receipt.await.unwrap().unwrap(), DispatchRouting::Local);
    prompt_seen_rx.await.unwrap();

    let mut cancel_envelope = slack_envelope("EvCancelOnce");
    cancel_envelope.forwarded = true;
    let cancel_routing = dispatch_with_receipt(
        &core,
        incoming("!cancel", conversation.clone(), Some(cancel_envelope.clone())),
        frontend.clone(),
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(cancel_routing, DispatchRouting::Local);
    cancel_seen_rx.await.unwrap();
    frontend.cancel_send_started.notified().await;
    frontend.final_send_seen.notified().await;

    assert_eq!(
        coordinator.acquisition_calls.load(Ordering::Relaxed),
        1,
        "the forwarded cancel must share the prompt's local lease"
    );
    assert!(matches!(
        peer.acquire_lease(&conversation.id()).await,
        LeaseAcquisition::Held { peer } if peer == "task-A"
    ));

    let duplicate = dispatch_with_receipt(
        &core,
        incoming("!cancel", conversation.clone(), Some(cancel_envelope)),
        frontend.clone(),
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(duplicate, DispatchRouting::Duplicate);

    frontend.continue_cancel_send.notify_one();
    tokio::time::timeout(Duration::from_secs(1), async {
        loop {
            if matches!(
                peer.acquire_lease(&conversation.id()).await,
                LeaseAcquisition::Acquired { .. }
            ) {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("the shared lease should release after the cancel confirmation");

    assert_eq!(
        frontend.sends().iter().filter(|text| text.as_str() == "🛑").count(),
        1,
        "duplicate cancel deliveries must produce one confirmation"
    );
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
                Work::Prompt { input, reply_tx, .. } => {
                    accept_prompt_input(input).await;
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
        lease_manager: LeaseManager::new(coordinator.clone()),
        rate_limit: kiro_bot::config::RateLimitConfig::default(),
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
                Work::Prompt { input, reply_tx, .. } => {
                    accept_prompt_input(input).await;
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
        lease_manager: LeaseManager::new(coordinator.clone()),
        rate_limit: kiro_bot::config::RateLimitConfig::default(),
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

/// (4) DDB Unavailable on every acquisition. Both tasks must reject routing
/// so the Socket Mode callback can leave the event unacknowledged.
#[tokio::test]
async fn unavailable_on_both_tasks_rejects_routing() {
    let inner_a = InMemoryClusterCoordinator::new("task-A", chrono::Duration::minutes(5));
    let inner_b = inner_a.sibling("task-B");
    let coord_a: Arc<dyn Coordinator> = Arc::new(UnavailableCoordinator {
        inner: inner_a,
        operation: UnavailableOperation::Lease,
    });
    let coord_b: Arc<dyn Coordinator> = Arc::new(UnavailableCoordinator {
        inner: inner_b,
        operation: UnavailableOperation::Lease,
    });

    let frontend_a = Arc::new(RecorderFrontend::new());
    let frontend_b = Arc::new(RecorderFrontend::new());
    let core_a = build_core(coord_a);
    let core_b = build_core(coord_b);

    let conv = Conversation::Channel("C-unavail".into());
    let env = slack_envelope("EvUnavail");

    let receipt_a = dispatch_with_receipt(
        &core_a,
        incoming("hello", conv.clone(), Some(env.clone())),
        frontend_a.clone(),
    );
    let receipt_b = dispatch_with_receipt(&core_b, incoming("hello", conv, Some(env)), frontend_b.clone());

    let error_a = receipt_a.await.unwrap().unwrap_err();
    let error_b = receipt_b.await.unwrap().unwrap_err();
    assert!(error_a.contains("coordinator unavailable"));
    assert!(error_b.contains("coordinator unavailable"));

    assert_eq!(
        frontend_a.send_count() + frontend_b.send_count(),
        0,
        "Unavailable on both tasks must drop with zero user-visible replies\n  A: {:?}\n  B: {:?}",
        frontend_a.sends(),
        frontend_b.sends()
    );
}

#[tokio::test]
async fn dedupe_backend_failure_rejects_routing_without_acknowledging() {
    let inner = InMemoryClusterCoordinator::new("task-A", chrono::Duration::minutes(5));
    let peer = inner.sibling("task-B");
    let coordinator: Arc<dyn Coordinator> = Arc::new(UnavailableCoordinator {
        inner,
        operation: UnavailableOperation::Dedupe,
    });
    let frontend = Arc::new(RecorderFrontend::new());
    let core = build_core(coordinator);

    let error = dispatch_with_receipt(
        &core,
        incoming(
            "hello",
            Conversation::Channel("C-dedupe-unavailable".into()),
            Some(slack_envelope("EvDedupeUnavailable")),
        ),
        frontend.clone(),
    )
    .await
    .unwrap()
    .unwrap_err();

    assert!(error.contains("recording event dedup"));
    assert_eq!(frontend.send_count(), 0);
    tokio::time::timeout(Duration::from_secs(1), async {
        loop {
            if matches!(
                peer.acquire_lease("channel:C-dedupe-unavailable").await,
                LeaseAcquisition::Acquired { .. }
            ) {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("failed dedup must release the conversation lease");
}

/// Wraps the inner coordinator and forces `forward()` to fail while the
/// original task continues to hold the lease.
struct ForwardFailingCoordinator {
    inner: InMemoryClusterCoordinator,
    forward_calls: Arc<AtomicUsize>,
}

#[async_trait]
impl Coordinator for ForwardFailingCoordinator {
    async fn dedupe_event_outcome(&self, id: &str) -> DedupeOutcome {
        self.inner.dedupe_event_outcome(id).await
    }

    async fn release_dedup(&self, id: &str, token: &DedupeToken) -> Result<()> {
        self.inner.release_dedup(id, token).await
    }

    async fn acquire_lease(&self, conv: &str) -> LeaseAcquisition {
        self.inner.acquire_lease(conv).await
    }

    async fn renew(&self, conv: &str, token: &LeaseToken) -> Result<()> {
        self.inner.renew(conv, token).await
    }

    async fn release(&self, conv: &str, token: &LeaseToken) -> Result<()> {
        self.inner.release(conv, token).await
    }

    async fn forward(&self, _peer: &str, _payload: ForwardEvent) -> Result<()> {
        self.forward_calls.fetch_add(1, Ordering::Relaxed);
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

/// A transport failure is not proof that the owner died. The receiving task
/// must leave the live lease alone instead of running a concurrent prompt.
#[tokio::test]
async fn forward_failure_does_not_steal_live_peer_lease() {
    let inner_a = InMemoryClusterCoordinator::new("task-A", chrono::Duration::minutes(5));
    let inner_b = inner_a.sibling("task-B");
    let forward_calls = Arc::new(AtomicUsize::new(0));
    let coord_a: Arc<dyn Coordinator> = Arc::new(ForwardFailingCoordinator {
        inner: inner_a,
        forward_calls: forward_calls.clone(),
    });
    let coord_b: Arc<dyn Coordinator> = Arc::new(ForwardFailingCoordinator {
        inner: inner_b,
        forward_calls,
    });

    let conv_id_str = Conversation::Channel("C-held".into()).id();
    assert!(matches!(
        coord_a.acquire_lease(&conv_id_str).await,
        LeaseAcquisition::Acquired { .. }
    ));

    let frontend_b = Arc::new(RecorderFrontend::new());
    let core_b = build_core(coord_b.clone());

    let env = slack_envelope("EvHeld");
    let receipt = dispatch_with_receipt(
        &core_b,
        incoming("hello", Conversation::Channel("C-held".into()), Some(env)),
        frontend_b.clone(),
    );

    let error = receipt.await.unwrap().unwrap_err();

    assert!(error.contains("simulated peer down"));
    assert_eq!(frontend_b.send_count(), 0, "B must not process while A owns the lease");
    assert!(
        coord_b.dedupe_event("EvHeld").await,
        "a failed forward must leave dedup uncommitted for Slack's retry"
    );
    assert!(matches!(
        coord_b.acquire_lease(&conv_id_str).await,
        LeaseAcquisition::Held { peer } if peer == "task-A"
    ));
}

#[tokio::test]
async fn forwarded_event_on_non_owner_is_rejected_without_forwarding_again() {
    let task_a = InMemoryClusterCoordinator::new("task-A", chrono::Duration::minutes(5));
    let task_b = task_a.sibling("task-B");
    let conversation = Conversation::Channel("C-forward-loop".into());
    assert!(matches!(
        task_a.acquire_lease(&conversation.id()).await,
        LeaseAcquisition::Acquired { .. }
    ));

    let forward_calls = Arc::new(AtomicUsize::new(0));
    let coordinator: Arc<dyn Coordinator> = Arc::new(ForwardFailingCoordinator {
        inner: task_b,
        forward_calls: forward_calls.clone(),
    });
    let core = build_core(coordinator.clone());
    let frontend = Arc::new(RecorderFrontend::new());
    let mut envelope = slack_envelope("EvForwardLoop");
    envelope.forwarded = true;

    let error = dispatch_with_receipt(&core, incoming("hello", conversation, Some(envelope)), frontend.clone())
        .await
        .unwrap()
        .unwrap_err();

    assert!(error.contains("forwarded event reached non-owner"));
    assert_eq!(forward_calls.load(Ordering::Relaxed), 0);
    assert_eq!(frontend.send_count(), 0);
    assert!(
        matches!(
            coordinator.dedupe_event_outcome("EvForwardLoop").await,
            DedupeOutcome::Accepted { .. }
        ),
        "a misrouted forward must remain retryable"
    );
}

#[tokio::test]
async fn successful_forward_leaves_dedup_for_the_receiving_task() {
    let coord_a = Arc::new(InMemoryClusterCoordinator::new("task-A", chrono::Duration::minutes(5)));
    let coord_b = Arc::new(coord_a.sibling("task-B"));
    let conversation = Conversation::Channel("C-forward".into());
    let conversation_id = conversation.id();
    assert!(matches!(
        coord_a.acquire_lease(&conversation_id).await,
        LeaseAcquisition::Acquired { .. }
    ));

    let core_b = build_core(coord_b.clone());
    let routing = dispatch_with_receipt(
        &core_b,
        incoming("hello", conversation, Some(slack_envelope("EvForward"))),
        Arc::new(RecorderFrontend::new()),
    )
    .await
    .unwrap()
    .unwrap();

    assert_eq!(routing, DispatchRouting::Forwarded);
    assert!(
        coord_b.dedupe_event("EvForward").await,
        "the forwarding source must not consume receiver-side dedup"
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

#[tokio::test]
async fn dedup_cleanup_removes_only_the_matching_generation() {
    let coordinator = kiro_bot::engine::coordinator::NoopCoordinator::new();
    let token = match coordinator.dedupe_event_outcome("EvGeneration").await {
        DedupeOutcome::Accepted { token } => token,
        outcome => panic!("expected accepted dedup reservation, got {outcome:?}"),
    };

    coordinator
        .release_dedup("EvGeneration", &DedupeToken::new("different-generation"))
        .await
        .unwrap();
    assert_eq!(
        coordinator.dedupe_event_outcome("EvGeneration").await,
        DedupeOutcome::Duplicate
    );

    coordinator.release_dedup("EvGeneration", &token).await.unwrap();
    assert!(matches!(
        coordinator.dedupe_event_outcome("EvGeneration").await,
        DedupeOutcome::Accepted { .. }
    ));
}
