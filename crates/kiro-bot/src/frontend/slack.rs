//! Slack Socket Mode frontend.
//!
//! Connects to Slack via WebSocket, receives events (messages, mentions, reactions),
//! and dispatches them through the bot core. Handles:
//! - Message and app_mention events → [`crate::engine::core::dispatch`]
//! - Reaction events → tool approval (✅/❌/🔓)
//! - Feedback button interactions → response quality persistence
//! - File downloads from message attachments
//! - Native Slack Markdown Block Kit rendering

mod approvals;
mod attachments;
mod delivery;
mod events;
mod feedback;
mod socket_admission;

use std::collections::HashMap;
use std::sync::{
    Arc,
    Mutex,
};
#[cfg(test)]
use std::time::Duration;

use anyhow::Result;
#[cfg(test)]
use slack_morphism::errors::{
    SlackClientApiError,
    SlackClientEndOfStreamError,
    SlackClientError,
    SlackClientHttpError,
    SlackRateLimitError,
};
use slack_morphism::prelude::*;

pub use self::approvals::{
    PendingApproval,
    PendingApprovals,
    spawn_approval_listener,
};
use self::attachments::AttachmentStore;
#[cfg(test)]
use self::delivery::*;
pub use self::events::dispatch_event;
pub use self::feedback::on_interaction;
pub use self::socket_admission::{
    SlackSocketState,
    on_error,
    on_push,
};
#[cfg(test)]
use crate::engine::acp::ProgressStatus;
use crate::engine::attachment_read::AttachmentReadAuthorizer;
use crate::engine::core::BotCore;
#[cfg(test)]
use crate::engine::core::GENAI_DISCLAIMER;
use crate::engine::user_map::UserMap;

// ---------------------------------------------------------------------------
// SlackFrontend
// ---------------------------------------------------------------------------

pub struct SlackFrontend {
    pub client: Arc<SlackHyperClient>,
    pub bot_token: SlackApiToken,
    pub user_map: Arc<UserMap>,
    pub conversation_history: u16,
    pub last_seen: Mutex<HashMap<String, SlackTs>>,
    pub recipient_teams: Mutex<HashMap<String, String>>,
    feedback_enabled: bool,
    attachment_store: AttachmentStore,
}

impl SlackFrontend {
    pub fn new(
        client: Arc<SlackHyperClient>,
        bot_token: SlackApiToken,
        user_map: Arc<UserMap>,
        conversation_history: u16,
        attachment_reads: Arc<AttachmentReadAuthorizer>,
        feedback_enabled: bool,
    ) -> Result<Self> {
        Ok(Self {
            client,
            bot_token,
            user_map,
            conversation_history,
            last_seen: Mutex::new(HashMap::new()),
            recipient_teams: Mutex::new(HashMap::new()),
            feedback_enabled,
            attachment_store: AttachmentStore::new_default(attachment_reads)?,
        })
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
    pub bot_id: String,
    pub user_map: Arc<UserMap>,
    pub pending_approvals: PendingApprovals,
    /// Native feedback controls and thumbs reactions flow into this writer for
    /// the nightly metrics Lambda. None when no feedback table is configured.
    pub feedback_writer: Option<Arc<dyn crate::engine::feedback::FeedbackWriter>>,
    /// This task's identity, as written into the cross-task approvals table.
    /// Read by the reaction handler to compare against the lookup-owner so a
    /// self-owned approval short-circuits without forwarding to ourselves.
    /// Empty for local CLI / unit-test contexts.
    pub own_task_id: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn delivery_retries_only_safe_failure_classes() {
        let rate_limit = SlackClientError::RateLimitError(
            SlackRateLimitError::new().with_retry_after(std::time::Duration::from_secs(7)),
        );
        assert_eq!(
            delivery_retry_delay(&rate_limit, 1, RetrySafety::PostMessage),
            Some(Duration::from_secs(7))
        );

        let server_error =
            SlackClientError::HttpError(SlackClientHttpError::new(http::StatusCode::SERVICE_UNAVAILABLE));
        let server_delay = delivery_retry_delay(&server_error, 2, RetrySafety::Idempotent).unwrap();
        assert!((Duration::from_millis(320)..=Duration::from_millis(480)).contains(&server_delay));
        assert!(delivery_retry_delay(&server_error, 2, RetrySafety::PostMessage).is_none());

        let api_error = SlackClientError::ApiError(SlackClientApiError::new("channel_not_found".to_string()));
        assert!(delivery_retry_delay(&api_error, 1, RetrySafety::Idempotent).is_none());
        assert!(!update_requires_new_message(&api_error));
        assert!(stream_start_can_fallback(&api_error));
        assert!(!stream_start_can_fallback(&SlackClientError::ApiError(
            SlackClientApiError::new("internal_error".to_string())
        )));
        for code in ["message_not_found", "cant_update_message", "edit_window_closed"] {
            assert!(update_requires_new_message(&SlackClientError::ApiError(
                SlackClientApiError::new(code.to_string())
            )));
        }

        let transport_error = SlackClientError::EndOfStream(SlackClientEndOfStreamError::new());
        assert!(delivery_retry_delay(&transport_error, 1, RetrySafety::Idempotent).is_some());
        assert!(delivery_retry_delay(&transport_error, 1, RetrySafety::PostMessage).is_none());
        assert!(!stream_start_can_fallback(&transport_error));

        let request_timeout = SlackClientError::HttpError(SlackClientHttpError::new(http::StatusCode::REQUEST_TIMEOUT));
        assert!(delivery_retry_delay(&request_timeout, 1, RetrySafety::Idempotent).is_some());
        assert!(delivery_retry_delay(&request_timeout, 1, RetrySafety::PostMessage).is_none());
        assert!(!stream_start_can_fallback(&request_timeout));

        let excessive_rate_limit = SlackClientError::RateLimitError(
            SlackRateLimitError::new().with_retry_after(DELIVERY_RETRY_MAX_DELAY + Duration::from_secs(1)),
        );
        assert!(delivery_retry_delay(&excessive_rate_limit, 1, RetrySafety::Idempotent).is_none());
    }

    fn markdown_text(content: &SlackMessageContent) -> &str {
        match &content.blocks.as_ref().unwrap()[0] {
            SlackBlock::Markdown(block) => &block.text,
            other => panic!("expected markdown block, got {other:?}"),
        }
    }

    fn context_text(content: &SlackMessageContent) -> String {
        match content.blocks.as_ref().unwrap().last().unwrap() {
            SlackBlock::Context(context) => context
                .elements
                .iter()
                .map(|element| match element {
                    SlackContextBlockElement::MarkDown(text) => text.text.as_str(),
                    other => panic!("expected markdown context, got {other:?}"),
                })
                .collect::<Vec<_>>()
                .join(" | "),
            other => panic!("expected context block, got {other:?}"),
        }
    }

    #[test]
    fn standard_markdown_is_preserved_for_slack() {
        let text = concat!(
            "# Heading\n\n",
            "**Bold** and [parenthesized link](https://example.com/docs_(v2)).\n\n",
            "```bash\n# shell comment\necho \"ok\"\n```",
        );
        let rendered = render_message(text);
        assert_eq!(rendered.chunks.len(), 1);
        assert_eq!(markdown_text(&rendered.chunks[0]), text);
    }

    #[test]
    fn renderer_includes_accessible_fallback_and_context() {
        let rendered = render_message(&format!(
            "## Root cause\nThe worker stopped.\n\nSources: `crates/kiro-bot/src/engine/acp.rs:10`\n\n{GENAI_DISCLAIMER}"
        ));
        let content = &rendered.chunks[0];
        assert_eq!(
            serde_json::to_value(content).unwrap(),
            serde_json::json!({
                "text": concat!(
                    "Root cause The worker stopped.\n",
                    "Sources: crates/kiro-bot/src/engine/acp.rs:10\n",
                    "AI-generated; verify before acting."
                ),
                "blocks": [
                    {
                        "type": "markdown",
                        "text": "## Root cause\nThe worker stopped."
                    },
                    {
                        "type": "context",
                        "elements": [
                            {
                                "type": "mrkdwn",
                                "text": "Sources: `crates/kiro-bot/src/engine/acp.rs:10`",
                                "verbatim": true
                            },
                            {
                                "type": "mrkdwn",
                                "text": "AI-generated; verify before acting.",
                                "verbatim": true
                            }
                        ]
                    }
                ]
            })
        );
    }

    #[test]
    fn accessible_fallback_reserves_room_for_sources_and_disclaimer() {
        let body = "answer ".repeat(FALLBACK_CHAR_LIMIT);
        let rendered = render_message(&format!(
            "{body}\n\nSources: `crates/kiro-bot/src/engine/acp.rs:10`\n\n{GENAI_DISCLAIMER}"
        ));
        let fallback = rendered.chunks.last().unwrap().text.as_deref().unwrap();

        assert!(char_len(fallback) <= FALLBACK_CHAR_LIMIT);
        assert!(fallback.contains("Sources: crates/kiro-bot/src/engine/acp.rs:10"));
        assert!(fallback.ends_with(DISCLAIMER_CONTEXT));
    }

    /// Read a rendered chunk back the way the thread-history path does: over the
    /// raw wire JSON, not the typed `SlackMessageContent`.
    fn context_of(content: &SlackMessageContent) -> Option<String> {
        let blocks = content
            .blocks
            .as_ref()
            .map(|blocks| serde_json::to_value(blocks).unwrap())
            .and_then(|blocks| blocks.as_array().cloned());
        context_message_text(content.text.as_deref(), blocks.as_deref())
    }

    #[test]
    fn thread_context_prefers_rendered_markdown_over_fallback_text() {
        let content = render_message("## Root cause\nThe worker stopped.").chunks.remove(0);

        assert_eq!(
            context_of(&content).as_deref(),
            Some("## Root cause\nThe worker stopped.")
        );
    }

    #[test]
    fn thread_context_preserves_rendered_sources_blocks() {
        let content = render_message(&format!(
            "The worker stopped.\n\nSources: [ACP lifecycle](https://example.com/acp)\n\n{GENAI_DISCLAIMER}"
        ))
        .chunks
        .remove(0);

        let context = context_of(&content).unwrap();

        assert!(context.starts_with("The worker stopped."));
        assert!(context.contains("Sources: <https://example.com/acp|ACP lifecycle>"));
    }

    /// The bot's own card chrome carries no answer text, so it is skipped rather
    /// than rendered into the agent's thread context.
    #[test]
    fn thread_context_skips_plan_and_feedback_card_chrome() {
        let blocks = serde_json::json!([
            {"type": "plan", "title": "Working on your request", "tasks": []},
            {"type": "markdown", "text": "The answer."},
            {"type": "context_actions", "elements": [{"type": "feedback_buttons"}]},
        ]);

        assert_eq!(
            context_message_text(Some("fallback"), blocks.as_array().map(Vec::as_slice)).as_deref(),
            Some("The answer.")
        );
    }

    /// A block type nobody has modelled yet still has to produce usable context:
    /// Slack always populates top-level `text`, so that is the recovery path.
    #[test]
    fn unknown_block_types_fall_back_to_the_accessibility_text() {
        let blocks = serde_json::json!([{"type": "some_future_card", "payload": {"deeply": "nested"}}]);

        assert_eq!(
            context_message_text(Some("Readable fallback"), blocks.as_array().map(Vec::as_slice)).as_deref(),
            Some("Readable fallback")
        );
        // With neither a known block nor a fallback there is nothing faithful to
        // report, so the message is skipped instead of emitting raw JSON.
        assert_eq!(context_message_text(None, blocks.as_array().map(Vec::as_slice)), None);
    }

    /// Half an answer is worse than the accessibility fallback: if any inline
    /// element inside a `rich_text` block is unrecognised, the recovered text
    /// would silently omit content, so the fallback wins instead.
    #[test]
    fn partially_understood_rich_text_prefers_the_complete_fallback() {
        let blocks = serde_json::json!([{
            "type": "rich_text",
            "elements": [{
                "type": "rich_text_section",
                "elements": [
                    {"type": "text", "text": "Deploy is blocked by "},
                    {"type": "some_future_inline", "text": "the thing that matters"},
                ]
            }]
        }]);

        assert_eq!(
            context_message_text(
                Some("Deploy is blocked by the thing that matters"),
                blocks.as_array().map(Vec::as_slice)
            )
            .as_deref(),
            Some("Deploy is blocked by the thing that matters")
        );

        // A known tag missing the field that carries its text is the same kind of
        // partial loss, so it must reach the fallback too rather than rendering
        // an entry with the emoji silently dropped.
        let missing_field = serde_json::json!([{
            "type": "rich_text",
            "elements": [{
                "type": "rich_text_section",
                "elements": [
                    {"type": "text", "text": "Ship it "},
                    {"type": "emoji"},
                ]
            }]
        }]);

        assert_eq!(
            context_message_text(Some("Ship it :rocket:"), missing_field.as_array().map(Vec::as_slice)).as_deref(),
            Some("Ship it :rocket:")
        );
    }

    #[test]
    fn oversized_answers_deliver_every_chunk_and_preserve_source_context() {
        let body = "A useful sentence with enough words to split safely. ".repeat(500);
        let rendered = render_message(&format!(
            "{body}\n\n{body}\n\nSources:\n- [Runbook](https://example.com/docs_(v2))\n\n{GENAI_DISCLAIMER}"
        ));

        assert!(rendered.chunks.len() > 1);
        assert!(
            rendered
                .chunks
                .iter()
                .all(|chunk| char_len(markdown_text(chunk)) <= MARKDOWN_CHAR_LIMIT)
        );
        let context = context_text(rendered.chunks.last().unwrap());
        assert!(context.contains("<https://example.com/docs_(v2)|Runbook>"));
        assert!(context.contains(DISCLAIMER_CONTEXT));
    }

    #[test]
    fn delivered_markdown_sequence_round_trips_complete_response() {
        let prose_line = format!("{} [guide](https://example.com/docs_(v2))", "word ".repeat(700));
        let body = format!(
            "{prose_line}\n{prose_line}\n{prose_line}\n```rust\nfn main() {{ println!(\"done\"); }}\n```\n{prose_line}"
        );
        let rendered = render_message(&format!(
            "{body}\nSources: [Runbook](https://example.com/runbook)\n{GENAI_DISCLAIMER}"
        ));
        let delivered = rendered.chunks.iter().map(markdown_text).collect::<Vec<_>>().join("\n");

        assert!(rendered.chunks.len() > 1);
        assert_eq!(delivered, body);
        assert!(
            rendered
                .chunks
                .iter()
                .all(|chunk| chunk.text.as_deref().is_some_and(|fallback| !fallback.is_empty()))
        );
        let context = context_text(rendered.chunks.last().unwrap());
        assert!(context.contains("<https://example.com/runbook|Runbook>"));
        assert!(context.contains(DISCLAIMER_CONTEXT));
    }

    #[test]
    fn oversized_source_footer_cannot_truncate_disclaimer() {
        let sources = format!(
            "Sources:\n{}",
            "- `crates/kiro-bot/src/frontend/slack.rs:1`\n".repeat(200)
        );
        let rendered = render_message(&format!("Answer.\n\n{sources}\n{GENAI_DISCLAIMER}"));
        let context = match rendered.chunks[0].blocks.as_ref().unwrap().last().unwrap() {
            SlackBlock::Context(context) => context,
            other => panic!("expected context block, got {other:?}"),
        };

        assert_eq!(context.elements.len(), 2);
        match context.elements.last().unwrap() {
            SlackContextBlockElement::MarkDown(text) => assert_eq!(text.text, DISCLAIMER_CONTEXT),
            other => panic!("expected disclaimer context, got {other:?}"),
        }
    }

    #[test]
    fn post_requests_disable_unfurls() {
        let mut rendered = render_message("[docs](https://example.com)");
        let content = rendered.chunks.remove(0);
        let request = post_request("C123".into(), content, Some("170.1".into()));
        assert_eq!(request.unfurl_links, Some(false));
        assert_eq!(request.unfurl_media, Some(false));
        assert_eq!(request.thread_ts.as_ref().map(|ts| ts.0.as_str()), Some("170.1"));
    }

    #[test]
    fn stream_requests_match_slack_plan_live_card_contract() {
        let start = serde_json::to_value(start_stream_request(
            "C123".into(),
            "170.1".into(),
            "U123".into(),
            "T123".into(),
        ))
        .unwrap();
        assert_eq!(start["channel"], "C123");
        assert_eq!(start["thread_ts"], "170.1");
        assert_eq!(start["recipient_user_id"], "U123");
        assert_eq!(start["recipient_team_id"], "T123");
        assert_eq!(start["task_display_mode"], "plan");
        assert_eq!(start["chunks"][0]["type"], "task_update");
        assert_eq!(start["chunks"][0]["id"], RESPONSE_TASK_ID);
        assert_eq!(start["chunks"][0]["title"], "Working on your request");
        assert_eq!(start["chunks"][0]["status"], "in_progress");

        let progress = SlackStreamChunk::TaskUpdate {
            id: "kiro-context".into(),
            title: "Gathering context".into(),
            status: SlackStreamTaskStatus::Complete,
        };
        let append = serde_json::to_value(SlackAppendStreamRequest {
            channel: "C123".into(),
            ts: "170.2".into(),
            markdown_text: None,
            chunks: vec![progress.clone()],
        })
        .unwrap();
        assert_eq!(append["ts"], "170.2");
        assert!(append.get("markdown_text").is_none());
        assert_eq!(append["chunks"][0]["type"], "task_update");
        assert_eq!(append["chunks"][0]["id"], "kiro-context");
        assert_eq!(append["chunks"][0]["title"], "Gathering context");
        assert_eq!(append["chunks"][0]["status"], "complete");

        let complete = serde_json::to_value(stop_stream_request(
            "C123".into(),
            "170.2".into(),
            &format!("**Answer**\n\n{GENAI_DISCLAIMER}"),
            ProgressStatus::Complete,
            vec![progress.clone()],
            true,
        ))
        .unwrap();
        assert_eq!(complete["chunks"][0]["id"], RESPONSE_TASK_ID);
        assert_eq!(complete["chunks"][0]["status"], "complete");
        assert_eq!(complete["chunks"][1]["id"], "kiro-context");
        assert_eq!(complete["chunks"][1]["status"], "complete");
        assert_eq!(complete["chunks"][2]["type"], "markdown_text");
        assert_eq!(complete["chunks"][2]["text"], "**Answer**");
        assert!(complete.get("markdown_text").is_none());
        assert_eq!(complete["blocks"][0]["type"], "context");
        assert_eq!(complete["blocks"][1]["type"], "context_actions");

        let fallback = serde_json::to_value(stop_stream_fallback_request(
            "C123".into(),
            "170.2".into(),
            &format!("**Answer**\n\n{GENAI_DISCLAIMER}"),
            ProgressStatus::Complete,
            vec![progress],
        ))
        .unwrap();
        assert_eq!(fallback["markdown_text"], "**Answer**");
        assert_eq!(fallback["chunks"][0]["id"], RESPONSE_TASK_ID);
        assert_eq!(fallback["chunks"][1]["id"], "kiro-context");
        assert_eq!(fallback["chunks"][1]["status"], "complete");
        assert_eq!(fallback["blocks"][0]["type"], "context");
        assert_eq!(fallback["blocks"].as_array().unwrap().len(), 1);

        let failed = serde_json::to_value(stop_stream_request(
            "C123".into(),
            "170.2".into(),
            "Error",
            ProgressStatus::Error,
            Vec::new(),
            false,
        ))
        .unwrap();
        assert_eq!(failed["chunks"][0]["status"], "error");
    }

    #[test]
    fn degraded_stream_update_preserves_feedback_after_start_or_append_fallback() {
        let mut chunks = degraded_final_chunks(
            &format!("Answer.\n\nSources: `crates/kiro-bot/src/frontend/slack.rs:1`\n\n{GENAI_DISCLAIMER}"),
            true,
        );
        let chunk = chunks.pop_front().unwrap();
        assert!(chunks.is_empty());
        let request = SlackApiChatUpdateRequest::new("C123".into(), chunk.content, "1700000000.2".into());
        let payload = request_with_feedback(&request, chunk.feedback_enabled);

        assert_eq!(payload["channel"], "C123");
        assert_eq!(payload["ts"], "1700000000.2");
        assert_eq!(
            payload["text"],
            concat!(
                "Answer.\n",
                "Sources: crates/kiro-bot/src/frontend/slack.rs:1\n",
                "AI-generated; verify before acting."
            )
        );
        assert_eq!(payload["blocks"][0]["type"], "markdown");
        assert_eq!(payload["blocks"][1]["type"], "context");
        assert_eq!(payload["blocks"][2], super::feedback::feedback_block());
    }

    #[test]
    fn degraded_stream_posts_feedback_only_with_the_final_chunk() {
        let text = format!(
            "{}\n\nSources: `crates/kiro-bot/src/frontend/slack.rs:1`\n\n{GENAI_DISCLAIMER}",
            "A complete answer sentence. ".repeat(1_000)
        );
        let chunks = degraded_final_chunks(&text, true);
        assert!(chunks.len() > 1);
        let last_index = chunks.len() - 1;

        for (index, chunk) in chunks.into_iter().enumerate() {
            let request = post_request("C123".into(), chunk.content, Some("1700000000.1".into()));
            let payload = request_with_feedback(&request, chunk.feedback_enabled);
            let feedback_count = payload["blocks"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|block| block["type"] == "context_actions")
                .count();

            assert_eq!(feedback_count, usize::from(index == last_index));
            assert_eq!(payload["thread_ts"], "1700000000.1");
        }
    }

    #[test]
    fn slack_task_cards_decode_for_thread_history() {
        let block: SlackBlock = serde_json::from_value(serde_json::json!({
            "type": "task_card",
            "task_id": "tool-1",
            "title": "Searching Kiro docs",
            "status": "in_progress"
        }))
        .unwrap();

        assert!(matches!(block, SlackBlock::TaskCard(_)));
    }

    #[test]
    fn long_answers_are_split_without_content_loss() {
        let paragraph = "A useful sentence with enough words to split safely. ".repeat(400);
        let body = format!("{paragraph}\n{paragraph}");
        let rendered = render_message(&body);
        assert!(rendered.chunks.len() > 1);
        assert!(
            rendered
                .chunks
                .iter()
                .all(|chunk| char_len(markdown_text(chunk)) <= MARKDOWN_CHAR_LIMIT)
        );
        let delivered = rendered.chunks.iter().map(markdown_text).collect::<Vec<_>>().join("\n");
        assert_eq!(
            delivered.split_whitespace().collect::<Vec<_>>(),
            body.split_whitespace().collect::<Vec<_>>()
        );
    }

    #[test]
    fn split_code_fences_remain_closed() {
        let code = format!("```rust\n{}\n```", "let value = 1;\n".repeat(1_000));
        let rendered = render_message(&code);
        assert!(rendered.chunks.len() > 1);
        for chunk in &rendered.chunks {
            let markdown = markdown_text(chunk);
            assert!(char_len(markdown) <= MARKDOWN_CHAR_LIMIT);
            assert_eq!(
                markdown.match_indices("```").count() % 2,
                0,
                "each Slack message must contain complete fences"
            );
        }
    }

    #[test]
    fn inline_self_closing_fence_remains_balanced_across_chunks() {
        let reply = "Before the inline snippet.\nContext remains visible.\n```status: ready```\nAfter the inline snippet.\nThe answer keeps going.\n";
        let chunks = split_markdown(reply, 40);

        assert!(chunks.len() > 1);
        assert_eq!(chunks.join("\n"), reply.trim_end());
        for chunk in &chunks {
            assert!(char_len(chunk) <= 40);
            assert_eq!(
                chunk.match_indices("```").count() % 2,
                0,
                "each delivered chunk must contain complete fences: {chunk:?}"
            );
        }
    }

    #[test]
    fn oversized_fence_info_strings_remain_bounded_and_closed() {
        for info_len in [
            MAX_FENCE_HEADER_CHARS - 3,
            MAX_FENCE_HEADER_CHARS - 2,
            MARKDOWN_CHAR_LIMIT - 4,
            MARKDOWN_CHAR_LIMIT + 100,
        ] {
            let code = format!("```{}\nbody\n```\n", "x".repeat(info_len));
            let chunks = split_markdown(&code, MARKDOWN_CHAR_LIMIT);
            assert!(!chunks.is_empty());
            assert!(chunks.len() <= 2, "unexpected chunk growth for info length {info_len}");
            let preserved_info_chars = chunks
                .iter()
                .map(|chunk| chunk.chars().filter(|character| *character == 'x').count())
                .sum::<usize>();
            assert!(
                preserved_info_chars >= info_len,
                "fence info string was truncated for info length {info_len}"
            );
            let joined = chunks.join("\n");
            assert_eq!(joined.matches("body").count(), 1);
            assert!(joined.rfind('x').unwrap() < joined.find("body").unwrap());
            for chunk in &chunks {
                assert!(char_len(chunk) <= MARKDOWN_CHAR_LIMIT);
                assert_eq!(
                    chunk.match_indices("```").count() % 2,
                    0,
                    "chunk must contain complete fences for info length {info_len}"
                );
            }
        }

        let rendered = render_message(&format!("```{}\nbody\n```", "x".repeat(MARKDOWN_CHAR_LIMIT + 100)));
        assert!(rendered.chunks.len() > 1);
    }

    #[test]
    fn oversized_self_closing_fence_is_split_into_balanced_chunks() {
        let content = "x".repeat(MARKDOWN_CHAR_LIMIT + 500);
        let chunks = split_markdown(&format!("```{content}```"), MARKDOWN_CHAR_LIMIT);

        assert!(chunks.len() > 1);
        assert_eq!(
            chunks
                .iter()
                .map(|chunk| chunk.trim_start_matches("```\n").trim_end_matches("\n```"))
                .collect::<String>(),
            content
        );
        for chunk in &chunks {
            assert!(char_len(chunk) <= MARKDOWN_CHAR_LIMIT);
            assert_eq!(chunk.match_indices("```").count() % 2, 0);
        }
    }

    #[test]
    #[should_panic(expected = "markdown chunk limit must allow fenced content")]
    fn markdown_splitter_rejects_tiny_limits() {
        split_markdown("```rust\nbody\n```", 8);
    }

    #[test]
    fn slack_mentions_and_controls_are_neutralized() {
        let rendered = render_message("Ask <@U123>, <!channel>, <!subteam^S123>, or <#C123>.");
        let markdown = markdown_text(&rendered.chunks[0]);
        assert!(!markdown.contains("<@"));
        assert!(!markdown.contains("<!"));
        assert!(!markdown.contains("<#"));
        assert!(markdown.contains("&lt;@U123>"));
    }

    #[test]
    fn thinking_tags_are_stripped_without_rewriting_visible_markdown() {
        let rendered = render_message("<thinking>internal</thinking>\n**visible**");
        assert_eq!(markdown_text(&rendered.chunks[0]), "**visible**");
    }

    #[test]
    fn known_runtime_errors_are_replaced_with_a_correlation_id() {
        let rendered = render_message("Error: Internal error: \"server shut down unexpectedly\"");
        let markdown = markdown_text(&rendered.chunks[0]);
        assert!(markdown.contains("Please try again."));
        assert!(markdown.contains("Reference: `KB-"));
        assert!(!markdown.contains("server shut down"));
    }

    #[test]
    fn completion_replaces_the_tracked_progress_message() {
        let mut tracker = DeliveryTracker::default();
        let key = DeliveryKey::new("C123".into(), Some("170.1".into()));
        tracker.record_ack("170.2".into(), key.clone());

        assert!(tracker.complete("170.2"));
        assert!(tracker.is_completed("170.2"));
        assert_eq!(tracker.begin_update(&key).unwrap().as_deref(), Some("170.2"));
        assert_eq!(
            tracker.replacements.get(&key).map(String::as_str),
            Some("170.2"),
            "reserving an update must not consume the placeholder"
        );
        assert!(tracker.begin_update(&key).is_err());
        tracker.finish_update(&key, "170.2");
        assert!(tracker.begin_update(&key).unwrap().is_none());
    }

    #[test]
    fn completed_delivery_expiry_removes_pending_replacement() {
        let mut tracker = DeliveryTracker::default();
        let key = DeliveryKey::new("C123".into(), Some("170.1".into()));
        tracker.record_ack("170.2".into(), key.clone());
        assert!(tracker.complete("170.2"));

        tracker.expire_delivery("170.2");

        assert!(!tracker.is_completed("170.2"));
        assert!(tracker.begin_update(&key).unwrap().is_none());
    }

    #[test]
    fn old_expiry_does_not_remove_a_newer_replacement() {
        let mut tracker = DeliveryTracker::default();
        let key = DeliveryKey::new("C123".into(), Some("170.1".into()));
        tracker.record_ack("170.2".into(), key.clone());
        assert!(tracker.complete("170.2"));
        tracker.record_ack("170.3".into(), key.clone());
        assert!(tracker.complete("170.3"));

        tracker.expire_delivery("170.2");

        assert_eq!(tracker.begin_update(&key).unwrap().as_deref(), Some("170.3"));
    }

    #[test]
    fn failed_final_update_preserves_a_reusable_progress_placeholder() {
        let mut tracker = DeliveryTracker::default();
        let key = DeliveryKey::new("C123".into(), Some("170.1".into()));
        tracker.record_ack("170.2".into(), key.clone());
        assert!(tracker.complete("170.2"));
        assert_eq!(tracker.begin_update(&key).unwrap().as_deref(), Some("170.2"));

        tracker.fail_update(&key);

        assert!(tracker.is_completed("170.2"));
        assert_eq!(tracker.begin_update(&key).unwrap().as_deref(), Some("170.2"));

        tracker.expire_delivery("170.2");
        assert!(!tracker.is_completed("170.2"));
    }

    #[test]
    fn completed_update_cannot_reuse_the_progress_placeholder() {
        let mut tracker = DeliveryTracker::default();
        let key = DeliveryKey::new("C123".into(), Some("170.1".into()));
        tracker.record_ack("170.2".into(), key.clone());
        assert!(tracker.complete("170.2"));

        let message_id = tracker.begin_update(&key).unwrap().unwrap();
        tracker.finish_update(&key, &message_id);

        assert!(tracker.is_completed(&message_id));
        assert!(tracker.begin_update(&key).unwrap().is_none());
    }

    #[test]
    fn exact_message_citations_take_precedence_over_history() {
        let mut tracker = DeliveryTracker::default();
        tracker.record_citations("1700000000.1".into(), vec!["docs/exact.md".into()]);

        let exact = tracker.citations("1700000000.1");
        let selected = if exact.is_empty() {
            vec!["docs/history.md".into()]
        } else {
            exact
        };

        assert_eq!(selected, vec!["docs/exact.md"]);
    }

    #[test]
    fn citations_can_be_tracked_for_multiple_message_timestamps() {
        let mut tracker = DeliveryTracker::default();
        let citations = vec!["crates/kiro-bot/src/frontend/slack.rs:1-20".into()];
        for message_id in ["1700000000.1", "1700000000.2", "1700000000.3"] {
            tracker.record_citations(message_id.into(), citations.clone());
        }

        for message_id in ["1700000000.1", "1700000000.2", "1700000000.3"] {
            assert_eq!(tracker.citations(message_id), citations);
        }
    }

    #[test]
    fn citation_capacity_evicts_oldest_and_expiry_removes_order_entry() {
        let mut tracker = DeliveryTracker::default();
        for index in 0..=1_000 {
            tracker.record_citations(index.to_string(), vec![format!("docs/{index}.md")]);
        }

        assert!(tracker.citations("0").is_empty());
        assert_eq!(tracker.citations("1"), vec!["docs/1.md"]);
        tracker.expire_citations("1");
        assert!(tracker.citations("1").is_empty());
        assert!(!tracker.citation_order.iter().any(|message_id| message_id == "1"));
    }

    #[test]
    fn progress_expiry_does_not_expire_newer_citations() {
        let mut tracker = DeliveryTracker::default();
        let key = DeliveryKey::new("C123".into(), Some("170.1".into()));
        tracker.record_ack("170.2".into(), key);
        assert!(tracker.complete("170.2"));
        tracker.record_citations("170.2".into(), vec!["docs/current.md".into()]);

        tracker.expire_delivery("170.2");

        assert_eq!(tracker.citations("170.2"), vec!["docs/current.md"]);
        tracker.expire_citations("170.2");
        assert!(tracker.citations("170.2").is_empty());
    }

    #[test]
    fn plain_text_unchanged() {
        let rendered = render_message("just text");
        assert_eq!(markdown_text(&rendered.chunks[0]), "just text");
    }
}
