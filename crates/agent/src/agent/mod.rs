pub mod agent_config;
pub mod agent_loop;
pub mod consts;
pub mod mcp;
mod permissions;
pub mod protocol;
pub mod rts;
pub mod task_executor;
pub mod tools;
pub mod types;
pub mod util;

use std::collections::{
    HashMap,
    HashSet,
    VecDeque,
};
use std::os::unix::fs::MetadataExt as _;
use std::path::Path;

use agent_config::definitions::{
    Config,
    HookConfig,
    HookTrigger,
};
use agent_config::load_mcp_configs;
use agent_config::parse::{
    CanonicalToolName,
    Resource,
    ResourceKind,
    ToolNameKind,
    ToolParseError,
    ToolParseErrorKind,
};
use agent_loop::model::{
    Models,
    ModelsState,
    TestModel,
};
use agent_loop::protocol::{
    AgentLoopEvent,
    AgentLoopEventKind,
    AgentLoopResponse,
    LoopError,
    SendRequestArgs,
};
use agent_loop::types::{
    ContentBlock,
    Message,
    Role,
    StreamErrorKind,
    ToolResultBlock,
    ToolResultContentBlock,
    ToolResultStatus,
    ToolSpec,
    ToolUseBlock,
};
use agent_loop::{
    AgentLoop,
    AgentLoopHandle,
    AgentLoopId,
    LoopState,
};
use bstr::ByteSlice as _;
use chrono::Utc;
use mcp::McpManager;
use permissions::evaluate_tool_permission;
use protocol::{
    AgentError,
    AgentEvent,
    AgentRequest,
    AgentResponse,
    ApprovalResult,
    InputItem,
    PermissionEvalResult,
    SendApprovalResultArgs,
    SendPromptArgs,
};
use rts::RtsModel;
use serde::{
    Deserialize,
    Serialize,
};
use task_executor::{
    Hook,
    HookExecutionId,
    HookExecutorResult,
    HookResult,
    StartHookExecution,
    StartToolExecution,
    TaskExecutor,
    TaskExecutorEvent,
    ToolExecutionEndEvent,
    ToolExecutionId,
    ToolExecutorResult,
    ToolFuture,
};
use tokio::io::{
    AsyncReadExt as _,
    BufReader,
};
use tokio::sync::{
    broadcast,
    oneshot,
};
use tokio_util::sync::CancellationToken;
use tools::ToolExecutionOutputItem;
use tracing::{
    debug,
    error,
    trace,
    warn,
};
use types::{
    AgentId,
    AgentSettings,
    AgentSnapshot,
    ConversationMetadata,
    ConversationState,
};
use util::path::canonicalize_path;
use util::request_channel::new_request_channel;
use util::truncate_safe_in_place;
use uuid::Uuid;

use crate::agent::consts::{
    DUMMY_TOOL_NAME,
    MAX_CONVERSATION_STATE_HISTORY_LEN,
};
use crate::agent::mcp::McpManagerHandle;
use crate::agent::tools::{
    BuiltInTool,
    ToolKind,
    ToolState,
    built_in_tool_names,
};
use crate::agent::util::error::{
    ErrorContext as _,
    UtilError,
};
use crate::agent::util::glob::{
    find_matches,
    matches_any_pattern,
};
use crate::agent::util::request_channel::{
    RequestReceiver,
    RequestSender,
    respond,
};
use crate::api_client::ApiClient;

pub const CONTEXT_ENTRY_START_HEADER: &str = "--- CONTEXT ENTRY BEGIN ---\n";
pub const CONTEXT_ENTRY_END_HEADER: &str = "--- CONTEXT ENTRY END ---\n\n";

#[derive(Debug)]
pub struct AgentHandle {
    sender: RequestSender<AgentRequest, AgentResponse, AgentError>,
    event_rx: broadcast::Receiver<AgentEvent>,
}

impl Clone for AgentHandle {
    fn clone(&self) -> Self {
        Self {
            sender: self.sender.clone(),
            event_rx: self.event_rx.resubscribe(),
        }
    }
}

impl AgentHandle {
    pub async fn recv(&mut self) -> Result<AgentEvent, broadcast::error::RecvError> {
        self.event_rx.recv().await
    }

    pub async fn send_prompt(&self, args: SendPromptArgs) -> Result<(), AgentError> {
        match self
            .sender
            .send_recv(AgentRequest::SendPrompt(args))
            .await
            .unwrap_or(Err(AgentError::Channel))?
        {
            AgentResponse::Success => Ok(()),
            other => Err(AgentError::Custom(format!("received unexpected response: {:?}", other))),
        }
    }

    pub async fn send_tool_use_approval_result(&self, args: SendApprovalResultArgs) -> Result<(), AgentError> {
        match self
            .sender
            .send_recv(AgentRequest::SendApprovalResult(args))
            .await
            .unwrap_or(Err(AgentError::Channel))?
        {
            AgentResponse::Success => Ok(()),
            other => Err(AgentError::Custom(format!("received unexpected response: {:?}", other))),
        }
    }
}

#[derive(Debug)]
pub struct Agent {
    id: AgentId,
    agent_config: Config,

    conversation_state: ConversationState,
    conversation_metadata: ConversationMetadata,
    execution_state: ExecutionState,
    tool_state: ToolState,

    agent_event_tx: broadcast::Sender<AgentEvent>,
    agent_event_rx: Option<broadcast::Receiver<AgentEvent>>,

    /// Contains an [AgentLoop] if the agent is in the middle of executing a user turn, otherwise
    /// is [None].
    agent_loop: Option<AgentLoopHandle>,

    /// Used for executing tools and hooks in the background
    task_executor: TaskExecutor,
    mcp_manager_handle: McpManagerHandle,

    /// Cached result of agent spawn hooks.
    ///
    /// Since these hooks are only executed when the agent is initialized, they are just cached
    /// here. It's important that these results do not change since they are added as part of
    /// context messages (which is very prone to breaking prompt caching!)
    ///
    /// A [Vec] is used instead of a [HashMap] to maintain iteration order.
    agent_spawn_hooks: Vec<(HookConfig, String)>,

    /// The backend/model provider
    model: Models,

    settings: AgentSettings,
}

impl Agent {
    pub async fn new_default() -> eyre::Result<Agent> {
        let mcp_manager_handle = McpManager::new().spawn();
        Self::init(AgentSnapshot::new_built_in_agent(), mcp_manager_handle).await
    }

    pub async fn from_config(config: Config) -> eyre::Result<Agent> {
        let mcp_manager_handle = McpManager::new().spawn();
        let snapshot = AgentSnapshot::new_empty(config);
        Self::init(snapshot, mcp_manager_handle).await
    }

    pub async fn init(snapshot: AgentSnapshot, mcp_manager_handle: McpManagerHandle) -> eyre::Result<Agent> {
        debug!(?snapshot, "initializing agent from snapshot");

        let (agent_event_tx, agent_event_rx) = broadcast::channel(64);

        let agent_config = snapshot.agent_config;
        let task_executor = TaskExecutor::new();

        let model = match snapshot.model_state {
            ModelsState::Rts {
                conversation_id,
                model_id,
            } => Models::Rts(RtsModel::new(
                ApiClient::new().await?,
                conversation_id.clone().unwrap_or(Uuid::new_v4().to_string()),
                model_id.clone(),
            )),
            ModelsState::Test => Models::Test(TestModel::new()),
        };

        Ok(Self {
            id: snapshot.id,
            agent_config,
            conversation_state: snapshot.conversation_state,
            conversation_metadata: snapshot.conversation_metadata,
            execution_state: snapshot.execution_state,
            tool_state: snapshot.tool_state,
            agent_event_tx,
            agent_event_rx: Some(agent_event_rx),
            agent_loop: None,
            task_executor,
            mcp_manager_handle,
            agent_spawn_hooks: Default::default(),
            model,
            settings: snapshot.settings,
        })
    }

    pub fn spawn(mut self) -> AgentHandle {
        let (tx, rx) = new_request_channel();
        let event_rx = self.agent_event_rx.take().expect("should exist");
        tokio::spawn(async move {
            self.initialize().await;
            self.main_loop(rx).await;
        });
        AgentHandle { sender: tx, event_rx }
    }

    /// TODO - do initialization logic depending on execution state
    async fn initialize(&mut self) {
        // Initialize MCP servers, waiting with timeout.
        match load_mcp_configs(&self.agent_config).await {
            Ok(res) => {
                for config in res.configs {
                    self.mcp_manager_handle.launch_server(config.name, config.config).await;
                }
            },
            Err(err) => {
                error!(?err, "failed to load MCP configs for agent");
            },
        }

        // Next, run agent spawn hooks.
        let hooks = self.get_hooks(HookTrigger::AgentSpawn).await;
        if !hooks.is_empty() {
            let hooks = hooks
                .into_iter()
                .map(|hook| {
                    (
                        HookExecutionId {
                            hook,
                            tool_context: None,
                        },
                        None,
                    )
                })
                .collect();
            if let Err(err) = self.start_hooks_execution(hooks, HookStage::AgentSpawn, None).await {
                error!(?err, "failed to execute agent spawn hooks");
            }
        } else {
            let _ = self.agent_event_tx.send(AgentEvent::Initialized);
        }
    }

    async fn main_loop(mut self, mut request_rx: RequestReceiver<AgentRequest, AgentResponse, AgentError>) {
        let mut task_executor_event_buf = Vec::new();

        loop {
            tokio::select! {
                req = request_rx.recv() => {
                    let Some(req) = req else {
                        warn!("session request receiver channel has closed, exiting");
                        break;
                    };
                    let res = self.handle_agent_request(req.payload).await;
                    respond!(req, res);
                },

                // Branch for handling the next stream event.
                //
                // We do some trickery to return a future that never resolves if we're not currently
                // consuming a response stream.
                res = async {
                    match self.agent_loop.as_mut() {
                        Some(handle) => {
                            handle.recv().await
                        },
                        None => std::future::pending().await,
                    }
                } => {
                    // let (handle, evt) = res;
                    let evt = res;
                    if let Err(e) = self.handle_agent_loop_event(evt).await {
                        error!(?e, "failed to handle agent loop event");
                        self.set_active_state(ActiveState::Errored(e)).await;
                    }
                },

                _ = self.task_executor.recv_next(&mut task_executor_event_buf) => {
                    for evt in task_executor_event_buf.drain(..) {
                        if let Err(e) = self.handle_task_executor_event(evt.clone()).await {
                            error!(?e, "failed to handle tool executor event");
                            self.set_active_state(ActiveState::Errored(e)).await;
                        }
                        let _ = self.agent_event_tx.send(AgentEvent::TaskExecutor(evt));
                    }
                }
            }
        }
    }

    fn active_state(&self) -> &ActiveState {
        &self.execution_state.active_state
    }

    async fn set_active_state(&mut self, new_state: ActiveState) {
        let from = self.execution_state.clone();
        self.execution_state.active_state = new_state;
        let to = self.execution_state.clone();
        let _ = self.agent_event_tx.send(AgentEvent::StateChange { from, to });
    }

    fn create_snapshot(&self) -> AgentSnapshot {
        AgentSnapshot {
            id: self.id.clone(),
            agent_config: self.agent_config.clone(),
            conversation_state: self.conversation_state.clone(),
            conversation_metadata: self.conversation_metadata.clone(),
            compaction_snapshots: vec![],
            execution_state: self.execution_state.clone(),
            model_state: self.model.state(),
            tool_state: self.tool_state.clone(),
            settings: self.settings.clone(),
        }
    }

    async fn get_agent_config(&self) -> &Config {
        &self.agent_config
    }

    async fn get_hooks(&self, trigger: HookTrigger) -> Vec<Hook> {
        let config = self.get_agent_config().await;
        let hooks_config = config.hooks();
        hooks_config
            .get(&trigger)
            .cloned()
            .into_iter()
            .flat_map(|configs| configs.into_iter().map(|config| Hook { trigger, config }))
            .collect::<Vec<_>>()
    }

    fn agent_loop_handle(&mut self) -> Result<&mut AgentLoopHandle, AgentError> {
        self.agent_loop
            .as_mut()
            .ok_or(AgentError::Custom("Agent is not executing a turn".to_string()))
    }

    /// Ends the current user turn by closing [Self::agent_loop] if it exists.
    async fn end_current_turn(&mut self) -> Result<(), AgentError> {
        let Some(mut handle) = self.agent_loop.take() else {
            return Ok(());
        };
        handle.close().await?;
        while let Some(evt) = handle.recv().await {
            if let AgentLoopEventKind::UserTurnEnd(md) = &evt {
                self.conversation_metadata.user_turn_metadatas.push(md.clone());
            }
            let _ = self
                .agent_event_tx
                .send(AgentEvent::agent_loop(handle.id().clone(), evt));
        }
        self.set_active_state(ActiveState::Idle).await;
        Ok(())
    }

    async fn handle_agent_request(&mut self, req: AgentRequest) -> Result<AgentResponse, AgentError> {
        debug!(?req, "handling agent request");

        match req {
            AgentRequest::SendPrompt(args) => self.handle_send_prompt(args).await,
            AgentRequest::Interrupt => self.handle_interrupt().await,
            AgentRequest::SendApprovalResult(args) => self.handle_approval_result(args).await,
            AgentRequest::CreateSnapshot => Ok(AgentResponse::Snapshot(self.create_snapshot())),
        }
    }

    /// Handlers for a [AgentRequest::Interrupt] request.
    async fn handle_interrupt(&mut self) -> Result<AgentResponse, AgentError> {
        match self.active_state() {
            ActiveState::Idle
            | ActiveState::Errored(_)
            | ActiveState::ExecutingRequest
            | ActiveState::WaitingForApproval { .. } => {},
            ActiveState::ExecutingHooks(executing_hooks) => {
                for id in executing_hooks.hooks.keys() {
                    self.task_executor.cancel_hook_execution(id);
                }
            },
            ActiveState::ExecutingTools { tools } => {
                for id in tools.keys() {
                    self.task_executor.cancel_tool_execution(id);
                }
            },
        }
        if let Some(handle) = &self.agent_loop {
            if let LoopState::PendingToolUseResults = handle.get_loop_state().await? {
                // If the agent is in the middle of sending tool uses, then add two new
                // messages:
                // 1. user tool results replaced with content: "Tool use was cancelled by the user"
                // 2. assistant message with content: "Tool uses were interrupted, waiting for the next user prompt"
                let tool_results = self
                    .conversation_state
                    .messages
                    .last()
                    .iter()
                    .flat_map(|m| {
                        m.content.iter().filter_map(|c| match c {
                            ContentBlock::ToolUse(tool_use) => Some(ContentBlock::ToolResult(ToolResultBlock {
                                tool_use_id: tool_use.tool_use_id.clone(),
                                content: vec![ToolResultContentBlock::Text(
                                    "Tool use was cancelled by the user".to_string(),
                                )],
                                status: ToolResultStatus::Error,
                            })),
                            _ => None,
                        })
                    })
                    .collect::<Vec<_>>();
                self.conversation_state
                    .messages
                    .push(Message::new(Role::User, tool_results, Some(Utc::now())));
                self.conversation_state.messages.push(Message::new(
                    Role::Assistant,
                    vec![ContentBlock::Text(
                        "Tool uses were interrupted, waiting for the next user prompt".to_string(),
                    )],
                    Some(Utc::now()),
                ));
            }
        }
        self.end_current_turn().await?;
        if !matches!(self.active_state(), ActiveState::Idle) {
            self.set_active_state(ActiveState::Idle).await;
        }
        Ok(AgentResponse::Success)
    }

    /// Handler for a [AgentRequest::SendApprovalResult] request.
    async fn handle_approval_result(&mut self, args: SendApprovalResultArgs) -> Result<AgentResponse, AgentError> {
        match &mut self.execution_state.active_state {
            ActiveState::WaitingForApproval { needs_approval, .. } => {
                let Some(approval_result) = needs_approval.get_mut(&args.id) else {
                    return Err(AgentError::Custom(format!(
                        "No tool use with the id '{}' requires approval",
                        args.id
                    )));
                };
                *approval_result = Some(args.result);
            },
            other => {
                return Err(AgentError::Custom(format!(
                    "Cannot send approval to agent with state: {:?}",
                    other
                )));
            },
        }

        // Check if we should send the result back to the model.
        // Either:
        // 1. All tools are approved
        // 2. If at least one is denied, immediately return the reason back to the model.
        let ActiveState::WaitingForApproval { needs_approval, tools } = &self.execution_state.active_state else {
            return Err("Agent is not waiting for approval".to_string().into());
        };

        let denied = needs_approval.values().any(|approval_result| {
            approval_result
                .as_ref()
                .is_some_and(|r| matches!(r, ApprovalResult::Deny { .. }))
        });
        if denied {
            let content = needs_approval
                .iter()
                .map(|(tool_use_id, approval_result)| {
                    let reason = match approval_result {
                        Some(ApprovalResult::Approve) => "Tool use was approved, but did not execute".to_string(),
                        Some(ApprovalResult::Deny { reason }) => {
                            let mut v = "Tool use was denied by the user.".to_string();
                            if let Some(r) = reason {
                                v.push_str(format!(" Reason: {}", r).as_str());
                            }
                            v
                        },
                        None => "Tool use was not executed".to_string(),
                    };
                    ContentBlock::ToolResult(ToolResultBlock {
                        tool_use_id: tool_use_id.clone(),
                        content: vec![ToolResultContentBlock::Text(reason)],
                        status: ToolResultStatus::Error,
                    })
                })
                .collect::<Vec<_>>();
            self.conversation_state
                .messages
                .push(Message::new(Role::User, content, Some(Utc::now())));
            self.send_request().await?;
            self.set_active_state(ActiveState::ExecutingRequest).await;
            return Ok(AgentResponse::Success);
        }

        let all_approved = needs_approval
            .values()
            .all(|approval_result| approval_result.as_ref().is_some_and(|r| r == &ApprovalResult::Approve));
        if all_approved {
            self.execute_tools(tools.clone()).await?;
        }

        Ok(AgentResponse::Success)
    }

    async fn handle_agent_loop_event(&mut self, evt: Option<AgentLoopEventKind>) -> Result<(), AgentError> {
        // debug!(?handle, ?evt, "handling new agent loop event");
        debug!(?evt, "handling new agent loop event");
        let loop_id = self.agent_loop_handle()?.id().clone();

        // If the event is None, then the channel has dropped, meaning the agent loop has exited.
        // Thus, return early.
        let Some(evt) = evt else {
            self.agent_loop = None;
            return Ok(());
        };

        // // Otherwise, the loop is still executing a turn - add back.
        // let loop_id = handle.id().clone();
        // self.agent_loop = Some(handle);

        match &evt {
            AgentLoopEventKind::ResponseStreamEnd { result, metadata } => match result {
                Ok(msg) => {
                    self.conversation_state.messages.push(msg.clone());
                    if !metadata.tool_uses.is_empty() {
                        self.handle_tool_uses(metadata.tool_uses.clone()).await?;
                    }
                },
                Err(err) => {
                    error!(?err, ?loop_id, "response stream encountered an error");
                    self.handle_loop_error_on_stream_end(err).await?;
                },
            },
            AgentLoopEventKind::UserTurnEnd(user_turn_metadata) => {
                self.conversation_metadata
                    .user_turn_metadatas
                    .push(user_turn_metadata.clone());
                self.set_active_state(ActiveState::Idle).await;
            },
            _ => (),
        }

        let _ = self
            .agent_event_tx
            .send(AgentEvent::AgentLoop(AgentLoopEvent { id: loop_id, kind: evt }));

        Ok(())
    }

    /// Handler for errors encountered while sending the request or while consuming the response.
    async fn handle_loop_error_on_stream_end(&mut self, err: &LoopError) -> Result<(), AgentError> {
        debug_assert!(matches!(self.active_state(), ActiveState::ExecutingRequest));
        debug_assert!(self.agent_loop.is_some());

        match err {
            LoopError::InvalidJson {
                assistant_text,
                invalid_tools,
            } => {
                // Historically, we've found the model to produce invalid JSON when
                // handling a complicated tool use - often times, the stream just ends
                // as if everything is ok while in the middle of returning the tool use
                // content.
                //
                // In this case, retry the request, except tell the model to split up
                // the work into simpler tool uses.

                // Create a fake assistant message
                let mut assistant_content = vec![ContentBlock::Text(assistant_text.clone())];
                let val = serde_json::Value::Object(
                    [(
                        "key".to_string(),
                        serde_json::Value::String(
                            "SYSTEM NOTE: the actual tool use arguments were too complicated to be generated"
                                .to_string(),
                        ),
                    )]
                    .into_iter()
                    .collect(),
                );
                assistant_content.append(
                    &mut invalid_tools
                        .iter()
                        .map(|v| {
                            ContentBlock::ToolUse(ToolUseBlock {
                                tool_use_id: v.tool_use_id.clone(),
                                name: v.name.clone(),
                                input: val.clone(),
                            })
                        })
                        .collect(),
                );
                self.conversation_state.messages.push(Message {
                    id: None,
                    role: Role::Assistant,
                    content: assistant_content,
                    timestamp: Some(Utc::now()),
                });

                self.conversation_state.messages.push(Message {
                        id: None,
                        role: Role::User,
                        content: vec![ContentBlock::Text(
                            "The generated tool was too large, try again but this time split up the work between multiple tool uses"
                                .to_string(),
                        )],
                        timestamp: Some(Utc::now()),
                    });

                self.send_request().await?;
            },
            LoopError::Stream(stream_err) => match &stream_err.kind {
                StreamErrorKind::StreamTimeout { .. } => {
                    self.conversation_state.messages.push(Message {
                        id: None,
                        role: Role::Assistant,
                        content: vec![ContentBlock::Text(
                            "Response timed out - message took too long to generate".to_string(),
                        )],
                        timestamp: Some(Utc::now()),
                    });
                    self.conversation_state.messages.push(Message {
                        id: None,
                        role: Role::User,
                        content: vec![ContentBlock::Text(
                            "You took too long to respond - try to split up the work into smaller steps.".to_string(),
                        )],
                        timestamp: Some(Utc::now()),
                    });
                    self.send_request().await?;
                },
                StreamErrorKind::Interrupted => {
                    // nothing to do
                },
                StreamErrorKind::Validation { .. }
                | StreamErrorKind::ServiceFailure
                | StreamErrorKind::Throttling
                | StreamErrorKind::ContextWindowOverflow
                | StreamErrorKind::Other(_) => {
                    self.set_active_state(ActiveState::Errored(err.clone().into())).await;
                    let _ = self.agent_event_tx.send(AgentEvent::RequestError(err.clone()));
                },
            },
        }

        Ok(())
    }

    /// Handler for a [AgentRequest::SendPrompt] request.
    async fn handle_send_prompt(&mut self, args: SendPromptArgs) -> Result<AgentResponse, AgentError> {
        match self.active_state() {
            ActiveState::Idle | ActiveState::Errored(_) => (),
            ActiveState::WaitingForApproval { .. } => (),
            ActiveState::ExecutingRequest | ActiveState::ExecutingHooks(_) | ActiveState::ExecutingTools { .. } => {
                return Err(AgentError::NotIdle);
            },
        }

        // Run per-prompt hooks, if required.
        let hooks = self.get_hooks(HookTrigger::UserPromptSubmit).await;
        if !hooks.is_empty() {
            let hooks = hooks
                .into_iter()
                .map(|hook| {
                    (
                        HookExecutionId {
                            hook,
                            tool_context: None,
                        },
                        None,
                    )
                })
                .collect();
            let prompt = args.text();
            self.start_hooks_execution(hooks, HookStage::PrePrompt { args }, prompt)
                .await?;
            Ok(AgentResponse::Success)
        } else {
            self.send_prompt_impl(args, vec![]).await
        }
    }

    async fn send_prompt_impl(
        &mut self,
        args: SendPromptArgs,
        prompt_hooks: Vec<String>,
    ) -> Result<AgentResponse, AgentError> {
        self.end_current_turn().await?;

        let mut user_msg_content = args
            .content
            .into_iter()
            .map(|c| match c {
                InputItem::Text(t) => ContentBlock::Text(t),
                InputItem::Image(img) => ContentBlock::Image(img),
            })
            .collect::<Vec<_>>();

        // Add per-prompt hooks, if required.
        for output in &prompt_hooks {
            user_msg_content.push(ContentBlock::Text(output.clone()));
        }

        self.conversation_state
            .messages
            .push(Message::new(Role::User, user_msg_content.clone(), Some(Utc::now())));

        // Create a new agent loop, and send the request.
        let loop_id = AgentLoopId::new(self.id.clone());
        let cancel_token = CancellationToken::new();
        self.agent_loop = Some(AgentLoop::new(loop_id.clone(), cancel_token).spawn());
        self.send_request()
            .await
            .expect("first agent loop request should never fail");
        self.set_active_state(ActiveState::ExecutingRequest).await;
        Ok(AgentResponse::Success)
    }

    /// Creates a [SendRequestArgs] used for sending requests to the backend based on the current
    /// conversation state.
    ///
    /// The returned conversation history will:
    /// 1. Have context messages prepended to the start of the message history
    /// 2. Have conversation history invariants enforced, mutating messages as required
    async fn format_request(&self) -> Result<SendRequestArgs, AgentError> {
        let mut messages = VecDeque::from(self.conversation_state.messages.clone());
        let mut tool_spec = self.make_tool_spec().await?;
        enforce_conversation_invariants(&mut messages, &mut tool_spec);

        let ctx_messages = self.create_context_messages().await;
        for msg in ctx_messages.into_iter().rev() {
            messages.push_front(msg);
        }

        Ok(SendRequestArgs::new(
            messages.into(),
            if tool_spec.is_empty() { None } else { Some(tool_spec) },
            self.agent_config.system_prompt().map(String::from),
        ))
    }

    async fn send_request(&mut self) -> Result<AgentLoopResponse, AgentError> {
        let model = self.model.clone();
        let request_args = self.format_request().await?;
        let res = self
            .agent_loop_handle()?
            .send_request(model, request_args.clone())
            .await?;
        let _ = self.agent_event_tx.send(AgentEvent::RequestSent(request_args));
        Ok(res)
    }

    async fn create_context_messages(&self) -> Vec<Message> {
        let config = self.get_agent_config().await;
        let summary = self.conversation_metadata.summaries.last().map(|s| s.content.as_str());
        let system_prompt = self.get_agent_config().await.system_prompt();
        let resources = collect_resources(config.resources()).await;

        let content = format_user_context_message(
            summary,
            system_prompt,
            resources.iter().map(|r| &r.content),
            self.agent_spawn_hooks.iter().map(|(_, c)| c),
        );
        let user_msg = Message::new(Role::User, vec![ContentBlock::Text(content)], None);
        let assistant_msg = Message::new(
            Role::Assistant,
            vec![ContentBlock::Text(
                "I will fully incorporate this information when generating my responses, and explicitly acknowledge relevant parts of the summary when answering questions.".to_string(),
            )],
            None,
        );

        vec![user_msg, assistant_msg]
    }

    /// Entrypoint for handling tool uses returned by the model.
    async fn handle_tool_uses(&mut self, tool_uses: Vec<ToolUseBlock>) -> Result<(), AgentError> {
        debug_assert!(matches!(self.active_state(), ActiveState::ExecutingRequest));

        // First, parse tool uses.
        let (tools, errors) = self.parse_tools(tool_uses).await;
        if !errors.is_empty() {
            let content = errors
                .into_iter()
                .map(|e| {
                    let err_msg = e.to_string();
                    ContentBlock::ToolResult(ToolResultBlock {
                        tool_use_id: e.tool_use.tool_use_id,
                        content: vec![ToolResultContentBlock::Text(err_msg)],
                        status: ToolResultStatus::Error,
                    })
                })
                .collect();
            self.conversation_state
                .messages
                .push(Message::new(Role::User, content, Some(Utc::now())));
            self.send_request().await?;
            return Ok(());
        }

        // Next, evaluate permissions.
        let mut needs_approval = Vec::new();
        let mut denied = Vec::new();
        for (block, tool) in &tools {
            let result = self.evaluate_tool_permission(tool).await?;
            match &result {
                PermissionEvalResult::Allow => (),
                PermissionEvalResult::Ask => needs_approval.push(block.tool_use_id.clone()),
                PermissionEvalResult::Deny { reason } => denied.push((block, tool, reason.clone())),
            }
            let _ = self.agent_event_tx.send(AgentEvent::ToolPermissionEvalResult {
                tool: tool.clone(),
                result,
            });
        }

        // Return denied tools immediately back to the model
        if !denied.is_empty() {
            let content = denied
                .into_iter()
                .map(|(block, _, _)| {
                    ContentBlock::ToolResult(ToolResultBlock {
                        tool_use_id: block.tool_use_id.clone(),
                        content: vec![ToolResultContentBlock::Text(
                            "Tool use was rejected because the arguments supplied are forbidden:".to_string(),
                        )],
                        status: ToolResultStatus::Error,
                    })
                })
                .collect();
            self.conversation_state
                .messages
                .push(Message::new(Role::User, content, Some(Utc::now())));
            self.send_request().await?;
            return Ok(());
        }

        // Process PreToolUse hooks, if any.
        let hooks = self.get_hooks(HookTrigger::PreToolUse).await;
        let mut hooks_to_execute = Vec::new();
        for (block, tool) in &tools {
            hooks_to_execute.extend(hooks.iter().filter(|h| hook_matches_tool(&h.config, tool)).map(|h| {
                (
                    HookExecutionId {
                        hook: h.clone(),
                        tool_context: Some((block, tool).into()),
                    },
                    Some((block.clone(), tool.clone())),
                )
            }));
        }
        if !hooks_to_execute.is_empty() {
            debug!(?hooks_to_execute, "found hooks to execute for preToolUse");
            let stage = HookStage::PreToolUse {
                tools: tools.clone(),
                needs_approval: needs_approval.clone(),
            };
            self.start_hooks_execution(hooks_to_execute, stage, None).await?;
            return Ok(());
        }

        // request permission for any asked tools
        if !needs_approval.is_empty() {
            self.request_tool_approvals(tools, needs_approval).await?;
            return Ok(());
        }

        // Start executing the tools, and update the agent state accordingly.
        self.execute_tools(tools).await?;

        Ok(())
    }

    async fn start_hooks_execution(
        &mut self,
        hooks: Vec<(HookExecutionId, Option<(ToolUseBlock, ToolKind)>)>,
        stage: HookStage,
        prompt: Option<String>,
    ) -> Result<(), AgentError> {
        let mut hooks_state = HashMap::new();
        for (id, tool_ctx) in hooks {
            let req = StartHookExecution {
                id: id.clone(),
                prompt: prompt.clone(),
            };
            hooks_state.insert(id, (tool_ctx, None));
            self.task_executor.start_hook_execution(req).await;
        }
        self.set_active_state(ActiveState::ExecutingHooks(ExecutingHooks {
            hooks: hooks_state,
            stage,
        }))
        .await;
        Ok(())
    }

    async fn handle_task_executor_event(&mut self, evt: TaskExecutorEvent) -> Result<(), AgentError> {
        debug!(?evt, "handling new task executor event");
        match evt {
            TaskExecutorEvent::ToolExecutionEnd(evt) => self.handle_tool_execution_end(evt).await,
            TaskExecutorEvent::HookExecutionEnd(evt) => match evt.result {
                HookExecutorResult::Completed { id, result, .. } => self.handle_hook_finished_event(id, result).await,
                HookExecutorResult::Cancelled { .. } => Ok(()),
            },
            TaskExecutorEvent::CachedHookRun(evt) => self.handle_hook_finished_event(evt.id, evt.result).await,
            _ => Ok(()),
        }
    }

    async fn handle_tool_execution_end(&mut self, evt: ToolExecutionEndEvent) -> Result<(), AgentError> {
        let ActiveState::ExecutingTools { tools } = &mut self.execution_state.active_state else {
            warn!(
                ?self.execution_state,
                ?evt,
                "received a tool execution event for an agent not processing tools"
            );
            return Ok(());
        };

        debug_assert!(tools.contains_key(&evt.id));
        tools.entry(evt.id).and_modify(|(_, res)| *res = Some(evt.result));

        let all_tools_finished = tools.values().all(|(_, res)| res.is_some());
        if !all_tools_finished {
            return Ok(());
        }

        let tools = tools.clone();
        let tool_results = tools
            .iter()
            .map(|(_, (_, res))| res.as_ref().expect("is some").clone())
            .collect();

        // Process PostToolUse hooks, if any.
        let hooks = self.get_hooks(HookTrigger::PostToolUse).await;
        let mut hooks_to_execute = Vec::new();
        for (_, ((block, tool), result)) in tools.iter() {
            let Some(result) = result else {
                continue;
            };
            let Some(output) = result.tool_execution_output() else {
                continue;
            };
            let Ok(output) = serde_json::to_value(output) else {
                continue;
            };
            hooks_to_execute.extend(hooks.iter().filter(|h| hook_matches_tool(&h.config, tool)).map(|h| {
                (
                    HookExecutionId {
                        hook: h.clone(),
                        tool_context: Some((block, tool, &output).into()),
                    },
                    Some((block.clone(), tool.clone())),
                )
            }));
        }
        if !hooks_to_execute.is_empty() {
            debug!("found hooks to execute for postToolUse");
            let stage = HookStage::PostToolUse { tool_results };
            self.start_hooks_execution(hooks_to_execute, stage, None).await?;
            return Ok(());
        }

        // All tools have finished executing, so send the results back to the model.
        self.send_tool_results(tool_results).await?;
        Ok(())
    }

    async fn handle_hook_finished_event(&mut self, id: HookExecutionId, result: HookResult) -> Result<(), AgentError> {
        let ActiveState::ExecutingHooks(ExecutingHooks { hooks, stage }) = &mut self.execution_state.active_state
        else {
            warn!(
                ?self.execution_state,
                ?id,
                "received a hook execution event while not executing hooks"
            );
            return Ok(());
        };

        debug_assert!(hooks.contains_key(&id));
        hooks
            .entry(id.clone())
            .and_modify(|(_, res)| *res = Some(result.clone()));

        // Cache the hook if it's a successful agent spawn hook.
        if result.is_success()
            && id.hook.trigger == HookTrigger::AgentSpawn
            && !self.agent_spawn_hooks.iter().any(|v| v.0 == id.hook.config)
        {
            if let Some(output) = result.output() {
                self.agent_spawn_hooks
                    .push((id.hook.config.clone(), output.to_string()));
            }
        }

        let all_hooks_finished = hooks.values().all(|(_, res)| res.is_some());
        if !all_hooks_finished {
            return Ok(());
        }

        // Unwrap the Option around the hook result for ease of use.
        let hook_results = hooks
            .iter()
            .map(|(id, (tool_ctx, res))| (id.clone(), (tool_ctx, res.as_ref().expect("is some").clone())))
            .collect::<HashMap<_, _>>();

        // All hooks have finished executing, so proceed to the next stage.
        match stage {
            HookStage::AgentSpawn => {
                self.set_active_state(ActiveState::Idle).await;
                let _ = self.agent_event_tx.send(AgentEvent::Initialized);
                Ok(())
            },
            HookStage::PrePrompt { args } => {
                let args = args.clone(); // borrow checker clone
                // Filter for only valid hooks.
                let prompt_hooks = hook_results
                    .iter()
                    .filter_map(|(id, (_, res))| {
                        if id.hook.trigger == HookTrigger::UserPromptSubmit
                            && res.is_success()
                            && res.output().is_some()
                        {
                            Some(res.output().expect("output is some").to_string())
                        } else {
                            None
                        }
                    })
                    .collect();
                self.send_prompt_impl(args, prompt_hooks).await?;
                Ok(())
            },
            HookStage::PreToolUse { tools, needs_approval } => {
                // If any command hooks exited with status 2, then we'll block.
                // Otherwise, execute the tools.
                let mut denied_tools = Vec::new();
                for (block, _) in &*tools {
                    let hook = hook_results.iter().find(|(_, (t, res))| {
                        res.exit_code() == Some(2) && t.as_ref().is_some_and(|v| v.0.tool_use_id == block.tool_use_id)
                    });
                    if let Some((_, (_, result))) = hook {
                        denied_tools.push((block.tool_use_id.clone(), result.clone()));
                    }
                }

                if !denied_tools.is_empty() {
                    // Send denied tool results back to the model.
                    let content = denied_tools
                        .into_iter()
                        .map(|(tool_use_id, hook_res)| {
                            ContentBlock::ToolResult(ToolResultBlock {
                                tool_use_id,
                                content: vec![ToolResultContentBlock::Text(format!(
                                    "PreToolHook blocked the tool execution: {}",
                                    hook_res.output().unwrap_or("no output provided")
                                ))],
                                status: ToolResultStatus::Error,
                            })
                        })
                        .collect();
                    self.conversation_state
                        .messages
                        .push(Message::new(Role::User, content, Some(Utc::now())));
                    self.send_request().await?;
                    return Ok(());
                }

                // Otherwise, continue to the approval stage.
                let tools = tools.clone();
                if !needs_approval.is_empty() {
                    let needs_approval = needs_approval.clone();
                    self.request_tool_approvals(tools, needs_approval).await?;
                } else {
                    self.execute_tools(tools).await?;
                }
                Ok(())
            },
            HookStage::PostToolUse { tool_results } => {
                let tool_results = tool_results.clone();
                self.send_tool_results(tool_results).await?;
                Ok(())
            },
        }
    }

    async fn make_tool_spec(&self) -> Result<Vec<ToolSpec>, AgentError> {
        let tool_names = self.get_tool_names().await?;

        let mut tool_specs = Vec::new();
        for name in tool_names {
            match &name {
                CanonicalToolName::BuiltIn(name) => tool_specs.push(BuiltInTool::generate_tool_spec(name)),
                name @ CanonicalToolName::Mcp { server_name, tool_name } => {
                    tool_specs.push(self.mcp_manager_handle.generate_tool_spec(name).await?);
                },
                CanonicalToolName::Agent { agent_name } => {
                    // TODO: generate tool spec from agent config
                },
            }
        }

        Ok(tool_specs)
    }

    /// Returns the name of all tools available to the given agent.
    async fn get_tool_names(&self) -> Result<Vec<CanonicalToolName>, AgentError> {
        let mut tool_names = HashSet::new();
        let built_in_tool_names = built_in_tool_names();
        let config = self.get_agent_config().await;

        for tool_name in config.tools() {
            if let Ok(kind) = ToolNameKind::parse(&tool_name) {
                match kind {
                    ToolNameKind::All => {
                        // Include all built-in's and MCP servers.
                        // 1. all built-ins
                        // 2. all configured MCP servers
                        for built_in in &built_in_tool_names {
                            tool_names.insert(built_in.clone());
                        }
                    },
                    ToolNameKind::McpFullName { .. } => {
                        if let Ok(tn) = tool_name.parse() {
                            tool_names.insert(tn);
                        }
                    },
                    ToolNameKind::McpServer { server_name } => {
                        // get all tools from the mcp server
                    },
                    ToolNameKind::McpGlob { server_name, glob_part } => {
                        // match only tools for the server name
                    },
                    ToolNameKind::BuiltInGlob(glob) => {
                        let built_ins = built_in_tool_names.iter().map(|tn| tn.tool_name());
                        for tn in find_matches(glob, built_ins) {
                            if let Ok(tn) = tn.parse() {
                                tool_names.insert(tn);
                            }
                        }
                    },
                    ToolNameKind::BuiltIn(name) => {
                        if let Ok(tn) = name.parse() {
                            tool_names.insert(tn);
                        }
                    },
                    ToolNameKind::AllBuiltIn => {
                        for built_in in &built_in_tool_names {
                            tool_names.insert(built_in.clone());
                        }
                    },
                    ToolNameKind::AgentGlob(_) => {
                        // check all agent names
                    },
                    ToolNameKind::Agent(_) => {},
                }
            }
        }

        Ok(tool_names.into_iter().collect())
    }

    /// Parses tool use blocks into concrete tools, returning those that failed to be parsed.
    async fn parse_tools(
        &mut self,
        tool_uses: Vec<ToolUseBlock>,
    ) -> (Vec<(ToolUseBlock, ToolKind)>, Vec<ToolParseError>) {
        let mut tools: Vec<(ToolUseBlock, ToolKind)> = Vec::new();
        let mut parse_errors: Vec<ToolParseError> = Vec::new();

        // Next, parse tool from the name.
        for tool_use in tool_uses {
            let canonical_tool_name = match self.resolve_tool_name(&tool_use.name).await {
                Ok(n) => n,
                Err(err) => {
                    parse_errors.push(ToolParseError::new(tool_use, err));
                    continue;
                },
            };
            let tool = match self.parse_tool(&canonical_tool_name, tool_use.input.clone()).await {
                Ok(t) => t,
                Err(err) => {
                    parse_errors.push(ToolParseError::new(tool_use, err));
                    continue;
                },
            };
            match self.validate_tool(&tool).await {
                Ok(_) => tools.push((tool_use, tool)),
                Err(err) => {
                    parse_errors.push(ToolParseError::new(tool_use, err));
                },
            }
        }

        (tools, parse_errors)
    }

    /// Returns a canonicalized tool name for a given agent
    ///
    /// # Arguments
    ///
    /// - `tool_name` - the name of the tool as returned by the model
    async fn resolve_tool_name(&self, tool_name: &str) -> Result<CanonicalToolName, ToolParseErrorKind> {
        // TODO
        // Resolve any tool name transformations, if required

        // Resolve any aliases, if required
        let config = self.get_agent_config().await;
        let aliases = config.tool_aliases();
        let tool_name = match aliases.iter().find(|(_, v)| *v == tool_name) {
            Some((canon_name, _)) => canon_name,
            None => tool_name,
        };

        // Afterwards, we should have a canonical tool name.
        let canon_tool_name = match tool_name.parse() {
            Ok(tn) => tn,
            // this should never happen
            Err(err) => return Err(ToolParseErrorKind::AmbiguousToolName(err)),
        };

        let tool_names = self.get_tool_names().await?;
        if !tool_names.contains(&canon_tool_name) {
            Err(ToolParseErrorKind::NameDoesNotExist(tool_name.to_string()))
        } else {
            Ok(canon_tool_name)
        }
    }

    async fn parse_tool(
        &self,
        name: &CanonicalToolName,
        args: serde_json::Value,
    ) -> Result<ToolKind, ToolParseErrorKind> {
        match name {
            CanonicalToolName::BuiltIn(name) => match BuiltInTool::from_parts(name, args) {
                Ok(tool) => Ok(ToolKind::BuiltIn(tool)),
                Err(err) => Err(err),
            },
            CanonicalToolName::Mcp { server_name, tool_name } => todo!(),
            CanonicalToolName::Agent { agent_name } => todo!(),
        }
    }

    async fn validate_tool(&self, tool: &ToolKind) -> Result<(), ToolParseErrorKind> {
        match tool {
            ToolKind::BuiltIn(built_in) => match built_in {
                BuiltInTool::FileRead(t) => t.validate().await.map_err(ToolParseErrorKind::invalid_args),
                BuiltInTool::FileWrite(t) => t.validate().await.map_err(ToolParseErrorKind::invalid_args),
                BuiltInTool::Grep(t) => Ok(()),
                BuiltInTool::Ls(t) => Ok(()),
                BuiltInTool::Mkdir(t) => Ok(()),
                BuiltInTool::ExecuteCmd(t) => Ok(()),
                BuiltInTool::Introspect(t) => Ok(()),
                BuiltInTool::SpawnSubagent => Ok(()),
                BuiltInTool::ImageRead(t) => Ok(()),
            },
            ToolKind::Mcp(t) => Ok(()),
        }
    }

    async fn evaluate_tool_permission(&mut self, tool: &ToolKind) -> Result<PermissionEvalResult, AgentError> {
        let config = self.get_agent_config().await;
        let allowed_tools = config.allowed_tools();
        match evaluate_tool_permission(allowed_tools, config.tool_settings(), tool) {
            Ok(res) => Ok(res),
            Err(err) => {
                warn!(?err, "failed to evaluate tool permission");
                Ok(PermissionEvalResult::Ask)
            },
        }
    }

    async fn request_tool_approvals(
        &mut self,
        tools: Vec<(ToolUseBlock, ToolKind)>,
        needs_approval: Vec<String>,
    ) -> Result<(), AgentError> {
        // First, update the agent state to WaitingForApproval
        let mut needs_approval_res = HashMap::new();
        for tool_use_id in &needs_approval {
            debug_assert!(
                tools.iter().find(|(b, _)| &b.tool_use_id == tool_use_id).is_some(),
                "unexpected tool use id requiring approval: tools: {:?} needs_approval: {:?}",
                tools,
                needs_approval
            );
            needs_approval_res.insert(tool_use_id.clone(), None);
        }
        self.set_active_state(ActiveState::WaitingForApproval {
            tools: tools.clone(),
            needs_approval: needs_approval_res,
        })
        .await;

        // Send notifications for each tool that requires approval
        for tool_use_id in &needs_approval {
            let Some((block, tool)) = tools.iter().find(|(b, _)| &b.tool_use_id == tool_use_id) else {
                continue;
            };
            let _ = self.agent_event_tx.send(AgentEvent::ApprovalRequest {
                id: block.tool_use_id.clone(),
                tool_use: (*block).clone(),
                context: tool.get_context().await,
            });
        }

        Ok(())
    }

    async fn execute_tools(&mut self, tools: Vec<(ToolUseBlock, ToolKind)>) -> Result<(), AgentError> {
        let mut tool_state = HashMap::new();
        for (block, tool) in tools {
            let id = ToolExecutionId::new(block.tool_use_id.clone());
            tool_state.insert(id.clone(), ((block.clone(), tool.clone()), None));
            self.start_tool_execution(id.clone(), tool).await?;
        }
        self.set_active_state(ActiveState::ExecutingTools { tools: tool_state })
            .await;
        Ok(())
    }

    /// Starts executing a tool for the given agent. Tools are executed in parallel on a background
    /// task.
    async fn start_tool_execution(&mut self, id: ToolExecutionId, tool: ToolKind) -> Result<(), AgentError> {
        let tool_clone = tool.clone();

        // Channel for handling tool-specific state updates.
        let (tx, rx) = oneshot::channel::<ToolState>();

        let fut: ToolFuture = match tool {
            ToolKind::BuiltIn(builtin) => match builtin {
                BuiltInTool::FileRead(t) => Box::pin(async move { t.execute().await }),
                BuiltInTool::FileWrite(t) => {
                    let file_write = self.tool_state.file_write.clone();
                    let mut tool_state = ToolState { file_write };
                    Box::pin(async move {
                        let res = t.execute(tool_state.file_write.as_mut()).await;
                        if res.is_ok() {
                            let _ = tx.send(tool_state);
                        }
                        res
                    })
                },
                BuiltInTool::ExecuteCmd(t) => Box::pin(async move { t.execute().await }),
                BuiltInTool::ImageRead(t) => todo!(),
                BuiltInTool::Introspect(t) => todo!(),
                BuiltInTool::Grep(t) => todo!(),
                BuiltInTool::Ls(t) => todo!(),
                BuiltInTool::Mkdir(t) => todo!(),
                BuiltInTool::SpawnSubagent => todo!(),
            },
            ToolKind::Mcp(t) => todo!(),
        };

        self.task_executor
            .start_tool_execution(StartToolExecution {
                id,
                tool: tool_clone,
                fut,
                context_rx: rx,
            })
            .await;
        Ok(())
    }

    async fn send_tool_results(&mut self, tool_results: Vec<ToolExecutorResult>) -> Result<(), AgentError> {
        let mut content = Vec::new();
        for result in tool_results {
            match result {
                ToolExecutorResult::Completed { id, result } => match result {
                    Ok(res) => {
                        for item in &res.items {
                            let content_item = match item {
                                ToolExecutionOutputItem::Text(s) => ToolResultContentBlock::Text(s.clone()),
                                ToolExecutionOutputItem::Json(v) => ToolResultContentBlock::Json(v.clone()),
                                ToolExecutionOutputItem::Image(i) => ToolResultContentBlock::Image(i.clone()),
                            };
                            content.push(ContentBlock::ToolResult(ToolResultBlock {
                                tool_use_id: id.tool_use_id().to_string(),
                                content: vec![content_item],
                                status: ToolResultStatus::Success,
                            }));
                        }
                    },
                    Err(err) => content.push(ContentBlock::ToolResult(ToolResultBlock {
                        tool_use_id: id.tool_use_id().to_string(),
                        content: vec![ToolResultContentBlock::Text(err.to_string())],
                        status: ToolResultStatus::Error,
                    })),
                },
                ToolExecutorResult::Cancelled { .. } => {
                    // Should never happen in this flow
                },
            }
        }

        self.conversation_state
            .messages
            .push(Message::new(Role::User, content, Some(Utc::now())));

        self.send_request().await?;
        self.set_active_state(ActiveState::ExecutingRequest).await;
        Ok(())
    }
}

fn format_user_context_message<T, U, S>(
    summary: Option<&str>,
    system_prompt: Option<&str>,
    resources: T,
    agent_spawn_hooks: U,
) -> String
where
    T: IntoIterator<Item = S>,
    U: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let mut context_content = String::new();
    if let Some(v) = summary {
        context_content.push_str(CONTEXT_ENTRY_START_HEADER);
        context_content.push_str("This summary contains ALL relevant information from our previous conversation including tool uses, results, code analysis, and file operations. YOU MUST reference this information when answering questions and explicitly acknowledge specific details from the summary when they're relevant to the current question.\n\n");
        context_content.push_str("SUMMARY CONTENT:\n");
        context_content.push_str(v);
        context_content.push('\n');
        context_content.push_str(CONTEXT_ENTRY_END_HEADER);
    }

    if let Some(prompt) = system_prompt {
        context_content.push_str(&format!("Follow this instruction: {}", prompt));
        context_content.push_str("\n\n");
    }

    for hook in agent_spawn_hooks {
        let content = hook.as_ref();
        context_content.push_str(CONTEXT_ENTRY_START_HEADER);
        context_content.push_str("This section (like others) contains important information that I want you to use in your responses. I have gathered this context from valuable programmatic script hooks. You must follow any requests and consider all of the information in this section");
        context_content.push_str(" for the entire conversation\n\n");
        context_content.push_str(content);
        context_content.push_str("\n\n");
        context_content.push_str(CONTEXT_ENTRY_END_HEADER);
    }

    for resource in resources {
        let content = resource.as_ref();
        context_content.push_str(CONTEXT_ENTRY_START_HEADER);
        context_content.push_str(content);
        context_content.push_str("\n\n");
        context_content.push_str(CONTEXT_ENTRY_END_HEADER);
    }

    context_content
}

/// Updates the history so that, when non-empty, the following invariants are in place:
/// - The history length is `<= MAX_CONVERSATION_STATE_HISTORY_LEN`. Oldest messages are dropped.
/// - Any tool uses that do not exist in the provided tool specs will have their arguments replaced
///   with dummy content.
fn enforce_conversation_invariants(messages: &mut VecDeque<Message>, tools: &mut Vec<ToolSpec>) {
    // First, trim the conversation history by finding the second oldest message from the user without
    // tool results - this will be the new oldest message in the history.
    //
    // Note that we reserve extra slots for context messages.
    const MAX_HISTORY_LEN: usize = MAX_CONVERSATION_STATE_HISTORY_LEN - 2;
    let need_to_trim_front = messages
        .front()
        .is_none_or(|m| !(m.role == Role::User && m.tool_results().is_none()))
        || messages.len() > MAX_HISTORY_LEN;
    if need_to_trim_front {
        match messages
            .iter()
            .enumerate()
            .find(|(i, v)| (messages.len() - i) < MAX_HISTORY_LEN && v.role == Role::User && v.tool_results().is_none())
        {
            Some((i, m)) => {
                trace!(i, ?m, "found valid starting user message with no tool results");
                messages.drain(0..i);
            },
            None => {
                trace!("no valid starting user message found in the history, clearing");
                messages.clear();
                return;
            },
        }
    }

    // Replace any missing tool use references with a dummy tool spec.
    let tool_names: HashSet<_> = tools.iter().map(|t| t.name.clone()).collect();
    let mut insert_dummy_spec = false;
    for msg in messages {
        for block in &mut msg.content {
            if let ContentBlock::ToolUse(v) = block {
                if !tool_names.contains(&v.name) {
                    v.name = DUMMY_TOOL_NAME.to_string();
                    insert_dummy_spec = true;
                }
            }
        }
    }
    if insert_dummy_spec {
        tools.push(ToolSpec {
            name: DUMMY_TOOL_NAME.to_string(),
            description: "This is a dummy tool. If you are seeing this that means the tool associated with this tool call is not in the list of available tools. This could be because a wrong tool name was supplied or the list of tools has changed since the conversation has started. Do not show this when user asks you to list tools.".to_string(),
            input_schema: serde_json::from_str(r#"{"type": "object", "properties": {}, "required": [] }"#).unwrap(),
        });
    }
}

async fn collect_resources<T, U>(resources: T) -> Vec<Resource>
where
    T: IntoIterator<Item = U>,
    U: AsRef<str>,
{
    use glob;

    let mut return_val = Vec::new();
    for resource in resources {
        let Ok(kind) = ResourceKind::parse(resource.as_ref()) else {
            continue;
        };
        match kind {
            ResourceKind::File { original, file_path } => {
                let Ok(path) = canonicalize_path(file_path) else {
                    continue;
                };
                let Ok((content, _)) = read_file_with_max_limit(path, MAX_RESOURCE_FILE_LENGTH, "...truncated").await
                else {
                    continue;
                };
                return_val.push(Resource {
                    config_value: original.to_string(),
                    content,
                });
            },
            ResourceKind::FileGlob { original, pattern } => {
                let Ok(entries) = glob::glob(pattern.as_str()) else {
                    continue;
                };
                for entry in entries {
                    let Ok(entry) = entry else {
                        continue;
                    };
                    if entry.is_file() {
                        let Ok((content, _)) =
                            read_file_with_max_limit(entry.as_path(), MAX_RESOURCE_FILE_LENGTH, "...truncated").await
                        else {
                            continue;
                        };
                        return_val.push(Resource {
                            config_value: original.to_string(),
                            content,
                        });
                    }
                }
            },
        }
    }

    return_val
}

const MAX_RESOURCE_FILE_LENGTH: u64 = 1024 * 10;

/// Reads a file to a maximum file length, returning the content and number of bytes truncated. If
/// the file has to be truncated, content is suffixed with `truncated_suffix`.
///
/// The returned content length is guaranteed to not be greater than `max_file_length`.
async fn read_file_with_max_limit(
    path: impl AsRef<Path>,
    max_file_length: u64,
    truncated_suffix: impl AsRef<str>,
) -> Result<(String, u64), UtilError> {
    let path = path.as_ref();
    let suffix = truncated_suffix.as_ref();
    let file = tokio::fs::File::open(path)
        .await
        .with_context(|| format!("Failed to open file at '{}'", path.to_string_lossy()))?;
    let md = file
        .metadata()
        .await
        .with_context(|| format!("Failed to query file metadata at '{}'", path.to_string_lossy()))?;

    let truncated_amount = if md.size() > max_file_length {
        // Edge case check to ensure the suffix is less than max file length.
        if suffix.len() as u64 > max_file_length {
            return Ok((String::new(), md.size()));
        }
        md.size() - max_file_length + suffix.len() as u64
    } else {
        0
    };

    // Read only the max supported length.
    let mut reader = BufReader::new(file).take(max_file_length);
    let mut content = Vec::new();
    reader
        .read_to_end(&mut content)
        .await
        .with_context(|| format!("Failed to read from file at '{}'", path.to_string_lossy()))?;

    // Truncate content safely.
    let mut content = content.to_str_lossy().to_string();
    truncate_safe_in_place(&mut content, max_file_length as usize, suffix);

    Ok((content, truncated_amount))
}

fn hook_matches_tool(config: &HookConfig, tool: &ToolKind) -> bool {
    let Some(matcher) = config.matcher() else {
        // No matcher -> hook runs for all tools.
        return true;
    };
    let Ok(kind) = ToolNameKind::parse(matcher) else {
        return false;
    };
    match kind {
        ToolNameKind::All => true,
        ToolNameKind::McpFullName { server_name, tool_name } => {
            tool.canonical_tool_name().as_full_name()
                == CanonicalToolName::from_mcp_parts(server_name.to_string(), tool_name.to_string()).as_full_name()
        },
        ToolNameKind::McpServer { server_name } => tool.mcp_server_name() == Some(server_name),
        ToolNameKind::McpGlob { server_name, glob_part } => {
            tool.mcp_server_name() == Some(server_name)
                && tool
                    .mcp_tool_name()
                    .is_some_and(|n| matches_any_pattern([glob_part], n))
        },
        ToolNameKind::AllBuiltIn => matches!(tool, ToolKind::BuiltIn(_)),
        ToolNameKind::BuiltInGlob(glob) => tool.builtin_tool_name().is_some_and(|n| matches_any_pattern([glob], n)),
        ToolNameKind::BuiltIn(name) => tool.builtin_tool_name().is_some_and(|n| n.as_ref() == name),
        ToolNameKind::AgentGlob(_) => false,
        ToolNameKind::Agent(_) => false,
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecutionState {
    pub active_state: ActiveState,
    pub executing_subagents: HashMap<AgentId, Option<String>>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ActiveState {
    #[default]
    Idle,
    Errored(AgentError),
    /// Agent is waiting for approval to execute tool uses
    WaitingForApproval {
        /// All tools requested by the model
        tools: Vec<(ToolUseBlock, ToolKind)>,
        /// Map from a tool use id to the approval result and tool to execute
        needs_approval: HashMap<String, Option<ApprovalResult>>,
    },
    /// Agent is currently executing hooks
    ExecutingHooks(ExecutingHooks),
    /// Agent is currently handling a prompt
    ///
    /// The agent is not able to receive new prompts while in this state
    ExecutingRequest,
    /// Agent is executing tools
    ExecutingTools {
        tools: HashMap<ToolExecutionId, ((ToolUseBlock, ToolKind), Option<ToolExecutorResult>)>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecutingHooks {
    /// Tracker for results.
    ///
    /// Also contains tool context used for the hook execution, if available - used to potentially
    /// block tool execution.
    hooks: HashMap<HookExecutionId, (Option<(ToolUseBlock, ToolKind)>, Option<HookResult>)>,
    /// Stage of execution.
    ///
    /// This is how we track what needs to be done post hook execution, e.g. send a prompt or run a
    /// tool.
    stage: HookStage,
}

/// Stage of execution.
///
/// This is how we track what needs to be done post hook execution, e.g. send a prompt or run a
/// tool.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum HookStage {
    /// Agent spawn hooks ran on startup
    AgentSpawn,
    /// Hooks before sending a prompt
    PrePrompt { args: SendPromptArgs },
    /// Hooks before checking for tool use approval.
    ///
    /// This occurs after tool validation, done as a user-controlled validation step.
    PreToolUse {
        /// All tools requested by the model
        tools: Vec<(ToolUseBlock, ToolKind)>,
        /// List of the tool use id's that require user approval
        needs_approval: Vec<String>,
    },
    /// Hooks after executing tool uses
    PostToolUse { tool_results: Vec<ToolExecutorResult> },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_collect_resources() {
        let r = collect_resources(vec!["file://AGENTS.md"]).await;
        println!("{:?}", r);
    }
}
