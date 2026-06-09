use std::borrow::Cow;
use std::collections::{
    HashMap,
    HashSet,
};
use std::path::PathBuf;
use std::process::ExitCode;
use std::str::FromStr;
use std::sync::Arc;

use agent::agent_config::LoadedAgentConfig;
use agent::agent_loop::model::Model;
use agent::agent_loop::protocol::{
    AgentLoopEventKind,
    LoopError,
    StreamResult,
};
use agent::agent_loop::types::{
    ContentBlock as AgentContentBlock,
    ImageBlock,
    ImageFormat,
    ImageSource,
    StreamErrorKind,
    StreamEvent,
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
    CommandResult,
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
    Agent as AgentToClient,
    ConnectionTo,
    Dispatch,
    Responder,
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
    SessionCreatedReason,
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
use crate::database::settings::Setting;
use crate::os::Os;
use crate::telemetry::core::{
    Event,
    RecordUserTurnCompletionArgs,
    estimated_cost_usd,
};
use crate::telemetry::observer::{
    AcpClientInfo,
    TelemetryContext,
    TelemetryObserver,
    TelemetryObserverHandle,
};
use crate::telemetry::{
    EventType,
    TelemetryResult,
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
        request_cx: Responder<PromptResponse>,
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
    GetModelId {
        respond_to: oneshot::Sender<String>,
    },
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
    /// Get invocable skills from agent_config resources.
    GetSkills {
        respond_to: oneshot::Sender<Result<HashMap<String, Vec<Prompt>>, String>>,
    },
    /// Resolve a skill by name, returning its content (frontmatter stripped).
    ResolveSkill {
        name: String,
        respond_to: oneshot::Sender<Result<Option<String>, String>>,
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
    SendExtNotification {
        method: String,
        params: serde_json::Value,
    },
    /// Get tool info for advertising.
    GetToolInfo {
        respond_to: oneshot::Sender<Result<Vec<agent::tui_commands::ToolInfo>, String>>,
    },
    /// Get MCP server info for advertising.
    GetMcpServerInfo {
        respond_to: oneshot::Sender<Result<Vec<agent::tui_commands::McpServerInfo>, String>>,
    },
    /// Graceful shutdown: terminate the agent and await MCP cleanup.
    Shutdown {
        respond_to: oneshot::Sender<()>,
    },
    /// Trigger command/prompt advertising to the client.
    AdvertiseCommands,
    EmitInitialMetadata,
    /// Queue an MCP registry refresh. The session forwards the registry to its agent
    /// the next time it observes that no prompt is in flight (defer-until-idle).
    RefreshMcpRegistry {
        registry: Box<dyn agent::mcp::McpRegistry>,
    },
    /// Background goal re-injection task failed after retries.
    GoalReinjectionFailed {
        tool_call_id: String,
        error: String,
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
        request_cx: Responder<PromptResponse>,
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

    /// Queue an MCP registry refresh. The session will apply it when idle.
    pub async fn refresh_mcp_registry(&self, registry: Box<dyn agent::mcp::McpRegistry>) -> Result<(), sacp::Error> {
        self.tx.send(AcpSessionRequest::RefreshMcpRegistry { registry }).await
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

    /// Get invocable skills from agent_config resources
    pub async fn get_skills(&self) -> Result<HashMap<String, Vec<Prompt>>, String> {
        let (respond_to, rx) = oneshot::channel();
        if self.tx.send(AcpSessionRequest::GetSkills { respond_to }).await.is_err() {
            return Err("Channel closed".to_string());
        }
        rx.await
            .map_err(|e| format!("Response channel closed: {e}").to_string())?
    }

    /// Resolve a skill by name, returning its content (frontmatter stripped)
    pub async fn resolve_skill(&self, name: String) -> Result<Option<String>, String> {
        let (respond_to, rx) = oneshot::channel();
        if self
            .tx
            .send(AcpSessionRequest::ResolveSkill { name, respond_to })
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

    /// Fire-and-forget: tell the session to emit initial metadata (context usage, effort).
    pub async fn emit_initial_metadata(&self) {
        let _ = self.tx.send(AcpSessionRequest::EmitInitialMetadata).await;
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
    /// `Some` only for subagent sessions; holds the parent session's ID.
    pub parent_session_id: Option<String>,
    /// Why this session was created. Combined with `parent_session_id`, used
    /// to distinguish subagent vs rewind-fork at session-load time.
    pub session_created_reason: SessionCreatedReason,
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
            parent_session_id: None,
            session_created_reason: SessionCreatedReason::default(),
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
    pub fn parent_session_id(mut self, id: String) -> Self {
        self.parent_session_id = Some(id);
        self
    }

    pub fn subagent_info(mut self, info: Option<SubagentInfo>) -> Self {
        self.subagent_info = info;
        self
    }
}

/// Builder for constructing and spawning an [`AcpSession`] actor.
///
/// NOTE on security defaults: `mcp_enabled` and `web_tools_enabled` default to
/// `false` (fail-closed) for governance-sensitive fields. This is intentionally
/// opposite of `AgentSettings`, where those same fields default to `true` (since
/// a default `AgentSettings` is used outside of a managed session context).
/// The `Default` impl is explicit rather than derived so a future developer
/// can't accidentally change the fail-closed semantics by adding/reordering fields.
pub struct AcpSessionBuilder<'a> {
    os: Option<Os>,
    session_id: Option<String>,
    cwd: Option<PathBuf>,
    load: bool,
    initial_agent_config: Option<Cow<'a, LoadedAgentConfig>>,
    user_embedded_msg: Option<&'a str>,
    is_subagent: bool,
    parent_session_id: Option<String>,
    global_mcp_path: Option<&'a PathBuf>,
    local_mcp_path: Option<&'a PathBuf>,
    model_id: Option<&'a str>,
    effort: Option<&'a str>,
    session_tx: Option<SessionManagerHandle>,
    client_cx: Option<ConnectionTo<sacp::Client>>,
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
    /// Whether web tools (web_search, web_fetch) are enabled by governance.
    /// Fail-closed default — callers MUST set this from resolved governance.
    web_tools_enabled: bool,
    /// Whether MCP is enabled by governance (Kiro console MCP toggle).
    /// Fail-closed default — callers MUST set this from resolved governance.
    mcp_enabled: bool,
    /// Optional MCP registry forwarded to [`agent::Agent::new`]. The agent
    /// applies the registry to its config before launching MCP servers and
    /// re-applies on swap / refresh.
    mcp_registry: Option<Box<dyn agent::mcp::McpRegistry>>,
}

#[allow(clippy::derivable_impls)] // intentional — see struct doc; locks in fail-closed semantics
impl<'a> Default for AcpSessionBuilder<'a> {
    fn default() -> Self {
        Self {
            os: None,
            session_id: None,
            cwd: None,
            load: false,
            initial_agent_config: None,
            user_embedded_msg: None,
            is_subagent: false,
            parent_session_id: None,
            global_mcp_path: None,
            local_mcp_path: None,
            model_id: None,
            effort: None,
            session_tx: None,
            client_cx: None,
            mock_registry: None,
            code_intelligence: None,
            available_agents: Vec::new(),
            agent_configs: Vec::new(),
            current_agent_name: None,
            trust_all_tools: false,
            trust_tools: None,
            acp_client_info: None,
            telemetry_event_store: None,
            subagent_info: None,
            legacy_session_exporter: None,
            session_injected_mcp_servers: Vec::new(),
            // Fail-closed: governance must be explicitly enabled by caller.
            web_tools_enabled: false,
            mcp_enabled: false,
            mcp_registry: None,
        }
    }
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

    pub fn parent_session_id(mut self, id: Option<String>) -> Self {
        self.parent_session_id = id;
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

    pub fn effort(mut self, level: Option<&'a str>) -> Self {
        self.effort = level;
        self
    }

    pub fn session_tx(mut self, session_tx: SessionManagerHandle) -> Self {
        self.session_tx.replace(session_tx);
        self
    }

    pub fn connection_cx(mut self, cx: ConnectionTo<sacp::Client>) -> Self {
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

    pub fn web_tools_enabled(mut self, enabled: bool) -> Self {
        self.web_tools_enabled = enabled;
        self
    }

    pub fn mcp_enabled(mut self, enabled: bool) -> Self {
        self.mcp_enabled = enabled;
        self
    }

    /// Set the MCP registry that the agent will apply to its config before
    /// launching MCP servers. Pass `None` (or simply skip this setter) when
    /// the host has no registry — the agent will use the config as-is.
    pub fn mcp_registry(mut self, registry: Option<Box<dyn agent::mcp::McpRegistry>>) -> Self {
        self.mcp_registry = registry;
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
    pub async fn start_session(mut self) -> eyre::Result<(AcpSessionHandle, oneshot::Receiver<()>, Option<String>)> {
        let os = self.os.take().ok_or_else(|| eyre::eyre!("Os is required"))?;

        let (tx, rx) = mpsc::channel(32);
        let (ready_tx, ready_rx) = oneshot::channel();
        let subagent_info = self.subagent_info.clone();
        let self_tx = tx.clone();
        let session = AcpSession::with_builder(os, rx, self_tx, self).await?;
        let initial_model_id = session.rts_state.model_id();
        tokio::spawn(async move { session.main_loop(ready_tx).await });

        Ok((
            AcpSessionHandle {
                tx: InnerSender::Strong(tx),
                _subagent_info: subagent_info,
            },
            ready_rx,
            initial_model_id,
        ))
    }
}

/// An actor representing an active ACP session.
///
/// Each session owns:
/// - An [`Agent`](agent::Agent) for LLM interactions
/// - A [`ConnectionTo`] for direct client communication (egress)
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
    connection_cx: ConnectionTo<sacp::Client>,
    is_subagent: bool,
    previous_agent_name: Option<String>,
    pending_plan: Option<String>,
    pending_swap: Option<agent::agent_config::LoadedAgentConfig>,
    pending_prompt_response: Option<tokio::sync::Mutex<Responder<PromptResponse>>>,
    /// Agent config to swap to when the session becomes idle (set by registry refresh)
    pending_mcp_registry: Option<Box<dyn agent::mcp::McpRegistry>>,
    compaction_summary: Option<String>,
    os: Os,
    cwd: PathBuf,
    telemetry_observer: TelemetryObserverHandle,
    legacy_session_exporter: Arc<dyn LegacySessionExporter>,
    /// MCP servers injected by the ACP client at session creation time.
    /// Preserved across agent swaps so they are re-merged into each new agent config.
    session_injected_mcp_servers: Vec<(String, agent::agent_config::definitions::McpServerConfig)>,
    /// Whether MCP is enabled by governance (Kiro console MCP toggle).
    mcp_enabled: bool,
    /// In-memory ring buffer of per-request metadata for `/stats`.
    request_stats: super::request_stats::RequestStats,
    /// Active goal loop controller. None if no goal is set.
    goal_controller: Option<super::goal::GoalController>,
    /// Sender back to self — used by background tasks (goal re-injection)
    /// to notify the actor of async outcomes without blocking the event loop.
    self_tx: mpsc::Sender<AcpSessionRequest>,
    chat_session_started_emitted: bool,
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
            request_stats: &self.request_stats,
            goal_controller: self.goal_controller.as_ref(),
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
                state.set_goal(self.goal_controller.as_ref().map(|c| c.to_snapshot()));
                if let Err(e) = self.session_db.update_state(state) {
                    warn!("Failed to persist session state: {}", e);
                }
            },
            Err(e) => {
                error!("Failed to get agent snapshot for session persistence: {}", e);
            },
        }
    }

    /// Evaluate goal completion on EndTurn.
    /// After EndTurn: if goal is active, re-inject prompt directly.
    /// Agent is already Idle when EndTurn fires (set_active_state(Idle) happens before broadcast).
    ///
    /// Note: previously this early-released to the user when the agent ended a turn without
    /// using any tools, on the theory that the agent was asking for clarification. That was
    /// too eager — agents often emit text-only intermediate replies that aren't questions.
    /// We now always count it as an iteration and re-inject the nudge. A future LLM-judge
    /// will replace this with a principled completion check.
    async fn evaluate_goal_on_end_turn(&mut self) {
        let Some(ref mut goal_ctrl) = self.goal_controller else {
            return;
        };

        goal_ctrl.iteration += 1;

        if goal_ctrl.iteration >= goal_ctrl.definition.max_iterations {
            goal_ctrl.mark_exhausted(format!(
                "Max iterations ({}) reached",
                goal_ctrl.definition.max_iterations
            ));
            self.send_goal_status_notification();
            self.emit_goal_telemetry("exhausted");
            self.release_goal_response().await;
            return;
        }

        let iteration = goal_ctrl.iteration;
        let max_iterations = goal_ctrl.definition.max_iterations;
        let description = goal_ctrl.definition.description.clone();

        let prompt = goal_ctrl.build_prompt(iteration + 1);

        self.send_goal_status_notification();

        let tool_call_id = format!("goal-iter-{}", iteration);
        let goal_content: ToolCallContent = ContentBlock::Text(TextContent::new(description.clone())).into();
        let _ = self.send_session_notification(SessionUpdate::ToolCall(
            ToolCall::new(
                ToolCallId::new(tool_call_id.clone()),
                format!("⟳ Goal iteration {}/{}", iteration + 1, max_iterations),
            )
            .kind(ToolKind::Other)
            .status(ToolCallStatus::Pending)
            .content(vec![goal_content]),
        ));

        tracing::info!("Goal: re-injecting iteration {}/{}", iteration, max_iterations);

        // The agent sets ActiveState::Idle before broadcasting EndTurn, but the
        // broadcast is buffered. By the time we process EndTurn here, the agent's
        // internal channel may not have drained yet. Retry with backoff in a
        // background task so we don't block the actor's event loop.
        let agent = self.agent.clone();
        let self_tx = self.self_tx.clone();
        let tool_call_id_clone = tool_call_id.clone();
        tokio::spawn(async move {
            let mut last_err = None;
            for attempt in 0..5u64 {
                tokio::time::sleep(std::time::Duration::from_millis(100 * (attempt + 1))).await;
                match agent
                    .send_prompt(agent::protocol::SendPromptArgs {
                        content: vec![agent::protocol::ContentChunk::Text(prompt.clone())],
                        should_continue_turn: None,
                    })
                    .await
                {
                    Ok(()) => {
                        last_err = None;
                        break;
                    },
                    Err(e) => {
                        tracing::warn!("Goal re-injection attempt {} failed: {e:?}", attempt + 1);
                        last_err = Some(e);
                    },
                }
            }
            if let Some(e) = last_err {
                tracing::error!("Goal re-injection failed after retries: {e:?}");
                // Notify the actor to handle the failure. Use self_tx to send
                // a GoalReinjectionFailed message back without blocking.
                let _ = self_tx
                    .send(AcpSessionRequest::GoalReinjectionFailed {
                        tool_call_id: tool_call_id_clone,
                        error: format!("{e:?}"),
                    })
                    .await;
            }
        });

        // Mark tool call as completed optimistically — the background task
        // will send GoalReinjectionFailed if it actually fails.
        let _ = self.send_session_notification(SessionUpdate::ToolCallUpdate(ToolCallUpdate::new(
            ToolCallId::new(tool_call_id),
            ToolCallUpdateFields::new().status(ToolCallStatus::Completed),
        )));
    }

    /// Handle a dispatch failure during an active goal. Does NOT increment the
    /// iteration counter. Retries with exponential backoff up to 3 times, then
    /// pauses the goal (same as Ctrl+C from the user's perspective).
    async fn handle_goal_dispatch_failure(&mut self) {
        let Some(ref mut goal_ctrl) = self.goal_controller else {
            return;
        };

        let should_retry = goal_ctrl.record_failure();
        let failures = goal_ctrl.consecutive_failures;

        if !should_retry {
            // Retries exhausted — pause the goal.
            tracing::warn!("Goal paused after {failures} consecutive dispatch failures");
            self.send_goal_status_notification();
            self.emit_goal_telemetry("dispatch_failure_exhausted");
            self.release_goal_response().await;
            return;
        }

        let backoff = goal_ctrl.backoff_delay();
        let iteration = goal_ctrl.iteration;
        let prompt = goal_ctrl.build_prompt(iteration + 1);

        tracing::info!(
            "Goal: dispatch failure #{failures}, retrying after {}s",
            backoff.as_secs()
        );

        let agent = self.agent.clone();
        let self_tx = self.self_tx.clone();
        tokio::spawn(async move {
            tokio::time::sleep(backoff).await;
            for attempt in 0..5u64 {
                tokio::time::sleep(std::time::Duration::from_millis(100 * (attempt + 1))).await;
                match agent
                    .send_prompt(agent::protocol::SendPromptArgs {
                        content: vec![agent::protocol::ContentChunk::Text(prompt.clone())],
                        should_continue_turn: None,
                    })
                    .await
                {
                    Ok(()) => return,
                    Err(e) => {
                        tracing::warn!("Goal retry re-injection attempt {} failed: {e:?}", attempt + 1);
                    },
                }
            }
            // All re-injection attempts failed — notify actor.
            let _ = self_tx
                .send(AcpSessionRequest::GoalReinjectionFailed {
                    tool_call_id: format!("goal-retry-{}", iteration),
                    error: "Re-injection failed after dispatch failure retry".into(),
                })
                .await;
        });
    }

    /// Release pending_prompt_response back to TUI (goal done/exhausted/error).
    async fn release_goal_response(&mut self) {
        if let Some(respond_to) = self.pending_prompt_response.take() {
            self.persist_session_state().await;
            let respond_to = respond_to.into_inner();
            let _ = respond_to.respond(PromptResponse::new(StopReason::EndTurn));
        }
    }

    /// Handle the agent calling the `goal` built-in tool.
    async fn handle_goal_action(&mut self, action: agent::tools::goal::GoalTool) {
        use agent::tools::goal::GoalTool;
        match action {
            GoalTool::Complete { .. } => {
                if let Some(ref mut ctrl) = self.goal_controller {
                    ctrl.state = super::goal::GoalState::Completed;
                    self.send_goal_status_notification();
                    self.emit_goal_telemetry("completed");
                }
                // Don't release here — let EndTurn fire naturally, which will see
                // goal is no longer WaitingForTurn and release the response.
            },
        }
    }

    /// If the command result is a goal-set, initialize the controller, hold the
    /// prompt response (so the TUI stays in streaming mode), and spawn the first
    /// iteration. Otherwise respond immediately with EndTurn.
    fn handle_goal_or_respond(
        &mut self,
        result: agent::tui_commands::CommandResult,
        request_cx: Responder<PromptResponse>,
    ) {
        let is_goal_set = result.success
            && result
                .data
                .as_ref()
                .is_some_and(|d| d.get("goal_action").and_then(|a| a.as_str()) == Some("set"));

        if is_goal_set
            && let Some(ref data) = result.data
            && let Some(def) = data.get("definition")
            && let Ok(definition) = serde_json::from_value::<agent::goal::GoalDefinition>(def.clone())
        {
            let ctrl = super::goal::GoalController::new(definition);
            let goal_prompt = ctrl.build_prompt(1);
            self.goal_controller = Some(ctrl);
            self.send_goal_status_notification();
            self.pending_prompt_response = Some(tokio::sync::Mutex::new(request_cx));
            let agent = self.agent.clone();
            tokio::spawn(async move {
                if let Err(e) = agent
                    .send_prompt(SendPromptArgs {
                        content: vec![agent::protocol::ContentChunk::Text(goal_prompt)],
                        should_continue_turn: None,
                    })
                    .await
                {
                    tracing::error!("Goal initial send_prompt failed: {e:?}");
                }
            });
            return;
        }

        // Non-goal-set or parse failure: respond immediately
        if let Err(e) = request_cx.respond(PromptResponse::new(StopReason::EndTurn)) {
            error!("Failed to respond to slash command: {e}");
        }
    }

    /// On EndTurn, decide whether to re-inject the goal prompt for another
    /// iteration or release the pending prompt response back to the TUI.
    async fn handle_end_turn_goal_or_respond(&mut self, md: &agent::agent_loop::protocol::UserTurnMetadata) {
        let goal_should_continue = self.goal_controller.as_ref().is_some_and(|c| c.should_continue());
        let was_cancelled = matches!(
            md.end_reason,
            agent::agent_loop::protocol::LoopEndReason::Cancelled
                | agent::agent_loop::protocol::LoopEndReason::ToolUseRejected
        );
        let was_error = md.end_reason == agent::agent_loop::protocol::LoopEndReason::Error
            || (goal_should_continue && Self::should_simulate_goal_failure());

        if goal_should_continue && !was_cancelled {
            if was_error {
                // Turn ended due to dispatch failure — don't count as iteration.
                // Retry with backoff or pause if retries exhausted.
                self.handle_goal_dispatch_failure().await;
            } else {
                // Successful turn — reset failure counter, proceed normally.
                if let Some(ref mut ctrl) = self.goal_controller {
                    ctrl.record_success();
                }
                // Keep pending_prompt_response held — the re-injected prompt
                // triggers another turn whose EndTurn will eventually respond.
                self.evaluate_goal_on_end_turn().await;
            }
        } else {
            // Goal complete, exhausted, or normal non-goal turn — release to TUI.
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
    }

    /// Test-only: when `KIRO_GOAL_SIMULATE_FAILURE=1` is set, every turn during
    /// an active goal is treated as a dispatch failure. Used by Knight Rider to
    /// validate retry + pause behavior without needing real network failures.
    #[cfg(any(test, debug_assertions))]
    fn should_simulate_goal_failure() -> bool {
        std::env::var("KIRO_GOAL_SIMULATE_FAILURE").is_ok_and(|v| v == "1")
    }

    #[cfg(not(any(test, debug_assertions)))]
    fn should_simulate_goal_failure() -> bool {
        false
    }

    fn send_goal_status_notification(&self) {
        let Some(ref ctrl) = self.goal_controller else { return };
        let (state, message) = match &ctrl.state {
            super::goal::GoalState::WaitingForTurn => ("active", ctrl.definition.description.clone()),
            super::goal::GoalState::Completed => {
                ("completed", format!("Goal achieved in {} iterations", ctrl.iteration))
            },
            super::goal::GoalState::Exhausted { reason } => ("exhausted", reason.clone()),
        };
        let elapsed = (chrono::Utc::now() - ctrl.started_at).num_seconds().max(0) as u64;
        let _ = self.send_ext_notification(
            super::extensions::methods::GOAL_STATUS,
            super::extensions::GoalStatusNotification {
                state: state.to_string(),
                iteration: ctrl.iteration,
                max_iterations: ctrl.definition.max_iterations,
                message: Some(message),
                elapsed_secs: elapsed,
            },
        );
    }

    /// Emit a `kirocli_goalCompleted` telemetry event when a goal reaches a
    /// terminal state. Reads iteration count, definition flags, and duration
    /// from `self.goal_controller`. Caller passes the terminal label
    /// ("completed" | "exhausted" | "cancelled").
    ///
    /// Must be called BEFORE `self.goal_controller` is cleared/replaced so
    /// the read sees the final state.
    fn emit_goal_telemetry(&self, terminal_state: &str) {
        let Some(ref ctrl) = self.goal_controller else {
            return;
        };
        let duration_sec = (chrono::Utc::now() - ctrl.started_at).num_seconds().max(0);
        let _ = self.os.telemetry.send_goal_completed(
            Some(self.session_id_str.clone()),
            terminal_state.to_string(),
            ctrl.iteration as i64,
            ctrl.definition.max_iterations as i64,
            duration_sec,
        );
    }

    fn emit_chat_slash_command_telemetry(&self, command: String, subcommand: Option<String>, result: &CommandResult) {
        self.telemetry_observer
            .send_telemetry_event(Event::new(EventType::ChatSlashCommandExecuted {
                conversation_id: self.session_id_str.clone(),
                command,
                subcommand,
                result: if result.success {
                    TelemetryResult::Succeeded
                } else {
                    TelemetryResult::Failed
                },
                reason: (!result.success).then(|| "CommandFailed".to_string()),
            }));
    }

    fn emit_chat_session_started_once(&mut self) {
        if self.chat_session_started_emitted || self.is_subagent {
            return;
        }
        self.chat_session_started_emitted = true;
        self.telemetry_observer
            .send_telemetry_event(Event::new(EventType::ChatSessionStarted {
                mode: kiro_telemetry::metric::Mode::from_name(&self.current_agent_name),
            }));
    }

    /// Extract metadata from a completed response stream and push to the ring buffer.
    fn record_request_stats(
        &self,
        result: &Result<agent::agent_loop::types::Message, agent::agent_loop::protocol::LoopError>,
        metadata: &agent::agent_loop::protocol::StreamMetadata,
    ) {
        let stream = metadata.stream.as_ref();
        let metrics = stream.and_then(|s| s.metrics.as_ref());
        let service = stream.and_then(|s| s.service.as_ref());
        let usage = stream.and_then(|s| s.usage.as_ref());

        let duration = metrics.map(|m| (m.request_end_time - m.request_start_time).to_std().unwrap_or_default());

        let error = match result {
            Ok(_) => None,
            Err(e) => Some(e.to_string()),
        };

        self.request_stats.push(super::request_stats::RequestRecord {
            request_id: service.and_then(|s| s.request_id.clone()),
            timestamp: metrics.map_or_else(chrono::Utc::now, |m| m.request_start_time),
            duration,
            time_to_first_chunk: metrics.and_then(|m| m.time_to_first_chunk),
            input_tokens: usage.and_then(|u| u.input_tokens),
            output_tokens: usage.and_then(|u| u.output_tokens),
            status_code: service.and_then(|s| s.status_code),
            had_tool_use: !metadata.tool_uses.is_empty(),
            error,
        });
    }

    async fn with_builder(
        os: Os,
        request_rx: mpsc::Receiver<AcpSessionRequest>,
        self_tx: mpsc::Sender<AcpSessionRequest>,
        mut builder: AcpSessionBuilder<'_>,
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
        let mut saved_additional_fields: Option<crate::cli::chat::legacy::additional_fields::AdditionalModelFields> =
            None;
        let mut saved_goal: Option<super::goal::GoalSnapshot> = None;

        let (session_db, snapshot) = if builder.load {
            // Load existing session
            let db = SessionDb::load(&session_id_str, Some(&cwd))?;
            let state = db.session().session_state;
            let entries = db.load_log_entries()?;

            // Preserve the model the session was actually using (e.g. after /model switch)
            saved_model_id = state
                .rts_model_state()
                .and_then(|s| s.model_info.as_ref().map(|m| m.model_id.clone()));
            saved_additional_fields = state.rts_model_state().and_then(|s| s.additional_fields.clone());
            saved_goal = state.goal().cloned();

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
                additional_fields: None,
            };
            let initial_state = SessionState::new(snapshot.conversation_metadata.clone(), rts_snapshot, permissions);
            let db = SessionDb::new(
                session_id_str.clone(),
                &cwd,
                initial_state,
                builder.parent_session_id.clone(),
                SessionCreatedReason::default(),
            )?;

            (db, snapshot)
        };

        let rts_state = Arc::new(RtsState::new(session_id_str.clone()).with_cwd(cwd.clone()));

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
        if let Some(model_id) = builder.model_id
            && let Err(e) = update_model_info(&api_client, &os.database, &rts_state, Some(model_id)).await
        {
            warn!("Failed to set CLI model override: {}", e);
        }

        // Restore saved effort/additional_fields from the loaded session (only if no CLI model override)
        if builder.model_id.is_none()
            && let Some(saved_af) = saved_additional_fields
        {
            rts_state.restore_additional_fields(saved_af);
        }

        // Apply CLI --effort override (silently ignored if model doesn't support it)
        if let Some(effort_level) = builder.effort
            && let Err(e) = rts_state.set_effort(effort_level)
        {
            warn!("--effort: {}", e);
        }

        // Restore goal from loaded session. The restored goal stays in WaitingForTurn
        // and will re-engage on the next user prompt — initialize() emits a status
        // notification so the user is aware they have an active goal.
        let restored_goal = saved_goal.map(super::goal::GoalController::from_snapshot);

        let snapshot = {
            let mut s = snapshot;
            s.settings.trust_all_tools = builder.trust_all_tools;
            s.settings.web_tools_enabled = builder.web_tools_enabled;
            s.settings.mcp_enabled = builder.mcp_enabled;
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
            s.settings.tool_search_enabled = os
                .database
                .settings
                .get_bool(Setting::ToolSearchEnabled)
                .unwrap_or(false);
            s.settings.tool_search_min_pct = os
                .database
                .settings
                .get(Setting::ToolSearchMinPct)
                .and_then(|v| v.as_f64());
            s.settings.tool_search_min_tokens = os
                .database
                .settings
                .get(Setting::ToolSearchMinTokens)
                .and_then(|v| v.as_u64());
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

                // Sync agent-defined KB resources before creating the store
                if let Some(cfg) = agent_config {
                    let _ = crate::util::knowledge_store::KnowledgeStore::sync_agent_resources(
                        name,
                        agent_path.as_deref(),
                        cfg.resource_paths(),
                        &os,
                    )
                    .await;
                }

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
            builder.mcp_registry.take(),
        )
        .await?;

        agent.set_sys_provider(super::acp_provider::AcpProvider::new(cwd.clone()));

        if let Some(msg) = builder.user_embedded_msg {
            agent.prepend_embedded_user_msg(msg);
        }

        let agent = agent.spawn();

        // Create telemetry observer actor
        let telemetry_context = TelemetryContext::new(
            Arc::clone(&rts_state),
            builder.acp_client_info.clone(),
            builder.is_subagent,
        );
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
            pending_plan: None,
            pending_swap: None,
            connection_cx,
            is_subagent: builder.is_subagent,
            session_db: Arc::new(session_db),
            rts_state,
            api_client,
            pending_prompt_response: None,
            pending_mcp_registry: None,
            compaction_summary: None,
            os,
            cwd,
            telemetry_observer,
            legacy_session_exporter: builder
                .legacy_session_exporter
                .unwrap_or_else(|| Arc::new(crate::agent::session::legacy_compat::NoOpLegacySessionExporter)),
            session_injected_mcp_servers: builder.session_injected_mcp_servers,
            mcp_enabled: builder.mcp_enabled,
            request_stats: Default::default(),
            goal_controller: restored_goal,
            self_tx,
            chat_session_started_emitted: false,
        })
    }

    async fn initialize(&mut self) -> eyre::Result<()> {
        // Emit historical notifications for loaded sessions
        if let Err(e) = self.emit_historical_notifications().await {
            warn!("Failed to emit historical notifications: {}", e);
        }

        // If a goal was restored from session, surface it so the user knows it's still
        // active. Otherwise the goal would silently re-engage on the next user prompt.
        if self.goal_controller.is_some() {
            self.send_goal_status_notification();
        }

        // Wait for agent to finish initialization
        loop {
            match self.agent.recv().await {
                Ok(AgentEvent::Initialized) => {
                    return Ok(());
                },
                Ok(AgentEvent::InitializeUpdate(init_event)) => {
                    self.telemetry_observer.send_event(
                        self.session_id_str.clone(),
                        AgentEvent::InitializeUpdate(init_event.clone()),
                    );
                    match init_event {
                        agent::protocol::InitializeUpdateEvent::Mcp(mcp_event) => {
                            if let Err(e) = self.handle_mcp_event(mcp_event).await {
                                error!("Failed to handle MCP event during initialization: {}", e);
                            }
                        },
                    }
                },
                Ok(event) => {
                    warn!("Unexpected event during initialization: {:?}", event);
                },
                Err(tokio::sync::broadcast::error::RecvError::Lagged(skipped)) => {
                    warn!(%skipped, "Agent event channel lagged during initialization; skipped events");
                },
                Err(tokio::sync::broadcast::error::RecvError::Closed) => {
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
            // Apply pending MCP registry refresh when idle (no prompt in progress).
            // The agent re-applies the stored registry to its current config and
            // reloads MCP servers internally; the host no longer pre-rewrites.
            #[allow(clippy::collapsible_if)]
            if self.pending_prompt_response.is_none()
                && let Some(registry) = self.pending_mcp_registry.take()
                && let Err(e) = self.agent.refresh_mcp_registry(registry).await
            {
                warn!(%e, "Failed to apply pending MCP registry refresh");
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
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(skipped)) => {
                            // The receiver couldn't keep up (e.g. a verbose shell command
                            // streaming tens of thousands of lines). Log and continue —
                            // killing the session would be much worse than dropping UI updates.
                            //
                            // Diagnostic: include `pending_prompt` so logs can correlate a lag
                            // event with an in-flight turn. If a lag fires while a prompt is
                            // pending, EndTurn / Stop(...) may be among the dropped events,
                            // leaving `pending_prompt_response` forever unresolved (silent stop).
                            warn!(
                                %skipped,
                                pending_prompt = self.pending_prompt_response.is_some(),
                                "Agent event channel lagged; skipped events"
                            );
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                            warn!("Agent event channel closed, exiting");
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

    fn current_effort(&self) -> Option<String> {
        self.rts_state.effort()
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
            effort: self.current_effort(),
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
            effort: self.current_effort(),
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
                            let telemetry_command = command.name().to_string();
                            let telemetry_subcommand = tui_command_telemetry_subcommand(&command);
                            let is_agent_swap = matches!(&command, TuiCommand::Agent(args) if args.agent_name.is_some())
                                || matches!(&command, TuiCommand::Guide(_));
                            let is_agent_create = matches!(&command, TuiCommand::Agent(args) if args.agent_name.as_deref().is_some_and(|n| n == "create" || n.starts_with("create ")));
                            let ctx = self.command_context();
                            let result = super::commands::execute(command, &ctx).await;
                            self.emit_chat_slash_command_telemetry(telemetry_command, telemetry_subcommand, &result);

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
                                        model: self.rts_state.model_id(),
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

                            // Don't send result.message as AgentMessageChunk for goal-set:
                            // it would merge with the agent's streaming output. The goal
                            // status notification already surfaces the set confirmation.
                            let is_goal_set = result.success
                                && result
                                    .data
                                    .as_ref()
                                    .is_some_and(|d| d.get("goal_action").and_then(|a| a.as_str()) == Some("set"));
                            if !result.message.is_empty() && !is_goal_set {
                                let _ = self.send_session_notification(SessionUpdate::AgentMessageChunk(
                                    SacpContentChunk::new(ContentBlock::Text(TextContent::new(format!(
                                        "{}\n",
                                        result.message
                                    )))),
                                ));
                            }

                            // Set up goal controller or respond immediately.
                            self.handle_goal_or_respond(result, request_cx);
                        },
                        slash_router::SlashRoute::Prompt { name, args, original } => {
                            self.emit_chat_session_started_once();
                            self.pending_prompt_response = Some(tokio::sync::Mutex::new(request_cx));
                            let agent = self.agent.clone();
                            let cwd = self.cwd.clone();
                            tokio::spawn(async move {
                                // Priority: local file > global file > skill > MCP (per docs)
                                if let Some(content) = agent::prompts::resolve_file_prompt(&cwd, &name) {
                                    let template = agent::prompts::PromptTemplateArgs::parse(&content);
                                    let expanded = template.expand(&content, &args);
                                    let _ = agent
                                        .send_prompt(SendPromptArgs {
                                            content: vec![agent::protocol::ContentChunk::Text(expanded)],
                                            should_continue_turn: None,
                                        })
                                        .await;
                                } else if let Some(body) = agent.resolve_skill(name.clone()).await.ok().flatten() {
                                    // Skill: frontmatter already stripped by resolve_skill()
                                    let template = agent::prompts::PromptTemplateArgs::parse(&body);
                                    let mut expanded = template.expand(&body, &args);
                                    // If the skill body has no placeholders and the user typed trailing
                                    // text, append it so the model sees the user's actual request.
                                    if !args.is_empty() && !template.has_all_args() && template.positional().is_empty()
                                    {
                                        expanded.push_str("\n\n");
                                        expanded.push_str(&args.join(" "));
                                    }
                                    let _ = agent
                                        .send_prompt(SendPromptArgs {
                                            content: vec![agent::protocol::ContentChunk::Text(expanded)],
                                            should_continue_turn: None,
                                        })
                                        .await;
                                } else {
                                    // Fall back to MCP prompt — fetch schema to map positional args to named params
                                    let schema =
                                        agent.get_mcp_prompts().await.ok().and_then(|prompts| {
                                            super::mcp_prompts::resolve_prompt_schema(prompts, &name)
                                        });
                                    let mcp_args = super::mcp_prompts::args_to_mcp_map(&args, schema.as_deref());
                                    match agent.get_mcp_prompt(name.clone(), mcp_args).await {
                                        Ok(messages) => {
                                            let resolved_text = super::mcp_prompts::extract_prompt_text(&messages);
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
                                            // Not a known prompt - send original text verbatim
                                            let _ = agent
                                                .send_prompt(SendPromptArgs {
                                                    content: vec![agent::protocol::ContentChunk::Text(original)],
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
                self.emit_chat_session_started_once();
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

                // Only update model when the new agent explicitly specifies one;
                // otherwise preserve the user's current model selection.
                if let Some(model) = agent_config.model()
                    && let Err(e) =
                        update_model_info(&self.api_client, &self.os.database, &self.rts_state, Some(model)).await
                {
                    warn!("Failed to update model during swap: {}", e);
                }
                // Reset stale context usage data since it's meaningless after swapping agents
                self.rts_state.set_context_usage_percentage(None);

                let new_name = agent_config.name().to_string();
                let new_agent_path = match agent_config.source() {
                    agent::agent_config::ConfigSource::Workspace { path }
                    | agent::agent_config::ConfigSource::Global { path } => Some(path.clone()),
                    _ => None,
                };
                let new_resources = agent_config.resource_paths().to_vec();

                // Create a new knowledge provider for the target agent BEFORE swapping
                let new_knowledge_provider = if std::env::var(KIRO_TEST_MODE).is_ok() {
                    None
                } else {
                    match crate::util::knowledge_store::KnowledgeStore::get_async_instance(
                        &self.os,
                        Some(&new_name),
                        new_agent_path.as_deref(),
                    )
                    .await
                    {
                        Ok(store) => Some(std::sync::Arc::new(
                            crate::util::knowledge_store::KnowledgeStoreProvider::new(store),
                        )
                            as std::sync::Arc<dyn agent::tools::KnowledgeProvider>),
                        Err(_) => None,
                    }
                };

                let result = self
                    .agent
                    .swap_agent(agent::protocol::SwapAgentArgs {
                        agent_config: *agent_config,
                        force: false,
                        knowledge_provider: new_knowledge_provider,
                    })
                    .await;

                if result.is_ok() {
                    // Sync KB resources for the new agent
                    let _ = crate::util::knowledge_store::KnowledgeStore::sync_agent_resources(
                        &new_name,
                        new_agent_path.as_deref(),
                        &new_resources,
                        &self.os,
                    )
                    .await;

                    self.previous_agent_name = Some(std::mem::replace(&mut self.current_agent_name, new_name));
                    self.persist_session_state().await;
                }

                let _ = respond_to.send(result);
            },
            AcpSessionRequest::SetModel { model_id, respond_to } => {
                let result =
                    update_model_info(&self.api_client, &self.os.database, &self.rts_state, Some(&model_id)).await;
                let _ = respond_to.send(result);
            },
            AcpSessionRequest::GetModelId { respond_to } => {
                let _ = respond_to.send(self.rts_state.model_id().unwrap_or_default());
            },
            AcpSessionRequest::Cancel => {
                // Send cancellation to the underlying agent so it stops any in-flight work.
                if let Err(e) = self.agent.cancel().await {
                    error!("Failed to cancel agent: {}", e);
                }
                // Immediately clear pending_prompt_response so the next prompt isn't
                // rejected with "Prompt already in progress". This is necessary because
                // the agent may not have started processing yet (e.g. slow network,
                // queued LLM call), so its Stop(Cancelled) event may never fire.
                // If the agent does emit EndTurn/Cancelled later, the .take() there
                // will be a harmless no-op since we already consumed it.
                if let Some(respond_to) = self.pending_prompt_response.take() {
                    let respond_to = respond_to.into_inner();
                    let _ = respond_to.respond(PromptResponse::new(StopReason::Cancelled));
                }
            },
            AcpSessionRequest::ExecuteCommand { command, respond_to } => {
                let telemetry_command = command.name().to_string();
                let telemetry_subcommand = tui_command_telemetry_subcommand(&command);
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
                self.emit_chat_slash_command_telemetry(telemetry_command, telemetry_subcommand, &result);

                if is_agent_swap
                    && result.success
                    && let Some(data) = &result.data
                    && let Some(name) = data.get("agent").and_then(|a| a.get("name")).and_then(|n| n.as_str())
                    && name != self.current_agent_name
                {
                    // Only update model when the new agent explicitly specifies one;
                    // otherwise preserve the user's current model selection.
                    let new_model = self
                        .agent_configs
                        .iter()
                        .find(|c| c.name() == name)
                        .and_then(|c| c.model().map(String::from));
                    if let Some(ref model) = new_model
                        && let Err(e) =
                            update_model_info(&self.api_client, &self.os.database, &self.rts_state, Some(model)).await
                    {
                        warn!("Failed to update model during agent switch: {}", e);
                    }

                    self.previous_agent_name = Some(std::mem::replace(&mut self.current_agent_name, name.to_string()));
                    let _ = self.send_ext_notification(
                        super::extensions::methods::AGENT_SWITCHED,
                        super::extensions::AgentSwitchedNotification {
                            session_id: self.session_id.clone(),
                            agent_name: self.current_agent_name.clone(),
                            previous_agent_name: self.previous_agent_name.clone(),
                            welcome_message: self.welcome_message_for(&self.current_agent_name),
                            model: self.rts_state.model_id(),
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

                // Handle goal actions from /goal command
                if result.success
                    && let Some(ref data) = result.data
                    && let Some(action) = data.get("goal_action").and_then(|a| a.as_str())
                {
                    if action == "clear" {
                        // Emit telemetry BEFORE clearing the controller so we can
                        // read iteration count / definition from it.
                        self.emit_goal_telemetry("cancelled");
                        self.goal_controller = None;
                        // Send cleared notification explicitly (controller is None so send_goal_status_notification
                        // would no-op)
                        let _ = self.send_ext_notification(
                            super::extensions::methods::GOAL_STATUS,
                            super::extensions::GoalStatusNotification {
                                state: "cleared".to_string(),
                                iteration: 0,
                                max_iterations: 0,
                                message: None,
                                elapsed_secs: 0,
                            },
                        );
                    }
                    self.send_goal_status_notification();
                }

                let _ = respond_to.send(result);

                // Persist state after commands (captures effort, model changes, etc.)
                self.persist_session_state().await;

                // Send metadata notification after every command (keeps TUI effort/context in sync)
                let notification = super::schema::MetadataNotification {
                    session_id: self.session_id_str.clone(),
                    context_usage_percentage: self.rts_state.context_usage_percentage(),
                    metering_usage: None,
                    turn_duration_ms: None,
                    effort: self.current_effort(),
                };
                if let Err(e) = self.connection_cx.send_notification(notification) {
                    warn!("Failed to send metadata after command execute: {}", e);
                }

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
                    super::schema::TuiCommandKind::Effort => super::commands::effort::get_options(&ctx),
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
                    super::schema::TuiCommandKind::Rewind
                    | super::schema::TuiCommandKind::Context
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
            AcpSessionRequest::GetSkills { respond_to } => {
                let result = self
                    .agent
                    .get_skills()
                    .await
                    .map_err(|e| format!("Failed to get skills: {}", e));
                let _ = respond_to.send(result);
            },
            AcpSessionRequest::ResolveSkill { name, respond_to } => {
                let result = self
                    .agent
                    .resolve_skill(name)
                    .await
                    .map_err(|e| format!("Failed to resolve skill: {}", e));
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
            AcpSessionRequest::EmitInitialMetadata => {
                self.emit_initial_context_usage().await;
            },
            AcpSessionRequest::RefreshMcpRegistry { registry } => {
                self.pending_mcp_registry = Some(registry);
            },
            AcpSessionRequest::GoalReinjectionFailed { tool_call_id, error } => {
                tracing::error!("Goal re-injection failed: {error}");
                let _ = self.send_session_notification(SessionUpdate::ToolCallUpdate(ToolCallUpdate::new(
                    ToolCallId::new(tool_call_id),
                    ToolCallUpdateFields::new().status(ToolCallStatus::Failed),
                )));
                if let Some(ref mut ctrl) = self.goal_controller {
                    ctrl.mark_exhausted(format!("Re-injection failed: {error}"));
                }
                self.send_goal_status_notification();
                self.emit_goal_telemetry("reinjection_failed");
                self.release_goal_response().await;
            },
        }
    }

    async fn handle_agent_event(&mut self, event: AgentEvent) {
        self.telemetry_observer
            .send_event(self.session_id_str.clone(), event.clone());

        // Capture per-request metadata into the in-memory ring buffer for /stats
        if let AgentEvent::Internal(agent::protocol::InternalEvent::AgentLoop(ref loop_event)) = event
            && let agent::agent_loop::protocol::AgentLoopEventKind::ResponseStreamEnd {
                ref result,
                ref metadata,
            } = loop_event.kind
        {
            self.record_request_stats(result, metadata);
        }

        let session_db = Arc::clone(&self.session_db);
        let rts_state = Arc::clone(&self.rts_state);

        match event {
            // MCP events during initialization (e.g., OAuth requests) come through InitializeUpdate,
            // not the regular Mcp variant, since they occur before the agent is fully initialized.
            AgentEvent::InitializeUpdate(init_event) => match init_event {
                agent::protocol::InitializeUpdateEvent::Mcp(mcp_event) => {
                    if let Err(e) = self.handle_mcp_event(mcp_event).await {
                        error!("Failed to handle MCP event during initialization: {}", e);
                    }
                },
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
                if let Some(update) = convert_update_event_to_session_update(update_event.clone())
                    && let Err(e) = self.send_session_notification(update)
                {
                    // Hot-path notification — every UpdateEvent flows through here. A
                    // pipe write failure is a strong silent-stop signal: the backend
                    // keeps running its turn, but the TUI sees nothing.
                    error!(?e, "send_session_notification failed (UpdateEvent forward)");
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
                    self.handle_end_turn_goal_or_respond(&md).await;
                }
                // Execute pending swap from switch_to_execution (agent is idle after end_current_turn)
                if let Some(agent_config) = self.pending_swap.take() {
                    let target_name = agent_config.name().to_string();
                    if let Err(e) = self
                        .agent
                        .swap_agent(agent::protocol::SwapAgentArgs {
                            agent_config,
                            force: false,
                            knowledge_provider: None,
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
                                model: self.rts_state.model_id(),
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

                // Handle goal state before releasing the response:
                // - If goal already completed (agent called goal(complete) tool before the error), suppress the
                //   error and release normally — the goal succeeded.
                // - If goal is still active (WaitingForTurn), mark it exhausted so the subsequent EndTurn (emitted
                //   by end_current_turn) doesn't spawn a ghost re-injection task after the user already received
                //   the error.
                let goal_already_completed = self
                    .goal_controller
                    .as_ref()
                    .is_some_and(|c| c.state == super::goal::GoalState::Completed);

                if let Some(ref mut ctrl) = self.goal_controller
                    && ctrl.should_continue()
                {
                    ctrl.mark_exhausted(format!("Agent error: {}", agent_error));
                    self.send_goal_status_notification();
                    self.emit_goal_telemetry("error");
                }

                if goal_already_completed {
                    // Goal already completed — the error is from the model's follow-up
                    // response after tool_result. Release gracefully instead of surfacing
                    // a confusing error to the user.
                    self.release_goal_response().await;
                } else if let Some(respond_to) = self.pending_prompt_response.take() {
                    let respond_to = respond_to.into_inner();
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
                        effort: self.current_effort(),
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
                    effort: self.current_effort(),
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
                } else if let AgentLoopEventKind::Stream(StreamResult::Ok(StreamEvent::RetryWarning(ref warning))) =
                    loop_event.kind
                {
                    let _ = self.send_ext_notification(methods::SESSION_UPDATE, ExtSessionUpdateNotification {
                        session_id: self.session_id.clone(),
                        update: ExtSessionUpdate::RetryWarning {
                            attempt: warning.attempt,
                            max_attempts: warning.max_attempts,
                            delay_secs: warning.delay_secs,
                            message: warning.message.clone(),
                        },
                    });
                }
            },
            AgentEvent::Stop(AgentStopReason::EndTurn) => {
                // Don't release if goal loop is active — EndTurn handler manages it
                if self.goal_controller.as_ref().is_some_and(|c| c.should_continue()) {
                    return;
                }
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
            AgentEvent::SteeringQueued { message } => {
                let _ = self.send_ext_notification(methods::SESSION_UPDATE, ExtSessionUpdateNotification {
                    session_id: self.session_id.clone(),
                    update: ExtSessionUpdate::SteeringQueued { message },
                });
            },
            AgentEvent::SteeringConsumed { content } => {
                let _ = self.send_ext_notification(methods::SESSION_UPDATE, ExtSessionUpdateNotification {
                    session_id: self.session_id.clone(),
                    update: ExtSessionUpdate::SteeringConsumed { content },
                });
            },
            AgentEvent::SteeringCleared => {
                let _ = self.send_ext_notification(methods::SESSION_UPDATE, ExtSessionUpdateNotification {
                    session_id: self.session_id.clone(),
                    update: ExtSessionUpdate::SteeringCleared,
                });
            },
            AgentEvent::GoalAction(action) => {
                self.handle_goal_action(action).await;
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
    client_cx: &ConnectionTo<sacp::Client>,
) -> Result<(), sacp::Error> {
    let commands: Vec<super::schema::AvailableCommand> = TuiCommand::all_commands()
        .into_iter()
        .filter(|cmd| {
            // Hide /voice from command list when rollout is not enabled
            if cmd.name() == "/voice" {
                return crate::rollout::Rollout::is_enabled(crate::rollout::Feature::Voice);
            }
            true
        })
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

    // Add skills as prompts (skills use the same prompt dispatch flow)
    if let Ok(skill_map) = agent_handle.get_skills().await {
        for (source, skill_prompts) in skill_map {
            for prompt in skill_prompts {
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

/// Attach MCP tool behavior annotations to the `_meta` map of a
/// `RequestPermissionRequest`.
///
/// ACP v1 (agent-client-protocol-schema 0.11) does not carry MCP tool
/// behavior annotations on `ToolCallUpdateFields` natively — its `Annotations`
/// type is for content-display priorities, not behavior. We surface the
/// behavior hints (`readOnlyHint`, `destructiveHint`, `idempotentHint`,
/// `openWorldHint`) via the documented `_meta` extension point so kiro-bot's
/// approval gate can read them.
///
/// TODO(acp-typed-annotations): when ACP grows a typed annotations field on
/// `ToolCallUpdateFields` (or `ToolCall`), populate that field directly and
/// delete this helper plus the matching `_meta` reader on the kiro-bot side
/// (`engine::acp::read_mcp_read_only_hint`).
fn attach_mcp_annotations(
    meta: &mut serde_json::Map<String, serde_json::Value>,
    annotations: &agent::tools::mcp::McpToolAnnotations,
) {
    meta.insert(
        "mcpAnnotations".into(),
        serde_json::to_value(annotations).unwrap_or(serde_json::Value::Null),
    );
}

async fn handle_approval_request(
    req: ApprovalRequest,
    client_cx: ConnectionTo<sacp::Client>,
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

    // Build a single _meta map and merge in any per-feature payloads. ACP v1
    // does not carry MCP tool behavior annotations or granular trust options
    // natively, so both ride via the documented `_meta` extension point.
    // When the protocol grows typed fields for either, drop the corresponding
    // helper below and switch.
    let mut meta = serde_json::Map::new();

    // MCP tool annotations (readOnlyHint / destructiveHint / idempotentHint /
    // openWorldHint) — only attach for MCP tools that actually carry hints.
    if let AgentToolKind::Mcp(mcp_tool) = &req.tool.kind
        && let Some(ann) = &mcp_tool.annotations
    {
        attach_mcp_annotations(&mut meta, ann);
    }

    // Granular trust options (path/command-level) so the TUI can offer them.
    if !req.trust_options.is_empty() {
        meta.insert(
            "trustOptions".into(),
            serde_json::to_value(&req.trust_options).unwrap_or_default(),
        );
    }

    if !meta.is_empty() {
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
        UpdateEvent::AgentThought(ContentChunk::Text(text)) => Some(SessionUpdate::AgentThoughtChunk(
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
            // ACP only models Completed/Failed for terminal tool calls. To
            // distinguish user-cancellation from a real failure we tunnel a
            // canonical reason string in the failure content; acp-client.ts
            // matches this prefix and translates it back to a `cancelled`
            // status so the chat log renders "✗ Cancelled" instead of the
            // generic FAILED chip. Mirrors the existing denied-by-user
            // detection (see acp-client.ts ~L893 and app-store.ts ~L2657).
            let (status, raw_output, content) = match result {
                ToolCallResult::Success(output) => (ToolCallStatus::Completed, serde_json::to_value(output).ok(), None),
                ToolCallResult::Error(_) => (ToolCallStatus::Failed, None, None),
                ToolCallResult::Cancelled => {
                    let cancel_text: ToolCallContent =
                        ContentBlock::Text(TextContent::new("Tool use was cancelled by the user".to_string())).into();
                    (ToolCallStatus::Failed, None, Some(vec![cancel_text]))
                },
            };

            let locations = get_tool_locations(&tool_call.tool);
            let title = get_tool_title(&tool_call.tool);

            let mut fields = ToolCallUpdateFields::new()
                .status(Some(status))
                .title(Some(title))
                .kind(Some(get_tool_kind(&tool_call.tool_use_block.name)))
                .raw_input(Some(tool_call.tool_use_block.input.clone()))
                .raw_output(raw_output)
                .locations(locations);
            if let Some(c) = content {
                fields = fields.content(Some(c));
            }
            Some(SessionUpdate::ToolCallUpdate(ToolCallUpdate::new(
                ToolCallId::new(tool_call.id),
                fields,
            )))
        },
        UpdateEvent::ToolCallFailed {
            tool_use_id,
            tool_name,
            raw_input,
            error,
            ..
        } => {
            let kind = get_tool_kind(&tool_name);
            // Surface the failure reason as a text content block so clients
            // render a descriptive error instead of a generic fallback.
            let error_content: ToolCallContent = ContentBlock::Text(TextContent::new(error.clone())).into();
            Some(SessionUpdate::ToolCallUpdate(ToolCallUpdate::new(
                ToolCallId::new(tool_use_id),
                ToolCallUpdateFields::new()
                    .status(Some(ToolCallStatus::Failed))
                    .title(Some(tool_name))
                    .kind(Some(kind))
                    .content(Some(vec![error_content]))
                    // Forward the model-generated arguments so the TUI can
                    // render them when execution was blocked.
                    .raw_input(Some(raw_input.clone())),
            )))
        },
        UpdateEvent::ToolCallUpdate {
            id,
            content: ContentChunk::Text(text),
        } => {
            let tool_content: ToolCallContent = ContentBlock::Text(TextContent::new(text)).into();
            Some(SessionUpdate::ToolCallUpdate(ToolCallUpdate::new(
                ToolCallId::new(id),
                ToolCallUpdateFields::new().content(Some(vec![tool_content])),
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
                    // Thinking blocks must map to AgentThoughtChunk (not AgentMessageChunk) —
                    // see the live-streaming counterpart at `update_event_to_session_update`
                    // for `UpdateEvent::AgentThought(...)`. Keep these two paths in sync;
                    // dropping a variant here silently discards reasoning on session resume.
                    AgentContentBlock::Thinking(thinking) => {
                        updates.push(SessionUpdate::AgentThoughtChunk(SacpContentChunk::new(
                            ContentBlock::Text(TextContent::new(thinking.text.clone())),
                        )));
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
            BuiltInToolName::ToolSearch => ToolKind::Search,
            BuiltInToolName::Task => ToolKind::Other,
            BuiltInToolName::Goal => ToolKind::Other,
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
            BuiltInTool::ToolSearch(t) => {
                let desc = t.tool_id.as_deref().or(t.query.as_deref()).unwrap_or("");
                format!("Loading tool: {}", truncate_str(desc, 40))
            },
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
            BuiltInTool::Goal(g) => {
                use agent::tools::goal::GoalTool;
                match g {
                    GoalTool::Complete { .. } => "Goal complete".to_string(),
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
                FsWrite::Insert(insert) => {
                    let old = std::fs::read_to_string(&abs_path).unwrap_or_default();
                    let mut new_content = old.clone();
                    if let Some(line) = insert.insert_line {
                        let line = line.clamp(0, new_content.lines().count() as u32);
                        let mut i = 0;
                        for l in syntect::util::LinesWithEndings::from(&new_content).take(line as usize) {
                            i += l.len();
                        }
                        i = i.min(new_content.len());
                        let mut text = insert.content.clone();
                        if !text.ends_with('\n') {
                            text.push('\n');
                        }
                        new_content.insert_str(i, &text);
                    } else {
                        if !new_content.ends_with('\n') {
                            new_content.push('\n');
                        }
                        new_content.push_str(&insert.content);
                    }
                    (Some(old), new_content)
                },
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
///
/// Priority: 1) explicit model arg, 2) user's saved default, 3) API default.
///
/// Unknown model ids (not returned by `ListAvailableModels`) are passed through to the backend.
/// If the backend rejects the id with `INVALID_MODEL_ID`, the user will see a friendly error
/// on their next message. This keeps backwards compatibility with hardcoded model ids in
/// existing integrations (e.g. models temporarily removed from `ListAvailableModels` but still
/// accepted by the backend).
async fn update_model_info(
    client: &ApiClient,
    database: &crate::database::Database,
    rts_state: &RtsState,
    model: Option<&str>,
) -> Result<(), String> {
    use crate::database::settings::Setting;

    let (models, api_default) = get_available_models(client)
        .await
        .map_err(|e| format!("Failed to fetch available models: {}", e))?;

    let model_info = if let Some(requested_model) = model {
        find_model(&models, requested_model)
            .cloned()
            .unwrap_or_else(|| synthesize_model_info(requested_model))
    } else if let Some(saved) = database.settings.get_string(Setting::ChatDefaultModel) {
        find_model(&models, &saved)
            .cloned()
            .unwrap_or_else(|| synthesize_model_info(&saved))
    } else {
        api_default
    };

    rts_state.set_model_info(Some(model_info));
    rts_state.apply_model_defaults(&database.settings);

    Ok(())
}

/// Construct a minimal `ModelInfo` for an id that wasn't returned by `ListAvailableModels`.
/// The backend is the authority on whether the id is valid — if not, it returns
/// `INVALID_MODEL_ID` which is surfaced via `ConverseStreamErrorKind::InvalidModelId`.
fn synthesize_model_info(requested_model: &str) -> crate::cli::chat::legacy::model::ModelInfo {
    warn!(
        "Model '{}' not in ListAvailableModels — passing through to backend for validation",
        requested_model
    );
    crate::cli::chat::legacy::model::ModelInfo {
        model_name: Some(requested_model.to_string()),
        description: None,
        model_id: requested_model.to_string(),
        context_window_tokens: crate::cli::chat::legacy::model::default_context_window_for_model(requested_model),
        rate_multiplier: None,
        rate_unit: None,
        additional_fields: None,
    }
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

    if let Some(e) = args.effort {
        let _ = session_manager_handle.set_next_effort(e).await;
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
    let serve_future = AgentToClient.builder()
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
                        response = response.auth_methods(vec![AuthMethod::Agent(sacp::schema::AuthMethodAgent::new("kiro-login", "Kiro Login").description(
                            format!("Run '{} login' in terminal to authenticate. See https://kiro.dev/docs/cli/authentication/", crate::constants::CLI_NAME),
                        ))]);
                    }
                    request_cx.respond(response)
                }
            },
            sacp::on_receive_request!(),
        )
        .on_receive_request(
            {
                let session_tx = session_manager_handle.clone();
                async move |request: NewSessionRequest, request_cx, cx: ConnectionTo<sacp::Client>| {
                    let session_id = SessionId::new(Uuid::new_v4().to_string());

                    // Publish session ID so all child processes inherit it
                    agent::util::consts::env_var::publish_session_id(&session_id.to_string());

                    let config = AcpSessionConfig::new(session_id.to_string(), request.cwd.clone())
                        .mcp_servers(request.mcp_servers);
                    let result = session_tx.start_session(&session_id, config, Some(cx.clone())).await?;

                    // Wait for agent initialization to complete before responding
                    let _ = result.ready_rx.await;

                    let modes = to_session_mode_state(result.current_agent_name, result.available_agents);
                    let models = to_session_model_state(result.current_model_id, result.available_models);

                    request_cx.respond(
                        NewSessionResponse::new(session_id.clone())
                            .modes(modes)
                            .models(models),
                    )?;

                    // Advertise after responding so the TUI has processed the session response
                    result.handle.advertise_commands().await;
                    result.handle.emit_initial_metadata().await;

                    // Notify TUI about agent loading issues
                    send_agent_load_notifications(&cx, &session_id, &result.requested_agent_name, &result.agent_config_errors, GovernanceNotices { mcp_enabled: result.mcp_enabled, api_failure: result.mcp_api_failure, web_tools_enabled: result.web_tools_enabled });

                    Ok(())
                }
            },
            sacp::on_receive_request!(),
        )
        .on_receive_request(
            {
                let session_tx = session_manager_handle.clone();
                async move |request: LoadSessionRequest, request_cx, cx: ConnectionTo<sacp::Client>| {
                    // Publish session ID so all child processes inherit it
                    agent::util::consts::env_var::publish_session_id(&request.session_id.to_string());

                    // Convert ACP MCP servers to agent configs
                    let config = AcpSessionConfig::new(request.session_id.to_string(), request.cwd.clone())
                        .load(true)
                        .mcp_servers(request.mcp_servers);
                    match session_tx.start_session(&request.session_id, config, Some(cx.clone())).await {
                        Ok(result) => {
                            // Wait for historical notifications to be sent before responding
                            let _ = result.ready_rx.await;

                            let modes = to_session_mode_state(result.current_agent_name, result.available_agents);
                            let models = to_session_model_state(result.current_model_id, result.available_models);

                            request_cx.respond(LoadSessionResponse::new().modes(modes).models(models))?;

                            // Advertise after responding so the TUI has processed the session response
                            result.handle.advertise_commands().await;
                            result.handle.emit_initial_metadata().await;

                            // Notify TUI about agent loading issues
                            send_agent_load_notifications(&cx, &request.session_id, &result.requested_agent_name, &result.agent_config_errors, GovernanceNotices { mcp_enabled: result.mcp_enabled, api_failure: result.mcp_api_failure, web_tools_enabled: result.web_tools_enabled });

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
        // Handle settings/set — performs a locked read-modify-write on the
        // global settings file so the TUI can safely write settings without
        // racing with the Rust backend.
        .on_receive_request(
            {
                async move |request: super::schema::SettingsSetRequest, request_cx, _cx| {
                    use crate::database::settings::Setting;
                    let key = Setting::try_from(request.key.as_str())
                        .map_err(|e| sacp::util::internal_error(format!("{e}")))?;
                    // Perform a read-modify-write directly on the global settings
                    // file (write is atomic via temp+rename), independent of the
                    // in-memory Settings snapshot (which is a clone and may be stale).
                    crate::database::settings::Settings::update_global_setting(key, request.value)
                        .await
                        .map_err(|e| sacp::util::internal_error(format!("{e}")))?;
                    request_cx.respond(super::schema::SettingsSetResponse {})
                }
            },
            sacp::on_receive_request!(),
        )
        // Handle _session/steer
        .on_receive_request(
            {
                let session_tx = session_manager_handle.clone();
                async move |request: super::schema::SessionSteerRequest, request_cx, _cx| {
                    let target = sacp::schema::SessionId::new(request.session_id);
                    let handle = session_tx.get_session_handle(&target).await?;
                    match handle.get_agent_handle().await {
                        Some(agent) => {
                            agent.steer_message(request.message).await
                                .map_err(|e| sacp::util::internal_error(e.to_string()))?;
                        }
                        None => {
                            return Err(sacp::util::internal_error("agent not available"));
                        }
                    }
                    request_cx.respond(super::schema::SessionSteerResponse { queued: true })?;
                    Ok(())
                }
            },
            sacp::on_receive_request!(),
        )
        // Handle _session/steer/clear
        .on_receive_request(
            {
                let session_tx = session_manager_handle.clone();
                async move |request: super::schema::SessionSteerClearRequest, request_cx, _cx| {
                    let target = sacp::schema::SessionId::new(request.session_id);
                    let handle = session_tx.get_session_handle(&target).await?;
                    match handle.get_agent_handle().await {
                        Some(agent) => {
                            agent.clear_steering().await
                                .map_err(|e| sacp::util::internal_error(e.to_string()))?;
                        }
                        None => {
                            return Err(sacp::util::internal_error("agent not available"));
                        }
                    }
                    request_cx.respond(super::schema::SessionSteerClearResponse { cleared: true })?;
                    Ok(())
                }
            },
            sacp::on_receive_request!(),
        )
        // Telemetry notification: mode changed in the TUI.
        .on_receive_notification(
            {
                let telemetry_thread = Some(os.telemetry.clone());
                async move |notif: super::schema::ModeChangedNotification, _cx: ConnectionTo<sacp::Client>| {
                    if let Some(ref telemetry) = telemetry_thread {
                        emit_kas_mode_changed_telemetry(telemetry, notif);
                    }
                    Ok(())
                }
            },
            sacp::on_receive_notification!(),
        )
        // Telemetry notification: UI mode resolved at session start.
        .on_receive_notification(
            {
                let telemetry_thread = Some(os.telemetry.clone());
                async move |notif: super::schema::UiModeSessionStartNotification, _cx: ConnectionTo<sacp::Client>| {
                    debug!(
                        ui_mode = %notif.ui_mode,
                        ui_mode_source = %notif.ui_mode_source,
                        ui_mode_default = %notif.ui_mode_default,
                        "uiModeSessionStart telemetry received from TUI"
                    );
                    if let Some(ref telemetry) = telemetry_thread {
                        let _ = telemetry.send_ui_mode_session_start(
                            notif.ui_mode,
                            notif.ui_mode_source,
                            notif.ui_mode_default,
                            notif.session_id,
                        );
                    }
                    Ok(())
                }
            },
            sacp::on_receive_notification!(),
        )
        // Telemetry notification: UI mode toggled mid-session.
        .on_receive_notification(
            {
                let telemetry_thread = Some(os.telemetry.clone());
                async move |notif: super::schema::UiModeChangedNotification, _cx: ConnectionTo<sacp::Client>| {
                    debug!(
                        from = %notif.from,
                        to = %notif.to,
                        source = %notif.source,
                        "uiModeChanged telemetry received from TUI"
                    );
                    if let Some(ref telemetry) = telemetry_thread {
                        let _ = telemetry.send_ui_mode_changed(
                            notif.from,
                            notif.to,
                            notif.source,
                            notif.session_id,
                        );
                    }
                    Ok(())
                }
            },
            sacp::on_receive_notification!(),
        )
        // Telemetry notification: persisted default UI mode changed.
        .on_receive_notification(
            {
                let telemetry_thread = Some(os.telemetry.clone());
                async move |notif: super::schema::UiModeDefaultChangedNotification, _cx: ConnectionTo<sacp::Client>| {
                    debug!(
                        from = %notif.from,
                        to = %notif.to,
                        "uiModeDefaultChanged telemetry received from TUI"
                    );
                    if let Some(ref telemetry) = telemetry_thread {
                        let _ = telemetry.send_ui_mode_default_changed(
                            notif.from,
                            notif.to,
                            notif.session_id,
                        );
                    }
                    Ok(())
                }
            },
            sacp::on_receive_notification!(),
        )
        .on_receive_dispatch(
            {
                let session_tx = session_manager_handle.clone();
                let telemetry_thread = Some(os.telemetry.clone());
                let database = os.database.clone();
                async move |message: Dispatch, _cx: ConnectionTo<sacp::Client>| {
                    let method = message.method().to_string();

                    // Handle session/set_model (unstable ACP method)
                    if method == "session/set_model" {
                        let Dispatch::Request(req, req_cx) = message else {
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
                        let Dispatch::Request(req, req_cx) = message else {
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
                        let Dispatch::Request(req, req_cx) = message else {
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
                    if let Dispatch::Notification(notif) = &message {
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
                            "_kiro.dev/telemetry/processHealth" => {
                                use super::schema::ProcessHealthPayload;
                                match serde_json::from_value::<ProcessHealthPayload>(notif.params().clone()) {
                                    Ok(p) => {
                                        if let Some(ref telemetry) = telemetry_thread {
                                            emit_kas_process_health_telemetry(telemetry, p);
                                        }
                                    },
                                    Err(e) => {
                                        debug!("Failed to deserialize processHealth payload: {e}");
                                    },
                                }
                                return Ok(sacp::Handled::Yes);
                            },
                            "_kiro.dev/telemetry/turnCompletion" => {
                                use super::schema::TurnCompletionTelemetryPayload;
                                match serde_json::from_value::<TurnCompletionTelemetryPayload>(notif.params().clone()) {
                                    Ok(payload) => {
                                        if let Some(ref telemetry) = telemetry_thread {
                                            emit_kas_turn_completion_telemetry(telemetry, &database, payload).await;
                                        }
                                    },
                                    Err(e) => {
                                        debug!("Failed to deserialize turnCompletion payload: {e}");
                                    },
                                }
                                return Ok(sacp::Handled::Yes);
                            },
                            "_kiro.dev/telemetry/chatSlashCommand" => {
                                use super::schema::ChatSlashCommandTelemetryPayload;
                                match serde_json::from_value::<ChatSlashCommandTelemetryPayload>(notif.params().clone())
                                {
                                    Ok(payload) => {
                                        if let Some(ref telemetry) = telemetry_thread {
                                            emit_kas_chat_slash_command_telemetry(telemetry, &database, payload).await;
                                        }
                                    },
                                    Err(e) => {
                                        debug!("Failed to deserialize chatSlashCommand payload: {e}");
                                    },
                                }
                                return Ok(sacp::Handled::Yes);
                            },
                            _ => {},
                        }
                    }
                    // Return unhandled for unknown messages
                    Ok(sacp::Handled::No { message, retry: false })
                }
            },
            sacp::on_receive_dispatch!(),
        )
        .connect_to(sacp::ByteStreams::new(
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

/// Governance toggle state forwarded to the TUI alongside agent-load notifications.
///
/// Grouped into a struct (rather than positional bools) to avoid argument
/// transposition between the several governance flags.
struct GovernanceNotices {
    /// Whether MCP is enabled by governance.
    mcp_enabled: bool,
    /// When a toggle is disabled, distinguishes admin-disabled (`false`) from the
    /// API-failure fail-closed path (`true`). Shared across toggles since they all
    /// derive from the same GetProfile call.
    api_failure: bool,
    /// Whether web tools (web_search, web_fetch) are enabled by governance.
    web_tools_enabled: bool,
}

/// Send agent loading notifications to the TUI client.
///
/// Notifies about:
/// - Agent not found (fell back to default)
/// - Agent config parse errors from startup
/// - MCP governance disabled (admin turned off MCP)
/// - Web tools governance disabled (admin turned off web tools)
fn send_agent_load_notifications(
    cx: &ConnectionTo<sacp::Client>,
    session_id: &SessionId,
    requested_agent_name: &Option<String>,
    agent_config_errors: &[super::session_manager::AgentConfigLoadError],
    governance: GovernanceNotices,
) {
    use super::extensions::{
        AgentConfigErrorNotification,
        AgentNotFoundNotification,
        McpGovernanceDisabledNotification,
        WebToolsGovernanceDisabledNotification,
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

    if !governance.mcp_enabled {
        let notif = McpGovernanceDisabledNotification {
            session_id: session_id.clone(),
            api_failure: governance.api_failure,
        };
        if let Ok(raw) = serde_json::value::to_raw_value(&notif) {
            let ext = sacp::schema::ExtNotification::new(methods::MCP_GOVERNANCE_DISABLED, std::sync::Arc::from(raw));
            let _ = cx.send_notification(sacp::schema::AgentNotification::ExtNotification(ext));
        }
    }

    if !governance.web_tools_enabled {
        // `api_failure` is shared: both toggles are resolved from the same GetProfile
        // call, so an API failure disables both together.
        let notif = WebToolsGovernanceDisabledNotification {
            session_id: session_id.clone(),
            api_failure: governance.api_failure,
        };
        if let Ok(raw) = serde_json::value::to_raw_value(&notif) {
            let ext =
                sacp::schema::ExtNotification::new(methods::WEB_TOOLS_GOVERNANCE_DISABLED, std::sync::Arc::from(raw));
            let _ = cx.send_notification(sacp::schema::AgentNotification::ExtNotification(ext));
        }
    }
}

fn tui_command_telemetry_subcommand(command: &TuiCommand) -> Option<String> {
    let value = match command {
        TuiCommand::Model(args) => args.model_name.as_deref(),
        TuiCommand::Agent(args) => args.agent_name.as_deref(),
        TuiCommand::Context(args) => args.subcommand.as_deref(),
        TuiCommand::Mcp(args) => args.subcommand.as_deref(),
        TuiCommand::Tools(args) => args.subcommand.as_deref(),
        TuiCommand::Knowledge(args) => args.subcommand.as_deref(),
        TuiCommand::Chat(args) => args.subcommand.as_deref(),
        TuiCommand::Code(args) => args.subcommand.as_deref(),
        TuiCommand::Stats(args) => args.subcommand.as_deref(),
        TuiCommand::Goal(args) => args.subcommand.as_deref(),
        _ => None,
    };
    known_subcommand(value, &command.subcommands())
}

fn known_subcommand(value: Option<&str>, allowed: &[&str]) -> Option<String> {
    let token = value?.split_whitespace().next()?.to_ascii_lowercase();
    allowed.contains(&token.as_str()).then_some(token)
}

#[cfg(test)]
mod command_usage_telemetry_tests {
    use agent::tui_commands::{
        AgentArgs,
        ChatArgs,
        ModelArgs,
        TuiCommand,
    };

    use super::tui_command_telemetry_subcommand;

    #[test]
    fn subcommand_extraction_uses_tui_command_registry() {
        let model = TuiCommand::Model(ModelArgs {
            model_name: Some("set-current-as-default".to_string()),
        });
        let chat = TuiCommand::Chat(ChatArgs {
            subcommand: Some("new hello".to_string()),
        });
        let agent = TuiCommand::Agent(AgentArgs {
            agent_name: Some("swap reviewer".to_string()),
        });
        let model_selection = TuiCommand::Model(ModelArgs {
            model_name: Some("claude-sonnet-4".to_string()),
        });

        assert_eq!(
            tui_command_telemetry_subcommand(&model).as_deref(),
            Some("set-current-as-default")
        );
        assert_eq!(tui_command_telemetry_subcommand(&chat).as_deref(), Some("new"));
        assert_eq!(tui_command_telemetry_subcommand(&agent).as_deref(), Some("swap"));
        assert_eq!(tui_command_telemetry_subcommand(&model_selection), None);
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

pub async fn emit_kas_turn_completion_telemetry(
    telemetry: &crate::telemetry::TelemetryThread,
    database: &crate::database::Database,
    payload: super::schema::TurnCompletionTelemetryPayload,
) {
    for mut event in kas_turn_completion_events(payload) {
        crate::telemetry::set_event_metadata(database, &mut event).await;
        if let Err(err) = telemetry.send_event(event) {
            debug!("Failed to send KAS turn completion telemetry: {err}");
        }
    }
}

pub fn emit_kas_mode_changed_telemetry(
    telemetry: &crate::telemetry::TelemetryThread,
    payload: super::schema::ModeChangedNotification,
) {
    if let Err(err) = telemetry.send_event(kas_mode_changed_event(payload)) {
        debug!("Failed to send KAS mode change telemetry: {err}");
    }
}

pub async fn emit_kas_chat_slash_command_telemetry(
    telemetry: &crate::telemetry::TelemetryThread,
    database: &crate::database::Database,
    payload: super::schema::ChatSlashCommandTelemetryPayload,
) {
    let mut event = kas_chat_slash_command_event(payload);
    crate::telemetry::set_event_metadata(database, &mut event).await;
    if let Err(err) = telemetry.send_event(event) {
        debug!("Failed to send KAS slash command telemetry: {err}");
    }
}

pub async fn emit_kas_chat_session_started_telemetry(
    telemetry: &crate::telemetry::TelemetryThread,
    database: &crate::database::Database,
    payload: super::schema::ChatSessionStartedTelemetryPayload,
) {
    let mut event = kas_chat_session_started_event(payload);
    crate::telemetry::set_event_metadata(database, &mut event).await;
    if let Err(err) = telemetry.send_event(event) {
        debug!("Failed to send KAS chat session telemetry: {err}");
    }
}

pub fn emit_kas_process_health_telemetry(
    telemetry: &crate::telemetry::TelemetryThread,
    payload: super::schema::ProcessHealthPayload,
) {
    if let Err(err) = telemetry.send_event(kas_process_health_event(payload)) {
        debug!("Failed to send KAS process health telemetry: {err}");
    }
}

pub fn kas_mode_changed_event(payload: super::schema::ModeChangedNotification) -> Event {
    kas_telemetry_event(EventType::ModeChanged {
        from_mode: payload.from_mode,
        to_mode: payload.to_mode,
        source: payload.source,
        session_id: payload.session_id,
    })
}

pub fn kas_chat_slash_command_event(payload: super::schema::ChatSlashCommandTelemetryPayload) -> Event {
    kas_telemetry_event(EventType::ChatSlashCommandExecuted {
        conversation_id: payload.session_id.unwrap_or_default(),
        command: payload.command,
        subcommand: payload.subcommand,
        result: if payload.success {
            TelemetryResult::Succeeded
        } else {
            TelemetryResult::Failed
        },
        reason: payload.reason,
    })
}

pub fn kas_chat_session_started_event(payload: super::schema::ChatSessionStartedTelemetryPayload) -> Event {
    kas_telemetry_event(EventType::ChatSessionStarted {
        mode: kiro_telemetry::metric::Mode::from_chat_session(None, payload.mode.as_deref()),
    })
}

pub fn kas_process_health_event(payload: super::schema::ProcessHealthPayload) -> Event {
    kas_telemetry_event(EventType::ProcessHealthMetric {
        agent_kind: payload.agent_kind.as_deref().map_or(
            kiro_telemetry::metric::AgentKind::Kas,
            kiro_telemetry::metric::AgentKind::from_name,
        ),
        rss_mb: payload.rss_mb.unwrap_or(0.0),
        heap_used_mb: payload.heap_used_mb.unwrap_or(0.0),
        peak_rss_mb: payload.peak_rss_mb.unwrap_or(0.0),
        cpu_user_pct: payload.cpu_user_pct.unwrap_or(0.0),
        cpu_system_pct: payload.cpu_system_pct.unwrap_or(0.0),
        last_render_ms: payload.last_render_ms.unwrap_or(0.0),
        max_render_ms: payload.max_render_ms.unwrap_or(0.0),
        renders_per_min: payload.renders_per_min.unwrap_or(0),
        full_redraws_per_min: payload.full_redraws_per_min.unwrap_or(0),
        yoga_node_count: payload.yoga_node_count.unwrap_or(0),
        event_loop_p99_ms: payload.event_loop_p99_ms,
        input_latency_p95_ms: payload.input_latency_p95_ms,
        session_duration_sec: payload.session_duration_sec.unwrap_or(0),
        cpu_cores: payload.cpu_cores.unwrap_or(0),
        total_memory_mb: payload.total_memory_mb.unwrap_or(0),
        terminal: payload.terminal.unwrap_or_default(),
        session_id: payload.session_id,
        version: payload.version,
        platform: payload.platform,
    })
}

pub fn kas_turn_completion_events(payload: super::schema::TurnCompletionTelemetryPayload) -> Vec<Event> {
    let model = payload.model_id.clone();
    let conversation_id = payload.session_id.clone().unwrap_or_default();
    let has_token_usage = kas_has_token_usage(&payload);
    let has_model_invocation = !payload.metering_usage.is_empty()
        || payload.turn_duration_ms.is_some()
        || payload.status.is_some()
        || has_token_usage;
    let mut events = Vec::new();

    for usage in payload.metering_usage.iter().filter(|usage| usage.value.is_finite()) {
        events.push(kas_telemetry_event(EventType::MeteringEvent {
            request_id: None,
            model: model.clone(),
            usage: usage.value,
            unit: usage.unit.clone(),
            unit_plural: usage.unit_plural.clone(),
        }));
    }

    if let Some(percentage) = payload.context_usage_percentage
        && percentage.is_finite()
        && percentage >= 0.0
    {
        events.push(kas_telemetry_event(EventType::ContextUsagePercentage {
            model: model.clone(),
            percentage,
        }));
    }

    events.extend(kas_tool_invocation_events(
        &conversation_id,
        &model,
        &payload.used_tools,
    ));

    let should_emit_turn_completion = !payload.metering_usage.is_empty()
        || payload.turn_duration_ms.is_some()
        || payload.status.is_some()
        || has_token_usage;
    if should_emit_turn_completion {
        let result = kas_turn_result(payload.status);
        let (reason, reason_desc) = kas_turn_failure_reason(payload.status);
        let token_usage = kas_token_usage(&payload);
        events.push(kas_telemetry_event(EventType::RecordUserTurnCompletion {
            conversation_id,
            result,
            args: RecordUserTurnCompletionArgs {
                model: model.clone(),
                reason,
                reason_desc,
                total_tokens: kas_total_tokens(&payload),
                uncached_input_tokens: positive_token_count(payload.uncached_input_tokens),
                output_tokens: positive_token_count(payload.output_tokens),
                cache_read_input_tokens: positive_token_count(payload.cache_read_input_tokens),
                cache_write_input_tokens: positive_token_count(payload.cache_write_input_tokens),
                estimated_cost_usd: kas_estimated_cost_usd(&model, token_usage),
                user_turn_duration_seconds: kas_turn_duration_seconds(payload.turn_duration_ms),
                emit_user_turn_counter: true,
                ..Default::default()
            },
        }));
    }

    if has_model_invocation {
        events.push(kas_telemetry_event(EventType::ModelInvocation { model }));
    }

    events
}

#[derive(Debug, PartialEq, Eq)]
struct KasToolTelemetry {
    tool_name: String,
    mcp_server_name: Option<String>,
    is_custom_tool: bool,
}

fn kas_tool_invocation_events(conversation_id: &str, model: &Option<String>, used_tools: &[String]) -> Vec<Event> {
    let mut seen = HashSet::new();
    let mut events = Vec::new();

    for raw_tool_name in used_tools {
        let Some(tool) = kas_tool_telemetry(raw_tool_name) else {
            continue;
        };
        let dedupe_key = format!(
            "{}\0{}",
            tool.mcp_server_name.as_deref().unwrap_or_default(),
            tool.tool_name
        );
        if !seen.insert(dedupe_key) {
            continue;
        }

        events.push(kas_telemetry_event(EventType::ToolUseSuggested {
            conversation_id: conversation_id.to_string(),
            utterance_id: None,
            user_input_id: None,
            tool_use_id: None,
            tool_name: Some(tool.tool_name),
            mcp_server_name: tool.mcp_server_name,
            is_accepted: true,
            is_trusted: true,
            is_success: Some(true),
            reason_desc: None,
            is_valid: Some(true),
            is_custom_tool: tool.is_custom_tool,
            input_token_size: None,
            output_token_size: None,
            custom_tool_call_latency: None,
            model: model.clone(),
            execution_duration: None,
            turn_duration: None,
            aws_service_name: None,
            aws_operation_name: None,
        }));
    }

    events
}

fn kas_tool_telemetry(raw_tool_name: &str) -> Option<KasToolTelemetry> {
    let tool_name = raw_tool_name.trim();
    if tool_name.is_empty() {
        return None;
    }

    if let Some(rest) = tool_name.strip_prefix('@')
        && let Some((server_name, mcp_tool_name)) = rest.split_once('/')
    {
        let server_name = server_name.trim();
        let mcp_tool_name = mcp_tool_name.trim();
        if !server_name.is_empty() && !mcp_tool_name.is_empty() {
            return Some(KasToolTelemetry {
                tool_name: mcp_tool_name.to_string(),
                mcp_server_name: Some(server_name.to_string()),
                is_custom_tool: true,
            });
        }
    }

    Some(KasToolTelemetry {
        tool_name: tool_name.to_string(),
        mcp_server_name: None,
        is_custom_tool: BuiltInToolName::from_str(tool_name).is_err(),
    })
}

fn kas_telemetry_event(ty: EventType) -> Event {
    let mut event = Event::new(ty);
    event.set_client_application_kind(kiro_telemetry::metric::ClientApplication::ChatCliV3);
    event.app_type = Some("KAS".to_string());
    event
}

fn kas_turn_result(status: Option<super::schema::TurnCompletionStatus>) -> TelemetryResult {
    match status {
        Some(super::schema::TurnCompletionStatus::Failed | super::schema::TurnCompletionStatus::Other) => {
            TelemetryResult::Failed
        },
        Some(super::schema::TurnCompletionStatus::Cancelled) => TelemetryResult::Cancelled,
        Some(super::schema::TurnCompletionStatus::Success) | None => TelemetryResult::Succeeded,
    }
}

fn kas_turn_failure_reason(status: Option<super::schema::TurnCompletionStatus>) -> (Option<String>, Option<String>) {
    match status {
        Some(super::schema::TurnCompletionStatus::Failed) => (
            Some("KasTurnFailed".to_string()),
            Some("KAS reported turn failure".to_string()),
        ),
        Some(super::schema::TurnCompletionStatus::Other) => (
            Some("KasTurnUnknownStatus".to_string()),
            Some("KAS reported an unknown turn status".to_string()),
        ),
        _ => (None, None),
    }
}

fn kas_turn_duration_seconds(turn_duration_ms: Option<f64>) -> i64 {
    let Some(milliseconds) = turn_duration_ms else {
        return 0;
    };
    if !milliseconds.is_finite() || milliseconds <= 0.0 {
        return 0;
    }
    let seconds = (milliseconds / 1000.0).floor().min(i64::MAX as f64) as i64;
    if seconds == 0 { 1 } else { seconds }
}

fn kas_has_token_usage(payload: &super::schema::TurnCompletionTelemetryPayload) -> bool {
    kas_total_tokens(payload).is_some()
        || positive_token_count(payload.uncached_input_tokens).is_some()
        || positive_token_count(payload.output_tokens).is_some()
        || positive_token_count(payload.cache_read_input_tokens).is_some()
        || positive_token_count(payload.cache_write_input_tokens).is_some()
}

fn kas_total_tokens(payload: &super::schema::TurnCompletionTelemetryPayload) -> Option<i64> {
    positive_token_count(payload.total_tokens).or_else(|| {
        let total = positive_token_count(payload.uncached_input_tokens).unwrap_or_default()
            + positive_token_count(payload.output_tokens).unwrap_or_default()
            + positive_token_count(payload.cache_read_input_tokens).unwrap_or_default();
        (total > 0).then_some(total)
    })
}

fn positive_token_count(value: Option<i64>) -> Option<i64> {
    value.filter(|value| *value > 0)
}

fn kas_token_usage(payload: &super::schema::TurnCompletionTelemetryPayload) -> kiro_telemetry::TokenUsage {
    kiro_telemetry::TokenUsage {
        uncached_input_tokens: positive_token_count(payload.uncached_input_tokens).unwrap_or_default() as u64,
        cache_read_input_tokens: positive_token_count(payload.cache_read_input_tokens).unwrap_or_default() as u64,
        cache_write_input_tokens: positive_token_count(payload.cache_write_input_tokens).unwrap_or_default() as u64,
        output_tokens: positive_token_count(payload.output_tokens).unwrap_or_default() as u64,
    }
}

fn kas_estimated_cost_usd(model: &Option<String>, usage: kiro_telemetry::TokenUsage) -> Option<f64> {
    if usage.uncached_input_tokens == 0
        && usage.cache_read_input_tokens == 0
        && usage.cache_write_input_tokens == 0
        && usage.output_tokens == 0
    {
        return None;
    }
    estimated_cost_usd(model, usage)
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
mod kas_turn_completion_telemetry_tests {
    use kiro_telemetry::testing::{
        expect_log,
        expect_metric,
        log_attr,
    };
    use kiro_telemetry::{
        TokenUsage,
        log as telemetry_log,
        metric,
    };

    use super::kas_turn_completion_events;
    use crate::agent::acp::schema::{
        ChatSessionStartedTelemetryPayload,
        ChatSlashCommandTelemetryPayload,
        ModeChangeSource,
        ModeChangedNotification,
        ProcessHealthPayload,
        TurnCompletionMeteringUsage,
        TurnCompletionStatus,
        TurnCompletionTelemetryPayload,
    };
    use crate::telemetry::core::{
        Event,
        EventLegacyExt,
    };
    use crate::telemetry::{
        EventType,
        TelemetryResult,
    };

    fn assert_kas_attribution(event: &Event) {
        assert_eq!(event.client_application.as_deref(), Some("chat_cli_v3"));
        assert_eq!(event.app_type.as_deref(), Some("KAS"));
    }

    #[test]
    fn kas_bridge_events_use_v3_attribution() {
        let mode = super::kas_mode_changed_event(ModeChangedNotification {
            from_mode: "kiro".to_string(),
            to_mode: "kiro_planner".to_string(),
            source: ModeChangeSource::ShiftTab,
            session_id: Some("kas-session-1".to_string()),
        });
        let slash = super::kas_chat_slash_command_event(ChatSlashCommandTelemetryPayload {
            session_id: Some("kas-session-1".to_string()),
            command: "/chat".to_string(),
            subcommand: Some("save".to_string()),
            success: true,
            reason: None,
        });
        let session = super::kas_chat_session_started_event(ChatSessionStartedTelemetryPayload {
            session_id: Some("kas-session-1".to_string()),
            mode: Some("kiro_planner".to_string()),
        });
        let process = super::kas_process_health_event(ProcessHealthPayload {
            agent_kind: Some("kas".to_string()),
            rss_mb: Some(128.0),
            heap_used_mb: None,
            peak_rss_mb: None,
            cpu_user_pct: Some(12.5),
            cpu_system_pct: Some(7.5),
            last_render_ms: None,
            max_render_ms: None,
            renders_per_min: None,
            full_redraws_per_min: None,
            yoga_node_count: None,
            event_loop_p99_ms: None,
            input_latency_p95_ms: None,
            session_duration_sec: None,
            cpu_cores: None,
            total_memory_mb: None,
            terminal: None,
            session_id: Some("kas-session-1".to_string()),
            version: "2.4.0".to_string(),
            platform: "darwin".to_string(),
        });

        for event in [&mode, &slash, &session, &process] {
            assert_kas_attribution(event);
        }

        let session_start = session.otel_metric_record().expect("chat session start metric");
        expect_metric(
            std::slice::from_ref(&session_start),
            metric::chat_session_started(metric::Mode::Plan, metric::ClientApplication::ChatCliV3),
        );

        let process_records = process.otel_metric_records();
        expect_metric(
            &process_records,
            metric::process_memory_rss(
                128.0 * 1024.0 * 1024.0,
                metric::VersionMinorBucket::Current,
                metric::AgentKind::Kas,
            ),
        );
    }

    #[test]
    fn kas_process_health_defaults_missing_agent_kind_to_kas() {
        let process = super::kas_process_health_event(ProcessHealthPayload {
            agent_kind: None,
            rss_mb: Some(128.0),
            heap_used_mb: None,
            peak_rss_mb: None,
            cpu_user_pct: Some(12.5),
            cpu_system_pct: Some(7.5),
            last_render_ms: None,
            max_render_ms: None,
            renders_per_min: None,
            full_redraws_per_min: None,
            yoga_node_count: None,
            event_loop_p99_ms: None,
            input_latency_p95_ms: None,
            session_duration_sec: None,
            cpu_cores: None,
            total_memory_mb: None,
            terminal: None,
            session_id: Some("kas-session-1".to_string()),
            version: "2.4.0".to_string(),
            platform: "darwin".to_string(),
        });

        match process.ty {
            EventType::ProcessHealthMetric { agent_kind, .. } => {
                assert_eq!(agent_kind, metric::AgentKind::Kas);
            },
            _ => panic!("expected process health event"),
        }
    }

    #[test]
    fn kas_turn_completion_emits_metering_context_and_turn_events() {
        let events = kas_turn_completion_events(TurnCompletionTelemetryPayload {
            session_id: Some("kas-session-1".to_string()),
            model_id: Some("claude-4-sonnet".to_string()),
            metering_usage: vec![TurnCompletionMeteringUsage {
                value: 1.5,
                unit: "credit".to_string(),
                unit_plural: "Credits".to_string(),
            }],
            turn_duration_ms: Some(1234.0),
            context_usage_percentage: Some(42.0),
            total_tokens: None,
            uncached_input_tokens: Some(10),
            output_tokens: Some(5),
            cache_read_input_tokens: Some(2),
            cache_write_input_tokens: Some(3),
            status: Some(TurnCompletionStatus::Success),
            used_tools: Vec::new(),
        });

        assert_eq!(events.len(), 4);
        assert!(
            events
                .iter()
                .all(|event| event.client_application.as_deref() == Some("chat_cli_v3"))
        );
        assert!(events.iter().all(|event| event.app_type.as_deref() == Some("KAS")));

        let metering = events[0].otel_log_record().expect("metering log");
        expect_log(
            std::slice::from_ref(&metering),
            telemetry_log::metering_event_record(telemetry_log::MeteringEventLog::from_names(
                None,
                Some("claude-4-sonnet"),
                Some("chat_cli_v3"),
                1.5,
                "credit",
                "Credits",
            )),
        );

        let context_usage = events[1].otel_metric_record().expect("context usage metric");
        expect_metric(
            std::slice::from_ref(&context_usage),
            metric::context_usage_percentage(
                42.0,
                metric::ModelClass::AnthropicSonnet,
                metric::ClientApplication::ChatCliV3,
                false,
            ),
        );

        match &events[2].ty {
            EventType::RecordUserTurnCompletion {
                conversation_id,
                result,
                args,
            } => {
                assert_eq!(conversation_id, "kas-session-1");
                assert_eq!(*result, TelemetryResult::Succeeded);
                assert_eq!(args.model.as_deref(), Some("claude-4-sonnet"));
                assert_eq!(args.total_tokens, Some(17));
                assert_eq!(args.uncached_input_tokens, Some(10));
                assert_eq!(args.output_tokens, Some(5));
                assert_eq!(args.cache_read_input_tokens, Some(2));
                assert_eq!(args.cache_write_input_tokens, Some(3));
                assert!(
                    args.estimated_cost_usd
                        .is_some_and(|cost| (cost - 0.00010785).abs() < 0.000000001)
                );
                assert_eq!(args.user_turn_duration_seconds, 1);
                assert!(args.emit_user_turn_counter);
            },
            other => panic!("expected RecordUserTurnCompletion, got {other:?}"),
        }
        let turn_log = events[2].otel_log_record().expect("turn completion log");
        assert_eq!(log_attr(&turn_log, "total_tokens"), Some("17"));
        assert_eq!(log_attr(&turn_log, "uncached_input_tokens"), Some("10"));
        assert_eq!(log_attr(&turn_log, "output_tokens"), Some("5"));
        assert_eq!(log_attr(&turn_log, "cache_read_input_tokens"), Some("2"));
        assert_eq!(log_attr(&turn_log, "cache_write_input_tokens"), Some("3"));
        assert_eq!(log_attr(&turn_log, "estimated_cost_usd"), Some("0.000107850"));
        let turn_records = events[2].otel_metric_records();
        let invocation = metric::InvocationContext::new(
            metric::ModelClass::AnthropicSonnet,
            metric::ClientApplication::ChatCliV3,
            false,
        );
        expect_metric(
            &turn_records,
            metric::user_turns_for_invocation(invocation, metric::ResultKind::Success, metric::Mode::Interactive),
        );
        expect_metric(
            &turn_records,
            metric::user_turn_duration_seconds_for_invocation(
                1.0,
                invocation,
                metric::ChatConversationKind::Interactive,
                metric::Mode::Interactive,
            ),
        );
        expect_metric(
            &turn_records,
            metric::tokens_consumed(
                10,
                metric::ModelClass::AnthropicSonnet,
                metric::TokenType::InputUncached,
                metric::ClientApplication::ChatCliV3,
                false,
            ),
        );
        expect_metric(
            &turn_records,
            metric::estimated_cost_usd(
                invocation
                    .estimated_cost_usd(TokenUsage {
                        uncached_input_tokens: 10,
                        cache_read_input_tokens: 2,
                        cache_write_input_tokens: 3,
                        output_tokens: 5,
                    })
                    .expect("sonnet pricing is known"),
                metric::ModelClass::AnthropicSonnet,
                metric::ClientApplication::ChatCliV3,
                false,
            ),
        );

        let invocation = events[3].otel_metric_record().expect("model invocation metric");
        expect_metric(
            std::slice::from_ref(&invocation),
            metric::model_invocation(metric::ModelClass::AnthropicSonnet),
        );
    }

    #[test]
    fn kas_context_only_payload_does_not_emit_empty_turn_completion() {
        let events = kas_turn_completion_events(TurnCompletionTelemetryPayload {
            session_id: Some("kas-session-1".to_string()),
            model_id: Some("claude-4-sonnet".to_string()),
            metering_usage: Vec::new(),
            turn_duration_ms: None,
            context_usage_percentage: Some(66.0),
            total_tokens: None,
            uncached_input_tokens: None,
            output_tokens: None,
            cache_read_input_tokens: None,
            cache_write_input_tokens: None,
            status: None,
            used_tools: Vec::new(),
        });

        assert_eq!(events.len(), 1);
        assert!(matches!(events[0].ty, EventType::ContextUsagePercentage { .. }));
    }

    #[test]
    fn kas_unknown_turn_status_maps_to_failed_fact_row() {
        let events = kas_turn_completion_events(TurnCompletionTelemetryPayload {
            session_id: Some("kas-session-1".to_string()),
            model_id: Some("claude-4-sonnet".to_string()),
            metering_usage: Vec::new(),
            turn_duration_ms: Some(50.0),
            context_usage_percentage: None,
            total_tokens: None,
            uncached_input_tokens: None,
            output_tokens: None,
            cache_read_input_tokens: None,
            cache_write_input_tokens: None,
            status: Some(TurnCompletionStatus::Other),
            used_tools: Vec::new(),
        });

        assert_eq!(events.len(), 2);
        match &events[0].ty {
            EventType::RecordUserTurnCompletion { result, args, .. } => {
                assert_eq!(*result, TelemetryResult::Failed);
                assert_eq!(args.reason.as_deref(), Some("KasTurnUnknownStatus"));
                assert_eq!(args.reason_desc.as_deref(), Some("KAS reported an unknown turn status"));
                assert_eq!(args.user_turn_duration_seconds, 1);
            },
            other => panic!("expected RecordUserTurnCompletion, got {other:?}"),
        }
        assert!(matches!(events[1].ty, EventType::ModelInvocation { .. }));
    }

    #[test]
    fn kas_turn_completion_emits_tool_usage_metrics_and_facts() {
        let events = kas_turn_completion_events(TurnCompletionTelemetryPayload {
            session_id: Some("kas-session-1".to_string()),
            model_id: Some("claude-4-sonnet".to_string()),
            metering_usage: Vec::new(),
            turn_duration_ms: None,
            context_usage_percentage: None,
            total_tokens: None,
            uncached_input_tokens: None,
            output_tokens: None,
            cache_read_input_tokens: None,
            cache_write_input_tokens: None,
            status: None,
            used_tools: vec![
                "fs_read".to_string(),
                "@local/echo".to_string(),
                "custom_tool".to_string(),
                "fs_read".to_string(),
                " ".to_string(),
            ],
        });

        assert_eq!(events.len(), 3);
        assert!(
            events
                .iter()
                .all(|event| event.client_application.as_deref() == Some("chat_cli_v3"))
        );

        let builtin_records = events[0].otel_metric_records();
        expect_metric(
            &builtin_records,
            metric::tool_call_total(metric::ToolOrigin::Builtin, Some("fs_read"), metric::Outcome::Success),
        );
        expect_metric(
            &builtin_records,
            metric::tool_invocations(metric::ToolOrigin::Builtin, metric::Outcome::Success),
        );
        let builtin_log = events[0].otel_log_record().expect("builtin tool log");
        assert_eq!(log_attr(&builtin_log, "tool_name"), Some("fs_read"));
        assert_eq!(log_attr(&builtin_log, "model_class"), Some("anthropic_sonnet"));

        let mcp_records = events[1].otel_metric_records();
        expect_metric(
            &mcp_records,
            metric::tool_call_total(metric::ToolOrigin::Mcp, None, metric::Outcome::Success),
        );
        let mcp_log = events[1].otel_log_record().expect("mcp tool log");
        assert_eq!(log_attr(&mcp_log, "tool_name"), Some("echo"));
        assert_eq!(log_attr(&mcp_log, "mcp_server_name"), Some("local"));

        let custom_log = events[2].otel_log_record().expect("custom tool log");
        assert_eq!(log_attr(&custom_log, "tool_name"), Some("custom_tool"));
        assert_eq!(log_attr(&custom_log, "mcp_server_name"), None);
    }

    #[test]
    fn kas_token_only_payload_emits_turn_fact_and_metrics() {
        let events = kas_turn_completion_events(TurnCompletionTelemetryPayload {
            session_id: Some("kas-session-1".to_string()),
            model_id: Some("claude-4-sonnet".to_string()),
            metering_usage: Vec::new(),
            turn_duration_ms: None,
            context_usage_percentage: None,
            total_tokens: None,
            uncached_input_tokens: Some(10),
            output_tokens: Some(5),
            cache_read_input_tokens: Some(2),
            cache_write_input_tokens: Some(3),
            status: None,
            used_tools: Vec::new(),
        });

        assert_eq!(events.len(), 2);
        match &events[0].ty {
            EventType::RecordUserTurnCompletion { result, args, .. } => {
                assert_eq!(*result, TelemetryResult::Succeeded);
                assert_eq!(args.total_tokens, Some(17));
                assert!(args.estimated_cost_usd.is_some());
            },
            other => panic!("expected RecordUserTurnCompletion, got {other:?}"),
        }

        let records = events[0].otel_metric_records();
        expect_metric(
            &records,
            metric::tokens_consumed(
                5,
                metric::ModelClass::AnthropicSonnet,
                metric::TokenType::Output,
                metric::ClientApplication::ChatCliV3,
                false,
            ),
        );
        expect_metric(
            &records,
            metric::estimated_cost_usd(
                metric::InvocationContext::new(
                    metric::ModelClass::AnthropicSonnet,
                    metric::ClientApplication::ChatCliV3,
                    false,
                )
                .estimated_cost_usd(TokenUsage {
                    uncached_input_tokens: 10,
                    cache_read_input_tokens: 2,
                    cache_write_input_tokens: 3,
                    output_tokens: 5,
                })
                .expect("sonnet pricing is known"),
                metric::ModelClass::AnthropicSonnet,
                metric::ClientApplication::ChatCliV3,
                false,
            ),
        );
        assert!(matches!(events[1].ty, EventType::ModelInvocation { .. }));
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
    fn test_insert_produces_diff_content() {
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

        assert_eq!(result.len(), 1, "Insert should produce one ToolCallContent::Diff");
        match &result[0] {
            ToolCallContent::Diff(diff) => {
                assert_eq!(diff.old_text, Some(String::new()));
                assert!(diff.new_text.contains("inserted line"));
            },
            _ => panic!("expected Diff"),
        }
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

#[cfg(test)]
mod convert_update_event_tests {
    use agent::protocol::{
        ContentChunk,
        UpdateEvent,
    };
    use sacp::schema::SessionUpdate;

    use super::convert_update_event_to_session_update;

    #[test]
    fn test_tool_call_update_text_maps_to_session_update() {
        let event = UpdateEvent::ToolCallUpdate {
            id: "test-id".to_string(),
            content: ContentChunk::Text("hello".to_string()),
        };

        let result = convert_update_event_to_session_update(event);
        assert!(
            result.is_some(),
            "ToolCallUpdate with Text content should produce a SessionUpdate"
        );

        let update = result.unwrap();

        // Verify it's a ToolCallUpdate variant with correct tool_call_id and content
        let json = serde_json::to_value(&update).expect("SessionUpdate should serialize");
        let json_str = serde_json::to_string_pretty(&json).unwrap();

        // The serialized form must contain the tool_call_id
        assert!(
            json_str.contains("test-id"),
            "serialized update should contain tool_call_id 'test-id', got: {json_str}"
        );

        // The serialized form must contain the text content
        assert!(
            json_str.contains("hello"),
            "serialized update should contain text 'hello', got: {json_str}"
        );

        // Verify via pattern matching that it's the right variant with correct fields
        match update {
            SessionUpdate::ToolCallUpdate(tool_call_update) => {
                let re_json = serde_json::to_value(&tool_call_update).expect("ToolCallUpdate should serialize");
                let pretty = serde_json::to_string_pretty(&re_json).unwrap();
                assert_eq!(
                    re_json["toolCallId"], "test-id",
                    "tool_call_id should be 'test-id', got: {pretty}"
                );

                // Verify the text "hello" appears in the serialized content
                assert!(
                    pretty.contains("hello"),
                    "serialized ToolCallUpdate should contain 'hello', got: {pretty}"
                );
            },
            other => panic!("expected ToolCallUpdate, got {:?}", other),
        }
    }

    #[test]
    fn test_tool_call_update_non_text_returns_none() {
        let event = UpdateEvent::ToolCallUpdate {
            id: "test-id".to_string(),
            content: ContentChunk::ResourceLink("some-link".to_string()),
        };

        let result = convert_update_event_to_session_update(event);
        assert!(
            result.is_none(),
            "ToolCallUpdate with non-Text content should return None"
        );
    }

    #[test]
    fn test_agent_content_text_maps_to_agent_message_chunk() {
        let event = UpdateEvent::AgentContent(ContentChunk::Text("agent says hi".to_string()));

        let result = convert_update_event_to_session_update(event);
        assert!(
            result.is_some(),
            "AgentContent with Text should produce a SessionUpdate"
        );

        match result.unwrap() {
            SessionUpdate::AgentMessageChunk(_) => {},
            other => panic!("expected AgentMessageChunk, got {:?}", other),
        }
    }

    #[test]
    fn test_agent_thought_text_maps_to_agent_thought_chunk() {
        let event = UpdateEvent::AgentThought(ContentChunk::Text("thinking about this...".to_string()));

        let result = convert_update_event_to_session_update(event);
        assert!(
            result.is_some(),
            "AgentThought with Text should produce a SessionUpdate"
        );

        match result.unwrap() {
            SessionUpdate::AgentThoughtChunk(_) => {},
            other => panic!("expected AgentThoughtChunk, got {:?}", other),
        }
    }

    #[test]
    fn test_agent_thought_non_text_returns_none() {
        let event = UpdateEvent::AgentThought(ContentChunk::ResourceLink("some-link".to_string()));

        let result = convert_update_event_to_session_update(event);
        assert!(
            result.is_none(),
            "AgentThought with non-Text content should return None"
        );
    }
}

#[cfg(test)]
mod log_entry_to_session_updates_tests {
    use agent::agent_loop::types::{
        ContentBlock as AgentContentBlock,
        ThinkingBlock,
    };
    use agent::event_log::{
        LogEntry,
        LogEntryV1,
    };
    use sacp::schema::SessionUpdate;

    use super::log_entry_to_session_updates;

    /// Regression: thinking blocks were being dropped on session resume
    /// because the replay path only converted Text content. The TUI's
    /// `ThinkingDisplay` only renders when it sees an `AgentThoughtChunk`,
    /// so resumed sessions appeared to have no reasoning history.
    ///
    /// This must stay aligned with the live counterpart:
    /// `convert_update_event_to_session_update` for `UpdateEvent::AgentThought`.
    #[test]
    fn assistant_message_thinking_block_emits_agent_thought_chunk() {
        let entry = LogEntry::V1(LogEntryV1::AssistantMessage {
            message_id: "m1".to_string(),
            content: vec![
                AgentContentBlock::Thinking(ThinkingBlock {
                    text: "Let me reason about this.".to_string(),
                    signature: None,
                    redacted_content: Vec::new(),
                    model_id: None,
                }),
                AgentContentBlock::Text("Here is my answer.".to_string()),
            ],
        });

        let updates = log_entry_to_session_updates(&entry);

        assert_eq!(updates.len(), 2, "expected one thought chunk and one message chunk");
        assert!(
            matches!(updates[0], SessionUpdate::AgentThoughtChunk(_)),
            "expected first update to be AgentThoughtChunk, got {:?}",
            updates[0]
        );
        assert!(
            matches!(updates[1], SessionUpdate::AgentMessageChunk(_)),
            "expected second update to be AgentMessageChunk, got {:?}",
            updates[1]
        );
    }
}
