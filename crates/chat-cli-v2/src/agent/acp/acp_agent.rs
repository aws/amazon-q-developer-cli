use std::borrow::Cow;
use std::collections::HashMap;
use std::path::PathBuf;
use std::process::ExitCode;
use std::str::FromStr;
use std::sync::Arc;

use agent::agent_config::LoadedAgentConfig;
use agent::agent_loop::model::Model;
use agent::agent_loop::protocol::{
    AgentLoopEventKind,
    LoopError,
};
use agent::agent_loop::types::{
    ContentBlock as AgentContentBlock,
    ImageBlock,
    ImageFormat,
    ImageSource,
    StreamErrorKind,
};
use agent::event_log::{
    LogEntry,
    LogEntryV1,
};
use agent::mcp::types::Prompt;
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
    InternalEvent,
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
use agent::util::path::canonicalize_path_sys;
use agent::util::providers::{
    RealProvider,
    SystemProvider,
};
use agent::{
    Agent,
    AgentHandle,
};
use code_agent_sdk::CodeIntelligence;
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
    McpCapabilities,
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
    RwLock,
    mpsc,
    oneshot,
};
use tokio_util::compat::TokioAsyncWriteCompatExt;
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
    ExtSessionUpdate,
    ExtSessionUpdateNotification,
    McpOauthRequestNotification,
    McpServerInitFailureNotification,
    McpServerInitializedNotification,
    RateLimitErrorNotification,
    SubagentInfo,
    methods,
};
use super::slash_router;
use super::subagent_tool::{
    InternalPromptError,
    handle_internal_prompt,
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
use crate::agent::session::legacy_compat::LegacySessionExporter;
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
use crate::telemetry::observer::{
    AcpClientInfo,
    TelemetryContext,
    TelemetryObserver,
    TelemetryObserverHandle,
};
use crate::util::consts::env_var::KIRO_TEST_MODE;
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
        respond_to: oneshot::Sender<Result<Summary, InternalPromptError>>,
    },
    /// Lightweight wake — sends a prompt and waits for turn to end.
    /// Unlike InternalPrompt, does NOT require a Summary tool call.
    Wake {
        message: String,
        respond_to: oneshot::Sender<eyre::Result<()>>,
    },
    /// Swap to a different agent configuration (e.g., switching modes).
    SwapAgent {
        agent_config: Box<agent::agent_config::LoadedAgentConfig>,
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
    /// Get MCP prompts from all servers.
    GetMcpPrompts {
        respond_to: oneshot::Sender<Result<HashMap<String, Vec<Prompt>>, String>>,
    },
    /// Get file-based prompts from .kiro/prompts/ directories.
    GetFilePrompts {
        respond_to: oneshot::Sender<Result<HashMap<String, Vec<Prompt>>, String>>,
    },
    /// Get a specific MCP prompt with arguments.
    GetMcpPrompt {
        name: String,
        arguments: HashMap<String, String>,
        respond_to: oneshot::Sender<Result<Vec<serde_json::Value>, String>>,
    },
    /// Get the agent handle for this session.
    GetAgentHandle {
        respond_to: oneshot::Sender<agent::AgentHandle>,
    },
    /// Send an extension notification to the TUI client.
    SendExtNotification { method: String, params: serde_json::Value },
    /// Get tool info for advertising.
    GetToolInfo {
        respond_to: oneshot::Sender<Result<Vec<agent::tui_commands::ToolInfo>, String>>,
    },
    /// Get MCP server info for advertising.
    GetMcpServerInfo {
        respond_to: oneshot::Sender<Result<Vec<agent::tui_commands::McpServerInfo>, String>>,
    },
    /// Graceful shutdown: terminate the agent and await MCP cleanup.
    Shutdown { respond_to: oneshot::Sender<()> },
    /// Trigger command/prompt advertising to the client.
    AdvertiseCommands,
    /// Queue an MCP server refresh with updated registry data.
    /// The actual swap happens in the event loop when the session is idle.
    RefreshMcpServers {
        agent_config: Box<agent::agent_config::LoadedAgentConfig>,
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
    pub async fn internal_prompt(&self, query: String) -> Result<Summary, InternalPromptError> {
        let (respond_to, rx) = oneshot::channel::<Result<Summary, InternalPromptError>>();
        self.tx
            .send(AcpSessionRequest::InternalPrompt { query, respond_to })
            .await
            .map_err(|e| InternalPromptError::Failed(format!("Channel send error: {e}")))?;
        rx.await
            .map_err(|e| InternalPromptError::Failed(format!("Channel recv error: {e}")))?
    }

    /// Lightweight wake — sends a prompt without requiring Summary response.
    /// Use this for interactive chat with persistent sessions.
    pub async fn wake_session(&self, message: String) -> eyre::Result<()> {
        let (respond_to, rx) = oneshot::channel::<eyre::Result<()>>();
        self.tx.send(AcpSessionRequest::Wake { message, respond_to }).await?;
        rx.await?
    }

    /// Swap to a different agent configuration
    pub async fn swap_agent(
        &self,
        agent_config: agent::agent_config::LoadedAgentConfig,
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

    /// Queue an MCP server refresh with updated registry data.
    /// The session will apply it when idle.
    pub async fn refresh_mcp_servers(
        &self,
        agent_config: agent::agent_config::LoadedAgentConfig,
    ) -> Result<(), sacp::Error> {
        self.tx
            .send(AcpSessionRequest::RefreshMcpServers {
                agent_config: agent_config.into(),
            })
            .await
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

    /// Get the agent handle for this session
    pub async fn get_agent_handle(&self) -> Option<agent::AgentHandle> {
        let (respond_to, rx) = oneshot::channel();
        if self
            .tx
            .send(AcpSessionRequest::GetAgentHandle { respond_to })
            .await
            .is_err()
        {
            return None;
        }
        rx.await.ok()
    }

    /// Send an extension notification to the TUI client for this session.
    pub async fn send_ext_notification_raw(&self, method: String, params: serde_json::Value) {
        let _ = self
            .tx
            .send(AcpSessionRequest::SendExtNotification { method, params })
            .await;
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

    /// Get MCP prompts from all servers
    pub async fn get_mcp_prompts(&self) -> Result<HashMap<String, Vec<Prompt>>, String> {
        let (respond_to, rx) = oneshot::channel();
        if self
            .tx
            .send(AcpSessionRequest::GetMcpPrompts { respond_to })
            .await
            .is_err()
        {
            return Err("Channel closed".to_string());
        }
        rx.await
            .map_err(|e| format!("Response channel closed: {e}").to_string())?
    }

    /// Get file-based prompts from .kiro/prompts/ directories
    pub async fn get_file_prompts(&self) -> Result<HashMap<String, Vec<Prompt>>, String> {
        let (respond_to, rx) = oneshot::channel();
        if self
            .tx
            .send(AcpSessionRequest::GetFilePrompts { respond_to })
            .await
            .is_err()
        {
            return Err("Channel closed".to_string());
        }
        rx.await
            .map_err(|e| format!("Response channel closed: {e}").to_string())?
    }

    /// Get a specific MCP prompt with arguments
    pub async fn get_mcp_prompt(
        &self,
        name: String,
        arguments: HashMap<String, String>,
    ) -> Result<Vec<serde_json::Value>, String> {
        let (respond_to, rx) = oneshot::channel();
        if self
            .tx
            .send(AcpSessionRequest::GetMcpPrompt {
                name,
                arguments,
                respond_to,
            })
            .await
            .is_err()
        {
            return Err("Channel closed".to_string());
        }
        rx.await
            .map_err(|e| format!("Response channel closed: {e}").to_string())?
    }

    pub async fn get_tool_info(&self) -> Result<Vec<agent::tui_commands::ToolInfo>, String> {
        let (respond_to, rx) = oneshot::channel();
        if self
            .tx
            .send(AcpSessionRequest::GetToolInfo { respond_to })
            .await
            .is_err()
        {
            return Err("Channel closed".to_string());
        }
        rx.await
            .map_err(|e| format!("Response channel closed: {e}").to_string())?
    }

    pub async fn get_mcp_server_info(&self) -> Result<Vec<agent::tui_commands::McpServerInfo>, String> {
        let (respond_to, rx) = oneshot::channel();
        if self
            .tx
            .send(AcpSessionRequest::GetMcpServerInfo { respond_to })
            .await
            .is_err()
        {
            return Err("Channel closed".to_string());
        }
        rx.await
            .map_err(|e| format!("Response channel closed: {e}").to_string())?
    }

    /// Gracefully shut down this session, awaiting MCP server cleanup.
    pub async fn shutdown(&self) {
        let (respond_to, rx) = oneshot::channel();
        if self.tx.send(AcpSessionRequest::Shutdown { respond_to }).await.is_err() {
            return;
        }
        _ = rx.await;
    }

    /// Fire-and-forget: tell the session to advertise commands/prompts to the client.
    pub async fn advertise_commands(&self) {
        let _ = self.tx.send(AcpSessionRequest::AdvertiseCommands).await;
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
    /// When true, all tool permission checks are bypassed
    pub trust_all_tools: bool,
    /// ACP client identity from InitializeRequest
    pub acp_client_info: Option<AcpClientInfo>,
    pub subagent_info: Option<SubagentInfo>,
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
            trust_all_tools: false,
            acp_client_info: None,
            subagent_info: None,
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

    pub fn subagent_info(mut self, info: Option<SubagentInfo>) -> Self {
        self.subagent_info = info;
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
    code_intelligence: Option<Arc<RwLock<CodeIntelligence>>>,
    available_agents: Vec<super::session_manager::AgentInfo>,
    agent_configs: Vec<LoadedAgentConfig>,
    current_agent_name: Option<String>,
    trust_all_tools: bool,
    trust_tools: Option<Vec<String>>,
    acp_client_info: Option<AcpClientInfo>,
    /// Telemetry event store for recording events in test scenarios. `None` in production.
    telemetry_event_store: Option<crate::agent::ipc_server::TelemetryEventStore>,
    subagent_info: Option<SubagentInfo>,
    legacy_session_exporter: Option<Arc<dyn LegacySessionExporter>>,
    session_injected_mcp_servers: Vec<(String, agent::agent_config::definitions::McpServerConfig)>,
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
        self.initial_agent_config = Some(agent_config);
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

    pub fn code_intelligence(mut self, client: Option<Arc<RwLock<CodeIntelligence>>>) -> Self {
        self.code_intelligence = client;
        self
    }

    pub fn available_agents(mut self, agents: Vec<super::session_manager::AgentInfo>) -> Self {
        self.available_agents = agents;
        self
    }

    pub fn agent_configs(mut self, configs: Vec<LoadedAgentConfig>) -> Self {
        self.agent_configs = configs;
        self
    }

    pub fn current_agent_name(mut self, name: String) -> Self {
        self.current_agent_name = Some(name);
        self
    }

    pub fn trust_all_tools(mut self, trust: bool) -> Self {
        self.trust_all_tools = trust;
        self
    }

    pub fn trust_tools(mut self, tools: Option<Vec<String>>) -> Self {
        self.trust_tools = tools;
        self
    }

    pub fn acp_client_info(mut self, info: Option<AcpClientInfo>) -> Self {
        self.acp_client_info = info;
        self
    }

    pub fn telemetry_event_store(mut self, store: Option<crate::agent::ipc_server::TelemetryEventStore>) -> Self {
        self.telemetry_event_store = store;
        self
    }

    pub fn subagent_info(mut self, info: Option<SubagentInfo>) -> Self {
        self.subagent_info = info;
        self
    }

    pub fn legacy_session_exporter(mut self, exporter: Arc<dyn LegacySessionExporter>) -> Self {
        self.legacy_session_exporter = Some(exporter);
        self
    }

    pub fn session_injected_mcp_servers(
        mut self,
        servers: Vec<(String, agent::agent_config::definitions::McpServerConfig)>,
    ) -> Self {
        self.session_injected_mcp_servers = servers;
        self
    }

    /// Spawns a new ACP session actor and returns a handle to communicate with it.
    ///
    /// The returned `ready_rx` resolves after historical notifications have been emitted
    /// (for loaded sessions) and the session is ready to accept prompts.
    ///
    /// Returns (handle, ready_rx, initial_model_id) where initial_model_id is the model
    /// set during session creation (avoids race condition with channel-based query).
    pub async fn start_session(
        mut self,
    ) -> eyre::Result<(AcpSessionHandle, oneshot::Receiver<()>, Option<String>, Option<String>)> {
        let os = self.os.take().ok_or_else(|| eyre::eyre!("Os is required"))?;

        let (tx, rx) = mpsc::channel(32);
        let (ready_tx, ready_rx) = oneshot::channel();
        let subagent_info = self.subagent_info.clone();
        let session = AcpSession::with_builder(os, rx, self).await?;
        let initial_model_id = session.rts_state.model_id();
        let requested_model_name = session.requested_model_name.clone();
        tokio::spawn(async move { session.main_loop(ready_tx).await });

        Ok((
            AcpSessionHandle {
                tx: InnerSender::Strong(tx),
                _subagent_info: subagent_info,
            },
            ready_rx,
            initial_model_id,
            requested_model_name,
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
    session_id_str: String,
    agent: AgentHandle,
    request_rx: mpsc::Receiver<AcpSessionRequest>,
    session_db: Arc<SessionDb>,
    rts_state: Arc<RtsState>,
    api_client: ApiClient,
    session_tx: SessionManagerHandle,
    available_agents: Vec<super::session_manager::AgentInfo>,
    agent_configs: Vec<LoadedAgentConfig>,
    local_mcp_path: Option<PathBuf>,
    global_mcp_path: Option<PathBuf>,
    current_agent_name: String,
    /// Connection to the TUI client
    connection_cx: JrConnectionCx<AgentToClient>,
    is_subagent: bool,
    previous_agent_name: Option<String>,
    /// The model name originally requested (before fallback). `None` if no
    /// specific model was requested or the requested model was found.
    requested_model_name: Option<String>,
    pending_plan: Option<String>,
    pending_swap: Option<agent::agent_config::LoadedAgentConfig>,
    pending_prompt_response: Option<tokio::sync::Mutex<JrRequestCx<PromptResponse>>>,
    /// Agent config to swap to when the session becomes idle (set by registry refresh)
    pending_mcp_refresh: Option<Box<agent::agent_config::LoadedAgentConfig>>,
    compaction_summary: Option<String>,
    os: Os,
    cwd: PathBuf,
    telemetry_observer: TelemetryObserverHandle,
    legacy_session_exporter: Arc<dyn LegacySessionExporter>,
    /// MCP servers injected by the ACP client at session creation time.
    /// Preserved across agent swaps so they are re-merged into each new agent config.
    session_injected_mcp_servers: Vec<(String, agent::agent_config::definitions::McpServerConfig)>,
}

impl AcpSession {
    fn welcome_message_for(&self, agent_name: &str) -> Option<String> {
        self.available_agents
            .iter()
            .find(|a| a.name == agent_name)
            .and_then(|a| a.welcome_message.clone())
    }

    /// Re-merge session-injected MCP servers into an agent config.
    /// Called before every agent swap so ACP-provided servers survive mode changes.
    fn merge_session_mcp_servers(&self, config: &mut LoadedAgentConfig) {
        if !self.session_injected_mcp_servers.is_empty() {
            config
                .config_mut()
                .add_mcp_servers(self.session_injected_mcp_servers.clone());
        }
    }

    /// Reload available agents from disk (e.g. after agent create)
    async fn reload_available_agents(&mut self) {
        use agent::agent_config::load_agents;

        // Use a provider that matches the session's cwd
        let provider = super::acp_provider::AcpProvider::new(self.cwd.clone());

        match load_agents(&provider).await {
            Ok((configs, _errors)) => {
                let mut new_agents: Vec<super::session_manager::AgentInfo> = configs
                    .iter()
                    .map(|c| super::session_manager::AgentInfo {
                        name: c.name().to_string(),
                        description: c.config().description().map(|s| s.to_string()),
                        source: match c.source() {
                            agent::agent_config::ConfigSource::Workspace { .. } => "Workspace".to_string(),
                            agent::agent_config::ConfigSource::Global { .. } => "Global".to_string(),
                            agent::agent_config::ConfigSource::BuiltIn => "Built-in".to_string(),
                            agent::agent_config::ConfigSource::Ephemeral => "".to_string(),
                        },
                        welcome_message: c.config().welcome_message().map(|s| s.to_string()),
                    })
                    .collect();
                let mut seen = std::collections::HashSet::new();
                new_agents.retain(|a| seen.insert(a.name.clone()));
                info!(
                    count = new_agents.len(),
                    names = ?new_agents.iter().map(|a| &a.name).collect::<Vec<_>>(),
                    "Reloaded available agents after create"
                );
                self.available_agents = new_agents;
                self.agent_configs = configs;
            },
            Err(e) => {
                warn!("Failed to reload agents after create: {}", e);
            },
        }
    }

    /// Create a CommandContext from the current session state
    fn command_context(&self) -> super::commands::CommandContext<'_> {
        super::commands::CommandContext {
            api_client: &self.api_client,
            rts_state: &self.rts_state,
            agent: &self.agent,
            session_tx: &self.session_tx,
            available_agents: &self.available_agents,
            agent_configs: &self.agent_configs,
            local_mcp_path: self.local_mcp_path.as_ref(),
            global_mcp_path: self.global_mcp_path.as_ref(),
            session_id: &self.session_id_str,
            current_agent_name: &self.current_agent_name,
            previous_agent_name: self.previous_agent_name.as_deref(),
            os: &self.os,
            cwd: &self.cwd,
            legacy_session_exporter: &self.legacy_session_exporter,
            session_injected_mcp_servers: &self.session_injected_mcp_servers,
        }
    }

    /// Persist the current session state (conversation, model, permissions, agent name) to disk.
    async fn persist_session_state(&self) {
        match self.agent.create_snapshot().await {
            Ok(snapshot) => {
                let mut state = SessionState::new(
                    snapshot.conversation_metadata,
                    self.rts_state.snapshot(),
                    snapshot.permissions,
                );
                state.set_agent_name(self.current_agent_name.clone());
                if let Err(e) = self.session_db.update_state(state) {
                    warn!("Failed to persist session state: {}", e);
                }
            },
            Err(e) => {
                error!("Failed to get agent snapshot for session persistence: {}", e);
            },
        }
    }

    async fn with_builder(
        os: Os,
        request_rx: mpsc::Receiver<AcpSessionRequest>,
        builder: AcpSessionBuilder<'_>,
    ) -> eyre::Result<Self> {
        let session_id_str = builder
            .session_id
            .ok_or_else(|| eyre::eyre!("session_id is required"))?;
        let cwd = builder.cwd.ok_or_else(|| eyre::eyre!("cwd is required"))?;
        let initial_agent_config = builder
            .initial_agent_config
            .ok_or_else(|| eyre::eyre!("initial_agent_config is required"))?
            .into_owned();
        let session_tx = builder.session_tx.expect("Missing session request sender");
        let connection_cx = builder.client_cx.expect("Missing client connection");

        // Determine if loading existing session or creating new one
        // Track the model ID from a loaded session so we can restore it below
        let mut saved_model_id: Option<String> = None;

        let (session_db, snapshot) = if builder.load {
            // Load existing session
            let db = SessionDb::load(&session_id_str, Some(&cwd))?;
            let state = db.session().session_state;
            let entries = db.load_log_entries()?;

            // Preserve the model the session was actually using (e.g. after /model switch)
            saved_model_id = state
                .rts_model_state()
                .and_then(|s| s.model_info.as_ref().map(|m| m.model_id.clone()));

            let conversation_id = Uuid::parse_str(&session_id_str)
                .map_err(|_e| eyre::eyre!("Invalid session ID '{}': must be a valid UUID", session_id_str))?;
            let conversation_state = ConversationState::new(conversation_id, entries);
            let snapshot = AgentSnapshot {
                agent_config: initial_agent_config,
                conversation_state,
                conversation_metadata: state.conversation_metadata().cloned().unwrap_or_default(),
                permissions: state
                    .permissions()
                    .cloned()
                    .unwrap_or_default()
                    .with_cwd(&cwd.to_string_lossy()),
                ..Default::default()
            };

            (db, snapshot)
        } else {
            // Create new session
            let conversation_id = Uuid::parse_str(&session_id_str)
                .map_err(|_e| eyre::eyre!("Invalid session ID '{}': must be a valid UUID", session_id_str))?;
            let permissions = RuntimePermissions::default().with_cwd(&cwd.to_string_lossy());
            let snapshot = AgentSnapshot {
                agent_config: initial_agent_config,
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

        // Build API client, using mock registry in test mode
        let (api_client, model): (ApiClient, Arc<dyn Model>) = if let Some(registry) = builder.mock_registry {
            let client = ApiClient::new_ipc_mock(registry);
            (client.clone(), Arc::new(RtsModel::new(client, Arc::clone(&rts_state))))
        } else {
            let client = os.client.clone();
            (client.clone(), Arc::new(RtsModel::new(client, Arc::clone(&rts_state))))
        };

        // Set model ID from agent config with validation
        if let Err(e) = update_model_info(&api_client, &os.database, &rts_state, snapshot.agent_config.model()).await {
            warn!("Failed to set initial model: {}", e);
        }

        // Restore the model the loaded session was actually using (e.g. after /model switch)
        if let Some(ref model_id) = saved_model_id
            && let Err(e) = update_model_info(&api_client, &os.database, &rts_state, Some(model_id)).await
        {
            warn!("Failed to restore saved session model: {}", e);
        }

        // Override with CLI --model if provided
        let model_not_found = if let Some(model_id) = builder.model_id {
            match update_model_info(&api_client, &os.database, &rts_state, Some(model_id)).await {
                Ok(not_found) => not_found,
                Err(e) => {
                    warn!("Failed to set CLI model override: {}", e);
                    None
                },
            }
        } else {
            None
        };

        let snapshot = {
            let mut s = snapshot;
            s.settings.trust_all_tools = builder.trust_all_tools;
            if let Some(tools) = builder.trust_tools {
                for tool in &tools {
                    if !tool.starts_with('@') && tool.parse::<agent::tools::BuiltInToolName>().is_err() {
                        warn!(
                            "--trust-tools: custom tool '{}' should be prefixed with @{{MCPSERVERNAME}}/",
                            tool
                        );
                    }
                }
                s.agent_config.allowed_tools_mut().extend(tools);
            }
            s
        };

        // Skip knowledge provider in test mode — ensure_models_downloaded fetches embedding
        // models to HOME, which downloads a large amount of data (10+ seconds).
        let knowledge_provider: Option<std::sync::Arc<dyn agent::tools::KnowledgeProvider>> =
            if std::env::var(KIRO_TEST_MODE).is_ok() {
                None
            } else {
                let name = builder.current_agent_name.as_deref().unwrap_or_default();
                let agent_config = builder.agent_configs.iter().find(|c| c.name() == name);
                let agent_path = agent_config.and_then(|c| match c.source() {
                    agent::agent_config::ConfigSource::Workspace { path }
                    | agent::agent_config::ConfigSource::Global { path } => Some(path.clone()),
                    _ => None,
                });
                match crate::util::knowledge_store::KnowledgeStore::get_async_instance(
                    &os,
                    Some(name),
                    agent_path.as_deref(),
                )
                .await
                {
                    Ok(store) => Some(std::sync::Arc::new(
                        crate::util::knowledge_store::KnowledgeStoreProvider::new(store),
                    )),
                    Err(_) => None,
                }
            };

        let mut agent = Agent::new(
            snapshot,
            builder.local_mcp_path,
            builder.global_mcp_path,
            model,
            McpManager::default().spawn(),
            builder.is_subagent,
            builder.code_intelligence,
            knowledge_provider,
            if builder.is_subagent {
                None
            } else {
                Some(std::sync::Arc::new(agent::tools::task::store::TaskStore::new(
                    &session_id_str,
                )))
            },
            builder.agent_configs.clone(),
        )
        .await?;

        agent.set_sys_provider(super::acp_provider::AcpProvider::new(cwd.clone()));

        if let Some(msg) = builder.user_embedded_msg {
            agent.prepend_embedded_user_msg(msg);
        }

        let agent = agent.spawn();

        // Create telemetry observer actor
        let telemetry_context = TelemetryContext::new(Arc::clone(&rts_state), builder.acp_client_info.clone());
        let telemetry_observer = TelemetryObserver::spawn(
            telemetry_context,
            os.telemetry.clone(),
            os.database.clone(),
            builder.telemetry_event_store,
        );

        Ok(Self {
            session_id: SessionId::new(session_id_str.clone()),
            session_id_str,
            agent,
            request_rx,
            session_tx,
            available_agents: builder.available_agents,
            agent_configs: builder.agent_configs,
            local_mcp_path: builder.local_mcp_path.cloned(),
            global_mcp_path: builder.global_mcp_path.cloned(),
            current_agent_name: builder.current_agent_name.unwrap_or_default(),
            previous_agent_name: None,
            requested_model_name: model_not_found,
            pending_plan: None,
            pending_swap: None,
            connection_cx,
            is_subagent: builder.is_subagent,
            session_db: Arc::new(session_db),
            rts_state,
            api_client,
            pending_prompt_response: None,
            pending_mcp_refresh: None,
            compaction_summary: None,
            os,
            cwd,
            telemetry_observer,
            legacy_session_exporter: builder
                .legacy_session_exporter
                .unwrap_or_else(|| Arc::new(crate::agent::session::legacy_compat::NoOpLegacySessionExporter)),
            session_injected_mcp_servers: builder.session_injected_mcp_servers,
        })
    }

    async fn initialize(&mut self) -> eyre::Result<()> {
        // Emit historical notifications for loaded sessions
        if let Err(e) = self.emit_historical_notifications().await {
            warn!("Failed to emit historical notifications: {}", e);
        }

        // Wait for agent to finish initialization
        loop {
            match self.agent.recv().await {
                Ok(AgentEvent::Initialized) => {
                    // Emit initial context usage so TUI shows it immediately
                    self.emit_initial_context_usage().await;
                    return Ok(());
                },
                Ok(AgentEvent::InitializeUpdate(init_event)) => {
                    let agent::protocol::InitializeUpdateEvent::Mcp(mcp_event) = init_event;
                    if let Err(e) = self.handle_mcp_event(mcp_event).await {
                        error!("Failed to handle MCP event during initialization: {}", e);
                    }
                },
                Ok(event) => {
                    warn!("Unexpected event during initialization: {:?}", event);
                },
                Err(_) => {
                    return Err(eyre::eyre!("Agent channel closed during initialization"));
                },
            }
        }
    }

    async fn main_loop(mut self, ready_tx: oneshot::Sender<()>) {
        if let Err(e) = self.initialize().await {
            error!("Failed to initialize session: {}", e);
            return;
        }
        let _ = ready_tx.send(());

        loop {
            // Apply pending MCP registry refresh when idle (no prompt in progress)
            if self.pending_prompt_response.is_none()
                && let Some(agent_config) = self.pending_mcp_refresh.take()
            {
                let resolver = crate::util::paths::PathResolver::new(&self.os);
                let local_mcp_path = resolver.workspace().mcp_config().ok();
                let global_mcp_path = resolver.global().mcp_config().ok();
                if let Err(e) = self
                    .agent
                    .swap_agent(agent::protocol::SwapAgentArgs {
                        agent_config: *agent_config,
                        local_mcp_path,
                        global_mcp_path,
                        force: true,
                    })
                    .await
                {
                    warn!(%e, "Failed to apply pending MCP registry refresh");
                }
            }

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
                            error!("Agent event channel closed, exiting");
                            break;
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
        let metering = if metadata.metering_usage.is_empty() {
            None
        } else {
            Some(metadata.metering_usage.clone())
        };
        let notification = super::schema::MetadataNotification {
            session_id: self.session_id_str.clone(),
            context_usage_percentage: metadata.context_usage_percentage,
            metering_usage: metering,
            turn_duration_ms: metadata.turn_duration.map(|d| d.as_millis() as u64),
        };
        self.connection_cx.send_notification(notification)
    }

    async fn emit_initial_context_usage(&self) {
        let snapshot = match self.agent.create_snapshot().await {
            Ok(s) => s,
            Err(_) => return,
        };
        let context_window = self
            .rts_state
            .model_info()
            .map_or(super::commands::context::DEFAULT_CONTEXT_WINDOW_TOKENS, |m| {
                m.context_window_tokens
            });
        let sizes = super::commands::context::calculate_component_sizes(&snapshot);
        let total_tokens = sizes.context_files + sizes.tools + sizes.kiro + sizes.user + sizes.system;
        let estimated_pct = (total_tokens as f32 / context_window as f32) * 100.0;

        tracing::debug!(
            context_files = sizes.context_files,
            tools = sizes.tools,
            kiro = sizes.kiro,
            user = sizes.user,
            system = sizes.system,
            total_tokens,
            context_window,
            estimated_pct,
            tool_specs_count = snapshot.tool_specs.len(),
            "emit_initial_context_usage"
        );

        let notification = super::schema::MetadataNotification {
            session_id: self.session_id_str.clone(),
            context_usage_percentage: Some(estimated_pct),
            metering_usage: None,
            turn_duration_ms: None,
        };
        if let Err(e) = self.connection_cx.send_notification(notification) {
            warn!("Failed to send initial context usage: {}", e);
        }
    }

    async fn emit_historical_notifications(&self) -> Result<(), sacp::Error> {
        let entries = self
            .session_db
            .load_log_entries()
            .map_err(|e| sacp::util::internal_error(format!("Failed to load log entries: {}", e)))?;

        let mut pending_tool_call_ids: Vec<String> = Vec::new();

        for entry in &entries {
            // Track tool calls that start (from AssistantMessage) and finish (from ToolResults)
            match entry {
                LogEntry::V1(LogEntryV1::AssistantMessage { content, .. }) => {
                    for block in content {
                        if let AgentContentBlock::ToolUse(tool_use) = block {
                            pending_tool_call_ids.push(tool_use.tool_use_id.clone());
                        }
                    }
                },
                LogEntry::V1(LogEntryV1::ToolResults { results, .. }) => {
                    pending_tool_call_ids.retain(|id| !results.contains_key(id));
                },
                LogEntry::V1(LogEntryV1::Compaction { .. } | LogEntryV1::Clear) => {
                    pending_tool_call_ids.clear();
                },
                LogEntry::V1(LogEntryV1::Prompt { .. } | LogEntryV1::ResetTo { .. } | LogEntryV1::CancelledPrompt) => {
                },
            }

            for update in log_entry_to_session_updates(entry) {
                self.send_session_notification(update)?;
            }
        }

        // Emit failed status for any tool calls that were never completed
        // (e.g. session was killed mid-execution)
        for tool_call_id in pending_tool_call_ids {
            self.send_session_notification(SessionUpdate::ToolCallUpdate(ToolCallUpdate::new(
                ToolCallId::new(tool_call_id),
                ToolCallUpdateFields::new().status(ToolCallStatus::Failed),
            )))?;
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

                // Check for slash command
                if let Some(route) = slash_router::parse(&request.prompt) {
                    match route {
                        slash_router::SlashRoute::Action(command) => {
                            let is_agent_swap = matches!(&command, TuiCommand::Agent(args) if args.agent_name.is_some())
                                || matches!(&command, TuiCommand::Guide(_));
                            let is_agent_create = matches!(&command, TuiCommand::Agent(args) if args.agent_name.as_deref().is_some_and(|n| n == "create" || n.starts_with("create ")));
                            let ctx = self.command_context();
                            let result = super::commands::execute(command, &ctx).await;

                            // Mirror ExecuteCommand: update current_agent_name on successful swap
                            if is_agent_swap
                                && result.success
                                && let Some(data) = &result.data
                                && let Some(name) =
                                    data.get("agent").and_then(|a| a.get("name")).and_then(|n| n.as_str())
                            {
                                self.previous_agent_name =
                                    Some(std::mem::replace(&mut self.current_agent_name, name.to_string()));

                                // Notify TUI so it updates the displayed agent name
                                let _ = self.send_ext_notification(
                                    super::extensions::methods::AGENT_SWITCHED,
                                    super::extensions::AgentSwitchedNotification {
                                        session_id: self.session_id.clone(),
                                        agent_name: self.current_agent_name.clone(),
                                        previous_agent_name: self.previous_agent_name.clone(),
                                        welcome_message: self.welcome_message_for(&self.current_agent_name),
                                    },
                                );

                                if let Err(e) = self.advertise_commands_and_prompts().await {
                                    warn!("Failed to advertise commands after slash agent swap: {}", e);
                                }
                            }

                            // Reload available agents after a successful agent create
                            if is_agent_create && result.success {
                                self.reload_available_agents().await;
                                if let Err(e) = self.advertise_commands_and_prompts().await {
                                    warn!("Failed to advertise commands after agent create: {}", e);
                                }
                            }

                            // Send result message as a text chunk so the TUI displays it
                            if !result.message.is_empty() {
                                let _ = self.send_session_notification(SessionUpdate::AgentMessageChunk(
                                    SacpContentChunk::new(ContentBlock::Text(TextContent::new(result.message))),
                                ));
                            }

                            // Respond directly — slash commands don't go through the agent loop
                            // so EndTurn never fires. Commands that also call send_prompt (e.g.
                            // /plan) will have their agent turn run in the background independently.
                            if let Err(e) = request_cx.respond(PromptResponse::new(StopReason::EndTurn)) {
                                error!("Failed to respond to slash command: {e}");
                            }
                        },
                        slash_router::SlashRoute::Prompt { name, args } => {
                            self.pending_prompt_response = Some(tokio::sync::Mutex::new(request_cx));
                            let agent = self.agent.clone();
                            let cwd = self.cwd.clone();
                            tokio::spawn(async move {
                                // Priority: local file > global file > MCP (per docs)
                                if let Some(content) = agent::prompts::resolve_file_prompt(&cwd, &name) {
                                    let template = agent::prompts::PromptTemplateArgs::parse(&content);
                                    let expanded = template.expand(&content, &args);
                                    let _ = agent
                                        .send_prompt(SendPromptArgs {
                                            content: vec![agent::protocol::ContentChunk::Text(expanded)],
                                            should_continue_turn: None,
                                        })
                                        .await;
                                } else {
                                    // Fall back to MCP prompt
                                    let mcp_args = slash_router::args_to_mcp_map(&args);
                                    match agent.get_mcp_prompt(name.clone(), mcp_args).await {
                                        Ok(messages) => {
                                            let resolved_text = slash_router::extract_prompt_text(&messages);
                                            if !resolved_text.is_empty() {
                                                let _ = agent
                                                    .send_prompt(SendPromptArgs {
                                                        content: vec![agent::protocol::ContentChunk::Text(
                                                            resolved_text,
                                                        )],
                                                        should_continue_turn: None,
                                                    })
                                                    .await;
                                            }
                                        },
                                        Err(_) => {
                                            // Not a known prompt - send as regular message
                                            let _ = agent
                                                .send_prompt(SendPromptArgs {
                                                    content: vec![agent::protocol::ContentChunk::Text(format!(
                                                        "/{}",
                                                        name
                                                    ))],
                                                    should_continue_turn: None,
                                                })
                                                .await;
                                        },
                                    }
                                }
                            });
                        },
                    }
                    return;
                }

                // Normal prompt - no slash command
                let agent = self.agent.clone();
                tokio::spawn(async move {
                    if let Err(e) = handle_prompt_request(request, agent).await {
                        error!("Failed to handle prompt request: {e}");
                    }
                });

                self.pending_prompt_response = Some(tokio::sync::Mutex::new(request_cx));
            },
            AcpSessionRequest::InternalPrompt { query, respond_to } => {
                let agent = self.agent.clone();

                tokio::spawn(async move {
                    let result = handle_internal_prompt(query, agent).await;
                    let _ = respond_to.send(result);
                });
            },
            AcpSessionRequest::Wake { message, respond_to } => {
                let agent = self.agent.clone();
                tokio::spawn(async move {
                    let result = agent
                        .send_prompt(agent::protocol::SendPromptArgs {
                            content: vec![agent::protocol::ContentChunk::Text(message)],
                            should_continue_turn: None,
                        })
                        .await
                        .map_err(|e| eyre::eyre!("Wake send_prompt error: {e:?}"));
                    let _ = respond_to.send(result);
                });
            },
            AcpSessionRequest::SwapAgent {
                mut agent_config,
                respond_to,
            } => {
                // Re-merge session-injected MCP servers into the new agent config
                self.merge_session_mcp_servers(&mut agent_config);

                if let Err(e) = update_model_info(
                    &self.api_client,
                    &self.os.database,
                    &self.rts_state,
                    agent_config.model(),
                )
                .await
                {
                    warn!("Failed to update model during swap: {}", e);
                }
                // Reset stale context usage data since it's meaningless after swapping agents
                self.rts_state.set_context_usage_percentage(None);

                let resolver = PathResolver::new(&self.os);
                let local_mcp_path = resolver.workspace().mcp_config().ok();
                let global_mcp_path = resolver.global().mcp_config().ok();

                let new_name = agent_config.name().to_string();
                let result = self
                    .agent
                    .swap_agent(agent::protocol::SwapAgentArgs {
                        agent_config: *agent_config,
                        local_mcp_path,
                        global_mcp_path,
                        force: false,
                    })
                    .await;

                if result.is_ok() {
                    self.previous_agent_name = Some(std::mem::replace(&mut self.current_agent_name, new_name));
                    self.persist_session_state().await;
                }

                let _ = respond_to.send(result);
            },
            AcpSessionRequest::SetModel { model_id, respond_to } => {
                let result = update_model_info(&self.api_client, &self.os.database, &self.rts_state, Some(&model_id))
                    .await
                    .map(|_| ());
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
                let is_agent_swap = matches!(&command, TuiCommand::Agent(args) if args.agent_name.is_some())
                    || matches!(&command, TuiCommand::Plan(_))
                    || matches!(&command, TuiCommand::Guide(_));
                let is_agent_create = matches!(&command, TuiCommand::Agent(args) if args.agent_name.as_deref().is_some_and(|n| n == "create" || n.starts_with("create ")));
                debug!(
                    is_agent_swap,
                    is_agent_create,
                    command = ?std::mem::discriminant(&command),
                    "ExecuteCommand: flags computed"
                );
                let ctx = self.command_context();
                let result = super::commands::execute(command, &ctx).await;
                debug!(
                    success = result.success,
                    message = %result.message,
                    has_data = result.data.is_some(),
                    "ExecuteCommand: result received"
                );

                if is_agent_swap
                    && result.success
                    && let Some(data) = &result.data
                    && let Some(name) = data.get("agent").and_then(|a| a.get("name")).and_then(|n| n.as_str())
                    && name != self.current_agent_name
                {
                    self.previous_agent_name = Some(std::mem::replace(&mut self.current_agent_name, name.to_string()));
                    let _ = self.send_ext_notification(
                        super::extensions::methods::AGENT_SWITCHED,
                        super::extensions::AgentSwitchedNotification {
                            session_id: self.session_id.clone(),
                            agent_name: self.current_agent_name.clone(),
                            previous_agent_name: self.previous_agent_name.clone(),
                            welcome_message: self.welcome_message_for(&self.current_agent_name),
                        },
                    );
                }

                // Reload available agents after a successful agent create
                if is_agent_create && result.success {
                    debug!("ExecuteCommand: reloading available agents after create");
                    self.reload_available_agents().await;
                } else if is_agent_create {
                    debug!(
                        success = result.success,
                        "ExecuteCommand: skipping reload (create failed or not create)"
                    );
                }

                let create_succeeded = is_agent_create && result.success;
                let _ = respond_to.send(result);
                if is_agent_swap && let Err(e) = self.advertise_commands_and_prompts().await {
                    warn!("Failed to advertise commands after agent swap: {}", e);
                }
                if create_succeeded && let Err(e) = self.advertise_commands_and_prompts().await {
                    warn!("Failed to advertise commands after agent create: {}", e);
                }
            },
            AcpSessionRequest::GetCommandOptions {
                command,
                partial,
                respond_to,
            } => {
                debug!(
                    available_agents_count = self.available_agents.len(),
                    agent_names = ?self.available_agents.iter().map(|a| &a.name).collect::<Vec<_>>(),
                    "GetCommandOptions: current available agents"
                );
                let ctx = self.command_context();
                let result = match command {
                    super::schema::TuiCommandKind::Model => super::commands::model::get_options(&partial, &ctx).await,
                    super::schema::TuiCommandKind::Agent => super::commands::agent::get_options(&partial, &ctx),
                    super::schema::TuiCommandKind::Prompts => super::commands::prompts::get_options(&self.agent).await,
                    super::schema::TuiCommandKind::Feedback => super::commands::issue::get_options(),
                    super::schema::TuiCommandKind::Chat => {
                        match super::commands::chat::list_sessions(ctx.session_tx, Some(ctx.cwd.to_path_buf())).await {
                            Ok(entries) => {
                                let options = entries.into_iter().map(Into::into).collect();
                                agent::tui_commands::CommandOptionsResponse {
                                    options,
                                    has_more: false,
                                }
                            },
                            Err(_) => agent::tui_commands::CommandOptionsResponse::default(),
                        }
                    },
                    super::schema::TuiCommandKind::Context
                    | super::schema::TuiCommandKind::Compact
                    | super::schema::TuiCommandKind::Clear
                    | super::schema::TuiCommandKind::Quit
                    | super::schema::TuiCommandKind::Usage
                    | super::schema::TuiCommandKind::Mcp
                    | super::schema::TuiCommandKind::Tools => agent::tui_commands::CommandOptionsResponse::default(),
                };
                let _ = respond_to.send(result);
            },
            AcpSessionRequest::GetMcpPrompts { respond_to } => {
                let result = self
                    .agent
                    .get_mcp_prompts()
                    .await
                    .map_err(|e| format!("Failed to get MCP prompts: {}", e));
                let _ = respond_to.send(result);
            },
            AcpSessionRequest::GetFilePrompts { respond_to } => {
                let result = self
                    .agent
                    .get_file_prompts()
                    .await
                    .map_err(|e| format!("Failed to get file prompts: {}", e));
                let _ = respond_to.send(result);
            },
            AcpSessionRequest::GetMcpPrompt {
                name,
                arguments,
                respond_to,
            } => {
                let result = self
                    .agent
                    .get_mcp_prompt(name, arguments)
                    .await
                    .map_err(|e| format!("Failed to get MCP prompt: {}", e));
                let _ = respond_to.send(result);
            },
            AcpSessionRequest::GetAgentHandle { respond_to } => {
                let _ = respond_to.send(self.agent.clone());
            },
            AcpSessionRequest::SendExtNotification { method, params } => {
                if let Ok(raw) = serde_json::value::to_raw_value(&params) {
                    let ext = sacp::schema::ExtNotification::new(method, std::sync::Arc::from(raw));
                    let _ = self
                        .connection_cx
                        .send_notification(sacp::schema::AgentNotification::ExtNotification(ext));
                }
            },
            AcpSessionRequest::GetToolInfo { respond_to } => {
                let result = self
                    .agent
                    .get_tool_info()
                    .await
                    .map_err(|e| format!("Failed to get tool info: {}", e));
                let _ = respond_to.send(result);
            },
            AcpSessionRequest::GetMcpServerInfo { respond_to } => {
                let result = self
                    .agent
                    .get_mcp_server_info()
                    .await
                    .map_err(|e| format!("Failed to get MCP server info: {}", e));
                let _ = respond_to.send(result);
            },
            AcpSessionRequest::Shutdown { respond_to } => {
                self.agent.shutdown().await;
                let _ = respond_to.send(());
            },
            AcpSessionRequest::AdvertiseCommands => {
                if let Err(e) = self.advertise_commands_and_prompts().await {
                    warn!("Failed to advertise commands: {}", e);
                }
            },
            AcpSessionRequest::RefreshMcpServers { agent_config } => {
                self.pending_mcp_refresh = Some(agent_config);
            },
        }
    }

    async fn handle_agent_event(&mut self, event: AgentEvent) {
        self.telemetry_observer
            .send_event(self.session_id_str.clone(), event.clone());

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
            AgentEvent::Update(ref update_event) => {
                // Intercept switch_to_execution before forwarding to TUI
                if let UpdateEvent::ToolCallFinished { tool_call, result } = update_event
                    && tool_call.tool_use_block.name == "switch_to_execution"
                    && let ToolCallResult::Success(output) = result
                {
                    #[derive(serde::Deserialize)]
                    struct SwitchResult {
                        approved: bool,
                        plan: String,
                    }
                    let json_str = output
                        .items
                        .first()
                        .and_then(|item| match item {
                            agent::tools::ToolExecutionOutputItem::Text(s) => Some(s.as_str()),
                            _ => None,
                        })
                        .unwrap_or_default();
                    if let Ok(sr) = serde_json::from_str::<SwitchResult>(json_str)
                        && sr.approved
                    {
                        self.handle_switch_to_execution(sr.plan).await;
                        return; // Do NOT forward to TUI as a regular tool call
                    }
                }
                // Normal path — forward to TUI
                if let Some(update) = convert_update_event_to_session_update(update_event.clone()) {
                    let _ = self.send_session_notification(update);
                }
            },
            AgentEvent::ApprovalRequest(req) => {
                info!(
                    "AgentEvent::ApprovalRequest: id={}, tool_use={:?}, context={:?}",
                    req.id, req.tool_use, req.context
                );
                // All sessions (main and subagent) forward approval requests to the TUI
                let connection_cx = self.connection_cx.clone();
                let session_id = self.session_id.clone();
                let agent = self.agent.clone();
                let is_subagent = self.is_subagent;
                tokio::spawn(async move {
                    handle_approval_request(req, connection_cx, session_id, agent, is_subagent).await;
                });
            },
            AgentEvent::LogEntryAppended { entry, .. } => {
                if let Err(e) = session_db.append_log_entry(&entry) {
                    warn!("Failed to persist log entry: {}", e);
                }
                // Set session title from the first user prompt
                if let LogEntry::V1(LogEntryV1::Prompt { content, .. }) = &entry
                    && session_db.session().title.is_none()
                    && let Some(title) = crate::agent::session::create_session_title(content)
                    && let Err(e) = session_db.set_title(title)
                {
                    warn!("Failed to set session title: {}", e);
                }
                if let LogEntry::V1(LogEntryV1::Compaction { summary, .. }) = &entry {
                    self.compaction_summary = Some(summary.clone());
                }
            },
            AgentEvent::EndTurn(md) => {
                // Update context usage in rts state and send to TUI
                if let Some(p) = md.context_usage_percentage {
                    tracing::debug!(backend_context_usage_pct = p, "EndTurn: backend context usage");
                    rts_state.set_context_usage_percentage(Some(p));
                }
                if let Err(e) = self.send_turn_metadata(&md) {
                    warn!("Failed to send turn metadata: {}", e);
                }

                // If a switch_to_execution swap is pending, keep the prompt response alive
                // so the TUI stays in isProcessing through the agent swap and plan execution.
                // The response will be sent when the plan execution's EndTurn fires instead.
                let has_pending_swap = self.pending_swap.is_some();

                if !has_pending_swap {
                    // Normal EndTurn — respond to the TUI to end the turn
                    if let Some(respond_to) = self.pending_prompt_response.take() {
                        self.persist_session_state().await;
                        let respond_to = respond_to.into_inner();
                        let stop_reason = match md.end_reason {
                            agent::agent_loop::protocol::LoopEndReason::UserTurnEnd => StopReason::EndTurn,
                            agent::agent_loop::protocol::LoopEndReason::ToolUseRejected => StopReason::Refusal,
                            agent::agent_loop::protocol::LoopEndReason::Cancelled => StopReason::Cancelled,
                            _ => StopReason::EndTurn,
                        };
                        let _ = respond_to.respond(PromptResponse::new(stop_reason));
                    }
                }
                // Execute pending swap from switch_to_execution (agent is idle after end_current_turn)
                if let Some(agent_config) = self.pending_swap.take() {
                    let target_name = agent_config.name().to_string();
                    let resolver = crate::util::paths::PathResolver::new(&self.os);
                    let local_mcp_path = resolver.workspace().mcp_config().ok();
                    let global_mcp_path = resolver.global().mcp_config().ok();
                    if let Err(e) = self
                        .agent
                        .swap_agent(agent::protocol::SwapAgentArgs {
                            agent_config,
                            local_mcp_path,
                            global_mcp_path,
                            force: false,
                        })
                        .await
                    {
                        tracing::error!("switch_to_execution swap failed: {e}");
                        self.pending_plan = None;
                        // Swap failed — respond to TUI now since we deferred it
                        if let Some(respond_to) = self.pending_prompt_response.take() {
                            let respond_to = respond_to.into_inner();
                            let _ = respond_to.respond(PromptResponse::new(StopReason::EndTurn));
                        }
                    } else {
                        self.previous_agent_name =
                            Some(std::mem::replace(&mut self.current_agent_name, target_name.clone()));
                        let _ = self.send_ext_notification(
                            crate::agent::acp::extensions::methods::AGENT_SWITCHED,
                            crate::agent::acp::extensions::AgentSwitchedNotification {
                                session_id: self.session_id.clone(),
                                agent_name: target_name.clone(),
                                previous_agent_name: self.previous_agent_name.clone(),
                                welcome_message: self.welcome_message_for(&target_name),
                            },
                        );
                        if let Err(e) = self.advertise_commands_and_prompts().await {
                            tracing::warn!("Failed to advertise after switch_to_execution: {e}");
                        }
                    }
                    // Inject pending plan after swap
                    if let Some(plan_prompt) = self.pending_plan.take() {
                        let agent = self.agent.clone();
                        tokio::spawn(async move {
                            let _ = agent
                                .send_prompt(agent::protocol::SendPromptArgs {
                                    content: vec![agent::protocol::ContentChunk::Text(plan_prompt)],
                                    should_continue_turn: None,
                                })
                                .await;
                        });
                    }
                }
            },
            AgentEvent::Stop(AgentStopReason::Error(agent_error)) => {
                // Check if this is a throttling error and send a rate limit notification
                if let agent::protocol::AgentError::AgentLoopError(LoopError::Stream(stream_error)) = &agent_error
                    && matches!(stream_error.kind, StreamErrorKind::Throttling)
                {
                    info!("Sending rate limit error notification to client");
                    if let Err(e) = self.send_ext_notification(methods::RATE_LIMIT_ERROR, RateLimitErrorNotification {
                        session_id: self.session_id.clone(),
                        message: "Rate limit exceeded. Please wait a moment before trying again.".to_string(),
                    }) {
                        error!("Failed to send rate limit notification: {}", e);
                    }
                }

                // Send error response directly to the client - this ends the turn so we take() it
                if let Some(respond_to) = self.pending_prompt_response.take() {
                    let respond_to = respond_to.into_inner();
                    // Include the actual error message for better user feedback
                    let error_message = format!("{}", agent_error);
                    let _ = respond_to.respond_with_error(sacp::util::internal_error(error_message));
                }
            },
            AgentEvent::SessionToolRequest(session_request) => {
                let session_tx = self.session_tx.clone();
                let session_id = self.session_id.clone();
                let agent = self.agent.clone();
                tokio::spawn(async move {
                    super::session_tool_handler::handle_session_tool_request(
                        session_request,
                        session_tx,
                        session_id,
                        agent,
                    )
                    .await;
                });
            },
            AgentEvent::Mcp(mcp_event) => {
                if let Err(e) = self.handle_mcp_event(mcp_event).await {
                    error!("Failed to handle MCP event: {}", e);
                }
            },
            AgentEvent::Compaction(compaction_event) => {
                tracing::info!("Received compaction event: {:?}", compaction_event);
                let status = match &compaction_event {
                    CompactionEvent::Started => CompactionStatus::Started,
                    CompactionEvent::Completed => CompactionStatus::Completed,
                    CompactionEvent::Failed { error } => CompactionStatus::Failed { error: error.clone() },
                };
                let summary = if matches!(compaction_event, CompactionEvent::Completed) {
                    self.compaction_summary.take()
                } else {
                    None
                };
                if let Err(e) = self.send_ext_notification(methods::COMPACTION_STATUS, CompactionStatusNotification {
                    session_id: self.session_id.clone(),
                    status,
                    summary,
                }) {
                    error!("Failed to send compaction notification: {}", e);
                }
                // After compaction completes, recompute and emit updated context usage
                if matches!(compaction_event, CompactionEvent::Completed) {
                    if let Ok(snapshot) = self.agent.create_snapshot().await {
                        let context_window = self
                            .rts_state
                            .model_info()
                            .map_or(super::commands::context::DEFAULT_CONTEXT_WINDOW_TOKENS, |m| {
                                m.context_window_tokens
                            });
                        let sizes = super::commands::context::calculate_component_sizes(&snapshot);
                        let baseline_tokens =
                            sizes.tools + sizes.context_files + sizes.kiro + sizes.user + sizes.system;
                        let baseline_percentage = (baseline_tokens as f32 / context_window as f32) * 100.0;
                        tracing::debug!(
                            tools = sizes.tools,
                            context_files = sizes.context_files,
                            system = sizes.system,
                            kiro = sizes.kiro,
                            user = sizes.user,
                            baseline_tokens,
                            context_window,
                            baseline_percentage,
                            tool_specs_count = snapshot.tool_specs.len(),
                            "compact: recomputed context usage"
                        );
                        self.rts_state.set_context_usage_percentage(Some(baseline_percentage));
                    }
                    let notification = super::schema::MetadataNotification {
                        session_id: self.session_id_str.clone(),
                        context_usage_percentage: self.rts_state.context_usage_percentage(),
                        metering_usage: None,
                        turn_duration_ms: None,
                    };
                    if let Err(e) = self.connection_cx.send_notification(notification) {
                        warn!("Failed to send metadata after compaction: {}", e);
                    }
                }
            },
            AgentEvent::Clear(_) => {
                tracing::info!("Received clear event");
                // Compute baseline context usage (tools + system prompt) instead of None
                if let Ok(snapshot) = self.agent.create_snapshot().await {
                    let context_window = self
                        .rts_state
                        .model_info()
                        .map_or(super::commands::context::DEFAULT_CONTEXT_WINDOW_TOKENS, |m| {
                            m.context_window_tokens
                        });
                    let sizes = super::commands::context::calculate_component_sizes(&snapshot);
                    let baseline_tokens = sizes.tools + sizes.context_files + sizes.system;
                    let baseline_percentage = (baseline_tokens as f32 / context_window as f32) * 100.0;
                    tracing::debug!(
                        tools = sizes.tools,
                        context_files = sizes.context_files,
                        system = sizes.system,
                        baseline_tokens,
                        context_window,
                        baseline_percentage,
                        tool_specs_count = snapshot.tool_specs.len(),
                        "clear: recomputed context usage"
                    );
                    self.rts_state.set_context_usage_percentage(Some(baseline_percentage));
                } else {
                    self.rts_state.set_context_usage_percentage(None);
                }

                let notification = super::schema::MetadataNotification {
                    session_id: self.session_id_str.clone(),
                    context_usage_percentage: self.rts_state.context_usage_percentage(),
                    metering_usage: None,
                    turn_duration_ms: None,
                };
                if let Err(e) = self.connection_cx.send_notification(notification) {
                    warn!("Failed to send metadata after clear: {}", e);
                }

                if let Err(e) = self.send_ext_notification(methods::CLEAR_STATUS, ClearStatusNotification {
                    session_id: self.session_id.clone(),
                }) {
                    error!("Failed to send clear notification: {}", e);
                }
            },
            AgentEvent::Internal(InternalEvent::AgentLoop(loop_event)) => {
                if let AgentLoopEventKind::ToolUseStart { id, name } = loop_event.kind {
                    let _ = self.send_ext_notification(methods::SESSION_UPDATE, ExtSessionUpdateNotification {
                        session_id: self.session_id.clone(),
                        update: ExtSessionUpdate::ToolCallChunk {
                            tool_call_id: id,
                            title: name.clone(),
                            kind: get_tool_kind(&name),
                        },
                    });
                }
            },
            AgentEvent::Stop(AgentStopReason::EndTurn) => {
                // Resolve the pending prompt response if it hasn't been resolved yet.
                if let Some(respond_to) = self.pending_prompt_response.take() {
                    warn!("Resolving pending prompt via Stop(EndTurn) — no EndTurn event was received");
                    let respond_to = respond_to.into_inner();
                    let _ = respond_to.respond(PromptResponse::new(StopReason::EndTurn));
                }
            },
            AgentEvent::SubagentSummary(summary) => {
                if self.is_subagent {
                    let session_tx = self.session_tx.clone();
                    let session_id = self.session_id.clone();
                    let task_result = summary.task_result.clone();
                    let task_desc = summary.task_description.clone();
                    let ctx_summary = summary.context_summary.clone();
                    tokio::spawn(async move {
                        if let Some(orch) = session_tx.get_orchestrated_session_by_id(&session_id).await
                            && let Some(parent_sid) = orch.parent_session
                        {
                            let result_text = format!(
                                "Task: {}\n\n{}\n\n{}",
                                task_desc,
                                ctx_summary.as_deref().unwrap_or(""),
                                task_result
                            );
                            let msg = format!("[Results from {}]\n\n{}", orch.name, result_text);
                            let _ = session_tx.deliver_subagent_result(&parent_sid, &msg).await;
                        }
                    });
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
                })?;

                // Re-advertise commands + prompts now that a new MCP server is ready
                self.advertise_commands_and_prompts().await
            },
            McpServerEvent::InitializeError { server_name, error } => {
                info!(?server_name, ?error, "Forwarding MCP server init failure to client");
                self.send_ext_notification(methods::MCP_SERVER_INIT_FAILURE, McpServerInitFailureNotification {
                    session_id: self.session_id.clone(),
                    server_name,
                    error,
                })?;

                // Re-advertise so /mcp panel shows updated "failed" status
                self.advertise_commands_and_prompts().await
            },
            // Other MCP events don't need forwarding to client
            McpServerEvent::Initializing { .. } => Ok(()),
            McpServerEvent::ToolListChanged { server_name } => {
                info!(?server_name, "MCP server tool list changed, re-advertising");
                self.advertise_commands_and_prompts().await
            },
        }
    }

    async fn advertise_commands_and_prompts(&self) -> Result<(), sacp::Error> {
        if self.is_subagent {
            return Ok(());
        }
        advertise_commands_and_prompts_to_client(&self.session_id_str, &self.agent, &self.connection_cx).await
    }

    async fn handle_switch_to_execution(&mut self, plan: String) {
        // Use previous_agent_name if it's not the planner itself, otherwise fall back to default
        let target = self
            .previous_agent_name
            .as_deref()
            .filter(|name| *name != crate::constants::PLANNER_AGENT_NAME)
            .unwrap_or(crate::constants::DEFAULT_AGENT_NAME)
            .to_string();

        let mut agent_config = match self.agent_configs.iter().find(|c| c.name() == target) {
            Some(c) => c.clone(),
            None => {
                tracing::error!("switch_to_execution: target agent '{}' not found", target);
                return;
            },
        };

        // Re-merge session-injected MCP servers into the new agent config
        self.merge_session_mcp_servers(&mut agent_config);

        // Defer swap and plan injection to after EndTurn (agent must be idle for swap_agent)
        self.pending_swap = Some(agent_config);
        self.pending_plan = Some(format!("Implement this plan:\n{}", plan));
    }
}

async fn advertise_commands_and_prompts_to_client(
    session_id: &str,
    agent_handle: &AgentHandle,
    client_cx: &JrConnectionCx<AgentToClient>,
) -> Result<(), sacp::Error> {
    let commands: Vec<super::schema::AvailableCommand> = TuiCommand::all_commands()
        .into_iter()
        .map(|cmd| super::schema::AvailableCommand {
            name: cmd.name().to_string(),
            description: cmd.description().to_string(),
            meta: cmd.meta(),
        })
        .collect();

    let mut prompts: Vec<super::schema::PromptInfo> = match agent_handle.get_mcp_prompts().await {
        Ok(mcp_prompts) => mcp_prompts
            .into_iter()
            .flat_map(|(server_name, server_prompts)| {
                server_prompts.into_iter().map(move |prompt| super::schema::PromptInfo {
                    name: prompt.name,
                    description: prompt.description,
                    arguments: prompt
                        .arguments
                        .unwrap_or_default()
                        .into_iter()
                        .map(|arg| super::schema::PromptArgumentInfo {
                            name: arg.name,
                            description: arg.description,
                            required: arg.required.unwrap_or(false),
                        })
                        .collect(),
                    server_name: server_name.clone(),
                })
            })
            .collect(),
        Err(e) => {
            warn!("Failed to get MCP prompts: {}", e);
            Vec::new()
        },
    };

    // Add file-based prompts
    if let Ok(file_prompts) = agent_handle.get_file_prompts().await {
        for (source, source_prompts) in file_prompts {
            for prompt in source_prompts {
                prompts.push(super::schema::PromptInfo {
                    name: prompt.name,
                    description: prompt.description,
                    arguments: prompt
                        .arguments
                        .unwrap_or_default()
                        .into_iter()
                        .map(|arg| super::schema::PromptArgumentInfo {
                            name: arg.name,
                            description: arg.description,
                            required: arg.required.unwrap_or(false),
                        })
                        .collect(),
                    server_name: source.clone(),
                });
            }
        }
    }

    // Collect tool advertisements
    let tools: Vec<super::schema::ToolAdvertisement> = match agent_handle.get_tool_info().await {
        Ok(tool_infos) => tool_infos
            .into_iter()
            .map(|t| super::schema::ToolAdvertisement {
                name: t.name,
                description: t.description,
                source: t.source,
            })
            .collect(),
        Err(e) => {
            warn!("Failed to get tool info for advertising: {}", e);
            Vec::new()
        },
    };

    // Collect MCP server advertisements
    let mcp_servers: Vec<super::schema::McpServerAdvertisement> = match agent_handle.get_mcp_server_info().await {
        Ok(server_infos) => server_infos
            .into_iter()
            .map(|s| {
                let status = match s.status {
                    agent::tui_commands::McpServerStatus::Running => "running",
                    agent::tui_commands::McpServerStatus::Loading => "loading",
                    agent::tui_commands::McpServerStatus::Failed => "failed",
                    agent::tui_commands::McpServerStatus::Disabled => "disabled",
                };
                super::schema::McpServerAdvertisement {
                    name: s.name,
                    status: status.to_string(),
                    tool_count: s.tool_count,
                }
            })
            .collect(),
        Err(e) => {
            warn!("Failed to get MCP server info for advertising: {}", e);
            Vec::new()
        },
    };

    let notification = super::schema::CommandsAvailableNotification {
        session_id: session_id.to_string(),
        commands,
        prompts,
        tools,
        mcp_servers,
    };
    client_cx.send_notification(notification)
}

async fn handle_approval_request(
    req: ApprovalRequest,
    client_cx: JrConnectionCx<AgentToClient>,
    session_id: SessionId,
    agent: AgentHandle,
    is_subagent: bool,
) {
    // Map agent permission options to ACP permission options
    // Filter out *ToolArgs variants as ACP only supports tool-level always options
    // For subagents, emit both allow_always (per-tool) and allow_all_session (blanket)
    let mut options: Vec<PermissionOption> = req
        .options
        .iter()
        .filter_map(|opt| {
            let (id, kind) = match opt.id {
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

    if is_subagent {
        options.push(PermissionOption::new(
            "allow_all_session",
            "Allow all for this session",
            PermissionOptionKind::AllowAlways,
        ));
    }

    debug!("Sending permission request: {:?}", req);
    let mut permission_request = RequestPermissionRequest::new(
        session_id,
        ToolCallUpdate::new(
            ToolCallId::new(req.id.clone()),
            ToolCallUpdateFields::new().title(Some(get_tool_title(&req.tool))),
        ),
        options,
    );

    // Attach granular trust options via _meta so the TUI can offer path/command-level trust
    if !req.trust_options.is_empty() {
        let mut meta = serde_json::Map::new();
        meta.insert(
            "trustOptions".into(),
            serde_json::to_value(&req.trust_options).unwrap_or_default(),
        );
        permission_request = permission_request.meta(meta);
    }

    let response = client_cx.send_request(permission_request).block_task().await;

    match response {
        Ok(res) => match res.outcome {
            sacp::schema::RequestPermissionOutcome::Selected(selected) => {
                use std::str::FromStr;

                // "Allow all for this session" — set trust_all_tools and approve current tool
                if selected.option_id.0.as_ref() == "allow_all_session" {
                    if let Err(e) = agent.set_trust_all_tools(true).await {
                        error!("Failed to set trust_all_tools: {}", e);
                    }
                    let _ = agent
                        .send_tool_use_approval_result(agent::protocol::SendApprovalResultArgs {
                            id: req.id,
                            result: agent::protocol::ApprovalResult {
                                option_id: agent::protocol::PermissionOptionId::AllowOnce,
                                reason: None,
                                trust_option: None,
                            },
                        })
                        .await;
                    return;
                }

                // Map ACP option_id to agent PermissionOptionId
                // allow_always with a trustOption in _meta means granular trust (args-level)
                let trust_option: Option<agent::protocol::TrustOption> = selected
                    .meta
                    .as_ref()
                    .and_then(|m| m.get("trustOption"))
                    .and_then(|v| serde_json::from_value(v.clone()).ok());
                let option_id = match selected.option_id.0.as_ref() {
                    "allow_always" if trust_option.is_some() => {
                        agent::protocol::PermissionOptionId::AllowAlwaysToolArgs
                    },
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
                let approval_result = agent::protocol::ApprovalResult {
                    option_id,
                    reason,
                    trust_option,
                };
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
                            trust_option: None,
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
                        trust_option: None,
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
        UpdateEvent::ToolCallFailed {
            tool_use_id, tool_name, ..
        } => {
            let kind = get_tool_kind(&tool_name);
            Some(SessionUpdate::ToolCallUpdate(ToolCallUpdate::new(
                ToolCallId::new(tool_use_id),
                ToolCallUpdateFields::new()
                    .status(Some(ToolCallStatus::Failed))
                    .title(Some(tool_name))
                    .kind(Some(kind)),
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
            BuiltInToolName::Summary => ToolKind::Other,
            BuiltInToolName::Grep => ToolKind::Search,
            BuiltInToolName::Glob => ToolKind::Search,
            BuiltInToolName::UseAws => ToolKind::Execute,
            BuiltInToolName::WebFetch => ToolKind::Read,
            BuiltInToolName::WebSearch => ToolKind::Search,
            BuiltInToolName::Code => ToolKind::Read, // Default, actual kind determined by operation
            BuiltInToolName::AgentCrew => ToolKind::Other,
            BuiltInToolName::SessionManagement => ToolKind::Other,
            BuiltInToolName::SwitchToExecution => ToolKind::Other,
            BuiltInToolName::Introspect => ToolKind::Read,
            BuiltInToolName::Knowledge => ToolKind::Other,
            BuiltInToolName::Task => ToolKind::Other,
        }
    } else {
        ToolKind::Other
    }
}

pub(crate) fn get_tool_title(tool: &Tool) -> String {
    match &tool.kind {
        AgentToolKind::BuiltIn(builtin) => match builtin {
            BuiltInTool::FileRead(fs_read) => {
                use agent::tools::fs_read::FsReadOperation;
                let files: Vec<_> = fs_read
                    .operations
                    .iter()
                    .map(|op| match op {
                        FsReadOperation::Line(f) => {
                            let start = f.offset.unwrap_or(0) + 1;
                            match f.limit {
                                Some(limit) => format!("{}:{}-{}", truncate_path(&f.path), start, start + limit - 1),
                                None => format!("{}:{}", truncate_path(&f.path), start),
                            }
                        },
                        FsReadOperation::Directory(d) => format!("listing {}", truncate_path(&d.path)),
                        FsReadOperation::Image(img) => {
                            let paths: Vec<_> = img.paths.iter().map(|p| p.as_str()).collect();
                            format!("image {}", paths.join(", "))
                        },
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
            BuiltInTool::ExecuteCmd(cmd) => format!("Running: {}", truncate_str(&cmd.command, 200)),
            BuiltInTool::UseAws(aws) => format!("AWS: {} {}", aws.service_name, aws.operation_name),
            BuiltInTool::Summary(_) => "Summarizing".to_string(),
            BuiltInTool::Mkdir(_) => "Creating directory".to_string(),
            BuiltInTool::Introspect(_) => "Introspecting".to_string(),
            BuiltInTool::WebFetch(_) => "Fetching web content".to_string(),
            BuiltInTool::WebSearch(_) => "Searching the web".to_string(),
            BuiltInTool::Code(code) => {
                use agent::tools::code::Code;
                match code {
                    Code::SearchSymbols(p) => format!("Searching symbols: {}", p.symbol_name),
                    Code::LookupSymbols(p) => format!("Looking up: {}", p.symbols.join(", ")),
                    Code::FindReferences(p) => format!("Finding references in {}", truncate_path(&p.file_path)),
                    Code::GotoDefinition(p) => format!("Going to definition in {}", truncate_path(&p.file_path)),
                    Code::GetDocumentSymbols(p) => format!("Getting symbols in {}", truncate_path(&p.file_path)),
                    Code::GetDiagnostics(p) => format!("Getting diagnostics for {}", truncate_path(&p.file_path)),
                    Code::GetHover(p) => format!("Getting hover info in {}", truncate_path(&p.file_path)),
                    Code::GetCompletions(p) => format!("Getting completions in {}", truncate_path(&p.file_path)),
                    Code::RenameSymbol(p) => format!("Renaming to '{}' in {}", p.new_name, truncate_path(&p.file_path)),
                    Code::Format(p) => format!(
                        "Formatting {}",
                        p.file_path
                            .as_deref()
                            .map_or_else(|| "workspace".to_string(), truncate_path)
                    ),
                    Code::PatternSearch(p) => format!("Pattern search: {}", truncate_str(&p.pattern, 40)),
                    Code::PatternRewrite(p) => format!("Pattern rewrite: {}", truncate_str(&p.pattern, 40)),
                    Code::GenerateCodebaseOverview(_) => "Generating codebase overview".to_string(),
                    Code::SearchCodebaseMap(_) => "Searching codebase map".to_string(),
                    Code::InitializeWorkspace => "Initializing workspace".to_string(),
                }
            },
            BuiltInTool::AgentCrew(_) => "Spawning agent crew".to_string(),
            BuiltInTool::SessionManagement(_) => "Managing sessions".to_string(),
            BuiltInTool::SwitchToExecution(_) => "Switching to execution agent".to_string(),
            BuiltInTool::Knowledge(_) => "Querying knowledge base".to_string(),
            BuiltInTool::Task(t) => {
                use agent::tools::task::task_tool::TaskTool;
                match t {
                    TaskTool::Create {
                        task_list_description, ..
                    } => {
                        format!("Creating task list: {}", truncate_str(task_list_description, 60))
                    },
                    TaskTool::Complete { completed_task_ids, .. } => {
                        let ids = completed_task_ids
                            .iter()
                            .map(|id| format!("#{id}"))
                            .collect::<Vec<_>>()
                            .join(", ");
                        format!("Completing {ids}")
                    },
                    TaskTool::Add { .. } => "Adding tasks".to_string(),
                    TaskTool::Remove { .. } => "Removing tasks".to_string(),
                    TaskTool::List { .. } => "Listing tasks".to_string(),
                }
            },
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
    get_tool_content_impl(tool, &RealProvider)
}

fn get_tool_content_impl(tool: &Tool, provider: &impl SystemProvider) -> Vec<ToolCallContent> {
    match &tool.kind {
        AgentToolKind::BuiltIn(BuiltInTool::FileWrite(fs_write)) => {
            let raw_path = fs_write.path();
            let abs_path =
                canonicalize_path_sys(raw_path, provider).map_or_else(|_| PathBuf::from(raw_path), PathBuf::from);
            let (old_text, new_text) = match fs_write {
                FsWrite::Create(create) => {
                    // Read existing file content for proper diffing when overwriting
                    let old = std::fs::read_to_string(&abs_path).ok();
                    (old, create.content.clone())
                },
                // StrReplace: old_text/new_text are the replacement snippet, not full file content.
                // The Diff path is still resolved to absolute so the TUI can locate the file.
                FsWrite::StrReplace(str_replace) => (Some(str_replace.old_str.clone()), str_replace.new_str.clone()),
                FsWrite::Insert(_) => return vec![],
            };

            vec![ToolCallContent::Diff(Diff::new(abs_path, new_text).old_text(old_text))]
        },
        _ => vec![],
    }
}

fn get_tool_locations(tool: &Tool) -> Option<Vec<ToolCallLocation>> {
    match &tool.kind {
        AgentToolKind::BuiltIn(builtin) => match builtin {
            BuiltInTool::FileRead(fs_read) => {
                use agent::tools::fs_read::FsReadOperation;
                let locations: Vec<_> = fs_read
                    .operations
                    .iter()
                    .flat_map(|op| match op {
                        FsReadOperation::Line(f) => {
                            let mut loc = ToolCallLocation::new(&f.path);
                            if let Some(offset) = f.offset {
                                loc = loc.line(offset + 1); // offset is 0-based, line is 1-based
                            }
                            vec![loc]
                        },
                        FsReadOperation::Directory(d) => vec![ToolCallLocation::new(&d.path)],
                        FsReadOperation::Image(img) => img.paths.iter().map(ToolCallLocation::new).collect(),
                    })
                    .collect();
                if locations.is_empty() { None } else { Some(locations) }
            },
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
            _ => None,
        },
        AgentToolKind::Mcp(_) => None,
    }
}

/// Update model ID in RTS state.
/// Validates the model ID against available models if specified.
/// Priority: 1) explicit model arg, 2) user's saved default, 3) API default.
///
/// Returns the requested model name if it wasn't found (fell back to default).
async fn update_model_info(
    client: &ApiClient,
    database: &crate::database::Database,
    rts_state: &RtsState,
    model: Option<&str>,
) -> Result<Option<String>, String> {
    use crate::database::settings::Setting;

    let (models, api_default) = get_available_models(client)
        .await
        .map_err(|e| format!("Failed to fetch available models: {}", e))?;

    let mut not_found = None;

    let model_info = if let Some(requested_model) = model {
        find_model(&models, requested_model).cloned().unwrap_or_else(|| {
            warn!(
                "Model '{}' not found in available models, falling back to default",
                requested_model
            );
            not_found = Some(requested_model.to_string());
            api_default.clone()
        })
    } else if let Some(saved) = database.settings.get_string(Setting::ChatDefaultModel) {
        find_model(&models, &saved).cloned().unwrap_or(api_default)
    } else {
        api_default
    };

    rts_state.set_model_info(Some(model_info));
    Ok(not_found)
}

/// Entry point for SACP agent
pub async fn execute(
    os: &mut Os,
    args: agent::types::AcpSpawnArgs,
    legacy_session_exporter: Arc<dyn LegacySessionExporter>,
) -> eyre::Result<ExitCode> {
    let resolver = PathResolver::new(os);
    let local_mcp_path = resolver.workspace().mcp_config().ok();
    let global_mcp_path = resolver.global().mcp_config().ok();

    let session_manager_handle = SessionManager::builder()
        .os(os.clone())
        .local_mcp_path(local_mcp_path)
        .global_mcp_path(global_mcp_path)
        .trust_all_tools(args.trust_all_tools)
        .trust_tools(args.trust_tools)
        .legacy_session_exporter(legacy_session_exporter)
        .spawn();

    if let Some(n) = args.agent {
        let _ = session_manager_handle.set_next_agent_name(n).await;
    }

    if let Some(m) = args.model {
        let _ = session_manager_handle.set_next_model_id(m).await;
    }

    // Check auth status upfront so the initialize response only advertises auth methods when needed
    let logged_in = crate::cli::is_logged_in(&mut os.database).await;

    // NOTE: It is _extremely_ easy to create a deadlock with sacp (read more about it
    // [here](https://docs.rs/sacp/10.1.0/sacp/concepts/ordering/index.html)). For that reason, it
    // is crucial that nothing we dispatch in these on_* callbacks are long running on the dispatch
    // thread, by which I mean the tasks dispatched here should end as soon as you hand off the
    // request (and not wait for a response). Take a look at
    // [crate::agent::acp::session_manager::SessionManager] for the general flow of request
    // response processing. The TLDR; is the request path and response path are _not_ done on the
    // same task.
    let (stdin_reader, stdin_closed) = super::stdin_reader::StdinReader::new();
    let serve_future = AgentToClient::builder()
        .name("kiro-cli-agent")
        .on_receive_request(
            {
                let session_tx = session_manager_handle.clone();
                async move |request: InitializeRequest, request_cx, _cx| {
                    // Store client info for telemetry (V2 vs ACP distinction)
                    if let Some(info) = request.client_info {
                        let _ = session_tx
                            .initialize(info.name, info.version)
                            .await;
                    }
                    let mut response = InitializeResponse::new(ProtocolVersion::LATEST)
                        .agent_capabilities(
                            AgentCapabilities::default()
                                .load_session(true)
                                .prompt_capabilities(PromptCapabilities::default().image(true))
                                .mcp_capabilities(McpCapabilities::default().http(true)),
                        )
                        .agent_info(
                            Implementation::new(crate::constants::AGENT_NAME, env!("CARGO_PKG_VERSION").to_string())
                                .title(crate::constants::AGENT_NAME),
                        );
                    if !logged_in {
                        response = response.auth_methods(vec![AuthMethod::new("kiro-login", "Kiro Login").description(
                            format!("Run '{} login' in terminal to authenticate. See https://kiro.dev/docs/cli/authentication/", crate::constants::CLI_NAME),
                        )]);
                    }
                    request_cx.respond(response)
                }
            },
            sacp::on_receive_request!(),
        )
        .on_receive_request(
            {
                let session_tx = session_manager_handle.clone();
                async move |request: NewSessionRequest, request_cx, cx: JrConnectionCx<AgentToClient>| {
                    let session_id = SessionId::new(Uuid::new_v4().to_string());

                    let config = AcpSessionConfig::new(session_id.to_string(), request.cwd.clone())
                        .mcp_servers(request.mcp_servers);
                    let result = session_tx.start_session(&session_id, config, Some(cx.clone())).await?;

                    // Wait for agent initialization to complete before responding
                    let _ = result.ready_rx.await;

                    let fallback_model_id = result.current_model_id.clone();
                    let modes = to_session_mode_state(result.current_agent_name, result.available_agents);
                    let models = to_session_model_state(result.current_model_id, result.available_models);

                    request_cx.respond(
                        NewSessionResponse::new(session_id.clone())
                            .modes(modes)
                            .models(models),
                    )?;

                    // Advertise after responding so the TUI has processed the session response
                    result.handle.advertise_commands().await;

                    // Notify TUI about agent loading issues
                    send_agent_load_notifications(&cx, &session_id, &result.requested_agent_name, &result.agent_config_errors, &result.requested_model_name, &fallback_model_id);

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
                    let config = AcpSessionConfig::new(request.session_id.to_string(), request.cwd.clone())
                        .load(true)
                        .mcp_servers(request.mcp_servers);
                    match session_tx.start_session(&request.session_id, config, Some(cx.clone())).await {
                        Ok(result) => {
                            // Wait for historical notifications to be sent before responding
                            let _ = result.ready_rx.await;

                            let fallback_model_id = result.current_model_id.clone();
                            let modes = to_session_mode_state(result.current_agent_name, result.available_agents);
                            let models = to_session_model_state(result.current_model_id, result.available_models);

                            request_cx.respond(LoadSessionResponse::new().modes(modes).models(models))?;

                            // Advertise after responding so the TUI has processed the session response
                            result.handle.advertise_commands().await;

                            // Notify TUI about agent loading issues
                            send_agent_load_notifications(&cx, &request.session_id, &result.requested_agent_name, &result.agent_config_errors, &result.requested_model_name, &fallback_model_id);

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
        // TODO: Replace with native sacp on_receive_request handler once sacp
        // adds ListSessionsRequest / ListSessionsResponse support. The wire
        // format matches the ACP session/list RFD and agent-client-protocol-schema >= 0.11.
        .on_receive_request(
            {
                let session_manager = session_manager_handle.clone();
                async move |request: super::schema::ListSessionsRequest, request_cx, _cx| {
                    let entries = super::commands::chat::list_sessions(&session_manager, request.cwd).await?;
                    request_cx.respond(super::schema::ListSessionsResponse {
                        sessions: entries,
                        next_cursor: None,
                    })
                }
            },
            sacp::on_receive_request!(),
        )
        // Handle session terminate
        .on_receive_request(
            {
                let session_tx = session_manager_handle.clone();
                async move |request: super::schema::TerminateSessionRequest, request_cx, _cx| {
                    let session_id = sacp::schema::SessionId::new(request.session_id);
                    session_tx.terminate_session(&session_id).await;
                    request_cx.respond(super::schema::TerminateSessionResponse {})?;
                    Ok(())
                }
            },
            sacp::on_receive_request!(),
        )
        // Handle settings/list
        .on_receive_request(
            {
                let os = os.clone();
                async move |_request: super::schema::SettingsListRequest, request_cx, _cx| {
                    request_cx.respond(super::schema::SettingsListResponse(os.database.settings.map().clone()))
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

                    // Handle _session/spawn ext method from TUI
                    use super::extensions::methods;
                    if method == methods::SESSION_SPAWN {
                        let MessageCx::Request(req, req_cx) = message else {
                            return Ok(sacp::Handled::Yes);
                        };
                        #[derive(serde::Deserialize)]
                        #[serde(rename_all = "camelCase")]
                        struct SpawnRequest {
                            session_id: String,
                            task: String,
                            name: Option<String>,
                            agent_name: Option<String>,
                        }
                        let params: SpawnRequest = serde_json::from_value(req.params().clone())
                            .map_err(|e| sacp::util::internal_error(format!("Invalid _session/spawn params: {}", e)))?;
                        let parent_session_id = SessionId::new(params.session_id);
                        let result = session_tx
                            .spawn_orchestrated_session(
                                &parent_session_id,
                                params.agent_name.unwrap_or_else(|| "kiro_default".to_string()),
                                params.task,
                                params.name,
                                None,
                                None,
                                true, // TUI-spawned sessions are persistent — stay alive for follow-up
                            )
                            .await
                            .map_err(|e| sacp::util::internal_error(format!("Spawn failed: {}", e)))?;
                        req_cx.respond(serde_json::json!({ "sessionId": result.session_id, "name": result.name }))?;
                        return Ok(sacp::Handled::Yes);
                    }
                    if method == methods::MESSAGE_SEND {
                        let MessageCx::Request(req, req_cx) = message else {
                            return Ok(sacp::Handled::Yes);
                        };
                        #[derive(serde::Deserialize)]
                        #[serde(rename_all = "camelCase")]
                        struct MessageSendRequest {
                            session_id: String,
                            content: String,
                        }
                        if let Ok(params) = serde_json::from_value::<MessageSendRequest>(req.params().clone()) {
                            let target = sacp::schema::SessionId::new(params.session_id);
                            if let Ok(handle) = session_tx.get_session_handle(&target).await {
                                tokio::spawn(async move {
                                    let _ = handle.wake_session(params.content).await;
                                });
                            }
                        }
                        req_cx.respond(serde_json::json!({ "ok": true }))?;
                        return Ok(sacp::Handled::Yes);
                    }

                    // Handle extension notifications
                    if let MessageCx::Notification(notif) = &message {
                        match notif.method() {
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
            stdin_reader,
        ));

    // Race serve against SIGTERM/SIGINT/pipe-close. sacp's serve() doesn't exit
    // on transport EOF (merged stream keeps other senders alive), so we detect
    // it independently via stdin_closed.
    #[cfg(unix)]
    {
        let mut sigterm = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("failed to install SIGTERM handler");
        tokio::select! {
            result = serve_future => {
                if let Err(e) = result {
                    error!("Connection error: {}", e);
                }
            }
            _ = sigterm.recv() => {
                info!("Received SIGTERM, shutting down");
            }
            _ = tokio::signal::ctrl_c() => {
                info!("Received SIGINT, shutting down");
            }
            _ = stdin_closed => {
                info!("Stdin closed (parent pipe gone), shutting down");
            }
        }
    }
    #[cfg(not(unix))]
    {
        tokio::select! {
            result = serve_future => {
                if let Err(e) = result {
                    error!("Connection error: {}", e);
                }
            }
            _ = tokio::signal::ctrl_c() => {
                info!("Received SIGINT, shutting down");
            }
            _ = stdin_closed => {
                info!("Stdin closed (parent pipe gone), shutting down");
            }
        }
    }

    // Gracefully shut down all sessions so MCP child processes are cleaned up
    // before the tokio runtime exits. Timeout ensures we don't hang indefinitely
    // if an actor is stuck.
    if tokio::time::timeout(std::time::Duration::from_secs(8), session_manager_handle.shutdown())
        .await
        .is_err()
    {
        warn!("Graceful shutdown timed out, some MCP processes may not have been cleaned up");
    }

    // Safety net: if the tokio runtime hangs during shutdown (e.g. a blocking
    // thread stuck in a syscall), force-exit after giving telemetry time to flush.
    // See https://github.com/tokio-rs/tokio/issues/2466
    std::thread::spawn(|| {
        std::thread::sleep(std::time::Duration::from_secs(3));
        #[allow(clippy::exit)]
        std::process::exit(0);
    });

    Ok(ExitCode::SUCCESS)
}

/// Send agent loading notifications to the TUI client.
///
/// Notifies about:
/// - Agent not found (fell back to default)
/// - Agent config parse errors from startup
fn send_agent_load_notifications(
    cx: &JrConnectionCx<AgentToClient>,
    session_id: &SessionId,
    requested_agent_name: &Option<String>,
    agent_config_errors: &[super::session_manager::AgentConfigLoadError],
    requested_model_name: &Option<String>,
    current_model_id: &str,
) {
    use super::extensions::{
        AgentConfigErrorNotification,
        AgentNotFoundNotification,
        ModelNotFoundNotification,
        methods,
    };

    if let Some(requested) = requested_agent_name {
        let notif = AgentNotFoundNotification {
            session_id: session_id.clone(),
            requested_agent: requested.clone(),
            fallback_agent: agent::consts::DEFAULT_AGENT_NAME.to_string(),
        };
        if let Ok(raw) = serde_json::value::to_raw_value(&notif) {
            let ext = sacp::schema::ExtNotification::new(methods::AGENT_NOT_FOUND, std::sync::Arc::from(raw));
            let _ = cx.send_notification(sacp::schema::AgentNotification::ExtNotification(ext));
        }
    }

    if let Some(requested) = requested_model_name {
        let notif = ModelNotFoundNotification {
            session_id: session_id.clone(),
            requested_model: requested.clone(),
            fallback_model: current_model_id.to_string(),
        };
        if let Ok(raw) = serde_json::value::to_raw_value(&notif) {
            let ext = sacp::schema::ExtNotification::new(methods::MODEL_NOT_FOUND, std::sync::Arc::from(raw));
            let _ = cx.send_notification(sacp::schema::AgentNotification::ExtNotification(ext));
        }
    }

    for error in agent_config_errors {
        let notif = AgentConfigErrorNotification {
            session_id: session_id.clone(),
            path: error.path.clone(),
            error: error.message.clone(),
        };
        if let Ok(raw) = serde_json::value::to_raw_value(&notif) {
            let ext = sacp::schema::ExtNotification::new(methods::AGENT_CONFIG_ERROR, std::sync::Arc::from(raw));
            let _ = cx.send_notification(sacp::schema::AgentNotification::ExtNotification(ext));
        }
    }
}

fn to_session_mode_state(current: String, agents: Vec<AgentInfo>) -> SessionModeState {
    let modes = agents
        .into_iter()
        .map(|agent| {
            let mut mode = SessionMode::new(agent.name.clone(), agent.name);
            if let Some(desc) = agent.description {
                mode = mode.description(desc);
            }
            if let Some(welcome) = agent.welcome_message {
                let mut meta = serde_json::Map::new();
                meta.insert("welcomeMessage".to_string(), serde_json::Value::String(welcome));
                mode = mode.meta(meta);
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

#[cfg(test)]
mod get_tool_content_tests {
    use std::fs;
    use std::path::PathBuf;

    use agent::tools::fs_read::FsRead;
    use agent::tools::fs_write::{
        FileCreate,
        FsWrite,
    };
    use agent::tools::{
        BuiltInTool,
        Tool,
        ToolKind as AgentToolKind,
    };
    use agent::util::providers::HomeProvider;
    use agent::util::test::{
        TestBase,
        TestProvider,
    };
    use sacp::schema::ToolCallContent;

    use super::get_tool_content_impl;

    fn make_create_tool(path: &str, content: &str) -> Tool {
        Tool {
            tool_use_purpose: None,
            kind: AgentToolKind::BuiltIn(BuiltInTool::FileWrite(FsWrite::Create(FileCreate {
                path: path.to_string(),
                content: content.to_string(),
                ..Default::default()
            }))),
        }
    }

    /// Existing file at an absolute path: old_text must be populated from disk.
    #[tokio::test]
    async fn test_create_existing_file_absolute_path_includes_old_text() {
        let test_base = TestBase::new().await;
        let file_path = test_base.join("test.txt");
        fs::write(&file_path, "original content").unwrap();

        let tool = make_create_tool(file_path.to_str().unwrap(), "new content");
        let result = get_tool_content_impl(&tool, test_base.provider());

        let expected_path = file_path.canonicalize().unwrap();
        assert_eq!(result.len(), 1);
        match &result[0] {
            ToolCallContent::Diff(diff) => {
                assert_eq!(diff.old_text.as_deref(), Some("original content"));
                assert_eq!(diff.new_text, "new content");
                assert_eq!(diff.path, expected_path);
            },
            _ => panic!("expected Diff"),
        }
    }

    /// New (non-existent) file: old_text must be None.
    #[test]
    fn test_create_new_file_has_no_old_text() {
        let provider = TestProvider::new();
        let tool = make_create_tool("/nonexistent/path/to/file.rs", "new content");
        let result = get_tool_content_impl(&tool, &provider);

        assert_eq!(result.len(), 1);
        match &result[0] {
            ToolCallContent::Diff(diff) => {
                assert!(diff.old_text.is_none(), "new file should have no old text");
                assert_eq!(diff.new_text, "new content");
            },
            _ => panic!("expected Diff"),
        }
    }

    /// Relative path must be resolved against the provider's cwd.
    #[tokio::test]
    async fn test_create_relative_path_resolves_with_cwd() {
        let test_base = TestBase::new().await;
        let file_path = test_base.join("relative_test.txt");
        fs::write(&file_path, "existing content").unwrap();

        let tool = make_create_tool("relative_test.txt", "updated content");
        let result = get_tool_content_impl(&tool, test_base.provider());

        let expected_path = file_path.canonicalize().unwrap();
        assert_eq!(result.len(), 1);
        match &result[0] {
            ToolCallContent::Diff(diff) => {
                assert_eq!(
                    diff.old_text.as_deref(),
                    Some("existing content"),
                    "relative path should be resolved against the provider's cwd"
                );
                assert_eq!(diff.new_text, "updated content");
                assert_eq!(diff.path, expected_path);
            },
            _ => panic!("expected Diff"),
        }
    }

    /// Tilde path (~/…) must be expanded to the provider's home directory.
    #[tokio::test]
    async fn test_create_tilde_path_resolves_home() {
        let test_base = TestBase::new().await;
        let home = test_base.provider().home().expect("TestBase should configure HOME");
        let file_path = home.join("tilde_test.txt");
        fs::write(&file_path, "home content").unwrap();

        let tool = make_create_tool("~/tilde_test.txt", "new home content");
        let result = get_tool_content_impl(&tool, test_base.provider());

        let expected_path = file_path.canonicalize().unwrap();
        assert_eq!(result.len(), 1);
        match &result[0] {
            ToolCallContent::Diff(diff) => {
                assert_eq!(
                    diff.old_text.as_deref(),
                    Some("home content"),
                    "tilde path should resolve to the provider's home directory"
                );
                assert_eq!(diff.path, expected_path);
            },
            _ => panic!("expected Diff"),
        }
    }

    /// StrReplace variant: old_text must come from old_str, new_text from new_str.
    #[test]
    fn test_str_replace_uses_old_str() {
        let provider = TestProvider::new();
        let fs_write: FsWrite = serde_json::from_value(serde_json::json!({
            "command": "strReplace",
            "path": "/some/file.rs",
            "oldStr": "old code",
            "newStr": "new code"
        }))
        .unwrap();

        let tool = Tool {
            tool_use_purpose: None,
            kind: AgentToolKind::BuiltIn(BuiltInTool::FileWrite(fs_write)),
        };
        let result = get_tool_content_impl(&tool, &provider);

        assert_eq!(result.len(), 1);
        match &result[0] {
            ToolCallContent::Diff(diff) => {
                assert_eq!(diff.old_text.as_deref(), Some("old code"));
                assert_eq!(diff.new_text, "new code");
                assert_eq!(diff.path, PathBuf::from("/some/file.rs"));
            },
            _ => panic!("expected Diff"),
        }
    }

    /// StrReplace with a relative path: Diff.path must be resolved against cwd.
    #[test]
    fn test_str_replace_relative_path_resolves_with_cwd() {
        let provider = TestProvider::new().with_cwd("/workspace/project");
        let fs_write: FsWrite = serde_json::from_value(serde_json::json!({
            "command": "strReplace",
            "path": "src/lib.rs",
            "oldStr": "old code",
            "newStr": "new code"
        }))
        .unwrap();

        let tool = Tool {
            tool_use_purpose: None,
            kind: AgentToolKind::BuiltIn(BuiltInTool::FileWrite(fs_write)),
        };
        let result = get_tool_content_impl(&tool, &provider);

        assert_eq!(result.len(), 1);
        match &result[0] {
            ToolCallContent::Diff(diff) => {
                assert_eq!(
                    diff.path,
                    PathBuf::from("/workspace/project/src/lib.rs"),
                    "relative StrReplace path should be resolved against cwd"
                );
                assert_eq!(diff.old_text.as_deref(), Some("old code"));
                assert_eq!(diff.new_text, "new code");
            },
            _ => panic!("expected Diff"),
        }
    }

    /// Insert variant must return no content (empty Vec).
    #[test]
    fn test_insert_returns_empty_content() {
        let provider = TestProvider::new();
        let fs_write: FsWrite = serde_json::from_value(serde_json::json!({
            "command": "insert",
            "path": "/some/file.rs",
            "insertLine": 5,
            "content": "inserted line"
        }))
        .unwrap();

        let tool = Tool {
            tool_use_purpose: None,
            kind: AgentToolKind::BuiltIn(BuiltInTool::FileWrite(fs_write)),
        };
        let result = get_tool_content_impl(&tool, &provider);

        assert!(result.is_empty(), "Insert should produce no ToolCallContent");
    }

    /// Non-FileWrite variant (e.g., FileRead) must return an empty Vec.
    #[test]
    fn test_non_file_write_tool_returns_empty() {
        let provider = TestProvider::new();
        let tool = Tool {
            tool_use_purpose: None,
            kind: AgentToolKind::BuiltIn(BuiltInTool::FileRead(FsRead { operations: vec![] })),
        };
        let result = get_tool_content_impl(&tool, &provider);

        assert!(result.is_empty(), "Non-FileWrite tool should return empty content");
    }
}
