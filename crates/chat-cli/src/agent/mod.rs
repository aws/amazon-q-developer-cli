pub mod acp;
pub mod rts;
use std::io::Write;
use std::sync::Arc;

use agent::AgentHandle;
use agent::agent_config::load_agents;
use agent::agent_loop::protocol::LoopEndReason;
use agent::mcp::{
    McpManager,
    McpServerEvent,
};
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
use agent::types::{
    AgentSettings,
    AgentSnapshot,
};
use chat_cli_ui::conduit::{
    ControlEnd,
    get_conduit,
};
use chat_cli_ui::protocol::{
    InputEvent,
    McpEvent as UiMcpEvent,
    TextMessageContent,
    ToolCallEnd,
    ToolCallPermissionRequest,
    ToolCallStart,
    UiEvent,
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
use tokio::sync::broadcast;
use tracing::{
    debug,
    error,
    info,
    warn,
};

use crate::constants::DEFAULT_AGENT_NAME;
use crate::os::Os;

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
pub struct Subagent<'a> {
    pub id: u16,
    pub query: &'a str,
    pub agent_name: Option<&'a str>,
    pub embedded_user_msg: Option<&'a str>,
    pub dangerously_trust_all_tools: bool,
}

impl<'a> Subagent<'a> {
    pub async fn query<D>(
        self,
        os: &Os,
        input_rx: broadcast::Receiver<InputEvent>,
        mut control_end: ControlEnd<D>,
    ) -> Result<Summary> {
        let mut snapshot = AgentSnapshot {
            settings: AgentSettings {
                // one day
                mcp_init_timeout: std::time::Duration::from_secs(86400),
            },
            ..Default::default()
        };

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
        agent.push_embedded_user_msg(SUBAGENT_EMBEDDED_USER_MSG);
        if let Some(msg) = self.embedded_user_msg {
            agent.push_embedded_user_msg(msg);
        }

        let agent_handle = agent.spawn();

        self.main_loop(agent_handle, input_rx, &mut control_end).await
    }

    async fn main_loop<D>(
        &self,
        mut agent: AgentHandle,
        mut input_rx: broadcast::Receiver<InputEvent>,
        control_end: &mut ControlEnd<D>,
    ) -> Result<Summary> {
        // First, wait for agent initialization
        while let Ok(evt) = agent.recv().await {
            match evt {
                AgentEvent::Mcp(evt) => {
                    let ui_mcp_event = match evt {
                        McpServerEvent::Initialized { server_name, .. } => UiMcpEvent::LoadSuccess { server_name },
                        McpServerEvent::InitializeError { server_name, error } => {
                            UiMcpEvent::LoadFailure { server_name, error }
                        },
                        McpServerEvent::OauthRequest { server_name, oauth_url } => {
                            UiMcpEvent::OauthRequest { server_name, oauth_url }
                        },
                        McpServerEvent::Initializing { server_name } => UiMcpEvent::Loading { server_name },
                    };
                    _ = control_end.send(UiEvent::McpEvent {
                        agent_id: self.id,
                        inner: ui_mcp_event,
                    });
                },
                // We need to wait until the agent is initialized before moving on
                AgentEvent::Initialized => {
                    break;
                },
                _ => {},
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
            tokio::select! {
                evt = input_rx.recv() => {
                    let Ok(evt) = evt else {
                        bail!("input channel closed");
                    };
                    if self.id != evt.get_id() {
                        continue;
                    }
                    debug!(?evt, "received new input event");

                    match evt {
                        InputEvent::Text { .. } => {},
                        InputEvent::Interrupt { .. } => {
                            agent.cancel().await?;
                            break;
                        },
                        InputEvent::ToolApproval { inner: id, .. } => {
                            agent
                                .send_tool_use_approval_result(SendApprovalResultArgs {
                                    id,
                                    result: ApprovalResult::Approve,
                                })
                                .await?;
                        },
                        InputEvent::ToolRejection { inner: id, .. } => {
                            agent
                                .send_tool_use_approval_result(SendApprovalResultArgs {
                                    id,
                                    result: ApprovalResult::Deny { reason: Some("User rejected this tool. Find an alternative or report inability to proceed.".to_string()) },
                                })
                                .await?;
                        },
                    }
                },

                evt = agent.recv() => {
                    let Ok(evt) = evt else {
                        bail!("channel closed");
                    };
                    debug!(?evt, "received new agent event");

                    // Check for exit conditions
                    match evt {
                        AgentEvent::Update(evt) => {
                            info!(?evt, "received update event");

                            match evt {
                                UpdateEvent::ToolCall(tool_call) => {
                                    _ = control_end.send(UiEvent::ToolCallStart {
                                        agent_id: self.id,
                                        inner: ToolCallStart {
                                            tool_call_id: tool_call.id,
                                            tool_call_name: tool_call.tool_use_block.name,
                                            parent_message_id: None,
                                            mcp_server_name: None,
                                            is_trusted: true,
                                        },
                                    });
                                },
                                UpdateEvent::ToolCallFinished { tool_call, result: _ } => {
                                    _ = control_end.send(UiEvent::ToolCallEnd {
                                        agent_id: self.id,
                                        inner: ToolCallEnd {
                                            tool_call_id: tool_call.id,
                                        },
                                    });
                                },
                                UpdateEvent::AgentContent(_content) => {
                                    // TODO: send actual content (for preview?)
                                    _ = control_end.send(UiEvent::TextMessageContent {
                                        agent_id: self.id,
                                        inner: TextMessageContent {
                                            message_id: Default::default(),
                                            delta: Default::default(),
                                        },
                                    });
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
                                _ = control_end.send(UiEvent::ToolCallPermissionRequest {
                                    agent_id: self.id,
                                    inner: ToolCallPermissionRequest {
                                        tool_call_id: tool_use.tool_use_id,
                                        name: tool_use.name,
                                        input: tool_use.input,
                                    }
                                });
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
                        AgentEvent::SubagentSummary(summary) => {
                            query_result.replace(summary);
                        },
                        _ => {},
                    }
                },
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

    crossterm::terminal::enable_raw_mode().ok();
    let summaries = rt.block_on(test_sub_agent_routine());
    crossterm::terminal::disable_raw_mode().ok();

    println!("summaries: {summaries:#?}");
}

async fn test_sub_agent_routine() -> Vec<Result<Summary>> {
    let subagents = [
        Subagent {
            id: 0_u16,
            query: "What notion docs do I have",
            agent_name: Some("test_test"),
            embedded_user_msg: None,
            dangerously_trust_all_tools: false,
        },
        Subagent {
            id: 1_u16,
            query: "When was the latest notion doc I have created",
            agent_name: Some("test_test"),
            embedded_user_msg: None,
            dangerously_trust_all_tools: false,
        },
    ];

    let os = Os::new().await.expect("failed to spawn os");
    let (view_end, input_rx, control_end) = get_conduit();
    let subagent_indicator = SubagentIndicator::new(
        &subagents
            .iter()
            .map(|subagent| (subagent.agent_name.unwrap_or(DEFAULT_AGENT_NAME), subagent.query))
            .collect::<Vec<(&str, &str)>>(),
        view_end,
    );

    let _guards = subagent_indicator.run();

    futures::future::join_all(
        subagents
            .into_iter()
            .map(|subagent| subagent.query(&os, input_rx.resubscribe(), control_end.clone())),
    )
    .await
}
