//! Slack Socket Mode frontend.
//!
//! Connects to Slack via WebSocket, receives events (messages, mentions, reactions),
//! and dispatches them through the bot core. Handles:
//! - Message and app_mention events → [`crate::engine::core::dispatch`]
//! - Reaction events → tool approval (✅/❌/🔓)
//! - File downloads from message attachments
//! - Markdown → Slack mrkdwn conversion

use std::collections::HashMap;
use std::sync::{
    Arc,
    LazyLock,
    Mutex,
};

use anyhow::Result;
use async_trait::async_trait;
use regex::Regex;
use slack_morphism::prelude::*;
use tokio::sync::oneshot;
use tracing::{
    error,
    info,
    warn,
};

use crate::engine::acp::{
    ApprovalRequest,
    ApprovalResponse,
};
use crate::engine::core::{
    BotCore,
    Conversation,
    Frontend,
    IncomingMessage,
    Reply,
    determine_reply_location,
};
use crate::engine::user_map::UserMap;

// ---------------------------------------------------------------------------
// Tool approval state
// ---------------------------------------------------------------------------

/// Pending approval: maps message timestamp → approval state.
pub type PendingApprovals = Arc<Mutex<HashMap<String, PendingApproval>>>;

pub struct PendingApproval {
    pub tool_name: String,
    pub options: Vec<(String, String)>,
    pub reply_tx: Option<oneshot::Sender<ApprovalResponse>>,
}


// ---------------------------------------------------------------------------
// File downloads
// ---------------------------------------------------------------------------

/// Download files attached to a Slack message, returning `(path, mimetype)` pairs.
async fn download_slack_files(files: &[SlackFile], bot_token: &str) -> Vec<(String, String)> {
    let dir = std::path::Path::new("/tmp/kiro-bot-files");
    let _ = std::fs::create_dir_all(dir);
    let client = reqwest::Client::new();
    let mut results = Vec::new();
    for file in files {
        let url = file.url_private_download.as_ref().or(file.url_private.as_ref());
        let Some(url) = url else { continue };
        let name = file.name.as_deref().unwrap_or("file");
        let ext = name.rsplit('.').next().unwrap_or("bin");
        let dest = dir.join(format!("{}.{}", file.id.0, ext));
        let mime = file.mimetype.as_ref().map(|m| m.to_string()).unwrap_or_default();
        match client
            .get(url.as_str())
            .header("Authorization", format!("Bearer {bot_token}"))
            .send()
            .await
            .and_then(|r| r.error_for_status())
        {
            Ok(resp) => match resp.bytes().await {
                Ok(bytes) => {
                    if std::fs::write(&dest, &bytes).is_ok() {
                        info!(path = %dest.display(), mime = %mime, "Downloaded file");
                        results.push((dest.to_string_lossy().into_owned(), mime));
                    }
                },
                Err(e) => error!("Failed to read file bytes: {e}"),
            },
            Err(e) => error!("Failed to download {url}: {e}"),
        }
    }
    results
}

// ---------------------------------------------------------------------------
// Markdown → Slack mrkdwn
// ---------------------------------------------------------------------------

/// Convert standard markdown to Slack mrkdwn format.
pub fn markdown_to_slack(text: &str) -> String {
    static THINKING: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?s)<thinking>.*?</thinking>\s*").unwrap());
    static HEADING: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?m)^#{1,6}\s+(.+)$").unwrap());
    static BOLD: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\*\*(.+?)\*\*").unwrap());
    static BOLD2: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"__(.+?)__").unwrap());
    static BULLET: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?m)^[-*]\s+").unwrap());
    static LINK: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\[([^\]]+)\]\(([^\)]+)\)").unwrap());

    let r = THINKING.replace_all(text, "");
    let r = HEADING.replace_all(&r, "*$1*");
    let r = BOLD.replace_all(&r, "*$1*");
    let r = BOLD2.replace_all(&r, "*$1*");
    let r = BULLET.replace_all(&r, "• ");
    LINK.replace_all(&r, "<$2|$1>").into_owned()
}

// ---------------------------------------------------------------------------
// SlackFrontend
// ---------------------------------------------------------------------------

pub struct SlackFrontend {
    pub client: Arc<SlackHyperClient>,
    pub bot_token: SlackApiToken,
    pub user_map: Arc<UserMap>,
    pub conversation_history: u16,
    pub last_seen: Mutex<HashMap<String, SlackTs>>,
}

#[async_trait]
impl Frontend for SlackFrontend {
    async fn send(&self, reply: Reply) -> Result<String> {
        let session = self.client.open_session(&self.bot_token);
        match reply {
            Reply::Send {
                conversation,
                reply_to,
                text,
            } => {
                let mut req = SlackApiChatPostMessageRequest::new(
                    conversation.into(),
                    SlackMessageContent::new().with_text(markdown_to_slack(&text)),
                );
                if let Some(ts) = reply_to {
                    req = req.with_thread_ts(ts.into());
                }
                let resp = session.chat_post_message(&req).await?;
                Ok(resp.ts.to_string())
            },
            Reply::Update {
                conversation,
                message_id,
                text,
            } => {
                session
                    .chat_update(&SlackApiChatUpdateRequest::new(
                        conversation.into(),
                        SlackMessageContent::new().with_text(text),
                        message_id.clone().into(),
                    ))
                    .await?;
                Ok(message_id)
            },
            Reply::Delete {
                conversation,
                message_id,
            } => {
                session
                    .chat_delete(&SlackApiChatDeleteRequest::new(
                        conversation.into(),
                        message_id.clone().into(),
                    ))
                    .await?;
                Ok(message_id)
            },
        }
    }

    async fn fetch_context(&self, conversation: &str, before: &str, thread_ts: Option<&str>) -> Vec<String> {
        if self.conversation_history == 0 {
            return vec![];
        }
        let session = self.client.open_session(&self.bot_token);
        let channel: SlackChannelId = conversation.into();
        let latest: SlackTs = before.into();

        let result = if let Some(ts) = thread_ts {
            let req = SlackApiConversationsRepliesRequest::new(channel, ts.into())
                .with_latest(latest)
                .with_limit(self.conversation_history);
            match session.conversations_replies(&req).await {
                Ok(resp) => resp
                    .messages
                    .iter()
                    .skip(1)
                    .rev()
                    .filter_map(|m| {
                        let text = m.content.text.as_deref()?;
                        let user = m
                            .sender
                            .user
                            .as_ref()
                            .map(|u| self.user_map.resolve(u.as_ref()).to_string())
                            .unwrap_or_else(|| "bot".into());
                        Some(format!("{user}: {text}"))
                    })
                    .collect(),
                Err(e) => {
                    info!("Thread context fetch failed: {e}");
                    vec![]
                },
            }
        } else {
            let oldest = self.last_seen.lock().unwrap().get(conversation).cloned();
            let mut req = SlackApiConversationsHistoryRequest::new()
                .with_channel(channel)
                .with_latest(latest)
                .with_limit(self.conversation_history);
            if let Some(oldest) = oldest {
                req = req.with_oldest(oldest);
            }
            match session.conversations_history(&req).await {
                Ok(resp) => resp
                    .messages
                    .iter()
                    .rev()
                    .filter_map(|m| {
                        let text = m.content.text.as_deref()?;
                        let user = m
                            .sender
                            .user
                            .as_ref()
                            .map(|u| self.user_map.resolve(u.as_ref()).to_string())
                            .unwrap_or_else(|| "bot".into());
                        Some(format!("{user}: {text}"))
                    })
                    .collect(),
                Err(e) => {
                    info!("Context fetch failed: {e}");
                    vec![]
                },
            }
        };
        self.last_seen
            .lock()
            .unwrap()
            .insert(conversation.to_string(), before.into());
        result
    }
}

// ---------------------------------------------------------------------------
// Slack event handling
// ---------------------------------------------------------------------------

/// Shared state passed to Slack event callbacks.
#[derive(Clone)]
pub struct SlackState {
    pub core: BotCore,
    pub frontend: Arc<SlackFrontend>,
    pub user_id: String,
    pub member_id: String,
    pub bot_user_id: String,
    pub user_map: Arc<UserMap>,
    pub pending_approvals: PendingApprovals,
    /// 👍/👎 reactions on bot-authored messages flow into this writer for the
    /// nightly metrics Lambda. None when no feedback table is configured —
    /// reactions are still consumed by the approval flow above, just not
    /// persisted as feedback.
    pub feedback_writer: Option<Arc<dyn crate::engine::feedback::FeedbackWriter>>,
    /// This task's identity, as written into the cross-task approvals table.
    /// Read by the reaction handler to compare against the lookup-owner so a
    /// self-owned approval short-circuits without forwarding to ourselves.
    /// Empty for local CLI / unit-test contexts.
    pub own_task_id: String,
}

static MENTION_PATTERN: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"<@[A-Z0-9]+>").unwrap());

/// Handle all Slack push events (messages, mentions, reactions).
pub async fn on_push(
    event: SlackPushEventCallback,
    _client: Arc<SlackHyperClient>,
    states: SlackClientEventsUserState,
) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let guard = states.read().await;
    let state = guard.get_user_state::<SlackState>().ok_or("no state")?.clone();
    drop(guard);
    dispatch_event(event, &state, /* forwarded */ false).await
}

/// Drive a `SlackPushEventCallback` through the same per-event handlers
/// `on_push` uses. Exposed so a forwarded event arriving at this task's
/// `/dispatch` endpoint can be processed exactly as if Slack had delivered
/// it natively. `forwarded = true` suppresses re-forwarding on a local miss
/// so peers don't ping-pong reactions.
pub async fn dispatch_event(
    event: SlackPushEventCallback,
    state: &SlackState,
    forwarded: bool,
) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>> {
    match event.event {
        SlackEventCallbackBody::Message(msg) => handle_message(msg, state).await?,
        SlackEventCallbackBody::AppMention(mention) => handle_mention(mention, state).await?,
        SlackEventCallbackBody::ReactionAdded(reaction) => handle_reaction(reaction, state, forwarded).await?,
        _ => {},
    }
    Ok(())
}

async fn handle_message(
    msg: SlackMessageEvent,
    state: &SlackState,
) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let raw_user_id = msg.sender.user.as_ref().map(|u| u.to_string()).unwrap_or_default();
    if msg.subtype.is_some() || msg.sender.bot_id.is_some() {
        return Ok(());
    }
    if !state.user_id.is_empty() && raw_user_id != state.user_id {
        return Ok(());
    }
    let user = state.frontend.user_map.resolve(&raw_user_id).to_string();
    let text = msg
        .content
        .as_ref()
        .and_then(|c| c.text.as_deref())
        .unwrap_or("")
        .to_string();
    let channel = match msg.origin.channel {
        Some(c) => c,
        None => return Ok(()),
    };
    let is_dm = msg.origin.channel_type.as_ref().map(|t| t.to_string()) == Some("im".into());
    let mentioned = !state.member_id.is_empty() && text.contains(&format!("<@{}>", state.member_id));
    let is_thread = msg.origin.thread_ts.is_some();
    let mut text = if mentioned {
        MENTION_PATTERN.replace_all(&text, "").trim().to_string()
    } else {
        text
    };

    // Download attached files
    if let Some(files) = msg.content.as_ref().and_then(|c| c.files.as_ref()) {
        for (path, mime) in download_slack_files(files, state.frontend.bot_token.token_value.0.as_str()).await {
            text = format!("[File downloaded ({mime}) to: {path} — read it]\n{text}");
        }
    }

    let scope = if is_dm {
        "dm".to_string()
    } else {
        format!("channel:{channel}")
    };
    let directed = is_dm || mentioned;
    if !state.core.response_policy.should_respond(&scope, directed, is_thread) {
        info!(scope, directed, is_thread, "Filtered by response policy");
        return Ok(());
    }

    let reply_to = determine_reply_location(
        &state.core.response_policy,
        &scope,
        &text,
        msg.origin.thread_ts.as_ref().map(|ts| ts.0.as_str()),
        &msg.origin.ts.0,
    );

    let mut context = state
        .frontend
        .fetch_context(
            channel.as_ref(),
            &msg.origin.ts.0,
            msg.origin.thread_ts.as_ref().map(|ts| ts.0.as_str()),
        )
        .await;
    if context.is_empty()
        && let Some(ts) = msg.origin.thread_ts.as_ref()
    {
        context.push(format!(
            "[This message is in a Slack thread. Channel: {channel}, thread_ts: {}. Use Slack tools to read the thread for context if needed.]",
            ts.0
        ));
    }

    let conversation = if is_dm {
        Conversation::Dm {
            channel: channel.to_string(),
            user: user.clone(),
        }
    } else if let Some(ref rt) = reply_to {
        let thread_ts = msg
            .origin
            .thread_ts
            .as_ref()
            .map(|ts| ts.0.clone())
            .unwrap_or_else(|| rt.clone());
        Conversation::Thread {
            channel: channel.to_string(),
            thread_ts,
        }
    } else {
        Conversation::Channel(channel.to_string())
    };

    crate::engine::core::dispatch(
        &state.core,
        IncomingMessage {
            user,
            slack_user_id: raw_user_id,
            text,
            conversation,
            reply_to,
            directed,
            context,
        },
        state.frontend.clone(),
    );
    Ok(())
}

async fn handle_mention(
    mention: SlackAppMentionEvent,
    state: &SlackState,
) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>> {
    if !state.user_id.is_empty() && mention.user.to_string() != state.user_id {
        return Ok(());
    }
    let user = state.frontend.user_map.resolve(mention.user.as_ref()).to_string();
    let mut text = MENTION_PATTERN
        .replace_all(mention.content.text.as_deref().unwrap_or(""), "")
        .trim()
        .to_string();

    if let Some(files) = mention.content.files.as_ref() {
        for (path, mime) in download_slack_files(files, state.frontend.bot_token.token_value.0.as_str()).await {
            text = format!("[File downloaded ({mime}) to: {path} — read it]\n{text}");
        }
    }

    let scope = format!("channel:{}", mention.channel);
    let reply_to = determine_reply_location(
        &state.core.response_policy,
        &scope,
        &text,
        mention.origin.thread_ts.as_ref().map(|ts| ts.0.as_str()),
        &mention.origin.ts.0,
    );

    let mut context = state
        .frontend
        .fetch_context(
            mention.channel.as_ref(),
            &mention.origin.ts.0,
            mention.origin.thread_ts.as_ref().map(|ts| ts.0.as_str()),
        )
        .await;
    if context.is_empty()
        && let Some(ts) = mention.origin.thread_ts.as_ref()
    {
        context.push(format!(
            "[This message is in a Slack thread. Channel: {}, thread_ts: {}. Use Slack tools to read the thread for context if needed.]",
            mention.channel, ts.0
        ));
    }

    let conversation = if let Some(ref rt) = reply_to {
        let thread_ts = mention
            .origin
            .thread_ts
            .as_ref()
            .map(|ts| ts.0.clone())
            .unwrap_or_else(|| rt.clone());
        Conversation::Thread {
            channel: mention.channel.to_string(),
            thread_ts,
        }
    } else {
        Conversation::Channel(mention.channel.to_string())
    };

    crate::engine::core::dispatch(
        &state.core,
        IncomingMessage {
            user,
            slack_user_id: mention.user.to_string(),
            text,
            conversation,
            reply_to,
            directed: true,
            context,
        },
        state.frontend.clone(),
    );
    Ok(())
}

async fn handle_reaction(
    reaction: SlackReactionAddedEvent,
    state: &SlackState,
    forwarded: bool,
) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let reactor = reaction.user.to_string();
    tracing::debug!(reactor, bot_user_id = %state.bot_user_id, member_id = %state.member_id, "Reaction event received");
    if reactor == state.bot_user_id || reactor == state.member_id {
        tracing::debug!("Ignoring bot's own reaction");
        return Ok(());
    }
    let SlackReactionsItem::Message(msg) = &reaction.item else {
        return Ok(());
    };
    let ts = msg.origin.ts.to_string();
    let channel = msg.origin.channel.clone();
    let emoji = reaction.reaction.0.as_str();

    let pending_count = state.pending_approvals.lock().unwrap().len();
    tracing::info!(
        emoji,
        ts,
        pending_count,
        "Reaction on message, checking pending approvals"
    );

    // Phase 6: 👍/👎 on a bot-authored message → persist as feedback. Runs
    // alongside the approval flow below — a 👍 on an approval prompt is
    // both an "allow_once" and (incidentally) a positive-feedback signal.
    if let Some(writer) = state.feedback_writer.as_ref()
        && let Some(reaction_kind) = crate::engine::feedback::Reaction::from_slack(emoji)
    {
        let channel_str = channel.as_ref().map(|c| c.to_string()).unwrap_or_default();
        // Pull chunk_ids from the most recent grounded assistant turn for
        // this conversation. We try the obvious convo ids in order:
        // - `dm:<reactor>` for DMs (handler ignored bot reactions earlier)
        // - `channel:<id>` for channel reactions
        // The first hit wins; loading both is cheap (DDB Query+Limit).
        let candidates = [format!("dm:{reactor}"), format!("channel:{channel_str}")];
        let mut chunk_ids = Vec::new();
        for convo in &candidates {
            let turns = state.core.coordinator.load_history(convo, 5).await.unwrap_or_default();
            let from_window = crate::engine::feedback::chunk_ids_for_recent_assistant_turn(&turns);
            if !from_window.is_empty() {
                chunk_ids = from_window;
                break;
            }
        }
        let record = crate::engine::feedback::FeedbackRecord {
            slack_msg_id: format!("{channel_str}:{ts}"),
            reaction: reaction_kind,
            chunk_ids,
            ts: chrono::Utc::now(),
        };
        if let Err(e) = writer.record(record).await {
            tracing::warn!(error = %e, "feedback PutItem failed; not blocking approval flow");
        }
    }

    let approval = state.pending_approvals.lock().unwrap().remove(&ts);
    let Some(mut approval) = approval else {
        // Local miss: maybe a peer task posted this approval. Look up the
        // owner in the cross-task approvals table (if any) and forward.
        // Skip forwarding for events that *already* arrived via /dispatch —
        // otherwise we ping-pong with the original owner.
        if !forwarded
            && let Ok(Some(peer)) = state.core.coordinator.lookup_approval_owner(&ts).await
            && !peer.is_empty()
            && peer != state.own_task_id
        {
            let payload = serde_json::json!({
                "token": "",
                "team_id": "",
                "api_app_id": "",
                "type": "event_callback",
                "event_id": format!("forward:{ts}:{emoji}"),
                "event_time": chrono::Utc::now().timestamp(),
                "authed_users": [],
                "event": {
                    "type": "reaction_added",
                    "user": reactor,
                    "reaction": emoji,
                    "item": &reaction.item,
                    "item_user": "",
                    "event_ts": ts.clone(),
                },
            });
            let payload = crate::engine::coordinator::ForwardEvent {
                slack_event_json: payload,
            };
            if let Err(e) = state.core.coordinator.forward(&peer, payload).await {
                warn!(error = %e, peer, msg_ts = %ts, "forward to approval owner failed");
            } else {
                tracing::info!(peer, msg_ts = %ts, "forwarded reaction to approval owner");
            }
        }
        return Ok(());
    };

    info!(emoji, tool = %approval.tool_name, "Reaction on approval message");
    let Some(reply_tx) = approval.reply_tx.take() else {
        return Ok(());
    };

    let (response, label) = match emoji {
        "white_check_mark" => {
            let opt = approval
                .options
                .iter()
                .find(|(id, _)| id.contains("allow_once") || id.contains("yes"))
                .or(approval.options.first());
            (
                opt.map(|(id, _)| ApprovalResponse::Selected(id.clone()))
                    .unwrap_or(ApprovalResponse::Denied),
                opt.map(|(_, l)| l.clone()).unwrap_or_else(|| "denied".into()),
            )
        },
        "unlock" => {
            let opt = approval
                .options
                .iter()
                .find(|(id, _)| id.contains("allow_always") || id.contains("always"))
                .or(approval.options.last());
            (
                opt.map(|(id, _)| ApprovalResponse::Selected(id.clone()))
                    .unwrap_or(ApprovalResponse::Denied),
                opt.map(|(_, l)| l.clone()).unwrap_or_else(|| "denied".into()),
            )
        },
        "x" => {
            let opt = approval
                .options
                .iter()
                .find(|(id, _)| id.contains("reject") || id.contains("no") || id.contains("deny"));
            (
                opt.map(|(id, _)| ApprovalResponse::Selected(id.clone()))
                    .unwrap_or(ApprovalResponse::Denied),
                opt.map(|(_, l)| l.clone()).unwrap_or_else(|| "denied".into()),
            )
        },
        _ => (ApprovalResponse::Denied, "denied".into()),
    };
    let _ = reply_tx.send(response);

    if let Some(ch) = channel {
        let frontend = state.frontend.clone();
        let consequence = match emoji {
            "white_check_mark" => "Tool will run this one time only.",
            "unlock" => "Tool is now trusted for the rest of this session.",
            _ => "Tool was blocked and will not run.",
        };
        let text = format!(
            "🔐 `{}`\n*Option selected:* {label}\n_{consequence}_",
            approval.tool_name
        );
        tokio::spawn(async move {
            let _ = frontend
                .send(Reply::Update {
                    conversation: ch.to_string(),
                    message_id: ts,
                    text,
                })
                .await;
        });
    }
    Ok(())
}

/// How long an approval row in the cross-task approvals DDB table is valid.
/// Must outlive the in-process `tokio::time::timeout(600s)` on the ACP-side
/// `reply_rx.await` plus a comfortable cushion so a slow user doesn't make
/// the row TTL out before they react.
const APPROVAL_DDB_TTL: std::time::Duration = std::time::Duration::from_secs(1800);

/// Derive the conversation id used as a hash for cross-task forwarding.
/// Mirrors `engine::core::Conversation::id` for thread/channel cases; DM
/// approvals are rare in practice so the simpler `channel:<id>` form is
/// fine — the routing key only needs to be stable across tasks.
fn conv_id_for_approval(channel: &str, thread_ts: Option<&str>) -> String {
    match thread_ts {
        Some(ts) => format!("thread:{channel}:{ts}"),
        None => format!("channel:{channel}"),
    }
}

/// Spawn a task that posts approval requests to Slack and seeds emoji reactions.
pub fn spawn_approval_listener(
    mut approval_rx: tokio::sync::mpsc::UnboundedReceiver<ApprovalRequest>,
    client: Arc<SlackHyperClient>,
    bot_token: SlackApiToken,
    pending: PendingApprovals,
    coordinator: Arc<dyn crate::engine::coordinator::Coordinator>,
) {
    tokio::spawn(async move {
        while let Some(req) = approval_rx.recv().await {
            let options_text = req
                .options
                .iter()
                .map(|(_, label)| label.as_str())
                .collect::<Vec<_>>()
                .join(" / ");
            let text = format!(
                "🔐 *Permission request*\n`{}`\nOptions: {options_text}\n\nReact: ✅ allow · ❌ deny · 🔓 trust\ncc: <@{}>",
                req.tool_name, req.slack_user_id
            );

            let session = client.open_session(&bot_token);
            let channel: SlackChannelId = req.channel.clone().into();
            let mut post =
                SlackApiChatPostMessageRequest::new(channel.clone(), SlackMessageContent::new().with_text(text));
            if let Some(ref ts) = req.thread_ts {
                post = post.with_thread_ts(ts.clone().into());
            }

            let msg_ts = match session.chat_post_message(&post).await {
                Ok(resp) => resp.ts.to_string(),
                Err(e) => {
                    error!("Failed to post approval message: {e}");
                    let _ = req.reply_tx.send(ApprovalResponse::Denied);
                    continue;
                },
            };

            let ts: SlackTs = msg_ts.clone().into();
            for emoji in ["white_check_mark", "x", "unlock"] {
                let _ = session
                    .reactions_add(&SlackApiReactionsAddRequest::new(
                        channel.clone(),
                        SlackReactionName(emoji.into()),
                        ts.clone(),
                    ))
                    .await;
            }

            // Cross-task: record this task as the owner of the approval before
            // installing the in-memory entry so a peer's reaction lookup
            // doesn't race against the local insert. Coordinator failure is
            // logged and treated as a soft fall-through to local-only mode —
            // the in-memory map still works, the bot just can't route across
            // tasks until DDB recovers.
            let conv_id = conv_id_for_approval(&req.channel, req.thread_ts.as_deref());
            if let Err(e) = coordinator
                .register_approval(&msg_ts, &conv_id, APPROVAL_DDB_TTL)
                .await
            {
                warn!(error = %e, msg_ts, "register_approval failed; falling back to local-only routing");
            }

            pending.lock().unwrap().insert(msg_ts.clone(), PendingApproval {
                tool_name: req.tool_name,
                options: req.options,
                reply_tx: Some(req.reply_tx),
            });
            tracing::info!(
                msg_ts,
                pending_count = pending.lock().unwrap().len(),
                "Registered pending approval"
            );
        }
    });
}

/// Slack error handler — reconnects on WebSocket resets.
pub fn on_error(
    err: Box<dyn std::error::Error + Send + Sync>,
    _: Arc<SlackHyperClient>,
    _: SlackClientEventsUserState,
) -> http::StatusCode {
    let msg = err.to_string();
    if msg.contains("ConnectionReset") || msg.contains("ResetWithoutClosingHandshake") {
        warn!("Slack WebSocket reconnecting: {err}");
    } else {
        error!("Slack: {err}");
    }
    http::StatusCode::OK
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bold_double_asterisk() {
        assert_eq!(markdown_to_slack("**hello**"), "*hello*");
    }

    #[test]
    fn heading_converts_to_bold() {
        assert_eq!(markdown_to_slack("# Title"), "*Title*");
    }

    #[test]
    fn bullet_converts() {
        assert_eq!(markdown_to_slack("- item one\n* item two"), "• item one\n• item two");
    }

    #[test]
    fn link_converts() {
        assert_eq!(
            markdown_to_slack("[click](https://example.com)"),
            "<https://example.com|click>"
        );
    }

    #[test]
    fn thinking_tags_stripped() {
        assert_eq!(markdown_to_slack("<thinking>internal</thinking>\nvisible"), "visible");
    }

    #[test]
    fn plain_text_unchanged() {
        assert_eq!(markdown_to_slack("just text"), "just text");
    }
}
