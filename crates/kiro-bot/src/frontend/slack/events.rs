#[cfg(test)]
use std::collections::HashMap;
use std::collections::VecDeque;
use std::sync::LazyLock;
use std::sync::atomic::{
    AtomicBool,
    Ordering,
};

use regex::Regex;
use slack_morphism::errors::SlackClientError;
use slack_morphism::prelude::*;
use tracing::{
    error,
    info,
    warn,
};

use super::approvals::handle_reaction;
use super::attachments::attachment_preparation;
use super::delivery::context_message_text;
#[cfg(test)]
use super::delivery::render_message;
use super::{
    SlackFrontend,
    SlackState,
};
use crate::engine::coordinator::{
    Turn,
    TurnRole,
};
use crate::engine::core::{
    Conversation,
    DispatchEnvelope,
    DispatchReceipt,
    Frontend,
    IncomingMessage,
    determine_reply_location,
    dispatch_with_receipt,
};
use crate::engine::user_map::UserMap;

static WARNED_GROUPS_HISTORY_SCOPE: AtomicBool = AtomicBool::new(false);
static MENTION_PATTERN: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"<@[A-Z0-9]+>").unwrap());

#[derive(Default)]
pub(super) struct ThreadSnapshot {
    owned_by_bot: bool,
    pub(super) context: Vec<String>,
}

/// A `conversations.replies` page, deserialized with `blocks` left as raw JSON.
///
/// `slack-morphism`'s `SlackHistoryMessage` cannot be used here: its `blocks`
/// are a `SlackBlock` enum with no catch-all variant, and the bot posts blocks
/// that enum cannot represent (`plan`, and `context_actions` carrying
/// `feedback_buttons`). Because the enum is internally tagged, one unrepresentable
/// block fails the entire page — so the bot could not read back its own threads.
#[derive(Debug, serde::Deserialize)]
struct ThreadRepliesPage {
    messages: Vec<ThreadHistoryMessage>,
    response_metadata: Option<ThreadRepliesMetadata>,
}

#[derive(Debug, serde::Deserialize)]
struct ThreadRepliesMetadata {
    next_cursor: Option<String>,
}

#[derive(Debug, serde::Deserialize)]
struct ThreadHistoryMessage {
    ts: String,
    user: Option<String>,
    bot_id: Option<String>,
    text: Option<String>,
    blocks: Option<Vec<serde_json::Value>>,
}

fn transcript_thread_snapshot(turns: Vec<Turn>) -> ThreadSnapshot {
    ThreadSnapshot {
        owned_by_bot: !turns.is_empty(),
        context: turns
            .into_iter()
            .map(|turn| {
                let role = match turn.role {
                    TurnRole::User => "user",
                    TurnRole::Assistant => "Kiro",
                };
                format!("{role}: {}", turn.text)
            })
            .collect(),
    }
}

struct ThreadAccumulator {
    root: Option<String>,
    replies: VecDeque<String>,
    context_limit: usize,
    owned_by_bot: bool,
}

impl ThreadAccumulator {
    fn new(context_limit: u16) -> Self {
        Self {
            root: None,
            replies: VecDeque::with_capacity(usize::from(context_limit)),
            context_limit: usize::from(context_limit),
            owned_by_bot: false,
        }
    }

    fn absorb(
        &mut self,
        message: &ThreadHistoryMessage,
        thread_ts: &str,
        bot_user_ids: &[&str],
        bot_id: &str,
        user_map: &UserMap,
    ) {
        self.owned_by_bot |= message.user.as_ref().is_some_and(|user| {
            bot_user_ids
                .iter()
                .any(|candidate| !candidate.is_empty() && candidate == user)
        }) || message
            .bot_id
            .as_ref()
            .is_some_and(|candidate| !bot_id.is_empty() && candidate == bot_id);

        if self.context_limit == 0 {
            return;
        }
        let Some(text) = context_message_text(message.text.as_deref(), message.blocks.as_deref()) else {
            return;
        };
        let user = message
            .user
            .as_ref()
            .map(|user| user_map.resolve(user).to_string())
            .unwrap_or_else(|| "bot".into());
        let entry = format!("{user}: {text}");
        if message.ts == thread_ts {
            self.root = Some(entry);
            return;
        }
        self.replies.push_back(entry);
        while self.replies.len() > self.context_limit {
            self.replies.pop_front();
        }
    }

    fn finish(mut self) -> ThreadSnapshot {
        let mut context = Vec::with_capacity(self.context_limit);
        if self.context_limit > 0
            && let Some(root) = self.root
        {
            context.push(root);
        }
        let reply_limit = self.context_limit.saturating_sub(context.len());
        while self.replies.len() > reply_limit {
            self.replies.pop_front();
        }
        context.extend(self.replies);
        ThreadSnapshot {
            owned_by_bot: self.owned_by_bot,
            context,
        }
    }
}

fn message_is_directed(is_dm: bool, mentioned: bool, thread_owned_by_bot: bool) -> bool {
    is_dm || mentioned || thread_owned_by_bot
}

pub(super) fn log_thread_context_error(error: &SlackClientError) {
    let error_text = error.to_string();
    if error_text.contains("missing_scope") {
        if !WARNED_GROUPS_HISTORY_SCOPE.swap(true, Ordering::Relaxed) {
            error!(
                required_scope = "groups:history",
                slack_error = error_text,
                "Slack private-channel thread context is unavailable; add the bot scope and reinstall the app"
            );
        }
    } else {
        warn!("Thread context fetch failed: {error}");
    }
}

const THREAD_HISTORY_PAGE_SIZE: u16 = 100;

fn thread_replies_params(
    channel: &str,
    thread_ts: &str,
    latest: &str,
    cursor: Option<&str>,
) -> Vec<(&'static str, Option<String>)> {
    vec![
        ("channel", Some(channel.to_string())),
        ("ts", Some(thread_ts.to_string())),
        ("cursor", cursor.map(str::to_string)),
        ("limit", Some(THREAD_HISTORY_PAGE_SIZE.to_string())),
        ("inclusive", Some(false.to_string())),
        ("latest", Some(latest.to_string())),
    ]
}

impl SlackFrontend {
    pub(super) fn mark_seen(&self, conversation: &str, before: &str) {
        self.last_seen
            .lock()
            .unwrap()
            .insert(conversation.to_string(), before.into());
    }

    pub(super) async fn fetch_thread_snapshot(
        &self,
        conversation: &str,
        before: &str,
        thread_ts: &str,
        bot_user_ids: &[&str],
        bot_id: &str,
    ) -> ClientResult<ThreadSnapshot> {
        let session = self.client.open_session(&self.bot_token);
        let mut cursor: Option<String> = None;
        let mut accumulator = ThreadAccumulator::new(self.conversation_history);

        loop {
            let params = thread_replies_params(conversation, thread_ts, before, cursor.as_deref());
            // Raw GET: `blocks` must stay untyped, or one unrepresentable block
            // fails the whole page.
            let response: ThreadRepliesPage = session
                .http_session_api
                .http_get("conversations.replies", &params, Some(&SLACK_TIER3_METHOD_CONFIG))
                .await?;
            for message in &response.messages {
                accumulator.absorb(message, thread_ts, bot_user_ids, bot_id, &self.user_map);
            }

            let next_cursor = response
                .response_metadata
                .and_then(|metadata| metadata.next_cursor)
                .filter(|next| !next.is_empty());
            if next_cursor.is_none() || next_cursor == cursor {
                break;
            }
            cursor = next_cursor;
        }

        Ok(accumulator.finish())
    }

    async fn recipient_team_id(&self, user_id: &str, fallback: &str) -> String {
        if let Some(team_id) = self.recipient_teams.lock().unwrap().get(user_id).cloned() {
            return team_id;
        }

        let session = self.client.open_session(&self.bot_token);
        let request = SlackApiUsersInfoRequest::new(user_id.into());
        match session.users_info(&request).await {
            Ok(response) => {
                let team_id = response
                    .user
                    .team_id
                    .map(|team_id| team_id.to_string())
                    .unwrap_or_else(|| fallback.to_string());
                self.recipient_teams
                    .lock()
                    .unwrap()
                    .insert(user_id.to_string(), team_id.clone());
                team_id
            },
            Err(error) => {
                warn!(%error, %user_id, "Unable to resolve Slack recipient team; using event workspace");
                fallback.to_string()
            },
        }
    }
}

async fn await_dispatch_routing(
    receipt: DispatchReceipt,
) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>> {
    match receipt.await {
        Ok(Ok(_)) => Ok(()),
        Ok(Err(error)) => Err(Box::new(std::io::Error::other(error))),
        Err(_) => Err(Box::new(std::io::Error::other(
            "dispatch task ended before routing was accepted",
        ))),
    }
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
    let event_id = event.event_id.0.clone();
    let team_id = event.team_id.0.clone();
    let envelope = Some(DispatchEnvelope {
        event_id: event_id.clone(),
        raw: serde_json::to_value(&event)?,
        forwarded,
    });
    match event.event {
        SlackEventCallbackBody::Message(msg) => handle_message(msg, state, &team_id, envelope).await?,
        SlackEventCallbackBody::AppMention(mention) => handle_mention(mention, state, &team_id, envelope).await?,
        SlackEventCallbackBody::ReactionAdded(reaction) => {
            handle_reaction(reaction, state, &event_id, forwarded).await?
        },
        _ => {},
    }
    Ok(())
}

async fn handle_message(
    msg: SlackMessageEvent,
    state: &SlackState,
    team_id: &str,
    envelope: Option<DispatchEnvelope>,
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

    // Channel mentions also arrive as app_mention with a different event id.
    // Let that event own dispatch to avoid duplicate replies.
    if mentioned && !is_dm {
        return Ok(());
    }

    let text = if mentioned {
        MENTION_PATTERN.replace_all(&text, "").trim().to_string()
    } else {
        text
    };

    let scope = if is_dm {
        "dm".to_string()
    } else {
        format!("channel:{channel}")
    };
    let thread_snapshot = if let Some(thread_ts) = msg.origin.thread_ts.as_ref() {
        let bot_user_ids = [state.bot_user_id.as_str(), state.member_id.as_str()];
        let snapshot = match state
            .frontend
            .fetch_thread_snapshot(
                channel.as_ref(),
                &msg.origin.ts.0,
                &thread_ts.0,
                &bot_user_ids,
                &state.bot_id,
            )
            .await
        {
            Ok(snapshot) => snapshot,
            Err(error) => {
                log_thread_context_error(&error);
                let conversation_id = Conversation::thread_session_id(channel.as_ref(), &thread_ts.0);
                match state
                    .core
                    .coordinator
                    .load_history(&conversation_id, usize::from(state.frontend.conversation_history))
                    .await
                {
                    Ok(turns) => transcript_thread_snapshot(turns),
                    Err(transcript_error) => {
                        warn!(
                            %transcript_error,
                            %conversation_id,
                            "Thread transcript fallback failed"
                        );
                        ThreadSnapshot::default()
                    },
                }
            },
        };
        state.frontend.mark_seen(channel.as_ref(), &msg.origin.ts.0);
        snapshot
    } else {
        ThreadSnapshot::default()
    };
    let directed = message_is_directed(is_dm, mentioned, thread_snapshot.owned_by_bot);
    if !state.core.response_policy.should_respond(&scope, directed, is_thread) {
        info!(scope, directed, is_thread, "Filtered by response policy");
        return Ok(());
    }
    let recipient_team_id = state.frontend.recipient_team_id(&raw_user_id, team_id).await;

    let reply_to = determine_reply_location(
        &state.core.response_policy,
        &scope,
        &text,
        msg.origin.thread_ts.as_ref().map(|ts| ts.0.as_str()),
        &msg.origin.ts.0,
    );

    let context = if is_thread {
        thread_snapshot.context
    } else {
        state
            .frontend
            .fetch_context(channel.as_ref(), &msg.origin.ts.0, None)
            .await
    };

    let conversation = if is_dm {
        Conversation::Dm {
            channel: channel.to_string(),
            user: user.clone(),
            thread_ts: msg
                .origin
                .thread_ts
                .as_ref()
                .map(|ts| ts.0.clone())
                .unwrap_or_else(|| msg.origin.ts.0.clone()),
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
    let prompt_preparation = attachment_preparation(
        msg.content.as_ref().and_then(|content| content.files.as_deref()),
        state.frontend.clone(),
        channel.to_string(),
        reply_to.clone().or_else(|| Some(msg.origin.ts.0.clone())),
        conversation.id(),
    );

    let receipt = dispatch_with_receipt(
        &state.core,
        IncomingMessage {
            user,
            slack_user_id: raw_user_id,
            slack_team_id: recipient_team_id,
            source_message_id: Some(msg.origin.ts.0),
            text,
            conversation,
            reply_to,
            directed,
            context,
            prompt_preparation,
            envelope,
        },
        state.frontend.clone(),
    );
    await_dispatch_routing(receipt).await
}

async fn handle_mention(
    mention: SlackAppMentionEvent,
    state: &SlackState,
    team_id: &str,
    envelope: Option<DispatchEnvelope>,
) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>> {
    if !state.user_id.is_empty() && mention.user.to_string() != state.user_id {
        return Ok(());
    }
    let user = state.frontend.user_map.resolve(mention.user.as_ref()).to_string();
    let text = MENTION_PATTERN
        .replace_all(mention.content.text.as_deref().unwrap_or(""), "")
        .trim()
        .to_string();

    let scope = format!("channel:{}", mention.channel);
    let reply_to = determine_reply_location(
        &state.core.response_policy,
        &scope,
        &text,
        mention.origin.thread_ts.as_ref().map(|ts| ts.0.as_str()),
        &mention.origin.ts.0,
    );

    let context = state
        .frontend
        .fetch_context(
            mention.channel.as_ref(),
            &mention.origin.ts.0,
            mention.origin.thread_ts.as_ref().map(|ts| ts.0.as_str()),
        )
        .await;
    let recipient_team_id = state.frontend.recipient_team_id(mention.user.as_ref(), team_id).await;

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
    let prompt_preparation = attachment_preparation(
        mention.content.files.as_deref(),
        state.frontend.clone(),
        mention.channel.to_string(),
        reply_to.clone().or_else(|| Some(mention.origin.ts.0.clone())),
        conversation.id(),
    );

    let receipt = dispatch_with_receipt(
        &state.core,
        IncomingMessage {
            user,
            slack_user_id: mention.user.to_string(),
            slack_team_id: recipient_team_id,
            source_message_id: Some(mention.origin.ts.0),
            text,
            conversation,
            reply_to,
            directed: true,
            context,
            prompt_preparation,
            envelope,
        },
        state.frontend.clone(),
    );
    await_dispatch_routing(receipt).await
}

#[cfg(test)]
mod tests {
    use super::*;

    fn history_message(
        ts: &str,
        user: Option<&str>,
        bot_id: Option<&str>,
        content: SlackMessageContent,
    ) -> ThreadHistoryMessage {
        serde_json::from_value(serde_json::json!({
            "ts": ts,
            "user": user,
            "bot_id": bot_id,
            "text": content.text,
            "blocks": content.blocks,
        }))
        .unwrap()
    }

    fn text_history_message(ts: &str, user: &str, text: &str) -> ThreadHistoryMessage {
        history_message(
            ts,
            Some(user),
            None,
            SlackMessageContent::new().with_text(text.to_string()),
        )
    }

    #[test]
    fn unmentioned_messages_are_directed_only_inside_kiro_owned_threads() {
        assert!(message_is_directed(false, false, true));
        assert!(!message_is_directed(false, false, false));
        assert!(message_is_directed(false, true, false));
        assert!(message_is_directed(true, false, false));
    }

    #[test]
    fn durable_transcript_recovers_thread_ownership_and_context() {
        let snapshot = transcript_thread_snapshot(vec![
            Turn {
                role: TurnRole::User,
                text: "How does ACP work?".into(),
                ts: chrono::Utc::now(),
                chunk_ids: Vec::new(),
            },
            Turn {
                role: TurnRole::Assistant,
                text: "It uses JSON-RPC over stdio.".into(),
                ts: chrono::Utc::now(),
                chunk_ids: vec!["docs/acp.md".into()],
            },
        ]);

        assert!(snapshot.owned_by_bot);
        assert_eq!(snapshot.context, vec![
            "user: How does ACP work?",
            "Kiro: It uses JSON-RPC over stdio."
        ]);
    }

    #[tokio::test]
    async fn durable_transcript_fallback_reads_the_dm_thread_session() {
        use crate::engine::coordinator::{
            Coordinator,
            NoopCoordinator,
        };

        let coordinator = NoopCoordinator::default();
        let conversation = Conversation::Dm {
            channel: "D123".into(),
            user: "alice".into(),
            thread_ts: "1700000000.1".into(),
        };
        coordinator
            .append_turn(&conversation.id(), Turn {
                role: TurnRole::Assistant,
                text: "Thread-scoped answer".into(),
                ts: chrono::Utc::now(),
                chunk_ids: Vec::new(),
            })
            .await
            .unwrap();

        let fallback_key = Conversation::thread_session_id("D123", "1700000000.1");
        let turns = coordinator.load_history(&fallback_key, 10).await.unwrap();
        let snapshot = transcript_thread_snapshot(turns);

        assert_eq!(fallback_key, conversation.id());
        assert!(snapshot.owned_by_bot);
        assert_eq!(snapshot.context, vec!["Kiro: Thread-scoped answer"]);
    }

    #[test]
    fn thread_snapshot_integrates_ownership_recent_history_and_rendered_sources() {
        let users = UserMap::from_map(HashMap::from([
            ("U_ALICE".to_string(), "alice".to_string()),
            ("U_KIRO".to_string(), "Kiro".to_string()),
        ]));
        let root = text_history_message("100.0", "U_ALICE", "<@U_KIRO> How does ACP work?");
        let old_reply = text_history_message("101.0", "U_ALICE", "Old detail");
        let bot_reply = history_message(
            "102.0",
            Some("U_KIRO"),
            Some("B_KIRO"),
            render_message("ACP connects the frontend and agent.\n\nSources: `crates/chat-cli-v2/src/lib.rs:1`")
                .chunks
                .remove(0),
        );
        let recent_reply = text_history_message("103.0", "U_ALICE", "Can you expand?");
        let newest_reply = text_history_message("104.0", "U_ALICE", "Specifically for V2.");
        let mut accumulator = ThreadAccumulator::new(4);

        for message in [&root, &old_reply] {
            accumulator.absorb(message, "100.0", &["U_KIRO"], "B_KIRO", &users);
        }
        for message in [&bot_reply, &recent_reply, &newest_reply] {
            accumulator.absorb(message, "100.0", &["U_KIRO"], "B_KIRO", &users);
        }
        let snapshot = accumulator.finish();

        assert!(snapshot.owned_by_bot);
        assert_eq!(snapshot.context.len(), 4);
        assert_eq!(snapshot.context[0], "alice: <@U_KIRO> How does ACP work?");
        assert!(snapshot.context[1].starts_with("Kiro: ACP connects the frontend and agent."));
        assert!(snapshot.context[1].contains("Sources: `crates/chat-cli-v2/src/lib.rs:1`"));
        assert_eq!(snapshot.context[2], "alice: Can you expand?");
        assert_eq!(snapshot.context[3], "alice: Specifically for V2.");
        assert!(!snapshot.context.iter().any(|entry| entry.contains("Old detail")));
    }

    #[test]
    fn arbitrary_thread_snapshot_is_not_owned_by_kiro() {
        let users = UserMap::empty();
        let messages = [
            text_history_message("100.0", "U_ALICE", "Unrelated root"),
            history_message(
                "101.0",
                Some("U_OTHER"),
                Some("B_OTHER"),
                SlackMessageContent::new().with_text("Another app replied".to_string()),
            ),
        ];
        let mut accumulator = ThreadAccumulator::new(10);
        for message in &messages {
            accumulator.absorb(message, "100.0", &["U_KIRO"], "B_KIRO", &users);
        }

        let snapshot = accumulator.finish();

        assert!(!snapshot.owned_by_bot);
        assert!(!message_is_directed(false, false, snapshot.owned_by_bot));
    }

    #[test]
    fn bot_id_alone_marks_a_thread_as_kiro_owned() {
        let message = history_message(
            "100.0",
            None,
            Some("B_KIRO"),
            SlackMessageContent::new().with_text("Kiro reply".to_string()),
        );
        let mut accumulator = ThreadAccumulator::new(10);

        accumulator.absorb(&message, "99.0", &[], "B_KIRO", &UserMap::empty());

        assert!(accumulator.finish().owned_by_bot);
    }

    /// Captured real `conversations.replies` wire payload for a thread whose bot
    /// reply carries the live progress card. Reading it back must preserve the
    /// answer and skip the card chrome.
    #[test]
    fn live_card_thread_preserves_complete_rich_text_in_order() {
        let response: ThreadRepliesPage = serde_json::from_str(include_str!("fixtures/live_card_thread.json")).unwrap();
        let users = UserMap::from_map(HashMap::from([
            ("U_ALICE".to_string(), "alice".to_string()),
            ("U_KIRO".to_string(), "Kiro".to_string()),
        ]));
        let mut accumulator = ThreadAccumulator::new(10);

        for message in &response.messages {
            accumulator.absorb(message, "1700000000.1", &["U_KIRO"], "B_KIRO", &users);
        }
        let snapshot = accumulator.finish();

        assert!(snapshot.owned_by_bot);
        assert_eq!(snapshot.context, vec![
            "alice: <@U_KIRO> Which command runs the TUI end-to-end suite?",
            concat!(
                "Kiro: The command is:\n",
                "`bun run test:e2e`\n\n",
                "It builds the Rust binary first and exercises the terminal harness.\n",
                "Source: <https://github.com/kiro-team/kiro-cli/blob/",
                "fc05a6803a4122c9170cf6bc809b9ee7786cfabb/docs/testing.md|docs/testing.md>.\n",
                "AI-generated; verify before acting."
            ),
            "alice: Does that build Rust first?"
        ]);
        // The rich_text answer was recovered, so the short accessibility fallback
        // must not also be spliced in.
        assert!(
            !snapshot
                .context
                .iter()
                .any(|entry| entry.contains("Short accessibility fallback"))
        );
    }

    /// Guards the reason `ThreadRepliesPage` exists. If someone reverts the read
    /// path to `slack-morphism`'s typed model, this documents why the whole page
    /// is lost: `SlackBlock` is internally tagged with no catch-all, so a single
    /// block it cannot represent fails every message in the response.
    #[test]
    fn typed_slack_blocks_cannot_parse_the_cards_the_bot_writes() {
        let fixture = include_str!("fixtures/live_card_thread.json");
        let raw: serde_json::Value = serde_json::from_str(fixture).unwrap();
        let bot_reply = &raw["messages"][1];

        let typed = serde_json::from_value::<SlackHistoryMessage>(bot_reply.clone());
        let error = typed
            .expect_err("plan block is not representable by SlackBlock")
            .to_string();
        assert!(error.contains("unknown variant `plan`"), "unexpected error: {error}");

        // `plan` is not the only offender: the feedback buttons Slack echoes back
        // use `positive_button`/`negative_button`, while the crate models
        // `positive`/`negative`.
        let mut without_plan = bot_reply.clone();
        without_plan["blocks"] = serde_json::Value::Array(
            bot_reply["blocks"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|block| block["type"] != "plan")
                .cloned()
                .collect(),
        );
        let error = serde_json::from_value::<SlackHistoryMessage>(without_plan)
            .expect_err("context_actions feedback buttons are also unrepresentable")
            .to_string();
        assert!(error.contains("missing field `positive`"), "unexpected error: {error}");

        // The untyped path used in production reads the same bytes successfully.
        let page: ThreadRepliesPage = serde_json::from_str(fixture).unwrap();
        assert_eq!(page.messages.len(), 3);
    }

    /// The feedback button is dead for the same reason, but one layer out of
    /// reach: `slack-morphism` decodes the entire socket-mode frame in a single
    /// `from_str::<SlackSocketModeEvent>` before dispatching to any callback, and
    /// the clicked message is echoed back inside that frame. So the frame fails,
    /// the interaction callback is never entered, and no amount of re-parsing
    /// *inside* the callback can recover the click — the fix has to move the
    /// boundary, not wrap it.
    ///
    /// Kept as an executable statement of the remaining bug so the next attempt
    /// starts from the real failure point instead of the symptom.
    ///
    /// `SlackSocketModeEvent` itself is not publicly reachable (it lives behind
    /// the crate's private `models` module), so this asserts on the `payload`
    /// type that the frame parse funnels through — the same failure, one field in.
    #[test]
    fn typed_interaction_payload_rejects_a_click_on_the_bot_own_card() {
        let frame: serde_json::Value =
            serde_json::from_str(include_str!("fixtures/live_card_feedback_interaction.json")).unwrap();

        let error = serde_json::from_value::<SlackInteractionEvent>(frame["payload"].clone())
            .expect_err("the echoed card blocks are not representable by SlackBlock")
            .to_string();

        assert!(error.contains("unknown variant `plan`"), "unexpected error: {error}");
    }

    #[test]
    fn thread_replies_params_carry_the_pagination_cursor() {
        assert_eq!(thread_replies_params("C123", "170.1", "170.9", None), vec![
            ("channel", Some("C123".into())),
            ("ts", Some("170.1".into())),
            ("cursor", None),
            ("limit", Some("100".into())),
            ("inclusive", Some("false".into())),
            ("latest", Some("170.9".into())),
        ]);
        assert_eq!(
            thread_replies_params("C123", "170.1", "170.9", Some("next-page"))[2],
            ("cursor", Some("next-page".into()))
        );
    }
}
