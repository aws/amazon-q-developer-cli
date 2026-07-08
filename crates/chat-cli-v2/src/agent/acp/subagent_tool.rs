use agent::AgentHandle;
use agent::agent_loop::protocol::LoopError;
use agent::protocol::{
    AgentError,
    AgentEvent,
    AgentStopReason,
    ContentChunk,
    SendPromptArgs,
};
use agent::tools::summary::Summary;
use sacp::schema::SessionId;
use serde::{
    Deserialize,
    Serialize,
};

/// Error type for internal prompt execution, distinguishing cancellation from other failures.
#[derive(Debug, thiserror::Error)]
pub enum InternalPromptError {
    #[error("Session cancelled")]
    Cancelled,
    #[error("{0}")]
    Failed(String),
}

impl InternalPromptError {
    pub fn is_cancelled(&self) -> bool {
        matches!(self, Self::Cancelled)
    }
}

/// Commands supported by the subagent tool
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "command", rename_all = "snake_case")]
pub enum SubagentCommand {
    /// List available agents
    ListAgents,
    /// Invoke one or more subagents
    InvokeSubagents { subagents: Vec<SubagentInvocation> },
}

/// Parameters for invoking a single subagent
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubagentInvocation {
    /// The query/task for the subagent
    pub query: String,
    /// Optional agent name to use
    pub agent_name: Option<String>,
    /// Optional context to provide
    pub relevant_context: Option<String>,
}

/// Result of a subagent invocation (blocking mode)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubagentResult {
    pub session_id: SessionId,
    pub agent_name: String,
    pub task_description: String,
    pub task_result: String,
}

/// Result when subagent is moved to background
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BackgroundedResult {
    pub session_id: SessionId,
    pub agent_name: String,
    pub initial_query: String,
}

impl BackgroundedResult {
    pub fn to_tool_output(&self) -> String {
        format!(
            "User has moved subagent to background.\n\
             Session ID: {}\n\
             Agent: {}\n\
             Initial query: {}\n\n\
             The subagent will continue working independently. \
             You will be informed of its status in future messages. \
             Do not wait for this task - proceed with other work.",
            self.session_id.0, self.agent_name, self.initial_query
        )
    }
}

const SUMMARY_FAILSAFE_MSG: &str = "You have not called the summary tool yet. Please call the summary tool now to provide your findings to the main agent before ending your task.";

/// Wrap the subagent's last message when it ended without ever calling the summary tool.
fn no_summary_fallback(last_message: &str) -> String {
    format!(
        "The subagent ended without calling the summary tool. \
         This is the content of its last message:\n\n{last_message}"
    )
}

/// Wrap the subagent's last message when its turn ended in an empty response.
/// An empty response is not treated as a stage failure: we degrade to the last
/// available message (which may be empty if the subagent never produced one).
fn empty_response_fallback(last_message: &str) -> String {
    format!(
        "The subagent returned an empty response without calling the summary tool. \
         This is the content of its last available message:\n\n{last_message}"
    )
}

/// Handle an internal prompt for subagent execution.
///
/// Waits for the agent to call the summary tool. If the agent ends its turn without
/// calling summary, sends a reminder. If it still refuses, extracts from the final message.
pub(crate) async fn handle_internal_prompt(
    query: String,
    mut agent: AgentHandle,
) -> Result<Summary, InternalPromptError> {
    agent
        .send_prompt(SendPromptArgs {
            content: vec![ContentChunk::Text(query.clone())],
            should_continue_turn: None,
        })
        .await
        .map_err(|e| InternalPromptError::Failed(format!("Failed to send prompt: {e:?}")))?;

    let mut summary: Option<Summary> = None;
    let mut has_sent_failsafe = false;
    // The last non-empty assistant message we have seen, used as a graceful
    // fallback when the subagent never calls the summary tool or ends on an
    // empty response.
    let mut last_message: Option<String> = None;

    loop {
        match agent.recv().await {
            Ok(event) => match event {
                AgentEvent::SubagentSummary(s) => {
                    summary = Some(s);
                },
                AgentEvent::EndTurn(metadata) => {
                    let turn_text = metadata
                        .result
                        .as_ref()
                        .and_then(|r| r.as_ref().ok())
                        .map(|msg| msg.text())
                        .unwrap_or_default();
                    if !turn_text.is_empty() {
                        last_message = Some(turn_text);
                    }
                    // Prefer the lossless channel (buffered by turn end), falling
                    // back to the broadcast-captured value.
                    if let Some(s) = agent.take_summary().await.or(summary.take()) {
                        return Ok(s);
                    } else if !has_sent_failsafe {
                        has_sent_failsafe = true;
                        if let Err(e) = agent
                            .send_prompt(SendPromptArgs {
                                content: vec![ContentChunk::Text(SUMMARY_FAILSAFE_MSG.to_string())],
                                should_continue_turn: None,
                            })
                            .await
                        {
                            return Err(InternalPromptError::Failed(format!(
                                "Failed to send failsafe prompt: {e}"
                            )));
                        }
                    } else {
                        // Last resort: the subagent refused to call summary; surface its
                        // last message instead of erroring.
                        return Ok(Summary {
                            task_description: query,
                            context_summary: None,
                            task_result: no_summary_fallback(&last_message.unwrap_or_default()),
                            result_type: None,
                        });
                    }
                },
                AgentEvent::Stop(AgentStopReason::Cancelled) => {
                    // Honor a summary delivered before the cancel landed (lossless
                    // channel first); otherwise the cancelled subagent has no result.
                    if let Some(s) = agent.take_summary().await.or(summary.take()) {
                        return Ok(s);
                    }
                    return Err(InternalPromptError::Cancelled);
                },
                AgentEvent::Stop(AgentStopReason::Error(e)) => {
                    // A summary delivered before the error landed wins. The subagent
                    // commonly calls summary successfully and then takes one more
                    // model round-trip that comes back empty, erroring the turn with
                    // EmptyResponse. Without draining the lossless channel here we
                    // would discard that real result and return the empty fallback.
                    if let Some(s) = agent.take_summary().await.or(summary.take()) {
                        return Ok(s);
                    }
                    // An empty response is not a failure: before empty responses
                    // started erroring, the subagent would fall back to its last
                    // message. Preserve that behavior by degrading to the last
                    // available message instead of failing the stage.
                    if matches!(&e, AgentError::AgentLoopError(LoopError::EmptyResponse)) {
                        return Ok(Summary {
                            task_description: query,
                            context_summary: None,
                            task_result: empty_response_fallback(&last_message.unwrap_or_default()),
                            result_type: None,
                        });
                    }
                    return Err(InternalPromptError::Failed(format!("Agent error: {e}")));
                },
                _ => {},
            },
            Err(tokio::sync::broadcast::error::RecvError::Lagged(skipped)) => {
                // Harmless now: the summary rides the lossless channel; only
                // UI/telemetry updates are lost on a broadcast lag.
                tracing::warn!(%skipped, "Subagent broadcast receiver lagged; skipped events");
            },
            Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                if let Some(s) = agent.take_summary().await.or(summary.take()) {
                    return Ok(s);
                }
                return Err(InternalPromptError::Cancelled);
            },
        }
    }
}
#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::time::Duration;

    use tokio::time::timeout;

    use super::*;
    use crate::agent::rts::{
        RtsModel,
        RtsState,
    };
    use crate::api_client::model::ChatResponseStream;
    use crate::api_client::send_message_output::MockStreamItem;
    use crate::api_client::{
        ApiClient,
        MockResponseRegistryHandle,
    };

    #[tokio::test]
    async fn test_handle_internal_prompt() {
        let snapshot = agent::types::AgentSnapshot::default();
        let registry = MockResponseRegistryHandle::spawn();
        let mock_api_client = ApiClient::new_ipc_mock(registry.clone());

        let session_id = "test-session";
        let state = Arc::new(RtsState::new(session_id.to_string()));
        let agent = agent::Agent::new(
            snapshot,
            None,
            None,
            Arc::new(RtsModel::new(mock_api_client, state)),
            agent::mcp::McpManager::default().spawn(),
            true,
            None,
            None,
            None,
            Vec::new(),
            None,
        )
        .await
        .expect("Failed to create agent");
        let agent = agent.spawn();

        // Mock first response (no summary tool call — triggers failsafe)
        let mock_responses = vec![MockStreamItem::Event(ChatResponseStream::AssistantResponseEvent {
            content: "Task completed successfully".to_string(),
        })];
        registry.push_events(session_id.to_string(), Some(mock_responses)).await;
        registry.push_events(session_id.to_string(), None).await;

        // Mock second response for the failsafe turn (still no summary — triggers last-resort extraction)
        let failsafe_responses = vec![MockStreamItem::Event(ChatResponseStream::AssistantResponseEvent {
            content: "Task completed successfully".to_string(),
        })];
        registry
            .push_events(session_id.to_string(), Some(failsafe_responses))
            .await;
        registry.push_events(session_id.to_string(), None).await;

        let result = timeout(
            Duration::from_secs(5),
            handle_internal_prompt("test query".to_string(), agent),
        )
        .await
        .expect("Should not timeout")
        .expect("Should succeed");

        assert_eq!(result.task_description, "test query");
        assert_eq!(
            result.task_result,
            "The subagent ended without calling the summary tool. \
             This is the content of its last message:\n\nTask completed successfully"
        );
        assert!(result.context_summary.is_none());
    }

    /// A response stream carrying only metering + metadata events and no content.
    /// The agent layer treats this as an empty response (and retries once).
    fn empty_response_stream() -> Vec<MockStreamItem> {
        vec![
            MockStreamItem::Event(ChatResponseStream::MeteringEvent {
                usage: Some(0.1),
                unit: Some("credit".to_string()),
                unit_plural: Some("credits".to_string()),
            }),
            MockStreamItem::Event(ChatResponseStream::MetadataEvent {
                total_tokens: Some(10),
                uncached_input_tokens: Some(8),
                output_tokens: Some(2),
                cache_read_input_tokens: None,
                cache_write_input_tokens: None,
                stop_reason: None,
                refusal_category: None,
                refusal_explanation: None,
                refusal_recommended_model: None,
            }),
        ]
    }

    async fn spawn_test_agent(registry: &MockResponseRegistryHandle, session_id: &str) -> agent::AgentHandle {
        let snapshot = agent::types::AgentSnapshot::default();
        let mock_api_client = ApiClient::new_ipc_mock(registry.clone());
        let state = Arc::new(RtsState::new(session_id.to_string()));
        let agent = agent::Agent::new(
            snapshot,
            None,
            None,
            Arc::new(RtsModel::new(mock_api_client, state)),
            agent::mcp::McpManager::default().spawn(),
            true,
            None,
            None,
            None,
            Vec::new(),
            None,
        )
        .await
        .expect("Failed to create agent");
        agent.spawn()
    }

    /// An empty response after the subagent has already produced a message must
    /// degrade to that last message (wrapped in the fallback template) instead
    /// of failing the stage.
    #[tokio::test]
    async fn empty_response_with_prior_message_degrades_to_last_message() {
        let registry = MockResponseRegistryHandle::spawn();
        let session_id = "empty-with-prior";
        let agent = spawn_test_agent(&registry, session_id).await;

        // Turn 1: real content, but no summary tool — triggers the failsafe re-prompt.
        registry
            .push_events(
                session_id.to_string(),
                Some(vec![MockStreamItem::Event(
                    ChatResponseStream::AssistantResponseEvent {
                        content: "partial findings".to_string(),
                    },
                )]),
            )
            .await;
        registry.push_events(session_id.to_string(), None).await;

        // Turn 2 (failsafe) returns empty, and the retry is empty too -> empty-response error.
        for _ in 0..2 {
            registry
                .push_events(session_id.to_string(), Some(empty_response_stream()))
                .await;
            registry.push_events(session_id.to_string(), None).await;
        }

        let result = timeout(
            Duration::from_secs(5),
            handle_internal_prompt("test query".to_string(), agent),
        )
        .await
        .expect("Should not timeout")
        .expect("Empty response should degrade to Ok, not fail");

        assert_eq!(
            result.task_result,
            "The subagent returned an empty response without calling the summary tool. \
             This is the content of its last available message:\n\npartial findings"
        );
    }

    /// An empty response with no prior message degrades to an empty body rather
    /// than failing the stage.
    #[tokio::test]
    async fn empty_response_with_no_prior_message_degrades_to_empty() {
        let registry = MockResponseRegistryHandle::spawn();
        let session_id = "empty-no-prior";
        let agent = spawn_test_agent(&registry, session_id).await;

        // First (and only) turn returns empty, retry empty too -> empty-response error.
        for _ in 0..2 {
            registry
                .push_events(session_id.to_string(), Some(empty_response_stream()))
                .await;
            registry.push_events(session_id.to_string(), None).await;
        }

        let result = timeout(
            Duration::from_secs(5),
            handle_internal_prompt("test query".to_string(), agent),
        )
        .await
        .expect("Should not timeout")
        .expect("Empty response should degrade to Ok, not fail");

        assert_eq!(
            result.task_result,
            "The subagent returned an empty response without calling the summary tool. \
             This is the content of its last available message:\n\n"
        );
    }

    /// Regression for the observed failure where a subagent calls the summary
    /// tool successfully, then takes one more model round-trip that comes back
    /// empty (erroring the turn with EmptyResponse). The real summary — already
    /// delivered on the lossless channel — must win over the empty-response
    /// fallback. Before the fix, the Stop(Error(EmptyResponse)) arm returned the
    /// fallback without checking the lossless channel, discarding the result.
    #[tokio::test]
    async fn summary_then_empty_trailing_turn_keeps_summary() {
        let registry = MockResponseRegistryHandle::spawn();
        let session_id = "summary-then-empty";
        let agent = spawn_test_agent(&registry, session_id).await;

        // Turn 1: model emits text and calls the summary tool with a real result.
        let summary_input = serde_json::json!({
            "taskDescription": "tell a joke",
            "taskResult": "Why do programmers prefer dark mode? Because light attracts bugs.",
        });
        registry
            .push_events(
                session_id.to_string(),
                Some(vec![
                    MockStreamItem::Event(ChatResponseStream::AssistantResponseEvent {
                        content: "Here is a joke.".to_string(),
                    }),
                    MockStreamItem::Event(ChatResponseStream::ToolUseEvent {
                        tool_use_id: "tooluse_summary".to_string(),
                        name: "summary".to_string(),
                        input: Some(summary_input.to_string()),
                        stop: Some(true),
                    }),
                ]),
            )
            .await;
        registry.push_events(session_id.to_string(), None).await;

        // Trailing round-trip(s) after the summary tool result come back empty,
        // which errors the turn with EmptyResponse (retried once, also empty).
        for _ in 0..2 {
            registry
                .push_events(session_id.to_string(), Some(empty_response_stream()))
                .await;
            registry.push_events(session_id.to_string(), None).await;
        }

        let result = timeout(
            Duration::from_secs(5),
            handle_internal_prompt("tell a joke".to_string(), agent),
        )
        .await
        .expect("Should not timeout")
        .expect("Should return the delivered summary, not an error");

        assert_eq!(
            result.task_result, "Why do programmers prefer dark mode? Because light attracts bugs.",
            "the real summary must win over the empty-response fallback"
        );
    }
}
