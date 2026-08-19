use std::collections::HashMap;
use std::sync::{
    Arc,
    Mutex,
};
use std::time::Duration;

use slack_morphism::prelude::*;
use tokio::sync::oneshot;
use tracing::{
    info,
    warn,
};

use super::SlackState;
use super::delivery::{
    correlation_id,
    log_post_error,
    neutralize_slack_controls,
    post_request,
    render_message,
};
use super::feedback::record_feedback;
use crate::engine::acp::{
    ApprovalRequest,
    ApprovalResponse,
};
use crate::engine::coordinator::DedupeOutcome;
use crate::engine::core::{
    Frontend,
    Reply,
};

/// Pending approval: maps message timestamp → approval state.
pub type PendingApprovals = Arc<Mutex<HashMap<String, PendingApproval>>>;

const APPROVAL_REACTION_MISMATCH_EVENT: &str = "approval_reaction_mismatch";

pub struct PendingApproval {
    pub tool_name: String,
    pub options: Vec<(String, String)>,
    pub reply_tx: Option<oneshot::Sender<ApprovalResponse>>,
    /// Only the requester can approve a tool call visible in a shared channel.
    pub requester: String,
}

fn wrong_requester(pending: &PendingApprovals, ts: &str, reactor: &str) -> Option<String> {
    pending
        .lock()
        .unwrap()
        .get(ts)
        .map(|a| a.requester.clone())
        .filter(|requester| requester != reactor)
}

fn is_approval_reaction(emoji: &str) -> bool {
    matches!(emoji, "white_check_mark" | "unlock" | "x")
}

fn option_for_reaction<'a>(options: &'a [(String, String)], emoji: &str) -> Option<&'a (String, String)> {
    options.iter().find(|(id, _)| match emoji {
        "white_check_mark" => id.contains("allow_once") || id.contains("yes"),
        "unlock" => id.contains("allow_always") || id.contains("always"),
        "x" => id.contains("reject") || id.contains("no") || id.contains("deny"),
        _ => false,
    })
}

fn approval_reactions(options: &[(String, String)]) -> Vec<&'static str> {
    ["white_check_mark", "x", "unlock"]
        .into_iter()
        .filter(|emoji| option_for_reaction(options, emoji).is_some())
        .collect()
}

fn take_local_approval(pending: &PendingApprovals, ts: &str, reactor: &str, emoji: &str) -> Option<PendingApproval> {
    if !is_approval_reaction(emoji) {
        return None;
    }
    let mut approvals = pending.lock().unwrap();
    let approval = approvals.get(ts)?;
    if approval.requester != reactor || option_for_reaction(&approval.options, emoji).is_none() {
        return None;
    }
    approvals.remove(ts)
}

fn approval_outcome(approval: &PendingApproval, emoji: &str) -> Option<(ApprovalResponse, String, &'static str)> {
    let consequence = match emoji {
        "white_check_mark" => "Tool will run this one time only.",
        "unlock" => "Tool is now trusted for the rest of this session.",
        "x" => "Tool was blocked and will not run.",
        _ => return None,
    };
    let option = option_for_reaction(&approval.options, emoji)?;
    Some((
        ApprovalResponse::Selected(option.0.clone()),
        option.1.clone(),
        consequence,
    ))
}

fn expire_local_approval(pending: &PendingApprovals, ts: &str) -> Option<PendingApproval> {
    pending.lock().unwrap().remove(ts)
}

pub(super) async fn handle_reaction(
    reaction: SlackReactionAddedEvent,
    state: &SlackState,
    event_id: &str,
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
    tracing::debug!(
        emoji,
        ts,
        pending_count,
        "Reaction on message, checking pending approvals"
    );

    let has_local_approval = state.pending_approvals.lock().unwrap().contains_key(&ts);
    if has_local_approval {
        if !is_approval_reaction(emoji) {
            tracing::debug!(emoji, msg_ts = %ts, "Ignoring unsupported approval reaction");
            return Ok(());
        }
        if let Some(requester) = wrong_requester(&state.pending_approvals, &ts, &reactor) {
            info!(
                event = APPROVAL_REACTION_MISMATCH_EVENT,
                reactor,
                requester,
                msg_ts = %ts,
                "{}",
                APPROVAL_REACTION_MISMATCH_EVENT
            );
            return Ok(());
        }
        if !dedupe_reaction_event(state.core.coordinator.as_ref(), event_id).await? {
            tracing::debug!(event_id, "duplicate reaction event, dropping");
            return Ok(());
        }
        let Some(mut approval) = take_local_approval(&state.pending_approvals, &ts, &reactor, emoji) else {
            return Ok(());
        };
        let Some((response, label, consequence)) = approval_outcome(&approval, emoji) else {
            return Ok(());
        };
        info!(emoji, tool = %approval.tool_name, "Approval resolved");
        if let Some(reply_tx) = approval.reply_tx.take() {
            let _ = reply_tx.send(response);
        }

        if let Some(channel) = channel {
            let frontend = state.frontend.clone();
            let text = format!(
                "**Permission resolved**\n\n`{}`\n\n**Option:** {label}\n\n_{consequence}_",
                approval.tool_name.replace('`', "'")
            );
            tokio::spawn(async move {
                let _ = frontend
                    .send(Reply::Update {
                        conversation: channel.to_string(),
                        message_id: ts,
                        text,
                    })
                    .await;
            });
        }
        return Ok(());
    }

    let feedback_reaction = crate::engine::feedback::Reaction::from_slack(emoji);
    if !is_approval_reaction(emoji) && feedback_reaction.is_none() {
        return Ok(());
    }

    let approval_owner = lookup_approval_owner_with_retry(state.core.coordinator.as_ref(), &ts).await?;
    if let Some(peer) = approval_owner {
        if !is_approval_reaction(emoji) {
            tracing::debug!(emoji, msg_ts = %ts, "Ignoring feedback reaction on approval message");
            return Ok(());
        }
        let payload = crate::engine::coordinator::ForwardEvent {
            slack_event_json: forwarded_reaction_event(event_id, &reactor, emoji, &reaction.item, &ts),
        };
        match route_approval_reaction(
            state.core.coordinator.as_ref(),
            &state.own_task_id,
            &peer,
            event_id,
            forwarded,
            &payload,
        )
        .await?
        {
            ApprovalReactionRouting::Forwarded => {
                tracing::debug!(peer, msg_ts = %ts, "Forwarded reaction to approval owner");
            },
            ApprovalReactionRouting::Duplicate => {
                tracing::debug!(event_id, "duplicate reaction event, dropping");
            },
            ApprovalReactionRouting::Local => {},
        }
        return Ok(());
    }

    if !dedupe_reaction_event(state.core.coordinator.as_ref(), event_id).await? {
        tracing::debug!(event_id, "duplicate reaction event, dropping");
        return Ok(());
    }
    if let Some(reaction_kind) = feedback_reaction {
        let channel = channel.as_ref().map(|value| value.to_string()).unwrap_or_default();
        record_feedback(
            state,
            &reactor,
            &channel,
            msg.origin.thread_ts.as_ref().map(|thread| thread.0.as_str()),
            &ts,
            reaction_kind,
        )
        .await;
    }
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ApprovalReactionRouting {
    Local,
    Forwarded,
    Duplicate,
}

async fn route_approval_reaction(
    coordinator: &dyn crate::engine::coordinator::Coordinator,
    own_task_id: &str,
    peer: &str,
    event_id: &str,
    forwarded: bool,
    payload: &crate::engine::coordinator::ForwardEvent,
) -> anyhow::Result<ApprovalReactionRouting> {
    if !peer.is_empty() && peer != own_task_id {
        anyhow::ensure!(
            !forwarded,
            "forwarded approval reaction reached non-owner task; owner is {peer}"
        );
        crate::engine::core::forward_with_retry(coordinator, peer, payload).await?;
        return Ok(ApprovalReactionRouting::Forwarded);
    }
    if dedupe_reaction_event(coordinator, event_id).await? {
        Ok(ApprovalReactionRouting::Local)
    } else {
        Ok(ApprovalReactionRouting::Duplicate)
    }
}

async fn dedupe_reaction_event(
    coordinator: &dyn crate::engine::coordinator::Coordinator,
    event_id: &str,
) -> anyhow::Result<bool> {
    reaction_dedupe_result(event_id, coordinator.dedupe_event_outcome(event_id).await)
}

fn reaction_dedupe_result(event_id: &str, outcome: DedupeOutcome) -> anyhow::Result<bool> {
    match outcome {
        DedupeOutcome::Accepted { .. } => Ok(true),
        DedupeOutcome::Duplicate => Ok(false),
        DedupeOutcome::Unavailable => {
            anyhow::bail!("coordinator unavailable while recording reaction event {event_id}")
        },
    }
}

fn forwarded_reaction_event(
    event_id: &str,
    reactor: &str,
    emoji: &str,
    item: &SlackReactionsItem,
    event_ts: &str,
) -> serde_json::Value {
    serde_json::json!({
        "token": "",
        "team_id": "",
        "api_app_id": "",
        "type": "event_callback",
        "event_id": event_id,
        "event_time": chrono::Utc::now().timestamp(),
        "authed_users": [],
        "event": {
            "type": "reaction_added",
            "user": reactor,
            "reaction": emoji,
            "item": item,
            "item_user": "",
            "event_ts": event_ts,
        },
    })
}

async fn lookup_approval_owner_with_retry(
    coordinator: &dyn crate::engine::coordinator::Coordinator,
    message_id: &str,
) -> anyhow::Result<Option<String>> {
    retry_approval_owner_lookup(message_id, || coordinator.lookup_approval_owner(message_id)).await
}

async fn retry_approval_owner_lookup<F, Fut>(message_id: &str, mut lookup: F) -> anyhow::Result<Option<String>>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = anyhow::Result<Option<String>>>,
{
    const ATTEMPTS: usize = 3;
    let mut delay = Duration::from_millis(50);
    for attempt in 1..=ATTEMPTS {
        match lookup().await {
            Ok(owner) => return Ok(owner),
            Err(error) if attempt < ATTEMPTS => {
                warn!(
                    %error,
                    message_id,
                    attempt,
                    "approval owner lookup failed; retrying"
                );
                tokio::time::sleep(delay).await;
                delay *= 2;
            },
            Err(error) => return Err(error.context("approval owner lookup failed after retries")),
        }
    }
    unreachable!("retry loop always returns")
}

// Outlives ACP's 600-second approval wait with recovery margin.
const APPROVAL_DDB_TTL: Duration = Duration::from_secs(1800);
const APPROVAL_TIMEOUT: Duration = Duration::from_secs(600);

fn conv_id_for_approval(channel: &str, thread_ts: Option<&str>) -> String {
    match thread_ts {
        Some(ts) => format!("thread:{channel}:{ts}"),
        None => format!("channel:{channel}"),
    }
}

fn approval_value(value: &str) -> String {
    let encoded = serde_json::to_string(value).expect("serializing a string cannot fail");
    neutralize_slack_controls(&encoded).replace('`', "'")
}

fn approval_message(req: &ApprovalRequest, options_text: &str, reaction_instructions: &str) -> String {
    let details = req.github_search.as_ref().map_or_else(String::new, |search| {
        format!(
            "\n\n**Repository:** `{}`\n**Query:** `{}`",
            approval_value(&search.repository),
            approval_value(&search.query),
        )
    });
    format!(
        "**Permission request**\n\n`{}`{details}\n\nOptions: {options_text}\n\nOnly REQUESTER_MENTION can approve. React with {reaction_instructions}.",
        neutralize_slack_controls(&req.tool_name.replace('`', "'")),
    )
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
                .map(|(_, label)| neutralize_slack_controls(&label.replace('`', "'")))
                .collect::<Vec<_>>()
                .join(" / ");
            let requester: String = req.slack_user_id.chars().filter(char::is_ascii_alphanumeric).collect();
            let requester = format!("<@{requester}>");
            let reactions = approval_reactions(&req.options);
            let reaction_instructions = reactions
                .iter()
                .filter_map(|emoji| match *emoji {
                    "white_check_mark" => Some("✅ to allow once"),
                    "x" => Some("❌ to deny"),
                    "unlock" => Some("🔓 to trust for this session"),
                    _ => None,
                })
                .collect::<Vec<_>>()
                .join(", ");
            let text = approval_message(&req, &options_text, &reaction_instructions);
            let mut rendered = render_message(&text);
            let mut content = rendered.chunks.remove(0);
            if let Some(blocks) = content.blocks.as_mut()
                && let Some(SlackBlock::Markdown(block)) = blocks.first_mut()
            {
                block.text = block.text.replace("REQUESTER_MENTION", &requester);
            }

            let session = client.open_session(&bot_token);
            let channel: SlackChannelId = req.channel.clone().into();
            let post = post_request(channel.clone(), content, req.thread_ts.clone());

            let msg_ts = match session.chat_post_message(&post).await {
                Ok(resp) => resp.ts.to_string(),
                Err(e) => {
                    let reference = correlation_id();
                    log_post_error(&e.to_string(), &reference);
                    let _ = req.reply_tx.send(ApprovalResponse::Denied);
                    continue;
                },
            };

            let ts: SlackTs = msg_ts.clone().into();
            for emoji in reactions {
                let _ = session
                    .reactions_add(&SlackApiReactionsAddRequest::new(
                        channel.clone(),
                        SlackReactionName(emoji.into()),
                        ts.clone(),
                    ))
                    .await;
            }

            // Register ownership first so peer lookups cannot race the local insert.
            let conv_id = conv_id_for_approval(&req.channel, req.thread_ts.as_deref());
            if let Err(e) = coordinator.register_approval(&msg_ts, &conv_id, APPROVAL_DDB_TTL).await {
                warn!(error = %e, msg_ts, "register_approval failed; falling back to local-only routing");
            }

            pending.lock().unwrap().insert(msg_ts.clone(), PendingApproval {
                tool_name: req.tool_name,
                options: req.options,
                reply_tx: Some(req.reply_tx),
                requester: req.slack_user_id,
            });
            tracing::debug!(
                msg_ts,
                pending_count = pending.lock().unwrap().len(),
                "Registered pending approval"
            );

            let pending_for_timeout = pending.clone();
            let client_for_timeout = client.clone();
            let token_for_timeout = bot_token.clone();
            let channel_for_timeout = channel.clone();
            tokio::spawn(async move {
                tokio::time::sleep(APPROVAL_TIMEOUT).await;
                let Some(expired) = expire_local_approval(&pending_for_timeout, &msg_ts) else {
                    return;
                };
                drop(expired);
                let mut rendered = render_message(
                    "**Permission request expired**\n\nRun the request again if you still want to approve it.",
                );
                let content = rendered.chunks.remove(0);
                let session = client_for_timeout.open_session(&token_for_timeout);
                if let Err(error) = session
                    .chat_update(&SlackApiChatUpdateRequest::new(
                        channel_for_timeout,
                        content,
                        msg_ts.clone().into(),
                    ))
                    .await
                {
                    warn!(%error, msg_ts, "Failed to mark approval as expired");
                }
            });
        }
    });
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{
        AtomicU64,
        Ordering,
    };

    use super::*;

    fn github_approval_request(query: &str, repository: &str) -> ApprovalRequest {
        let (reply_tx, _reply_rx) = oneshot::channel();
        ApprovalRequest {
            tool_name: "Search GitHub issues".to_string(),
            tool_call_id: "tc-1".to_string(),
            github_search: Some(crate::engine::acp::GitHubSearchApproval {
                query: query.to_string(),
                repository: repository.to_string(),
            }),
            options: vec![("allow_once".to_string(), "Allow once".to_string())],
            channel: "C1".to_string(),
            thread_ts: Some("1.0".to_string()),
            slack_user_id: "U_REQUESTER".to_string(),
            reply_tx,
        }
    }

    #[test]
    fn github_approval_message_contains_complete_neutralized_query_and_repository() {
        let request = github_approval_request(
            "server <@U_BYSTANDER> shut down\n```unexpected```",
            "kiro-team/kiro-cli",
        );
        let text = approval_message(&request, "Allow once", "yes to allow");

        assert!(text.contains("**Repository:** `\"kiro-team/kiro-cli\"`"));
        assert!(text.contains("server &lt;@U_BYSTANDER> shut down\\n'''unexpected'''"));
        assert!(!text.contains("<@U_BYSTANDER>"));

        let max_query = "q".repeat(crate::engine::acp::MAX_GITHUB_ISSUE_QUERY_CHARS);
        let max_repository = "r".repeat(crate::engine::acp::MAX_GITHUB_ISSUE_REPOSITORY_CHARS);
        let text = approval_message(
            &github_approval_request(&max_query, &max_repository),
            "Allow once",
            "yes to allow",
        );
        assert!(text.contains(&format!("**Repository:** `\"{max_repository}\"`")));
        assert!(text.contains(&format!("**Query:** `\"{max_query}\"`")));
    }

    fn reaction_event(event_id: &str) -> SlackPushEventCallback {
        serde_json::from_value(serde_json::json!({
            "team_id": "T1",
            "api_app_id": "A1",
            "event_id": event_id,
            "event_time": 1,
            "event": {
                "type": "reaction_added",
                "user": "U1",
                "reaction": "+1",
                "item": {
                    "type": "message",
                    "channel": "C1",
                    "ts": "1.0"
                },
                "item_user": "UBOT",
                "event_ts": "2.0"
            }
        }))
        .unwrap()
    }

    #[test]
    fn forwarded_reactions_preserve_the_outer_slack_event_id() {
        let event = reaction_event("EvOuter");
        let SlackEventCallbackBody::ReactionAdded(reaction) = event.event else {
            panic!("expected reaction event");
        };

        let forwarded = forwarded_reaction_event(
            &event.event_id.0,
            &reaction.user.0,
            &reaction.reaction.0,
            &reaction.item,
            "2.0",
        );

        assert_eq!(forwarded["event_id"], "EvOuter");
    }

    #[tokio::test]
    async fn approval_forwarding_defers_dedup_to_the_receiving_task() {
        use crate::engine::coordinator::{
            Coordinator,
            ForwardEvent,
            InMemoryClusterCoordinator,
        };

        let source = InMemoryClusterCoordinator::new("task-source", chrono::Duration::minutes(5));
        let receiver = source.sibling("task-owner");
        let payload = ForwardEvent {
            slack_event_json: serde_json::json!({"event_id": "EvApproval"}),
        };

        let forwarded = route_approval_reaction(&source, "task-source", "task-owner", "EvApproval", false, &payload)
            .await
            .unwrap();
        assert_eq!(forwarded, ApprovalReactionRouting::Forwarded);

        let accepted = route_approval_reaction(&receiver, "task-owner", "task-owner", "EvApproval", true, &payload)
            .await
            .unwrap();
        let duplicate = route_approval_reaction(&receiver, "task-owner", "task-owner", "EvApproval", true, &payload)
            .await
            .unwrap();

        assert_eq!(accepted, ApprovalReactionRouting::Local);
        assert_eq!(duplicate, ApprovalReactionRouting::Duplicate);
        assert!(
            !source.dedupe_event("EvApproval").await,
            "the receiver, not the forwarding source, must commit dedup"
        );
    }

    #[test]
    fn reaction_dedup_backend_failure_is_rejected_for_retry() {
        let error = reaction_dedupe_result("EvUnavailable", DedupeOutcome::Unavailable).unwrap_err();

        assert!(error.to_string().contains("coordinator unavailable"));
        assert!(!reaction_dedupe_result("EvDuplicate", DedupeOutcome::Duplicate).unwrap());
    }

    #[tokio::test]
    async fn transient_approval_owner_lookup_is_retried_without_becoming_none() {
        let attempts = AtomicU64::new(0);
        let result = retry_approval_owner_lookup("1.0", || {
            let attempt = attempts.fetch_add(1, Ordering::Relaxed);
            std::future::ready(if attempt < 2 {
                Err(anyhow::anyhow!("temporary lookup failure"))
            } else {
                Ok(Some("task-owner".to_string()))
            })
        })
        .await
        .unwrap();

        assert_eq!(result.as_deref(), Some("task-owner"));
        assert_eq!(attempts.load(Ordering::Relaxed), 3);
    }

    fn pending_with_requester(ts: &str, requester: &str) -> PendingApprovals {
        let (tx, _rx) = oneshot::channel();
        let map = HashMap::from([(ts.to_string(), PendingApproval {
            tool_name: "execute_bash".to_string(),
            options: vec![("allow_once".to_string(), "Allow once".to_string())],
            reply_tx: Some(tx),
            requester: requester.to_string(),
        })]);
        Arc::new(Mutex::new(map))
    }

    #[test]
    fn only_the_requester_can_approve() {
        let pending = pending_with_requester("1700000000.1", "U_REQUESTER");

        assert_eq!(
            wrong_requester(&pending, "1700000000.1", "U_BYSTANDER"),
            Some("U_REQUESTER".to_string()),
            "a bystander's reaction must be rejected"
        );
        assert!(
            pending.lock().unwrap().contains_key("1700000000.1"),
            "rejecting a bystander must leave the approval pending for the real requester"
        );
        assert_eq!(
            wrong_requester(&pending, "1700000000.1", "U_REQUESTER"),
            None,
            "the requester's own reaction must be accepted"
        );
        assert_eq!(
            wrong_requester(&pending, "9999999999.9", "U_ANYONE"),
            None,
            "unknown ts falls through to the cross-task peer lookup"
        );
    }

    #[test]
    fn unsupported_reactions_leave_approvals_pending() {
        let pending = pending_with_requester("1700000000.1", "U_REQUESTER");

        assert!(take_local_approval(&pending, "1700000000.1", "U_REQUESTER", "heart").is_none());
        assert!(pending.lock().unwrap().contains_key("1700000000.1"));
    }

    #[test]
    fn approval_mismatch_log_event_token_is_stable() {
        assert_eq!(APPROVAL_REACTION_MISMATCH_EVENT, "approval_reaction_mismatch");
    }

    #[test]
    fn a_different_user_cannot_consume_an_approval() {
        let pending = pending_with_requester("1700000000.1", "U_REQUESTER");

        assert!(take_local_approval(&pending, "1700000000.1", "U_BYSTANDER", "white_check_mark").is_none());
        assert!(pending.lock().unwrap().contains_key("1700000000.1"));
    }

    #[test]
    fn supported_reactions_consume_the_approval() {
        let pending = pending_with_requester("1700000000.1", "U_REQUESTER");

        let approval = take_local_approval(&pending, "1700000000.1", "U_REQUESTER", "white_check_mark").unwrap();
        let (response, label, _) = approval_outcome(&approval, "white_check_mark").unwrap();

        assert!(matches!(response, ApprovalResponse::Selected(ref id) if id == "allow_once"));
        assert_eq!(label, "Allow once");
        assert!(!pending.lock().unwrap().contains_key("1700000000.1"));
    }

    #[test]
    fn reactions_are_derived_from_the_offered_options() {
        let fresh_write = vec![
            ("allow_once".into(), "Allow once".into()),
            ("reject_once".into(), "Reject".into()),
        ];
        assert_eq!(approval_reactions(&fresh_write), vec!["white_check_mark", "x"]);

        let reusable = vec![
            ("allow_once".into(), "Allow once".into()),
            ("allow_always".into(), "Always".into()),
            ("reject_once".into(), "Reject".into()),
        ];
        assert_eq!(approval_reactions(&reusable), vec!["white_check_mark", "x", "unlock"]);
    }

    #[test]
    fn unlock_cannot_consume_a_fresh_write_approval() {
        let pending = pending_with_requester("1700000000.1", "U_REQUESTER");

        assert!(take_local_approval(&pending, "1700000000.1", "U_REQUESTER", "unlock").is_none());
        assert!(pending.lock().unwrap().contains_key("1700000000.1"));
    }

    #[test]
    fn expired_approvals_are_removed() {
        let (reply_tx, mut reply_rx) = oneshot::channel();
        let pending = Arc::new(Mutex::new(HashMap::from([(
            "1700000000.1".to_string(),
            PendingApproval {
                tool_name: "execute_bash".into(),
                options: Vec::new(),
                reply_tx: Some(reply_tx),
                requester: "U_REQUESTER".into(),
            },
        )])));

        drop(expire_local_approval(&pending, "1700000000.1"));
        assert!(expire_local_approval(&pending, "1700000000.1").is_none());
        assert!(pending.lock().unwrap().is_empty());
        assert!(matches!(
            reply_rx.try_recv(),
            Err(tokio::sync::oneshot::error::TryRecvError::Closed)
        ));
    }
}
