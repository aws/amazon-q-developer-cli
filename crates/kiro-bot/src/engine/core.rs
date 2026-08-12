//! Frontend-agnostic bot core: message dispatch, command resolution, and reply routing.
//!
//! The core receives [`IncomingMessage`]s from any frontend, resolves them to
//! [`Action`]s, checks authorization, and dispatches work to the ACP pool.

use std::sync::Arc;

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
    Work,
};
use crate::engine::authz::Authorizer;
use crate::engine::coordinator::{
    ForwardEvent,
    LeaseGuard,
    LeaseOutcome,
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

// ---------------------------------------------------------------------------
// Conversation types
// ---------------------------------------------------------------------------

/// A conversation scope used for session keying and Cedar authorization.
#[derive(Debug, Clone)]
pub enum Conversation {
    /// Direct message. Cedar resource: `Conversation::"dm:<user>"`.
    Dm { channel: String, user: String },
    /// Channel-level. Cedar resource: `Conversation::"channel:<id>"`.
    Channel(String),
    /// Thread-scoped. Authorization inherits from the parent channel.
    Thread { channel: String, thread_ts: String },
}

impl Conversation {
    /// Unique ID used as ACP session key.
    pub fn id(&self) -> String {
        match self {
            Self::Dm { user, .. } => format!("dm:{user}"),
            Self::Channel(id) => format!("channel:{id}"),
            Self::Thread { channel, thread_ts } => format!("thread:{channel}:{thread_ts}"),
        }
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
            Self::Thread { channel, .. } => format!("channel:{channel}"),
            other => other.id(),
        }
    }
}

// ---------------------------------------------------------------------------
// Message and reply types
// ---------------------------------------------------------------------------

pub struct IncomingMessage {
    pub user: String,
    pub slack_user_id: String,
    pub text: String,
    pub conversation: Conversation,
    pub reply_to: Option<String>,
    pub directed: bool,
    pub context: Vec<String>,
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
}

#[derive(Debug)]
pub enum Reply {
    Send {
        conversation: String,
        reply_to: Option<String>,
        text: String,
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
}

impl BotCore {
    pub fn work_capacity(max_active_work_items: usize) -> Arc<tokio::sync::Semaphore> {
        Arc::new(tokio::sync::Semaphore::new(max_active_work_items))
    }
}

fn enqueue_work(core: &BotCore, work: Work) -> Option<tokio::sync::OwnedSemaphorePermit> {
    let conversation_id = work.conversation_id().unwrap_or("runtime").to_string();
    let permit = match core.work_capacity.clone().try_acquire_owned() {
        Ok(permit) => permit,
        Err(_) => {
            core.acp_info.lock().unwrap().overload_rejections += 1;
            tracing::warn!(
                %conversation_id,
                "ACP work capacity exhausted"
            );
            work.reject("⏳ The bot is at capacity — try again shortly");
            return None;
        },
    };
    match core.work_sender.send(work) {
        Ok(()) => Some(permit),
        Err(error) => {
            tracing::error!(%conversation_id, "ACP control channel is closed");
            error.0.reject("Error: ACP runtime is unavailable");
            None
        },
    }
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

async fn stop_progress_updates(progress_handle: tokio::task::JoinHandle<()>) {
    progress_handle.abort();
    match progress_handle.await {
        Ok(()) => {},
        Err(error) if error.is_cancelled() => {},
        Err(error) => tracing::warn!(%error, "progress update task failed"),
    }
}

async fn replace_progress_with_reply(
    frontend: &Arc<dyn Frontend>,
    platform_id: String,
    ack_id: String,
    reply_to: Option<String>,
    text: String,
    progress_handle: tokio::task::JoinHandle<()>,
) {
    stop_progress_updates(progress_handle).await;
    let _ = frontend
        .send(Reply::Delete {
            conversation: platform_id.clone(),
            message_id: ack_id,
        })
        .await;
    let _ = frontend
        .send(Reply::Send {
            conversation: platform_id,
            reply_to,
            text,
        })
        .await;
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

async fn report_lease_loss(frontend: &Arc<dyn Frontend>, platform_id: &str, ack_id: &str, reply_to: &Option<String>) {
    let _ = frontend
        .send(Reply::Delete {
            conversation: platform_id.to_string(),
            message_id: ack_id.to_string(),
        })
        .await;
    let _ = frontend
        .send(Reply::Send {
            conversation: platform_id.to_string(),
            reply_to: reply_to.clone(),
            text: "Error: Coordination lease was lost, so this request was cancelled. Please retry.".into(),
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
/// event cluster-wide via [`crate::engine::coordinator::Coordinator`]: dedup by `event_id`, then
/// acquire a per-conversation lease. Only the winning task reaches the
/// per-action work below; losers drop silently or forward to the lease
/// holder. See [`DispatchEnvelope`] for the inputs.
pub fn dispatch(core: &BotCore, msg: IncomingMessage, frontend: Arc<dyn Frontend>) {
    let conversation = msg.conversation.clone();
    let conv_id = conversation.id();
    let platform_id = conversation.platform_id().to_string();
    let reply_to = msg.reply_to.clone();
    let session_key = conv_id.clone();

    let action = resolve_action(&msg.text);

    // Cancel bypasses dedup, lease, and the inflight guard — it has to fire
    // while a prompt is in-flight. Sending 🛑 from both tasks is acceptable
    // (idempotent on the ACP side) and preferable to dropping a cancel that
    // never reaches the leader.
    if matches!(action, Action::Cancel) {
        if core
            .work_sender
            .send(Work::Cancel {
                conversation: session_key,
            })
            .is_err()
        {
            tracing::error!(conversation_id = %conv_id, "ACP control channel is closed");
        }
        tokio::spawn(async move {
            let _ = frontend
                .send(Reply::Send {
                    conversation: platform_id,
                    reply_to,
                    text: "🛑".into(),
                })
                .await;
        });
        return;
    }

    let core = core.clone();
    let envelope = msg.envelope.clone();

    tokio::spawn(async move {
        let request_id = envelope
            .as_ref()
            .map(|env| env.event_id.clone())
            .unwrap_or_else(|| format!("local:{conv_id}"));
        // 1. Cluster-wide dedup. Skip when there's no envelope (CLI / cron / forwarded-from-peer events) —
        //    those callers have already arbitrated and `NoopCoordinator` would dedup per-process anyway.
        if let Some(ref env) = envelope
            && !core.coordinator.dedupe_event(&env.event_id).await
        {
            tracing::debug!(
                request_id = %request_id,
                conversation_id = %conv_id,
                "duplicate event, dropping"
            );
            return;
        }

        // 2. Lease arbitration. Held → forward; on forward failure, force-acquire and proceed locally.
        //    Unavailable → silent drop (operators alarm on the structured log; users retry).
        let mut lease = None;
        if let Some(ref env) = envelope {
            match core.coordinator.try_acquire(&conv_id).await {
                LeaseOutcome::Acquired => {
                    lease = Some(LeaseGuard::start(
                        core.coordinator.clone(),
                        conv_id.clone(),
                        request_id.clone(),
                    ));
                },
                LeaseOutcome::Held { peer } => {
                    let payload = ForwardEvent {
                        slack_event_json: env.raw.clone(),
                    };
                    match core.coordinator.forward(&peer, payload).await {
                        Ok(()) => {
                            tracing::debug!(
                                %request_id,
                                %peer,
                                conversation_id = %conv_id,
                                "forwarded to lease holder"
                            );
                            return;
                        },
                        Err(e) => {
                            tracing::warn!(
                                %request_id,
                                error = %e,
                                %peer,
                                conversation_id = %conv_id,
                                "forward failed; attempting force_acquire"
                            );
                            if !core.coordinator.force_acquire(&conv_id, &peer).await {
                                tracing::warn!(
                                    %request_id,
                                    conversation_id = %conv_id,
                                    "force_acquire failed; dropping (lease still held by live peer or coordinator down)"
                                );
                                return;
                            }
                            lease = Some(LeaseGuard::start(
                                core.coordinator.clone(),
                                conv_id.clone(),
                                request_id.clone(),
                            ));
                        },
                    }
                },
                LeaseOutcome::Unavailable => {
                    tracing::warn!(
                        target: "kiro_bot::coordinator",
                        %request_id,
                        event_id = %env.event_id,
                        conversation_id = %conv_id,
                        "coordinator_unavailable_dropped: lease unavailable, dropping event"
                    );
                    return;
                },
            }
        }

        // 3. Inflight guard. Read-only commands skip it.
        let mut holds_inflight = false;
        if !action.is_readonly() {
            if !core.inflight.lock().unwrap().insert(session_key.clone()) {
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
            core,
            frontend,
        )
        .await;
    });
}

/// Runs an action after dispatch has acquired its local and cluster guards.
#[allow(clippy::too_many_arguments)]
async fn run_action(
    action: Action,
    msg: IncomingMessage,
    conversation: Conversation,
    conv_id: String,
    platform_id: String,
    reply_to: Option<String>,
    session_key: String,
    request_id: String,
    mut lease_loss: Option<tokio::sync::watch::Receiver<Option<String>>>,
    core: BotCore,
    frontend: Arc<dyn Frontend>,
) {
    let authz_scope = conversation.authz_id();
    match action {
        Action::Prompt { text } => {
            if let Some(authz) = &core.authz {
                match authz.can_use_bot(&msg.user, &conversation) {
                    Ok(true) => {},
                    _ => {
                        info!("Bot access denied for user {} in {}", msg.user, conv_id);
                        let denied = format!("❌ Access denied for user `{}` in `{}`", msg.user, conv_id);
                        let _ = frontend
                            .send(Reply::Send {
                                conversation: platform_id,
                                reply_to,
                                text: denied,
                            })
                            .await;
                        return;
                    },
                }
            }

            // Capture a copy of the user prompt before move so we can run
            // the post-reply retrieval check without re-fetching it.
            let prompt_for_check = text.clone();

            let ack_id = match frontend
                .send(Reply::Send {
                    conversation: platform_id.clone(),
                    reply_to: reply_to.clone(),
                    text: "_Looking into it..._".into(),
                })
                .await
            {
                Ok(id) => id,
                Err(e) => {
                    tracing::error!("ack failed: {}", e);
                    return;
                },
            };

            let (reply_tx, mut reply_rx) = oneshot::channel();
            let (progress_tx, mut progress_rx) = mpsc::unbounded_channel::<String>();
            let (approval_channel, approval_thread) = match &conversation {
                Conversation::Dm { channel, .. } => (channel.clone(), reply_to.clone()),
                Conversation::Channel(id) => (id.clone(), reply_to.clone()),
                Conversation::Thread { channel, thread_ts } => (channel.clone(), Some(thread_ts.clone())),
            };
            let work_permit = enqueue_work(&core, Work::Prompt {
                text,
                context: msg.context,
                conversation: session_key.clone(),
                channel: approval_channel,
                thread_ts: approval_thread,
                user: msg.user.clone(),
                slack_user_id: msg.slack_user_id.clone(),
                reply_tx,
                progress_tx,
            });

            // Stream tool status updates to the placeholder message
            let progress_handle = {
                let frontend2 = frontend.clone();
                let conv = platform_id.clone();
                let ack = ack_id.clone();
                tokio::spawn(async move {
                    while let Some(status) = progress_rx.recv().await {
                        let _ = frontend2
                            .send(Reply::Update {
                                conversation: conv.clone(),
                                message_id: ack.clone(),
                                text: status,
                            })
                            .await;
                    }
                })
            };

            let reply_text =
                match await_prompt_reply(&core, &session_key, &request_id, &mut reply_rx, lease_loss.as_mut()).await {
                    Ok(reply) => reply,
                    Err(PromptWaitFailure::ReplyChannelClosed) => "Error".into(),
                    Err(PromptWaitFailure::LeaseLost(failure)) => {
                        tracing::debug!(%request_id, conversation_id = %session_key, %failure);
                        stop_progress_updates(progress_handle).await;
                        drop(work_permit);
                        report_lease_loss(&frontend, &platform_id, &ack_id, &reply_to).await;
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
                    let (retry_progress_tx, _retry_progress_rx) = mpsc::unbounded_channel::<String>();
                    let retry_permit = enqueue_work(&core, Work::Prompt {
                        text: retry_prompt.to_string(),
                        context: Vec::new(),
                        conversation: session_key.clone(),
                        channel: match &conversation {
                            Conversation::Dm { channel, .. } => channel.clone(),
                            Conversation::Channel(id) => id.clone(),
                            Conversation::Thread { channel, .. } => channel.clone(),
                        },
                        thread_ts: match &conversation {
                            Conversation::Thread { thread_ts, .. } => Some(thread_ts.clone()),
                            _ => reply_to.clone(),
                        },
                        user: msg.user.clone(),
                        slack_user_id: msg.slack_user_id.clone(),
                        reply_tx: retry_tx,
                        progress_tx: retry_progress_tx,
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
                            stop_progress_updates(progress_handle).await;
                            report_lease_loss(&frontend, &platform_id, &ack_id, &reply_to).await;
                            return;
                        },
                    }
                },
                _ => reply_text,
            };

            replace_progress_with_reply(
                &frontend,
                platform_id,
                ack_id,
                reply_to,
                with_genai_disclaimer(&final_reply_text),
                progress_handle,
            )
            .await;
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
                    let permit = enqueue_work(&core, Work::NewSession {
                        conversation: session_key.clone(),
                        reply_tx: tx,
                    });
                    if let Ok(m) = rx.await {
                        send(m).await;
                    }
                    drop(permit);
                },
                Action::SetAgent { name } => {
                    match check_authz(&core.authz, |a| a.can_use_agent(&user, &name, &authz_scope)) {
                        Ok(true) => {
                            let (tx, rx) = oneshot::channel();
                            let permit = enqueue_work(&core, Work::SetMode {
                                conversation: session_key.clone(),
                                mode: name,
                                reply_tx: tx,
                            });
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
                        let permit = enqueue_work(&core, Work::SetModel {
                            conversation: session_key.clone(),
                            model: name,
                            reply_tx: tx,
                        });
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
                    let permit = enqueue_work(&core, Work::Status {
                        conversation: session_key.clone(),
                        reply_tx: tx,
                    });
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
                Action::Cancel => unreachable!(),
                Action::Unknown => {},
                Action::Prompt { .. } => unreachable!(),
            }
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
                Reply::Update { .. } => {
                    self.update_active.store(true, std::sync::atomic::Ordering::SeqCst);
                    self.events.lock().unwrap().push("update-started");
                    self.update_started.notify_one();
                    let _guard = UpdateActiveGuard { frontend: self };
                    std::future::pending::<()>().await;
                    unreachable!()
                },
                Reply::Delete { .. } => {
                    assert!(!self.update_active.load(std::sync::atomic::Ordering::SeqCst));
                    self.events.lock().unwrap().push("delete");
                    Ok("deleted".into())
                },
                Reply::Send { .. } => {
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
    async fn progress_update_finishes_before_placeholder_replacement() {
        let frontend = Arc::new(BlockingProgressFrontend::default());
        let frontend_dyn: Arc<dyn Frontend> = frontend.clone();
        let (progress_tx, mut progress_rx) = mpsc::unbounded_channel();
        let progress_frontend = frontend_dyn.clone();
        let progress_handle = tokio::spawn(async move {
            while let Some(text) = progress_rx.recv().await {
                let _ = progress_frontend
                    .send(Reply::Update {
                        conversation: "channel".into(),
                        message_id: "ack".into(),
                        text,
                    })
                    .await;
            }
        });
        progress_tx.send("working".into()).unwrap();
        frontend.update_started.notified().await;

        replace_progress_with_reply(
            &frontend_dyn,
            "channel".into(),
            "ack".into(),
            None,
            "done".into(),
            progress_handle,
        )
        .await;

        assert_eq!(*frontend.events.lock().unwrap(), [
            "update-started",
            "update-finished",
            "delete",
            "send"
        ]);
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
    fn dm_authz_is_own_id() {
        let c = Conversation::Dm {
            channel: "D000".into(),
            user: "alice".into(),
        };
        assert_eq!(c.authz_id(), "dm:alice");
    }

    #[tokio::test]
    async fn work_capacity_rejects_overload_with_explicit_reply() {
        let work_capacity = BotCore::work_capacity(1);
        let all_permits = work_capacity.clone().acquire_owned().await.unwrap();
        let (work_sender, _work_receiver) = mpsc::unbounded_channel();
        let acp_info = Arc::new(std::sync::Mutex::new(AcpInfo::default()));
        let core = BotCore {
            work_sender,
            work_capacity,
            inflight: Arc::new(std::sync::Mutex::new(std::collections::HashSet::new())),
            authz: None,
            response_policy: Arc::new(ResponsePolicyConfig::default_policy()),
            acp_info: acp_info.clone(),
            coordinator: Arc::new(crate::engine::coordinator::NoopCoordinator::new()),
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
        let core = BotCore {
            work_sender,
            work_capacity: BotCore::work_capacity(1),
            inflight: Arc::new(std::sync::Mutex::new(std::collections::HashSet::new())),
            authz: None,
            response_policy: Arc::new(ResponsePolicyConfig::default_policy()),
            acp_info: acp_info.clone(),
            coordinator: Arc::new(crate::engine::coordinator::NoopCoordinator::new()),
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
