use std::borrow::Cow;
use std::path::PathBuf;
use std::process::ExitCode;
use std::str::FromStr;
use std::sync::Arc;

use agent::agent_config::LoadedAgentConfig;
use agent::agent_loop::model::Model;
use agent::agent_loop::types::{
    ContentBlock as AgentContentBlock,
    ImageBlock,
    ImageFormat,
    ImageSource,
};
use agent::event_log::{
    LogEntry,
    LogEntryV1,
};
use agent::mcp::{
    McpManager,
    McpServerEvent,
};
use agent::permissions::RuntimePermissions;
use agent::protocol::{
    AgentEvent,
    AgentStopReason,
    ApprovalRequest,
    CompactionEvent,
    ContentChunk,
    SendPromptArgs,
    ToolCallResult,
    UpdateEvent,
};
use agent::tools::fs_write::FsWrite;
use agent::tools::summary::Summary;
use agent::tools::{
    BuiltInTool,
    BuiltInToolName,
    Tool,
    ToolKind as AgentToolKind,
};
use agent::tui_commands::{
    CommandOptionsResponse,
    TuiCommand,
};
use agent::types::{
    AgentSnapshot,
    ConversationState,
};
use agent::{
    Agent,
    AgentHandle,
};
use sacp::schema::{
    AGENT_METHOD_NAMES,
    AgentCapabilities,
    AuthMethod,
    CancelNotification,
    ContentBlock,
    ContentChunk as SacpContentChunk,
    Diff,
    Implementation,
    InitializeRequest,
    InitializeResponse,
    LoadSessionRequest,
    LoadSessionResponse,
    ModelInfo as AcpModelInfo,
    NewSessionRequest,
    NewSessionResponse,
    PermissionOption,
    PermissionOptionKind,
    PromptCapabilities,
    PromptRequest,
    PromptResponse,
    ProtocolVersion,
    RequestPermissionRequest,
    SessionId,
    SessionMode,
    SessionModeState,
    SessionModelState,
    SessionNotification,
    SessionUpdate,
    StopReason,
    TextContent,
    ToolCall,
    ToolCallContent,
    ToolCallId,
    ToolCallLocation,
    ToolCallStatus,
    ToolCallUpdate,
    ToolCallUpdateFields,
    ToolKind,
};
use sacp::{
    AgentToClient,
    JrConnectionCx,
    JrRequestCx,
    MessageCx,
};
use tokio::sync::{
    mpsc,
    oneshot,
};
use tokio_util::compat::{
    TokioAsyncReadCompatExt,
    TokioAsyncWriteCompatExt,
};
use tracing::{
    debug,
    error,
    info,
    warn,
};
use uuid::Uuid;

use super::extensions::{
    ClearStatusNotification,
    CompactionStatus,
    CompactionStatusNotification,
    McpOauthRequestNotification,
    McpServerInitializedNotification,
    SubagentInfo,
    methods,
};
use super::subagent_tool::{
    handle_internal_prompt,
    handle_subagent_request,
};
use crate::agent::acp::session_manager::{
    AgentInfo,
    SessionManager,
    SessionManagerHandle,
};
use crate::agent::rts::{
    RtsModel,
    RtsState,
};
use crate::agent::session::{
    SessionDb,
    SessionState,
};
use crate::api_client::{
    ApiClient,
    MockResponseRegistryHandle,
};
use crate::cli::chat::legacy::model::{
    ModelInfo,
    find_model,
    get_available_models,
};
use crate::os::Os;
use crate::util::paths::PathResolver;

/// Messages that can be sent to an [`AcpSession`] actor via [`AcpSessionHandle`].
///
/// Each variant represents a different operation the session can perform. Most variants
/// include a `respond_to` channel for returning results to the caller.
#[derive(Debug)]
pub enum AcpSessionRequest {
    /// External prompt from ACP client (TUI).
    /// The response is sent via the `request_cx` when the turn completes.
    Prompt {
        request: PromptRequest,
        request_cx: JrRequestCx<PromptResponse>,
    },
    /// Internal prompt for subagent execution (no ACP connection needed).
    /// Used when spawning subagents that run without TUI interaction.
    InternalPrompt {
        query: String,
        respond_to: oneshot::Sender<eyre::Result<Summary>>,
    },
    /// Swap to a different agent configuration (e.g., switching modes).
    SwapAgent {
        agent_config: Box<agent::agent_config::definitions::AgentConfig>,
        respond_to: oneshot::Sender<Result<(), agent::protocol::AgentError>>,
    },
    /// Set the model ID for this session.
    SetModel {
        model_id: String,
        respond_to: oneshot::Sender<Result<(), String>>,
    },
    /// Get the current model ID for this session.
    GetModelId { respond_to: oneshot::Sender<String> },
    /// Cancel the current operation and end the turn.
    Cancel,
    /// Execute a slash command via an extension method.
    ExecuteCommand {
        command: TuiCommand,
        respond_to: oneshot::Sender<agent::tui_commands::CommandResult>,
    },
    /// Get options for a command (for autocomplete).
    GetCommandOptions {
        command: super::schema::TuiCommandKind,
        partial: String,
        respond_to: oneshot::Sender<CommandOptionsResponse>,
    },
}

#[derive(Debug)]
enum InnerSender<T> {
    Strong(mpsc::Sender<T>),
    Weak(mpsc::WeakSender<T>),
}

impl<T> Clone for InnerSender<T> {
    fn clone(&self) -> Self {
        match self {
            InnerSender::Strong(tx) => InnerSender::Weak(tx.downgrade()),
            InnerSender::Weak(tx) => InnerSender::Weak(tx.clone()),
        }
    }
}

impl<T> InnerSender<T> {
    async fn send(&self, msg: T) -> Result<(), sacp::Error> {
        match self {
            InnerSender::Strong(tx) => tx
                .send(msg)
                .await
                .map_err(|_e| sacp::util::internal_error("Channel closed")),
            InnerSender::Weak(tx) => tx
                .upgrade()
                .ok_or_else(|| sacp::util::internal_error("Weak sender dropped"))?
                .send(msg)
                .await
                .map_err(|_e| sacp::util::internal_error("Channel closed")),
        }
    }
}

/// Handle for communicating with an [`AcpSession`] actor.
///
/// # Method Patterns
///
/// ## Dispatch-Only Methods (Preferred)
/// These methods send a request and return immediately. Responses are sent
/// via the dedicated egress path (agent loop → session → client connection):
/// - `handle_prompt()` - Response via PromptResponse
/// - `cancel()` - Cancellation confirmed via session updates
/// - `request_permission()` - Approval result via agent events
/// - `add_trusted_tool()` - No response needed
///
/// ## Request-Response Methods (Avoid in dispatch handlers)
/// WARNING: These methods block waiting for a response. Using them in dispatch
/// handlers can cause deadlocks. They should be converted to dispatch-only in
/// a future refactor:
/// - `get_model_id()`
/// - `set_model()`
/// - `swap_agent()`
/// - `execute_command()`
/// - `get_command_options()`
/// - `internal_prompt()`
#[derive(Clone, Debug)]
pub struct AcpSessionHandle {
    tx: InnerSender<AcpSessionRequest>,
    /// If this session is a background subagent, contains its metadata
    pub _subagent_info: Option<SubagentInfo>,
}

impl AcpSessionHandle {
    pub async fn handle_prompt(
        &self,
        request: PromptRequest,
        request_cx: JrRequestCx<PromptResponse>,
    ) -> Result<(), sacp::Error> {
        self.tx.send(AcpSessionRequest::Prompt { request, request_cx }).await
    }

    /// Send an internal prompt (for subagent execution, no ACP connection needed)
    pub async fn internal_prompt(&self, query: String) -> eyre::Result<Summary> {
        let (respond_to, rx) = oneshot::channel::<eyre::Result<Summary>>();
        self.tx
            .send(AcpSessionRequest::InternalPrompt { query, respond_to })
            .await?;
        rx.await?
    }

    /// Swap to a different agent configuration
    pub async fn swap_agent(
        &self,
        agent_config: agent::agent_config::definitions::AgentConfig,
    ) -> Result<(), agent::protocol::AgentError> {
        let (respond_to, rx) = oneshot::channel();
        self.tx
            .send(AcpSessionRequest::SwapAgent {
                agent_config: agent_config.into(),
                respond_to,
            })
            .await
            .map_err(|_e| agent::protocol::AgentError::Channel)?;
        rx.await.map_err(|_e| agent::protocol::AgentError::Channel)?
    }

    /// Set the model ID for this session
    pub async fn set_model(&self, model_id: String) -> Result<(), String> {
        let (respond_to, rx) = oneshot::channel();
        self.tx
            .send(AcpSessionRequest::SetModel { model_id, respond_to })
            .await
            .map_err(|_e| "Channel closed".to_string())?;
        rx.await.map_err(|_e| "Response channel closed".to_string())?
    }

    /// Get the current model ID for this session
    pub async fn get_model_id(&self) -> Result<String, String> {
        let (respond_to, rx) = oneshot::channel();
        self.tx
            .send(AcpSessionRequest::GetModelId { respond_to })
            .await
            .map_err(|_e| "Channel closed".to_string())?;
        rx.await.map_err(|_e| "Response channel closed".to_string())
    }

    pub async fn cancel(&self) -> Result<(), sacp::Error> {
        self.tx.send(AcpSessionRequest::Cancel).await
    }

    /// Execute a slash command
    pub async fn execute_command(&self, command: TuiCommand) -> agent::tui_commands::CommandResult {
        let (respond_to, rx) = oneshot::channel();
        if self
            .tx
            .send(AcpSessionRequest::ExecuteCommand { command, respond_to })
            .await
            .is_err()
        {
            return agent::tui_commands::CommandResult::error("Channel closed");
        }
        rx.await
            .unwrap_or_else(|_| agent::tui_commands::CommandResult::error("Response channel closed"))
    }

    /// Get options for a command (for autocomplete)
    pub async fn get_command_options(
        &self,
        command: super::schema::TuiCommandKind,
        partial: String,
    ) -> CommandOptionsResponse {
        let (respond_to, rx) = oneshot::channel();
        if self
            .tx
            .send(AcpSessionRequest::GetCommandOptions {
                command,
                partial,
                respond_to,
            })
            .await
            .is_err()
        {
            return CommandOptionsResponse::default();
        }
        rx.await.unwrap_or_default()
    }
}

/// Configuration for spawning an [`AcpSession`].
///
/// This is an owned type suitable for sending through channels (e.g., to SessionManager).
/// Use [`AcpSessionBuilder`] for the actual session construction.
#[derive(Debug, Clone)]
pub struct AcpSessionConfig {
    pub session_id: String,
    pub cwd: PathBuf,
    /// If true, load existing session from disk; otherwise create new session
    pub load: bool,
    pub initial_agent_name: Option<String>,
    pub user_embedded_msg: Option<String>,
    pub is_subagent: bool,
    pub model_id: Option<String>,
    /// MCP servers provided by the ACP client
    pub mcp_servers: Vec<sacp::schema::McpServer>,
}

impl AcpSessionConfig {
    pub fn new(session_id: String, cwd: PathBuf) -> Self {
        Self {
            session_id,
            cwd,
            load: false,
            initial_agent_name: None,
            user_embedded_msg: None,
            is_subagent: false,
            model_id: None,
            mcp_servers: Vec::new(),
        }
    }

    pub fn load(mut self, load: bool) -> Self {
        self.load = load;
        self
    }

    pub fn initial_agent_name(mut self, name: String) -> Self {
        self.initial_agent_name = Some(name);
        self
    }

    pub fn user_embedded_msg(mut self, msg: String) -> Self {
        self.user_embedded_msg = Some(msg);
        self
    }

    pub fn mcp_servers(mut self, servers: Vec<sacp::schema::McpServer>) -> Self {
        self.mcp_servers = servers;
        self
    }

    #[allow(clippy::wrong_self_convention)]
    pub fn is_subagent(mut self, is_subagent: bool) -> Self {
        self.is_subagent = is_subagent;
        self
    }
}

/// Builder for constructing and spawning an [`AcpSession`] actor.
#[derive(Default)]
pub struct AcpSessionBuilder<'a> {
    os: Option<Os>,
    session_id: Option<String>,
    cwd: Option<PathBuf>,
    load: bool,
    initial_agent_config: Option<Cow<'a, LoadedAgentConfig>>,
    user_embedded_msg: Option<&'a str>,
    is_subagent: bool,
    global_mcp_path: Option<&'a PathBuf>,
    local_mcp_path: Option<&'a PathBuf>,
    model_id: Option<&'a str>,
    session_tx: Option<SessionManagerHandle>,
    client_cx: Option<JrConnectionCx<AgentToClient>>,
    mock_registry: Option<MockResponseRegistryHandle>,
}

impl<'a> AcpSessionBuilder<'a> {
    pub fn os(mut self, os: Os) -> Self {
        self.os = Some(os);
        self
    }

    pub fn session_id(mut self, id: String) -> Self {
        self.session_id = Some(id);
        self
    }

    pub fn cwd(mut self, cwd: PathBuf) -> Self {
        self.cwd = Some(cwd);
        self
    }

    pub fn load(mut self, load: bool) -> Self {
        self.load = load;
        self
    }

    pub fn initial_agent_config(mut self, agent_config: Cow<'a, LoadedAgentConfig>) -> Self {
        self.initial_agent_config.replace(agent_config);
        self
    }

    pub fn user_embedded_msg(mut self, msg: Option<&'a str>) -> Self {
        self.user_embedded_msg = msg;
        self
    }

    pub fn set_as_subagent(mut self, is_subagent: bool) -> Self {
        self.is_subagent = is_subagent;
        self
    }

    pub fn global_mcp_path(mut self, path: Option<&'a PathBuf>) -> Self {
        self.global_mcp_path = path;
        self
    }

    pub fn local_mcp_path(mut self, path: Option<&'a PathBuf>) -> Self {
        self.local_mcp_path = path;
        self
    }

    pub fn model_id(mut self, id: Option<&'a str>) -> Self {
        self.model_id = id;
        self
    }

    pub fn session_tx(mut self, session_tx: SessionManagerHandle) -> Self {
        self.session_tx.replace(session_tx);
        self
    }

    pub fn connection_cx(mut self, cx: JrConnectionCx<AgentToClient>) -> Self {
        self.client_cx = Some(cx);
        self
    }

    pub fn mock_registry(mut self, registry: MockResponseRegistryHandle) -> Self {
        self.mock_registry = Some(registry);
        self
    }

    /// Spawns a new ACP session actor and returns a handle to communicate with it.
    ///
    /// The returned `ready_rx` resolves after historical notifications have been emitted
    /// (for loaded sessions) and the session is ready to accept prompts.
    pub async fn start_session(mut self) -> eyre::Result<(AcpSessionHandle, oneshot::Receiver<()>)> {
        let os = self.os.take().ok_or_else(|| eyre::eyre!("Os is required"))?;

        let (tx, rx) = mpsc::channel(32);
        let (ready_tx, ready_rx) = oneshot::channel();
        let session = AcpSession::with_builder(os, rx, self).await?;
        tokio::spawn(async move { session.main_loop(ready_tx).await });

        Ok((
            AcpSessionHandle {
                tx: InnerSender::Strong(tx),
                _subagent_info: None,
            },
            ready_rx,
        ))
    }
}

/// An actor representing an active ACP session.
///
/// Each session owns:
/// - An [`Agent`](agent::Agent) for LLM interactions
/// - A [`JrConnectionCx`] for direct client communication (egress)
/// - A [`SessionDb`] for persistence
///
/// The session handles:
/// - Converting ACP protocol messages to agent requests (ingress)
/// - Converting agent events to ACP notifications (egress via owned connection)
/// - Tool approval flow with trusted tool tracking
/// - Custom extension handlers (slash commands, etc.)
struct AcpSession {
    session_id: SessionId,
    agent: AgentHandle,
    request_rx: mpsc::Receiver<AcpSessionRequest>,
    session_db: Arc<SessionDb>,
    rts_state: Arc<RtsState>,
    api_client: ApiClient,
    session_tx: SessionManagerHandle,
    connection_cx: JrConnectionCx<AgentToClient>,
    pending_prompt_response: Option<tokio::sync::Mutex<JrRequestCx<PromptResponse>>>,
    os: Os,
}

impl AcpSession {
    async fn with_builder(
        os: Os,
        request_rx: mpsc::Receiver<AcpSessionRequest>,
        builder: AcpSessionBuilder<'_>,
    ) -> eyre::Result<Self> {
        let session_id_str = builder
            .session_id
            .ok_or_else(|| eyre::eyre!("session_id is required"))?;
        let cwd = builder.cwd.ok_or_else(|| eyre::eyre!("cwd is required"))?;
        let session_tx = builder.session_tx.expect("Missing session request sender");
        let connection_cx = builder.client_cx.expect("Missing client connection");

        // Determine if loading existing session or creating new one
        let (session_db, snapshot) = if builder.load {
            // Load existing session
            let db = SessionDb::load(&session_id_str, Some(&cwd))?;
            let state = db.session().session_state;
            let entries = db.load_log_entries()?;

            let conversation_id = Uuid::parse_str(&session_id_str)
                .map_err(|_e| eyre::eyre!("Invalid session ID '{}': must be a valid UUID", session_id_str))?;
            let conversation_state = ConversationState::new(conversation_id, entries);
            let snapshot = AgentSnapshot {
                agent_config: if let Some(agent_config) = builder.initial_agent_config {
                    agent_config.config().clone()
                } else {
                    Default::default()
                },
                conversation_state,
                conversation_metadata: state.conversation_metadata().clone(),
                permissions: state.permissions().clone().with_cwd(&cwd.to_string_lossy()),
                ..Default::default()
            };

            (db, snapshot)
        } else {
            // Create new session
            let conversation_id = Uuid::parse_str(&session_id_str)
                .map_err(|_e| eyre::eyre!("Invalid session ID '{}': must be a valid UUID", session_id_str))?;
            let permissions = RuntimePermissions::default().with_cwd(&cwd.to_string_lossy());
            let snapshot = AgentSnapshot {
                agent_config: if let Some(agent_config) = builder.initial_agent_config {
                    agent_config.config().clone()
                } else {
                    Default::default()
                },
                conversation_state: ConversationState::new(conversation_id, Vec::new()),
                permissions: permissions.clone(),
                ..Default::default()
            };
            let rts_snapshot = crate::agent::rts::RtsStateSnapshot {
                conversation_id: session_id_str.clone(),
                model_info: None,
                context_usage_percentage: None,
            };
            let initial_state = SessionState::new(snapshot.conversation_metadata.clone(), rts_snapshot, permissions);
            let db = SessionDb::new(session_id_str.clone(), &cwd, initial_state)?;

            (db, snapshot)
        };

        let rts_state = Arc::new(RtsState::new(session_id_str.clone()));

        // Set model ID from agent config with validation
        if let Err(e) = update_model_info(&os, &rts_state, snapshot.agent_config.model()).await {
            warn!("Failed to set initial model: {}", e);
        }

        let (api_client, model): (ApiClient, Arc<dyn Model>) = if let Some(registry) = builder.mock_registry {
            let client = ApiClient::new_ipc_mock(registry);
            (client.clone(), Arc::new(RtsModel::new(client, Arc::clone(&rts_state))))
        } else {
            let client = os.client.clone();
            (client.clone(), Arc::new(RtsModel::new(client, Arc::clone(&rts_state))))
        };

        let mut agent = Agent::new(
            snapshot,
            builder.local_mcp_path,
            builder.global_mcp_path,
            model,
            McpManager::default().spawn(),
            builder.is_subagent,
        )
        .await?;

        agent.set_sys_provider(super::acp_provider::AcpProvider::new(cwd));

        if let Some(msg) = builder.user_embedded_msg {
            agent.prepend_embedded_user_msg(msg);
        }

        let agent = agent.spawn();

        Ok(Self {
            session_id: SessionId::new(session_id_str),
            agent,
            request_rx,
            session_tx,
            connection_cx,
            session_db: Arc::new(session_db),
            rts_state,
            api_client,
            pending_prompt_response: None,
            os,
        })
    }

    async fn main_loop(mut self, ready_tx: oneshot::Sender<()>) {
        // Emit historical notifications for loaded sessions
        if let Err(e) = self.emit_historical_notifications().await {
            warn!("Failed to emit historical notifications: {}", e);
        }

        // Signal that session is ready
        let _ = ready_tx.send(());

        loop {
            tokio::select! {
                // Handle new ACP requests
                req = self.request_rx.recv() => {
                    let Some(req) = req else {
                        warn!("ACP session request channel closed, exiting");
                        break;
                    };
                    self.handle_request(req).await;
                }

                agent_event = self.agent.recv() => {
                    match agent_event {
                        Ok(event) => {
                            debug!("Received agent event: {:?}", &event);
                            self.handle_agent_event(event).await;
                        }
                        Err(_) => {
                            warn!("Agent event channel closed");
                        }
                    }
                }
            }
        }
    }

    fn send_session_notification(&self, update: SessionUpdate) -> Result<(), sacp::Error> {
        self.connection_cx
            .send_notification(SessionNotification::new(self.session_id.clone(), update))
    }

    fn send_ext_notification<T: serde::Serialize>(&self, method: &str, params: T) -> Result<(), sacp::Error> {
        let params_raw = serde_json::value::to_raw_value(&params)
            .map_err(|e| sacp::util::internal_error(format!("Failed to serialize params: {}", e)))?;
        let ext_notification = sacp::schema::ExtNotification::new(method, std::sync::Arc::from(params_raw));
        self.connection_cx
            .send_notification(sacp::schema::AgentNotification::ExtNotification(ext_notification))
    }

    fn send_turn_metadata(&self, metadata: &agent::agent_loop::protocol::UserTurnMetadata) -> Result<(), sacp::Error> {
        let notification = super::schema::MetadataNotification {
            session_id: self.session_id.to_string(),
            context_usage_percentage: metadata.context_usage_percentage,
        };
        self.connection_cx.send_notification(notification)
    }

    async fn emit_historical_notifications(&self) -> Result<(), sacp::Error> {
        let entries = self
            .session_db
            .load_log_entries()
            .map_err(|e| sacp::util::internal_error(format!("Failed to load log entries: {}", e)))?;

        for entry in entries {
            for update in log_entry_to_session_updates(&entry) {
                self.send_session_notification(update)?;
            }
        }
        Ok(())
    }

    async fn handle_request(&mut self, req: AcpSessionRequest) {
        match req {
            AcpSessionRequest::Prompt { request, request_cx } => {
                if self.pending_prompt_response.is_some() {
                    let _ = request_cx.respond_with_error(sacp::util::internal_error("Prompt already in progress"));
                    return;
                }

                let agent = self.agent.clone();

                tokio::spawn(async move {
                    if let Err(e) = handle_prompt_request(request, agent).await {
                        error!("Failed to handle prompt request: {e}");
                    }
                });

                // Store the response channel for unified egress to use
                self.pending_prompt_response = Some(tokio::sync::Mutex::new(request_cx));
            },
            AcpSessionRequest::InternalPrompt { query, respond_to } => {
                let agent = self.agent.clone();

                tokio::spawn(async move {
                    let result = handle_internal_prompt(query, agent).await;
                    let _ = respond_to.send(result);
                });
            },
            AcpSessionRequest::SwapAgent {
                agent_config,
                respond_to,
            } => {
                if let Err(e) = update_model_info(&self.os, &self.rts_state, agent_config.model()).await {
                    warn!("Failed to update model during swap: {}", e);
                }
                let resolver = PathResolver::new(&self.os);
                let local_mcp_path = resolver.workspace().mcp_config().ok();
                let global_mcp_path = resolver.global().mcp_config().ok();
                let result = self
                    .agent
                    .swap_agent(agent::protocol::SwapAgentArgs {
                        agent_config: *agent_config,
                        local_mcp_path,
                        global_mcp_path,
                    })
                    .await;
                let _ = respond_to.send(result);
            },
            AcpSessionRequest::SetModel { model_id, respond_to } => {
                let result = update_model_info(&self.os, &self.rts_state, Some(&model_id)).await;
                let _ = respond_to.send(result);
            },
            AcpSessionRequest::GetModelId { respond_to } => {
                let _ = respond_to.send(self.rts_state.model_id().unwrap_or_default());
            },
            AcpSessionRequest::Cancel => {
                // Send cancellation request to the underlying agent. This is only half of the ACP protocol.
                // To conform with the protocol, we would also need to respond to the client with cancelled
                // as stop reason. This egress part is assumed to happen through the normal agent event flow
                // when the agent emits AgentEvent::Stop(AgentStopReason::Cancelled).
                if let Err(e) = self.agent.cancel().await {
                    error!("Failed to cancel agent: {}", e);
                }
                error!("cancel called on agent handle");
            },
            AcpSessionRequest::ExecuteCommand { command, respond_to } => {
                let result =
                    super::command_handler::execute_command(command, &self.api_client, &self.rts_state, &self.agent)
                        .await;
                let _ = respond_to.send(result);
            },
            AcpSessionRequest::GetCommandOptions {
                command,
                partial,
                respond_to,
            } => {
                let result = super::command_handler::get_command_options(
                    command,
                    &partial,
                    &self.api_client,
                    &self.rts_state,
                    &self.agent,
                )
                .await;
                let _ = respond_to.send(result);
            },
        }
    }

    async fn handle_agent_event(&mut self, event: AgentEvent) {
        let session_db = Arc::clone(&self.session_db);
        let rts_state = Arc::clone(&self.rts_state);

        match event {
            // MCP events during initialization (e.g., OAuth requests) come through InitializeUpdate,
            // not the regular Mcp variant, since they occur before the agent is fully initialized.
            AgentEvent::InitializeUpdate(init_event) => {
                let agent::protocol::InitializeUpdateEvent::Mcp(mcp_event) = init_event;
                if let Err(e) = self.handle_mcp_event(mcp_event).await {
                    error!("Failed to handle MCP event during initialization: {}", e);
                }
            },
            AgentEvent::Update(update_event) => {
                if let Some(update) = convert_update_event_to_session_update(update_event) {
                    let _ = self.send_session_notification(update);
                }
            },
            AgentEvent::ApprovalRequest(req) => {
                info!(
                    "AgentEvent::ApprovalRequest: id={}, tool_use={:?}, context={:?}",
                    req.id, req.tool_use, req.context
                );
                let connection_cx = self.connection_cx.clone();
                let session_id = self.session_id.clone();
                let agent = self.agent.clone();
                tokio::spawn(async move {
                    handle_approval_request(req, connection_cx, session_id, agent).await;
                });
            },
            AgentEvent::LogEntryAppended { entry, .. } => {
                if let Err(e) = session_db.append_log_entry(&entry) {
                    warn!("Failed to persist log entry: {}", e);
                }
            },
            AgentEvent::EndTurn(md) => {
                // Update context usage in rts state and send to TUI
                if let Some(p) = md.context_usage_percentage {
                    rts_state.set_context_usage_percentage(Some(p));
                }
                if let Err(e) = self.send_turn_metadata(&md) {
                    warn!("Failed to send turn metadata: {}", e);
                }
                // Send prompt response directly to the client - this ends the turn so we take() it
                if let Some(respond_to) = self.pending_prompt_response.take() {
                    match self.agent.create_snapshot().await {
                        Ok(snapshot) => {
                            let state = SessionState::new(
                                snapshot.conversation_metadata,
                                rts_state.snapshot(),
                                snapshot.permissions,
                            );
                            if let Err(e) = session_db.update_state(state) {
                                warn!("Failed to persist session state: {}", e);
                            }
                        },
                        Err(e) => {
                            error!("Failed to get agent snapshot for session persistence: {}", e);
                        },
                    }
                    let respond_to = respond_to.into_inner();
                    let stop_reason = match md.end_reason {
                        agent::agent_loop::protocol::LoopEndReason::UserTurnEnd => StopReason::EndTurn,
                        agent::agent_loop::protocol::LoopEndReason::ToolUseRejected => StopReason::Refusal,
                        agent::agent_loop::protocol::LoopEndReason::Cancelled => StopReason::Cancelled,
                        // This does not quite match 1 to 1 so we'll settle for this for now
                        _ => StopReason::EndTurn,
                    };
                    let _ = respond_to.respond(PromptResponse::new(stop_reason));
                }
            },
            AgentEvent::Stop(AgentStopReason::Error(_)) => {
                // Send error response directly to the client - this ends the turn so we take() it
                if let Some(respond_to) = self.pending_prompt_response.take() {
                    let respond_to = respond_to.into_inner();
                    let _ = respond_to.respond_with_error(sacp::util::internal_error("Agent error"));
                }
            },
            AgentEvent::SpawnSubagentRequest(spawn_request) => {
                let session_tx = self.session_tx.clone();
                handle_subagent_request(spawn_request, session_tx).await;
            },
            AgentEvent::Mcp(mcp_event) => {
                if let Err(e) = self.handle_mcp_event(mcp_event).await {
                    error!("Failed to handle MCP event: {}", e);
                }
            },
            AgentEvent::Compaction(compaction_event) => {
                tracing::info!("Received compaction event: {:?}", compaction_event);
                let status = match compaction_event {
                    CompactionEvent::Started => CompactionStatus::Started,
                    CompactionEvent::Completed => CompactionStatus::Completed,
                    CompactionEvent::Failed { error } => CompactionStatus::Failed { error },
                };
                if let Err(e) = self.send_ext_notification(methods::COMPACTION_STATUS, CompactionStatusNotification {
                    session_id: self.session_id.clone(),
                    status,
                }) {
                    error!("Failed to send compaction notification: {}", e);
                }
            },
            AgentEvent::Clear(_) => {
                tracing::info!("Received clear event");
                if let Err(e) = self.send_ext_notification(methods::CLEAR_STATUS, ClearStatusNotification {
                    session_id: self.session_id.clone(),
                }) {
                    error!("Failed to send clear notification: {}", e);
                }
            },
            _ => {
                // Other events that don't need processing
            },
        }
    }

    async fn handle_mcp_event(&self, event: McpServerEvent) -> Result<(), sacp::Error> {
        match event {
            McpServerEvent::OauthRequest { server_name, oauth_url } => {
                info!(?server_name, ?oauth_url, "Forwarding OAuth request to client");
                self.send_ext_notification(methods::MCP_OAUTH_REQUEST, McpOauthRequestNotification {
                    session_id: self.session_id.clone(),
                    server_name,
                    oauth_url,
                })
            },
            McpServerEvent::Initialized { server_name, .. } => {
                info!(?server_name, "Forwarding MCP server initialized to client");
                self.send_ext_notification(methods::MCP_SERVER_INITIALIZED, McpServerInitializedNotification {
                    session_id: self.session_id.clone(),
                    server_name,
                })
            },
            // Other MCP events don't need forwarding to client
            _ => Ok(()),
        }
    }
}

async fn handle_approval_request(
    req: ApprovalRequest,
    client_cx: JrConnectionCx<AgentToClient>,
    session_id: SessionId,
    agent: AgentHandle,
) {
    // Map agent permission options to ACP permission options
    // Filter out *ToolArgs variants as ACP only supports tool-level always options
    let options: Vec<PermissionOption> = req
        .options
        .iter()
        .filter_map(|opt| {
            let (id, kind) = match opt.id {
                // TODO: use id's from the agent instead of hard-coded ACP id's.
                agent::protocol::PermissionOptionId::AllowOnce => ("allow_once", PermissionOptionKind::AllowOnce),
                agent::protocol::PermissionOptionId::AllowAlwaysTool => {
                    ("allow_always", PermissionOptionKind::AllowAlways)
                },
                agent::protocol::PermissionOptionId::AllowAlwaysToolArgs => return None,
                agent::protocol::PermissionOptionId::RejectOnce => ("reject_once", PermissionOptionKind::RejectOnce),
                agent::protocol::PermissionOptionId::RejectAlwaysTool => return None,
                agent::protocol::PermissionOptionId::RejectAlwaysToolArgs => return None,
                agent::protocol::PermissionOptionId::Custom(_) => return None,
            };
            Some(PermissionOption::new(id, &opt.label, kind))
        })
        .collect();

    debug!("Sending permission request: {:?}", req);
    let response = client_cx
        .send_request(RequestPermissionRequest::new(
            session_id,
            ToolCallUpdate::new(
                ToolCallId::new(req.id.clone()),
                ToolCallUpdateFields::new().title(Some(get_tool_title(&req.tool))),
            ),
            options,
        ))
        .block_task()
        .await;

    match response {
        Ok(res) => match res.outcome {
            sacp::schema::RequestPermissionOutcome::Selected(selected) => {
                use std::str::FromStr;
                // Map ACP option_id to agent PermissionOptionId
                // ACP's "allow_always"/"reject_always" map to our tool-level variants
                let option_id = match selected.option_id.0.as_ref() {
                    "allow_always" => agent::protocol::PermissionOptionId::AllowAlwaysTool,
                    "reject_always" => agent::protocol::PermissionOptionId::RejectAlwaysTool,
                    other => agent::protocol::PermissionOptionId::from_str(other)
                        .unwrap_or_else(|_| agent::protocol::PermissionOptionId::Custom(other.to_string())),
                };
                let reason = if option_id.is_reject() {
                    Some("User denied tool execution".to_string())
                } else {
                    None
                };
                let approval_result = agent::protocol::ApprovalResult { option_id, reason };
                if let Err(e) = agent
                    .send_tool_use_approval_result(agent::protocol::SendApprovalResultArgs {
                        id: req.id,
                        result: approval_result,
                    })
                    .await
                {
                    error!("Failed to send approval result: {}", e);
                }
            },
            sacp::schema::RequestPermissionOutcome::Cancelled => {
                if let Err(e) = agent.cancel().await {
                    error!("Failed to cancel agent: {}", e);
                }
            },
            _ => {
                if let Err(e) = agent
                    .send_tool_use_approval_result(agent::protocol::SendApprovalResultArgs {
                        id: req.id,
                        result: agent::protocol::ApprovalResult {
                            option_id: agent::protocol::PermissionOptionId::RejectOnce,
                            reason: Some("Unknown response".to_string()),
                        },
                    })
                    .await
                {
                    error!("Failed to send approval result: {}", e);
                }
            },
        },
        Err(e) => {
            error!("Failed to get permission response: {:?}", e);
            if let Err(e) = agent
                .send_tool_use_approval_result(agent::protocol::SendApprovalResultArgs {
                    id: req.id,
                    result: agent::protocol::ApprovalResult {
                        option_id: agent::protocol::PermissionOptionId::RejectOnce,
                        reason: Some(format!("Permission request failed: {}", e)),
                    },
                })
                .await
            {
                error!("Failed to send approval result: {}", e);
            }
        },
    }
}

/// Handle a prompt request (runs in separate task) - INGRESS ONLY
async fn handle_prompt_request(request: PromptRequest, agent: AgentHandle) -> Result<(), sacp::Error> {
    // Convert and send request to agent
    let content: Vec<agent::protocol::ContentChunk> = request
        .prompt
        .iter()
        .filter_map(|block| match block {
            ContentBlock::Text(text_content) => Some(agent::protocol::ContentChunk::Text(text_content.text.clone())),
            ContentBlock::ResourceLink(link) => {
                let mut json = serde_json::to_value(link).unwrap_or_default();
                if let Some(obj) = json.as_object_mut() {
                    obj.insert(
                        "type".to_string(),
                        serde_json::Value::String("resource_link".to_string()),
                    );
                }
                Some(agent::protocol::ContentChunk::ResourceLink(json.to_string()))
            },
            ContentBlock::Image(img) => {
                use base64::Engine;
                let format = mime_to_image_format(&img.mime_type)?;
                let bytes = base64::engine::general_purpose::STANDARD.decode(&img.data).ok()?;
                Some(agent::protocol::ContentChunk::Image(ImageBlock {
                    format,
                    source: ImageSource::Bytes(bytes),
                }))
            },
            _ => None,
        })
        .collect();

    agent
        .send_prompt(SendPromptArgs {
            content,
            should_continue_turn: None,
        })
        .await
        .inspect_err(|e| error!("encountered error during send prompt: {e}"))
        .map_err(|_e| sacp::util::internal_error("Failed to send prompt"))?;

    Ok(())
}

fn convert_update_event_to_session_update(update_event: UpdateEvent) -> Option<SessionUpdate> {
    match update_event {
        UpdateEvent::AgentContent(ContentChunk::Text(text)) => Some(SessionUpdate::AgentMessageChunk(
            SacpContentChunk::new(ContentBlock::Text(TextContent::new(text))),
        )),
        UpdateEvent::ToolCall(tool_call) => {
            let locations = get_tool_locations(&tool_call.tool);
            let title = get_tool_title(&tool_call.tool);

            let mut acp_tool_call = ToolCall::new(ToolCallId::new(tool_call.id), title)
                .kind(get_tool_kind(&tool_call.tool_use_block.name))
                .status(ToolCallStatus::Pending)
                .content(get_tool_content(&tool_call.tool))
                .raw_input(Some(tool_call.tool_use_block.input.clone()));

            if let Some(locations) = locations {
                acp_tool_call = acp_tool_call.locations(locations);
            }

            Some(SessionUpdate::ToolCall(acp_tool_call))
        },
        UpdateEvent::ToolCallFinished { tool_call, result } => {
            let (status, raw_output) = match result {
                ToolCallResult::Success(output) => (ToolCallStatus::Completed, serde_json::to_value(output).ok()),
                ToolCallResult::Error(_) => (ToolCallStatus::Failed, None),
                ToolCallResult::Cancelled => (ToolCallStatus::Failed, None),
            };

            let locations = get_tool_locations(&tool_call.tool);
            let title = get_tool_title(&tool_call.tool);

            Some(SessionUpdate::ToolCallUpdate(ToolCallUpdate::new(
                ToolCallId::new(tool_call.id),
                ToolCallUpdateFields::new()
                    .status(Some(status))
                    .title(Some(title))
                    .kind(Some(get_tool_kind(&tool_call.tool_use_block.name)))
                    .raw_input(Some(tool_call.tool_use_block.input.clone()))
                    .raw_output(raw_output)
                    .locations(locations),
            )))
        },
        _ => None,
    }
}

/// Convert a log entry to session update notifications for historical replay.
fn log_entry_to_session_updates(entry: &LogEntry) -> Vec<SessionUpdate> {
    match entry {
        LogEntry::V1(LogEntryV1::Prompt { content, .. }) => content
            .iter()
            .filter_map(agent_content_to_acp)
            .map(|content| SessionUpdate::UserMessageChunk(SacpContentChunk::new(content)))
            .collect(),
        LogEntry::V1(LogEntryV1::AssistantMessage { content, .. }) => {
            let mut updates = Vec::new();
            for block in content {
                match block {
                    AgentContentBlock::ToolUse(tool_use) => {
                        updates.push(SessionUpdate::ToolCall(
                            ToolCall::new(ToolCallId::new(tool_use.tool_use_id.clone()), tool_use.name.clone())
                                .kind(get_tool_kind(&tool_use.name))
                                .raw_input(Some(tool_use.input.clone())),
                        ));
                    },
                    _ => {
                        if let Some(content) = agent_content_to_acp(block) {
                            updates.push(SessionUpdate::AgentMessageChunk(SacpContentChunk::new(content)));
                        }
                    },
                }
            }
            updates
        },
        LogEntry::V1(LogEntryV1::ToolResults { results, .. }) => results
            .iter()
            .map(|(tool_use_id, tool_result)| {
                let status = match &tool_result.result {
                    ToolCallResult::Success(_) => ToolCallStatus::Completed,
                    ToolCallResult::Error(_) | ToolCallResult::Cancelled => ToolCallStatus::Failed,
                };
                SessionUpdate::ToolCallUpdate(ToolCallUpdate::new(
                    ToolCallId::new(tool_use_id.clone()),
                    ToolCallUpdateFields::new().status(status),
                ))
            })
            .collect(),
        // Compaction, ResetTo, CancelledPrompt, Clear don't emit notifications
        LogEntry::V1(
            LogEntryV1::Compaction { .. }
            | LogEntryV1::ResetTo { .. }
            | LogEntryV1::CancelledPrompt
            | LogEntryV1::Clear,
        ) => vec![],
    }
}

fn agent_content_to_acp(block: &AgentContentBlock) -> Option<ContentBlock> {
    match block {
        AgentContentBlock::Text(text) => Some(ContentBlock::Text(TextContent::new(text.clone()))),
        _ => None,
    }
}

fn get_tool_kind(tool_name: &str) -> ToolKind {
    if let Ok(builtin_tool) = BuiltInToolName::from_str(tool_name) {
        match builtin_tool {
            BuiltInToolName::FsRead => ToolKind::Read,
            BuiltInToolName::FsWrite => ToolKind::Edit,
            BuiltInToolName::ExecuteCmd => ToolKind::Execute,
            BuiltInToolName::ImageRead => ToolKind::Read,
            BuiltInToolName::Ls => ToolKind::Read,
            BuiltInToolName::Summary => ToolKind::Other,
            BuiltInToolName::SpawnSubagent => ToolKind::Other,
            BuiltInToolName::Grep => ToolKind::Search,
            BuiltInToolName::Glob => ToolKind::Search,
            BuiltInToolName::UseAws => ToolKind::Execute,
        }
    } else {
        ToolKind::Other
    }
}

pub(crate) fn get_tool_title(tool: &Tool) -> String {
    match &tool.kind {
        AgentToolKind::BuiltIn(builtin) => match builtin {
            BuiltInTool::FileRead(fs_read) => {
                let files: Vec<_> = fs_read
                    .ops
                    .iter()
                    .map(|op| {
                        let start = op.offset.unwrap_or(0) + 1;
                        match op.limit {
                            Some(limit) => format!("{}:{}-{}", truncate_path(&op.path), start, start + limit - 1),
                            None => format!("{}:{}", truncate_path(&op.path), start),
                        }
                    })
                    .collect();
                format!("Reading {}", files.join(", "))
            },
            BuiltInTool::FileWrite(fs_write) => {
                let action = match fs_write {
                    FsWrite::Create(_) => "Creating",
                    FsWrite::StrReplace(_) | FsWrite::Insert(_) => "Editing",
                };
                format!("{} {}", action, truncate_path(fs_write.path()))
            },
            BuiltInTool::Grep(grep) => {
                let pattern = truncate_str(&grep.pattern, 60);
                match &grep.path {
                    Some(path) => format!("Searching for '{}' in {}", pattern, truncate_path(path)),
                    None => format!("Searching for '{}'", pattern),
                }
            },
            BuiltInTool::Glob(glob) => {
                let pattern = truncate_str(&glob.pattern, 60);
                match &glob.path {
                    Some(path) => format!("Finding {} in {}", pattern, truncate_path(path)),
                    None => format!("Finding {}", pattern),
                }
            },
            BuiltInTool::Ls(ls) => format!("Listing {}", truncate_path(&ls.path)),
            BuiltInTool::ExecuteCmd(cmd) => format!("Running: {}", truncate_str(&cmd.command, 200)),
            BuiltInTool::UseAws(aws) => format!("AWS: {} {}", aws.service_name, aws.operation_name),
            BuiltInTool::ImageRead(img) => {
                let paths: Vec<_> = img.paths.iter().map(|p| p.as_str()).collect();
                format_paths_title("Reading image", &paths)
            },
            BuiltInTool::SpawnSubagent(_) => "Spawning subagent".to_string(),
            BuiltInTool::Summary(_) => "Summarizing".to_string(),
            BuiltInTool::Mkdir(_) => "Creating directory".to_string(),
            BuiltInTool::Introspect(_) => "Introspecting".to_string(),
        },
        AgentToolKind::Mcp(mcp) => format!("Running: @{}/{}", mcp.server_name, mcp.tool_name),
    }
}

fn truncate_str(s: &str, max_len: usize) -> String {
    let mut result = s.to_string();
    agent::util::truncate_safe_in_place(&mut result, max_len, Some("..."));
    result
}

fn truncate_path(path: &str) -> String {
    let p = std::path::Path::new(path);
    p.file_name()
        .map_or_else(|| truncate_str(path, 30), |f| f.to_string_lossy().to_string())
}

fn format_paths_title(action: &str, paths: &[&str]) -> String {
    match paths.len() {
        0 => action.to_string(),
        1 => format!("{} {}", action, truncate_path(paths[0])),
        n => format!("{} {} (+{} more)", action, truncate_path(paths[0]), n - 1),
    }
}

fn get_tool_content(tool: &Tool) -> Vec<ToolCallContent> {
    match &tool.kind {
        AgentToolKind::BuiltIn(BuiltInTool::FileWrite(fs_write)) => {
            let path = fs_write.path();
            let (old_text, new_text) = match fs_write {
                FsWrite::Create(create) => (None, create.content.clone()),
                FsWrite::StrReplace(str_replace) => (Some(str_replace.old_str.clone()), str_replace.new_str.clone()),
                FsWrite::Insert(_) => return vec![],
            };

            vec![ToolCallContent::Diff(Diff::new(path, new_text).old_text(old_text))]
        },
        _ => vec![],
    }
}

fn get_tool_locations(tool: &Tool) -> Option<Vec<ToolCallLocation>> {
    match &tool.kind {
        AgentToolKind::BuiltIn(builtin) => match builtin {
            BuiltInTool::FileRead(fs_read) => Some(
                fs_read
                    .ops
                    .iter()
                    .map(|op| {
                        let mut loc = ToolCallLocation::new(&op.path);
                        if let Some(offset) = op.offset {
                            loc = loc.line(offset + 1); // offset is 0-based, line is 1-based
                        }
                        loc
                    })
                    .collect(),
            ),
            BuiltInTool::FileWrite(fs_write) => {
                let lines = fs_write.start_lines();
                if lines.is_empty() {
                    None
                } else {
                    Some(
                        lines
                            .into_iter()
                            .map(|line| ToolCallLocation::new(fs_write.path()).line(line))
                            .collect(),
                    )
                }
            },
            BuiltInTool::ImageRead(image_read) => Some(image_read.paths.iter().map(ToolCallLocation::new).collect()),
            _ => None,
        },
        AgentToolKind::Mcp(_) => None,
    }
}

/// Update model ID in RTS state.
/// Validates the model ID against available models if specified.
/// Returns Ok(()) if model was set, Err with message if validation failed.
async fn update_model_info(os: &Os, rts_state: &RtsState, model: Option<&str>) -> Result<(), String> {
    let (models, default) = get_available_models(os)
        .await
        .map_err(|e| format!("Failed to fetch available models: {}", e))?;

    let model_info = if let Some(requested_model) = model {
        find_model(&models, requested_model)
            .ok_or_else(|| format!("Model '{}' not found", requested_model))?
            .clone()
    } else {
        default
    };

    rts_state.set_model_info(Some(model_info));
    Ok(())
}

/// Entry point for SACP agent
pub async fn execute(os: &mut Os) -> eyre::Result<ExitCode> {
    use super::extensions::TerminateSessionNotification;

    let resolver = PathResolver::new(os);
    let local_mcp_path = resolver.workspace().mcp_config().ok();
    let global_mcp_path = resolver.global().mcp_config().ok();
    let local_agent_path = resolver.workspace().agents_dir().ok();
    let global_agent_path = resolver.global().agents_dir().ok();

    let session_manager_handle = SessionManager::builder()
        .os(os.clone())
        .local_agent_path(local_agent_path)
        .global_agent_path(global_agent_path)
        .local_mcp_path(local_mcp_path)
        .global_mcp_path(global_mcp_path)
        .spawn();

    // NOTE: It is _extremely_ easy to create a deadlock with sacp (read more about it
    // [here](https://docs.rs/sacp/10.1.0/sacp/concepts/ordering/index.html)). For that reason, it
    // is crucial that nothing we dispatch in these on_* callbacks are long running on the dispatch
    // thread, by which I mean the tasks dispatched here should end as soon as you hand off the
    // request (and not wait for a response). Take a look at
    // [crate::agent::acp::session_manager::SessionManager] for the general flow of request
    // response processing. The TLDR; is the request path and response path are _not_ done on the
    // same task.
    AgentToClient::builder()
        .name("kiro-cli-agent")
        .on_receive_request(
            // TODO: use the InitializeRequest param passed in
            async move |_request: InitializeRequest, request_cx, _cx| {
                request_cx.respond(
                    InitializeResponse::new(ProtocolVersion::LATEST)
                        .agent_capabilities(
                            AgentCapabilities::default()
                                .load_session(true)
                                .prompt_capabilities(PromptCapabilities::default().image(true)),
                        )
                        .agent_info(
                            Implementation::new("Kiro Agent", env!("CARGO_PKG_VERSION").to_string())
                                .title("Kiro Agent"),
                        )
                        .auth_methods(vec![AuthMethod::new("kiro-login", "Kiro Login").description(
                            "Run 'kiro-cli login' in terminal to authenticate. See https://kiro.dev/docs/cli/authentication/",
                        )]),
                )
            },
            sacp::on_receive_request!(),
        )
        .on_receive_request(
            {
                let session_tx = session_manager_handle.clone();
                async move |request: NewSessionRequest, request_cx, cx: JrConnectionCx<AgentToClient>| {
                    let session_id = SessionId::new(Uuid::new_v4().to_string());

                    let config = AcpSessionConfig::new(session_id.to_string(), request.cwd)
                        .mcp_servers(request.mcp_servers);
                    let result = session_tx.start_session(&session_id, config, Some(cx.clone())).await?;

                    let modes = to_session_mode_state(result.current_agent_name, result.available_agents);
                    let models = to_session_model_state(result.current_model_id, result.available_models);

                    // Respond to session/new FIRST (per ACP spec)
                    request_cx.respond(
                        NewSessionResponse::new(session_id.clone())
                            .modes(modes)
                            .models(models),
                    )?;

                    // Send available commands via custom extension notification
                    let commands: Vec<super::schema::AvailableCommand> = TuiCommand::all_commands()
                        .into_iter()
                        .map(|cmd| super::schema::AvailableCommand {
                            name: cmd.name().to_string(),
                            description: cmd.description().to_string(),
                            meta: cmd.meta(),
                        })
                        .collect();

                    let notification = super::schema::CommandsAvailableNotification {
                        session_id: session_id.to_string(),
                        commands,
                    };
                    let _ = cx.send_notification(notification);

                    Ok(())
                }
            },
            sacp::on_receive_request!(),
        )
        .on_receive_request(
            {
                let session_tx = session_manager_handle.clone();
                async move |request: LoadSessionRequest, request_cx, cx: JrConnectionCx<AgentToClient>| {
                    // Convert ACP MCP servers to agent configs
                    let config = AcpSessionConfig::new(request.session_id.to_string(), request.cwd)
                        .load(true)
                        .mcp_servers(request.mcp_servers);
                    match session_tx.start_session(&request.session_id, config, Some(cx.clone())).await {
                        Ok(result) => {
                            // Wait for historical notifications to be sent before responding
                            let _ = result.ready_rx.await;

                            let modes = to_session_mode_state(result.current_agent_name, result.available_agents);
                            let models = to_session_model_state(result.current_model_id, result.available_models);

                            // Respond FIRST
                            request_cx.respond(LoadSessionResponse::new().modes(modes).models(models))?;

                            // Send available commands via custom extension notification
                            let commands: Vec<super::schema::AvailableCommand> = TuiCommand::all_commands()
                                .into_iter()
                                .map(|cmd| super::schema::AvailableCommand {
                                    name: cmd.name().to_string(),
                                    description: cmd.description().to_string(),
                                    meta: cmd.meta(),
                                })
                                .collect();

                            let notification = super::schema::CommandsAvailableNotification {
                                session_id: request.session_id.to_string(),
                                commands,
                            };
                            let _ = cx.send_notification(notification);

                            Ok(())
                        },
                        Err(e) => request_cx.respond_with_error(e),
                    }
                }
            },
            sacp::on_receive_request!(),
        )
        .on_receive_request(
            {
                let session_tx = session_manager_handle.clone();
                async move |request: PromptRequest, request_cx, _cx| match session_tx
                    .get_session_handle(&request.session_id)
                    .await
                {
                    Ok(handle) => handle.handle_prompt(request, request_cx).await,
                    Err(e) => request_cx.respond_with_error(e),
                }
            },
            sacp::on_receive_request!(),
        )
        .on_receive_request(
            {
                let session_tx = session_manager_handle.clone();
                async move |request: sacp::schema::SetSessionModeRequest, request_cx, _cx| match session_tx
                    .set_mode(&request.session_id, request.mode_id.0.to_string())
                    .await
                {
                    Ok(()) => request_cx.respond(sacp::schema::SetSessionModeResponse::default()),
                    Err(e) => request_cx.respond_with_error(e),
                }
            },
            sacp::on_receive_request!(),
        )
        // Handle command execution via typed request
        .on_receive_request(
            {
                let session_tx = session_manager_handle.clone();
                async move |request: super::schema::CommandExecuteRequest, request_cx, _cx| {
                    let session_id = sacp::schema::SessionId::new(request.session_id);
                    match session_tx.get_session_handle(&session_id).await {
                        Ok(handle) => {
                            tokio::spawn(async move {
                                let result = handle.execute_command(request.command).await;
                                if let Err(e) = request_cx.respond(result.into()) {
                                    tracing::error!("Failed to send command response: {}", e);
                                }
                            });
                            Ok(())
                        }
                        Err(e) => request_cx.respond_with_error(e),
                    }
                }
            },
            sacp::on_receive_request!(),
        )
        // Handle command options via typed request
        .on_receive_request(
            {
                let session_tx = session_manager_handle.clone();
                async move |request: super::schema::CommandOptionsRequest, request_cx, _cx| {
                    let session_id = sacp::schema::SessionId::new(request.session_id);
                    match session_tx.get_session_handle(&session_id).await {
                        Ok(handle) => {
                            let opts = handle.get_command_options(request.command, request.partial).await;
                            request_cx.respond(opts.into())
                        }
                        Err(e) => request_cx.respond_with_error(e),
                    }
                }
            },
            sacp::on_receive_request!(),
        )
        .on_receive_message(
            {
                let session_tx = session_manager_handle.clone();
                async move |message: MessageCx, _cx: JrConnectionCx<AgentToClient>| {
                    let method = message.method().to_string();

                    // Handle session/set_model (unstable ACP method)
                    if method == "session/set_model" {
                        let MessageCx::Request(req, req_cx) = message else {
                            return Ok(sacp::Handled::Yes);
                        };
                        let request: sacp::schema::SetSessionModelRequest = serde_json::from_value(req.params().clone())
                            .map_err(|e| sacp::util::internal_error(format!("Invalid request: {}", e)))?;
                        let handle = session_tx.get_session_handle(&request.session_id).await?;
                        handle
                            .set_model(request.model_id.0.to_string())
                            .await
                            .map_err(sacp::util::internal_error)?;
                        req_cx.respond(serde_json::json!({}))?;
                        return Ok(sacp::Handled::Yes);
                    }

                    // Handle extension notifications
                    use super::extensions::methods;
                    if let MessageCx::Notification(notif) = &message {
                        let method = notif.method();

                        match method {
                            methods::SESSION_TERMINATE => {
                                if let Ok(params) =
                                    serde_json::from_value::<TerminateSessionNotification>(notif.params().clone())
                                {
                                    session_tx.terminate_session(&params.session_id).await;
                                    return Ok(sacp::Handled::Yes);
                                }
                            },
                            name if name == AGENT_METHOD_NAMES.session_cancel => {
                                if let Ok(cancel_notif) =
                                    serde_json::from_value::<CancelNotification>(notif.params().clone())
                                {
                                    if let Ok(handle) = session_tx.get_session_handle(&cancel_notif.session_id).await {
                                        let _ = handle.cancel().await;
                                    }
                                    return Ok(sacp::Handled::Yes);
                                }
                            },
                            _ => {},
                        }
                    }
                    // Return unhandled for unknown messages
                    Ok(sacp::Handled::No { message, retry: false })
                }
            },
            sacp::on_receive_message!(),
        )
        .serve(sacp::ByteStreams::new(
            tokio::io::stdout().compat_write(),
            tokio::io::stdin().compat(),
        ))
        .await
        .map_err(|e| eyre::eyre!("Connection error: {}", e))?;

    Ok(ExitCode::SUCCESS)
}

fn to_session_mode_state(current: String, agents: Vec<AgentInfo>) -> SessionModeState {
    let modes = agents
        .into_iter()
        .map(|agent| {
            let mut mode = SessionMode::new(agent.name.clone(), agent.name);
            if let Some(desc) = agent.description {
                mode = mode.description(desc);
            }
            mode
        })
        .collect();
    SessionModeState::new(current, modes)
}

fn to_session_model_state(current: String, models: Vec<ModelInfo>) -> SessionModelState {
    let acp_models = models
        .into_iter()
        .map(|m| {
            let mut info = AcpModelInfo::new(m.model_id.clone(), m.model_name.unwrap_or(m.model_id));
            if let Some(desc) = m.description {
                info = info.description(desc);
            }
            info
        })
        .collect();
    SessionModelState::new(current, acp_models)
}

fn mime_to_image_format(mime: &str) -> Option<ImageFormat> {
    match mime {
        "image/png" => Some(ImageFormat::Png),
        "image/jpeg" | "image/jpg" => Some(ImageFormat::Jpeg),
        "image/gif" => Some(ImageFormat::Gif),
        "image/webp" => Some(ImageFormat::Webp),
        _ => None,
    }
}
