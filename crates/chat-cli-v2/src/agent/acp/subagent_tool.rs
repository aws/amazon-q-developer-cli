use agent::AgentHandle;
use agent::protocol::{
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

    loop {
        match agent.recv().await {
            Ok(event) => match event {
                AgentEvent::SubagentSummary(s) => {
                    summary = Some(s);
                },
                AgentEvent::EndTurn(metadata) => {
                    if let Some(s) = summary {
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
                        // Last resort: extract from final message
                        let text = metadata
                            .result
                            .and_then(|r| r.ok())
                            .map(|msg| msg.text())
                            .unwrap_or_default();
                        return Ok(Summary {
                            task_description: query,
                            context_summary: None,
                            task_result: text,
                        });
                    }
                },
                AgentEvent::Stop(AgentStopReason::Cancelled) => {
                    return Err(InternalPromptError::Cancelled);
                },
                AgentEvent::Stop(AgentStopReason::Error(e)) => {
                    return Err(InternalPromptError::Failed(format!("Agent error: {e}")));
                },
                _ => {},
            },
            Err(tokio::sync::broadcast::error::RecvError::Lagged(skipped)) => {
                tracing::warn!(%skipped, "Subagent broadcast receiver lagged; skipped events");
            },
            Err(tokio::sync::broadcast::error::RecvError::Closed) => {
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
        assert_eq!(result.task_result, "Task completed successfully");
        assert!(result.context_summary.is_none());
    }
}
