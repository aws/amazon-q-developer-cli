use slack_morphism::prelude::*;
use tracing::warn;

use super::SlackState;
use super::delivery::DELIVERIES;
use crate::engine::core::Conversation;
use crate::engine::feedback::{
    FeedbackRecord,
    Reaction,
};

pub(super) const FEEDBACK_ACTION_ID: &str = "kiro_response_feedback";
const POSITIVE_FEEDBACK_VALUE: &str = "positive_feedback";
const NEGATIVE_FEEDBACK_VALUE: &str = "negative_feedback";

#[derive(Debug, serde::Serialize)]
struct FeedbackText {
    #[serde(rename = "type")]
    kind: &'static str,
    text: &'static str,
}

#[derive(Debug, serde::Serialize)]
struct FeedbackButton {
    text: FeedbackText,
    value: &'static str,
    accessibility_label: &'static str,
}

#[derive(Debug, serde::Serialize)]
struct FeedbackButtons {
    #[serde(rename = "type")]
    kind: &'static str,
    action_id: &'static str,
    positive_button: FeedbackButton,
    negative_button: FeedbackButton,
}

#[derive(Debug, serde::Serialize)]
struct FeedbackBlock {
    #[serde(rename = "type")]
    kind: &'static str,
    elements: [FeedbackButtons; 1],
}

pub(super) fn feedback_block() -> serde_json::Value {
    serde_json::to_value(FeedbackBlock {
        kind: "context_actions",
        elements: [FeedbackButtons {
            kind: "feedback_buttons",
            action_id: FEEDBACK_ACTION_ID,
            positive_button: FeedbackButton {
                text: FeedbackText {
                    kind: "plain_text",
                    text: "Good response",
                },
                value: POSITIVE_FEEDBACK_VALUE,
                accessibility_label: "Submit positive feedback on this response",
            },
            negative_button: FeedbackButton {
                text: FeedbackText {
                    kind: "plain_text",
                    text: "Needs improvement",
                },
                value: NEGATIVE_FEEDBACK_VALUE,
                accessibility_label: "Submit negative feedback on this response",
            },
        }],
    })
    .expect("feedback block is serializable")
}

fn reaction_for_actions(actions: Option<&[SlackInteractionActionInfo]>) -> Option<Reaction> {
    let action = actions?
        .iter()
        .find(|action| action.action_type.0 == "feedback_buttons" && action.action_id.0 == FEEDBACK_ACTION_ID)?;
    match action.value.as_deref()? {
        POSITIVE_FEEDBACK_VALUE => Some(Reaction::ThumbsUp),
        NEGATIVE_FEEDBACK_VALUE => Some(Reaction::ThumbsDown),
        _ => None,
    }
}

fn bot_sender_matches(sender: &SlackMessageSender, bot_user_id: &str, bot_id: &str) -> bool {
    sender
        .user
        .as_ref()
        .is_some_and(|user| !bot_user_id.is_empty() && user.0 == bot_user_id)
        || sender
            .bot_id
            .as_ref()
            .is_some_and(|candidate| !bot_id.is_empty() && candidate.0 == bot_id)
}

fn citation_lookup_keys(channel: &str, thread_ts: Option<&str>, resolved_user: &str) -> Vec<String> {
    let mut keys = Vec::with_capacity(3);
    if let Some(thread_ts) = thread_ts {
        keys.push(Conversation::thread_session_id(channel, thread_ts));
    }
    keys.push(format!("dm:{resolved_user}"));
    keys.push(format!("channel:{channel}"));
    keys
}

#[derive(Debug, PartialEq, Eq)]
struct FeedbackSubmission {
    reactor: String,
    channel: String,
    thread_ts: Option<String>,
    message_ts: String,
    reaction: Reaction,
}

pub(super) async fn record_feedback(
    state: &SlackState,
    reactor: &str,
    channel: &str,
    thread_ts: Option<&str>,
    message_ts: &str,
    reaction: Reaction,
) {
    let Some(writer) = state.feedback_writer.as_ref() else {
        return;
    };

    let mut chunk_ids = { DELIVERIES.lock().unwrap().citations(message_ts) };
    if chunk_ids.is_empty() {
        let resolved_user = state.frontend.user_map.resolve(reactor);
        for conversation in citation_lookup_keys(channel, thread_ts, resolved_user) {
            let turns = match state.core.coordinator.load_history(&conversation, 5).await {
                Ok(turns) => turns,
                Err(error) => {
                    warn!(%error, %conversation, "Feedback citation lookup failed");
                    continue;
                },
            };
            chunk_ids = crate::engine::feedback::chunk_ids_for_recent_assistant_turn(&turns);
            if !chunk_ids.is_empty() {
                break;
            }
        }
    }

    let record = FeedbackRecord {
        slack_msg_id: format!("{channel}:{message_ts}"),
        reaction,
        chunk_ids,
        ts: chrono::Utc::now(),
    };
    if let Err(error) = writer.record(record).await {
        warn!(%error, "Feedback write failed");
    }
}

fn validated_submission(event: SlackInteractionEvent, bot_user_id: &str, bot_id: &str) -> Option<FeedbackSubmission> {
    let SlackInteractionEvent::BlockActions(event) = event else {
        return None;
    };
    let reaction = reaction_for_actions(event.actions.as_deref())?;
    let message = event.message.as_ref()?;
    if !bot_sender_matches(&message.sender, bot_user_id, bot_id) {
        warn!("Ignoring feedback action on a message not authored by this bot");
        return None;
    }
    let SlackInteractionActionContainer::Message(container) = &event.container else {
        return None;
    };
    if message.origin.ts != container.message_ts {
        warn!("Ignoring feedback action with mismatched message identity");
        return None;
    }
    if event
        .channel
        .as_ref()
        .zip(container.channel_id.as_ref())
        .is_some_and(|(channel, container_channel)| channel.id != *container_channel)
    {
        warn!("Ignoring feedback action with mismatched channel identity");
        return None;
    }
    let channel = event
        .channel
        .as_ref()
        .map(|channel| channel.id.to_string())
        .or_else(|| container.channel_id.as_ref().map(ToString::to_string))?;
    let user = event.user.as_ref()?;

    Some(FeedbackSubmission {
        reactor: user.id.0.clone(),
        channel,
        thread_ts: message.origin.thread_ts.as_ref().map(|thread| thread.0.clone()),
        message_ts: container.message_ts.0.clone(),
        reaction,
    })
}

fn handle_interaction<F, Fut>(event: SlackInteractionEvent, bot_user_id: &str, bot_id: &str, persist: F)
where
    F: FnOnce(FeedbackSubmission) -> Fut,
    Fut: std::future::Future<Output = ()> + Send + 'static,
{
    if let Some(submission) = validated_submission(event, bot_user_id, bot_id) {
        tokio::spawn(persist(submission));
    }
}

pub async fn on_interaction(
    event: SlackInteractionEvent,
    _client: std::sync::Arc<SlackHyperClient>,
    states: SlackClientEventsUserState,
) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let guard = states.read().await;
    let state = guard
        .get_user_state::<std::sync::Arc<SlackState>>()
        .ok_or("no state")?
        .clone();
    drop(guard);
    let bot_user_id = state.bot_user_id.clone();
    let bot_id = state.bot_id.clone();
    handle_interaction(event, &bot_user_id, &bot_id, move |submission| async move {
        record_feedback(
            &state,
            &submission.reactor,
            &submission.channel,
            submission.thread_ts.as_deref(),
            &submission.message_ts,
            submission.reaction,
        )
        .await;
    });
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{
        AtomicUsize,
        Ordering,
    };

    use serde_json::json;

    use super::*;

    fn feedback_interaction(
        sender: serde_json::Value,
        action_type: &str,
        action_id: &str,
        value: &str,
    ) -> SlackInteractionEvent {
        serde_json::from_value(json!({
            "type": "block_actions",
            "team": {"id": "T123"},
            "user": {"id": "U123"},
            "api_app_id": "A123",
            "container": {
                "type": "message",
                "message_ts": "1700000000.2",
                "channel_id": "C123"
            },
            "trigger_id": "trigger",
            "channel": {"id": "C123"},
            "message": {
                "ts": "1700000000.2",
                "thread_ts": "1700000000.1",
                "text": "Answer",
                "user": sender
            },
            "actions": [{
                "type": action_type,
                "action_id": action_id,
                "value": value
            }]
        }))
        .unwrap()
    }

    #[test]
    fn feedback_block_matches_slack_context_actions_contract() {
        let block = feedback_block();
        assert_eq!(block["type"], "context_actions");
        assert_eq!(block["elements"][0]["type"], "feedback_buttons");
        assert_eq!(block["elements"][0]["action_id"], FEEDBACK_ACTION_ID);
        assert_eq!(
            block["elements"][0]["positive_button"]["value"],
            POSITIVE_FEEDBACK_VALUE
        );
        assert_eq!(
            block["elements"][0]["negative_button"]["value"],
            NEGATIVE_FEEDBACK_VALUE
        );
        assert_eq!(
            block["elements"][0]["positive_button"]["accessibility_label"],
            "Submit positive feedback on this response"
        );
    }

    #[test]
    fn only_exact_feedback_actions_are_accepted() {
        let positive: SlackInteractionActionInfo = serde_json::from_value(json!({
            "type": "feedback_buttons",
            "action_id": FEEDBACK_ACTION_ID,
            "value": POSITIVE_FEEDBACK_VALUE
        }))
        .unwrap();
        assert_eq!(reaction_for_actions(Some(&[positive])), Some(Reaction::ThumbsUp));

        for action in [
            json!({
                "type": "feedback_buttons",
                "action_id": "different_action",
                "value": POSITIVE_FEEDBACK_VALUE
            }),
            json!({
                "type": "feedback_buttons",
                "action_id": FEEDBACK_ACTION_ID,
                "value": "run_command"
            }),
        ] {
            let action = serde_json::from_value(action).unwrap();
            assert_eq!(reaction_for_actions(Some(&[action])), None);
        }
    }

    #[test]
    fn feedback_requires_a_kiro_authored_message() {
        let kiro_user: SlackMessageSender = serde_json::from_value(json!({"user": "UKIRO"})).unwrap();
        let kiro_bot: SlackMessageSender = serde_json::from_value(json!({"bot_id": "BKIRO"})).unwrap();
        let other: SlackMessageSender = serde_json::from_value(json!({"user": "UOTHER"})).unwrap();

        assert!(bot_sender_matches(&kiro_user, "UKIRO", "BKIRO"));
        assert!(bot_sender_matches(&kiro_bot, "UKIRO", "BKIRO"));
        assert!(!bot_sender_matches(&other, "UKIRO", "BKIRO"));
    }

    #[tokio::test]
    async fn interaction_callback_does_not_wait_for_persistence() {
        let (started_tx, started_rx) = tokio::sync::oneshot::channel();
        let (release_tx, release_rx) = tokio::sync::oneshot::channel();

        handle_interaction(
            feedback_interaction(
                json!("UKIRO"),
                "feedback_buttons",
                FEEDBACK_ACTION_ID,
                POSITIVE_FEEDBACK_VALUE,
            ),
            "UKIRO",
            "BKIRO",
            move |submission| async move {
                assert_eq!(submission.reaction, Reaction::ThumbsUp);
                started_tx.send(()).unwrap();
                release_rx.await.unwrap();
            },
        );

        started_rx.await.unwrap();
        release_tx.send(()).unwrap();
    }

    #[test]
    fn forged_feedback_interactions_do_not_schedule_persistence() {
        let scheduled = AtomicUsize::new(0);
        for event in [
            feedback_interaction(
                json!("UOTHER"),
                "feedback_buttons",
                FEEDBACK_ACTION_ID,
                POSITIVE_FEEDBACK_VALUE,
            ),
            feedback_interaction(json!("UKIRO"), "button", FEEDBACK_ACTION_ID, POSITIVE_FEEDBACK_VALUE),
            feedback_interaction(
                json!("UKIRO"),
                "feedback_buttons",
                "different_action",
                POSITIVE_FEEDBACK_VALUE,
            ),
            feedback_interaction(json!("UKIRO"), "feedback_buttons", FEEDBACK_ACTION_ID, "run_command"),
        ] {
            handle_interaction(event, "UKIRO", "BKIRO", |_| {
                scheduled.fetch_add(1, Ordering::SeqCst);
                async {}
            });
        }

        assert_eq!(scheduled.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn citation_lookup_prefers_the_thread_scoped_dm_session() {
        assert_eq!(citation_lookup_keys("D123", Some("1700000000.1"), "alice"), vec![
            "thread:D123:1700000000.1",
            "dm:alice",
            "channel:D123",
        ]);
    }
}
