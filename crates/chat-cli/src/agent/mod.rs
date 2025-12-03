pub mod acp;
pub mod rts;
use std::io::Write;
use std::path::PathBuf;
use std::sync::Arc;

use agent::AgentHandle;
use agent::agent_config::load_agents;
use agent::agent_loop::protocol::UserTurnMetadata;
use agent::mcp::{
    McpManager,
    McpServerEvent,
};
use agent::protocol::{
    AgentEvent,
    AgentStopReason,
    ApprovalResult,
    ContentChunk,
    InitializeUpdateEvent,
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
    AgentEvent as AgentEventForUi,
    AgentEventKind,
    InputEvent,
    InputEventKind,
    McpEvent as UiMcpEvent,
    MetaEvent,
    SessionEvent,
    TextMessageContent,
    ToolCallEnd,
    ToolCallPermissionRequest,
    ToolCallStart,
};
use chat_cli_ui::subagent_indicator::{
    SubagentExecutionSummary,
    SubagentIndicator,
};
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
use crate::telemetry::TelemetryThread;
use crate::util::paths::PathResolver;

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
    // TODO: inherit this from the main session?
    pub dangerously_trust_all_tools: bool,
    pub local_agent_path: &'a PathBuf,
    pub global_agent_path: &'a PathBuf,
    pub local_mcp_path: &'a PathBuf,
    pub global_mcp_path: &'a PathBuf,
}

impl<'a> Subagent<'a> {
    pub async fn query<D>(
        self,
        os: &Os,
        input_rx: broadcast::Receiver<InputEvent>,
        mut control_end: ControlEnd<D>,
        parent_conversation_id: &str,
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
            let (configs, _) = load_agents(self.local_agent_path, self.global_agent_path).await?;
            if let Some(cfg) = configs.into_iter().find(|c| c.name() == name) {
                snapshot.agent_config = cfg.config().clone();
            } else {
                bail!("unable to find agent with name: {}", name);
            }
        };

        let mcp_manager_handle = McpManager::default().spawn();
        let mut agent = agent::Agent::new(
            snapshot,
            Some(self.local_mcp_path),
            Some(self.global_mcp_path),
            model,
            mcp_manager_handle,
        )
        .await?;
        agent.push_embedded_user_msg(SUBAGENT_EMBEDDED_USER_MSG);
        if let Some(msg) = self.embedded_user_msg {
            agent.push_embedded_user_msg(msg);
        }

        let agent_handle = agent.spawn();
        let telemetry_thread = &os.telemetry;

        self.main_loop(
            agent_handle,
            input_rx,
            &mut control_end,
            telemetry_thread,
            parent_conversation_id,
        )
        .await
    }

    async fn main_loop<D>(
        &self,
        mut agent: AgentHandle,
        mut input_rx: broadcast::Receiver<InputEvent>,
        control_end: &mut ControlEnd<D>,
        telemetry_thread: &TelemetryThread,
        parent_conversation_id: &str,
    ) -> Result<Summary> {
        // First, wait for agent initialization
        loop {
            tokio::select! {
                // While we wait we would still need to handle user input
                input_evt = input_rx.recv() => {
                    let Ok(input_evt) = input_evt else {
                        bail!("input channel closed");
                    };

                    let InputEvent { agent_id: _, kind } = input_evt;

                    if let InputEventKind::Interrupt = kind {
                        bail!("user interrupted");
                    }
                },

                agent_evt = agent.recv() => {
                    let Ok(agent_evt) = agent_evt else {
                        bail!("agent loop channel closed");
                    };

                    match agent_evt {
                        AgentEvent::InitializeUpdate(initialize_update_evt) => {
                            let ui_mcp_event = match initialize_update_evt {
                                InitializeUpdateEvent::Mcp(evt) => match evt {
                                    McpServerEvent::Initialized { server_name, .. } => UiMcpEvent::LoadSuccess { server_name },
                                    McpServerEvent::InitializeError { server_name, error } => {
                                        UiMcpEvent::LoadFailure { server_name, error }
                                    },
                                    McpServerEvent::OauthRequest { server_name, oauth_url } => {
                                        UiMcpEvent::OauthRequest { server_name, oauth_url }
                                    },
                                    McpServerEvent::Initializing { server_name } => UiMcpEvent::Loading { server_name },
                                }
                            };
                            _ = control_end.send(SessionEvent::AgentEvent(chat_cli_ui::protocol::AgentEvent {
                                agent_id: self.id,
                                kind: AgentEventKind::McpEvent(ui_mcp_event)
                            }));
                        },
                        // We need to wait until the agent is initialized before moving on
                        AgentEvent::Initialized => {
                            break;
                        },
                        _ => {},
                    }
                },
            }
        }

        agent
            .send_prompt(SendPromptArgs {
                content: vec![ContentChunk::Text(self.query.to_string())],
                should_continue_turn: None,
            })
            .await?;

        // Holds the final result of the user turn.
        let mut user_turn_metadata = Vec::<UserTurnMetadata>::new();
        let mut query_result = None::<Summary>;

        loop {
            tokio::select! {
                input_evt = input_rx.recv() => {
                    let Ok(input_evt) = input_evt else {
                        bail!("input channel closed");
                    };
                    debug!(?input_evt, "received new input event");

                    let InputEvent { agent_id, kind } = input_evt;

                    if agent_id.is_none_or(|id| self.id != id) {
                        continue;
                    }


                    match kind {
                        InputEventKind::Text(_) => {},
                        InputEventKind::Interrupt => {
                            agent.cancel().await?;
                            break;
                        },
                        InputEventKind::ToolApproval(id) => {
                            agent
                                .send_tool_use_approval_result(SendApprovalResultArgs {
                                    id,
                                    result: ApprovalResult::Approve,
                                })
                                .await?;
                        },
                        InputEventKind::ToolRejection(id) => {
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
                                    _ = control_end.send(SessionEvent::AgentEvent(AgentEventForUi {
                                        agent_id: self.id,
                                        kind: AgentEventKind::ToolCallStart(
                                            ToolCallStart {
                                                tool_call_id: tool_call.id,
                                                tool_call_name: tool_call.tool_use_block.name,
                                                parent_message_id: None,
                                                mcp_server_name: None,
                                                is_trusted: true,
                                            }
                                        )
                                    }));
                                },
                                UpdateEvent::ToolCallFinished { tool_call, result: _ } => {
                                    _ = control_end.send(SessionEvent::AgentEvent(AgentEventForUi {
                                        agent_id: self.id,
                                        kind: AgentEventKind::ToolCallEnd(
                                            ToolCallEnd {
                                                tool_call_id: tool_call.id,
                                            }
                                        )
                                    }));
                                },
                                UpdateEvent::AgentContent(content) => {
                                    if let ContentChunk::Text(text) = content {
                                        _ = control_end.send(SessionEvent::AgentEvent(AgentEventForUi {
                                            agent_id: self.id,
                                            kind: AgentEventKind::TextMessageContent(
                                                TextMessageContent {
                                                    message_id: Default::default(),
                                                    delta: text.into_bytes(),
                                                }
                                            )
                                        }));
                                    } else {
                                        _ = control_end.send(SessionEvent::AgentEvent(AgentEventForUi {
                                            agent_id: self.id,
                                            kind: AgentEventKind::TextMessageContent(
                                                TextMessageContent {
                                                    message_id: Default::default(),
                                                    delta: Default::default(),
                                                }
                                            )
                                        }));
                                    }
                                },
                                _ => {},
                            }
                        },
                        AgentEvent::EndTurn(metadata) => {
                            if query_result.is_some() {
                                user_turn_metadata.push(metadata.clone());
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
                                _ = control_end.send(SessionEvent::AgentEvent(AgentEventForUi {
                                    agent_id: self.id,
                                    kind: AgentEventKind::ToolCallPermissionRequest(
                                        ToolCallPermissionRequest {
                                            tool_call_id: tool_use.tool_use_id,
                                            name: tool_use.name,
                                            input: tool_use.input,
                                        }
                                    )
                                }));
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

        let md = user_turn_metadata
            .iter()
            .fold(SubagentExecutionSummary::default(), |mut acc, md| {
                let tool_call_count = acc.tool_call_count.get_or_insert(0);
                *tool_call_count = tool_call_count.saturating_add(md.number_of_cycles);

                acc.token_count = acc.token_count.saturating_add(md.token_count);

                if let Some(turn_duration) = md.turn_duration.as_ref() {
                    let duration = acc.duration.get_or_insert(std::time::Duration::from_secs(0));
                    *duration = duration.saturating_add(*turn_duration);
                }

                acc
            });
        let token_count = Some(md.token_count as i64);
        let tool_call_count = md.tool_call_count.map(|count| count as i64);

        _ = telemetry_thread.send_subagent_invocation(parent_conversation_id.to_string(), token_count, tool_call_count);

        // TODO: do we want to set a special variant for this so we don't have to marshall and
        // unmarshall?
        if let Ok(payload) = serde_json::to_value(md) {
            _ = control_end.send(SessionEvent::AgentEvent(AgentEventForUi {
                agent_id: self.id,
                kind: AgentEventKind::MetaEvent(MetaEvent {
                    meta_type: "EndTurn".to_string(),
                    payload,
                }),
            }));
        }

        query_result.ok_or(eyre::eyre!("subagent missing query result"))
    }
}

/// Tests the subagent widget in isolation without requiring a full chat session.
///
/// This function creates a standalone runtime and executes multiple subagent queries
/// concurrently to demonstrate and test the subagent widget functionality. It's primarily
/// used for development and testing purposes.
///
/// # Arguments
///
/// * `queries` - A vector of tuples containing (agent_name, query_text) pairs. Each tuple
///   represents a subagent that will be spawned with the specified agent configuration and query.
#[allow(dead_code)]
pub fn subagent_widget_demo(queries: Vec<(String, String)>) {
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("failed to build runtime");

    let summaries = rt.block_on(test_sub_agent_routine(queries));

    println!("summaries: {summaries:#?}");
}

#[allow(dead_code)]
async fn test_sub_agent_routine(queries: Vec<(String, String)>) -> Vec<Result<Summary>> {
    let os = Os::new().await.expect("failed to spawn os");
    let resolver = PathResolver::new(&os);
    let local_agent_path = resolver.workspace().agents_dir().expect("failed to retrieve path");
    let global_agent_path = resolver.global().agents_dir().expect("failed to retrieve path");
    let local_mcp_path = resolver.workspace().mcp_config().expect("failed to retrieve path");
    let global_mcp_path = resolver.global().mcp_config().expect("failed to retrieve path");
    let subagents = queries
        .iter()
        .enumerate()
        .map(|(id, (agent_name, query))| Subagent {
            id: id as u16,
            query: query.as_str(),
            agent_name: Some(agent_name.as_str()),
            embedded_user_msg: None,
            dangerously_trust_all_tools: false,
            local_agent_path: &local_agent_path,
            global_agent_path: &global_agent_path,
            local_mcp_path: &local_mcp_path,
            global_mcp_path: &global_mcp_path,
        })
        .collect::<Vec<_>>();

    let stub_id = "";
    let (view_end, input_rx, control_end) = get_conduit();
    let subagent_indicator = SubagentIndicator::new(
        &subagents
            .iter()
            .map(|subagent| (subagent.agent_name.unwrap_or(DEFAULT_AGENT_NAME), subagent.query))
            .collect::<Vec<(&str, &str)>>(),
        view_end,
    );

    let mut indicator_handle = subagent_indicator.run();

    let res = futures::future::join_all(
        subagents
            .into_iter()
            .map(|subagent| subagent.query(&os, input_rx.resubscribe(), control_end.clone(), stub_id)),
    )
    .await;

    _ = indicator_handle.wait_for_clean_screen().await;

    res
}
