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
    Work,
};
use crate::engine::authz::Authorizer;
use crate::engine::response_policy::{
    Location,
    ResponsePolicyConfig,
};

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
    pub inflight: Arc<std::sync::Mutex<std::collections::HashSet<String>>>,
    pub authz: Option<Arc<Authorizer>>,
    pub response_policy: Arc<ResponsePolicyConfig>,
    pub acp_info: Arc<std::sync::Mutex<AcpInfo>>,
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

/// Dispatch an incoming message to the appropriate handler.
///
/// For prompts, spawns an async task that sends a progress placeholder,
/// routes the message to the ACP pool, and replaces the placeholder with
/// the agent's response.
pub fn dispatch(core: &BotCore, msg: IncomingMessage, frontend: Arc<dyn Frontend>) {
    let conversation = msg.conversation.clone();
    let conv_id = conversation.id();
    let authz_scope = conversation.authz_id();
    let platform_id = conversation.platform_id().to_string();
    let reply_to = msg.reply_to.clone();
    let session_key = conv_id.clone();

    let action = resolve_action(&msg.text);

    // Cancel bypasses inflight guard — must fire while a prompt is in-flight
    if matches!(action, Action::Cancel) {
        let _ = core.work_sender.send(Work::Cancel {
            conversation: session_key,
        });
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

    if !core.inflight.lock().unwrap().insert(session_key.clone()) {
        return;
    }

    let core = core.clone();

    match action {
        Action::Prompt { text } => {
            if let Some(authz) = &core.authz {
                match authz.can_use_bot(&msg.user, &conversation) {
                    Ok(true) => {},
                    _ => {
                        info!("Bot access denied for user {} in {}", msg.user, conv_id);
                        core.inflight.lock().unwrap().remove(&session_key);
                        return;
                    },
                }
            }

            tokio::spawn(async move {
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
                        core.inflight.lock().unwrap().remove(&session_key);
                        return;
                    },
                };

                let (reply_tx, reply_rx) = oneshot::channel();
                let (progress_tx, mut progress_rx) = mpsc::unbounded_channel::<String>();
                let (approval_channel, approval_thread) = match &conversation {
                    Conversation::Dm { channel, .. } => (channel.clone(), reply_to.clone()),
                    Conversation::Channel(id) => (id.clone(), reply_to.clone()),
                    Conversation::Thread { channel, thread_ts } => (channel.clone(), Some(thread_ts.clone())),
                };
                let _ = core.work_sender.send(Work::Prompt {
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
                {
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
                    });
                }

                let reply_text = reply_rx.await.unwrap_or("Error".into());
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
                        text: reply_text,
                    })
                    .await;
                core.inflight.lock().unwrap().remove(&session_key);
            });
        },
        action => {
            let user = msg.user.clone();
            tokio::spawn(async move {
                match check_authz(&core.authz, |a| a.can_use_bot(&user, &conversation)) {
                    Ok(true) => {},
                    _ => {
                        core.inflight.lock().unwrap().remove(&session_key);
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
                                "*Commands:*\n",
                                "`!help` — show this message\n",
                                "`!new` — new session\n",
                                "`!agent <name>` — switch agent\n",
                                "`!model <name>` — switch model\n",
                                "`!status` — current agent/model/session\n",
                                "`!agents` — list available agents\n",
                                "`!cancel` — cancel current request",
                            )
                            .into(),
                        )
                        .await;
                    },
                    Action::NewSession => {
                        let (tx, rx) = oneshot::channel();
                        let _ = core.work_sender.send(Work::NewSession {
                            conversation: session_key.clone(),
                            reply_tx: tx,
                        });
                        if let Ok(m) = rx.await {
                            send(m).await;
                        }
                    },
                    Action::SetAgent { name } => {
                        match check_authz(&core.authz, |a| a.can_use_agent(&user, &name, &authz_scope)) {
                            Ok(true) => {
                                let (tx, rx) = oneshot::channel();
                                let _ = core.work_sender.send(Work::SetMode {
                                    conversation: session_key.clone(),
                                    mode: name,
                                    reply_tx: tx,
                                });
                                if let Ok(m) = rx.await {
                                    send(m).await;
                                }
                            },
                            Ok(false) => {
                                send(format!("❌ Unauthorized: You don't have access to agent '{name}'")).await
                            },
                            Err(e) => send(format!("❌ Authorization error: {e}")).await,
                        }
                    },
                    Action::SetModel { name } => match check_authz(&core.authz, |a| a.can_use_model(&user, &name)) {
                        Ok(true) => {
                            let (tx, rx) = oneshot::channel();
                            let _ = core.work_sender.send(Work::SetModel {
                                conversation: session_key.clone(),
                                model: name,
                                reply_tx: tx,
                            });
                            if let Ok(m) = rx.await {
                                send(m).await;
                            }
                        },
                        Ok(false) => send(format!("❌ Unauthorized: You don't have access to model '{name}'")).await,
                        Err(e) => send(format!("❌ Authorization error: {e}")).await,
                    },
                    Action::Status => {
                        let (tx, rx) = oneshot::channel();
                        let _ = core.work_sender.send(Work::Status {
                            conversation: session_key.clone(),
                            reply_tx: tx,
                        });
                        if let Ok(m) = rx.await {
                            send(m).await;
                        }
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
                core.inflight.lock().unwrap().remove(&session_key);
            });
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_text_is_prompt() {
        assert_eq!(resolve_action("hello"), Action::Prompt { text: "hello".into() });
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
}
