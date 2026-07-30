use std::path::PathBuf;
use std::sync::Arc;

use agent::AgentHandle;
use agent::agent_config::{
    build_default_agent,
    load_agents,
};
use agent::agent_loop::protocol::{
    LoopEndReason,
    LoopError,
    UserTurnMetadata,
};
use agent::mcp::{
    McpManager,
    McpServerEvent,
};
use agent::protocol::{
    AgentError,
    AgentEvent,
    AgentStopReason,
    ApprovalRequest,
    ApprovalResult,
    ContentChunk,
    InitializeUpdateEvent,
    PermissionOptionId,
    SendApprovalResultArgs,
    SendPromptArgs,
    ToolCallFailureReason,
    ToolCallResult,
    UpdateEvent,
};
use agent::tools::summary::Summary;
use agent::types::{
    AgentSettings,
    AgentSnapshot,
};
use agent::util::providers::{
    CwdProvider,
    RealProvider,
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
use kiro_telemetry::metric;
use rts::{
    RtsModel,
    RtsModelState,
};
use serde::{
    Deserialize,
    Serialize,
};
use serde_json::Number;
use tokio::sync::broadcast;
use tracing::{
    debug,
    error,
    info,
    trace,
    warn,
};

use super::rts;
use crate::constants::DEFAULT_AGENT_NAME;
use crate::os::Os;
use crate::telemetry::core::{
    ChatConversationType,
    RecordUserTurnCompletionArgs,
    ToolUseEventBuilder,
};
use crate::telemetry::{
    TelemetryResult,
    TelemetryThread,
};

// TODO: use the one supplied by science (this one has been modified for testing)
const SUBAGENT_EMBEDDED_USER_MSG: &str = r#"
CRITICAL: You MUST call the `summary` tool before ending your turn. Do NOT end with a plain text response — always close out by calling the summary tool with your findings.

You are a subagent executing a task delegated to you by the main agent.
Reminder: When your task is complete, you MUST call the summary tool (not just respond with text).
"#;

const SUMMARY_FAILSAFE_MSG: &str = r#"
You have not called the summary tool yet. Please call the summary tool now to provide your findings to the main agent before ending your task.
"#;

const CONTEXT_START: &str = r#"
=== Subagent Task Context Start ===
"#;

const CONTEXT_END: &str = r#"
=== Subagent Task Context End ===
"#;

/// Wrap the subagent's last message when its turn ended in an empty response,
/// which degrades to a result rather than failing the stage (mirrors v2 / #3075).
fn empty_response_fallback(last_message: &str) -> String {
    format!(
        "The subagent returned an empty response without calling the summary tool. \
         This is the content of its last available message:\n\n{last_message}"
    )
}

/// Build the [`Summary`] for a terminal `Stop(Error)`: an empty response degrades
/// to the last message; every other error reports a failure.
fn disposition_for_stop_error(query: &str, error: &AgentError, last_message: Option<&str>) -> Summary {
    if matches!(error, AgentError::AgentLoopError(LoopError::EmptyResponse)) {
        Summary {
            task_description: query.to_string(),
            context_summary: None,
            task_result: empty_response_fallback(last_message.unwrap_or_default()),
            result_type: None,
        }
    } else {
        Summary {
            task_description: query.to_string(),
            context_summary: None,
            task_result: format!("subagent has failed due to the following error: {error:?}"),
            result_type: None,
        }
    }
}

/// Whether a terminal `Stop(Error)` should be surfaced as a successful (degraded)
/// result rather than an error. Only empty-response degrades.
fn stop_error_is_degradable(error: &AgentError) -> bool {
    matches!(error, AgentError::AgentLoopError(LoopError::EmptyResponse))
}

// TODO: Generalize this and reuse this elsewhere
struct TelemetrySink<'a> {
    parent_conversation_id: &'a str,
    parent_tool_use_id: &'a str,
    subagent_name: &'a str,
    builtin_tool_uses: u32,
    mcp_tool_uses: u32,
    record_user_turn_completion_args: Option<RecordUserTurnCompletionArgs>,
    telemetry_result: Option<TelemetryResult>,
    telemetry_thread: &'a TelemetryThread,
}

impl<'a> TelemetrySink<'a> {
    fn new(
        parent_conversation_id: &'a str,
        parent_tool_use_id: &'a str,
        subagent_name: &'a str,
        telemetry_thread: &'a TelemetryThread,
    ) -> Self {
        Self {
            parent_conversation_id,
            parent_tool_use_id,
            subagent_name,
            builtin_tool_uses: 0,
            mcp_tool_uses: 0,
            record_user_turn_completion_args: None,
            telemetry_result: None,
            telemetry_thread,
        }
    }

    fn update_stop_reason(&mut self, stop_reason: String) {
        self.telemetry_result = Some(TelemetryResult::Failed);
        let args = self
            .record_user_turn_completion_args
            .get_or_insert(RecordUserTurnCompletionArgs {
                is_subagent: true,
                parent_tool_use_id: Some(self.parent_tool_use_id.to_string()),
                emit_user_turn_counter: true,
                emit_turn_numeric_metrics: Some(false),
                ..Default::default()
            });
        args.reason.replace(stop_reason);
    }
}

fn accumulate_subagent_turn(
    args: &mut RecordUserTurnCompletionArgs,
    metadata: &UserTurnMetadata,
    parent_tool_use_id: &str,
) {
    args.is_subagent = true;
    args.parent_tool_use_id = Some(parent_tool_use_id.to_string());
    args.emit_user_turn_counter = true;
    args.emit_turn_numeric_metrics = Some(true);
    if metadata.model.is_some() {
        args.model.clone_from(&metadata.model);
    }
    args.message_ids
        .extend(metadata.message_ids.iter().filter_map(Clone::clone));
    args.chat_conversation_type = Some(
        if metadata.number_of_cycles > 0 || matches!(args.chat_conversation_type, Some(ChatConversationType::ToolUse)) {
            ChatConversationType::ToolUse
        } else {
            ChatConversationType::NotToolUse
        },
    );
    args.user_prompt_length = args
        .user_prompt_length
        .saturating_add(metadata.user_prompt_length.min(i64::MAX as usize) as i64);
    args.assistant_response_length = args
        .assistant_response_length
        .saturating_add(metadata.assistant_response_length.min(i64::MAX as usize) as i64);
    args.follow_up_count = args
        .follow_up_count
        .saturating_add(i64::from(metadata.total_request_count.saturating_sub(1)));
    args.user_turn_duration_seconds = args.user_turn_duration_seconds.saturating_add(
        metadata
            .turn_duration
            .map_or(0, |duration| duration.as_secs().min(i64::MAX as u64) as i64),
    );
    add_token_count(&mut args.uncached_input_tokens, metadata.input_token_count);
    add_token_count(&mut args.output_tokens, metadata.output_token_count);
    add_token_count(&mut args.cache_read_input_tokens, metadata.cache_read_input_token_count);
    add_token_count(
        &mut args.cache_write_input_tokens,
        metadata.cache_write_input_token_count,
    );
    let total_tokens = metadata
        .input_token_count
        .saturating_add(metadata.output_token_count)
        .saturating_add(metadata.cache_read_input_token_count);
    add_token_count(&mut args.total_tokens, total_tokens);
    if let Some(attempts) = metadata.request_attempts {
        args.request_attempts = Some(args.request_attempts.unwrap_or(0).saturating_add(attempts));
    }
    if !matches!(metadata.end_reason, LoopEndReason::UserTurnEnd) {
        args.reason = Some(metadata.end_reason.to_string());
    }
}

fn add_token_count(total: &mut Option<i64>, value: u32) {
    if value > 0 {
        *total = Some(total.unwrap_or(0).saturating_add(i64::from(value)));
    }
}

fn telemetry_result(metadata: &[UserTurnMetadata], existing: Option<TelemetryResult>) -> Option<TelemetryResult> {
    if matches!(existing, Some(TelemetryResult::Failed)) {
        existing
    } else if metadata
        .iter()
        .any(|metadata| matches!(metadata.end_reason, LoopEndReason::DidNotRun | LoopEndReason::Error))
    {
        Some(TelemetryResult::Failed)
    } else if metadata
        .iter()
        .any(|metadata| matches!(metadata.end_reason, LoopEndReason::Cancelled))
    {
        Some(TelemetryResult::Cancelled)
    } else if metadata.is_empty() {
        existing
    } else {
        Some(TelemetryResult::Succeeded)
    }
}

fn subagent_tool_use_event(
    conversation_id: &str,
    tool_use_id: String,
    tool_name: String,
    mcp_server_name: Option<String>,
    outcome: metric::ToolMetricOutcome,
    is_valid: Option<bool>,
    reason_desc: Option<String>,
) -> ToolUseEventBuilder {
    let mut event = ToolUseEventBuilder::new(conversation_id.to_string(), tool_use_id, None);
    event.tool_name = Some(tool_name);
    event.is_custom_tool = mcp_server_name.is_some();
    event.mcp_server_name = mcp_server_name;
    event.is_accepted = outcome != metric::ToolMetricOutcome::Denied;
    event.is_valid = is_valid;
    event.is_success = match outcome {
        metric::ToolMetricOutcome::Success => Some(true),
        metric::ToolMetricOutcome::Error => Some(false),
        metric::ToolMetricOutcome::Denied
        | metric::ToolMetricOutcome::Cancelled
        | metric::ToolMetricOutcome::Unknown => None,
    };
    event.reason_desc = reason_desc;
    event
}

impl<'a> Drop for TelemetrySink<'a> {
    fn drop(&mut self) {
        _ = self.telemetry_thread.send_subagent_invocation(
            self.parent_conversation_id.to_string(),
            self.subagent_name.to_string(),
            self.builtin_tool_uses,
            self.mcp_tool_uses,
            self.parent_tool_use_id.to_string(),
        );

        let mut args = self.record_user_turn_completion_args.take();
        if let Some(args) = args.as_mut() {
            args.is_subagent = true;
            args.parent_tool_use_id = Some(self.parent_tool_use_id.to_string());
        }
        let result = self.telemetry_result.take();
        let conversation_id = self.parent_conversation_id.to_string();

        match (args, result) {
            (Some(args), Some(result)) => {
                _ = self
                    .telemetry_thread
                    .send_subagent_record_user_turn_completion(conversation_id, result, args);
            },
            (Some(args), None) => {
                let result = TelemetryResult::Cancelled;
                _ = self
                    .telemetry_thread
                    .send_subagent_record_user_turn_completion(conversation_id, result, args);
            },
            (_, _) => {},
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(dead_code)]
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
    pub task_context: Option<&'a str>,
    pub dangerously_trust_all_tools: bool,
    pub is_interactive: bool,
    pub local_mcp_path: &'a PathBuf,
    pub global_mcp_path: &'a PathBuf,
    pub parent_tool_use_id: &'a str,
    pub code_intelligence: Option<std::sync::Arc<tokio::sync::RwLock<code_agent_sdk::CodeIntelligence>>>,
    pub registry_data: Option<&'a crate::mcp_registry::McpRegistryResponse>,
    pub web_tools_enabled: bool,
}

impl<'a> Subagent<'a> {
    pub async fn query<D>(
        self,
        os: &Os,
        input_rx: broadcast::Receiver<InputEvent>,
        mut control_end: ControlEnd<D>,
        parent_conversation_id: &str,
    ) -> Result<Summary> {
        let cwd = RealProvider.cwd().unwrap_or_default();
        let mut snapshot = AgentSnapshot {
            settings: AgentSettings {
                // one day
                mcp_init_timeout: std::time::Duration::from_secs(86400),
                disable_auto_compact: Default::default(),
                trust_all_tools: false,
                web_tools_enabled: self.web_tools_enabled,
                tool_search_enabled: false,
                ..Default::default()
            },
            permissions: agent::permissions::RuntimePermissions::default().with_cwd(&cwd.to_string_lossy()),
            ..Default::default()
        };

        // Load agent config first so we can extract model_id for RtsModel
        match self.agent_name {
            Some(name) if name == DEFAULT_AGENT_NAME => {
                snapshot.agent_config = build_default_agent(&RealProvider);
            },
            Some(name) => {
                // V1 subagents always inherit default resources (the `chat.disableInheritingDefaultResources`
                // setting is a V2 feature). Pass `true` to preserve existing behavior.
                let (configs, _) = load_agents(&RealProvider, true).await?;
                if let Some(cfg) = configs.into_iter().find(|c| c.name() == name) {
                    snapshot.agent_config = cfg;
                } else {
                    bail!("unable to find agent with name: {}", name);
                }
            },
            None => {
                snapshot.agent_config = build_default_agent(&RealProvider);
            },
        };

        // Build a registry adapter from V1's registry data so the agent can apply
        // it (filter servers/tools, resolve placeholders, inject mcp.json overrides)
        // automatically before launching MCP servers and re-apply on swap. Reuses
        // V2's `RegistryAdapter` since V1 and V2's `McpRegistryResponse` types are
        // structurally identical duplicates.
        let mcp_registry: Option<Box<dyn agent::mcp::McpRegistry>> = match self.registry_data {
            Some(r) => match crate::mcp_registry::to_v2_registry_response(r) {
                Ok(v2_response) => {
                    let adapter = chat_cli_v2::mcp_registry::RegistryAdapter::new(
                        v2_response,
                        Some(self.local_mcp_path),
                        Some(self.global_mcp_path),
                    )
                    .await;
                    Some(Box::new(adapter) as Box<dyn agent::mcp::McpRegistry>)
                },
                Err(e) => {
                    warn!(
                        error = %e,
                        "Failed to convert V1 MCP registry response to V2; subagent will run without registry"
                    );
                    None
                },
            },
            None => None,
        };

        // TODO: V1 uses a separate RtsModel implementation from V2 (chat-cli-v2/src/agent/rts)
        // because they have incompatible ApiClient types. V2's RtsModel has additional features
        // (token usage, context usage %, context_window_size) that V1 lacks, but these are only
        // used for telemetry/compaction which subagents don't need. Consider unifying if ApiClient
        // is ever consolidated.
        let model = {
            use crate::cli::chat::cli::model::{
                find_model,
                get_available_models,
            };

            let state = RtsModelState::new();
            info!(?state.conversation_id, "generated new conversation id");

            let model_id = match get_available_models(os).await {
                Ok((models, default_model)) => match snapshot.agent_config.config().model() {
                    Some(requested) => Some(find_model(&models, requested).map_or_else(
                        || {
                            warn!(
                                model = requested,
                                "agent config model not in ListAvailableModels — passing through to backend"
                            );
                            requested.to_string()
                        },
                        |m| m.model_id.clone(),
                    )),
                    None => Some(default_model.model_id),
                },
                Err(e) => {
                    warn!("failed to fetch available models for subagent: {e}");
                    None
                },
            };

            Arc::new(RtsModel::new(os.client.clone(), state.conversation_id, model_id))
        };

        let mcp_manager_handle = McpManager::default().spawn();
        let mut agent = agent::Agent::new(
            snapshot,
            Some(self.local_mcp_path),
            Some(self.global_mcp_path),
            model,
            mcp_manager_handle,
            true,
            self.code_intelligence.clone(),
            None,
            None,
            Vec::new(),
            mcp_registry,
        )
        .await?;

        agent.prepend_embedded_user_msg(SUBAGENT_EMBEDDED_USER_MSG);
        if let Some(msg) = self.task_context {
            let msg = format!("{CONTEXT_START}{msg}{CONTEXT_END}");
            agent.append_embedded_user_msg(&msg);
        }

        let agent_handle = agent.spawn();

        self.main_loop(
            agent_handle,
            input_rx,
            &mut control_end,
            os,
            parent_conversation_id,
            self.parent_tool_use_id,
        )
        .await
    }

    async fn main_loop<D>(
        &self,
        mut agent: AgentHandle,
        mut input_rx: broadcast::Receiver<InputEvent>,
        control_end: &mut ControlEnd<D>,
        os: &Os,
        parent_conversation_id: &str,
        parent_tool_use_id: &str,
    ) -> Result<Summary> {
        let mut telemetry_sink = TelemetrySink::new(
            parent_conversation_id,
            parent_tool_use_id,
            self.agent_name.unwrap_or("kiro_default"),
            &os.telemetry,
        );

        // First, wait for agent initialization
        loop {
            tokio::select! {
                // While we wait we would still need to handle user input
                input_evt = input_rx.recv() => {
                    let Ok(input_evt) = input_evt else {
                        error!("input channel closed: {input_evt:#?}");
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
                                    McpServerEvent::InitializeError { server_name, error, .. } => {
                                        UiMcpEvent::LoadFailure { server_name, error }
                                    },
                                    McpServerEvent::OauthRequest { server_name, oauth_url } => {
                                        UiMcpEvent::OauthRequest { server_name, oauth_url }
                                    },
                                    McpServerEvent::Initializing { server_name } => UiMcpEvent::Loading { server_name },
                                    McpServerEvent::ToolListChanged { .. } => continue,
                                }
                            };
                            _ = control_end.send(SessionEvent::AgentEvent(chat_cli_ui::protocol::AgentEvent {
                                agent_id: self.id,
                                kind: AgentEventKind::McpEvent(ui_mcp_event)
                            }));
                        },
                        // We need to wait until the agent is initialized before moving on
                        AgentEvent::Initialized => {
                            let meta = MetaEvent {
                                meta_type: "Initialized".to_string(),
                                payload: serde_json::Value::Number(Number::from(self.id)),
                            };
                            _ = control_end.send(SessionEvent::AgentEvent(chat_cli_ui::protocol::AgentEvent {
                                agent_id: self.id,
                                kind: AgentEventKind::MetaEvent(meta)
                            }));
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
        let mut has_sent_failsafe_msg = false;
        telemetry_sink
            .record_user_turn_completion_args
            .replace(Default::default());

        enum QueryStatus {
            Ongoing,
            Resolved(Summary),
            Interrupted,
            Error(Summary),
        }

        let mut query_status = QueryStatus::Ongoing;
        // Accumulates the subagent's streamed assistant text for the current turn,
        // used as a graceful fallback if the turn ends in an empty response. Reset
        // at the start of each turn so a later empty turn doesn't resurrect stale
        // text — the most recent non-empty turn is what we degrade to.
        let mut current_turn_text = String::new();
        let mut last_message: Option<String> = None;

        loop {
            tokio::select! {
                input_evt = input_rx.recv() => {
                    let Ok(input_evt) = input_evt else {
                        break;
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
                            query_status = QueryStatus::Interrupted;
                            break;
                        },
                        InputEventKind::ToolApproval(id) => {
                            agent
                                .send_tool_use_approval_result(SendApprovalResultArgs {
                                    id,
                                    result: ApprovalResult {
                                        option_id: PermissionOptionId::AllowOnce,
                                        reason: None,
                                        trust_option: None,
                                    },
                                })
                                .await?;
                        },
                        InputEventKind::ToolRejection(id) => {
                            agent
                                .send_tool_use_approval_result(SendApprovalResultArgs {
                                    id,
                                    result: ApprovalResult {
                                        option_id: PermissionOptionId::RejectOnce,
                                        reason: Some("User rejected this tool. Find an alternative or report inability to proceed.".to_string()),
                                        trust_option: None,
                                    },
                                })
                                .await?;
                        },
                    }
                },

                evt = agent.recv() => {
                    let evt = match evt {
                        Ok(evt) => evt,
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                            warn!(n, "subagent broadcast receiver lagged, skipping {n} events");
                            continue;
                        },
                        Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                            // Broadcast closed; recover a still-buffered summary before giving up.
                            if !matches!(query_status, QueryStatus::Resolved(_))
                                && let Some(s) = agent.take_summary().await
                            {
                                query_status = QueryStatus::Resolved(s);
                            }
                            break;
                        },
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
                                UpdateEvent::ToolCallFinished { tool_call, result } => {
                                    let tool_name = tool_call.tool.kind.canonical_tool_name().tool_name().to_string();
                                    let mcp_server_name =
                                        tool_call.tool.kind.mcp_server_name().map(str::to_string);
                                    let (outcome, reason_desc) = match result {
                                        ToolCallResult::Success(_) => (metric::ToolMetricOutcome::Success, None),
                                        ToolCallResult::Error(error) => {
                                            (metric::ToolMetricOutcome::Error, Some(error.to_string()))
                                        },
                                        ToolCallResult::Cancelled => (metric::ToolMetricOutcome::Cancelled, None),
                                    };
                                    let tool_use_id = tool_call.id;
                                    let event = subagent_tool_use_event(
                                        parent_conversation_id,
                                        tool_use_id.clone(),
                                        tool_name.clone(),
                                        mcp_server_name,
                                        outcome,
                                        Some(true),
                                        reason_desc,
                                    );
                                    os.telemetry
                                        .send_tool_use_suggested(
                                            &os.database,
                                            event,
                                            metric::ExecutionContext::Subagent,
                                        )
                                        .await
                                        .ok();
                                    _ = control_end.send(SessionEvent::AgentEvent(AgentEventForUi {
                                        agent_id: self.id,
                                        kind: AgentEventKind::ToolCallEnd(
                                            ToolCallEnd {
                                                tool_name,
                                                tool_call_id: tool_use_id,
                                            }
                                        )
                                    }));
                                },
                                UpdateEvent::ToolCallFailed {
                                    tool_use_id,
                                    tool_name,
                                    tool_identity,
                                    reason,
                                    error,
                                    ..
                                } => {
                                    let (tool_name, mcp_server_name) = tool_identity
                                        .map_or((tool_name, None), |identity| {
                                            (identity.tool_name, identity.mcp_server_name)
                                        });
                                    let (outcome, is_valid) = match reason {
                                        ToolCallFailureReason::ParseError => {
                                            (metric::ToolMetricOutcome::Error, Some(false))
                                        },
                                        ToolCallFailureReason::PermissionDenied
                                        | ToolCallFailureReason::HookRejected => {
                                            (metric::ToolMetricOutcome::Denied, Some(true))
                                        },
                                    };
                                    let event = subagent_tool_use_event(
                                        parent_conversation_id,
                                        tool_use_id,
                                        tool_name,
                                        mcp_server_name,
                                        outcome,
                                        is_valid,
                                        Some(error),
                                    );
                                    os.telemetry
                                        .send_tool_use_suggested(
                                            &os.database,
                                            event,
                                            metric::ExecutionContext::Subagent,
                                        )
                                        .await
                                        .ok();
                                },
                                UpdateEvent::AgentContent(content) => {
                                    if let ContentChunk::Text(text) = content {
                                        // Track streamed assistant text so we can degrade
                                        // gracefully if the turn ends in an empty response.
                                        current_turn_text.push_str(&text);
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
                            // Snapshot this turn's text for the empty-response fallback.
                            let turn_text = std::mem::take(&mut current_turn_text);
                            trace!(turn_text_len = turn_text.len(), has_sent_failsafe_msg, "subagent EndTurn received");
                            if !turn_text.trim().is_empty() {
                                last_message = Some(turn_text);
                            }
                            // Recover the summary from the lossless channel in case the
                            // broadcast dropped the SubagentSummary event under load.
                            if !matches!(query_status, QueryStatus::Resolved(_))
                                && let Some(s) = agent.take_summary().await
                            {
                                trace!("recovered summary from lossless channel on EndTurn");
                                query_status = QueryStatus::Resolved(s);
                            }
                            if matches!(query_status, QueryStatus::Resolved(_)) {
                                user_turn_metadata.push(metadata.clone());
                                break;
                            } else if !has_sent_failsafe_msg {
                                trace!("no summary on EndTurn — sending failsafe message");
                                agent
                                    .send_prompt(SendPromptArgs {
                                        content: vec![ContentChunk::Text(SUMMARY_FAILSAFE_MSG.to_string())],
                                        should_continue_turn: None,
                                    })
                                    .await?;
                                has_sent_failsafe_msg = true;
                            } else {
                                bail!("Subagent has refused to give result. Try again");
                            }
                        },
                        AgentEvent::Stop(AgentStopReason::Error(agent_error)) => {
                            trace!(?agent_error, ?has_sent_failsafe_msg, last_message_len = last_message.as_ref().map(|m| m.len()), "subagent Stop(Error) received");
                            telemetry_sink.update_stop_reason(agent_error.to_string());
                            // Honor a summary delivered just before the error landed.
                            if !matches!(query_status, QueryStatus::Resolved(_))
                                && let Some(s) = agent.take_summary().await
                            {
                                trace!("recovered summary from lossless channel after Stop(Error)");
                                query_status = QueryStatus::Resolved(s);
                                break;
                            }
                            // Empty response: send the failsafe prompt instead of immediately
                            // degrading — gives the model one more chance to call summary.
                            if stop_error_is_degradable(&agent_error) && !has_sent_failsafe_msg {
                                trace!("EmptyResponse on first attempt — sending failsafe instead of degrading");
                                agent
                                    .send_prompt(SendPromptArgs {
                                        content: vec![ContentChunk::Text(SUMMARY_FAILSAFE_MSG.to_string())],
                                        should_continue_turn: None,
                                    })
                                    .await?;
                                has_sent_failsafe_msg = true;
                                continue;
                            }
                            // Failsafe already sent or non-degradable error — degrade/fail.
                            let summary = disposition_for_stop_error(
                                self.query,
                                &agent_error,
                                last_message.as_deref(),
                            );
                            query_status = if stop_error_is_degradable(&agent_error) {
                                trace!("EmptyResponse after failsafe — degrading to fallback");
                                QueryStatus::Resolved(summary)
                            } else {
                                QueryStatus::Error(summary)
                            };
                            break;
                        },
                        AgentEvent::ApprovalRequest(ApprovalRequest { id, tool_use, .. }) => {
                            match (self.is_interactive, self.dangerously_trust_all_tools) {
                                (_, true) => {
                                    warn!(?tool_use, "trust all is enabled, ignoring approval request");
                                    agent
                                        .send_tool_use_approval_result(SendApprovalResultArgs {
                                            id: id.clone(),
                                            result: ApprovalResult {
                                                option_id: PermissionOptionId::AllowOnce,
                                                reason: None,
                                                trust_option: None,
                                            },
                                        })
                                        .await?;
                                }
                                (true, false) => {
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
                                },
                                (false, false) => {
                                    error!("subagent cannot run in non-interactive mode with tool permission request");
                                    query_status = QueryStatus::Error(Summary {
                                        task_description: self.query.to_string(),
                                        context_summary: None,
                                        task_result: "Subagent cannot run in non-interactive mode with tool permission request".to_string(),
                                        result_type: None,
                                    });
                                    break;
                                },
                            }
                        },
                        AgentEvent::SubagentSummary(summary) => {
                            query_status = QueryStatus::Resolved(summary);
                        },
                        _ => {},
                    }
                },
            }
        }

        let mut summary = SubagentExecutionSummary::default();

        // Generally there should only be one turn in the user turn metadata array.
        // In scenarios where the model needed to be reminded to summarize the findings is when
        // there would be more than one entry in this array, in which case we shall aggregate it.
        for md in &user_turn_metadata {
            let tool_call_count = &mut summary.tool_call_count;
            let total_tool_uses = md.number_of_cycles;

            telemetry_sink.mcp_tool_uses = telemetry_sink
                .mcp_tool_uses
                .saturating_add(total_tool_uses)
                .saturating_sub(md.builtin_tool_uses);
            telemetry_sink.builtin_tool_uses = telemetry_sink.builtin_tool_uses.saturating_add(md.builtin_tool_uses);
            *tool_call_count = tool_call_count.saturating_add(md.number_of_cycles);

            summary.token_count = summary
                .token_count
                .saturating_add(md.input_token_count.into())
                .saturating_add(md.output_token_count.into());

            if let Some(turn_duration) = md.turn_duration.as_ref() {
                let duration = summary.duration.get_or_insert(std::time::Duration::from_secs(0));
                *duration = duration.saturating_add(*turn_duration);
            }

            let args = telemetry_sink.record_user_turn_completion_args.get_or_insert_default();
            accumulate_subagent_turn(args, md, telemetry_sink.parent_tool_use_id);
        }
        telemetry_sink.telemetry_result = telemetry_result(&user_turn_metadata, telemetry_sink.telemetry_result.take());

        // TODO: do we want to set a special variant for this so we don't have to marshall and
        // unmarshall?
        if let Ok(payload) = serde_json::to_value(summary) {
            _ = control_end.send(SessionEvent::AgentEvent(AgentEventForUi {
                agent_id: self.id,
                kind: AgentEventKind::MetaEvent(MetaEvent {
                    meta_type: "EndTurn".to_string(),
                    payload,
                }),
            }));
        }

        match query_status {
            QueryStatus::Ongoing => bail!("subagent has exited unexpectedly"),
            QueryStatus::Interrupted => bail!("User has interrupted the operation"),
            QueryStatus::Resolved(summary) | QueryStatus::Error(summary) => Ok(summary),
        }
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
async fn test_sub_agent_routine(queries: Vec<(String, String)>) -> Result<Vec<Summary>> {
    let os = Os::new().await.expect("failed to spawn os");
    let resolver = os.path_resolver();
    let local_mcp_path = resolver.workspace().mcp_config().expect("failed to retrieve path");
    let global_mcp_path = resolver.global().mcp_config().expect("failed to retrieve path");
    let is_interactive = true;
    let subagents = queries
        .iter()
        .enumerate()
        .map(|(id, (agent_name, query))| Subagent {
            id: id as u16,
            query: query.as_str(),
            agent_name: Some(agent_name.as_str()),
            task_context: None,
            dangerously_trust_all_tools: false,
            is_interactive,
            local_mcp_path: &local_mcp_path,
            global_mcp_path: &global_mcp_path,
            parent_tool_use_id: "",
            code_intelligence: None,
            registry_data: None,
            web_tools_enabled: true,
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
        is_interactive,
    );

    let mut indicator_handle = subagent_indicator.run();

    let res = futures::future::try_join_all(
        subagents
            .into_iter()
            .map(|subagent| subagent.query(&os, input_rx.resubscribe(), control_end.clone(), stub_id)),
    )
    .await;

    _ = indicator_handle.wait_for_clean_screen().await;

    res
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use agent::agent_config::definitions::ToolsSettings;
    use agent::agent_loop::AgentLoopId;
    use agent::permissions::{
        RuntimePermissions,
        evaluate_tool_permission,
    };
    use agent::protocol::PermissionEvalResult;
    use agent::tools::fs_read::file::FileOp;
    use agent::tools::fs_read::{
        FsRead,
        FsReadOperation,
    };
    use agent::tools::{
        BuiltInTool,
        ToolKind,
    };
    use agent::types::AgentId;
    use agent::util::providers::CwdProvider;
    use chrono::Utc;

    use super::*;

    /// Verifies that the subagent snapshot includes CWD in allowed read paths,
    /// so fs_read within the working directory doesn't require approval.
    #[test]
    fn test_subagent_snapshot_allows_fs_read_in_cwd() {
        let cwd = RealProvider.cwd().unwrap();
        let cwd_str = cwd.to_string_lossy();
        let permissions = RuntimePermissions::default().with_cwd(&cwd_str);

        // Simulate fs_read for a file in CWD
        let tool = ToolKind::BuiltIn(BuiltInTool::FileRead(FsRead {
            operations: vec![FsReadOperation::Line(FileOp {
                path: format!("{}/some_file.rs", cwd_str),
                limit: None,
                offset: None,
            })],
        }));

        let result = evaluate_tool_permission(
            &permissions,
            &HashSet::new(),
            &ToolsSettings::default(),
            &tool,
            &RealProvider,
        )
        .unwrap();

        assert!(
            matches!(result, PermissionEvalResult::Allow),
            "fs_read in CWD should be auto-allowed for subagents, got: {result:?}"
        );
    }

    #[test]
    fn subagent_turn_metadata_populates_token_metrics() {
        let metadata = UserTurnMetadata {
            loop_id: AgentLoopId::new(AgentId::new("subagent".to_string())),
            result: None,
            message_ids: vec![Some("user".to_string()), Some("assistant".to_string())],
            total_request_count: 2,
            number_of_cycles: 1,
            builtin_tool_uses: 1,
            turn_duration: Some(std::time::Duration::from_secs(3)),
            end_reason: LoopEndReason::UserTurnEnd,
            end_timestamp: Utc::now(),
            input_token_count: 100,
            output_token_count: 50,
            cache_read_input_token_count: 25,
            cache_write_input_token_count: 10,
            model: Some("claude-sonnet-4".to_string()),
            assistant_response_length: 200,
            request_attempts: Some(3),
            context_usage_percentage: None,
            metering_usage: Vec::new(),
            user_prompt_length: 80,
        };
        let mut args = RecordUserTurnCompletionArgs::default();

        assert_eq!(
            telemetry_result(std::slice::from_ref(&metadata), Some(TelemetryResult::Failed)),
            Some(TelemetryResult::Failed)
        );
        accumulate_subagent_turn(&mut args, &metadata, "parent-tool-use");
        assert_eq!(args.emit_turn_numeric_metrics, Some(true));

        let mut event = crate::telemetry::core::Event::new(crate::telemetry::EventType::RecordUserTurnCompletion {
            conversation_id: "conversation".to_string(),
            result: TelemetryResult::Succeeded,
            args,
        });
        event.set_engine(metric::Engine::V1);
        event.set_client_application_kind(metric::ClientApplication::ChatCli);
        event.app_type = Some("V1".to_string());
        event.is_subagent = true;
        event.set_session_interface(metric::SessionInterface::InteractiveCli);
        event.metric_context.agent_mode = Some(metric::AgentMode::Default);
        let records = kiro_telemetry_legacy::event_to_otel_metric_records(&event);
        assert_eq!(
            records
                .iter()
                .filter(|record| record.name == "kiro_cli_tokens_consumed")
                .count(),
            3
        );
        assert!(records.iter().all(|record| record.name != "kiro_cli_user_turns"));
    }

    #[test]
    fn empty_subagent_metadata_preserves_a_stop_error() {
        assert_eq!(
            telemetry_result(&[], Some(TelemetryResult::Failed)),
            Some(TelemetryResult::Failed)
        );
    }

    #[test]
    fn subagent_tool_events_preserve_typed_outcomes() {
        let success = subagent_tool_use_event(
            "conversation",
            "tool-use".to_string(),
            "read".to_string(),
            None,
            metric::ToolMetricOutcome::Success,
            Some(true),
            None,
        );
        assert!(success.is_accepted);
        assert_eq!(success.is_success, Some(true));
        assert!(!success.is_custom_tool);

        let denied = subagent_tool_use_event(
            "conversation",
            "tool-use".to_string(),
            "search".to_string(),
            Some("server".to_string()),
            metric::ToolMetricOutcome::Denied,
            Some(true),
            Some("permission denied".to_string()),
        );
        assert!(!denied.is_accepted);
        assert_eq!(denied.is_success, None);
        assert!(denied.is_custom_tool);
        assert_eq!(denied.mcp_server_name.as_deref(), Some("server"));
    }

    /// Verifies that fs_read outside CWD still requires approval.
    #[test]
    fn test_subagent_snapshot_asks_for_fs_read_outside_cwd() {
        let cwd = RealProvider.cwd().unwrap();
        let cwd_str = cwd.to_string_lossy();
        let permissions = RuntimePermissions::default().with_cwd(&cwd_str);

        let tool = ToolKind::BuiltIn(BuiltInTool::FileRead(FsRead {
            operations: vec![FsReadOperation::Line(FileOp {
                path: "/tmp/outside_cwd/secret.txt".to_string(),
                limit: None,
                offset: None,
            })],
        }));

        let result = evaluate_tool_permission(
            &permissions,
            &HashSet::new(),
            &ToolsSettings::default(),
            &tool,
            &RealProvider,
        )
        .unwrap();

        assert!(
            matches!(result, PermissionEvalResult::Ask { .. }),
            "fs_read outside CWD should require approval, got: {result:?}"
        );
    }

    /// An empty-response error is degradable: it should be reported as a
    /// successful (degraded) result, not a failure.
    #[test]
    fn empty_response_error_is_degradable() {
        let err = AgentError::AgentLoopError(LoopError::EmptyResponse);
        assert!(
            stop_error_is_degradable(&err),
            "EmptyResponse should degrade to a result instead of failing the stage"
        );
    }

    /// Any non-empty-response error is a real failure and must not be degraded.
    #[test]
    fn other_errors_are_not_degradable() {
        let err = AgentError::AgentLoopError(LoopError::InvalidJson {
            assistant_text: "oops".to_string(),
            invalid_tools: Vec::new(),
            valid_tools: Vec::new(),
        });
        assert!(
            !stop_error_is_degradable(&err),
            "a genuine error must still fail the subagent stage"
        );
    }

    /// On an empty response after the subagent produced text, the degraded
    /// summary surfaces that last message wrapped in the fallback template.
    #[test]
    fn empty_response_degrades_to_last_message() {
        let err = AgentError::AgentLoopError(LoopError::EmptyResponse);
        let summary = disposition_for_stop_error("count LOC", &err, Some("partial findings"));

        assert_eq!(summary.task_description, "count LOC");
        assert!(
            summary.task_result.contains("partial findings"),
            "degraded result should include the last message, got: {}",
            summary.task_result
        );
        assert!(
            summary.task_result.contains("empty response"),
            "degraded result should explain the empty response, got: {}",
            summary.task_result
        );
    }

    /// On an empty response with no prior message, the degraded summary is still
    /// well-formed (template with an empty body) rather than a hard error.
    #[test]
    fn empty_response_with_no_prior_message_degrades_to_template() {
        let err = AgentError::AgentLoopError(LoopError::EmptyResponse);
        let summary = disposition_for_stop_error("count LOC", &err, None);

        assert_eq!(summary.task_description, "count LOC");
        assert!(
            summary.task_result.contains("empty response"),
            "should still produce the explanatory template, got: {}",
            summary.task_result
        );
        assert!(
            !summary.task_result.contains("failed due to the following error"),
            "empty response must not be reported as a failure"
        );
    }

    /// A genuine (non-empty) error produces a failure-shaped summary describing
    /// the error, matching the prior behavior.
    #[test]
    fn genuine_error_reports_failure() {
        let err = AgentError::AgentLoopError(LoopError::InvalidJson {
            assistant_text: "bad".to_string(),
            invalid_tools: Vec::new(),
            valid_tools: Vec::new(),
        });
        let summary = disposition_for_stop_error("count LOC", &err, Some("ignored on failure"));

        assert!(
            summary.task_result.contains("failed due to the following error"),
            "non-empty errors should report a failure, got: {}",
            summary.task_result
        );
    }
}
