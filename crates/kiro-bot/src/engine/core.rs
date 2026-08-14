//! Frontend-agnostic bot core: message dispatch, command resolution, and reply routing.
//!
//! The core receives [`IncomingMessage`]s from any frontend, resolves them to
//! [`Action`]s, checks authorization, and dispatches work to the ACP pool.

use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use async_trait::async_trait;
use tokio::sync::{
    mpsc,
    oneshot,
};
use tracing::info;

use crate::engine::acp::{
    AcpInfo,
    CANCEL_CONFIRM_TIMEOUT,
    ProgressStatus,
    ProgressUpdate,
    PromptInput,
    PromptPayload,
    Work,
};
use crate::engine::authz::Authorizer;
use crate::engine::coordinator::{
    DedupeOutcome,
    DedupeToken,
    ForwardEvent,
    LeaseGuard,
    LeaseManager,
    ManagedLeaseAcquisition,
    RateLimitOutcome,
    Turn,
    TurnRole,
};
use crate::engine::response_policy::{
    Location,
    ResponsePolicyConfig,
};

/// Appended to every model-generated answer. Slack users can't see which
/// replies came from an LLM, so the disclaimer travels with the answer rather
/// than living only in the app description.
pub const GENAI_DISCLAIMER: &str = "_AI-generated — may be incorrect or incomplete. Verify before acting on it._";

/// Attach [`GENAI_DISCLAIMER`] unless it's already the tail of the text.
pub fn with_genai_disclaimer(text: &str) -> String {
    if text.trim_end().ends_with(GENAI_DISCLAIMER) {
        return text.to_string();
    }
    format!("{}\n\n{GENAI_DISCLAIMER}", text.trim_end())
}

fn progress_status_for_reply(text: &str) -> ProgressStatus {
    let text = text.trim();
    if text.starts_with("Error") || text == "❌ Cancelled" {
        ProgressStatus::Error
    } else {
        ProgressStatus::Complete
    }
}

// ---------------------------------------------------------------------------
// Conversation types
// ---------------------------------------------------------------------------

/// A conversation scope used for session keying and Cedar authorization.
#[derive(Debug, Clone)]
pub enum Conversation {
    /// Slack DM thread. Cedar resource: `Conversation::"dm:<user>"`.
    Dm {
        channel: String,
        user: String,
        thread_ts: String,
    },
    /// Channel-level. Cedar resource: `Conversation::"channel:<id>"`.
    Channel(String),
    /// Thread-scoped. Authorization inherits from the parent channel.
    Thread { channel: String, thread_ts: String },
}

impl Conversation {
    /// Unique ID used as ACP session key.
    pub fn id(&self) -> String {
        match self {
            Self::Dm { channel, thread_ts, .. } | Self::Thread { channel, thread_ts } => {
                Self::thread_session_id(channel, thread_ts)
            },
            Self::Channel(id) => format!("channel:{id}"),
        }
    }

    /// Build the session key shared by Slack channel and DM threads.
    pub(crate) fn thread_session_id(channel: &str, thread_ts: &str) -> String {
        format!("thread:{channel}:{thread_ts}")
    }

    /// Raw Slack channel ID for sending messages.
    pub fn platform_id(&self) -> &str {
        match self {
            Self::Dm { channel, .. } | Self::Channel(channel) | Self::Thread { channel, .. } => channel,
        }
    }

    /// ID used for Cedar authorization — threads inherit parent channel access.
    pub fn authz_id(&self) -> String {
        match self {
            Self::Dm { user, .. } => format!("dm:{user}"),
            Self::Thread { channel, .. } => format!("channel:{channel}"),
            Self::Channel(id) => format!("channel:{id}"),
        }
    }
}

// ---------------------------------------------------------------------------
// Message and reply types
// ---------------------------------------------------------------------------

#[async_trait]
pub trait PromptPreparation: Send {
    async fn prepare(self: Box<Self>, text: &mut String);
}

pub struct IncomingMessage {
    pub user: String,
    pub slack_user_id: String,
    pub slack_team_id: String,
    pub source_message_id: Option<String>,
    pub text: String,
    pub conversation: Conversation,
    pub reply_to: Option<String>,
    pub directed: bool,
    pub context: Vec<String>,
    /// Frontend-specific input work that must happen only after this event
    /// wins dedup, ownership, authorization, and rate admission.
    pub prompt_preparation: Option<Box<dyn PromptPreparation>>,
    /// Source-frontend dispatch envelope. `Some` for Slack-delivered events
    /// (so the dispatch path can dedupe by `event_id` and forward the raw
    /// JSON to a peer); `None` for CLI / cron / other single-process
    /// frontends where cross-task arbitration is a no-op.
    pub envelope: Option<DispatchEnvelope>,
}

/// Per-event metadata threaded from a Slack-style frontend into
/// [`dispatch`] so it can deduplicate the event cluster-wide and forward
/// the raw payload to a peer task that already owns the conversation lease.
#[derive(Debug, Clone)]
pub struct DispatchEnvelope {
    /// Cluster-wide unique id (e.g. Slack `event_id`). Used as the dedup key.
    pub event_id: String,
    /// Raw event JSON to ship to a peer via `coordinator.forward()`.
    pub raw: serde_json::Value,
    /// Whether this event arrived through the peer dispatch endpoint.
    pub forwarded: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DispatchRouting {
    Local,
    Forwarded,
    Duplicate,
}

pub type DispatchReceipt = oneshot::Receiver<std::result::Result<DispatchRouting, String>>;

#[derive(Debug)]
pub enum Reply {
    Send {
        conversation: String,
        reply_to: Option<String>,
        text: String,
    },
    StartProgress {
        conversation: String,
        reply_to: String,
        recipient_user_id: String,
        recipient_team_id: String,
    },
    Progress {
        conversation: String,
        message_id: String,
        update: ProgressUpdate,
    },
    FinishProgress {
        conversation: String,
        message_id: String,
        reply_to: String,
        text: String,
        status: ProgressStatus,
    },
    Update {
        conversation: String,
        message_id: String,
        text: String,
    },
    Delete {
        conversation: String,
        message_id: String,
    },
}

// ---------------------------------------------------------------------------
// Frontend trait
// ---------------------------------------------------------------------------

#[async_trait]
pub trait Frontend: Send + Sync {
    async fn send(&self, reply: Reply) -> Result<String>;
    async fn fetch_context(&self, conversation: &str, before: &str, thread_ts: Option<&str>) -> Vec<String>;
}

/// Format conversation context messages for the agent prompt.
pub fn format_context(context: &[String]) -> Option<String> {
    if context.is_empty() {
        return None;
    }
    Some(format!("Recent conversation context:\n{}", context.join("\n")))
}

// ---------------------------------------------------------------------------
// Bot core state
// ---------------------------------------------------------------------------

/// Shared state across all frontends and conversations.
#[derive(Clone)]
pub struct BotCore {
    pub work_sender: mpsc::UnboundedSender<Work>,
    pub work_capacity: Arc<tokio::sync::Semaphore>,
    pub inflight: Arc<std::sync::Mutex<std::collections::HashSet<String>>>,
    pub authz: Option<Arc<Authorizer>>,
    pub response_policy: Arc<ResponsePolicyConfig>,
    pub acp_info: Arc<std::sync::Mutex<AcpInfo>>,
    /// Cross-task coordinator (Phase 4). Defaults to a per-process in-memory
    /// `NoopCoordinator`, which preserves the legacy single-task behaviour.
    /// `DynamoCoordinator` plugs in via this field when the runtime config
    /// asks for it.
    pub coordinator: Arc<dyn crate::engine::coordinator::Coordinator>,
    /// Process-local lease ownership shared by every clone of this core.
    pub lease_manager: LeaseManager,
    pub rate_limit: crate::config::RateLimitConfig,
}

impl BotCore {
    pub fn work_capacity(max_active_work_items: usize) -> Arc<tokio::sync::Semaphore> {
        Arc::new(tokio::sync::Semaphore::new(max_active_work_items))
    }
}

const FORWARD_ATTEMPTS: usize = 3;

fn try_admit_work(
    core: &BotCore,
    conversation_id: &str,
) -> std::result::Result<tokio::sync::OwnedSemaphorePermit, String> {
    if core.work_sender.is_closed() {
        tracing::error!(%conversation_id, "ACP control channel is closed");
        return Err("ACP runtime is unavailable".into());
    }
    match core.work_capacity.clone().try_acquire_owned() {
        Ok(permit) => Ok(permit),
        Err(_) => {
            core.acp_info.lock().unwrap().overload_rejections += 1;
            tracing::warn!(
                %conversation_id,
                "ACP work capacity exhausted"
            );
            Err("ACP work capacity exhausted".into())
        },
    }
}

fn try_enqueue_admitted_work(
    core: &BotCore,
    work: Work,
    permit: tokio::sync::OwnedSemaphorePermit,
) -> std::result::Result<tokio::sync::OwnedSemaphorePermit, (tokio::sync::OwnedSemaphorePermit, Box<Work>)> {
    let conversation_id = work.conversation_id().unwrap_or("runtime").to_string();
    match core.work_sender.send(work) {
        Ok(()) => Ok(permit),
        Err(error) => {
            tracing::error!(%conversation_id, "ACP control channel is closed");
            Err((permit, Box::new(error.0)))
        },
    }
}

fn enqueue_admitted_work(
    core: &BotCore,
    work: Work,
    permit: tokio::sync::OwnedSemaphorePermit,
) -> tokio::sync::OwnedSemaphorePermit {
    match try_enqueue_admitted_work(core, work, permit) {
        Ok(permit) => permit,
        Err((permit, work)) => {
            work.reject("Error: ACP runtime is unavailable");
            permit
        },
    }
}

fn enqueue_work(core: &BotCore, work: Work) -> Option<tokio::sync::OwnedSemaphorePermit> {
    let conversation_id = work.conversation_id().unwrap_or("runtime").to_string();
    match try_admit_work(core, &conversation_id) {
        Ok(permit) => Some(enqueue_admitted_work(core, work, permit)),
        Err(error) => {
            if error.contains("capacity") {
                work.reject("⏳ The bot is at capacity — try again shortly");
            } else {
                work.reject(format!("Error: {error}"));
            }
            None
        },
    }
}

pub(crate) async fn forward_with_retry(
    coordinator: &dyn crate::engine::coordinator::Coordinator,
    peer: &str,
    payload: &ForwardEvent,
) -> Result<()> {
    for attempt in 1..=FORWARD_ATTEMPTS {
        match coordinator.forward(peer, payload.clone()).await {
            Ok(()) => return Ok(()),
            Err(error) if attempt < FORWARD_ATTEMPTS => {
                let delay = std::time::Duration::from_millis(50 * attempt as u64);
                tracing::warn!(%error, peer, attempt, ?delay, "Retrying Slack event forwarding");
                tokio::time::sleep(delay).await;
            },
            Err(error) => return Err(error),
        }
    }
    unreachable!("forward retry loop always returns")
}

async fn wait_for_lease_loss(rx: &mut tokio::sync::watch::Receiver<Option<String>>) -> String {
    loop {
        if let Some(failure) = rx.borrow().clone() {
            return failure;
        }
        if rx.changed().await.is_err() {
            return "lease heartbeat stopped".into();
        }
    }
}

async fn stop_progress_updates(mut progress_handle: tokio::task::JoinHandle<()>) {
    let result = tokio::select! {
        result = &mut progress_handle => result,
        _ = tokio::time::sleep(Duration::from_secs(5)) => {
            progress_handle.abort();
            progress_handle.await
        },
    };
    match result {
        Ok(()) => {},
        Err(error) if error.is_cancelled() => {},
        Err(error) => tracing::warn!(%error, "progress update task failed"),
    }
}

async fn finish_progress_with_reply(
    frontend: &Arc<dyn Frontend>,
    platform_id: String,
    ack_id: String,
    reply_to: String,
    text: String,
    status: ProgressStatus,
    progress_handle: tokio::task::JoinHandle<()>,
) -> Result<()> {
    stop_progress_updates(progress_handle).await;
    frontend
        .send(Reply::FinishProgress {
            conversation: platform_id,
            message_id: ack_id,
            reply_to,
            text,
            status,
        })
        .await?;
    Ok(())
}

enum PromptWaitFailure {
    LeaseLost(String),
    ReplyChannelClosed,
}

async fn await_prompt_reply(
    core: &BotCore,
    session_key: &str,
    request_id: &str,
    reply_rx: &mut oneshot::Receiver<String>,
    lease_loss: Option<&mut tokio::sync::watch::Receiver<Option<String>>>,
) -> Result<String, PromptWaitFailure> {
    let Some(lease_loss) = lease_loss else {
        return reply_rx.await.map_err(|_| PromptWaitFailure::ReplyChannelClosed);
    };
    tokio::select! {
        reply = reply_rx => reply.map_err(|_| PromptWaitFailure::ReplyChannelClosed),
        failure = wait_for_lease_loss(lease_loss) => {
            tracing::error!(
                %request_id,
                conversation_id = %session_key,
                %failure,
                "active prompt cancelled after coordinator lease loss"
            );
            let (cancel_tx, cancel_rx) = oneshot::channel();
            let cancellation_confirmed = if core.work_sender.send(Work::CancelAndWait {
                conversation: session_key.to_string(),
                reply_tx: cancel_tx,
            }).is_ok() {
                matches!(
                    tokio::time::timeout(CANCEL_CONFIRM_TIMEOUT, cancel_rx).await,
                    Ok(Ok(()))
                )
            } else {
                tracing::error!(
                    %request_id,
                    conversation_id = %session_key,
                    "ACP control channel closed during lease-loss cancellation"
                );
                false
            };
            if !cancellation_confirmed {
                tracing::error!(
                    %request_id,
                    conversation_id = %session_key,
                    "ACP did not confirm cancellation after lease loss"
                );
                let mut info = core.acp_info.lock().unwrap();
                info.last_failure = Some(format!(
                    "lease-loss cancellation was not confirmed for {session_key}"
                ));
            }
            Err(PromptWaitFailure::LeaseLost(failure))
        }
    }
}

async fn report_lease_loss(frontend: &Arc<dyn Frontend>, platform_id: &str, ack_id: &str, reply_to: &str) {
    let _ = frontend
        .send(Reply::FinishProgress {
            conversation: platform_id.to_string(),
            message_id: ack_id.to_string(),
            reply_to: reply_to.to_string(),
            text: "Error: Coordination lease was lost, so this request was cancelled. Please retry.".into(),
            status: ProgressStatus::Error,
        })
        .await;
}

// ---------------------------------------------------------------------------
// Command resolution
// ---------------------------------------------------------------------------

/// Resolved action from a user message.
#[derive(Debug, PartialEq)]
pub enum Action {
    Prompt { text: String },
    Help,
    NewSession,
    SetAgent { name: String },
    SetModel { name: String },
    Cancel,
    Status,
    ListAgents,
    Unknown,
}

impl Action {
    /// Read-only commands that don't hold a worker or block other requests.
    fn is_readonly(&self) -> bool {
        matches!(self, Action::Help | Action::Status | Action::ListAgents)
    }

    fn requires_work_capacity(&self) -> bool {
        matches!(
            self,
            Action::Prompt { .. }
                | Action::NewSession
                | Action::SetAgent { .. }
                | Action::SetModel { .. }
                | Action::Status
        )
    }
}

/// Parse a message into an action. `!`-prefixed messages are bot commands;
/// everything else is a prompt to the agent.
pub fn resolve_action(text: &str) -> Action {
    match text.strip_prefix('!') {
        None => Action::Prompt { text: text.to_string() },
        Some(rest) => {
            let (cmd, args) = rest.split_once(' ').unwrap_or((rest, ""));
            let args = args.trim();
            match cmd {
                "" | "help" => Action::Help,
                "new" => Action::NewSession,
                "agent" if !args.is_empty() => Action::SetAgent { name: args.to_string() },
                "model" if !args.is_empty() => Action::SetModel { name: args.to_string() },
                "cancel" => Action::Cancel,
                "status" => Action::Status,
                "agents" => Action::ListAgents,
                _ => Action::Unknown,
            }
        },
    }
}

/// Determine where to send the reply based on response policy and thread state.
pub fn determine_reply_location(
    policy: &ResponsePolicyConfig,
    scope: &str,
    text: &str,
    reply_to: Option<&str>,
    msg_id: &str,
) -> Option<String> {
    if let Some(ts) = reply_to {
        return Some(ts.to_string());
    }
    if policy.force_thread(scope, text) {
        return Some(msg_id.to_string());
    }
    match policy.reply_location(scope) {
        Location::Thread => Some(msg_id.to_string()),
        Location::Same | Location::Dm => None,
    }
}

fn check_authz(authz: &Option<Arc<Authorizer>>, check_fn: impl FnOnce(&Authorizer) -> Result<bool>) -> Result<bool> {
    match authz.as_ref() {
        Some(a) => check_fn(a),
        None => Ok(true),
    }
}

// ---------------------------------------------------------------------------
// Dispatch
// ---------------------------------------------------------------------------

/// Releases the cluster lease and inflight slot on every dispatch exit.
struct DispatchGuard {
    inflight: Arc<std::sync::Mutex<std::collections::HashSet<String>>>,
    conv_id: String,
    holds_inflight: bool,
    lease: Option<LeaseGuard>,
}

struct DedupeReservation {
    event_id: String,
    token: DedupeToken,
}

struct PromptAdmission {
    input_tx: oneshot::Sender<PromptPayload>,
    reply_rx: oneshot::Receiver<String>,
    progress_tx: mpsc::UnboundedSender<ProgressUpdate>,
    progress_rx: mpsc::UnboundedReceiver<ProgressUpdate>,
    work_permit: tokio::sync::OwnedSemaphorePermit,
}

async fn release_dedup_reservation(
    coordinator: &dyn crate::engine::coordinator::Coordinator,
    reservation: Option<DedupeReservation>,
) {
    let Some(reservation) = reservation else {
        return;
    };
    if let Err(error) = coordinator
        .release_dedup(&reservation.event_id, &reservation.token)
        .await
    {
        tracing::error!(
            %error,
            event_id = %reservation.event_id,
            "Failed to release rejected event dedup reservation"
        );
    }
}

impl Drop for DispatchGuard {
    fn drop(&mut self) {
        self.lease.take();
        if self.holds_inflight {
            self.inflight.lock().unwrap().remove(&self.conv_id);
        }
    }
}

/// Dispatch an incoming message to the appropriate handler.
///
/// For prompts, spawns an async task that sends a progress placeholder,
/// routes the message to the ACP pool, and replaces the placeholder with
/// the agent's response.
///
/// In a multi-task fleet, every Slack delivery fans to all tasks. To keep
/// the user from seeing duplicate replies, this function arbitrates the
/// event cluster-wide via [`crate::engine::coordinator::Coordinator`].
pub fn dispatch(core: &BotCore, msg: IncomingMessage, frontend: Arc<dyn Frontend>) {
    drop(dispatch_inner(core, msg, frontend, false));
}

/// Dispatch an event and report when routing has been accepted locally,
/// accepted by a peer, or recognized as a previously accepted duplicate.
pub fn dispatch_with_receipt(core: &BotCore, msg: IncomingMessage, frontend: Arc<dyn Frontend>) -> DispatchReceipt {
    dispatch_inner(core, msg, frontend, true)
}

fn dispatch_inner(
    core: &BotCore,
    msg: IncomingMessage,
    frontend: Arc<dyn Frontend>,
    receipt_required: bool,
) -> DispatchReceipt {
    let (routing_tx, routing_rx) = oneshot::channel();
    let conversation = msg.conversation.clone();
    let conv_id = conversation.id();
    let platform_id = conversation.platform_id().to_string();
    let reply_to = msg.reply_to.clone();
    let session_key = conv_id.clone();

    let action = resolve_action(&msg.text);

    let core = core.clone();
    let envelope = msg.envelope.clone();

    tokio::spawn(async move {
        let request_id = envelope
            .as_ref()
            .map(|env| env.event_id.clone())
            .unwrap_or_else(|| format!("local:{conv_id}"));

        // Route before committing dedup so Slack can retry failed forwarding.
        let mut lease = None;
        let mut dedup = None;
        if let Some(ref env) = envelope {
            let acquisition = core.lease_manager.acquire(&conv_id, &request_id).await;
            if receipt_required && routing_tx.is_closed() {
                drop(acquisition);
                return;
            }
            match acquisition {
                ManagedLeaseAcquisition::Acquired(guard) => {
                    lease = Some(guard);
                    match core.coordinator.dedupe_event_outcome(&env.event_id).await {
                        DedupeOutcome::Accepted { token } => {
                            dedup = Some(DedupeReservation {
                                event_id: env.event_id.clone(),
                                token,
                            });
                        },
                        DedupeOutcome::Duplicate => {
                            tracing::debug!(
                                request_id = %request_id,
                                conversation_id = %conv_id,
                                "duplicate event, dropping"
                            );
                            let _ = routing_tx.send(Ok(DispatchRouting::Duplicate));
                            return;
                        },
                        DedupeOutcome::Unavailable => {
                            tracing::warn!(
                                target: "kiro_bot::coordinator",
                                %request_id,
                                event_id = %env.event_id,
                                conversation_id = %conv_id,
                                "coordinator unavailable while recording event dedup"
                            );
                            let _ =
                                routing_tx.send(Err("coordinator unavailable while recording event dedup".to_string()));
                            return;
                        },
                    }
                },
                ManagedLeaseAcquisition::Held { peer } if env.forwarded => {
                    tracing::warn!(
                        %request_id,
                        %peer,
                        conversation_id = %conv_id,
                        "forwarded event reached a task that does not own the conversation"
                    );
                    let _ = routing_tx.send(Err(format!(
                        "forwarded event reached non-owner; conversation is held by {peer}"
                    )));
                    return;
                },
                ManagedLeaseAcquisition::Held { peer } => {
                    if receipt_required && routing_tx.is_closed() {
                        return;
                    }
                    let payload = ForwardEvent {
                        slack_event_json: env.raw.clone(),
                    };
                    let forwarded = forward_with_retry(core.coordinator.as_ref(), &peer, &payload).await;
                    match forwarded {
                        Ok(()) => {
                            tracing::debug!(
                                %request_id,
                                %peer,
                                conversation_id = %conv_id,
                                "forwarded to lease holder"
                            );
                            let _ = routing_tx.send(Ok(DispatchRouting::Forwarded));
                            return;
                        },
                        Err(e) => {
                            tracing::warn!(
                                %request_id,
                                error = %e,
                                %peer,
                                conversation_id = %conv_id,
                                "forward failed; leaving lease with its current owner"
                            );
                            let _ = routing_tx.send(Err(format!("forward to {peer} failed: {e}")));
                            return;
                        },
                    }
                },
                ManagedLeaseAcquisition::Unavailable => {
                    tracing::warn!(
                        target: "kiro_bot::coordinator",
                        %request_id,
                        event_id = %env.event_id,
                        conversation_id = %conv_id,
                        "coordinator unavailable while routing event"
                    );
                    let _ = routing_tx.send(Err("coordinator unavailable while acquiring lease".to_string()));
                    return;
                },
            }
        } else if receipt_required && routing_tx.is_closed() {
            return;
        }

        if receipt_required && routing_tx.is_closed() {
            release_dedup_reservation(core.coordinator.as_ref(), dedup).await;
            return;
        }

        // Inflight guard. Read-only commands skip it.
        let mut holds_inflight = false;
        if !action.is_readonly() && !matches!(action, Action::Cancel) {
            if !core.inflight.lock().unwrap().insert(session_key.clone()) {
                let _ = routing_tx.send(Ok(DispatchRouting::Local));
                let _ = frontend
                    .send(Reply::Send {
                        conversation: platform_id.clone(),
                        reply_to: reply_to.clone(),
                        text: "⏳ A request is already in progress — this message was dropped".into(),
                    })
                    .await;
                let _guard = DispatchGuard {
                    inflight: core.inflight.clone(),
                    conv_id: conv_id.clone(),
                    holds_inflight: false,
                    lease,
                };
                drop(_guard);
                return;
            }
            holds_inflight = true;
        }

        // From here on, _guard releases the lease and the inflight slot on
        // every exit path (success, panic, early return).
        let _guard = DispatchGuard {
            inflight: core.inflight.clone(),
            conv_id: conv_id.clone(),
            holds_inflight,
            lease,
        };
        let lease_loss = _guard.lease.as_ref().map(LeaseGuard::subscribe_loss);

        let mut work_permit = if action.requires_work_capacity() {
            match try_admit_work(&core, &session_key) {
                Ok(permit) => Some(permit),
                Err(error) => {
                    release_dedup_reservation(core.coordinator.as_ref(), dedup).await;
                    let _ = routing_tx.send(Err(error.clone()));
                    if !receipt_required {
                        let _ = frontend
                            .send(Reply::Send {
                                conversation: platform_id,
                                reply_to,
                                text: format!("Error: {error}"),
                            })
                            .await;
                    }
                    return;
                },
            }
        } else {
            None
        };

        let prompt_reply = if matches!(action, Action::Prompt { .. }) {
            let authorized = match core
                .authz
                .as_ref()
                .map(|authz| authz.can_use_bot(&msg.user, &conversation))
                .transpose()
            {
                Ok(Some(true)) | Ok(None) => true,
                Ok(Some(false)) | Err(_) => {
                    info!("Bot access denied for user {} in {}", msg.user, conv_id);
                    false
                },
            };
            if !authorized {
                Some(format!("❌ Access denied for user `{}` in `{}`", msg.user, conv_id))
            } else {
                match crate::engine::rate_limit::check(core.coordinator.as_ref(), &core.rate_limit, &msg.slack_user_id)
                    .await
                {
                    Ok(RateLimitOutcome::Allowed) => None,
                    Ok(RateLimitOutcome::Limited { retry_after }) => {
                        Some(crate::engine::rate_limit::notice(retry_after))
                    },
                    Err(error) => {
                        tracing::warn!(
                            %error,
                            user_id = %msg.slack_user_id,
                            "fleet rate-limit admission failed"
                        );
                        release_dedup_reservation(core.coordinator.as_ref(), dedup).await;
                        let _ = routing_tx.send(Err(format!("prompt rate admission failed: {error}")));
                        return;
                    },
                }
            }
        } else {
            None
        };

        if let Some(text) = prompt_reply {
            match frontend
                .send(Reply::Send {
                    conversation: platform_id,
                    reply_to,
                    text,
                })
                .await
            {
                Ok(_) => {
                    let _ = routing_tx.send(Ok(DispatchRouting::Local));
                },
                Err(error) => {
                    tracing::error!(
                        %error,
                        %request_id,
                        conversation_id = %session_key,
                        "Prompt policy reply delivery failed"
                    );
                    release_dedup_reservation(core.coordinator.as_ref(), dedup).await;
                    let _ = routing_tx.send(Err(format!("prompt policy reply delivery failed: {error}")));
                },
            }
            return;
        }

        let prompt_admission = if matches!(action, Action::Prompt { .. }) {
            let (input_tx, input_rx) = oneshot::channel();
            let (reply_tx, reply_rx) = oneshot::channel();
            let (progress_tx, progress_rx) = mpsc::unbounded_channel();
            let permit = work_permit.take().expect("prompt actions reserve work capacity");
            match try_enqueue_admitted_work(
                &core,
                Work::Prompt {
                    input: PromptInput::Deferred(input_rx),
                    conversation: session_key.clone(),
                    reply_tx,
                    progress_tx: progress_tx.clone(),
                },
                permit,
            ) {
                Ok(work_permit) => Some(PromptAdmission {
                    input_tx,
                    reply_rx,
                    progress_tx,
                    progress_rx,
                    work_permit,
                }),
                Err((_permit, _work)) => {
                    release_dedup_reservation(core.coordinator.as_ref(), dedup).await;
                    let _ = routing_tx.send(Err("ACP runtime is unavailable".into()));
                    return;
                },
            }
        } else {
            None
        };
        let _ = routing_tx.send(Ok(DispatchRouting::Local));
        run_action(
            action,
            msg,
            conversation,
            conv_id,
            platform_id,
            reply_to,
            session_key,
            request_id,
            lease_loss,
            prompt_admission,
            work_permit,
            core,
            frontend,
        )
        .await;
    });
    routing_rx
}

/// Runs an action after dispatch has acquired its local and cluster guards.
#[allow(clippy::too_many_arguments)]
async fn run_action(
    action: Action,
    mut msg: IncomingMessage,
    conversation: Conversation,
    conv_id: String,
    platform_id: String,
    reply_to: Option<String>,
    session_key: String,
    request_id: String,
    mut lease_loss: Option<tokio::sync::watch::Receiver<Option<String>>>,
    prompt_admission: Option<PromptAdmission>,
    mut work_permit: Option<tokio::sync::OwnedSemaphorePermit>,
    core: BotCore,
    frontend: Arc<dyn Frontend>,
) {
    let authz_scope = conversation.authz_id();
    match action {
        Action::Prompt { mut text } => {
            let PromptAdmission {
                input_tx,
                mut reply_rx,
                progress_tx,
                mut progress_rx,
                work_permit,
            } = prompt_admission.expect("accepted prompts carry queued work");
            let transcript_prompt = text.clone();

            let stream_thread = reply_to
                .clone()
                .or_else(|| msg.source_message_id.clone())
                .unwrap_or_else(|| request_id.clone());
            // Slack streams must reply to a user message, including policies that otherwise reply inline.
            let ack_id = match frontend
                .send(Reply::StartProgress {
                    conversation: platform_id.clone(),
                    reply_to: stream_thread.clone(),
                    recipient_user_id: msg.slack_user_id.clone(),
                    recipient_team_id: msg.slack_team_id.clone(),
                })
                .await
            {
                Ok(id) => Some(id),
                Err(e) => {
                    tracing::error!(%e, %request_id, "progress stream delivery failed; continuing with direct reply");
                    None
                },
            };

            if let Err(error) = core
                .coordinator
                .append_turn(&session_key, Turn {
                    role: TurnRole::User,
                    text: transcript_prompt,
                    ts: chrono::Utc::now(),
                    chunk_ids: Vec::new(),
                })
                .await
            {
                tracing::error!(%error, conversation = %session_key, "Failed to append user transcript turn");
            }

            if let Some(preparation) = msg.prompt_preparation.take() {
                preparation.prepare(&mut text).await;
            }

            // Capture a copy of the user prompt before move so we can run
            // the post-reply retrieval check without re-fetching it.
            let prompt_for_check = text.clone();

            let (approval_channel, approval_thread) = match &conversation {
                Conversation::Dm { channel, .. } => (channel.clone(), reply_to.clone()),
                Conversation::Channel(id) => (id.clone(), reply_to.clone()),
                Conversation::Thread { channel, thread_ts } => (channel.clone(), Some(thread_ts.clone())),
            };
            if input_tx
                .send(PromptPayload {
                    text,
                    context: msg.context,
                    channel: approval_channel,
                    thread_ts: approval_thread,
                    user: msg.user.clone(),
                    slack_user_id: msg.slack_user_id.clone(),
                })
                .is_err()
            {
                tracing::error!(%request_id, conversation_id = %session_key, "queued ACP prompt closed before preparation completed");
            }

            let progress_handle = {
                let frontend2 = frontend.clone();
                let conv = platform_id.clone();
                let ack = ack_id.clone();
                tokio::spawn(async move {
                    while let Some(update) = progress_rx.recv().await {
                        let Some(ack) = ack.as_ref() else {
                            continue;
                        };
                        if let Err(error) = frontend2
                            .send(Reply::Progress {
                                conversation: conv.clone(),
                                message_id: ack.clone(),
                                update,
                            })
                            .await
                        {
                            tracing::warn!(
                                %error,
                                conversation_id = %conv,
                                message_id = %ack,
                                "Slack progress update failed"
                            );
                        }
                    }
                })
            };

            let reply_text =
                match await_prompt_reply(&core, &session_key, &request_id, &mut reply_rx, lease_loss.as_mut()).await {
                    Ok(reply) => reply,
                    Err(PromptWaitFailure::ReplyChannelClosed) => "Error".into(),
                    Err(PromptWaitFailure::LeaseLost(failure)) => {
                        tracing::debug!(%request_id, conversation_id = %session_key, %failure);
                        drop(progress_tx);
                        stop_progress_updates(progress_handle).await;
                        drop(work_permit);
                        if let Some(ack_id) = ack_id.as_deref() {
                            report_lease_loss(&frontend, &platform_id, ack_id, &stream_thread).await;
                        } else {
                            let _ = frontend
                            .send(Reply::Send {
                                conversation: platform_id,
                                reply_to: Some(stream_thread),
                                text:
                                    "Error: Coordination lease was lost, so this request was cancelled. Please retry."
                                        .into(),
                            })
                            .await;
                        }
                        return;
                    },
                };
            drop(work_permit);

            // Post-reply retrieval check. If the model answered a
            // kiro-shaped question without citing, log a structured
            // warning AND inject a coercive retry through the same ACP
            // session asking it to redo the answer with retrieval. The
            // retry takes the place of the original reply so the user
            // never sees the un-cited draft.
            use crate::engine::retrieval_check::{
                RetrievalCheck,
                check,
            };
            let final_reply_text = match check(&prompt_for_check, &reply_text) {
                RetrievalCheck::MissingCitation => {
                    tracing::warn!(
                        target: "retrieval_check",
                        conversation = %session_key,
                        user = %msg.user,
                        prompt_preview = %prompt_for_check.chars().take(120).collect::<String>(),
                        "model answered a kiro-related question without citing — issuing retry"
                    );
                    // Coercive retry. Don't repeat the user's question —
                    // the ACP session retains its own history. Just give
                    // the model a one-line procedural correction.
                    let retry_prompt = "[system retry] You answered the previous question \
                        without calling search_kiro_knowledge. That violates the workflow. \
                        Call search_kiro_knowledge now with a focused query, then re-answer \
                        the original question and end with a `Sources:` line citing the \
                        retrieved chunk paths. Do not apologize or explain — just produce \
                        the corrected answer.";
                    let (retry_tx, mut retry_rx) = oneshot::channel();
                    let retry_permit = enqueue_work(&core, Work::Prompt {
                        input: PromptInput::ready(
                            retry_prompt.to_string(),
                            Vec::new(),
                            match &conversation {
                                Conversation::Dm { channel, .. } => channel.clone(),
                                Conversation::Channel(id) => id.clone(),
                                Conversation::Thread { channel, .. } => channel.clone(),
                            },
                            match &conversation {
                                Conversation::Thread { thread_ts, .. } => Some(thread_ts.clone()),
                                _ => reply_to.clone(),
                            },
                            msg.user.clone(),
                            msg.slack_user_id.clone(),
                        ),
                        conversation: session_key.clone(),
                        reply_tx: retry_tx,
                        progress_tx: progress_tx.clone(),
                    });
                    let retry_result =
                        await_prompt_reply(&core, &session_key, &request_id, &mut retry_rx, lease_loss.as_mut()).await;
                    drop(retry_permit);
                    match retry_result {
                        Ok(retried) => {
                            if matches!(check(&prompt_for_check, &retried), RetrievalCheck::Cited) {
                                retried
                            } else {
                                tracing::warn!(
                                    target: "retrieval_check",
                                    conversation = %session_key,
                                    "retry also lacked a citation — sending original answer with a soft note"
                                );
                                format!(
                                    "{reply_text}\n\n_(I answered from training; if this should be grounded in our docs, ask me to cite a source.)_"
                                )
                            }
                        },
                        Err(PromptWaitFailure::ReplyChannelClosed) => {
                            tracing::error!(target: "retrieval_check", "retry channel closed");
                            reply_text
                        },
                        Err(PromptWaitFailure::LeaseLost(failure)) => {
                            tracing::debug!(%request_id, conversation_id = %session_key, %failure);
                            drop(progress_tx);
                            stop_progress_updates(progress_handle).await;
                            if let Some(ack_id) = ack_id.as_deref() {
                                report_lease_loss(&frontend, &platform_id, ack_id, &stream_thread).await;
                            } else {
                                let _ = frontend
                                    .send(Reply::Send {
                                        conversation: platform_id,
                                        reply_to: Some(stream_thread),
                                        text:
                                            "Error: Coordination lease was lost, so this request was cancelled. Please retry."
                                                .into(),
                                    })
                                    .await;
                            }
                            return;
                        },
                    }
                },
                _ => reply_text,
            };
            let response_status = progress_status_for_reply(&final_reply_text);

            drop(progress_tx);
            let final_reply = with_genai_disclaimer(&final_reply_text);
            let delivery = if let Some(ack_id) = ack_id {
                finish_progress_with_reply(
                    &frontend,
                    platform_id,
                    ack_id,
                    stream_thread,
                    final_reply,
                    response_status,
                    progress_handle,
                )
                .await
            } else {
                stop_progress_updates(progress_handle).await;
                frontend
                    .send(Reply::Send {
                        conversation: platform_id,
                        reply_to: Some(stream_thread),
                        text: final_reply,
                    })
                    .await
                    .map(|_| ())
            };
            if let Err(error) = delivery {
                tracing::error!(
                    %error,
                    %request_id,
                    conversation_id = %session_key,
                    "Final response delivery failed after retries"
                );
                return;
            }

            let chunk_ids = crate::engine::feedback::extract_cited_sources(&final_reply_text);
            if let Err(error) = core
                .coordinator
                .append_turn(&session_key, Turn {
                    role: TurnRole::Assistant,
                    text: final_reply_text,
                    ts: chrono::Utc::now(),
                    chunk_ids,
                })
                .await
            {
                tracing::warn!(%error, conversation = %session_key, "Failed to append assistant transcript turn");
            }
        },
        action => {
            let user = msg.user.clone();
            match check_authz(&core.authz, |a| a.can_use_bot(&user, &conversation)) {
                Ok(true) => {},
                _ => {
                    let _ = frontend
                        .send(Reply::Send {
                            conversation: platform_id.clone(),
                            reply_to: reply_to.clone(),
                            text: format!("❌ Access denied for user `{}` in `{}`", user, conv_id),
                        })
                        .await;
                    return;
                },
            }

            let send = |text: String| {
                let f = frontend.clone();
                let conv = platform_id.clone();
                let rt = reply_to.clone();
                async move {
                    let _ = f
                        .send(Reply::Send {
                            conversation: conv,
                            reply_to: rt,
                            text,
                        })
                        .await;
                }
            };

            match action {
                Action::Help => {
                    send(
                        concat!(
                            "I answer questions about Kiro CLI using generative AI.\n\n",
                            "*Commands:*\n",
                            "`!help` — show this message\n",
                            "`!new` — new session\n",
                            "`!agent <name>` — switch agent\n",
                            "`!model <name>` — switch model\n",
                            "`!status` — current agent/model/session\n",
                            "`!agents` — list available agents\n",
                            "`!cancel` — cancel current request\n\n",
                            "_AI-generated — may be incorrect or incomplete. Verify before acting on it._",
                        )
                        .into(),
                    )
                    .await;
                },
                Action::NewSession => {
                    let (tx, rx) = oneshot::channel();
                    let permit = enqueue_admitted_work(
                        &core,
                        Work::NewSession {
                            conversation: session_key.clone(),
                            reply_tx: tx,
                        },
                        work_permit.take().expect("new-session actions reserve work capacity"),
                    );
                    if let Ok(m) = rx.await {
                        send(m).await;
                    }
                    drop(permit);
                },
                Action::SetAgent { name } => {
                    match check_authz(&core.authz, |a| a.can_use_agent(&user, &name, &authz_scope)) {
                        Ok(true) => {
                            let (tx, rx) = oneshot::channel();
                            let permit = enqueue_admitted_work(
                                &core,
                                Work::SetMode {
                                    conversation: session_key.clone(),
                                    mode: name,
                                    reply_tx: tx,
                                },
                                work_permit.take().expect("set-agent actions reserve work capacity"),
                            );
                            if let Ok(m) = rx.await {
                                send(m).await;
                            }
                            drop(permit);
                        },
                        Ok(false) => send(format!("❌ Unauthorized: You don't have access to agent '{name}'")).await,
                        Err(e) => send(format!("❌ Authorization error: {e}")).await,
                    }
                },
                Action::SetModel { name } => match check_authz(&core.authz, |a| a.can_use_model(&user, &name)) {
                    Ok(true) => {
                        let (tx, rx) = oneshot::channel();
                        let permit = enqueue_admitted_work(
                            &core,
                            Work::SetModel {
                                conversation: session_key.clone(),
                                model: name,
                                reply_tx: tx,
                            },
                            work_permit.take().expect("set-model actions reserve work capacity"),
                        );
                        if let Ok(m) = rx.await {
                            send(m).await;
                        }
                        drop(permit);
                    },
                    Ok(false) => send(format!("❌ Unauthorized: You don't have access to model '{name}'")).await,
                    Err(e) => send(format!("❌ Authorization error: {e}")).await,
                },
                Action::Status => {
                    let (tx, rx) = oneshot::channel();
                    let permit = enqueue_admitted_work(
                        &core,
                        Work::Status {
                            conversation: session_key.clone(),
                            reply_tx: tx,
                        },
                        work_permit.take().expect("status actions reserve work capacity"),
                    );
                    if let Ok(m) = rx.await {
                        send(m).await;
                    }
                    drop(permit);
                },
                Action::ListAgents => {
                    let agents: Vec<String> = {
                        let info = core.acp_info.lock().unwrap();
                        info.available_modes
                            .iter()
                            .filter(|m| {
                                check_authz(&core.authz, |a| a.can_use_agent(&user, &m.id, &authz_scope))
                                    .unwrap_or(false)
                            })
                            .map(|m| match &m.description {
                                Some(d) => format!("• `{}` ({}) — {}", m.id, m.name, d),
                                None => format!("• `{}` ({})", m.id, m.name),
                            })
                            .collect()
                    };
                    send(if agents.is_empty() {
                        "No agents available".into()
                    } else {
                        format!("*Available agents:*\n{}", agents.join("\n"))
                    })
                    .await;
                },
                Action::Cancel => {
                    if core
                        .work_sender
                        .send(Work::Cancel {
                            conversation: session_key,
                        })
                        .is_err()
                    {
                        tracing::error!(conversation_id = %conv_id, "ACP control channel is closed");
                    } else {
                        send("🛑".into()).await;
                    }
                },
                Action::Unknown => {},
                Action::Prompt { .. } => unreachable!(),
            }
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct FailOnceFrontend {
        attempts: std::sync::atomic::AtomicUsize,
        delivered: std::sync::Mutex<Vec<String>>,
    }

    #[async_trait::async_trait]
    impl Frontend for FailOnceFrontend {
        async fn send(&self, reply: Reply) -> Result<String> {
            let Reply::Send { text, .. } = reply else {
                panic!("policy replies must use Reply::Send");
            };
            let attempt = self.attempts.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            self.delivered.lock().unwrap().push(text);
            if attempt == 0 {
                anyhow::bail!("terminal Slack delivery failure");
            }
            Ok("sent".into())
        }

        async fn fetch_context(&self, _: &str, _: &str, _: Option<&str>) -> Vec<String> {
            Vec::new()
        }
    }

    fn policy_reply_core(
        coordinator: Arc<crate::engine::coordinator::NoopCoordinator>,
        authz: Option<Arc<Authorizer>>,
    ) -> (BotCore, mpsc::UnboundedReceiver<Work>) {
        let (work_sender, work_receiver) = mpsc::unbounded_channel();
        let coordinator_dyn: Arc<dyn crate::engine::coordinator::Coordinator> = coordinator;
        (
            BotCore {
                work_sender,
                work_capacity: BotCore::work_capacity(1),
                inflight: Arc::new(std::sync::Mutex::new(std::collections::HashSet::new())),
                authz,
                response_policy: Arc::new(ResponsePolicyConfig::default_policy()),
                acp_info: Arc::new(std::sync::Mutex::new(AcpInfo::default())),
                coordinator: coordinator_dyn.clone(),
                lease_manager: LeaseManager::new(coordinator_dyn),
                rate_limit: crate::config::RateLimitConfig {
                    max_prompts: 1,
                    window_secs: 60,
                },
            },
            work_receiver,
        )
    }

    fn policy_reply_message(event_id: &str, channel: &str) -> IncomingMessage {
        IncomingMessage {
            user: "test-user".into(),
            slack_user_id: "U_TEST".into(),
            slack_team_id: "T_TEST".into(),
            source_message_id: Some("171.001".into()),
            text: "hello".into(),
            conversation: Conversation::Channel(channel.into()),
            reply_to: Some("171.001".into()),
            directed: true,
            context: Vec::new(),
            prompt_preparation: None,
            envelope: Some(DispatchEnvelope {
                event_id: event_id.into(),
                raw: serde_json::json!({"event_id": event_id}),
                forwarded: false,
            }),
        }
    }

    async fn wait_for_dispatch_exit(core: &BotCore) {
        for _ in 0..10 {
            if core.inflight.lock().unwrap().is_empty() {
                return;
            }
            tokio::task::yield_now().await;
        }
        panic!("dispatch did not release its inflight guard");
    }

    async fn assert_failed_policy_reply_is_retryable(
        core: &BotCore,
        message: impl Fn() -> IncomingMessage,
    ) -> Vec<String> {
        let frontend = Arc::new(FailOnceFrontend {
            attempts: std::sync::atomic::AtomicUsize::new(0),
            delivered: std::sync::Mutex::new(Vec::new()),
        });
        let first = dispatch_with_receipt(core, message(), frontend.clone())
            .await
            .expect("first dispatch receipt");
        assert!(
            matches!(first, Err(ref error) if error.contains("terminal Slack delivery failure")),
            "terminal delivery failure must be surfaced, got {first:?}"
        );
        wait_for_dispatch_exit(core).await;

        assert_eq!(
            dispatch_with_receipt(core, message(), frontend.clone())
                .await
                .expect("retry dispatch receipt"),
            Ok(DispatchRouting::Local)
        );
        wait_for_dispatch_exit(core).await;
        frontend.delivered.lock().unwrap().clone()
    }

    #[derive(Default)]
    struct BlockingProgressFrontend {
        events: std::sync::Mutex<Vec<&'static str>>,
        update_active: std::sync::atomic::AtomicBool,
        update_started: tokio::sync::Notify,
    }

    struct UpdateActiveGuard<'a> {
        frontend: &'a BlockingProgressFrontend,
    }

    impl Drop for UpdateActiveGuard<'_> {
        fn drop(&mut self) {
            self.frontend
                .update_active
                .store(false, std::sync::atomic::Ordering::SeqCst);
            self.frontend.events.lock().unwrap().push("update-finished");
        }
    }

    #[async_trait::async_trait]
    impl Frontend for BlockingProgressFrontend {
        async fn send(&self, reply: Reply) -> Result<String> {
            match reply {
                Reply::Progress { .. } => {
                    self.update_active.store(true, std::sync::atomic::Ordering::SeqCst);
                    self.events.lock().unwrap().push("update-started");
                    self.update_started.notify_one();
                    let _guard = UpdateActiveGuard { frontend: self };
                    std::future::pending::<()>().await;
                    unreachable!()
                },
                Reply::FinishProgress { .. } => {
                    assert!(!self.update_active.load(std::sync::atomic::Ordering::SeqCst));
                    self.events.lock().unwrap().push("finish");
                    Ok("finished".into())
                },
                Reply::Send { .. } | Reply::StartProgress { .. } | Reply::Update { .. } | Reply::Delete { .. } => {
                    assert!(!self.update_active.load(std::sync::atomic::Ordering::SeqCst));
                    self.events.lock().unwrap().push("send");
                    Ok("sent".into())
                },
            }
        }

        async fn fetch_context(&self, _: &str, _: &str, _: Option<&str>) -> Vec<String> {
            Vec::new()
        }
    }

    #[tokio::test]
    async fn progress_update_finishes_before_stream_finalization() {
        let frontend = Arc::new(BlockingProgressFrontend::default());
        let frontend_dyn: Arc<dyn Frontend> = frontend.clone();
        let (progress_tx, mut progress_rx) = mpsc::unbounded_channel();
        let progress_frontend = frontend_dyn.clone();
        let progress_handle = tokio::spawn(async move {
            while let Some(update) = progress_rx.recv().await {
                let _ = progress_frontend
                    .send(Reply::Progress {
                        conversation: "channel".into(),
                        message_id: "ack".into(),
                        update,
                    })
                    .await;
            }
        });
        progress_tx
            .send(ProgressUpdate {
                id: "tool".into(),
                title: "Working".into(),
                status: crate::engine::acp::ProgressStatus::InProgress,
            })
            .unwrap();
        frontend.update_started.notified().await;

        finish_progress_with_reply(
            &frontend_dyn,
            "channel".into(),
            "ack".into(),
            "root".into(),
            "done".into(),
            ProgressStatus::Complete,
            progress_handle,
        )
        .await
        .unwrap();

        assert_eq!(*frontend.events.lock().unwrap(), [
            "update-started",
            "update-finished",
            "finish"
        ]);
    }

    #[tokio::test]
    async fn access_denied_reply_delivery_failure_is_surfaced_and_retryable() {
        let coordinator = Arc::new(crate::engine::coordinator::NoopCoordinator::new());
        let policy_path = concat!(env!("CARGO_MANIFEST_DIR"), "/kiro-help/policies/agents.cedar");
        let (core, _work_receiver) = policy_reply_core(
            coordinator,
            Some(Arc::new(
                Authorizer::new(policy_path, None, None).expect("load test policy"),
            )),
        );
        assert!(
            !core
                .authz
                .as_ref()
                .unwrap()
                .can_use_bot("test-user", &Conversation::Channel("C_NOT_ALLOWED".into()))
                .unwrap()
        );

        let delivered =
            assert_failed_policy_reply_is_retryable(&core, || policy_reply_message("EvDenied", "C_NOT_ALLOWED")).await;

        assert_eq!(delivered.len(), 2);
        assert!(delivered.iter().all(|text| text.contains("Access denied")));
    }

    #[tokio::test]
    async fn rate_limit_reply_delivery_failure_is_surfaced_and_retryable() {
        use crate::engine::coordinator::Coordinator;

        let coordinator = Arc::new(crate::engine::coordinator::NoopCoordinator::new());
        assert_eq!(
            coordinator
                .admit_prompt("U_TEST", 1, std::time::Duration::from_secs(60))
                .await
                .unwrap(),
            RateLimitOutcome::Allowed
        );
        assert!(matches!(
            coordinator
                .admit_prompt("U_TEST", 1, std::time::Duration::from_secs(60))
                .await
                .unwrap(),
            RateLimitOutcome::Limited { .. }
        ));
        let (core, _work_receiver) = policy_reply_core(coordinator, None);

        let delivered =
            assert_failed_policy_reply_is_retryable(&core, || policy_reply_message("EvLimited", "C_ALLOWED")).await;

        assert_eq!(delivered.len(), 2);
        assert!(delivered.iter().all(|text| text.contains("try again")));
    }

    #[test]
    fn plain_text_is_prompt() {
        assert_eq!(resolve_action("hello"), Action::Prompt { text: "hello".into() });
    }

    #[test]
    fn genai_disclaimer_is_appended_once() {
        let once = with_genai_disclaimer("The answer is 42.");
        assert!(once.starts_with("The answer is 42."));
        assert!(once.ends_with(GENAI_DISCLAIMER));
        assert_eq!(
            with_genai_disclaimer(&once),
            once,
            "re-wrapping must not stack duplicate disclaimers"
        );
        assert!(
            with_genai_disclaimer("  trailing space  ").ends_with(GENAI_DISCLAIMER),
            "trailing whitespace must not defeat the already-present check"
        );
    }

    #[test]
    fn failed_and_cancelled_replies_finish_as_errors() {
        assert_eq!(progress_status_for_reply("Error: timed out"), ProgressStatus::Error);
        assert_eq!(progress_status_for_reply("❌ Cancelled"), ProgressStatus::Error);
        assert_eq!(progress_status_for_reply("Done"), ProgressStatus::Complete);
    }

    #[test]
    fn help_command() {
        assert_eq!(resolve_action("!help"), Action::Help);
    }

    #[test]
    fn new_command() {
        assert_eq!(resolve_action("!new"), Action::NewSession);
    }

    #[test]
    fn agent_command_with_name() {
        assert_eq!(resolve_action("!agent code-reviewer"), Action::SetAgent {
            name: "code-reviewer".into()
        });
    }

    #[test]
    fn agent_command_without_name_is_unknown() {
        assert_eq!(resolve_action("!agent"), Action::Unknown);
    }

    #[test]
    fn cancel_command() {
        assert_eq!(resolve_action("!cancel"), Action::Cancel);
    }

    #[test]
    fn status_command() {
        assert_eq!(resolve_action("!status"), Action::Status);
    }

    #[test]
    fn agents_command() {
        assert_eq!(resolve_action("!agents"), Action::ListAgents);
    }

    #[test]
    fn unknown_command() {
        assert_eq!(resolve_action("!foo"), Action::Unknown);
    }

    #[test]
    fn thread_authz_inherits_channel() {
        let c = Conversation::Thread {
            channel: "C123".into(),
            thread_ts: "111.222".into(),
        };
        assert_eq!(c.authz_id(), "channel:C123");
    }

    #[test]
    fn dm_roots_isolate_sessions_but_share_authorization() {
        let first = Conversation::Dm {
            channel: "D000".into(),
            user: "alice".into(),
            thread_ts: "111.222".into(),
        };
        let second = Conversation::Dm {
            channel: "D000".into(),
            user: "alice".into(),
            thread_ts: "333.444".into(),
        };

        assert_eq!(first.id(), "thread:D000:111.222");
        assert_eq!(second.id(), "thread:D000:333.444");
        assert_ne!(first.id(), second.id());
        assert_eq!(first.authz_id(), "dm:alice");
        assert_eq!(second.authz_id(), "dm:alice");
    }

    #[tokio::test]
    async fn work_capacity_rejects_overload_with_explicit_reply() {
        let work_capacity = BotCore::work_capacity(1);
        let all_permits = work_capacity.clone().acquire_owned().await.unwrap();
        let (work_sender, _work_receiver) = mpsc::unbounded_channel();
        let acp_info = Arc::new(std::sync::Mutex::new(AcpInfo::default()));
        let coordinator: Arc<dyn crate::engine::coordinator::Coordinator> =
            Arc::new(crate::engine::coordinator::NoopCoordinator::new());
        let core = BotCore {
            work_sender,
            work_capacity,
            inflight: Arc::new(std::sync::Mutex::new(std::collections::HashSet::new())),
            authz: None,
            response_policy: Arc::new(ResponsePolicyConfig::default_policy()),
            acp_info: acp_info.clone(),
            coordinator: coordinator.clone(),
            lease_manager: LeaseManager::new(coordinator),
            rate_limit: crate::config::RateLimitConfig::default(),
        };
        let (reply_tx, reply_rx) = oneshot::channel();

        let permit = enqueue_work(&core, Work::Status {
            conversation: "conversation-overload".into(),
            reply_tx,
        });

        assert!(permit.is_none());
        assert_eq!(reply_rx.await.unwrap(), "⏳ The bot is at capacity — try again shortly");
        assert_eq!(acp_info.lock().unwrap().overload_rejections, 1);
        drop(all_permits);
    }

    #[test]
    fn work_capacity_is_scoped_per_core() {
        let first = BotCore::work_capacity(1);
        let second = BotCore::work_capacity(1);

        let _first_permit = first.try_acquire_owned().unwrap();
        assert!(second.try_acquire_owned().is_ok());
    }

    #[tokio::test(start_paused = true)]
    async fn unconfirmed_lease_loss_cancellation_does_not_fail_runtime() {
        let (work_sender, mut work_receiver) = mpsc::unbounded_channel();
        tokio::spawn(async move {
            if let Some(Work::CancelAndWait { reply_tx, .. }) = work_receiver.recv().await {
                let _reply_tx = reply_tx;
                std::future::pending::<()>().await;
            }
        });
        let acp_info = Arc::new(std::sync::Mutex::new(AcpInfo {
            runtime_state: crate::engine::acp::AcpRuntimeState::Running,
            ..AcpInfo::default()
        }));
        let coordinator: Arc<dyn crate::engine::coordinator::Coordinator> =
            Arc::new(crate::engine::coordinator::NoopCoordinator::new());
        let core = BotCore {
            work_sender,
            work_capacity: BotCore::work_capacity(1),
            inflight: Arc::new(std::sync::Mutex::new(std::collections::HashSet::new())),
            authz: None,
            response_policy: Arc::new(ResponsePolicyConfig::default_policy()),
            acp_info: acp_info.clone(),
            coordinator: coordinator.clone(),
            lease_manager: LeaseManager::new(coordinator),
            rate_limit: crate::config::RateLimitConfig::default(),
        };
        let (_reply_tx, mut reply_rx) = oneshot::channel();
        let (_lease_tx, mut lease_rx) = tokio::sync::watch::channel(Some("lease expired".to_string()));

        let result = await_prompt_reply(
            &core,
            "conversation-timeout",
            "request-timeout",
            &mut reply_rx,
            Some(&mut lease_rx),
        )
        .await;

        assert!(matches!(result, Err(PromptWaitFailure::LeaseLost(_))));
        let info = acp_info.lock().unwrap();
        assert_eq!(info.runtime_state, crate::engine::acp::AcpRuntimeState::Running);
        assert_eq!(
            info.last_failure.as_deref(),
            Some("lease-loss cancellation was not confirmed for conversation-timeout")
        );
    }
}
