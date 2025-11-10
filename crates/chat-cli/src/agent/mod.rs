pub mod acp;
pub mod rts;
use std::io::Write;
use std::sync::Arc;

use agent::AgentHandle;
use agent::agent_config::load_agents;
use agent::agent_loop::protocol::LoopEndReason;
use agent::mcp::McpManager;
use agent::protocol::{
    AgentEvent,
    AgentStopReason,
    ApprovalResult,
    ContentChunk,
    SendApprovalResultArgs,
    SendPromptArgs,
    UpdateEvent,
};
use agent::tools::summary::Summary;
use agent::types::AgentSnapshot;
use chat_cli_ui::conduit::{
    ControlEnd,
    DestinationStderr,
    get_legacy_conduits,
};
use chat_cli_ui::protocol::{
    Event as UiEvent,
    MetaEvent,
    TextMessageContent,
    ToolCallEnd,
    ToolCallStart,
};
use chat_cli_ui::subagent_indicator::SubagentIndicator;
use eyre::{
    Result,
    bail,
};
use rts::{
    RtsModel,
    RtsModelState,
};
use serde::{
    Deserialize,
    Serialize,
};
use tracing::{
    debug,
    error,
    info,
    warn,
};

use crate::os::Os;
use crate::theme::StyledText;

// TODO: use the one supplied by science (this one has been modified for testing)
const SUBAGENT_EMBEDDED_USER_MSG: &str = r#"
You are a subagent executing a task delegated to you by the main agent.
After what is asked of you has concluded, call the summary tool to convey your findings to the main agent.
"#;

const SUMMARY_FAILSAFE_MSG: &str = r#"
You have not called the summary tool yet. Please call the summary tool now to provide your findings to the main agent before ending your task.
"#;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct JsonOutput {
    /// Whether or not the user turn completed successfully
    is_error: bool,
    /// Text from the final message, if available
    result: Option<String>,
    /// The number of requests sent to the model
    number_of_requests: u32,
    /// The number of tool use / tool result pairs in the turn
    ///
    /// This could be less than the number of requests in the case of retries
    number_of_cycles: u32,
    /// Duration of the turn, in milliseconds
    duration_ms: u32,
}

#[derive(Debug)]
pub struct SubAgent<'a> {
    pub query: &'a str,
    pub agent_name: Option<&'a str>,
    pub embedded_user_msg: Option<&'a str>,
    pub dangerously_trust_all_tools: bool,
}

impl<'a> SubAgent<'a> {
    pub async fn query(self, os: &mut Os, control_end: &mut ControlEnd<DestinationStderr>) -> Result<Summary> {
        let mut snapshot = AgentSnapshot::default();

        let model = {
            let rts_state: RtsModelState = snapshot
                .model_state
                .as_ref()
                .and_then(|s| {
                    serde_json::from_value(s.clone())
                        .map_err(|err| error!(?err, ?s, "failed to deserialize RTS state"))
                        .ok()
                })
                .unwrap_or({
                    let state = RtsModelState::new();
                    info!(?state.conversation_id, "generated new conversation id");
                    state
                });
            Arc::new(RtsModel::new(
                os.client.clone(),
                rts_state.conversation_id,
                rts_state.model_id,
            ))
        };

        if let Some(name) = self.agent_name {
            let (configs, _) = load_agents().await?;
            if let Some(cfg) = configs.into_iter().find(|c| c.name() == name) {
                snapshot.agent_config = cfg.config().clone();
            } else {
                bail!("unable to find agent with name: {}", name);
            }
        };

        let mcp_manager_handle = McpManager::default().spawn();
        let mut agent = agent::Agent::new(snapshot, model, mcp_manager_handle).await?;
        if let Some(msg) = self.embedded_user_msg {
            agent.set_embedded_user_msg(msg);
        }

        let agent_handle = agent.spawn();

        self.main_loop(agent_handle, control_end).await
    }

    async fn main_loop(
        &self,
        mut agent: AgentHandle,
        control_end: &mut ControlEnd<DestinationStderr>,
    ) -> Result<Summary> {
        // First, wait for agent initialization
        while let Ok(evt) = agent.recv().await {
            if matches!(evt, AgentEvent::Mcp(_)) {
                info!(?evt, "received mcp agent event");
                // TODO: Send it through conduit
            }
            if matches!(evt, AgentEvent::Initialized) {
                break;
            }
        }

        agent
            .send_prompt(SendPromptArgs {
                content: vec![ContentChunk::Text(self.query.to_string())],
                should_continue_turn: None,
            })
            .await?;

        // Holds the final result of the user turn.
        #[allow(unused_assignments)]
        let mut user_turn_metadata = None;
        let mut query_result = None::<Summary>;

        loop {
            let Ok(evt) = agent.recv().await else {
                bail!("channel closed");
            };
            debug!(?evt, "received new agent event");

            // Check for exit conditions
            match evt {
                AgentEvent::Update(evt) => {
                    info!(?evt, "received update event");

                    match evt {
                        UpdateEvent::ToolCall(tool_call) => {
                            _ = control_end.send(UiEvent::ToolCallStart(ToolCallStart {
                                tool_call_id: tool_call.id,
                                tool_call_name: tool_call.tool_use_block.name,
                                parent_message_id: None,
                                mcp_server_name: None,
                                is_trusted: true,
                            }));
                        },
                        UpdateEvent::ToolCallFinished { tool_call, result: _ } => {
                            _ = control_end.send(UiEvent::ToolCallEnd(ToolCallEnd {
                                tool_call_id: tool_call.id,
                            }));
                        },
                        UpdateEvent::AgentContent(_content) => {
                            // TODO: send actual content (for preview?)
                            _ = control_end.send(UiEvent::TextMessageContent(TextMessageContent {
                                message_id: Default::default(),
                                delta: Default::default(),
                            }));
                        },
                        _ => {},
                    }
                },
                AgentEvent::EndTurn(metadata) => {
                    if query_result.is_some() {
                        user_turn_metadata = Some(metadata.clone());
                        break;
                    } else {
                        agent
                            .send_prompt(SendPromptArgs {
                                content: vec![ContentChunk::Text(SUMMARY_FAILSAFE_MSG.to_string())],
                                should_continue_turn: None,
                            })
                            .await?;
                    }
                },
                AgentEvent::Stop(AgentStopReason::Error(agent_error)) => {
                    bail!("agent encountered an error: {:?}", agent_error)
                },
                AgentEvent::ApprovalRequest { id, tool_use, .. } => {
                    if !self.dangerously_trust_all_tools {
                        bail!("Tool approval is required: {:?}", tool_use);
                    } else {
                        warn!(?tool_use, "trust all is enabled, ignoring approval request");
                        agent
                            .send_tool_use_approval_result(SendApprovalResultArgs {
                                id: id.clone(),
                                result: ApprovalResult::Approve,
                            })
                            .await?;
                    }
                },
                AgentEvent::Mcp(evt) => {
                    info!(?evt, "received mcp agent event");
                },
                AgentEvent::SubagentSummary(summary) => {
                    query_result.replace(summary);
                },
                _ => {},
            }
        }

        let md = user_turn_metadata.expect("user turn metadata should exist");
        let is_error = md.end_reason != LoopEndReason::UserTurnEnd || md.result.as_ref().is_none_or(|v| v.is_err());
        let result = md.result.and_then(|r| r.ok().map(|m| m.text()));

        let output = JsonOutput {
            result,
            is_error,
            number_of_requests: md.total_request_count,
            number_of_cycles: md.number_of_cycles,
            duration_ms: md.turn_duration.map(|d| d.as_millis() as u32).unwrap_or_default(),
        };

        info!(?output, "sub agent routine completed");

        query_result.ok_or(eyre::eyre!("subagent missing query result"))
    }
}

pub fn temp_func() {
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("failed to build runtime");

    let summary = rt.block_on(test_sub_agent_routine());
    println!("summary: {summary:#?}");
}

async fn test_sub_agent_routine() -> Summary {
    let sub_agent = SubAgent {
        query: "What notion docs do I have",
        agent_name: Some("test_test"),
        embedded_user_msg: Some(SUBAGENT_EMBEDDED_USER_MSG),
        dangerously_trust_all_tools: true,
    };

    let mut os = Os::new().await.expect("failed to spawn os");
    let (view_end, _byte_receiver, mut control_end_stderr, _control_end_stdout) = get_legacy_conduits(true);
    let subagent_indicator = SubagentIndicator::new("test_test", "notion doc search", view_end);
    let _guard = subagent_indicator.run();

    sub_agent
        .query(&mut os, &mut control_end_stderr)
        .await
        .expect("failed to retrieve summary")
}
