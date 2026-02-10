pub mod agent_config;
pub mod agent_loop;
pub mod compact;
pub mod consts;
pub mod event_log;
pub mod mcp;
pub mod permissions;
pub mod protocol;
pub mod shell_permission;
pub mod task_executor;
pub mod tool_permission;
mod tool_utils;
pub mod tools;
pub mod tui_commands;
pub mod types;
pub mod util;

use std::collections::{
    HashMap,
    HashSet,
    VecDeque,
};
use std::path::PathBuf;
use std::sync::Arc;

use agent_config::LoadedMcpServerConfigs;
use agent_config::definitions::{
    AgentConfig,
    HookConfig,
    HookTrigger,
};
use agent_config::parse::{
    CanonicalToolName,
    ResourceKind,
    ToolNameKind,
};
use agent_loop::model::Model;
use agent_loop::protocol::{
    AgentLoopEvent,
    AgentLoopEventKind,
    AgentLoopResponse,
    LoopError,
    SendRequestArgs,
    UserTurnMetadata,
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
};
use chrono::Utc;
use code_agent_sdk::CodeIntelligence;
use consts::MAX_RESOURCE_FILE_LENGTH;
use event_log::{
    LogEntry,
    ToolResult as LogToolResult,
};
use futures::stream::FuturesUnordered;
use mcp::McpServerEvent;
use permissions::{
    RuntimePermissions,
    apply_approval_to_permissions,
    evaluate_tool_permission,
};
use protocol::{
    AgentError,
    AgentEvent,
    AgentRequest,
    AgentResponse,
    AgentStopReason,
    ContentChunk,
    InitializeUpdateEvent,
    InternalEvent,
    PermissionEvalResult,
    PermissionOption,
    PermissionOptionId,
    SendApprovalResultArgs,
    SendPromptArgs,
    SwapAgentArgs,
    ToolCall,
    ToolCallResult,
    UpdateEvent,
};
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
use tokio::sync::{
    RwLock,
    broadcast,
    mpsc,
    oneshot,
};
use tokio::time::Instant;
use tokio_stream::StreamExt as _;
use tokio_util::sync::CancellationToken;
use tool_utils::{
    SanitizedToolSpecs,
    add_tool_use_purpose_arg,
    sanitize_tool_specs,
};
use tools::{
    Tool,
    ToolExecutionError,
    ToolExecutionOutput,
    ToolExecutionOutputItem,
    ToolParseError,
    ToolParseErrorKind,
};
use tracing::{
    debug,
    error,
    info,
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
use util::path::canonicalize_path_sys;
use util::providers::{
    RealProvider,
    SystemProvider,
};
use util::read_file_with_max_limit;
use util::request_channel::new_request_channel;
use uuid::Uuid;

use crate::agent::compact::{
    CompactStrategy,
    create_compaction_request,
};
use crate::agent::consts::{
    DUMMY_TOOL_NAME,
    MAX_CONVERSATION_STATE_HISTORY_LEN,
};
use crate::agent::mcp::{
    McpManager,
    McpManagerHandle,
};
use crate::agent::protocol::{
    ClearEvent,
    CompactionEvent,
};
use crate::agent::tools::{
    BuiltInTool,
    ToolKind,
    ToolState,
    built_in_tool_names,
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

pub const CONTEXT_ENTRY_START_HEADER: &str = "--- CONTEXT ENTRY BEGIN ---\n";
pub const CONTEXT_ENTRY_END_HEADER: &str = "--- CONTEXT ENTRY END ---\n\n";
pub const SKILL_FILES_MESSAGE: &str = "The following file entries contain: name, filepath, and description. You SHOULD decide when to read the full file using the filepath based on its description:\n\n";

/// Handle for communicating with an [`Agent`] actor.
#[derive(Debug)]
pub struct AgentHandle {
    sender: RequestSender<AgentRequest, AgentResponse, AgentError>,
    event_rx: broadcast::Receiver<AgentEvent>,
}

impl Drop for AgentHandle {
    fn drop(&mut self) {
        if self.sender.count() == 1 {
            self.terminate();
        }
    }
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
            other => Err(AgentError::Custom(format!("received unexpected response: {other:?}"))),
        }
    }

    pub async fn send_tool_use_approval_result(&self, args: SendApprovalResultArgs) -> Result<(), AgentError> {
        tracing::error!("tool use approval sent");
        match self
            .sender
            .send_recv(AgentRequest::SendApprovalResult(args))
            .await
            .unwrap_or(Err(AgentError::Channel))?
        {
            AgentResponse::Success => Ok(()),
            other => Err(AgentError::Custom(format!("received unexpected response: {other:?}"))),
        }
    }

    pub async fn create_snapshot(&self) -> Result<AgentSnapshot, AgentError> {
        match self
            .sender
            .send_recv(AgentRequest::CreateSnapshot)
            .await
            .unwrap_or(Err(AgentError::Channel))?
        {
            AgentResponse::Snapshot(snapshot) => Ok(snapshot),
            other => Err(AgentError::Custom(format!("received unexpected response: {other:?}"))),
        }
    }

    pub async fn cancel(&self) -> Result<(), AgentError> {
        match self
            .sender
            .send_recv(AgentRequest::Cancel)
            .await
            .unwrap_or(Err(AgentError::Channel))?
        {
            AgentResponse::Success => Ok(()),
            other => Err(AgentError::Custom(format!("received unexpected response: {other:?}"))),
        }
    }

    pub async fn swap_agent(&self, args: SwapAgentArgs) -> Result<(), AgentError> {
        match self
            .sender
            .send_recv(AgentRequest::SwapAgent(Box::new(args)))
            .await
            .unwrap_or(Err(AgentError::Channel))?
        {
            AgentResponse::SwapComplete => Ok(()),
            other => Err(AgentError::Custom(format!("received unexpected response: {other:?}"))),
        }
    }

    pub fn terminate(&self) {
        _ = self.sender.try_blocking_send_recv(AgentRequest::Terminate);
    }

    pub async fn compact_conversation(&self) -> Result<(), AgentError> {
        match self
            .sender
            .send_recv(AgentRequest::CompactConversation)
            .await
            .unwrap_or(Err(AgentError::Channel))?
        {
            AgentResponse::Success => Ok(()),
            other => Err(AgentError::Custom(format!("received unexpected response: {other:?}"))),
        }
    }

    pub async fn clear_conversation(&self) -> Result<(), AgentError> {
        match self
            .sender
            .send_recv(AgentRequest::ClearConversation)
            .await
            .unwrap_or(Err(AgentError::Channel))?
        {
            AgentResponse::Success => Ok(()),
            other => Err(AgentError::Custom(format!("received unexpected response: {other:?}"))),
        }
    }
}

/// Core LLM agent that implements an [`AgentConfig`].
///
/// Use [`Agent::spawn`] to start the actor and obtain an [`AgentHandle`].
#[derive(Debug)]
pub struct Agent {
    id: AgentId,
    agent_config: AgentConfig,

    conversation_state: ConversationState,
    conversation_metadata: ConversationMetadata,
    execution_state: ExecutionState,
    tool_state: ToolState,
    /// Runtime permissions accumulated during the session
    permissions: RuntimePermissions,

    agent_event_tx: broadcast::Sender<AgentEvent>,
    agent_event_rx: Option<broadcast::Receiver<AgentEvent>>,

    agent_event_buf: Vec<AgentEvent>,

    /// Contains an [AgentLoop] if the agent is in the middle of executing a user turn, otherwise
    /// is [None].
    agent_loop: Option<AgentLoopHandle>,

    /// Contains an [AgentLoop] for compaction requests, separate from the main agent loop.
    compaction_loop: Option<AgentLoopHandle>,

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
    model: Arc<dyn Model>,

    /// Configuration settings to alter agent behavior.
    settings: AgentSettings,

    /// Cached result when creating a tool spec for sending to the backend.
    ///
    /// Required since we may perform transformations on the tool names and descriptions that are
    /// sent to the model.
    cached_tool_specs: Option<SanitizedToolSpecs>,
    /// Cached result of loading all MCP configs according to the agent config during
    /// initialization.
    ///
    /// Done for simplicity and to avoid rereading global MCP config files every time we process a
    /// request.
    cached_mcp_configs: LoadedMcpServerConfigs,

    /// Provider for system context like env vars, home dir, current working dir
    sys_provider: Arc<dyn SystemProvider>,
    /// Denotes whether or not this agent is being spawned as a subagent
    is_subagent: bool,
    /// Shared code intelligence client for LSP operations (optional)
    code_intelligence: Option<Arc<RwLock<CodeIntelligence>>>,
}

impl Agent {
    /// Creates an agent using the given initial state.
    ///
    /// To actually initialize the agent and begin interacting with it, call [Agent::spawn].
    ///
    /// # Arguments
    ///
    /// * `snapshot` - Agent state to initialize with
    /// * `local_mcp_path` - The path to workspace level mcp.json
    /// * `global_mcp_path` - The path to global mcp.json
    /// * `model` - The backend implementation to use
    /// * `mcp_manager_handle` - Handle to an actor managing MCP servers
    /// * `is_subagent` - whether or not the agent is spawned as a subagent
    /// * `code_intelligence` - Shared code intelligence client (optional)
    pub async fn new(
        snapshot: AgentSnapshot,
        local_mcp_path: Option<&PathBuf>,
        global_mcp_path: Option<&PathBuf>,
        model: Arc<dyn Model>,
        mcp_manager_handle: McpManagerHandle,
        is_subagent: bool,
        code_intelligence: Option<Arc<RwLock<CodeIntelligence>>>,
    ) -> eyre::Result<Agent> {
        debug!(?snapshot, "initializing agent from snapshot");

        let (agent_event_tx, agent_event_rx) = broadcast::channel(1024);

        let agent_config = snapshot.agent_config;

        let cached_mcp_configs =
            LoadedMcpServerConfigs::from_agent_config(&agent_config, local_mcp_path, global_mcp_path).await;
        let sys_provider: Arc<dyn SystemProvider> = Arc::new(RealProvider);
        let task_executor = TaskExecutor::new(Arc::clone(&sys_provider));

        Ok(Self {
            id: snapshot.id,
            agent_config,
            conversation_state: snapshot.conversation_state,
            conversation_metadata: snapshot.conversation_metadata,
            execution_state: snapshot.execution_state,
            tool_state: snapshot.tool_state,
            permissions: snapshot.permissions,
            agent_event_tx,
            agent_event_rx: Some(agent_event_rx),
            agent_event_buf: Vec::new(),
            agent_loop: None,
            compaction_loop: None,
            task_executor,
            mcp_manager_handle,
            agent_spawn_hooks: Default::default(),
            model,
            settings: snapshot.settings,
            cached_tool_specs: None,
            cached_mcp_configs,
            sys_provider,
            is_subagent,
            code_intelligence,
        })
    }

    pub fn set_sys_provider(&mut self, provider: impl SystemProvider) {
        self.sys_provider = Arc::new(provider);
        self.task_executor = TaskExecutor::new(Arc::clone(&self.sys_provider));
    }

    /// Starts the agent task, returning a handle from which messages can be sent and events can be
    /// received.
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
        {
            if !self.cached_mcp_configs.overridden_configs.is_empty() {
                warn!(?self.cached_mcp_configs.overridden_configs, "ignoring overridden configs");
            }

            // Here we need to monitor mcp manager for events that are related to initialization
            // and surface them. One example is oauth request.
            let ct = CancellationToken::new();
            let _guard = ct.clone().drop_guard();
            let mut mcp_manager_handle = self.mcp_manager_handle.clone();
            let agent_event_tx = self.agent_event_tx.clone();

            tokio::spawn(async move {
                loop {
                    tokio::select! {
                        _ = ct.cancelled() => {
                            break;
                        },

                        evt = mcp_manager_handle.recv() => {
                            let Ok(evt) = evt else {
                                error!("mcp manager handle channel closed");
                                break;
                            };

                            _ = agent_event_tx.send(AgentEvent::InitializeUpdate(InitializeUpdateEvent::Mcp(evt)));
                        }
                    }
                }
            });

            self.launch_mcp_servers().await;
        }

        // Next, run agent spawn hooks.
        let hooks = self.get_hooks(HookTrigger::AgentSpawn);
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
            self.start_hooks_execution(hooks, HookStage::AgentSpawn, None).await;
        } else {
            self.agent_event_buf.push(AgentEvent::Initialized);
        }
    }

    #[inline]
    async fn launch_mcp_servers(&mut self) {
        let mut results = FuturesUnordered::new();

        for config in self
            .cached_mcp_configs
            .configs
            .iter()
            .filter(|config| config.is_enabled())
        {
            let Ok(rx) = self
                .mcp_manager_handle
                .launch_server(config.server_name.clone(), config.config.clone())
                .await
            else {
                warn!(?config.server_name, "failed to launch MCP config, skipping");
                continue;
            };
            let name = config.server_name.clone();
            results.push(async move { (name, rx.await) });
        }

        // Continually loop through the receivers until all have completed.
        let mut launched_servers = Vec::new();
        let (success_tx, mut success_rx) = mpsc::channel(8);
        let mut failed_servers = Vec::new();
        let (failed_tx, mut failed_rx) = mpsc::channel(8);
        let init_results_handle = tokio::spawn(async move {
            while let Some((name, res)) = results.next().await {
                debug!(?name, ?res, "received result from LaunchServer request");
                let Ok(res) = res else {
                    warn!(?name, "channel unexpectedly dropped during MCP initialization");
                    let _ = failed_tx.send(name).await;
                    continue;
                };
                match res {
                    Ok(_) => {
                        let _ = success_tx.send(name).await;
                    },
                    Err(err) => {
                        error!(?name, ?err, "failed to launch MCP server");
                        let _ = failed_tx.send(name).await;
                    },
                }
            }
        });

        let timeout_at = Instant::now() + self.settings.mcp_init_timeout;
        loop {
            tokio::select! {
                name = success_rx.recv() => {
                    let Some(name) = name else {
                        // If None is returned in either success/failed receivers, then the
                        // senders have dropped, meaning initialization has completed.
                        break;
                    };
                    debug!(?name, "MCP server successfully initialized");
                    launched_servers.push(name.clone());
                },
                name = failed_rx.recv() => {
                    let Some(name) = name else {
                        break;
                    };
                    warn!(?name, "MCP server failed initialization");
                    failed_servers.push(name);
                },
                _ = tokio::time::sleep_until(timeout_at) => {
                    warn!("timed out before all MCP servers could be initialized");
                    break;
                },
            }
        }
        info!(?launched_servers, ?failed_servers, "MCP server initialization finished");
        init_results_handle.abort();
    }

    async fn main_loop(mut self, mut request_rx: RequestReceiver<AgentRequest, AgentResponse, AgentError>) {
        let mut task_executor_event_buf = Vec::new();

        loop {
            for event in self.agent_event_buf.drain(..) {
                let _ = self.agent_event_tx.send(event);
            }

            tokio::select! {
                req = request_rx.recv() => {
                    let Some(req) = req else {
                        warn!("session request receiver channel has closed, exiting");
                        break;
                    };
                    let res = self.handle_agent_request(req.payload).await;

                    if let Ok(AgentResponse::TerminateAcknowledged) = res {
                        respond!(req, res);
                        break;
                    } else {
                        respond!(req, res);
                    }
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
                    let evt = res;
                    if let Err(e) = self.handle_agent_loop_event(evt).await {
                        error!(?e, "failed to handle agent loop event");
                        self.set_active_state(ActiveState::Errored(e)).await;
                    }
                },

                // Branch for handling compaction loop events
                res = async {
                    match self.compaction_loop.as_mut() {
                        Some(handle) => handle.recv().await,
                        None => std::future::pending().await,
                    }
                } => {
                    if let Err(e) = self.handle_compaction_loop_event(res).await {
                        error!(?e, "failed to handle compaction loop event");
                        self.agent_event_buf.push(AgentEvent::Compaction(CompactionEvent::Failed {
                            error: e.to_string(),
                        }));
                        self.set_active_state(ActiveState::Errored(e)).await;
                    }
                },

                _ = self.task_executor.recv_next(&mut task_executor_event_buf) => {
                    for evt in task_executor_event_buf.drain(..) {
                        if let Err(e) = self.handle_task_executor_event(evt.clone()).await {
                            error!(?e, "failed to handle tool executor event");
                            self.set_active_state(ActiveState::Errored(e)).await;
                        }
                        self.agent_event_buf.push(evt.into());
                    }
                },

                evt = self.mcp_manager_handle.recv() => {
                    match evt {
                        Ok(evt) => {
                            self.handle_mcp_events(evt).await;
                        },
                        Err(e) => {
                            error!(?e, "mcp manager handle closed");
                        }
                    }
                },
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
        self.agent_event_buf
            .push(AgentEvent::Internal(InternalEvent::StateChange { from, to }));
    }

    /// Clears all conversation-related state for a fresh start.
    fn clear_conversation(&mut self) {
        // Append Clear to event log (keeps session, clears messages)
        self.conversation_state.append_log(LogEntry::clear());
        self.conversation_metadata = ConversationMetadata::default();
        self.tool_state = ToolState::default();
        self.agent_event_buf.push(AgentEvent::Clear(ClearEvent));
    }

    fn create_snapshot(&self) -> AgentSnapshot {
        AgentSnapshot {
            id: self.id.clone(),
            agent_config: self.agent_config.clone(),
            conversation_state: self.conversation_state.clone(),
            conversation_metadata: self.conversation_metadata.clone(),
            execution_state: self.execution_state.clone(),
            model_state: self.model.state(),
            tool_state: self.tool_state.clone(),
            settings: self.settings.clone(),
            permissions: self.permissions.clone(),
        }
    }

    async fn get_agent_config(&self) -> &AgentConfig {
        &self.agent_config
    }

    fn get_hooks(&self, trigger: HookTrigger) -> Vec<Hook> {
        let config = &self.agent_config;
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

    /// Ends the current user turn by cancelling [Self::agent_loop] if it exists.
    async fn end_current_turn(&mut self) -> Result<Option<UserTurnMetadata>, AgentError> {
        let Some(mut handle) = self.agent_loop.take() else {
            return Ok(None);
        };

        // Check if the last message is from assistant and has tool uses that need to be cancelled
        let has_pending_tool_uses = self
            .conversation_state
            .messages()
            .last()
            .filter(|m| m.role == Role::Assistant)
            .and_then(|m| m.tool_uses())
            .is_some();

        if has_pending_tool_uses {
            // If the agent is in the middle of sending tool uses, then add two new
            // messages:
            // 1. user tool results replaced with content: "Tool use was cancelled by the user"
            // 2. assistant message with content: "Tool uses were interrupted, waiting for the next user prompt"
            let mut content = Vec::new();
            let mut results = HashMap::new();
            if let Some(m) = self.conversation_state.messages().last() {
                for c in &m.content {
                    if let ContentBlock::ToolUse(tool_use) = c {
                        content.push(ContentBlock::ToolResult(ToolResultBlock {
                            tool_use_id: tool_use.tool_use_id.clone(),
                            content: vec![ToolResultContentBlock::Text(
                                "Tool use was cancelled by the user".to_string(),
                            )],
                            status: ToolResultStatus::Error,
                        }));
                        results.insert(tool_use.tool_use_id.clone(), LogToolResult {
                            tool: None,
                            result: ToolCallResult::Cancelled,
                        });
                    }
                }
            }
            self.append_tool_results(content, results);
            self.append_assistant_message(Message::new(
                Role::Assistant,
                vec![ContentBlock::Text(
                    "Tool uses were interrupted, waiting for the next user prompt".to_string(),
                )],
                Some(Utc::now()),
            ));
        }

        handle.cancel().await?;
        while let Some(evt) = handle.recv().await {
            self.agent_event_buf
                .push(AgentLoopEvent::new(handle.id().clone(), evt.clone()).into());
            if let AgentLoopEventKind::UserTurnEnd(md) = evt {
                self.conversation_metadata.user_turn_metadatas.push(md.clone());

                // Cancel the user message if needed
                let should_cancel_prompt = matches!(self.active_state(), ActiveState::ExecutingRequest)
                    && self
                        .conversation_state
                        .messages()
                        .last()
                        .is_some_and(|m| m.role == Role::User);
                if should_cancel_prompt {
                    let entry = LogEntry::cancelled_prompt();
                    let index = self.conversation_state.append_log(entry.clone());
                    self.agent_event_buf.push(AgentEvent::LogEntryAppended { entry, index });
                }

                self.agent_event_buf.push(AgentEvent::EndTurn(md.clone()));
                return Ok(Some(md));
            }
        }
        Err(AgentError::Custom(
            "agent loop did not return user turn metadata".to_string(),
        ))
    }

    async fn handle_agent_request(&mut self, req: AgentRequest) -> Result<AgentResponse, AgentError> {
        debug!(?req, "handling agent request");

        match req {
            AgentRequest::SendPrompt(args) => self.handle_send_prompt(args).await,
            AgentRequest::Cancel => self.handle_cancel_request().await,
            AgentRequest::SendApprovalResult(args) => self.handle_approval_result(args).await,
            AgentRequest::CreateSnapshot => Ok(AgentResponse::Snapshot(self.create_snapshot())),
            AgentRequest::GetMcpPrompts => {
                let mut response = HashMap::new();
                for server_name in self.cached_mcp_configs.server_names() {
                    match self.mcp_manager_handle.get_prompts(server_name.clone()).await {
                        Ok(p) => {
                            response.insert(server_name, p);
                        },
                        Err(err) => {
                            warn!(server_name, ?err, "failed to get prompts from server");
                        },
                    }
                }
                Ok(AgentResponse::McpPrompts(response))
            },
            AgentRequest::Terminate => {
                // TODO: Fill this in to ensure a full clean up
                _ = self.handle_cancel_request().await;
                self.mcp_manager_handle.terminate();

                Ok(AgentResponse::TerminateAcknowledged)
            },
            AgentRequest::SwapAgent(args) => self.handle_swap_agent(*args).await,
            AgentRequest::CompactConversation => {
                if !matches!(self.active_state(), ActiveState::Idle) {
                    return Err(AgentError::NotIdle);
                }
                self.start_compaction(CompactStrategy::default_strategy()).await?;
                Ok(AgentResponse::Success)
            },
            AgentRequest::ClearConversation => {
                if !matches!(self.active_state(), ActiveState::Idle) {
                    return Err(AgentError::NotIdle);
                }
                self.clear_conversation();
                Ok(AgentResponse::Success)
            },
        }
    }

    async fn handle_swap_agent(&mut self, args: SwapAgentArgs) -> Result<AgentResponse, AgentError> {
        // Only allow swap when agent is idle
        if !matches!(self.active_state(), ActiveState::Idle) {
            return Err(AgentError::NotIdle);
        }

        // Some clients (e.g. jetbrains) for whatever reason would send swap request with every
        // request. It is not clear to me whether or not ACP is meant to be used this way. As a
        // preemptive measure, we'll try to guard against this via first checking if we even need
        // to swap.
        if self.agent_config.name() == args.agent_config.name() {
            return Ok(AgentResponse::SwapComplete);
        }

        // 1. Terminate existing MCP servers
        self.mcp_manager_handle.terminate();

        // 2. Create new MCP manager (terminate kills the old one)
        self.mcp_manager_handle = McpManager::default().spawn();

        // 3. Update agent config and clear cached tool specs
        self.agent_config = args.agent_config;
        self.cached_tool_specs = None;

        // 4. Reload MCP configs from new agent config
        self.cached_mcp_configs = LoadedMcpServerConfigs::from_agent_config(
            &self.agent_config,
            args.local_mcp_path.as_ref(),
            args.global_mcp_path.as_ref(),
        )
        .await;

        // 5. Launch new MCP servers
        self.launch_mcp_servers().await;

        Ok(AgentResponse::SwapComplete)
    }

    /// Handlers for a [AgentRequest::Cancel] request.
    async fn handle_cancel_request(&mut self) -> Result<AgentResponse, AgentError> {
        match self.active_state() {
            ActiveState::Idle
            | ActiveState::Errored(_)
            | ActiveState::ExecutingRequest
            | ActiveState::Compacting { .. }
            | ActiveState::WaitingForApproval(_) => {},
            ActiveState::ExecutingHooks(executing_hooks) => {
                for hook in executing_hooks.hooks() {
                    self.task_executor.cancel_hook_execution(&hook.id);
                }
            },
            ActiveState::ExecutingTools(executing_tools) => {
                for tool in executing_tools.tools() {
                    self.task_executor.cancel_tool_execution(&tool.id);
                }
            },
        }

        // Send a stop event if required.
        if (self.end_current_turn().await?).is_some() {
            match self.active_state() {
                ActiveState::WaitingForApproval(_)
                | ActiveState::ExecutingHooks(_)
                | ActiveState::ExecutingRequest
                | ActiveState::Compacting { .. }
                | ActiveState::ExecutingTools(_) => {
                    self.agent_event_buf.push(AgentEvent::Stop(AgentStopReason::Cancelled));
                },
                // For errored state, we should have already emitted a stop event.
                ActiveState::Idle | ActiveState::Errored(_) => (),
            };
        }

        if !matches!(self.active_state(), ActiveState::Idle) {
            self.set_active_state(ActiveState::Idle).await;
        }

        Ok(AgentResponse::Success)
    }

    /// Handler for a [AgentRequest::SendApprovalResult] request.
    async fn handle_approval_result(&mut self, args: SendApprovalResultArgs) -> Result<AgentResponse, AgentError> {
        let ActiveState::WaitingForApproval(state) = &mut self.execution_state.active_state else {
            return Err(AgentError::Custom(format!(
                "Cannot send approval to agent with state: {:?}",
                self.execution_state.active_state
            )));
        };

        // Update permissions for "always" options
        if let Some((_, tool)) = state.tools.iter().find(|(b, _)| b.tool_use_id == args.id) {
            apply_approval_to_permissions(&mut self.permissions, tool.kind(), &args.result, &self.sys_provider);
        }

        // Store the selected option
        let Some(approval_state) = state.needs_approval.get_mut(&args.id) else {
            return Err(AgentError::Custom(format!(
                "No tool use with the id '{}' requires approval",
                args.id
            )));
        };
        approval_state.selected = Some(args.result.option_id);

        // Check if any tool was denied - if so, return all results to the model
        let any_denied = state
            .needs_approval
            .values()
            .any(|s| s.selected.as_ref().is_some_and(|id| id.is_reject()));

        if any_denied {
            let mut content = Vec::new();
            let mut results = HashMap::new();
            for (tool_use_id, approval_state) in &state.needs_approval {
                let reason = match &approval_state.selected {
                    Some(id) if id.is_allow() => "Tool use was approved, but did not execute",
                    Some(id) if id.is_reject() => "Tool use was denied by the user.",
                    _ => "Tool use was not executed",
                };
                content.push(ContentBlock::ToolResult(ToolResultBlock {
                    tool_use_id: tool_use_id.clone(),
                    content: vec![ToolResultContentBlock::Text(reason.to_string())],
                    status: ToolResultStatus::Error,
                }));
                let tool = state
                    .tools
                    .iter()
                    .find(|(b, _)| &b.tool_use_id == tool_use_id)
                    .map(|(_, t)| t);
                results.insert(tool_use_id.clone(), LogToolResult {
                    tool: tool.map(|t| Box::new(t.clone())),
                    result: ToolCallResult::Error(ToolExecutionError::Custom(reason.to_string())),
                });
            }
            self.append_tool_results(content, results);
            let args = self.format_request().await;
            self.send_request(args).await?;
            self.set_active_state(ActiveState::ExecutingRequest).await;
            return Ok(AgentResponse::Success);
        }

        // Check if all tools are approved - if so, execute them
        let all_approved = state
            .needs_approval
            .values()
            .all(|s| s.selected.as_ref().is_some_and(|id| id.is_allow()));

        if all_approved {
            let tools = state.tools.clone();
            self.execute_tools(tools).await?;
        }

        Ok(AgentResponse::Success)
    }

    async fn handle_agent_loop_event(&mut self, evt: Option<AgentLoopEventKind>) -> Result<(), AgentError> {
        debug!(?evt, "handling new agent loop event");
        let loop_id = self.agent_loop_handle()?.id().clone();

        // If the event is None, then the channel has dropped, meaning the agent loop has exited.
        // Thus, return early.
        let Some(evt) = evt else {
            self.agent_loop = None;
            return Ok(());
        };

        self.agent_event_buf
            .push(AgentLoopEvent::new(loop_id.clone(), evt.clone()).into());

        match evt {
            AgentLoopEventKind::ResponseStreamEnd { result, metadata } => match result {
                Ok(msg) => {
                    self.append_assistant_message(msg.clone());
                    if !metadata.tool_uses.is_empty() {
                        self.handle_tool_uses(metadata.tool_uses.clone()).await?;
                    }
                },
                Err(err) => {
                    error!(?err, ?loop_id, "response stream encountered an error");
                    self.handle_loop_error_on_stream_end(&err).await?;
                },
            },
            AgentLoopEventKind::UserTurnEnd(md) => {
                self.conversation_metadata.user_turn_metadatas.push(md.clone());

                // Execute Stop hooks if required
                let hooks = self.get_hooks(HookTrigger::Stop);
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
                    self.start_hooks_execution(
                        hooks,
                        HookStage::Stop {
                            user_turn_metadata: Box::new(md),
                        },
                        None,
                    )
                    .await;
                    return Ok(());
                }

                // Otherwise, end turn.
                self.set_active_state(ActiveState::Idle).await;
                self.agent_event_buf.push(AgentEvent::EndTurn(md));
                self.agent_event_buf.push(AgentEvent::Stop(AgentStopReason::EndTurn));
            },
            AgentLoopEventKind::AssistantText(text) => self
                .agent_event_buf
                .push(AgentEvent::Update(UpdateEvent::AgentContent(text.into()))),
            AgentLoopEventKind::ReasoningContent(text) => self
                .agent_event_buf
                .push(AgentEvent::Update(UpdateEvent::AgentThought(text.into()))),
            _ => (),
        }

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
                self.append_assistant_message(Message::new(Role::Assistant, assistant_content, Some(Utc::now())));

                self.append_user_message(vec![ContentBlock::Text(
                    "The generated tool was too large, try again but this time split up the work between multiple tool uses"
                        .to_string(),
                )]);

                let args = self.format_request().await;
                self.send_request(args).await?;
            },
            LoopError::Stream(stream_err) => match &stream_err.kind {
                StreamErrorKind::StreamTimeout { .. } => {
                    self.append_assistant_message(Message::new(
                        Role::Assistant,
                        vec![ContentBlock::Text(
                            "Response timed out - message took too long to generate".to_string(),
                        )],
                        Some(Utc::now()),
                    ));
                    self.append_user_message(vec![ContentBlock::Text(
                        "You took too long to respond - try to split up the work into smaller steps.".to_string(),
                    )]);

                    let args = self.format_request().await;
                    self.send_request(args).await?;
                },
                StreamErrorKind::Interrupted => {
                    // nothing to do
                },
                StreamErrorKind::ContextWindowOverflow if !self.settings.disable_auto_compact => {
                    self.start_compaction(CompactStrategy::default_strategy()).await?;
                },
                StreamErrorKind::Validation { .. }
                | StreamErrorKind::ServiceFailure
                | StreamErrorKind::ContextWindowOverflow
                | StreamErrorKind::Throttling
                | StreamErrorKind::Other(_) => {
                    self.set_active_state(ActiveState::Errored(err.clone().into())).await;
                    self.agent_event_buf
                        .push(AgentEvent::Stop(AgentStopReason::Error(err.clone().into())));
                },
            },
        }

        Ok(())
    }

    /// Handler for a [AgentRequest::SendPrompt] request.
    async fn handle_send_prompt(&mut self, args: SendPromptArgs) -> Result<AgentResponse, AgentError> {
        match self.active_state() {
            ActiveState::Idle => (),
            ActiveState::Errored(_) => {
                if !args.should_continue_turn() {
                    self.end_current_turn().await?;
                }
            },
            ActiveState::WaitingForApproval { .. } => (),
            ActiveState::ExecutingRequest
            | ActiveState::ExecutingHooks(_)
            | ActiveState::ExecutingTools { .. }
            | ActiveState::Compacting { .. } => {
                return Err(AgentError::NotIdle);
            },
        }

        // Run per-prompt hooks, if required.
        let hooks = self.get_hooks(HookTrigger::UserPromptSubmit);
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
                .await;
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
        let mut user_msg_content = args
            .content
            .into_iter()
            .map(|c| match c {
                ContentChunk::Text(t) => ContentBlock::Text(t),
                ContentChunk::Image(img) => ContentBlock::Image(img),
                ContentChunk::ResourceLink(json) => ContentBlock::Text(json),
            })
            .collect::<Vec<_>>();

        // Add per-prompt hooks, if required.
        for output in &prompt_hooks {
            user_msg_content.push(ContentBlock::Text(output.clone()));
        }

        self.append_user_message(user_msg_content.clone());

        // Create a new agent loop, and send the request.
        let loop_id = AgentLoopId::new(self.id.clone());
        let cancel_token = CancellationToken::new();
        self.agent_loop = Some(AgentLoop::new(loop_id.clone(), cancel_token).spawn());
        let args = self.format_request().await;
        self.send_request(args)
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
    async fn format_request(&mut self) -> SendRequestArgs {
        let latest_summary = self.conversation_state.event_log().latest_summary().map(String::from);
        format_request(
            VecDeque::from(self.conversation_state.messages().to_vec()),
            self.make_tool_spec().await,
            &self.agent_config,
            self.agent_spawn_hooks.iter().map(|(_, c)| c),
            &self.sys_provider,
            latest_summary,
        )
        .await
    }

    async fn send_request(&mut self, request_args: SendRequestArgs) -> Result<AgentLoopResponse, AgentError> {
        debug!(?request_args, "sending request");
        let model = Arc::clone(&self.model);
        let res = self
            .agent_loop_handle()?
            .send_request(model, request_args.clone())
            .await?;
        self.agent_event_buf
            .push(AgentEvent::Internal(InternalEvent::RequestSent(request_args)));
        Ok(res)
    }

    /// Starts compaction of the conversation history.
    ///
    /// This can be triggered either:
    /// - Automatically when context window overflow occurs
    /// - Manually via `CompactConversation` request
    async fn start_compaction(&mut self, strategy: CompactStrategy) -> Result<(), AgentError> {
        debug!(?strategy, "starting compaction");

        if !matches!(self.active_state(), ActiveState::Compacting { .. }) {
            self.agent_event_buf
                .push(AgentEvent::Compaction(CompactionEvent::Started));
        }

        let latest_summary = self.conversation_state.event_log().latest_summary().map(String::from);
        let compaction_request = create_compaction_request(
            self.conversation_state.messages(),
            &strategy,
            self.model.context_window_size(),
            None::<String>,
            latest_summary.as_deref(),
        );

        // Spawn a new agent loop specifically for compaction
        let loop_id = AgentLoopId::new(self.id.clone());
        let cancel_token = CancellationToken::new();
        let mut compaction_handle = AgentLoop::new(loop_id, cancel_token).spawn();

        let model = Arc::clone(&self.model);
        compaction_handle
            .send_request(model, compaction_request.clone())
            .await?;

        self.agent_event_buf
            .push(AgentEvent::Internal(InternalEvent::RequestSent(compaction_request)));

        self.compaction_loop = Some(compaction_handle);
        self.set_active_state(ActiveState::Compacting { strategy }).await;
        Ok(())
    }

    /// Handles events from the compaction agent loop.
    async fn handle_compaction_loop_event(&mut self, evt: Option<AgentLoopEventKind>) -> Result<(), AgentError> {
        debug!(?evt, "handling compaction loop event");

        let Some(evt) = evt else {
            self.compaction_loop = None;
            return Ok(());
        };

        let ActiveState::Compacting { strategy } = self.execution_state.active_state else {
            return Err(AgentError::Custom("Not in compacting state".to_string()));
        };

        if let AgentLoopEventKind::ResponseStreamEnd { result, metadata } = evt {
            match result {
                Ok(msg) => {
                    // Compaction should not produce tool uses
                    if !metadata.tool_uses.is_empty() {
                        return Err(AgentError::Custom(
                            "Compaction response unexpectedly contained tool uses".to_string(),
                        ));
                    }

                    // Finalize compaction and get the log entry
                    let context_window_size = self.model.context_window_size();
                    let (entry, index) =
                        compact::finalize_compaction(&mut self.conversation_state, msg, &strategy, context_window_size);

                    info!("compaction completed successfully");
                    self.compaction_loop = None;
                    self.agent_event_buf.push(AgentEvent::LogEntryAppended { entry, index });
                    self.agent_event_buf
                        .push(AgentEvent::Compaction(CompactionEvent::Completed));

                    // Retry if last message is from user, otherwise go idle
                    if self
                        .conversation_state
                        .messages()
                        .last()
                        .is_some_and(|m| m.role == Role::User)
                    {
                        debug!("last message is from the user, retrying the request");
                        let pending_request = self.format_request().await;
                        self.set_active_state(ActiveState::ExecutingRequest).await;
                        self.send_request(pending_request).await?;
                    } else {
                        debug!("last message is not from the user, going to idle state");
                        self.set_active_state(ActiveState::Idle).await;
                    }
                },
                Err(err) => {
                    self.compaction_loop = None;

                    // Retry with aggressive strategy if context overflow and not already truncating
                    let is_context_overflow = matches!(
                        &err,
                        LoopError::Stream(stream_err) if matches!(stream_err.kind, StreamErrorKind::ContextWindowOverflow)
                    );

                    if is_context_overflow && !strategy.truncate_large_messages {
                        debug!("compaction failed due to context overflow, retrying with aggressive strategy");
                        self.start_compaction(CompactStrategy::aggressive_strategy()).await?;
                    } else {
                        self.agent_event_buf
                            .push(AgentEvent::Compaction(CompactionEvent::Failed {
                                error: err.to_string(),
                            }));
                        self.set_active_state(ActiveState::Errored(err.clone().into())).await;
                        self.agent_event_buf
                            .push(AgentEvent::Stop(AgentStopReason::Error(err.into())));
                    }
                },
            }
        }

        Ok(())
    }

    /// Entrypoint for handling tool uses returned by the model.
    ///
    /// The process for handling tool uses follows the pipeline:
    /// 1. *Parse tools* - If any fail parsing, return errors back to the model.
    /// 2. *Evaluate permissions* - If any are denied, return the denied reasons back to the model.
    /// 3. *Run preToolUse hooks, if any* - If a hook rejects a tool use, return back to the model.
    /// 4. *Request approvals, if required* - If a tool use is denied by the user, return back to
    ///    the model.
    /// 5. *Execute tools*
    async fn handle_tool_uses(&mut self, tool_uses: Vec<ToolUseBlock>) -> Result<(), AgentError> {
        trace!(?tool_uses, "handling tool uses");
        debug_assert!(matches!(self.active_state(), ActiveState::ExecutingRequest));

        // First, parse tool uses.
        let (tools, errors) = self.parse_tools(tool_uses).await;
        if !errors.is_empty() {
            // Send parse errors back to the model.
            trace!(?errors, "failed to parse tools");
            let mut content = Vec::new();
            let mut results = HashMap::new();
            for e in errors {
                let tool_use_id = e.tool_use.tool_use_id.clone();
                let err_msg = e.to_string();
                content.push(ContentBlock::ToolResult(ToolResultBlock {
                    tool_use_id: tool_use_id.clone(),
                    content: vec![ToolResultContentBlock::Text(err_msg.clone())],
                    status: ToolResultStatus::Error,
                }));
                results.insert(tool_use_id, LogToolResult {
                    tool: None,
                    result: ToolCallResult::Error(ToolExecutionError::Custom(err_msg)),
                });
            }
            self.append_tool_results(content, results);
            let args = self.format_request().await;
            self.send_request(args).await?;
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
            self.agent_event_buf
                .push(AgentEvent::Internal(InternalEvent::ToolPermissionEvalResult {
                    tool: tool.clone(),
                    result,
                }));
        }

        // Return denied tools immediately back to the model
        if !denied.is_empty() {
            let mut content = Vec::new();
            let mut results = HashMap::new();
            for (block, tool, _) in denied {
                let err_msg = "Tool use was rejected because the arguments supplied are forbidden:".to_string();
                content.push(ContentBlock::ToolResult(ToolResultBlock {
                    tool_use_id: block.tool_use_id.clone(),
                    content: vec![ToolResultContentBlock::Text(err_msg.clone())],
                    status: ToolResultStatus::Error,
                }));
                results.insert(block.tool_use_id.clone(), LogToolResult {
                    tool: Some(Box::new(tool.clone())),
                    result: ToolCallResult::Error(ToolExecutionError::Custom(err_msg)),
                });
            }
            self.append_tool_results(content, results);
            let args = self.format_request().await;
            self.send_request(args).await?;
            return Ok(());
        }

        // Process PreToolUse hooks, if any.
        let hooks = self.get_hooks(HookTrigger::PreToolUse);
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
            self.start_hooks_execution(hooks_to_execute, stage, None).await;
            return Ok(());
        }

        self.process_tool_uses(tools, needs_approval).await
    }

    /// Processes successfully parsed tool uses, requesting permission if required, and then
    /// executing.
    async fn process_tool_uses(
        &mut self,
        tools: Vec<(ToolUseBlock, Tool)>,
        needs_approval: Vec<String>,
    ) -> Result<(), AgentError> {
        for tool in &tools {
            self.agent_event_buf.push(
                ToolCall {
                    id: tool.0.tool_use_id.clone(),
                    tool: tool.1.clone(),
                    tool_use_block: tool.0.clone(),
                }
                .into(),
            );
        }

        // request permission for any asked tools
        if !needs_approval.is_empty() {
            self.request_tool_approvals(tools, needs_approval).await?;
            return Ok(());
        }

        self.execute_tools(tools).await
    }

    async fn start_hooks_execution(
        &mut self,
        hooks: Vec<(HookExecutionId, Option<(ToolUseBlock, Tool)>)>,
        stage: HookStage,
        prompt: Option<String>,
    ) {
        let mut hooks_state = Vec::new();
        for (id, tool_ctx) in hooks {
            let req = StartHookExecution {
                id: id.clone(),
                prompt: prompt.clone(),
            };
            hooks_state.push(ExecutingHook {
                id: id.clone(),
                tool_use_block: tool_ctx.as_ref().map(|ctx| ctx.0.clone()),
                tool: tool_ctx.map(|ctx| ctx.1),
                result: None,
            });
            self.task_executor.start_hook_execution(req).await;
        }
        self.set_active_state(ActiveState::ExecutingHooks(ExecutingHooks {
            hooks: hooks_state,
            stage,
        }))
        .await;
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
        let ActiveState::ExecutingTools(executing_tools) = &mut self.execution_state.active_state else {
            warn!(
                ?self.execution_state,
                ?evt,
                "received a tool execution event for an agent not processing tools"
            );
            return Ok(());
        };

        debug_assert!(executing_tools.get_tool(&evt.id).is_some());
        if let Some(tool) = executing_tools.get_tool_mut(&evt.id) {
            tool.result = Some(evt.result.clone());

            // Emit ToolCallFinished event for the completed tool
            let tool_call = ToolCall {
                id: tool.tool_use_block.tool_use_id.clone(),
                tool: tool.tool.clone(),
                tool_use_block: tool.tool_use_block.clone(),
            };

            let result = match &evt.result {
                ToolExecutorResult::Completed { result: Ok(output), .. } => ToolCallResult::Success(output.clone()),
                ToolExecutorResult::Completed { result: Err(error), .. } => ToolCallResult::Error(error.clone()),
                ToolExecutorResult::Cancelled { .. } => ToolCallResult::Cancelled,
            };

            self.agent_event_buf
                .push(AgentEvent::Update(UpdateEvent::ToolCallFinished { tool_call, result }));
        }

        if !executing_tools.all_tools_finished() {
            return Ok(());
        }

        // Clone to bypass borrow checker
        let executing_tools = executing_tools.clone();

        // Process PostToolUse hooks, if any.
        let hooks = self.get_hooks(HookTrigger::PostToolUse);
        let mut hooks_to_execute = Vec::new();
        for executing_tool in executing_tools.tools() {
            let Some(result) = executing_tool.result.as_ref() else {
                continue;
            };
            let Some(output) = result.tool_execution_output() else {
                continue;
            };
            let Ok(output) = serde_json::to_value(output) else {
                continue;
            };
            hooks_to_execute.extend(
                hooks
                    .iter()
                    .filter(|h| hook_matches_tool(&h.config, &executing_tool.tool))
                    .map(|h| {
                        (
                            HookExecutionId {
                                hook: h.clone(),
                                tool_context: Some(
                                    (&executing_tool.tool_use_block, &executing_tool.tool, &output).into(),
                                ),
                            },
                            Some((executing_tool.tool_use_block.clone(), executing_tool.tool.clone())),
                        )
                    }),
            );
        }
        if !hooks_to_execute.is_empty() {
            debug!("found hooks to execute for postToolUse");
            let stage = HookStage::PostToolUse {
                executing_tools: executing_tools.clone(),
            };
            self.start_hooks_execution(hooks_to_execute, stage, None).await;
            return Ok(());
        }

        // All tools have finished executing, so send the results back to the model.
        self.send_tool_results(&executing_tools).await?;
        Ok(())
    }

    async fn handle_hook_finished_event(&mut self, id: HookExecutionId, result: HookResult) -> Result<(), AgentError> {
        let ActiveState::ExecutingHooks(executing_hooks) = &mut self.execution_state.active_state else {
            warn!(
                ?self.execution_state,
                ?id,
                "received a hook execution event while not executing hooks"
            );
            return Ok(());
        };

        debug_assert!(executing_hooks.get_hook(&id).is_some());
        if let Some(hook) = executing_hooks.get_hook_mut(&id) {
            hook.result = Some(result.clone());
        }

        // Cache the hook if it's a successful agent spawn hook.
        if result.is_success()
            && id.hook.trigger == HookTrigger::AgentSpawn
            && !self.agent_spawn_hooks.iter().any(|v| v.0 == id.hook.config)
            && let Some(output) = result.output()
        {
            self.agent_spawn_hooks
                .push((id.hook.config.clone(), output.to_string()));
        }

        if !executing_hooks.all_hooks_finished() {
            return Ok(());
        }

        // All hooks have finished executing, so proceed to the next stage.
        match &executing_hooks.stage {
            HookStage::AgentSpawn => {
                self.set_active_state(ActiveState::Idle).await;
                self.agent_event_buf.push(AgentEvent::Initialized);
                Ok(())
            },
            HookStage::PrePrompt { args } => {
                let args = args.clone(); // borrow checker clone
                let hooks = executing_hooks.per_prompt_hooks();
                self.send_prompt_impl(args, hooks).await?;
                Ok(())
            },
            HookStage::PreToolUse { tools, needs_approval } => {
                // If any command hooks exited with status 2, then we'll block.
                // Otherwise, execute the tools.
                let mut denied_tools = Vec::new();
                for (block, tool) in tools {
                    if let Some(hook) = executing_hooks.has_failure_exit_code_for_tool(&block.tool_use_id) {
                        denied_tools.push((
                            block.tool_use_id.clone(),
                            tool.clone(),
                            hook.result.as_ref().cloned().expect("is some"),
                        ));
                    }
                }
                if !denied_tools.is_empty() {
                    // Send denied tool results back to the model.
                    let mut content = Vec::new();
                    let mut results = HashMap::new();
                    for (tool_use_id, tool, hook_res) in denied_tools {
                        let err_msg = format!(
                            "PreToolHook blocked the tool execution: {}",
                            hook_res.output().unwrap_or("no output provided")
                        );
                        content.push(ContentBlock::ToolResult(ToolResultBlock {
                            tool_use_id: tool_use_id.clone(),
                            content: vec![ToolResultContentBlock::Text(err_msg.clone())],
                            status: ToolResultStatus::Error,
                        }));
                        results.insert(tool_use_id, LogToolResult {
                            tool: Some(Box::new(tool)),
                            result: ToolCallResult::Error(ToolExecutionError::Custom(err_msg)),
                        });
                    }
                    self.append_tool_results(content, results);
                    let args = self.format_request().await;
                    self.send_request(args).await?;
                    return Ok(());
                }

                // Otherwise, continue to the approval stage.
                let tools = tools.clone();
                let needs_approval = needs_approval.clone();
                Ok(self.process_tool_uses(tools, needs_approval).await?)
            },
            HookStage::PostToolUse { executing_tools } => {
                let executing_tools = executing_tools.clone();
                self.send_tool_results(&executing_tools).await?;
                Ok(())
            },
            HookStage::Stop { user_turn_metadata } => {
                self.agent_event_buf
                    .push(AgentEvent::EndTurn((**user_turn_metadata).clone()));
                self.agent_event_buf.push(AgentEvent::Stop(AgentStopReason::EndTurn));
                self.set_active_state(ActiveState::Idle).await;
                Ok(())
            },
        }
    }

    async fn make_tool_spec(&mut self) -> Vec<ToolSpec> {
        let tool_names = self.get_tool_names().await;
        let mut mcp_server_tool_specs = HashMap::new();
        for name in &tool_names {
            if let CanonicalToolName::Mcp { server_name, .. } = name
                && !mcp_server_tool_specs.contains_key(server_name)
            {
                let Ok(tools) = self.mcp_manager_handle.get_tool_specs(server_name.clone()).await else {
                    continue;
                };
                mcp_server_tool_specs.insert(server_name.clone(), tools);
            }
        }

        let lsp_initialized = self
            .code_intelligence
            .as_ref()
            .and_then(|c| c.try_read().ok())
            .is_some_and(|c| c.is_code_intelligence_initialized());

        let sanitized_specs = sanitize_tool_specs(
            tool_names,
            mcp_server_tool_specs,
            self.agent_config.tool_aliases(),
            lsp_initialized,
        );
        if !sanitized_specs.transformed_tool_specs().is_empty() {
            warn!(transformed_tool_spec = ?sanitized_specs.transformed_tool_specs(), "some tool specs were transformed");
        }
        if !sanitized_specs.filtered_specs().is_empty() {
            warn!(filtered_specs = ?sanitized_specs.filtered_specs(), "filtered some tool specs");
        }
        let mut tool_specs = sanitized_specs.tool_specs();
        add_tool_use_purpose_arg(&mut tool_specs);
        self.cached_tool_specs = Some(sanitized_specs);
        tool_specs
    }

    /// Returns the name of all tools available to the given agent.
    ///
    /// The tools available to the agent may change overtime, for example:
    /// * MCP servers loading or exiting
    /// * MCP tool spec changes
    /// * Actor messages that update the agent's config
    ///
    /// This function ensures that we create a list of known tool names to be available
    /// for the agent's current state.
    async fn get_tool_names(&self) -> Vec<CanonicalToolName> {
        let mut tool_names = {
            let mut tool_names = HashSet::new();

            if self.is_subagent {
                tool_names.insert(CanonicalToolName::BuiltIn(tools::BuiltInToolName::Summary));
            }

            tool_names
        };

        let built_in_tool_names = {
            let mut names = built_in_tool_names();

            // For the time being, subagent is not yet fully implemented in the agent crate so
            // we'll remove it from the tool list.
            // We'll also remove summary tool from the tool list since that is added above in the
            // initialization of tool_names
            names.retain(|name| {
                name != &CanonicalToolName::BuiltIn(tools::BuiltInToolName::SpawnSubagent)
                    && name != &CanonicalToolName::BuiltIn(tools::BuiltInToolName::Summary)
            });

            names
        };

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

                        for config in &self.cached_mcp_configs.configs {
                            let Ok(specs) = self.mcp_manager_handle.get_tool_specs(config.server_name.clone()).await
                            else {
                                continue;
                            };
                            for spec in specs {
                                tool_names
                                    .insert(CanonicalToolName::from_mcp_parts(config.server_name.clone(), spec.name));
                            }
                        }
                    },
                    ToolNameKind::McpFullName { .. } => {
                        if let Ok(tn) = tool_name.parse() {
                            tool_names.insert(tn);
                        }
                    },
                    ToolNameKind::McpServer { server_name } => {
                        // get all tools from the mcp server
                        let Ok(specs) = self.mcp_manager_handle.get_tool_specs(server_name.to_string()).await else {
                            continue;
                        };
                        for spec in specs {
                            tool_names.insert(CanonicalToolName::from_mcp_parts(server_name.to_string(), spec.name));
                        }
                    },
                    ToolNameKind::McpGlob { server_name, glob_part } => {
                        // match only tools for the server name
                        let Ok(specs) = self.mcp_manager_handle.get_tool_specs(server_name.to_string()).await else {
                            continue;
                        };
                        for spec in specs {
                            if matches_any_pattern([glob_part], &spec.name) {
                                tool_names
                                    .insert(CanonicalToolName::from_mcp_parts(server_name.to_string(), spec.name));
                            }
                        }
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

        tool_names.into_iter().collect()
    }

    /// Parses tool use blocks into concrete tools, returning those that failed to be parsed.
    async fn parse_tools(&mut self, tool_uses: Vec<ToolUseBlock>) -> (Vec<(ToolUseBlock, Tool)>, Vec<ToolParseError>) {
        let mut tools: Vec<(ToolUseBlock, Tool)> = Vec::new();
        let mut parse_errors: Vec<ToolParseError> = Vec::new();

        for tool_use in tool_uses {
            let canonical_tool_name = match &self.cached_tool_specs {
                Some(specs) => match specs.tool_map().get(&tool_use.name) {
                    Some(spec) => spec.canonical_name().clone(),
                    None => {
                        parse_errors.push(ToolParseError::new(
                            tool_use.clone(),
                            ToolParseErrorKind::NameDoesNotExist(tool_use.name),
                        ));
                        continue;
                    },
                },
                None => {
                    // should never happen
                    debug_assert!(false, "parsing tools without having cached tool specs");
                    continue;
                },
            };
            let mut tool = match Tool::parse(&canonical_tool_name, tool_use.input.clone()) {
                Ok(t) => t,
                Err(err) => {
                    parse_errors.push(ToolParseError::new(tool_use, err));
                    continue;
                },
            };
            match self.validate_tool(&mut tool).await {
                Ok(_) => tools.push((tool_use, tool)),
                Err(err) => {
                    parse_errors.push(ToolParseError::new(tool_use, err));
                },
            }
        }

        (tools, parse_errors)
    }

    async fn validate_tool(&self, tool: &mut Tool) -> Result<(), ToolParseErrorKind> {
        match &mut tool.kind {
            ToolKind::BuiltIn(built_in) => match built_in {
                BuiltInTool::FileRead(t) => t
                    .validate(&self.sys_provider)
                    .await
                    .map_err(ToolParseErrorKind::invalid_args),
                BuiltInTool::FileWrite(t) => t
                    .validate(&self.sys_provider)
                    .await
                    .map_err(ToolParseErrorKind::invalid_args),
                BuiltInTool::Grep(t) => t
                    .validate(&self.sys_provider)
                    .await
                    .map_err(ToolParseErrorKind::invalid_args),
                BuiltInTool::Glob(t) => t
                    .validate(&self.sys_provider)
                    .await
                    .map_err(ToolParseErrorKind::invalid_args),
                BuiltInTool::Ls(t) => t
                    .validate(&self.sys_provider)
                    .await
                    .map_err(ToolParseErrorKind::invalid_args),
                BuiltInTool::Mkdir(_) => Ok(()),
                BuiltInTool::ExecuteCmd(_) => Ok(()),
                BuiltInTool::Introspect(_) => Ok(()),
                BuiltInTool::Summary(_) => Ok(()),
                BuiltInTool::SpawnSubagent(_) => Ok(()),
                BuiltInTool::ImageRead(t) => t.validate().await.map_err(ToolParseErrorKind::invalid_args),
                BuiltInTool::UseAws(t) => t.validate().await.map_err(ToolParseErrorKind::invalid_args),
                BuiltInTool::WebFetch(_) => Ok(()),
                BuiltInTool::WebSearch(_) => Ok(()),
                BuiltInTool::Code(t) => t
                    .validate(&self.sys_provider)
                    .await
                    .map_err(ToolParseErrorKind::invalid_args),
            },
            ToolKind::Mcp(_) => Ok(()),
        }
    }

    async fn evaluate_tool_permission(&mut self, tool: &Tool) -> Result<PermissionEvalResult, AgentError> {
        match evaluate_tool_permission(
            &self.permissions,
            self.agent_config.allowed_tools(),
            &self.agent_config.tool_settings().cloned().unwrap_or_default(),
            tool.kind(),
            &self.sys_provider,
        ) {
            Ok(res) => Ok(res),
            Err(err) => {
                warn!(?err, "failed to evaluate tool permission");
                Ok(PermissionEvalResult::Ask)
            },
        }
    }

    async fn request_tool_approvals(
        &mut self,
        tools: Vec<(ToolUseBlock, Tool)>,
        needs_approval: Vec<String>,
    ) -> Result<(), AgentError> {
        // First, update the agent state to WaitingForApproval
        let mut needs_approval_map = HashMap::new();
        for tool_use_id in &needs_approval {
            let Some((_, tool)) = tools.iter().find(|(b, _)| &b.tool_use_id == tool_use_id) else {
                warn!(tool_use_id, "tool requiring approval not found in tools list");
                continue;
            };
            needs_approval_map.insert(tool_use_id.clone(), ApprovalState {
                options: tool.permission_options(),
                selected: None,
            });
        }
        self.set_active_state(ActiveState::WaitingForApproval(WaitingForApproval {
            tools: tools.clone(),
            needs_approval: needs_approval_map,
        }))
        .await;

        // Send notifications for each tool that requires approval
        for tool_use_id in &needs_approval {
            let Some((block, tool)) = tools.iter().find(|(b, _)| &b.tool_use_id == tool_use_id) else {
                continue;
            };
            self.agent_event_buf
                .push(AgentEvent::ApprovalRequest(protocol::ApprovalRequest {
                    id: block.tool_use_id.clone(),
                    tool_use: (*block).clone(),
                    tool: tool.clone(),
                    context: tool.get_context().await,
                    options: tool.permission_options(),
                }));
        }

        Ok(())
    }

    async fn execute_tools(&mut self, tools: Vec<(ToolUseBlock, Tool)>) -> Result<(), AgentError> {
        let mut tool_state = Vec::new();
        for (block, tool) in tools {
            let id = ToolExecutionId::new(block.tool_use_id.clone());
            tool_state.push(ExecutingTool {
                id: id.clone(),
                tool_use_block: block.clone(),
                tool: tool.clone(),
                result: None,
            });
            self.start_tool_execution(id.clone(), tool).await?;
        }
        self.set_active_state(ActiveState::ExecutingTools(ExecutingTools(tool_state)))
            .await;
        Ok(())
    }

    /// Starts executing a tool for the given agent. Tools are executed in parallel on a background
    /// task.
    async fn start_tool_execution(&mut self, id: ToolExecutionId, tool: Tool) -> Result<(), AgentError> {
        trace!(?id, ?tool, "starting tool execution");
        let tool_clone = tool.clone();

        // Channel for handling tool-specific state updates.
        let (tx, rx) = oneshot::channel::<ToolState>();

        let provider = Arc::clone(&self.sys_provider);

        let fut: ToolFuture = match tool.kind {
            ToolKind::BuiltIn(builtin) => match builtin {
                BuiltInTool::FileRead(t) => Box::pin(async move { t.execute(&provider).await }),
                BuiltInTool::FileWrite(t) => {
                    let file_write = self.tool_state.file_write.clone();
                    let mut tool_state = ToolState { file_write };
                    Box::pin(async move {
                        let res = t.execute(tool_state.file_write.as_mut(), &provider).await;
                        if res.is_ok() {
                            let _ = tx.send(tool_state);
                        }
                        res
                    })
                },
                BuiltInTool::ExecuteCmd(t) => Box::pin(async move { t.execute(&provider).await }),
                BuiltInTool::ImageRead(t) => Box::pin(async move { t.execute().await }),
                BuiltInTool::Introspect(_) => panic!("unimplemented"),
                BuiltInTool::Grep(t) => Box::pin(async move { t.execute(&provider).await }),
                BuiltInTool::Glob(t) => Box::pin(async move { t.execute(&provider).await }),
                BuiltInTool::Ls(t) => Box::pin(async move { t.execute(&provider).await }),
                BuiltInTool::Mkdir(_) => panic!("unimplemented"),
                BuiltInTool::SpawnSubagent(t) => {
                    let event_tx = self.agent_event_tx.clone();
                    Box::pin(async move { t.execute(event_tx).await })
                },
                BuiltInTool::Summary(t) => {
                    let result_tx = self.agent_event_tx.clone();
                    Box::pin(async move { t.execute(result_tx).await })
                },
                BuiltInTool::UseAws(t) => Box::pin(async move { t.execute().await }),
                BuiltInTool::WebFetch(t) => Box::pin(async move { t.execute().await }),
                BuiltInTool::WebSearch(t) => {
                    let model = Arc::clone(&self.model);
                    Box::pin(async move { t.execute(&*model).await })
                },
                BuiltInTool::Code(t) => {
                    let code_intel = self.code_intelligence.clone();
                    Box::pin(async move {
                        match code_intel {
                            Some(ci) => t.execute(&ci, &*provider).await,
                            None => Err(ToolExecutionError::Custom(
                                "Code intelligence not available. Run '/code init' to initialize.".to_string(),
                            )),
                        }
                    })
                },
            },
            ToolKind::Mcp(t) => {
                let mcp_tool = t.clone();
                let rx = self
                    .mcp_manager_handle
                    .execute_tool(t.server_name, t.tool_name, t.params)
                    .await?;
                Box::pin(async move {
                    let Ok(res) = rx.await else {
                        return Err(ToolExecutionError::Custom("channel dropped".to_string()));
                    };
                    match res {
                        Ok(resp) => {
                            if resp.is_error.is_none_or(|v| !v) {
                                Ok(ToolExecutionOutput::new(vec![ToolExecutionOutputItem::Json(
                                    serde_json::json!(resp),
                                )]))
                            } else {
                                warn!(?mcp_tool, "Tool call failed");
                                Ok(ToolExecutionOutput::new(vec![ToolExecutionOutputItem::Json(
                                    serde_json::json!(resp),
                                )]))
                            }
                        },
                        Err(err) => Err(ToolExecutionError::Custom(format!(
                            "failed to send call tool request to the MCP server: {err}"
                        ))),
                    }
                })
            },
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

    async fn send_tool_results(&mut self, executing_tools: &ExecutingTools) -> Result<(), AgentError> {
        let mut content = Vec::new();
        let mut results = HashMap::new();

        for executing_tool in executing_tools.tools() {
            debug_assert!(executing_tool.result.is_some(), "tool result must be Some");
            let Some(result) = &executing_tool.result else {
                continue;
            };
            let tool_use_id = executing_tool.tool_use_block.tool_use_id.clone();

            match result {
                ToolExecutorResult::Completed { result, .. } => match result {
                    Ok(res) => {
                        let mut content_items = Vec::new();
                        for item in &res.items {
                            let content_item = match item {
                                ToolExecutionOutputItem::Text(s) => ToolResultContentBlock::Text(s.clone()),
                                ToolExecutionOutputItem::Json(v) => ToolResultContentBlock::Json(v.clone()),
                                ToolExecutionOutputItem::Image(i) => ToolResultContentBlock::Image(i.clone()),
                            };
                            content_items.push(content_item);
                        }
                        content.push(ContentBlock::ToolResult(ToolResultBlock {
                            tool_use_id: tool_use_id.clone(),
                            content: content_items,
                            status: ToolResultStatus::Success,
                        }));
                        results.insert(tool_use_id, LogToolResult {
                            tool: Some(Box::new(executing_tool.tool.clone())),
                            result: ToolCallResult::Success(res.clone()),
                        });
                    },
                    Err(err) => {
                        content.push(ContentBlock::ToolResult(ToolResultBlock {
                            tool_use_id: tool_use_id.clone(),
                            content: vec![ToolResultContentBlock::Text(err.to_string())],
                            status: ToolResultStatus::Error,
                        }));
                        results.insert(tool_use_id, LogToolResult {
                            tool: Some(Box::new(executing_tool.tool.clone())),
                            result: ToolCallResult::Error(err.clone()),
                        });
                    },
                },
                ToolExecutorResult::Cancelled { .. } => {
                    // Should never happen in this flow
                },
            }
        }

        self.append_tool_results(content, results);
        let args = self.format_request().await;
        self.send_request(args).await?;
        self.set_active_state(ActiveState::ExecutingRequest).await;
        Ok(())
    }

    async fn handle_mcp_events(&mut self, evt: McpServerEvent) {
        let converted_evt = AgentEvent::Mcp(evt);
        self.agent_event_buf.push(converted_evt);
    }

    /// This prepends the embedded user msg to the system prompt field of the agent
    pub fn prepend_embedded_user_msg(&mut self, msg: &str) {
        self.agent_config.prepend_to_system_prompt(msg);
    }

    /// This appends the embedded user msg to the system prompt field of the agent
    pub fn append_embedded_user_msg(&mut self, msg: &str) {
        self.agent_config.append_to_system_prompt(msg);
    }

    /// Append a user message to the conversation and emit the log event.
    fn append_user_message(&mut self, content: Vec<ContentBlock>) {
        let entry = LogEntry::prompt(Uuid::new_v4().to_string(), content);
        let index = self.conversation_state.append_log(entry.clone());
        self.agent_event_buf.push(AgentEvent::LogEntryAppended { entry, index });
    }

    /// Append tool results to the conversation and emit the log event.
    fn append_tool_results(&mut self, content: Vec<ContentBlock>, results: HashMap<String, LogToolResult>) {
        let entry = LogEntry::tool_results(Uuid::new_v4().to_string(), content, results);
        let index = self.conversation_state.append_log(entry.clone());
        self.agent_event_buf.push(AgentEvent::LogEntryAppended { entry, index });
    }

    /// Append an assistant message to the conversation and emit the log event.
    fn append_assistant_message(&mut self, msg: Message) {
        let message_id = msg.id.clone().unwrap_or_else(|| Uuid::new_v4().to_string());
        let entry = LogEntry::assistant_message(message_id, msg.content);
        let index = self.conversation_state.append_log(entry.clone());
        self.agent_event_buf.push(AgentEvent::LogEntryAppended { entry, index });
    }
}

/// Creates a request structure for sending to the model.
///
/// Internally, this function will:
/// 1. Create context messages according to what is configured in the agent config and agent spawn
///    hook content.
/// 2. Modify the message history to align with conversation invariants enforced by the backend.
async fn format_request<T, U, P>(
    mut messages: VecDeque<Message>,
    mut tool_spec: Vec<ToolSpec>,
    agent_config: &AgentConfig,
    agent_spawn_hooks: T,
    provider: &P,
    latest_summary: Option<String>,
) -> SendRequestArgs
where
    T: IntoIterator<Item = U>,
    U: AsRef<str>,
    P: SystemProvider,
{
    enforce_conversation_invariants(&mut messages, &mut tool_spec);

    let ctx_messages = create_context_messages(agent_config, agent_spawn_hooks, latest_summary, provider).await;
    for msg in ctx_messages.into_iter().rev() {
        messages.push_front(msg);
    }

    SendRequestArgs::new(
        messages.into(),
        if tool_spec.is_empty() { None } else { Some(tool_spec) },
        agent_config.system_prompt().map(String::from),
    )
}

/// Creates context messages using the provided arguments.
///
/// # Background
///
/// **Context messages** are fake user/assistant messages inserted at the beginning of a
/// conversation that contains global context (think: content that would otherwise go in the system
/// prompt).
///
/// The content included in these messages includes:
/// * Resources from the agent config
/// * The `prompt` field from the agent config
/// * Conversation start hooks
/// * Latest conversation summary from compaction
///
/// We use context messages since the API does not allow any system prompt parameterization.
async fn create_context_messages<T, U, P>(
    agent_config: &AgentConfig,
    agent_spawn_hooks: T,
    latest_summary: Option<String>,
    provider: &P,
) -> Vec<Message>
where
    T: IntoIterator<Item = U>,
    U: AsRef<str>,
    P: SystemProvider,
{
    let system_prompt = agent_config.system_prompt();
    let (files, skills) = collect_resources(agent_config.resources(), provider).await;

    let content = format_user_context_message(
        system_prompt,
        files.iter().map(|r| &r.content),
        skills.iter().map(|r| &r.content),
        agent_spawn_hooks,
        latest_summary,
    );
    if content.is_empty() {
        return vec![];
    }
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

fn format_user_context_message<T, U, S, V, W, X>(
    system_prompt: Option<&str>,
    files: T,
    skills: W,
    agent_spawn_hooks: U,
    latest_summary: Option<String>,
) -> String
where
    T: IntoIterator<Item = S>,
    U: IntoIterator<Item = V>,
    W: IntoIterator<Item = X>,
    S: AsRef<str>,
    V: AsRef<str>,
    X: AsRef<str>,
{
    let mut context_content = String::new();

    if let Some(summary) = latest_summary {
        context_content.push_str(CONTEXT_ENTRY_START_HEADER);
        context_content.push_str("This summary contains ALL relevant information from our previous conversation including tool uses, results, code analysis, and file operations. YOU MUST reference this information when answering questions and explicitly acknowledge specific details from the summary when they're relevant to the current question.\n\nSUMMARY CONTENT:\n");
        context_content.push_str(&summary);
        context_content.push('\n');
        context_content.push_str(CONTEXT_ENTRY_END_HEADER);
    }

    if let Some(prompt) = system_prompt {
        context_content.push_str(&format!("Follow this instruction: {prompt}"));
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

    for file in files {
        let content = file.as_ref();
        context_content.push_str(CONTEXT_ENTRY_START_HEADER);
        context_content.push_str(content);
        context_content.push_str("\n\n");
        context_content.push_str(CONTEXT_ENTRY_END_HEADER);
    }

    // Skills block - all skills grouped together with instruction
    let skills: Vec<_> = skills.into_iter().collect();
    if !skills.is_empty() {
        context_content.push_str(CONTEXT_ENTRY_START_HEADER);
        context_content.push_str(SKILL_FILES_MESSAGE);
        for skill in skills {
            context_content.push_str(skill.as_ref());
            context_content.push('\n');
        }
        context_content.push_str(CONTEXT_ENTRY_END_HEADER);
    }

    context_content
}

/// Violations of conversation history invariants.
#[derive(Debug, Default)]
pub struct ConversationInvariantViolations {
    /// First message is not a User message without tool results.
    pub invalid_first_message: bool,
    /// Indices of User messages not followed by an Assistant message (excludes last message).
    pub user_not_followed_by_assistant: Vec<usize>,
    /// Indices of Assistant messages not followed by a User message (excludes last message).
    pub assistant_not_followed_by_user: Vec<usize>,
    /// (message_index, tool_use_id) for tool results without corresponding tool use in preceding
    /// assistant.
    pub orphaned_tool_results: Vec<(usize, String)>,
    /// (message_index, tool_use_id) for tool uses without corresponding tool result in following
    /// user.
    pub missing_tool_results: Vec<(usize, String)>,
}

impl ConversationInvariantViolations {
    pub fn is_valid(&self) -> bool {
        !self.invalid_first_message
            && self.user_not_followed_by_assistant.is_empty()
            && self.assistant_not_followed_by_user.is_empty()
            && self.orphaned_tool_results.is_empty()
            && self.missing_tool_results.is_empty()
    }
}

/// Detects conversation history invariant violations without modifying the messages.
pub fn detect_invariant_violations(messages: &[Message]) -> ConversationInvariantViolations {
    let mut violations = ConversationInvariantViolations::default();

    if messages.is_empty() {
        return violations;
    }

    // Check first message is User without tool results
    if messages[0].role != Role::User || messages[0].tool_results().is_some() {
        violations.invalid_first_message = true;
    }

    // Check orphaned tool results in first message (no preceding assistant)
    for tool_result in messages[0].tool_results_iter() {
        violations
            .orphaned_tool_results
            .push((0, tool_result.tool_use_id.clone()));
    }

    // Check consecutive message pairs
    for (i, pair) in messages.windows(2).enumerate() {
        let curr = &pair[0];
        let next = &pair[1];

        match curr.role {
            Role::User => {
                if next.role != Role::Assistant {
                    violations.user_not_followed_by_assistant.push(i);
                }
            },
            Role::Assistant => {
                if next.role != Role::User {
                    violations.assistant_not_followed_by_user.push(i);
                } else {
                    // Check tool use/result pairing
                    for tool_result in next.tool_results_iter() {
                        if curr.get_tool_use(&tool_result.tool_use_id).is_none() {
                            violations
                                .orphaned_tool_results
                                .push((i + 1, tool_result.tool_use_id.clone()));
                        }
                    }
                    for tool_use in curr.tool_uses_iter() {
                        if next.get_tool_result(&tool_use.tool_use_id).is_none() {
                            violations.missing_tool_results.push((i, tool_use.tool_use_id.clone()));
                        }
                    }
                }
            },
        }
    }

    violations
}

/// Updates the history so that, when non-empty, the following invariants are in place:
/// - The history length is `<= MAX_CONVERSATION_STATE_HISTORY_LEN`. Oldest messages are dropped.
/// - Any tool uses that do not exist in the provided tool specs will have their arguments replaced
///   with dummy content.
pub(super) fn enforce_conversation_invariants(messages: &mut VecDeque<Message>, tools: &mut Vec<ToolSpec>) {
    if messages.is_empty() {
        return;
    }

    // Trim the conversation history by finding the oldest message from the user without
    // tool results - this will be the new oldest message in the history.
    //
    // Note that we reserve extra slots for context messages.
    const MAX_HISTORY_LEN: usize = MAX_CONVERSATION_STATE_HISTORY_LEN - 2;
    if messages.len() > MAX_HISTORY_LEN {
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

    debug_assert!(messages.front().is_some_and(|msg| msg.role == Role::User));

    // For any user messages that have tool results but the preceding assistant message has no tool
    // uses, replace the tool result content as normal prompt content.
    for asst_user_pair in messages.make_contiguous()[1..].chunks_exact_mut(2) {
        let mut ids = Vec::new();
        for tool_result in asst_user_pair[1].tool_results_iter() {
            if asst_user_pair[0].get_tool_use(&tool_result.tool_use_id).is_none() {
                ids.push(tool_result.tool_use_id.clone());
            }
        }
        for id in ids {
            asst_user_pair[1].replace_tool_result_as_content(id);
        }
    }
    // Do the same as above but for the first message in the history.
    {
        let mut ids = Vec::new();
        for tool_result in messages[0].tool_results_iter() {
            ids.push(tool_result.tool_use_id.clone());
        }
        for id in ids {
            messages[0].replace_tool_result_as_content(id);
        }
    }

    // For user messages that follow a tool use but have no corresponding tool result, add
    // "cancelled" tool use results.
    for asst_user_pair in messages.make_contiguous()[1..].chunks_exact_mut(2) {
        let mut ids = Vec::new();
        for tool_use in asst_user_pair[0].tool_uses_iter() {
            if asst_user_pair[1].get_tool_result(&tool_use.tool_use_id).is_none() {
                ids.push(tool_use.tool_use_id.clone());
            }
        }
        for id in ids {
            asst_user_pair[1]
                .content
                .push(ContentBlock::ToolResult(ToolResultBlock {
                    tool_use_id: id,
                    content: vec![ToolResultContentBlock::Text(
                        "Tool use was cancelled by the user".to_string(),
                    )],
                    status: ToolResultStatus::Error,
                }));
        }
    }

    // Replace any missing tool use references with a dummy tool spec.
    let tool_names: HashSet<_> = tools.iter().map(|t| t.name.clone()).collect();
    let mut insert_dummy_spec = false;
    for msg in messages {
        for block in &mut msg.content {
            if let ContentBlock::ToolUse(v) = block
                && !tool_names.contains(&v.name)
            {
                v.name = DUMMY_TOOL_NAME.to_string();
                insert_dummy_spec = true;
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

#[derive(Debug, Clone)]
struct Resource {
    /// Exact value from the config this resource was taken from
    #[allow(dead_code)]
    config_value: String,
    /// Resource content
    content: String,
}

/// Extract YAML frontmatter from file content
fn extract_yaml_frontmatter(content: &str) -> Option<&str> {
    if !content.starts_with("---\n") {
        return None;
    }
    let rest = &content[4..];
    let end = rest.find("\n---")?;
    Some(&rest[..end])
}

/// Parse skill frontmatter and format as hint
fn format_skill_hint(file_path: &str, content: &str) -> Option<String> {
    let yaml = extract_yaml_frontmatter(content)?;

    // Simple parsing - look for name: and description: lines
    let mut name = None;
    let mut description = None;
    for line in yaml.lines() {
        if let Some(v) = line.strip_prefix("name:") {
            name = Some(v.trim());
        } else if let Some(v) = line.strip_prefix("description:") {
            description = Some(v.trim());
        }
    }

    let name = name.unwrap_or(file_path);
    let description = description.unwrap_or("No description available");
    Some(format!("{name}: {description} (file: {file_path})"))
}

/// Returns (files, skills) - files have full content, skills have metadata hints
async fn collect_resources<T, U, P>(resources: T, provider: &P) -> (Vec<Resource>, Vec<Resource>)
where
    T: IntoIterator<Item = U>,
    U: AsRef<str>,
    P: SystemProvider,
{
    use glob;

    let mut files = Vec::new();
    let mut skills = Vec::new();

    for resource in resources {
        let Ok(kind) = ResourceKind::parse(resource.as_ref(), provider) else {
            continue;
        };
        match kind {
            ResourceKind::File { original, file_path } => {
                let Ok(path) = canonicalize_path_sys(file_path, provider) else {
                    continue;
                };
                let Ok((content, _)) = read_file_with_max_limit(path, MAX_RESOURCE_FILE_LENGTH, "...truncated").await
                else {
                    continue;
                };
                files.push(Resource {
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
                        files.push(Resource {
                            config_value: original.to_string(),
                            content,
                        });
                    }
                }
            },
            ResourceKind::Skill { original, file_path } => {
                let Ok(path) = canonicalize_path_sys(&file_path, provider) else {
                    continue;
                };
                let Ok((content, _)) = read_file_with_max_limit(&path, MAX_RESOURCE_FILE_LENGTH, "...truncated").await
                else {
                    continue;
                };
                let hint = format_skill_hint(&file_path, &content).unwrap_or(content);
                skills.push(Resource {
                    config_value: original.to_string(),
                    content: hint,
                });
            },
            ResourceKind::SkillGlob { original, pattern } => {
                let Ok(entries) = glob::glob(pattern.as_str()) else {
                    continue;
                };
                for entry in entries {
                    let Ok(entry) = entry else {
                        continue;
                    };
                    if entry.is_file() {
                        let file_path = entry.to_string_lossy().to_string();
                        let Ok((content, _)) =
                            read_file_with_max_limit(entry.as_path(), MAX_RESOURCE_FILE_LENGTH, "...truncated").await
                        else {
                            continue;
                        };
                        let hint = format_skill_hint(&file_path, &content).unwrap_or(content);
                        skills.push(Resource {
                            config_value: original.to_string(),
                            content: hint,
                        });
                    }
                }
            },
        }
    }

    (files, skills)
}

fn hook_matches_tool(config: &HookConfig, tool: &Tool) -> bool {
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
        ToolNameKind::AllBuiltIn => matches!(tool.kind(), ToolKind::BuiltIn(_)),
        ToolNameKind::BuiltInGlob(glob) => tool.builtin_tool_name().is_some_and(|n| matches_any_pattern([glob], n)),
        ToolNameKind::BuiltIn(name) => {
            // Parse matcher as BuiltInToolName to support all aliases
            // e.g., "read", "fs_read", "fsRead" all match the FsRead tool
            if let Ok(matcher_tool) = name.parse::<tools::BuiltInToolName>() {
                tool.builtin_tool_name().is_some_and(|n| n == matcher_tool)
            } else {
                false
            }
        },
        ToolNameKind::AgentGlob(_) => false,
        ToolNameKind::Agent(_) => false,
    }
}

/// Contains data related to the agent's current state of execution.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecutionState {
    pub active_state: ActiveState,
    pub executing_subagents: HashMap<AgentId, Option<String>>,
}

/// Represents the agent's current state of execution.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ActiveState {
    #[default]
    Idle,
    /// Agent has encountered an error.
    Errored(AgentError),
    /// Agent is waiting for approval to execute tool uses
    WaitingForApproval(WaitingForApproval),
    /// Agent is executing hooks
    ExecutingHooks(ExecutingHooks),
    /// Agent is handling a prompt
    ///
    /// The agent is not able to receive new prompts while in this state
    ExecutingRequest,
    /// Agent is executing tools
    ExecutingTools(ExecutingTools),
    /// Agent is compacting conversation history
    Compacting {
        /// The strategy used for compaction
        strategy: CompactStrategy,
    },
}

/// Tracks approval state for a single tool use.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApprovalState {
    /// Available permission options for this tool
    pub options: Vec<PermissionOption>,
    /// The option selected by the user, if any
    pub selected: Option<PermissionOptionId>,
}

/// State for tools waiting for user approval.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WaitingForApproval {
    /// Tools pending approval with their parsed definitions
    pub tools: Vec<(ToolUseBlock, Tool)>,
    /// Approval state keyed by tool use ID
    pub needs_approval: HashMap<String, ApprovalState>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutingTools(Vec<ExecutingTool>);

impl ExecutingTools {
    fn tools(&self) -> &[ExecutingTool] {
        &self.0
    }

    fn get_tool(&self, id: &ToolExecutionId) -> Option<&ExecutingTool> {
        self.0.iter().find(|tool| &tool.id == id)
    }

    fn get_tool_mut(&mut self, id: &ToolExecutionId) -> Option<&mut ExecutingTool> {
        self.0.iter_mut().find(|tool| &tool.id == id)
    }

    fn all_tools_finished(&self) -> bool {
        self.0.iter().all(|tool| tool.result.is_some())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ExecutingTool {
    id: ToolExecutionId,
    tool_use_block: ToolUseBlock,
    tool: Tool,
    result: Option<ToolExecutorResult>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecutingHooks {
    /// Tracker for results.
    ///
    /// Also contains tool context used for the hook execution, if available - used to potentially
    /// block tool execution.
    #[allow(clippy::type_complexity)]
    hooks: Vec<ExecutingHook>,
    // hooks: HashMap<HookExecutionId, (Option<(ToolUseBlock, Tool)>, Option<HookResult>)>,
    /// See [HookStage].
    stage: HookStage,
}

impl ExecutingHooks {
    fn hooks(&self) -> &[ExecutingHook] {
        &self.hooks
    }

    fn get_hook(&self, id: &HookExecutionId) -> Option<&ExecutingHook> {
        self.hooks.iter().find(|hook| &hook.id == id)
    }

    fn get_hook_mut(&mut self, id: &HookExecutionId) -> Option<&mut ExecutingHook> {
        self.hooks.iter_mut().find(|hook| &hook.id == id)
    }

    fn all_hooks_finished(&self) -> bool {
        self.hooks.iter().all(|hook| hook.result.is_some())
    }

    /// Returns finished per prompt hooks
    fn per_prompt_hooks(&self) -> Vec<String> {
        self.hooks
            .iter()
            .filter_map(|hook| {
                if hook.id.hook.trigger == HookTrigger::UserPromptSubmit
                    && hook
                        .result
                        .as_ref()
                        .is_some_and(|res| res.is_success() && res.output().is_some())
                {
                    Some(
                        hook.result
                            .clone()
                            .expect("result is some")
                            .output()
                            .expect("output is some")
                            .to_string(),
                    )
                } else {
                    None
                }
            })
            .collect()
    }

    fn has_failure_exit_code_for_tool(&self, tool_use_id: impl AsRef<str>) -> Option<&ExecutingHook> {
        self.hooks.iter().find(|hook| {
            hook.exit_code().is_some_and(|code| code == 2)
                && hook
                    .tool_use_block
                    .as_ref()
                    .is_some_and(|tool| tool.tool_use_id == tool_use_id.as_ref())
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ExecutingHook {
    id: HookExecutionId,
    /// The tool use block requested by the model if this hook is part of a tool use.
    tool_use_block: Option<ToolUseBlock>,
    /// The tool that was executed if this hook is part of a tool use.
    tool: Option<Tool>,
    result: Option<HookResult>,
}

impl ExecutingHook {
    fn exit_code(&self) -> Option<i32> {
        self.result.as_ref().and_then(|res| res.exit_code())
    }
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
        tools: Vec<(ToolUseBlock, Tool)>,
        /// List of the tool use id's that require user approval
        needs_approval: Vec<String>,
    },
    /// Hooks after executing tool uses
    PostToolUse { executing_tools: ExecutingTools },
    /// Hooks when the assistant finishes responding
    Stop {
        /// The [UserTurnMetadata] for the completed user turn
        user_turn_metadata: Box<UserTurnMetadata>,
    },
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::util::test::TestBase;

    #[tokio::test]
    async fn test_collect_resources() {
        let mut test_base = TestBase::new().await;

        let files = [
            (".amazonq/rules/first.md", "first"),
            (".amazonq/rules/dir/subdir.md", "subdir"),
            ("~/home.txt", "home"),
        ];

        for file in files {
            test_base = test_base.with_file(file).await;
        }

        let (resources, skills) =
            collect_resources(["file://.amazonq/rules/**/*.md", "file://~/home.txt"], &test_base).await;

        assert!(skills.is_empty());
        for file in files {
            assert!(resources.iter().any(|r| r.content == file.1));
        }
    }
}
