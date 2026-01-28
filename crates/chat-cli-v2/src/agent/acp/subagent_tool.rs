use agent::AgentHandle;
use agent::protocol::{
    AgentEvent,
    AgentStopReason,
    ContentChunk,
    SendPromptArgs,
};
use agent::tools::summary::Summary;
use agent::tools::{
    SubagentResponse,
    ToolExecutionOutput,
    UseSubagent,
};
use eyre::bail;
use sacp::schema::SessionId;
use serde::{
    Deserialize,
    Serialize,
};
use tracing::warn;

use super::session_manager::SessionManagerHandle;
use crate::agent::acp::acp_agent::AcpSessionConfig;

const SUMMARY_FAILSAFE_MSG: &str = "You have not called the summary tool yet. Please call the summary tool now to provide your findings to the main agent before ending your task.";

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

/// This function handles the lifecycle of a subagent.
/// The execution logic of which is also evident in [agent::tools::spawn_subagent], which shall be
/// reiterated here for ease of understanding.
///
/// The spawn subagent tool has two portions, an internal and an external portion.
///
/// The internal portion refers to the interface known to the agent. The invocation of which is
/// simply an emission of an [AgentEvent::SpawnSubagentRequest].
/// This event is then intercepted on the acp layer and handled by this function, which is the
/// external portion of this tool.
///
/// The separation of the tool execution into two portions is necessitated by the fact that a
/// subagent session is also an acp session (because we want to enable this session to be
/// communicated to with the TUI, which needs to go through the protocol layer). And because acp is
/// a concept that transcends the abstraction level of agent crate, the instantiation of which
/// would need to be done outside of agent crate, hence "external".
pub(crate) async fn handle_subagent_request(
    request: agent::tools::use_subagent::SubagentRequest,
    session_tx: SessionManagerHandle,
) {
    use agent::tools::ToolExecutionOutputItem;

    let result = match &request.request {
        UseSubagent::ListAgents => {
            // TODO: Return actual list of available agents
            // Depending on how collection of agents are handled, we might need to have this live
            // somewhere else
            Ok(SubagentResponse {
                output: ToolExecutionOutput::new(vec![ToolExecutionOutputItem::Text(
                    "Available agents:\n- default (Default agent)".to_string(),
                )]),
            })
        },
        UseSubagent::InvokeSubagents { subagents } => {
            const SUBAGENT_EMBEDDED_MSG: &str = "You are a subagent executing a task delegated to you by the main agent. After what is asked of you has concluded, call the summary tool to convey your findings to the main agent.";

            // Spawn sessions and send prompts concurrently
            let mut futures = Vec::with_capacity(subagents.len());
            for invocation in subagents {
                let query = invocation.query.clone();
                let context = invocation.relevant_context.clone();
                let agent_name = invocation.agent_name.clone();
                let session_tx = session_tx.clone();

                futures.push(async move {
                    // Create a new session for this subagent
                    let session_id = SessionId::new(uuid::Uuid::new_v4().to_string());
                    let mut config =
                        AcpSessionConfig::new(session_id.to_string(), std::env::current_dir().unwrap_or_default())
                            .user_embedded_msg(SUBAGENT_EMBEDDED_MSG.to_string())
                            .is_subagent(true);

                    if let Some(name) = agent_name {
                        config = config.initial_agent_name(name);
                    }

                    let result = session_tx.start_session(&session_id, config, None).await?;

                    let prompt = match context {
                        Some(ctx) => format!("{query}\n\nContext:\n{ctx}"),
                        None => query.clone(),
                    };
                    let summary = result.handle.internal_prompt(prompt).await?;
                    Ok::<_, eyre::Report>(summary)
                });
            }

            let items = futures::future::join_all(futures)
                .await
                .into_iter()
                .map(|res| match res {
                    Ok(summary) => ToolExecutionOutputItem::Json(serde_json::json!(summary)),
                    Err(report) => ToolExecutionOutputItem::Text(report.to_string()),
                })
                .collect::<Vec<_>>();

            Ok(SubagentResponse {
                output: ToolExecutionOutput { items },
            })
        },
    };

    if let Err(e) = request.response_tx.send(result).await {
        warn!("Failed to send spawn subagent response: {}", e);
    }
}

/// Handle an internal prompt for subagent execution
pub(crate) async fn handle_internal_prompt(query: String, mut agent: AgentHandle) -> eyre::Result<Summary> {
    agent
        .send_prompt(SendPromptArgs {
            content: vec![ContentChunk::Text(query.clone())],
            should_continue_turn: None,
        })
        .await?;

    let mut summary: Option<Summary> = None;
    let mut has_sent_failsafe = false;

    loop {
        match agent.recv().await {
            Ok(event) => match event {
                AgentEvent::SubagentSummary(s) => {
                    summary = Some(s);
                },
                AgentEvent::EndTurn(_) => {
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
                            bail!("Failed to send failsafe prompt: {e}");
                        }
                    } else {
                        bail!("Subagent refused to provide summary");
                    }
                },
                AgentEvent::Stop(AgentStopReason::Error(e)) => {
                    bail!("Agent error: {e}");
                },
                _ => {},
            },
            Err(_) => {
                bail!("Agent channel closed");
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
        )
        .await
        .expect("Failed to create agent")
        .spawn();

        // Mock a complete response that includes tool call and execution
        let mock_responses = vec![
            MockStreamItem::Event(ChatResponseStream::AssistantResponseEvent {
                content: "I'll provide a summary of the task.".to_string(),
            }),
            MockStreamItem::Event(ChatResponseStream::ToolUseEvent {
                tool_use_id: "tool_123".to_string(),
                name: "summary".to_string(),
                input: Some(
                    r#"{"taskDescription": "test query", "taskResult": "Task completed successfully"}"#.to_string(),
                ),
                stop: Some(false),
            }),
        ];

        // Push mock responses
        registry.push_events(session_id.to_string(), Some(mock_responses)).await;
        registry.push_events(session_id.to_string(), None).await; // Signal completion

        let result = timeout(
            Duration::from_secs(1),
            handle_internal_prompt("test query".to_string(), agent),
        )
        .await;

        // The test should timeout because the mock doesn't provide the actual tool execution result
        // This demonstrates that the agent is waiting for the summary tool to execute and emit
        // SubagentSummary
        assert!(
            result.is_err(),
            "Expected timeout - agent should wait for SubagentSummary event"
        );
    }
}
