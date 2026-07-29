pub mod agent_config;
pub mod agent_loop;
pub mod compact;
pub mod consts;
pub mod event_log;
pub mod goal;
pub mod mcp;
pub mod permissions;
pub mod prompts;
pub mod protocol;
mod resource_budget;
pub mod shell_permission;
pub mod task_executor;
pub mod tool_index;
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

use agent_config::definitions::{
    HookConfig,
    HookTrigger,
};
use agent_config::parse::{
    CanonicalToolName,
    ResourceKind,
    ToolNameKind,
};
use agent_config::{
    LoadedAgentConfig,
    LoadedMcpServerConfigs,
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
    MessageMetadata,
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
pub use consts::{
    CONTEXT_ENTRY_END_HEADER,
    CONTEXT_ENTRY_START_HEADER,
    DEFERRED_TOOLS_MESSAGE,
    RESPONSE_INTERRUPTED_MESSAGE,
    SKILL_FILES_MESSAGE,
    TOOL_USES_INTERRUPTED_MESSAGE,
};
use event_log::{
    LogEntry,
    ToolResult as LogToolResult,
};
use futures::stream::FuturesUnordered;
use mcp::McpServerEvent;
use mcp::types::Prompt;
use permissions::{
    PathAccessType,
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
    ToolCallFailureReason,
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
use tool_index::{
    ToolIndex,
    ToolLoadConfig,
    filter_specs_by_allowed_tools,
    filter_tool_names,
    should_activate_tool_search,
};
use tool_utils::{
    SanitizedToolSpecs,
    add_tool_use_purpose_arg,
    sanitize_tool_specs,
};
use tools::task::store::TaskStore;
use tools::{
    Tool,
    ToolCallIdentity,
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
    DUMMY_TOOL_RESULT_MESSAGE,
    MAX_CONSECUTIVE_UNEXECUTABLE_TOOL_TURNS,
    REPEATED_UNEXECUTABLE_TOOL_MESSAGE,
};
use crate::agent::mcp::{
    McpManager,
    McpManagerHandle,
};
use crate::agent::protocol::{
    ClearEvent,
    CompactionEvent,
};
use crate::agent::tools::summary::Summary;
use crate::agent::tools::{
    BuiltInTool,
    ToolKind,
    ToolState,
};
use crate::agent::util::glob::matches_any_pattern;
use crate::agent::util::request_channel::{
    RequestReceiver,
    RequestSender,
    respond,
};

/// Handle for communicating with an [`Agent`] actor.
#[derive(Debug)]
pub struct AgentHandle {
    sender: RequestSender<AgentRequest, AgentResponse, AgentError>,
    event_rx: broadcast::Receiver<AgentEvent>,
    /// Receiver for the lossless summary channel. Shared across handle clones
    /// (only the subagent driver drains it); see [`Agent::summary_tx`].
    summary_rx: Arc<tokio::sync::Mutex<mpsc::UnboundedReceiver<Summary>>>,
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
            summary_rx: Arc::clone(&self.summary_rx),
        }
    }
}

impl AgentHandle {
    pub async fn recv(&mut self) -> Result<AgentEvent, broadcast::error::RecvError> {
        self.event_rx.recv().await
    }

    /// Drain the lossless summary channel, returning the latest buffered
    /// [`Summary`] (or `None`). Non-blocking; recovers a summary even when the
    /// lossy event broadcast dropped the `SubagentSummary` event.
    pub async fn take_summary(&self) -> Option<Summary> {
        let mut rx = self.summary_rx.lock().await;
        let mut latest = None;
        while let Ok(s) = rx.try_recv() {
            latest = Some(s);
        }
        latest
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
        tracing::trace!("tool use approval sent");
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

    pub async fn get_mcp_prompts(&self) -> Result<HashMap<String, Vec<Prompt>>, AgentError> {
        match self
            .sender
            .send_recv(AgentRequest::GetMcpPrompts)
            .await
            .unwrap_or(Err(AgentError::Channel))?
        {
            AgentResponse::McpPrompts(prompts) => Ok(prompts),
            other => Err(AgentError::Custom(format!("received unexpected response: {other:?}"))),
        }
    }

    pub async fn get_file_prompts(&self) -> Result<HashMap<String, Vec<Prompt>>, AgentError> {
        match self
            .sender
            .send_recv(AgentRequest::GetFilePrompts)
            .await
            .unwrap_or(Err(AgentError::Channel))?
        {
            AgentResponse::FilePrompts(prompts) => Ok(prompts),
            other => Err(AgentError::Custom(format!("received unexpected response: {other:?}"))),
        }
    }

    pub async fn get_skills(&self) -> Result<HashMap<String, Vec<Prompt>>, AgentError> {
        match self
            .sender
            .send_recv(AgentRequest::GetSkills)
            .await
            .unwrap_or(Err(AgentError::Channel))?
        {
            AgentResponse::Skills(skills) => Ok(skills),
            other => Err(AgentError::Custom(format!("received unexpected response: {other:?}"))),
        }
    }

    pub async fn resolve_skill(&self, name: String) -> Result<Option<String>, AgentError> {
        match self
            .sender
            .send_recv(AgentRequest::ResolveSkill { name })
            .await
            .unwrap_or(Err(AgentError::Channel))?
        {
            AgentResponse::SkillContent(content) => Ok(content),
            other => Err(AgentError::Custom(format!("received unexpected response: {other:?}"))),
        }
    }

    pub async fn get_mcp_prompt(
        &self,
        name: String,
        arguments: HashMap<String, String>,
    ) -> Result<Vec<serde_json::Value>, AgentError> {
        match self
            .sender
            .send_recv(AgentRequest::GetMcpPrompt { name, arguments })
            .await
            .unwrap_or(Err(AgentError::Channel))?
        {
            AgentResponse::McpPrompt(messages) => Ok(messages),
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

    /// Push a new MCP registry snapshot to the running agent.
    ///
    /// The agent replaces its stored registry, re-applies it to the current
    /// agent config, and reloads its MCP servers. Returns
    /// [`AgentError::NotIdle`] if the agent is not idle; callers (such as
    /// `AcpSession`) typically defer the call until the next idle window
    /// rather than racing it against an in-flight prompt.
    pub async fn refresh_mcp_registry(&self, registry: Box<dyn mcp::McpRegistry>) -> Result<(), AgentError> {
        match self
            .sender
            .send_recv(AgentRequest::RefreshMcpRegistry(registry))
            .await
            .unwrap_or(Err(AgentError::Channel))?
        {
            AgentResponse::Success => Ok(()),
            other => Err(AgentError::Custom(format!("received unexpected response: {other:?}"))),
        }
    }

    /// Swap in a freshly-loaded agent config and surgically reconcile its MCP
    /// servers (start added, stop removed, restart changed, leave unchanged).
    ///
    /// Unlike [`swap_agent`](Self::swap_agent), this does not tear down and
    /// relaunch every server — it only touches servers whose presence or config
    /// changed. Used by the config file watcher for live, low-churn updates.
    /// Returns [`AgentError::NotIdle`] if the agent is not idle.
    pub async fn reconcile_mcp_servers(&self, config: Box<LoadedAgentConfig>) -> Result<(), AgentError> {
        match self
            .sender
            .send_recv(AgentRequest::ReconcileMcpServers(config))
            .await
            .unwrap_or(Err(AgentError::Channel))?
        {
            AgentResponse::Success => Ok(()),
            other => Err(AgentError::Custom(format!("received unexpected response: {other:?}"))),
        }
    }

    pub fn terminate(&self) {
        trace!("AgentHandle::terminate() — fire-and-forget Terminate request");
        // Fire-and-forget: enqueue the Terminate request without waiting for
        // the response. The agent loop will process it and break out. We don't
        // need the TerminateAcknowledged response since this handle is being
        // dropped — waiting would race (try_recv never sees the response) and
        // log a spurious error.
        self.sender.try_send_no_recv(AgentRequest::Terminate);
    }

    /// Async version of [`terminate`](Self::terminate) that awaits the agent's cleanup
    /// (including MCP server shutdown) before returning. Use this during graceful shutdown
    /// to ensure child processes are cleaned up before the tokio runtime exits.
    pub async fn shutdown(&self) {
        _ = self.sender.send_recv(AgentRequest::Terminate).await;
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

    pub async fn get_mcp_server_info(&self) -> Result<Vec<tui_commands::McpServerInfo>, AgentError> {
        match self
            .sender
            .send_recv(AgentRequest::GetMcpServerInfo)
            .await
            .unwrap_or(Err(AgentError::Channel))?
        {
            AgentResponse::McpServerInfo(info) => Ok(info),
            other => Err(AgentError::Custom(format!("received unexpected response: {other:?}"))),
        }
    }

    /// Force (re-)authentication for a single remote MCP server.
    ///
    /// Marks the server's config with `force_auth = true`, shuts it down, and
    /// relaunches it so the OAuth flow runs. Returns [`AgentError::NotIdle`] if the
    /// agent is not idle, or an error if the named server doesn't exist or isn't a
    /// remote (HTTP) server. The relaunch is fire-and-forget; observe progress via
    /// MCP server events.
    pub async fn reauth_mcp_server(&self, server_name: String) -> Result<(), AgentError> {
        match self
            .sender
            .send_recv(AgentRequest::ReauthMcpServer { server_name })
            .await
            .unwrap_or(Err(AgentError::Channel))?
        {
            AgentResponse::Success => Ok(()),
            other => Err(AgentError::Custom(format!("received unexpected response: {other:?}"))),
        }
    }

    /// Abort a pending/forced authentication for a single remote MCP server.
    ///
    /// Clears the server's `force_auth` flag, shuts it down (cancelling any
    /// in-flight OAuth flow and local redirect loopback), and relaunches it under
    /// the normal flow. Returns [`AgentError::NotIdle`] if the agent is not idle,
    /// or an error if the named server doesn't exist or isn't a remote (HTTP) server.
    pub async fn abort_mcp_server_auth(&self, server_name: String) -> Result<(), AgentError> {
        match self
            .sender
            .send_recv(AgentRequest::AbortMcpServerAuth { server_name })
            .await
            .unwrap_or(Err(AgentError::Channel))?
        {
            AgentResponse::Success => Ok(()),
            other => Err(AgentError::Custom(format!("received unexpected response: {other:?}"))),
        }
    }

    /// Remove the persisted OAuth credentials (token + dynamic client registration)
    /// for a single remote MCP server. Does not stop or relaunch the server.
    /// Returns an error if the named server doesn't exist or isn't a remote (HTTP) server.
    pub async fn remove_mcp_server_credentials(&self, server_name: String) -> Result<(), AgentError> {
        match self
            .sender
            .send_recv(AgentRequest::RemoveMcpServerCredentials { server_name })
            .await
            .unwrap_or(Err(AgentError::Channel))?
        {
            AgentResponse::Success => Ok(()),
            other => Err(AgentError::Custom(format!("received unexpected response: {other:?}"))),
        }
    }

    pub async fn get_tool_info(&self) -> Result<Vec<tui_commands::ToolInfo>, AgentError> {
        match self
            .sender
            .send_recv(AgentRequest::GetToolInfo)
            .await
            .unwrap_or(Err(AgentError::Channel))?
        {
            AgentResponse::ToolInfo(info) => Ok(info),
            other => Err(AgentError::Custom(format!("received unexpected response: {other:?}"))),
        }
    }

    pub async fn add_resource(&self, path: String) -> Result<(), AgentError> {
        match self
            .sender
            .send_recv(AgentRequest::AddResource(path))
            .await
            .unwrap_or(Err(AgentError::Channel))?
        {
            AgentResponse::Success => Ok(()),
            other => Err(AgentError::Custom(format!("received unexpected response: {other:?}"))),
        }
    }

    pub async fn remove_resource(&self, path: String) -> Result<(), AgentError> {
        match self
            .sender
            .send_recv(AgentRequest::RemoveResource(path))
            .await
            .unwrap_or(Err(AgentError::Channel))?
        {
            AgentResponse::Success => Ok(()),
            other => Err(AgentError::Custom(format!("received unexpected response: {other:?}"))),
        }
    }

    pub async fn get_resources(&self) -> Result<Vec<String>, AgentError> {
        match self
            .sender
            .send_recv(AgentRequest::GetResources)
            .await
            .unwrap_or(Err(AgentError::Channel))?
        {
            AgentResponse::Resources(resources) => Ok(resources),
            other => Err(AgentError::Custom(format!("received unexpected response: {other:?}"))),
        }
    }

    pub async fn clear_session_resources(&self) -> Result<(), AgentError> {
        match self
            .sender
            .send_recv(AgentRequest::ClearSessionResources)
            .await
            .unwrap_or(Err(AgentError::Channel))?
        {
            AgentResponse::Success => Ok(()),
            other => Err(AgentError::Custom(format!("received unexpected response: {other:?}"))),
        }
    }

    pub async fn get_last_assistant_message(&self) -> Result<Option<String>, AgentError> {
        match self
            .sender
            .send_recv(AgentRequest::GetLastAssistantMessage)
            .await
            .unwrap_or(Err(AgentError::Channel))?
        {
            AgentResponse::LastAssistantMessage(msg) => Ok(msg),
            other => Err(AgentError::Custom(format!("received unexpected response: {other:?}"))),
        }
    }

    pub async fn trust_all_tools(&self) -> Result<(), AgentError> {
        match self
            .sender
            .send_recv(AgentRequest::TrustAllTools)
            .await
            .unwrap_or(Err(AgentError::Channel))?
        {
            AgentResponse::Success => Ok(()),
            other => Err(AgentError::Custom(format!("received unexpected response: {other:?}"))),
        }
    }

    pub async fn trust_tools(&self, names: Vec<String>) -> Result<(Vec<String>, Vec<String>), AgentError> {
        match self
            .sender
            .send_recv(AgentRequest::TrustTools(names))
            .await
            .unwrap_or(Err(AgentError::Channel))?
        {
            AgentResponse::ToolTrustResult { changed, invalid } => Ok((changed, invalid)),
            other => Err(AgentError::Custom(format!("received unexpected response: {other:?}"))),
        }
    }

    pub async fn untrust_tools(&self, names: Vec<String>) -> Result<(Vec<String>, Vec<String>), AgentError> {
        match self
            .sender
            .send_recv(AgentRequest::UntrustTools(names))
            .await
            .unwrap_or(Err(AgentError::Channel))?
        {
            AgentResponse::ToolTrustResult { changed, invalid } => Ok((changed, invalid)),
            other => Err(AgentError::Custom(format!("received unexpected response: {other:?}"))),
        }
    }

    pub async fn reset_tool_permissions(&self) -> Result<(), AgentError> {
        match self
            .sender
            .send_recv(AgentRequest::ResetToolPermissions)
            .await
            .unwrap_or(Err(AgentError::Channel))?
        {
            AgentResponse::Success => Ok(()),
            other => Err(AgentError::Custom(format!("received unexpected response: {other:?}"))),
        }
    }

    pub async fn set_trust_all_tools(&self, trust: bool) -> Result<(), AgentError> {
        self.sender
            .send_recv(AgentRequest::SetTrustAllTools(trust))
            .await
            .unwrap_or(Err(AgentError::Channel))?;
        Ok(())
    }

    /// Invalidate cached tool specs to simulate MCP ToolListChanged race condition.
    /// Exposed for integration testing.
    pub async fn invalidate_cached_tool_specs(&self) -> Result<(), AgentError> {
        self.sender
            .send_recv(AgentRequest::InvalidateCachedToolSpecs)
            .await
            .unwrap_or(Err(AgentError::Channel))?;
        Ok(())
    }

    /// Queue a steering message for injection at the next tool boundary.
    pub async fn steer_message(&self, message: String) -> Result<(), AgentError> {
        match self
            .sender
            .send_recv(AgentRequest::SteerMessage { message })
            .await
            .unwrap_or(Err(AgentError::Channel))?
        {
            AgentResponse::Success => Ok(()),
            other => Err(AgentError::Custom(format!("received unexpected response: {other:?}"))),
        }
    }

    /// Clear any queued steering message without consuming it.
    pub async fn clear_steering(&self) -> Result<(), AgentError> {
        match self
            .sender
            .send_recv(AgentRequest::ClearSteering)
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
    agent_config: LoadedAgentConfig,

    conversation_state: ConversationState,
    conversation_metadata: ConversationMetadata,
    execution_state: ExecutionState,
    tool_state: ToolState,
    /// Runtime permissions accumulated during the session
    permissions: RuntimePermissions,

    agent_event_tx: broadcast::Sender<AgentEvent>,
    agent_event_rx: Option<broadcast::Receiver<AgentEvent>>,

    /// Lossless channel for the subagent `summary` result. The shared event
    /// broadcast is lossy (can evict `SubagentSummary` under load, surfacing as
    /// "No result"); this carries the summary on its own so it can't be dropped.
    summary_tx: mpsc::UnboundedSender<Summary>,
    summary_rx: Option<mpsc::UnboundedReceiver<Summary>>,

    agent_event_buf: Vec<AgentEvent>,

    /// Contains an [AgentLoop] if the agent is in the middle of executing a user turn, otherwise
    /// is [None].
    agent_loop: Option<AgentLoopHandle>,

    /// Contains an [AgentLoop] for compaction requests, separate from the main agent loop.
    compaction_loop: Option<AgentLoopHandle>,

    /// Used for executing tools and hooks in the background
    task_executor: TaskExecutor,
    mcp_manager_handle: McpManagerHandle,

    /// Workspace-level `mcp.json` path resolved by the host. Stored on the
    /// agent so MCP-config reloads (swap, registry refresh) don't need the
    /// host to plumb the path back in on every call.
    local_mcp_path: Option<PathBuf>,
    /// Global `mcp.json` path resolved by the host. See [`Self::local_mcp_path`].
    global_mcp_path: Option<PathBuf>,

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
    /// Knowledge base provider (optional, injected by the host)
    knowledge_provider: Option<Arc<dyn tools::KnowledgeProvider>>,
    /// Task store for task management tools (None for subagents and V1 agents)
    task_store: Option<Arc<TaskStore>>,
    /// Paths added via /context add during this session (not from agent config)
    session_resource_paths: HashSet<String>,
    /// All available agent configs, used for dynamic tool spec generation (e.g. AgentCrew)
    available_agent_configs: Vec<LoadedAgentConfig>,
    /// MCP registry to apply to the agent config before launching MCP servers.
    ///
    /// When set, the registry's [`McpRegistry::apply`](mcp::McpRegistry::apply)
    /// is invoked at construction time and on agent swap, transforming the
    /// [`LoadedAgentConfig`] (filtering servers/tools, resolving registry
    /// placeholders) before MCP servers are loaded. Hosts can push a fresh
    /// registry to a running agent via the agent request channel; the agent
    /// will re-apply it.
    ///
    /// `None` means no registry — the agent uses its config as-is. This is the
    /// expected case for unmanaged users, V1 subagents, and most tests.
    mcp_registry: Option<Box<dyn mcp::McpRegistry>>,
    /// BM25 tool index for tool_search (built from MCP tool specs)
    tool_search_index: ToolIndex,
    /// Configuration for tool search matching thresholds
    tool_search_config: ToolLoadConfig,
    /// Set of MCP tools activated via tool_search auto-load
    tool_search_activated: HashSet<CanonicalToolName>,
    /// Whether tool search is effectively active (computed from settings + thresholds)
    tool_search_active: bool,

    /// Queued steering messages for mid-turn injection.
    /// Consumed at the next tool boundary (send_tool_results) or end-of-turn.
    /// Each steer carries a stable `steer-<uuid>` id so queued/consumed/cleared
    /// notifications can be correlated by id (matching the KAS contract). When
    /// drained, the steers' text is concatenated with "\n\n" into a single LLM
    /// continuation request, while one consume notification is emitted per steer.
    queued_steers: Vec<QueuedSteer>,

    /// Number of consecutive agent-loop turns that produced no executable tool
    /// calls (only parse errors and/or `dummy` placeholder calls). Incremented
    /// each time we would auto-resend synthesized failure/guidance results with
    /// nothing to execute, and reset to 0 whenever a real tool dispatches or a
    /// new user prompt arrives. Once it reaches
    /// [`MAX_CONSECUTIVE_UNEXECUTABLE_TOOL_TURNS`] the turn is force-ended to
    /// prevent an unbounded unavailable-tool retry loop.
    consecutive_unexecutable_tool_turns: usize,

    /// In-flight forced (re-)auth flows: **shadow** server name → **target** name.
    /// A loaded server's shadow runs OAuth alongside the original and is promoted on
    /// success; an unloaded one uses `target -> target`. Drives the `authenticating`
    /// flag and guards against concurrent reauth.
    reauth_shadows: HashMap<String, String>,
}

/// A single queued steering message awaiting injection.
#[derive(Debug, Clone)]
struct QueuedSteer {
    /// Stable `steer-<uuid>` id, surfaced on the queued/consumed/cleared
    /// notifications so clients can track each steer by id.
    id: String,
    /// The raw, user-typed steering text (trimmed).
    text: String,
}

/// Join queued steers' text into the full queue snapshot (the value carried by
/// the queued notification and injected into the LLM as a single block).
fn steer_snapshot(steers: &[QueuedSteer]) -> String {
    steers.iter().map(|s| s.text.as_str()).collect::<Vec<_>>().join("\n\n")
}

impl Agent {
    /// Prefix for the hidden "shadow" MCP server used to run a forced
    /// (re-)authentication flow alongside a still-running original. Shadow servers
    /// are never surfaced to the UI as separate entries.
    const REAUTH_SHADOW_PREFIX: &'static str = "__reauth__";

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
    /// * `knowledge_provider` - Knowledge base provider (optional)
    /// * `task_store` - Task store for task management (None for subagents and V1 agents)
    /// * `available_agent_configs` - All loaded agent configs for dynamic tool spec generation
    /// * `mcp_registry` - Optional MCP registry to apply to the agent config before launching MCP
    ///   servers (filters servers/tools, resolves registry placeholders). The agent stores the
    ///   registry and re-applies it on swap and on registry refresh.
    #[allow(clippy::too_many_arguments)]
    pub async fn new(
        snapshot: AgentSnapshot,
        local_mcp_path: Option<&PathBuf>,
        global_mcp_path: Option<&PathBuf>,
        model: Arc<dyn Model>,
        mcp_manager_handle: McpManagerHandle,
        is_subagent: bool,
        code_intelligence: Option<Arc<RwLock<CodeIntelligence>>>,
        knowledge_provider: Option<Arc<dyn tools::KnowledgeProvider>>,
        task_store: Option<Arc<TaskStore>>,
        available_agent_configs: Vec<LoadedAgentConfig>,
        mcp_registry: Option<Box<dyn mcp::McpRegistry>>,
    ) -> eyre::Result<Agent> {
        debug!(?snapshot, "initializing agent from snapshot");

        let (agent_event_tx, agent_event_rx) = broadcast::channel(8192);
        // Lossless side-channel for the summary tool result (see field docs).
        let (summary_tx, summary_rx) = mpsc::unbounded_channel();

        let mut agent_config = snapshot.agent_config;
        // Enforce MCP governance at construction — defense-in-depth in case the caller
        // (SessionManager / TUI) forgot to strip MCP before creating the session.
        if !snapshot.settings.mcp_enabled {
            agent_config.config_mut().clear_mcp_configs();
        }

        // Apply registry transformations (filter servers/tools, resolve placeholders) before
        // loading MCP server configs, so launches see the registry-resolved view.
        if let Some(registry) = mcp_registry.as_ref() {
            registry.apply(&mut agent_config);
        }

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
            summary_tx,
            summary_rx: Some(summary_rx),
            agent_event_buf: Vec::new(),
            agent_loop: None,
            compaction_loop: None,
            task_executor,
            mcp_manager_handle,
            local_mcp_path: local_mcp_path.cloned(),
            global_mcp_path: global_mcp_path.cloned(),
            agent_spawn_hooks: Default::default(),
            model,
            settings: snapshot.settings,
            cached_tool_specs: None,
            cached_mcp_configs,
            sys_provider,
            is_subagent,
            code_intelligence,
            knowledge_provider,
            task_store,
            session_resource_paths: HashSet::new(),
            available_agent_configs,
            mcp_registry,
            tool_search_index: ToolIndex::default(),
            tool_search_config: ToolLoadConfig::from_env(),
            tool_search_activated: HashSet::new(),
            tool_search_active: false,
            queued_steers: Vec::new(),
            consecutive_unexecutable_tool_turns: 0,
            reauth_shadows: HashMap::new(),
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
        let summary_rx = self.summary_rx.take().expect("should exist");
        tokio::spawn(async move {
            self.initialize().await;
            self.main_loop(rx).await;
        });
        AgentHandle {
            sender: tx,
            event_rx,
            // Wrapped so the handle stays `Clone` (clones share the single
            // consumer); only `handle_internal_prompt` actually drains it.
            summary_rx: Arc::new(tokio::sync::Mutex::new(summary_rx)),
        }
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
                .enumerate()
                .map(|(index, hook)| {
                    (
                        HookExecutionId {
                            hook,
                            tool_context: None,
                            index,
                        },
                        None,
                    )
                })
                .collect();
            self.start_hooks_execution(hooks, HookStage::AgentSpawn, None, None)
                .await;
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
                        // Best-effort response — the caller may have already dropped the
                        // receiver (fire-and-forget terminate). Don't log an error.
                        let _ = req.res_tx.send(res);
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
                        self.enter_error_state(e).await;
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
                        self.agent_event_buf.push(evt.clone().into());
                        if let Err(e) = self.handle_task_executor_event(evt).await {
                            error!(?e, "failed to handle tool executor event");
                            self.set_active_state(ActiveState::Errored(e)).await;
                        }
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
        let from = Box::new(self.execution_state.clone());
        self.execution_state.active_state = new_state;
        let to = Box::new(self.execution_state.clone());
        self.agent_event_buf
            .push(AgentEvent::Internal(InternalEvent::StateChange { from, to }));
    }

    /// Clears all conversation-related state for a fresh start.
    fn clear_conversation(&mut self) {
        // Append Clear to event log (keeps session, clears messages)
        let entry = LogEntry::clear();
        let index = self.conversation_state.append_log(entry.clone());
        self.conversation_metadata = ConversationMetadata::default();
        self.tool_state = ToolState::default();
        self.agent_event_buf.push(AgentEvent::LogEntryAppended { entry, index });
        self.agent_event_buf.push(AgentEvent::Clear(ClearEvent));
    }

    fn create_snapshot(&self) -> AgentSnapshot {
        // Get tool specs from cache if available
        let tool_specs = self
            .cached_tool_specs
            .as_ref()
            .map(|s| s.tool_map().values().map(|t| t.tool_spec().clone()).collect())
            .unwrap_or_default();

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
            tool_specs,
            session_resource_paths: self.session_resource_paths.clone(),
            has_knowledge_provider: self.knowledge_provider.is_some(),
        }
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

    /// Transition to error state, emit `Stop(Error)`, then end the current turn
    /// so that `EndTurn` is always emitted as the final event.
    async fn enter_error_state(&mut self, err: AgentError) {
        self.set_active_state(ActiveState::Errored(err.clone())).await;
        self.agent_event_buf.push(AgentEvent::Stop(AgentStopReason::Error(err)));
        if let Err(e) = self.end_current_turn(true).await {
            warn!(?e, "failed to end current turn after entering error state");
        }
    }

    /// Ends the current user turn by cancelling [Self::agent_loop] if it exists.
    ///
    /// `salvage_pending_summary` recovers a pending (un-executed) `summary` tool
    /// use onto the lossless summary channel before it is replaced with a
    /// synthetic cancelled result. This is set only on the error-teardown path
    /// (e.g. an empty/cancelled trailing model response erroring the turn after
    /// the subagent already produced its result); explicit user cancellation
    /// passes `false` so a killed subagent reports no result, as intended.
    async fn end_current_turn(
        &mut self,
        salvage_pending_summary: bool,
    ) -> Result<Option<UserTurnMetadata>, AgentError> {
        let Some(mut handle) = self.agent_loop.take() else {
            return Ok(None);
        };

        // Check if tool calls didn't complete (cancelled mid-execution, approval denied, etc.).
        // If so, add placeholder "cancelled" tool results to maintain the alternating message
        // invariant. Skip this if we already have completed tool results pending commit — in that
        // case the tool ran successfully and we should preserve the real results.
        let has_pending_tool_results = matches!(&self.execution_state.active_state, ActiveState::ExecutingRequest {
            pending_user_message: Some(PendingUserMessage::ToolResults { .. }),
            ..
        });
        let has_pending_tool_uses = !has_pending_tool_results
            && self
                .conversation_state
                .messages()
                .last()
                .filter(|m| m.role == Role::Assistant)
                .and_then(|m| m.tool_uses())
                .is_some();

        if has_pending_tool_uses {
            let mut content = Vec::new();
            let mut results = HashMap::new();
            if let Some(m) = self.conversation_state.messages().last() {
                for c in &m.content {
                    if let ContentBlock::ToolUse(tool_use) = c {
                        // A subagent commonly calls `summary` as the last thing it does, but the
                        // turn can be torn down (e.g. an empty/cancelled trailing model response
                        // erroring the turn) before the tool actually executes. The model already
                        // produced the real result in the tool_use input, so salvage it onto the
                        // lossless summary channel here; otherwise the parent degrades to the
                        // empty-response fallback and the user's result is silently lost. Gated to
                        // the error path only — explicit user cancellation reports no result.
                        if salvage_pending_summary
                            && let Some(summary) = recover_pending_summary(&tool_use.name, &tool_use.input)
                        {
                            if self.summary_tx.send(summary).is_err() {
                                trace!("no summary receiver while salvaging cancelled summary tool use");
                            } else {
                                trace!("salvaged summary from cancelled summary tool use on turn teardown");
                            }
                        }
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
            // synthetic id; message only sent as history, not as the active prompt of a request
            self.append_tool_results(Uuid::new_v4().to_string(), content, results);
            self.append_assistant_message(Message::new(
                // synthetic id; message only sent as history, not as the active prompt of a request
                Uuid::new_v4().to_string(),
                Role::Assistant,
                vec![ContentBlock::Text(consts::TOOL_USES_INTERRUPTED_MESSAGE.to_string())],
                Some(Utc::now()),
            ));
        }

        handle.cancel().await?;
        while let Some(evt) = handle.recv().await {
            self.agent_event_buf
                .push(AgentLoopEvent::new(handle.id().clone(), evt.clone()).into());
            if let AgentLoopEventKind::UserTurnEnd(md) = evt {
                self.conversation_metadata.user_turn_metadatas.push(md.clone());

                // Commit the pending user message so it's preserved in history.
                // The model needs context of what the user asked even if the turn was cancelled.
                // We also append a placeholder assistant message to maintain the alternating
                // User/Assistant invariant required by the API.
                if let ActiveState::ExecutingRequest {
                    pending_user_message: Some(pending),
                    ..
                } = &self.execution_state.active_state
                {
                    match pending.clone() {
                        PendingUserMessage::Prompt { id, content, meta } => {
                            self.append_user_message(id, content, meta);
                        },
                        PendingUserMessage::ToolResults { id, content, results } => {
                            self.append_tool_results(id, content, results);
                        },
                    }
                    self.append_assistant_message(Message::new(
                        // synthetic id; message only sent as history, not as the active prompt of a request
                        Uuid::new_v4().to_string(),
                        Role::Assistant,
                        vec![ContentBlock::Text(consts::RESPONSE_INTERRUPTED_MESSAGE.to_string())],
                        Some(Utc::now()),
                    ));
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
            AgentRequest::GetFilePrompts => {
                let response = match self.sys_provider.cwd() {
                    Ok(cwd) => prompts::discover(&cwd),
                    Err(_) => HashMap::new(),
                };
                Ok(AgentResponse::FilePrompts(response))
            },
            AgentRequest::GetSkills => {
                let response =
                    prompts::discover_skills_from_resources(&self.agent_config.resources(), &self.sys_provider);
                Ok(AgentResponse::Skills(response))
            },
            AgentRequest::ResolveSkill { name } => {
                let content =
                    prompts::resolve_skill_from_resources(&self.agent_config.resources(), &self.sys_provider, &name);
                Ok(AgentResponse::SkillContent(content))
            },
            AgentRequest::GetMcpPrompt { name, arguments } => {
                // Parse server name from prompt name (format: server_name/prompt_name)
                let (server_name, prompt_name) = match name.split_once('/') {
                    Some((server, prompt)) => (server.to_string(), prompt.to_string()),
                    None => {
                        // If no server specified, find which server has this prompt
                        let mut found_server = None;
                        for server_name in self.cached_mcp_configs.server_names() {
                            if let Ok(prompts) = self.mcp_manager_handle.get_prompts(server_name.clone()).await
                                && prompts.iter().any(|p| p.name == name)
                            {
                                if found_server.is_some() {
                                    return Err(AgentError::Custom(format!(
                                        "Ambiguous prompt name '{}'. Multiple servers have this prompt. Use server_name/{} format.",
                                        name, name
                                    )));
                                }
                                found_server = Some(server_name);
                            }
                        }
                        match found_server {
                            Some(server) => (server, name),
                            None => {
                                return Err(AgentError::Custom(format!("Prompt '{}' not found in any server", name)));
                            },
                        }
                    },
                };

                match self
                    .mcp_manager_handle
                    .get_prompt(server_name, prompt_name, arguments)
                    .await
                {
                    Ok(messages) => Ok(AgentResponse::McpPrompt(messages)),
                    Err(err) => Err(AgentError::Custom(format!("Failed to get prompt: {}", err))),
                }
            },
            AgentRequest::Terminate => {
                _ = self.handle_cancel_request().await;
                self.mcp_manager_handle.shutdown().await;

                Ok(AgentResponse::TerminateAcknowledged)
            },
            AgentRequest::SwapAgent(args) => self.handle_swap_agent(*args).await,
            AgentRequest::RefreshMcpRegistry(registry) => self.handle_refresh_mcp_registry(registry).await,
            AgentRequest::ReconcileMcpServers(config) => self.handle_reconcile_mcp_servers(*config).await,
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
            AgentRequest::GetMcpServerInfo => {
                let mut servers = Vec::new();
                for config in &self.cached_mcp_configs.configs {
                    let server_name = &config.server_name;
                    let (status, tool_count) = if !config.is_enabled() {
                        (tui_commands::McpServerStatus::Disabled, 0)
                    } else {
                        match self.mcp_manager_handle.get_tool_specs(server_name.clone()).await {
                            Ok(specs) => (tui_commands::McpServerStatus::Running, specs.len()),
                            Err(
                                mcp::McpManagerError::ServerNotInitialized { .. }
                                | mcp::McpManagerError::ServerCurrentlyInitializing { .. },
                            ) => (tui_commands::McpServerStatus::Loading, 0),
                            Err(ref e) => {
                                warn!(server_name, error = %e, "MCP server failed");
                                (tui_commands::McpServerStatus::Failed, 0)
                            },
                        }
                    };
                    servers.push(tui_commands::McpServerInfo {
                        name: server_name.clone(),
                        status,
                        tool_count,
                        // A forced (re-)auth is in flight for this server if it is a
                        // target of any tracked shadow. The shadow itself is never
                        // listed here (it isn't in cached_mcp_configs).
                        authenticating: self.reauth_shadows.values().any(|t| t == server_name),
                    });
                }
                Ok(AgentResponse::McpServerInfo(servers))
            },
            AgentRequest::ReauthMcpServer { server_name } => self.handle_reauth_mcp_server(server_name).await,
            AgentRequest::AbortMcpServerAuth { server_name } => self.handle_abort_mcp_server_auth(server_name).await,
            AgentRequest::RemoveMcpServerCredentials { server_name } => {
                self.handle_remove_mcp_server_credentials(server_name).await
            },
            AgentRequest::GetToolInfo => {
                // Use cached tool specs if available, otherwise build them fresh
                let tool_specs = if let Some(ref cached) = self.cached_tool_specs {
                    cached.tool_map().clone()
                } else {
                    // Build tool specs (this also caches them)
                    self.make_tool_spec().await;
                    self.cached_tool_specs
                        .as_ref()
                        .map(|c| c.tool_map().clone())
                        .unwrap_or_default()
                };

                let allowed_tools = self.agent_config.allowed_tools();

                let mut tools: Vec<tui_commands::ToolInfo> = tool_specs
                    .values()
                    .map(|spec| {
                        let canonical = spec.canonical_name();
                        let source = match canonical {
                            agent_config::parse::CanonicalToolName::BuiltIn(_) => "built-in".to_string(),
                            agent_config::parse::CanonicalToolName::Mcp { server_name, .. } => {
                                format!("mcp:{server_name}")
                            },
                            agent_config::parse::CanonicalToolName::Agent { agent_name } => {
                                format!("agent:{agent_name}")
                            },
                        };

                        let status = if self.permissions.is_tool_denied(canonical) {
                            tui_commands::ToolStatus::Denied
                        } else if self.permissions.is_tool_trusted(canonical) || self.settings.trust_all_tools {
                            tui_commands::ToolStatus::Allowed
                        } else {
                            // Check config-level allowed_tools
                            let tool_name = canonical.as_full_name();
                            let is_config_allowed = match canonical {
                                agent_config::parse::CanonicalToolName::BuiltIn(built_in) => {
                                    allowed_tools.contains("@builtin")
                                        || allowed_tools.contains("@builtin/")
                                        || allowed_tools.contains("@builtin/*")
                                        || util::glob::matches_any_pattern(allowed_tools, &tool_name)
                                        || built_in.aliases().is_some_and(|aliases| {
                                            aliases.iter().any(|alias| {
                                                util::glob::matches_any_pattern(allowed_tools, alias)
                                                    || util::glob::matches_any_pattern(
                                                        allowed_tools,
                                                        format!("@builtin/{alias}"),
                                                    )
                                            })
                                        })
                                },
                                agent_config::parse::CanonicalToolName::Mcp { server_name, .. } => {
                                    allowed_tools.contains(&format!("@{server_name}"))
                                        || allowed_tools.contains(&format!("@{server_name}/"))
                                        || util::glob::matches_any_pattern(allowed_tools, &tool_name)
                                },
                                agent_config::parse::CanonicalToolName::Agent { .. } => false,
                            };
                            if is_config_allowed {
                                tui_commands::ToolStatus::Allowed
                            } else {
                                tui_commands::ToolStatus::RequiresApproval
                            }
                        };

                        tui_commands::ToolInfo {
                            name: spec.tool_spec().name.clone(),
                            source,
                            description: spec.tool_spec().description.clone(),
                            status,
                        }
                    })
                    .collect();
                tools.sort_by(|a, b| a.source.cmp(&b.source).then(a.name.cmp(&b.name)));
                Ok(AgentResponse::ToolInfo(tools))
            },
            AgentRequest::AddResource(path) => {
                let resource_path = format!("file://{path}");
                let resource = match resource_path.parse::<agent_config::types::ResourcePath>() {
                    Ok(r) => r,
                    Err(e) => return Err(AgentError::Custom(format!("Invalid resource path: {e}"))),
                };
                if self.agent_config.add_resource(resource) {
                    self.session_resource_paths.insert(format!("file://{path}"));
                    Ok(AgentResponse::Success)
                } else {
                    Err(AgentError::Custom(format!("Rule '{path}' already exists.")))
                }
            },
            AgentRequest::RemoveResource(path) => {
                // Try both with and without file:// prefix
                let removed = self.agent_config.remove_resource(&path)
                    || self.agent_config.remove_resource(&format!("file://{path}"));
                if removed {
                    self.session_resource_paths.remove(&path);
                    self.session_resource_paths.remove(&format!("file://{path}"));
                    Ok(AgentResponse::Success)
                } else {
                    Err(AgentError::Custom(format!("Resource not found: {path}")))
                }
            },
            AgentRequest::GetResources => {
                let resources = self
                    .agent_config
                    .resources()
                    .iter()
                    .map(|r| r.as_ref().to_string())
                    .collect();
                Ok(AgentResponse::Resources(resources))
            },
            AgentRequest::ClearSessionResources => {
                self.agent_config.clear_all_resources();
                self.session_resource_paths.clear();
                Ok(AgentResponse::Success)
            },
            AgentRequest::GetLastAssistantMessage => {
                let msg = self
                    .conversation_state
                    .messages()
                    .iter()
                    .rev()
                    .find(|m| m.role == Role::Assistant)
                    .map(|m| {
                        m.content
                            .iter()
                            .filter_map(|c| match c {
                                ContentBlock::Text(t) => Some(t.as_str()),
                                _ => None,
                            })
                            .collect::<Vec<_>>()
                            .join("")
                    })
                    .filter(|s| !s.is_empty());
                Ok(AgentResponse::LastAssistantMessage(msg))
            },
            AgentRequest::TrustAllTools => {
                let tool_specs = self.resolve_tool_specs().await;
                for spec in tool_specs.values() {
                    self.permissions.trust_tool(spec.canonical_name().clone());
                }
                self.settings.trust_all_tools = true;
                Ok(AgentResponse::Success)
            },
            AgentRequest::TrustTools(names) => {
                let tool_specs = self.resolve_tool_specs().await;
                let (changed, invalid) = self.resolve_and_apply_trust(&tool_specs, &names, true);
                Ok(AgentResponse::ToolTrustResult { changed, invalid })
            },
            AgentRequest::UntrustTools(names) => {
                let tool_specs = self.resolve_tool_specs().await;
                let (changed, invalid) = self.resolve_and_apply_trust(&tool_specs, &names, false);
                Ok(AgentResponse::ToolTrustResult { changed, invalid })
            },
            AgentRequest::ResetToolPermissions => {
                self.settings.trust_all_tools = false;
                self.permissions.reset();
                if let Ok(cwd) = self.sys_provider.cwd() {
                    self.permissions
                        .grant_path_canonicalized(cwd.to_string_lossy().into_owned(), PathAccessType::Read);
                }
                Ok(AgentResponse::Success)
            },
            AgentRequest::SetTrustAllTools(trust) => {
                self.settings.trust_all_tools = trust;
                Ok(AgentResponse::Success)
            },
            AgentRequest::InvalidateCachedToolSpecs => {
                self.cached_tool_specs = None;
                Ok(AgentResponse::Success)
            },
            AgentRequest::SteerMessage { message } => {
                let trimmed = message.trim();
                if trimmed.is_empty() {
                    return Err(AgentError::Custom("empty steering message".into()));
                }
                // Append onto the existing queue. Successive steers drain
                // together at the next tool boundary or end of turn.
                //
                // Each steer gets a stable `steer-<uuid>` id so the queued,
                // consumed, and cleared notifications can be correlated by id
                // (matching the KAS contract).
                //
                // No size cap is applied — the human typing rate is the
                // natural bound. If a runaway front-end becomes a problem
                // in practice, a cap can be added here.
                let id = format!("steer-{}", Uuid::new_v4().as_simple());
                self.queued_steers.push(QueuedSteer {
                    id: id.clone(),
                    text: trimmed.to_string(),
                });
                // The queued notification carries the full queue snapshot so
                // consumers overwrite their local copy rather than append.
                let snapshot = steer_snapshot(&self.queued_steers);
                self.agent_event_buf.push(AgentEvent::SteeringQueued {
                    message_id: id,
                    content: snapshot,
                });
                Ok(AgentResponse::Success)
            },
            AgentRequest::ClearSteering => {
                if !self.queued_steers.is_empty() {
                    let message_ids = self.queued_steers.drain(..).map(|s| s.id).collect();
                    self.agent_event_buf.push(AgentEvent::SteeringCleared { message_ids });
                }
                Ok(AgentResponse::Success)
            },
        }
    }

    async fn resolve_tool_specs(&mut self) -> HashMap<String, tool_utils::SanitizedToolSpec> {
        if let Some(ref cached) = self.cached_tool_specs {
            cached.tool_map().clone()
        } else {
            self.make_tool_spec().await;
            self.cached_tool_specs
                .as_ref()
                .map(|c| c.tool_map().clone())
                .unwrap_or_default()
        }
    }

    fn resolve_and_apply_trust(
        &mut self,
        tool_specs: &HashMap<String, tool_utils::SanitizedToolSpec>,
        names: &[String],
        trust: bool,
    ) -> (Vec<String>, Vec<String>) {
        let mut changed = Vec::new();
        let mut invalid = Vec::new();
        for name in names {
            if let Some(spec) = tool_specs.get(name) {
                let canonical = spec.canonical_name().clone();
                if trust {
                    self.permissions.trust_tool(canonical);
                } else {
                    self.permissions.untrust_tool(&canonical);
                }
                changed.push(name.clone());
            } else {
                invalid.push(name.clone());
            }
        }
        (changed, invalid)
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
        if !args.force && self.agent_config.name() == args.agent_config.name() {
            return Ok(AgentResponse::SwapComplete);
        }

        // 1. Terminate existing MCP servers
        self.mcp_manager_handle.terminate();

        // 2. Create new MCP manager (terminate kills the old one)
        self.mcp_manager_handle = McpManager::default().spawn();

        self.reauth_shadows.clear();

        // 3. Update agent config and clear cached tool specs
        self.agent_config = args.agent_config;
        // Enforce MCP governance on the incoming config — defense-in-depth in case the
        // caller (SessionManager / TUI) forgot to strip MCP before swapping.
        if !self.settings.mcp_enabled {
            self.agent_config.config_mut().clear_mcp_configs();
        }
        // Apply the stored registry to the swapped-in config so registry-driven filtering
        // and placeholder resolution stay consistent across agent swaps. Hosts no longer
        // need to pre-rewrite the config before calling swap.
        if let Some(registry) = self.mcp_registry.as_ref() {
            registry.apply(&mut self.agent_config);
        }
        self.cached_tool_specs = None;
        self.session_resource_paths.clear();

        // 4. Reload MCP configs from new agent config
        self.cached_mcp_configs = LoadedMcpServerConfigs::from_agent_config(
            &self.agent_config,
            self.local_mcp_path.as_ref(),
            self.global_mcp_path.as_ref(),
        )
        .await;

        // 5. Launch new MCP servers
        self.launch_mcp_servers().await;

        // 6. Update knowledge provider if a new one was provided
        if let Some(provider) = args.knowledge_provider {
            self.knowledge_provider = Some(provider);
        }

        Ok(AgentResponse::SwapComplete)
    }

    /// Replace the agent's MCP registry with a fresh snapshot and reload MCP
    /// servers. See [`AgentHandle::refresh_mcp_registry`] for caller-side docs.
    async fn handle_refresh_mcp_registry(
        &mut self,
        registry: Box<dyn mcp::McpRegistry>,
    ) -> Result<AgentResponse, AgentError> {
        // Refresh is only safe when the agent is idle: it tears down MCP servers
        // mid-flight, which would corrupt an in-progress turn. Callers (e.g.
        // AcpSession) defer the call until the next idle window.
        if !matches!(self.active_state(), ActiveState::Idle) {
            return Err(AgentError::NotIdle);
        }

        // Skip the work entirely when MCP is governance-disabled. No registry
        // can resurrect MCP servers in that case.
        if !self.settings.mcp_enabled {
            self.mcp_registry = Some(registry);
            return Ok(AgentResponse::Success);
        }

        // 1. Store the new registry so subsequent swaps re-apply it.
        self.mcp_registry = Some(registry);

        // 2. Re-apply registry transformations to the current agent config.
        if let Some(registry) = self.mcp_registry.as_ref() {
            registry.apply(&mut self.agent_config);
        }

        // 3. Tear down existing MCP servers and spin up a fresh manager.
        self.mcp_manager_handle.terminate();
        self.mcp_manager_handle = McpManager::default().spawn();

        self.reauth_shadows.clear();

        // 4. Invalidate cached tool specs so the next prompt picks up the new server set.
        self.cached_tool_specs = None;

        // 5. Reload MCP configs from the now-transformed agent config and launch the new server set.
        self.cached_mcp_configs = LoadedMcpServerConfigs::from_agent_config(
            &self.agent_config,
            self.local_mcp_path.as_ref(),
            self.global_mcp_path.as_ref(),
        )
        .await;
        self.launch_mcp_servers().await;

        Ok(AgentResponse::Success)
    }

    /// Swap in a freshly-loaded agent config and surgically reconcile MCP
    /// servers. See [`AgentHandle::reconcile_mcp_servers`] for caller-side docs.
    ///
    /// This is the event-driven, low-churn counterpart to a full swap: it
    /// computes the minimal launch/stop/restart plan against the currently
    /// applied configs and leaves unchanged servers running.
    async fn handle_reconcile_mcp_servers(
        &mut self,
        mut config: LoadedAgentConfig,
    ) -> Result<AgentResponse, AgentError> {
        // Idle-only: reconcile may stop/restart servers, which would corrupt an
        // in-progress turn. Callers defer until the next idle window.
        if !matches!(self.active_state(), ActiveState::Idle) {
            return Err(AgentError::NotIdle);
        }

        // Honour MCP governance: a governance-disabled session runs no servers.
        if !self.settings.mcp_enabled {
            config.config_mut().clear_mcp_configs();
        }

        // Keep registry-driven resolution consistent across the swap, exactly
        // as swap_agent / refresh_mcp_registry do.
        if let Some(registry) = self.mcp_registry.as_ref() {
            registry.apply(&mut config);
        }

        // Build the new desired MCP set from the fresh config (+ legacy mcp.json).
        let new_mcp_configs = LoadedMcpServerConfigs::from_agent_config(
            &config,
            self.local_mcp_path.as_ref(),
            self.global_mcp_path.as_ref(),
        )
        .await;

        // Diff enabled servers: current (applied) vs desired (new). Only enabled
        // servers should be running, so disabled entries are filtered from both —
        // a server toggled to disabled drops out of desired and gets stopped.
        let to_map = |loaded: &LoadedMcpServerConfigs| {
            loaded
                .configs
                .iter()
                .filter(|c| c.is_enabled())
                .map(|c| (c.server_name.clone(), c.config.clone()))
                .collect::<HashMap<String, _>>()
        };
        let current = to_map(&self.cached_mcp_configs);
        let desired = to_map(&new_mcp_configs);
        let plan = mcp::reconcile::reconcile_mcp(&current, &desired);

        // Apply surgically. Stops first, then restarts (stop + relaunch), then
        // launches. Unchanged servers are never touched. Launch init proceeds in
        // the background; the manager promotes servers on the Initialized event,
        // so we drop the returned receivers here.
        for name in &plan.stop {
            if let Err(e) = self.mcp_manager_handle.stop_server(name.clone()).await {
                warn!(server_name = %name, error = %e, "failed to stop MCP server during reconcile");
            }
        }
        for (name, cfg) in &plan.restart {
            if let Err(e) = self.mcp_manager_handle.stop_server(name.clone()).await {
                warn!(server_name = %name, error = %e, "failed to stop MCP server during reconcile restart");
            }
            if let Err(e) = self.mcp_manager_handle.launch_server(name.clone(), cfg.clone()).await {
                warn!(server_name = %name, error = %e, "failed to relaunch MCP server during reconcile");
            }
        }
        for (name, cfg) in &plan.launch {
            if let Err(e) = self.mcp_manager_handle.launch_server(name.clone(), cfg.clone()).await {
                warn!(server_name = %name, error = %e, "failed to launch MCP server during reconcile");
            }
        }

        // Drop forced-auth shadows whose target is being stopped/restarted here (the
        // shadow runs under a hidden name not in the plan, so stop it explicitly).
        if !self.reauth_shadows.is_empty() {
            let affected: std::collections::HashSet<&str> = plan
                .stop
                .iter()
                .map(String::as_str)
                .chain(plan.restart.iter().map(|(n, _)| n.as_str()))
                .collect();
            let stale: Vec<(String, String)> = self
                .reauth_shadows
                .iter()
                .filter(|(_, target)| affected.contains(target.as_str()))
                .map(|(s, t)| (s.clone(), t.clone()))
                .collect();
            for (shadow_name, target) in stale {
                // The loaded-case shadow runs under a hidden name not in the plan,
                // so stop it explicitly. A not-loaded shadow shares the target name
                // and was already stopped by the plan above.
                if shadow_name != target
                    && let Err(e) = self.mcp_manager_handle.shutdown_server(shadow_name.clone()).await
                {
                    warn!(%shadow_name, target, error = %e, "failed to drop reauth shadow during reconcile");
                }
                self.reauth_shadows.remove(&shadow_name);
            }
        }

        // Adopt the new config. Only invalidate the tool-spec and resource
        // caches when the plan actually changed something — a no-op reconcile
        // (e.g. an unrelated mcp.json touch) must not drop resource
        // subscriptions or force a tool-spec rebuild.
        self.agent_config = config;
        self.cached_mcp_configs = new_mcp_configs;
        if !plan.is_empty() {
            self.cached_tool_specs = None;
            self.session_resource_paths.clear();
        }

        Ok(AgentResponse::Success)
    }

    /// Compute the shadow server name for a target server undergoing forced auth.
    fn reauth_shadow_name(target: &str) -> String {
        format!("{}{target}", Self::REAUTH_SHADOW_PREFIX)
    }

    /// Force (re-)authentication for a single remote MCP server.
    ///
    /// See [`AgentHandle::reauth_mcp_server`] for caller-side docs.
    ///
    /// To avoid dropping the user's existing access on a misfired reauth, a
    /// currently-loaded server is **not** torn down. Instead a hidden "shadow"
    /// server (named via [`Self::reauth_shadow_name`]) is launched with forced auth
    /// alongside the original; only once the shadow initializes is it promoted to
    /// replace the original (see [`Self::handle_mcp_events`]). If the server isn't
    /// currently loaded, no shadow is needed and it is relaunched directly with
    /// forced auth.
    ///
    /// The launch is fire-and-forget: the (interactive) OAuth flow runs inside the
    /// server actor, off the agent's main loop, so the agent stays responsive to a
    /// subsequent abort. Progress/outcome is surfaced via MCP server events.
    async fn handle_reauth_mcp_server(&mut self, server_name: String) -> Result<AgentResponse, AgentError> {
        if !matches!(self.active_state(), ActiveState::Idle) {
            return Err(AgentError::NotIdle);
        }
        if !self.settings.mcp_enabled {
            return Err(AgentError::Custom("MCP is disabled".to_string()));
        }

        // Concurrency guard: only one in-flight forced auth per target server.
        if self.reauth_shadows.values().any(|target| target == &server_name) {
            return Err(AgentError::Custom(format!(
                "MCP server '{server_name}' is already authenticating"
            )));
        }

        // Validate the server exists and is remote, building a one-off config with
        // forced auth. `force_auth` is intentionally NOT persisted on the cached
        // config — it applies to this launch only, so a later reconcile/relaunch
        // won't force auth again.
        let forced_config = self.forced_auth_config(&server_name)?;

        // A running server gets a shadow so its tools stay available during the
        // (interactive) flow; a not-loaded server is relaunched under its own name.
        let is_loaded = self
            .mcp_manager_handle
            .get_tool_specs(server_name.clone())
            .await
            .is_ok();

        let launch_name = if is_loaded {
            Self::reauth_shadow_name(&server_name)
        } else {
            server_name.clone()
        };

        // Clear any stale instance occupying the launch name so the launch can't
        // collide. For the loaded case this only touches the (hidden) shadow name,
        // never the original; for the not-loaded case it tears down a stale
        // failed/initializing instance under the real name.
        let _ = self.mcp_manager_handle.shutdown_server(launch_name.clone()).await;

        match self
            .mcp_manager_handle
            .launch_server(launch_name.clone(), forced_config)
            .await
        {
            Ok(_rx) => {
                self.reauth_shadows.insert(launch_name, server_name);
                Ok(AgentResponse::Success)
            },
            Err(e) => Err(AgentError::Custom(format!(
                "failed to start authentication for MCP server '{server_name}': {e}"
            ))),
        }
    }

    /// Abort a pending/forced authentication for a single remote MCP server.
    ///
    /// See [`AgentHandle::abort_mcp_server_auth`] for caller-side docs. If the
    /// server is being re-authenticated via a shadow (the loaded case), the only
    /// action is to **drop the shadow** — the original keeps running untouched, so
    /// the user retains the server's capabilities. If the forced auth was running
    /// directly on a not-loaded server, that server is relaunched under the normal
    /// (non-forced) flow. Aborting when nothing is in flight is a no-op success so
    /// the UI's cancel action is idempotent.
    async fn handle_abort_mcp_server_auth(&mut self, server_name: String) -> Result<AgentResponse, AgentError> {
        if !matches!(self.active_state(), ActiveState::Idle) {
            return Err(AgentError::NotIdle);
        }
        if !self.settings.mcp_enabled {
            return Err(AgentError::Custom("MCP is disabled".to_string()));
        }

        let shadow = self
            .reauth_shadows
            .iter()
            .find(|(_, target)| *target == &server_name)
            .map(|(shadow, _)| shadow.clone());

        match shadow {
            // Loaded case: drop the shadow only; the original is untouched.
            Some(shadow_name) if shadow_name != server_name => {
                self.reauth_shadows.remove(&shadow_name);
                if let Err(e) = self.mcp_manager_handle.shutdown_server(shadow_name.clone()).await {
                    warn!(server_name, %shadow_name, error = %e, "failed to drop reauth shadow on abort");
                }
                // The original is still running — refresh it in the UI so any
                // pending-OAuth state shown during the attempt is cleared.
                self.refresh_mcp_server_in_ui(&server_name);
                Ok(AgentResponse::Success)
            },
            // Not-loaded case (shadow == target): forced auth ran on the real
            // server. Drop tracking and relaunch under the normal flow so the user
            // keeps any unauthenticated capabilities.
            Some(_) => {
                self.reauth_shadows.remove(&server_name);
                self.reload_mcp_server_normally(&server_name).await;
                Ok(AgentResponse::Success)
            },
            None => Ok(AgentResponse::Success),
        }
    }

    /// Build a one-off config clone for `server_name` with forced auth enabled.
    ///
    /// Errors if the server is unknown or not a remote (HTTP) server. Does not
    /// mutate the cached config — forced auth is a one-shot for the next launch.
    fn forced_auth_config(&self, server_name: &str) -> Result<agent_config::definitions::McpServerConfig, AgentError> {
        let Some(loaded) = self
            .cached_mcp_configs
            .configs
            .iter()
            .find(|c| c.server_name == server_name)
        else {
            return Err(AgentError::Custom(format!("No MCP server named '{server_name}'")));
        };

        match &loaded.config {
            agent_config::definitions::McpServerConfig::Remote(remote) => {
                let mut remote = remote.clone();
                remote.force_auth = true;
                Ok(agent_config::definitions::McpServerConfig::Remote(remote))
            },
            _ => Err(AgentError::Custom(format!(
                "MCP server '{server_name}' is not a remote (HTTP) server; forced auth is only supported for remote servers"
            ))),
        }
    }

    /// Push a synthetic MCP `Initialized` event for `server_name` so the UI
    /// refreshes a server whose underlying state is unchanged. Used after dropping
    /// a reauth shadow (abort or shadow failure) to clear the pending-OAuth state
    /// that was shown on the still-running original during the attempt.
    fn refresh_mcp_server_in_ui(&mut self, server_name: &str) {
        self.agent_event_buf.push(AgentEvent::Mcp(McpServerEvent::Initialized {
            server_name: server_name.to_string(),
            serve_duration: std::time::Duration::ZERO,
            list_tools_duration: None,
            list_prompts_duration: None,
        }));
    }

    /// Relaunch a remote MCP server under the normal (non-forced) flow.
    ///
    /// Used when a forced authentication on a not-currently-loaded server fails or
    /// is aborted, so the user keeps the unauthenticated capabilities the server
    /// offers. Because forced auth is never persisted on the cached config, this
    /// tears down the failed actor and relaunches straight from the cached config.
    async fn reload_mcp_server_normally(&mut self, server_name: &str) {
        let Some(config) = self
            .cached_mcp_configs
            .configs
            .iter()
            .find(|c| c.server_name == server_name)
            .map(|c| c.config.clone())
        else {
            return;
        };

        if let Err(e) = self.mcp_manager_handle.shutdown_server(server_name.to_string()).await {
            warn!(server_name, error = %e, "failed to shut down server before normal-flow reload");
        }
        self.cached_tool_specs = None;
        if let Err(e) = self
            .mcp_manager_handle
            .launch_server(server_name.to_string(), config)
            .await
        {
            error!(server_name, error = %e, "failed to relaunch server under normal flow");
        }
    }

    /// Remove the persisted OAuth credentials for a single remote MCP server.
    ///
    /// See [`AgentHandle::remove_mcp_server_credentials`] for caller-side docs.
    /// Deletes the cached token and dynamic client registration files. Does not
    /// stop or relaunch the server — a running server keeps its in-memory session;
    /// the removal takes effect on the next launch.
    async fn handle_remove_mcp_server_credentials(&mut self, server_name: String) -> Result<AgentResponse, AgentError> {
        if !self.settings.mcp_enabled {
            return Err(AgentError::Custom("MCP is disabled".to_string()));
        }

        // Look up the server's URL — only remote (HTTP) servers have OAuth credentials.
        let url = match self
            .cached_mcp_configs
            .configs
            .iter()
            .find(|c| c.server_name == server_name)
            .map(|c| &c.config)
        {
            Some(agent_config::definitions::McpServerConfig::Remote(remote)) => remote.url.clone(),
            Some(_) => {
                return Err(AgentError::Custom(format!(
                    "MCP server '{server_name}' is not a remote (HTTP) server; it has no persisted credentials"
                )));
            },
            None => {
                return Err(AgentError::Custom(format!("No MCP server named '{server_name}'")));
            },
        };

        self.mcp_manager_handle
            .remove_server_credentials(server_name.clone(), url)
            .await
            .map_err(|e| AgentError::Custom(format!("failed to remove credentials for '{server_name}': {e}")))?;

        Ok(AgentResponse::Success)
    }

    async fn handle_cancel_request(&mut self) -> Result<AgentResponse, AgentError> {
        match self.active_state() {
            ActiveState::Idle
            | ActiveState::Errored(_)
            | ActiveState::ExecutingRequest { .. }
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
        if (self.end_current_turn(false).await?).is_some() {
            match self.active_state() {
                ActiveState::WaitingForApproval(_)
                | ActiveState::ExecutingHooks(_)
                | ActiveState::ExecutingRequest { .. }
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

        // Clear any queued steering message on cancel and notify the TUI.
        // The TUI captures the queued content locally before issuing cancel
        // and replays it as a fresh prompt after cancel resolves ("cancel =
        // redirect" UX). The backend clear ensures a subsequent turn doesn't
        // accidentally inherit stale steering content.
        if !self.queued_steers.is_empty() {
            let message_ids = self.queued_steers.drain(..).map(|s| s.id).collect();
            self.agent_event_buf.push(AgentEvent::SteeringCleared { message_ids });
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
        approval_state.rejection_reason = args.result.reason.clone();

        // Wait until every queued approval is answered before acting on the batch,
        // so rejecting one prompt doesn't continue the turn while others are pending.
        if state.needs_approval.values().any(|s| s.selected.is_none()) {
            return Ok(AgentResponse::Success);
        }

        // Each tool's disposition is self-contained: reject one, still execute
        // the approved siblings. `pre_built_*` starts with parse-error results
        // synthesized before approval so the model gets a tool_result for every
        // tool_use it emitted (else enforce_conversation_invariants back-fills a
        // false "cancelled by the user" result).
        let mut pre_built_content = state.pre_built_content.clone();
        let mut pre_built_results = state.pre_built_results.clone();
        let mut approved_tools: Vec<(ToolUseBlock, Tool)> = Vec::new();
        for (block, tool) in &state.tools {
            let tool_use_id = &block.tool_use_id;
            // Tools absent from `needs_approval` were auto-allowed (e.g. a
            // trusted fs_read riding alongside an Ask tool); execute them.
            let Some(approval_state) = state.needs_approval.get(tool_use_id) else {
                approved_tools.push((block.clone(), tool.clone()));
                continue;
            };
            if approval_state.selected.as_ref().is_some_and(|id| id.is_allow()) {
                approved_tools.push((block.clone(), tool.clone()));
                continue;
            }
            // Denial result for the model + a ToolCallFailed UI event so the
            // client renders it denied now, not still-executing until turn end.
            let reason = approval_state
                .rejection_reason
                .clone()
                .unwrap_or_else(|| "Tool use was denied by the user.".to_string());
            pre_built_content.push(ContentBlock::ToolResult(ToolResultBlock {
                tool_use_id: tool_use_id.clone(),
                content: vec![ToolResultContentBlock::Text(reason.clone())],
                status: ToolResultStatus::Error,
            }));
            pre_built_results.insert(tool_use_id.clone(), LogToolResult {
                tool: Some(Box::new(tool.clone())),
                result: ToolCallResult::Error(ToolExecutionError::Custom(reason.clone())),
            });
            self.agent_event_buf
                .push(AgentEvent::Update(UpdateEvent::ToolCallFailed {
                    tool_use_id: tool_use_id.clone(),
                    tool_name: block.name.clone(),
                    tool_identity: Some(ToolCallIdentity::from_tool(tool)),
                    raw_input: block.input.clone(),
                    reason: ToolCallFailureReason::PermissionDenied,
                    error: reason,
                }));
        }

        if approved_tools.is_empty() {
            // Nothing to execute — send the denials (draining queued steering)
            // directly as the follow-up request.
            if !self.queued_steers.is_empty() {
                let steers = std::mem::take(&mut self.queued_steers);
                let snapshot = steer_snapshot(&steers);
                pre_built_content.push(ContentBlock::Text(format_steering_message(&snapshot)));
                for steer in steers {
                    self.agent_event_buf.push(AgentEvent::SteeringConsumed {
                        message_id: steer.id,
                        content: steer.text,
                    });
                }
            }
            let pending = PendingUserMessage::new_tool_results(pre_built_content, pre_built_results);
            let args = self.format_request(&pending).await;
            self.send_request(args).await?;
            self.set_active_state(ActiveState::ExecutingRequest {
                compaction_retry: None,
                empty_response_retried: false,
                pending_user_message: Some(pending),
            })
            .await;
            return Ok(AgentResponse::Success);
        }

        // Execute the approved tools; send_tool_results merges the denial
        // results (pre_built_*) with the executed ones and drains steering.
        self.execute_tools(approved_tools, pre_built_content, pre_built_results)
            .await?;

        Ok(AgentResponse::Success)
    }

    async fn handle_agent_loop_event(&mut self, evt: Option<AgentLoopEventKind>) -> Result<(), AgentError> {
        debug!(?evt, "handling new agent loop event");
        let loop_id = self.agent_loop_handle()?.id().clone();

        // If the event is None, then the channel has dropped, meaning the agent loop has exited.
        // Emit a Stop event so the ACP prompt response is always resolved
        let Some(evt) = evt else {
            warn!("agent loop channel dropped without EndTurn, emitting Stop event");
            self.agent_loop = None;
            self.set_active_state(ActiveState::Idle).await;
            self.agent_event_buf.push(AgentEvent::Stop(AgentStopReason::EndTurn));
            return Ok(());
        };

        self.agent_event_buf
            .push(AgentLoopEvent::new(loop_id.clone(), evt.clone()).into());

        match evt {
            AgentLoopEventKind::ResponseStreamEnd { result, metadata } => match result {
                Ok(msg) => {
                    // Append pending user message now that we have a successful response
                    if let ActiveState::ExecutingRequest {
                        pending_user_message: Some(pending),
                        ..
                    } = &self.execution_state.active_state
                    {
                        match pending.clone() {
                            PendingUserMessage::Prompt { id, content, meta } => {
                                self.append_user_message(id, content, meta);
                            },
                            PendingUserMessage::ToolResults { id, content, results } => {
                                self.append_tool_results(id, content, results);
                            },
                        }
                    }
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
                    let assistant_response = md.result.as_ref().and_then(|r| r.as_ref().ok()).map(|msg| msg.text());
                    let hooks = hooks
                        .into_iter()
                        .enumerate()
                        .map(|(index, hook)| {
                            (
                                HookExecutionId {
                                    hook,
                                    tool_context: None,
                                    index,
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
                        assistant_response,
                    )
                    .await;
                    return Ok(());
                }

                // Drain queued steering message at end-of-turn, or emit end-of-turn
                // events if the queue is empty.
                self.drain_steering_or_end_turn(md).await?;
            },
            AgentLoopEventKind::AssistantText(text) => self
                .agent_event_buf
                .push(AgentEvent::Update(UpdateEvent::AgentContent(text.into()))),
            AgentLoopEventKind::ReasoningContent(text) => self
                .agent_event_buf
                .push(AgentEvent::Update(UpdateEvent::AgentThought(text.into()))),
            AgentLoopEventKind::ThinkingText(text) => self
                .agent_event_buf
                .push(AgentEvent::Update(UpdateEvent::AgentThought(text.into()))),
            _ => (),
        }

        Ok(())
    }

    /// Handler for errors encountered while sending the request or while consuming the response.
    async fn handle_loop_error_on_stream_end(&mut self, err: &LoopError) -> Result<(), AgentError> {
        debug_assert!(matches!(self.active_state(), ActiveState::ExecutingRequest { .. }));
        debug_assert!(self.agent_loop.is_some());

        match err {
            LoopError::InvalidJson {
                assistant_text,
                invalid_tools,
                valid_tools,
            } => {
                // Historically, we've found the model to produce invalid JSON when
                // handling a complicated tool use - often times, the stream just ends
                // as if everything is ok while in the middle of returning the tool use
                // content.
                //
                // In this case, retry the request, except tell the model to split up
                // the work into simpler tool uses.

                // Create a fake assistant message with ALL tool uses (valid + invalid)
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

                // Add completed tool uses as-is
                for tool in valid_tools {
                    assistant_content.push(ContentBlock::ToolUse(tool.clone()));
                }

                // Add invalid tool uses with placeholder args
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
                // Append the original pending user message since we got a (partial) response
                if let ActiveState::ExecutingRequest {
                    pending_user_message: Some(pending),
                    ..
                } = &self.execution_state.active_state
                {
                    match pending.clone() {
                        PendingUserMessage::Prompt { id, content, meta } => self.append_user_message(id, content, meta),
                        PendingUserMessage::ToolResults { id, content, results } => {
                            self.append_tool_results(id, content, results);
                        },
                    }
                }

                self.append_assistant_message(Message::new(
                    // synthetic id; message only sent as history, not as the active prompt of a request
                    Uuid::new_v4().to_string(),
                    Role::Assistant,
                    assistant_content,
                    Some(Utc::now()),
                ));

                let error_msg = "The generated tool was too large, try again but this time split up the work between multiple tool uses";

                if valid_tools.is_empty() {
                    // No valid tool uses — send a simple text retry prompt (original behavior)
                    let retry_pending =
                        PendingUserMessage::new_prompt(vec![ContentBlock::Text(error_msg.to_string())], None);

                    let args = self.format_request(&retry_pending).await;
                    self.execution_state.active_state = ActiveState::ExecutingRequest {
                        compaction_retry: None,
                        empty_response_retried: false,
                        pending_user_message: Some(retry_pending),
                    };
                    self.send_request(args).await?;
                } else {
                    // Valid tool uses exist — send tool results for all tool uses to maintain
                    // the conversation invariant (every ToolUse must have a ToolResult)
                    let mut content = Vec::new();

                    for tool in valid_tools {
                        content.push(ContentBlock::ToolResult(ToolResultBlock {
                            tool_use_id: tool.tool_use_id.clone(),
                            content: vec![ToolResultContentBlock::Text(
                                "Tool use was not executed because another tool use in the same response had invalid JSON".to_string(),
                            )],
                            status: ToolResultStatus::Error,
                        }));
                    }
                    for tool in invalid_tools {
                        content.push(ContentBlock::ToolResult(ToolResultBlock {
                            tool_use_id: tool.tool_use_id.clone(),
                            content: vec![ToolResultContentBlock::Text(error_msg.to_string())],
                            status: ToolResultStatus::Error,
                        }));
                    }
                    content.push(ContentBlock::Text(error_msg.to_string()));

                    let retry_pending = PendingUserMessage::new_tool_results(content.clone(), HashMap::new());

                    let args = self.format_request(&retry_pending).await;
                    self.execution_state.active_state = ActiveState::ExecutingRequest {
                        compaction_retry: None,
                        empty_response_retried: false,
                        pending_user_message: Some(retry_pending),
                    };
                    self.send_request(args).await?;
                }
            },
            LoopError::EmptyResponse => {
                // The model returned a clean stream with no content. Retry the same request
                // once. If the retry also returns empty, surface the error to the user
                // instead of looping.
                let already_retried = matches!(&self.execution_state.active_state, ActiveState::ExecutingRequest {
                    empty_response_retried: true,
                    ..
                },);
                trace!(already_retried, "handling LoopError::EmptyResponse in agent loop");
                if already_retried {
                    warn!("empty response on retry - entering error state");
                    self.enter_error_state(err.clone().into()).await;
                } else {
                    let pending = match &self.execution_state.active_state {
                        ActiveState::ExecutingRequest {
                            pending_user_message: Some(p),
                            ..
                        } => p.clone(),
                        _ => {
                            error!("empty response with no pending user message - entering error state");
                            self.enter_error_state(err.clone().into()).await;
                            return Ok(());
                        },
                    };
                    warn!("empty response from model - retrying once with the same request");
                    let args = self.format_request(&pending).await;
                    self.execution_state.active_state = ActiveState::ExecutingRequest {
                        compaction_retry: None,
                        empty_response_retried: true,
                        pending_user_message: Some(pending),
                    };
                    self.send_request(args).await?;
                }
            },
            LoopError::Stream(stream_err) => match &stream_err.kind {
                StreamErrorKind::StreamTimeout { .. } => {
                    // Append the original pending user message since we got a (partial) response
                    if let ActiveState::ExecutingRequest {
                        pending_user_message: Some(pending),
                        ..
                    } = &self.execution_state.active_state
                    {
                        match pending.clone() {
                            PendingUserMessage::Prompt { id, content, meta } => {
                                self.append_user_message(id, content, meta);
                            },
                            PendingUserMessage::ToolResults { id, content, results } => {
                                self.append_tool_results(id, content, results);
                            },
                        }
                    }

                    self.append_assistant_message(Message::new(
                        // synthetic id; message only sent as history, not as the active prompt of a request
                        Uuid::new_v4().to_string(),
                        Role::Assistant,
                        vec![ContentBlock::Text(
                            "Response timed out - message took too long to generate".to_string(),
                        )],
                        Some(Utc::now()),
                    ));

                    // Set new pending for the retry prompt
                    let retry_pending = PendingUserMessage::new_prompt(
                        vec![ContentBlock::Text(
                            "You took too long to respond - try to split up the work into smaller steps.".to_string(),
                        )],
                        None,
                    );

                    let args = self.format_request(&retry_pending).await;
                    self.execution_state.active_state = ActiveState::ExecutingRequest {
                        compaction_retry: None,
                        empty_response_retried: false,
                        pending_user_message: Some(retry_pending),
                    };
                    self.send_request(args).await?;
                },
                StreamErrorKind::Interrupted => {
                    // nothing to do
                },
                StreamErrorKind::ContextWindowOverflow if !self.settings.disable_auto_compact => {
                    // Check if this is a retry after compaction
                    let compaction_retry = match self.active_state() {
                        ActiveState::ExecutingRequest {
                            compaction_retry: Some(r),
                            ..
                        } => Some(*r),
                        _ => None,
                    };
                    info!("is compaction retry: {:?}", compaction_retry);

                    if let Some(retry) = compaction_retry {
                        if retry.is_prompt_truncated {
                            error!("compaction retry failed after truncation, going to error state");
                            self.enter_error_state(err.clone().into()).await;
                        } else {
                            // Compaction succeeded but retry overflowed - truncate and retry
                            warn!("compaction succeeded, but the retry overflowed - attempting again with truncation");

                            // Get the current pending message and truncate it
                            let truncated_pending = match self.active_state() {
                                ActiveState::ExecutingRequest {
                                    pending_user_message: Some(pending),
                                    ..
                                } => {
                                    // The id passed here is unused: we only use this Message
                                    // to call truncate() on the content. We then extract msg.content
                                    // and msg.meta into a new PendingUserMessage that gets its own id.
                                    let mut msg = Message::new(
                                        Uuid::new_v4().to_string(),
                                        Role::User,
                                        pending.content().to_vec(),
                                        None,
                                    );
                                    msg.truncate(compact::DEFAULT_MAX_MESSAGE_LEN, Some("...truncated due to length"));
                                    PendingUserMessage::new_prompt(msg.content, msg.meta)
                                },
                                _ => {
                                    error!("expected ExecutingRequest with pending message");
                                    return Ok(());
                                },
                            };

                            let pending_request = self.format_request(&truncated_pending).await;
                            self.set_active_state(ActiveState::ExecutingRequest {
                                compaction_retry: Some(CompactionRetry {
                                    is_prompt_truncated: true,
                                }),
                                empty_response_retried: false,
                                pending_user_message: Some(truncated_pending),
                            })
                            .await;
                            self.send_request(pending_request).await?;
                        }
                    } else {
                        self.start_compaction(CompactStrategy::default_strategy()).await?;
                    }
                },
                StreamErrorKind::Validation { .. }
                | StreamErrorKind::ServiceFailure
                | StreamErrorKind::ContextWindowOverflow
                | StreamErrorKind::Throttling
                | StreamErrorKind::ModelOverloaded { .. }
                | StreamErrorKind::MonthlyLimitReached { .. }
                | StreamErrorKind::InvalidModelId { .. }
                | StreamErrorKind::Other { .. } => {
                    self.enter_error_state(err.clone().into()).await;
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
                    self.end_current_turn(false).await?;
                }
            },
            ActiveState::WaitingForApproval { .. } => (),
            ActiveState::ExecutingRequest { .. }
            | ActiveState::ExecutingHooks(_)
            | ActiveState::ExecutingTools { .. }
            | ActiveState::Compacting { .. } => {
                return Err(AgentError::NotIdle);
            },
        }

        // A fresh user prompt starts a new logical turn — reset the
        // unavailable-tool breaker so prior dummy/parse-error turns don't carry
        // over and prematurely trip it.
        self.consecutive_unexecutable_tool_turns = 0;

        // Run per-prompt hooks, if required.
        let hooks = self.get_hooks(HookTrigger::UserPromptSubmit);
        if !hooks.is_empty() {
            let hooks = hooks
                .into_iter()
                .enumerate()
                .map(|(index, hook)| {
                    (
                        HookExecutionId {
                            hook,
                            tool_context: None,
                            index,
                        },
                        None,
                    )
                })
                .collect();
            let prompt = args.text();
            self.start_hooks_execution(hooks, HookStage::PrePrompt { args }, prompt, None)
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
        let user_msg_content = args
            .content
            .into_iter()
            .map(|c| match c {
                ContentChunk::Text(t) => ContentBlock::Text(t),
                ContentChunk::Image(img) => ContentBlock::Image(img),
                ContentChunk::ResourceLink(json) => ContentBlock::Text(json),
            })
            .collect::<Vec<_>>();

        // Build metadata with timestamp and per-prompt hook context.
        let additional_context = if prompt_hooks.is_empty() {
            String::new()
        } else {
            let mut ctx = String::new();
            ctx.push_str(CONTEXT_ENTRY_START_HEADER);
            ctx.push_str("This section (like others) contains important information that I want you to use in your responses. I have gathered this context from valuable programmatic script hooks. You must follow any requests and consider all of the information in this section\n\n");
            for hook in &prompt_hooks {
                ctx.push_str(&format!("{hook}\n\n"));
            }
            ctx.push_str(CONTEXT_ENTRY_END_HEADER);
            ctx
        };
        let meta = Some(MessageMetadata {
            timestamp: Some(Utc::now()),
            additional_context,
        });

        let pending = PendingUserMessage::new_prompt(user_msg_content, meta);

        // Create a new agent loop, and send the request.
        let loop_id = AgentLoopId::new(self.id.clone());
        let cancel_token = CancellationToken::new();
        self.agent_loop = Some(AgentLoop::new(loop_id.clone(), cancel_token).spawn());
        let args = self.format_request(&pending).await;
        self.send_request(args)
            .await
            .expect("first agent loop request should never fail");
        self.set_active_state(ActiveState::ExecutingRequest {
            compaction_retry: None,
            empty_response_retried: false,
            pending_user_message: Some(pending),
        })
        .await;
        Ok(AgentResponse::Success)
    }

    /// Creates a [SendRequestArgs] used for sending requests to the backend based on the current
    /// conversation state.
    ///
    /// The returned conversation history will:
    /// 1. Have context messages prepended to the start of the message history
    /// 2. Have conversation history invariants enforced, mutating messages as required
    async fn format_request(&mut self, pending: &PendingUserMessage) -> SendRequestArgs {
        let latest_summary = self.conversation_state.event_log().latest_summary().map(String::from);
        let mut messages = VecDeque::from(self.conversation_state.messages().to_vec());
        let mut user_msg = Message::new(pending.id().to_string(), Role::User, pending.content().to_vec(), None);
        // Preserve metadata from the pending message (e.g. per-prompt hook context).
        if let Some(meta) = pending.meta() {
            user_msg.meta = Some(meta.clone());
        }
        messages.push_back(user_msg);

        let task_context = self.task_store.as_ref().and_then(|s| s.format_context().ok().flatten());
        let knowledge_context = match &self.knowledge_provider {
            Some(provider) => provider.list_available().await,
            None => None,
        };

        // Build tool specs first — this rebuilds tool_index, so the deferred
        // tools list below reflects the current MCP tools on every request.
        let tool_specs = self.make_tool_spec().await;

        let deferred_tools_list = if self.tool_search_active {
            let list = self.tool_search_index.format_tool_list();
            if list.is_empty() { None } else { Some(list) }
        } else {
            None
        };

        let model_name = self.model.display_name().filter(|s| !s.is_empty());
        format_request(
            messages,
            tool_specs,
            &self.agent_config,
            self.agent_spawn_hooks.iter().map(|(_, c)| c),
            &self.sys_provider,
            latest_summary,
            task_context,
            knowledge_context,
            self.model.context_window_size(),
            self.tool_search_active,
            deferred_tools_list,
            model_name.as_deref(),
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

        // Preserve pending_user_message from ExecutingRequest state
        let pending_user_message = match &self.execution_state.active_state {
            ActiveState::ExecutingRequest {
                pending_user_message, ..
            } => pending_user_message.clone(),
            ActiveState::Compacting {
                pending_user_message, ..
            } => pending_user_message.clone(),
            _ => None,
        };

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
        self.set_active_state(ActiveState::Compacting {
            strategy,
            pending_user_message,
        })
        .await;
        Ok(())
    }

    /// Handles events from the compaction agent loop.
    async fn handle_compaction_loop_event(&mut self, evt: Option<AgentLoopEventKind>) -> Result<(), AgentError> {
        debug!(?evt, "handling compaction loop event");

        let Some(evt) = evt else {
            self.compaction_loop = None;
            return Ok(());
        };

        let ActiveState::Compacting {
            strategy,
            pending_user_message,
        } = self.execution_state.active_state.clone()
        else {
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

                    // Retry if we have a pending user message, otherwise go idle
                    if let Some(pending) = pending_user_message {
                        debug!("have pending user message, retrying the request");
                        let pending_request = self.format_request(&pending).await;
                        self.set_active_state(ActiveState::ExecutingRequest {
                            compaction_retry: Some(CompactionRetry::default()),
                            empty_response_retried: false,
                            pending_user_message: Some(pending),
                        })
                        .await;
                        self.send_request(pending_request).await?;
                    } else {
                        debug!("no pending user message, going to idle state");
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
                        self.enter_error_state(err.into()).await;
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
        debug_assert!(matches!(self.active_state(), ActiveState::ExecutingRequest { .. }));

        // First, parse tool uses.
        let (tools, errors, dummy_tool_uses) = self.parse_tools(tool_uses).await;

        // Parse errors don't short-circuit the rest of the batch. When the
        // model dispatches multiple tools in parallel and one fails parse-
        // time validation (e.g. an fs_read tool with a path that doesn't
        // exist), the previous behavior dropped every parsed-OK sibling on
        // the floor — never executed, never sent a tool_result. The model's
        // tool_use → tool_result invariant was then patched up by
        // enforce_conversation_invariants synthesizing fake "Tool use was
        // cancelled by the user" results, which falsely told the model the
        // user had interrupted siblings the user never touched. Now we hold
        // the parse-error results aside and continue down the normal path
        // with the parsed-OK tools; send_tool_results merges the held
        // results into the eventual outbound batch so every tool_use the
        // model emitted gets a real, accurate tool_result paired with it.
        let mut pre_built_content: Vec<ContentBlock> = Vec::new();
        let mut pre_built_results: HashMap<String, LogToolResult> = HashMap::new();

        // Resolve any `dummy` placeholder calls to a benign instructional
        // tool_result. The model gets actionable guidance (e.g. "call
        // switch_to_execution") instead of a hard NameDoesNotExist error, so it
        // can self-correct rather than re-calling the unavailable tool.
        for tool_use in &dummy_tool_uses {
            let tool_use_id = tool_use.tool_use_id.clone();
            pre_built_content.push(ContentBlock::ToolResult(ToolResultBlock {
                tool_use_id: tool_use_id.clone(),
                content: vec![ToolResultContentBlock::Text(DUMMY_TOOL_RESULT_MESSAGE.to_string())],
                status: ToolResultStatus::Success,
            }));
            pre_built_results.insert(tool_use_id, LogToolResult {
                tool: None,
                result: ToolCallResult::Success(ToolExecutionOutput::new(vec![ToolExecutionOutputItem::Text(
                    DUMMY_TOOL_RESULT_MESSAGE.to_string(),
                )])),
            });
        }

        if !errors.is_empty() {
            trace!(?errors, "failed to parse tools");
            for e in errors {
                let tool_use_id = e.tool_use.tool_use_id.clone();
                let tool_name = e.tool_use.name.clone();
                let raw_input = e.tool_use.input.clone();
                // Full message (with the "Failed to parse the tool use: ..."
                // wrapper) goes to the model so it has the protocol context.
                // The user-facing error uses just the inner kind message to
                // avoid surfacing the wrapper in the UI.
                let model_err_msg = e.to_string();
                let user_err_msg = e.kind.to_string();
                let tool_identity = e
                    .canonical_name
                    .as_ref()
                    .map(ToolCallIdentity::from_canonical_tool_name);
                pre_built_content.push(ContentBlock::ToolResult(ToolResultBlock {
                    tool_use_id: tool_use_id.clone(),
                    content: vec![ToolResultContentBlock::Text(model_err_msg.clone())],
                    status: ToolResultStatus::Error,
                }));
                pre_built_results.insert(tool_use_id.clone(), LogToolResult {
                    tool: None,
                    result: ToolCallResult::Error(ToolExecutionError::Custom(model_err_msg)),
                });
                // Notify that this tool call failed before execution
                self.agent_event_buf
                    .push(AgentEvent::Update(UpdateEvent::ToolCallFailed {
                        tool_use_id,
                        tool_name,
                        tool_identity,
                        raw_input,
                        reason: ToolCallFailureReason::ParseError,
                        error: user_err_msg,
                    }));
            }
        }

        // Nothing in this batch is executable — every tool_use was a parse
        // error and/or a `dummy` placeholder. Send the synthesized results
        // straight back so the model can react, BUT guard against an unbounded
        // unavailable-tool retry loop: after too many consecutive non-executable
        // turns, stop resending and force-end the turn so the prompt resolves
        // instead of the agent (and the ACP bridge's pending prompt) hanging.
        if tools.is_empty() {
            if pre_built_content.is_empty() {
                // No tool_uses to act on at all. handle_tool_uses is only
                // invoked with a non-empty batch, so this is defensive and
                // currently unreachable. Fail loud in debug/test builds: a
                // future refactor that lands here would return without ending
                // the turn, leaving the agent stuck in ExecutingRequest with
                // the loop alive (re-introducing the prompt hang this breaker
                // was added to fix).
                debug_assert!(
                    false,
                    "handle_tool_uses reached with no actionable tool_uses; returning here would leave the turn unended"
                );
                return Ok(());
            }
            self.consecutive_unexecutable_tool_turns += 1;
            if self.consecutive_unexecutable_tool_turns >= MAX_CONSECUTIVE_UNEXECUTABLE_TOOL_TURNS {
                warn!(
                    count = self.consecutive_unexecutable_tool_turns,
                    "ending turn after repeated turns with no executable tool calls"
                );
                return self
                    .end_turn_with_unexecutable_results(pre_built_content, pre_built_results)
                    .await;
            }
            let pending = PendingUserMessage::new_tool_results(pre_built_content.clone(), pre_built_results);
            let args = self.format_request(&pending).await;
            self.send_request(args).await?;
            self.set_active_state(ActiveState::ExecutingRequest {
                compaction_retry: None,
                empty_response_retried: false,
                pending_user_message: Some(pending),
            })
            .await;
            return Ok(());
        }

        // We have at least one executable tool — real progress was made, so
        // reset the unavailable-tool breaker. Any held parse-error / dummy
        // results in `pre_built_*` flow through to send_tool_results and get
        // merged into the eventual outbound batch.
        self.consecutive_unexecutable_tool_turns = 0;

        // Next, evaluate permissions.
        let mut needs_approval = Vec::new();
        let mut denied = Vec::new();
        let mut trust_options_map: HashMap<String, Vec<protocol::TrustOption>> = HashMap::new();
        for (block, tool) in &tools {
            let result = self.evaluate_tool_permission(tool).await?;
            match &result {
                PermissionEvalResult::Allow => (),
                PermissionEvalResult::Ask { trust_options } => {
                    needs_approval.push(block.tool_use_id.clone());
                    if !trust_options.is_empty() {
                        trust_options_map.insert(block.tool_use_id.clone(), trust_options.clone());
                    }
                },
                PermissionEvalResult::Deny { reason } => denied.push((block, tool, reason.clone())),
            }
            self.agent_event_buf
                .push(AgentEvent::Internal(InternalEvent::ToolPermissionEvalResult {
                    tool_use_id: block.tool_use_id.clone(),
                    tool: tool.clone(),
                    result,
                }));
        }

        // Return denied tools immediately back to the model
        if !denied.is_empty() {
            // Carry forward parse-error results from above so the model sees
            // a tool_result for every tool_use it emitted.
            let mut content = std::mem::take(&mut pre_built_content);
            let mut results = std::mem::take(&mut pre_built_results);
            for (block, tool, reason) in denied {
                // Full detail (including matched pattern / reason) goes to
                // the model so it can avoid retrying. The user-facing error
                // stays generic so internal deny-list patterns aren't leaked
                // to the UI.
                let model_err_msg =
                    format!("Tool use was rejected because the arguments supplied are forbidden: {reason}");
                let user_err_msg = "Tool use was rejected because the arguments supplied are forbidden".to_string();
                content.push(ContentBlock::ToolResult(ToolResultBlock {
                    tool_use_id: block.tool_use_id.clone(),
                    content: vec![ToolResultContentBlock::Text(model_err_msg.clone())],
                    status: ToolResultStatus::Error,
                }));
                results.insert(block.tool_use_id.clone(), LogToolResult {
                    tool: Some(Box::new(tool.clone())),
                    result: ToolCallResult::Error(ToolExecutionError::Custom(model_err_msg)),
                });
                // Notify that this tool call was denied
                self.agent_event_buf
                    .push(AgentEvent::Update(UpdateEvent::ToolCallFailed {
                        tool_use_id: block.tool_use_id.clone(),
                        tool_name: block.name.clone(),
                        tool_identity: Some(ToolCallIdentity::from_tool(tool)),
                        raw_input: block.input.clone(),
                        reason: ToolCallFailureReason::PermissionDenied,
                        error: user_err_msg,
                    }));
            }
            let pending = PendingUserMessage::new_tool_results(content.clone(), results);
            let args = self.format_request(&pending).await;
            self.send_request(args).await?;
            self.set_active_state(ActiveState::ExecutingRequest {
                compaction_retry: None,
                empty_response_retried: false,
                pending_user_message: Some(pending),
            })
            .await;
            return Ok(());
        }

        // Process PreToolUse hooks, if any.
        let hooks = self.get_hooks(HookTrigger::PreToolUse);
        let mut hooks_to_execute = Vec::new();
        for (block, tool) in &tools {
            hooks_to_execute.extend(
                hooks
                    .iter()
                    .filter(|h| hook_matches_tool(&h.config, tool))
                    .enumerate()
                    .map(|(index, h)| {
                        (
                            HookExecutionId {
                                hook: h.clone(),
                                tool_context: Some((block, tool).into()),
                                index,
                            },
                            Some((block.clone(), tool.clone())),
                        )
                    }),
            );
        }
        if !hooks_to_execute.is_empty() {
            debug!(?hooks_to_execute, "found hooks to execute for preToolUse");
            let stage = HookStage::PreToolUse {
                tools: tools.clone(),
                needs_approval: needs_approval.clone(),
                trust_options_map: trust_options_map.clone(),
                pre_built_content: std::mem::take(&mut pre_built_content),
                pre_built_results: std::mem::take(&mut pre_built_results),
            };
            self.start_hooks_execution(hooks_to_execute, stage, None, None).await;
            return Ok(());
        }

        self.process_tool_uses(
            tools,
            needs_approval,
            trust_options_map,
            pre_built_content,
            pre_built_results,
        )
        .await
    }

    /// Processes successfully parsed tool uses, requesting permission if required, and then
    /// executing.
    ///
    /// `pre_built_content` and `pre_built_results` carry tool_results synthesized before this
    /// stage — e.g. parse-error siblings from the same model batch. They flow through to
    /// send_tool_results so the model sees a tool_result for every tool_use it emitted, even
    /// when some couldn't be parsed and others were executed normally.
    async fn process_tool_uses(
        &mut self,
        tools: Vec<(ToolUseBlock, Tool)>,
        needs_approval: Vec<String>,
        trust_options_map: HashMap<String, Vec<protocol::TrustOption>>,
        pre_built_content: Vec<ContentBlock>,
        pre_built_results: HashMap<String, LogToolResult>,
    ) -> Result<(), AgentError> {
        for tool in &tools {
            // For the subagent (AgentCrew) tool, substitute `{task}` placeholders
            // in each stage's `prompt_template` on the display copy of the tool
            // input. The schema instructs the model to use `{task}` literally;
            // backend execution substitutes when feeding the spawned subagent
            // (agent_crew.rs spawn_ready_stages) but the model's raw input flows
            // verbatim to the TUI via tool_use_block.input. Without this,
            // subagent panel rows showing `prompt_template` render the literal
            // `{task}` token instead of the resolved prompt the user actually
            // dispatched. Mutating only the cloned display copy — the parsed
            // `Tool` and the executor still see the original.
            let mut display_block = tool.0.clone();
            if matches!(&tool.1.kind, tools::ToolKind::BuiltIn(tools::BuiltInTool::AgentCrew(_))) {
                tools::agent_crew::substitute_task_placeholder(&mut display_block.input);
            }
            self.agent_event_buf.push(
                ToolCall {
                    id: tool.0.tool_use_id.clone(),
                    tool: tool.1.clone(),
                    tool_use_block: display_block,
                }
                .into(),
            );
        }

        // request permission for any asked tools
        if !needs_approval.is_empty() {
            self.request_tool_approvals(
                tools,
                needs_approval,
                trust_options_map,
                pre_built_content,
                pre_built_results,
            )
            .await?;
            return Ok(());
        }

        self.execute_tools(tools, pre_built_content, pre_built_results).await
    }

    async fn start_hooks_execution(
        &mut self,
        hooks: Vec<(HookExecutionId, Option<(ToolUseBlock, Tool)>)>,
        stage: HookStage,
        prompt: Option<String>,
        assistant_response: Option<String>,
    ) {
        let mut hooks_state = Vec::new();
        for (id, tool_ctx) in hooks {
            let req = StartHookExecution {
                id: id.clone(),
                prompt: prompt.clone(),
                assistant_response: assistant_response.clone(),
                session_id: Some(self.conversation_state.id.to_string()),
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

            // Emit ToolCallFinished event for the completed tool. Mirror the
            // process_tool_uses display-copy substitution for AgentCrew so
            // post-completion scrollback shows resolved prompts, not the
            // literal `{task}` placeholder. See process_tool_uses for context.
            let mut display_block = tool.tool_use_block.clone();
            if matches!(
                &tool.tool.kind,
                tools::ToolKind::BuiltIn(tools::BuiltInTool::AgentCrew(_))
            ) {
                tools::agent_crew::substitute_task_placeholder(&mut display_block.input);
            }
            let tool_call = ToolCall {
                id: tool.tool_use_block.tool_use_id.clone(),
                tool: tool.tool.clone(),
                tool_use_block: display_block,
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
                    .enumerate()
                    .map(|(index, h)| {
                        (
                            HookExecutionId {
                                hook: h.clone(),
                                tool_context: Some(
                                    (&executing_tool.tool_use_block, &executing_tool.tool, &output).into(),
                                ),
                                index,
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
            self.start_hooks_execution(hooks_to_execute, stage, None, None).await;
            return Ok(());
        }

        // Check if switch_to_execution was called and approved — if so, end the turn
        // instead of sending tool results back to the LLM. This mirrors V1's behavior
        // of returning to PromptUser when a pending agent swap is detected.
        if self.should_end_turn_for_switch_to_execution(&executing_tools) {
            self.end_current_turn(false).await?;
            // Transition to Idle so the ACP layer can immediately swap_agent
            // when it processes the EndTurn event.
            if !matches!(self.active_state(), ActiveState::Idle) {
                self.set_active_state(ActiveState::Idle).await;
            }
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
            HookStage::PreToolUse {
                tools,
                needs_approval,
                trust_options_map,
                pre_built_content,
                pre_built_results,
            } => {
                // If any command hooks exited with status 2, then we'll block.
                // Otherwise, execute the tools.
                let mut denied_tools = Vec::new();
                for (block, tool) in tools {
                    if let Some(hook) = executing_hooks.has_failure_exit_code_for_tool(&block.tool_use_id) {
                        denied_tools.push((
                            block.tool_use_id.clone(),
                            block.name.clone(),
                            block.input.clone(),
                            tool.clone(),
                            hook.result.as_ref().cloned().expect("is some"),
                        ));
                    }
                }
                if !denied_tools.is_empty() {
                    // Send denied tool results back to the model. Carry forward
                    // any parse-error siblings from the original handle_tool_uses
                    // invocation so the model still sees a tool_result for every
                    // tool_use it emitted.
                    let mut content = pre_built_content.clone();
                    let mut results = pre_built_results.clone();
                    for (tool_use_id, tool_name, raw_input, tool, hook_res) in denied_tools {
                        let tool_identity = ToolCallIdentity::from_tool(&tool);
                        let err_msg = format!(
                            "PreToolHook blocked the tool execution: {}",
                            hook_res.output().unwrap_or("no output provided")
                        );
                        content.push(ContentBlock::ToolResult(ToolResultBlock {
                            tool_use_id: tool_use_id.clone(),
                            content: vec![ToolResultContentBlock::Text(err_msg.clone())],
                            status: ToolResultStatus::Error,
                        }));
                        results.insert(tool_use_id.clone(), LogToolResult {
                            tool: Some(Box::new(tool)),
                            result: ToolCallResult::Error(ToolExecutionError::Custom(err_msg.clone())),
                        });
                        // Notify that this tool call was rejected by hook
                        self.agent_event_buf
                            .push(AgentEvent::Update(UpdateEvent::ToolCallFailed {
                                tool_use_id,
                                tool_name,
                                tool_identity: Some(tool_identity),
                                raw_input,
                                reason: ToolCallFailureReason::HookRejected,
                                error: err_msg,
                            }));
                    }
                    let pending = PendingUserMessage::new_tool_results(content.clone(), results);
                    let args = self.format_request(&pending).await;
                    self.send_request(args).await?;
                    self.set_active_state(ActiveState::ExecutingRequest {
                        compaction_retry: None,
                        empty_response_retried: false,
                        pending_user_message: Some(pending),
                    })
                    .await;
                    return Ok(());
                }

                // Otherwise, continue to the approval stage.
                let tools = tools.clone();
                let needs_approval = needs_approval.clone();
                let trust_options_map = trust_options_map.clone();
                let pre_built_content = pre_built_content.clone();
                let pre_built_results = pre_built_results.clone();
                Ok(self
                    .process_tool_uses(
                        tools,
                        needs_approval,
                        trust_options_map,
                        pre_built_content,
                        pre_built_results,
                    )
                    .await?)
            },
            HookStage::PostToolUse { executing_tools } => {
                let executing_tools = executing_tools.clone();
                self.send_tool_results(&executing_tools).await?;
                Ok(())
            },
            HookStage::Stop { user_turn_metadata } => {
                // Check if any stop hook wants to block the stop and continue the conversation.
                // A hook can return JSON: {"decision": "block", "reason": "..."}
                let block_reason = executing_hooks.hooks().iter().find_map(|hook| {
                    let output = hook.result.as_ref()?.output()?;
                    let json: serde_json::Value = serde_json::from_str(output).ok()?;
                    if json.get("decision")?.as_str()? == "block" {
                        json.get("reason")?.as_str().map(|s| s.to_string())
                    } else {
                        None
                    }
                });
                // Clone the metadata out of the &mut self.execution_state borrow
                // so the drain-or-end-turn helper below can take &mut self.
                let md = (**user_turn_metadata).clone();

                if let Some(reason) = block_reason {
                    // Send the reason as a new user message to continue the conversation.
                    // The existing AgentLoop is still alive in UserTurnEnded state and can
                    // accept new requests, so we reuse it rather than spawning a new one.
                    let pending = PendingUserMessage::new_prompt(vec![ContentBlock::Text(reason)], None);
                    let args = self.format_request(&pending).await;
                    self.send_request(args).await?;
                    self.set_active_state(ActiveState::ExecutingRequest {
                        compaction_retry: None,
                        empty_response_retried: false,
                        pending_user_message: Some(pending),
                    })
                    .await;
                    Ok(())
                } else {
                    // Drain queued steering message at end-of-turn after stop hooks complete,
                    // or emit end-of-turn events if the queue is empty.
                    self.drain_steering_or_end_turn(md).await?;
                    Ok(())
                }
            },
        }
    }

    async fn make_tool_spec(&mut self) -> Vec<ToolSpec> {
        // Pre-fetch tool specs for all configured MCP servers
        let mut mcp_server_tool_specs = HashMap::new();
        for config in &self.cached_mcp_configs.configs {
            if !mcp_server_tool_specs.contains_key(&config.server_name)
                && let Ok(specs) = self.mcp_manager_handle.get_tool_specs(config.server_name.clone()).await
            {
                mcp_server_tool_specs.insert(config.server_name.clone(), specs);
            }
        }

        // Calculate total MCP tool spec tokens for conditional TST activation
        let mcp_tool_spec_tokens: usize = mcp_server_tool_specs
            .values()
            .flat_map(|specs| specs.iter())
            .map(|spec| {
                (spec.name.len()
                    + spec.description.len()
                    + serde_json::to_string(&spec.input_schema).map_or(0, |s| s.len()))
                    / consts::BYTES_PER_TOKEN
            })
            .sum();
        let lsp_initialized = self
            .code_intelligence
            .as_ref()
            .and_then(|c| c.try_read().ok())
            .is_some_and(|c| c.is_code_intelligence_initialized());

        let mut tool_names = tools::get_available_tool_names(
            &self.agent_config.tools(),
            &mcp_server_tool_specs,
            &self.cached_mcp_configs.configs,
            self.is_subagent,
            self.knowledge_provider.is_some(),
            self.settings.web_tools_enabled,
        );

        // Determine tool_search_active only after we know the agent's available tools.
        // If ToolSearch isn't in the agent's tool list, skip all tool search logic
        // (index rebuild, MCP filtering, deferred tools list injection).
        let tool_search_active = tool_names.contains(&CanonicalToolName::BuiltIn(tools::BuiltInToolName::ToolSearch))
            && should_activate_tool_search(&self.settings, mcp_tool_spec_tokens, self.model.context_window_size());
        self.tool_search_active = tool_search_active;

        if tool_search_active {
            let filtered_specs = filter_specs_by_allowed_tools(&mcp_server_tool_specs, &tool_names);
            self.rebuild_tool_search_index(&filtered_specs);
        } else {
            tool_names.remove(&CanonicalToolName::BuiltIn(tools::BuiltInToolName::ToolSearch));
        }

        let default_tool_settings = Default::default();
        let tool_settings = self.agent_config.tool_settings().unwrap_or(&default_tool_settings);
        // Filter MCP tools based on tool_search_active and tool_search_activated
        let tool_names = filter_tool_names(
            tool_search_active,
            tool_names.into_iter().collect(),
            &self.tool_search_activated,
            &self.settings.mandatory_mcp_names,
        );
        let sanitized_specs = sanitize_tool_specs(
            tool_names.into_iter().collect(),
            mcp_server_tool_specs,
            self.agent_config.tool_aliases(),
            lsp_initialized,
            &self.available_agent_configs,
            tool_settings,
        );
        if !sanitized_specs.transformed_tool_specs().is_empty() {
            warn!(transformed_tool_spec = ?sanitized_specs.transformed_tool_specs(), "some tool specs were transformed");
        }
        if !sanitized_specs.filtered_specs().is_empty() {
            warn!(filtered_specs = ?sanitized_specs.filtered_specs(), "filtered some tool specs");
        }
        let mut tool_specs = sanitized_specs.tool_specs_with_priority(self.is_subagent);
        add_tool_use_purpose_arg(&mut tool_specs);
        self.cached_tool_specs = Some(sanitized_specs);
        tool_specs
    }

    /// Parses tool use blocks into concrete tools, returning those that failed to be parsed.
    /// Parses a batch of model-emitted tool uses into executable tools.
    ///
    /// Returns a triple of `(executable, parse_errors, dummy)`:
    /// - `executable`: tool uses that mapped to a known tool and passed validation.
    /// - `parse_errors`: tool uses that failed name lookup, schema parsing, or validation.
    /// - `dummy`: tool uses naming the [`DUMMY_TOOL_NAME`] placeholder. These are neither
    ///   executable nor hard errors — the caller resolves them to a benign instructional
    ///   tool_result (see [`DUMMY_TOOL_RESULT_MESSAGE`]) so the model can self-correct instead of
    ///   looping on an unavailable tool.
    async fn parse_tools(
        &mut self,
        tool_uses: Vec<ToolUseBlock>,
    ) -> (Vec<(ToolUseBlock, Tool)>, Vec<ToolParseError>, Vec<ToolUseBlock>) {
        let mut tools: Vec<(ToolUseBlock, Tool)> = Vec::new();
        let mut parse_errors: Vec<ToolParseError> = Vec::new();
        let mut dummy_tool_uses: Vec<ToolUseBlock> = Vec::new();

        for tool_use in tool_uses {
            // The `dummy` placeholder is advertised by enforce_conversation_invariants when
            // history references a tool the current agent can't dispatch (it is never
            // registered in the tool map). Treat a model call to it as a benign no-op handled
            // by the caller, rather than a NameDoesNotExist error that would drive a tight
            // unavailable-tool retry loop.
            if tool_use.name == DUMMY_TOOL_NAME {
                dummy_tool_uses.push(tool_use);
                continue;
            }

            // If cached_tool_specs was invalidated (e.g. by a late MCP Initialized or
            // ToolListChanged event arriving between format_request and parse_tools),
            // rebuild them so we don't silently drop the tool the model just requested.
            if self.cached_tool_specs.is_none() {
                warn!("cached_tool_specs invalidated before parse_tools, rebuilding");
                self.make_tool_spec().await;
            }

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
                    // Rebuild failed — return a proper error instead of silently dropping.
                    warn!(
                        "cached_tool_specs is None even after rebuild; cannot parse tool '{}'",
                        tool_use.name
                    );
                    parse_errors.push(ToolParseError::new(
                        tool_use.clone(),
                        ToolParseErrorKind::NameDoesNotExist(tool_use.name),
                    ));
                    continue;
                },
            };
            let mut tool = match Tool::parse(&canonical_tool_name, tool_use.input.clone()) {
                Ok(t) => t,
                Err(err) => {
                    parse_errors.push(ToolParseError::new(tool_use, err).with_canonical_name(canonical_tool_name));
                    continue;
                },
            };
            match self.validate_tool(&mut tool).await {
                Ok(_) => tools.push((tool_use, tool)),
                Err(err) => {
                    parse_errors
                        .push(ToolParseError::new(tool_use, err).with_canonical_name(tool.canonical_tool_name()));
                },
            }
        }

        (tools, parse_errors, dummy_tool_uses)
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
                BuiltInTool::Mkdir(_) => Ok(()),
                BuiltInTool::ExecuteCmd(_) => Ok(()),
                BuiltInTool::Introspect(_) => Ok(()),
                BuiltInTool::Summary(_) => Ok(()),
                BuiltInTool::Goal(_) => Ok(()),
                BuiltInTool::UseAws(t) => t.validate().await.map_err(ToolParseErrorKind::invalid_args),
                BuiltInTool::WebFetch(_) => Ok(()),
                BuiltInTool::WebSearch(_) => Ok(()),
                BuiltInTool::Code(t) => t
                    .validate(&self.sys_provider)
                    .await
                    .map_err(ToolParseErrorKind::invalid_args),
                BuiltInTool::AgentCrew(_) => Ok(()),
                BuiltInTool::SessionManagement(_) => Ok(()),
                BuiltInTool::SwitchToExecution(_) => Ok(()),
                BuiltInTool::Knowledge(_) => Ok(()),
                BuiltInTool::ToolSearch(_) => Ok(()),
                BuiltInTool::Task(_) => Ok(()),
            },
            ToolKind::Mcp(mcp_tool) => {
                // Phase 2 (kiro-bot Taskei integration): populate `annotations`
                // from the MCP catalog so ACP clients can read `readOnlyHint`
                // via `RequestPermissionRequest._meta.mcpAnnotations`. Lookup
                // failure is non-fatal — annotations are advisory hints, not
                // contract data, so we log-and-continue rather than reject the
                // tool.
                match self
                    .mcp_manager_handle
                    .get_tool_annotations(mcp_tool.server_name.clone(), mcp_tool.tool_name.clone())
                    .await
                {
                    Ok(annotations) => {
                        mcp_tool.annotations = annotations;
                    },
                    Err(err) => {
                        warn!(
                            server_name = %mcp_tool.server_name,
                            tool_name = %mcp_tool.tool_name,
                            ?err,
                            "failed to fetch MCP tool annotations; continuing without hints"
                        );
                    },
                }
                Ok(())
            },
        }
    }

    async fn evaluate_tool_permission(&mut self, tool: &Tool) -> Result<PermissionEvalResult, AgentError> {
        if self.settings.trust_all_tools {
            return Ok(PermissionEvalResult::Allow);
        }
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
                Ok(PermissionEvalResult::ask())
            },
        }
    }

    async fn request_tool_approvals(
        &mut self,
        tools: Vec<(ToolUseBlock, Tool)>,
        needs_approval: Vec<String>,
        trust_options_map: HashMap<String, Vec<protocol::TrustOption>>,
        pre_built_content: Vec<ContentBlock>,
        pre_built_results: HashMap<String, LogToolResult>,
    ) -> Result<(), AgentError> {
        // First, update the agent state to WaitingForApproval
        let mut needs_approval_map = HashMap::new();
        for tool_use_id in &needs_approval {
            let Some((_, tool)) = tools.iter().find(|(b, _)| &b.tool_use_id == tool_use_id) else {
                warn!(tool_use_id, "tool requiring approval not found in tools list");
                continue;
            };
            let options = tool.permission_options();
            needs_approval_map.insert(tool_use_id.clone(), ApprovalState {
                options: options.clone(),
                selected: None,
                rejection_reason: None,
            });
        }
        self.set_active_state(ActiveState::WaitingForApproval(WaitingForApproval {
            tools: tools.clone(),
            needs_approval: needs_approval_map,
            pre_built_content,
            pre_built_results,
        }))
        .await;

        // Send notifications for each tool that requires approval
        for tool_use_id in &needs_approval {
            let Some((block, tool)) = tools.iter().find(|(b, _)| &b.tool_use_id == tool_use_id) else {
                continue;
            };
            let trust_options = trust_options_map.get(tool_use_id).cloned().unwrap_or_default();
            let options = tool.permission_options();
            self.agent_event_buf
                .push(AgentEvent::ApprovalRequest(protocol::ApprovalRequest {
                    id: block.tool_use_id.clone(),
                    tool_use: (*block).clone(),
                    tool: tool.clone(),
                    context: tool.get_context().await,
                    options,
                    trust_options,
                }));
        }

        Ok(())
    }

    async fn execute_tools(
        &mut self,
        tools: Vec<(ToolUseBlock, Tool)>,
        pre_built_content: Vec<ContentBlock>,
        pre_built_results: HashMap<String, LogToolResult>,
    ) -> Result<(), AgentError> {
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
        self.set_active_state(ActiveState::ExecutingTools(ExecutingTools {
            tools: tool_state,
            pre_built_content,
            pre_built_results,
        }))
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
                BuiltInTool::ExecuteCmd(t) => {
                    let event_tx = self.agent_event_tx.clone();
                    let tool_use_id = id.tool_use_id().to_string();
                    Box::pin(async move { t.execute(&provider, Some((tool_use_id, event_tx))).await })
                },
                BuiltInTool::Introspect(t) => Box::pin(async move { t.execute().await }),
                BuiltInTool::Grep(t) => Box::pin(async move { t.execute(&provider).await }),
                BuiltInTool::Glob(t) => Box::pin(async move { t.execute(&provider).await }),
                BuiltInTool::Mkdir(_) => panic!("unimplemented"),
                BuiltInTool::Summary(t) => {
                    let summary_tx = self.summary_tx.clone();
                    let result_tx = self.agent_event_tx.clone();
                    Box::pin(async move { t.execute(summary_tx, result_tx).await })
                },
                BuiltInTool::Goal(t) => {
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
                BuiltInTool::AgentCrew(t) => {
                    let event_tx = self.agent_event_tx.clone();
                    let tool_use_id = id.tool_use_id().to_string();
                    let crew_settings = self
                        .agent_config
                        .tool_settings()
                        .map(|s| s.crew.clone())
                        .unwrap_or_default();
                    Box::pin(async move { t.execute(tool_use_id, event_tx, &crew_settings).await })
                },
                BuiltInTool::SessionManagement(t) => {
                    let event_tx = self.agent_event_tx.clone();
                    Box::pin(async move { t.execute(event_tx).await })
                },
                BuiltInTool::SwitchToExecution(t) => Box::pin(async move { Ok(t.execute()) }),
                BuiltInTool::Knowledge(t) => {
                    let provider = self.knowledge_provider.clone();
                    Box::pin(async move {
                        match provider {
                            Some(p) => t.execute(&*p).await,
                            None => Err(ToolExecutionError::Custom(
                                "Knowledge tool is not available in this session.".to_string(),
                            )),
                        }
                    })
                },
                BuiltInTool::ToolSearch(t) => {
                    let result = tools::ToolSearch::execute(
                        t.tool_id.as_deref(),
                        t.query.as_deref(),
                        t.max_results,
                        &self.tool_search_index,
                        &self.tool_search_config,
                    );
                    match result {
                        Ok((output, effects)) => {
                            if !effects.tools_to_activate.is_empty() {
                                self.cached_tool_specs = None;
                            }
                            for tool_name in effects.tools_to_activate {
                                self.tool_search_activated.insert(tool_name);
                            }
                            Box::pin(async move { Ok(output) })
                        },
                        Err(e) => Box::pin(async move { Err(e) }),
                    }
                },
                BuiltInTool::Task(t) => {
                    let store = self.task_store.clone();
                    Box::pin(async move {
                        match store {
                            Some(s) => t.execute(&s),
                            None => Err(ToolExecutionError::Custom(
                                "Task tool is not available in this context".to_string(),
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
                            if resp.is_error.is_some_and(|v| v) {
                                warn!(?mcp_tool, "Tool call failed");
                            }
                            Ok(ToolExecutionOutput::new(vec![ToolExecutionOutputItem::Json(
                                mcp::tool_result_to_model_json(resp),
                            )]))
                        },
                        Err(err) => {
                            let msg = err.to_string();
                            if msg.contains(crate::agent::mcp::service::MCP_AUTH_REFRESH_FAILED)
                                || msg.contains(crate::agent::mcp::service::MCP_AUTH_REAUTH_FAILED)
                            {
                                Err(ToolExecutionError::Custom(format!(
                                    "Authentication failed for MCP server '{}'. Token refresh failed. Please re-authenticate using /mcp.",
                                    mcp_tool.server_name
                                )))
                            } else {
                                Err(ToolExecutionError::Custom(format!(
                                    "failed to send call tool request to the MCP server: {err}"
                                )))
                            }
                        },
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

    /// Check if switch_to_execution was among the completed tools and returned approved.
    /// When this is true, the turn should end without sending tool results back to the LLM,
    /// allowing the caller (ACP layer) to swap agents and inject the plan as a new prompt.
    #[allow(clippy::unused_self)]
    fn should_end_turn_for_switch_to_execution(&self, executing_tools: &ExecutingTools) -> bool {
        use tools::switch_to_execution::SwitchToExecutionResult;

        for tool in executing_tools.tools() {
            if let ToolKind::BuiltIn(BuiltInTool::SwitchToExecution(_)) = &tool.tool.kind
                && let Some(ToolExecutorResult::Completed { result: Ok(output), .. }) = &tool.result
                && let Some(ToolExecutionOutputItem::Text(json_str)) = output.items.first()
                && let Ok(result) = serde_json::from_str::<SwitchToExecutionResult>(json_str)
                && result.approved
            {
                return true;
            }
        }
        false
    }

    /// End-of-turn handler: if the steering queue is non-empty, extend the
    /// current turn by sending the queued content as the next user message
    /// (the turn continues in-place rather than ending and starting a new
    /// one). Otherwise, emit `EndTurn(md)` + `Stop(EndTurn)` and go idle.
    ///
    /// Extending the existing turn (rather than starting a new one) avoids:
    ///   - double per-turn metering (one `EndTurn(md)` per drained message)
    ///   - ACP's "cancel/end resolves the prompt response" race that would leave subsequent turn
    ///     events streaming into a TUI with no active prompt
    ///
    /// The `UserTurnMetadata` parameter represents the LLM's natural
    /// end-of-turn. When we extend, we drop that metadata — the turn isn't
    /// really ending, so its metering rolls up into the extended turn's
    /// final `EndTurn` instead.
    async fn drain_steering_or_end_turn(&mut self, md: UserTurnMetadata) -> Result<(), AgentError> {
        if !self.queued_steers.is_empty() {
            let steers = std::mem::take(&mut self.queued_steers);
            // Emit one consume notification per steer (carrying its id + raw
            // text) so clients can reconcile each queued steer by id, matching
            // the KAS contract. The drained text is still concatenated into a
            // single LLM continuation request below.
            let snapshot = steer_snapshot(&steers);
            for steer in steers {
                self.agent_event_buf.push(AgentEvent::SteeringConsumed {
                    message_id: steer.id,
                    content: steer.text,
                });
            }
            // Extend the existing agent loop (still alive in UserTurnEnded
            // state) with the drained content. This mirrors the stop-hook
            // block path above: reuse the loop rather than spawning a new
            // one, so the drained content is "continuation of the same
            // user turn" semantically.
            let pending = PendingUserMessage::new_prompt(vec![ContentBlock::Text(snapshot)], None);
            let args = self.format_request(&pending).await;
            self.send_request(args).await?;
            self.set_active_state(ActiveState::ExecutingRequest {
                compaction_retry: None,
                empty_response_retried: false,
                pending_user_message: Some(pending),
            })
            .await;
        } else {
            self.agent_event_buf.push(AgentEvent::EndTurn(md));
            self.agent_event_buf.push(AgentEvent::Stop(AgentStopReason::EndTurn));
            self.set_active_state(ActiveState::Idle).await;
        }
        Ok(())
    }

    /// Force-ends the current turn after [`MAX_CONSECUTIVE_UNEXECUTABLE_TOOL_TURNS`]
    /// consecutive turns produced no executable tool calls (only parse errors
    /// and/or `dummy` placeholder calls). This breaks the otherwise-unbounded
    /// auto-resend loop.
    ///
    /// `content`/`results` are the synthesized tool_results for the offending
    /// batch. We commit them to history so every dangling tool_use is paired
    /// with a result (keeping the conversation valid for the next turn), surface
    /// a short assistant message explaining the stop, then cancel the agent loop
    /// so the turn ends and the ACP bridge releases its pending prompt response.
    async fn end_turn_with_unexecutable_results(
        &mut self,
        content: Vec<ContentBlock>,
        results: HashMap<String, LogToolResult>,
    ) -> Result<(), AgentError> {
        // Pair the unanswered tool_uses with their synthesized results so the
        // alternating tool_use/tool_result invariant holds for the next turn.
        self.append_tool_results(Uuid::new_v4().to_string(), content, results);

        // Surface a clear assistant message and persist it (this also restores
        // role alternation: the just-appended tool_results are a user message).
        self.agent_event_buf.push(AgentEvent::Update(UpdateEvent::AgentContent(
            REPEATED_UNEXECUTABLE_TOOL_MESSAGE.to_string().into(),
        )));
        self.append_assistant_message(Message::new(
            // synthetic id; message only sent as history, not as the active prompt of a request
            Uuid::new_v4().to_string(),
            Role::Assistant,
            vec![ContentBlock::Text(REPEATED_UNEXECUTABLE_TOOL_MESSAGE.to_string())],
            Some(Utc::now()),
        ));

        // Reset so a subsequent user prompt starts with a clean breaker count.
        self.consecutive_unexecutable_tool_turns = 0;

        // Drop the (already-committed) stale pending so end_current_turn() won't
        // re-append it when draining UserTurnEnd, and so it doesn't synthesize
        // "cancelled" results for the now-answered tool_uses. Then end the turn:
        // end_current_turn cancels the loop, which emits UserTurnEnd → EndTurn,
        // and the ACP bridge releases the pending prompt response on EndTurn.
        // Emit EndTurn (inside end_current_turn) before Stop to match the
        // drain_steering_or_end_turn ordering.
        self.set_active_state(ActiveState::Idle).await;
        self.end_current_turn(false).await?;
        self.agent_event_buf.push(AgentEvent::Stop(AgentStopReason::EndTurn));
        Ok(())
    }

    async fn send_tool_results(&mut self, executing_tools: &ExecutingTools) -> Result<(), AgentError> {
        let mut content = executing_tools.pre_built_content.clone();
        let mut results = executing_tools.pre_built_results.clone();

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

        // Drain queued steering messages and append as user content.
        // The combined snapshot becomes a single LLM content block, while one
        // consume notification is emitted per steer (id + raw text) to match
        // the KAS contract's per-message identity tracking.
        if !self.queued_steers.is_empty() {
            let steers = std::mem::take(&mut self.queued_steers);
            let snapshot = steer_snapshot(&steers);
            content.push(ContentBlock::Text(format_steering_message(&snapshot)));
            for steer in steers {
                self.agent_event_buf.push(AgentEvent::SteeringConsumed {
                    message_id: steer.id,
                    content: steer.text,
                });
            }
        }

        let pending = PendingUserMessage::new_tool_results(content.clone(), results);
        let args = self.format_request(&pending).await;
        self.send_request(args).await?;
        self.set_active_state(ActiveState::ExecutingRequest {
            compaction_retry: None,
            empty_response_retried: false,
            pending_user_message: Some(pending),
        })
        .await;
        Ok(())
    }

    async fn handle_mcp_events(&mut self, evt: McpServerEvent) {
        // Intercept events for in-flight forced-auth shadows before normal
        // handling. Shadows are hidden from the UI: their events are suppressed or
        // rewritten to the target server name (and drive promotion/cleanup).
        if self.handle_reauth_shadow_event(&evt).await {
            return;
        }

        // Invalidate cached tool specs when the tool set may have changed.
        if matches!(
            evt,
            McpServerEvent::ToolListChanged { .. } | McpServerEvent::Initialized { .. }
        ) {
            self.cached_tool_specs = None;
        }

        let converted_evt = AgentEvent::Mcp(evt.clone());
        self.agent_event_buf.push(converted_evt);
    }

    /// Handle an MCP event that belongs to an in-flight forced-auth flow.
    ///
    /// Returns `true` if the event was for a tracked shadow/target (and was
    /// handled here — including any forwarding), or `false` if it's unrelated to
    /// forced auth and should be handled normally by [`Self::handle_mcp_events`].
    ///
    /// Behaviour for a tracked event:
    /// - `OauthRequest` → rewritten to the **target** so the UI shows the prompt on the master
    ///   server (the shadow is never displayed).
    /// - `Initialized` → promote the shadow to the target (loaded case), invalidate caches, and
    ///   forward `Initialized(target)`.
    /// - `InitializeError` → loaded case: drop the shadow, leave the original running, and refresh
    ///   the target in the UI; not-loaded case: surface the error and relaunch under the normal
    ///   flow.
    /// - `Initializing` / `ToolListChanged` → suppressed (the shadow stays hidden).
    async fn handle_reauth_shadow_event(&mut self, evt: &McpServerEvent) -> bool {
        let Some(target) = self.reauth_shadows.get(evt.server_name()).cloned() else {
            return false;
        };
        let shadow_name = evt.server_name().to_string();
        // `shadow_name == target` is the not-loaded case (launched under the real
        // name with no separate shadow).
        let is_shadow = shadow_name != target;

        match evt {
            McpServerEvent::OauthRequest { oauth_url, .. } => {
                self.agent_event_buf.push(AgentEvent::Mcp(McpServerEvent::OauthRequest {
                    server_name: target,
                    oauth_url: oauth_url.clone(),
                }));
            },
            McpServerEvent::Initialized { .. } => {
                self.reauth_shadows.remove(&shadow_name);
                if is_shadow
                    && let Err(e) = self
                        .mcp_manager_handle
                        .promote_server(shadow_name.clone(), target.clone())
                        .await
                {
                    warn!(target = %target, error = %e, "failed to promote reauth shadow");
                }
                // Tools likely changed now that the server is authenticated.
                self.cached_tool_specs = None;
                self.refresh_mcp_server_in_ui(&target);
            },
            McpServerEvent::InitializeError { error, .. } => {
                self.reauth_shadows.remove(&shadow_name);
                if is_shadow {
                    // Loaded case: the original is still running. Drop the shadow
                    // (clearing its failed marker) and refresh the original in the
                    // UI so its pending-OAuth state is cleared.
                    if let Err(e) = self.mcp_manager_handle.shutdown_server(shadow_name.clone()).await {
                        warn!(target = %target, %shadow_name, error = %e, "failed to drop failed reauth shadow");
                    }
                    self.refresh_mcp_server_in_ui(&target);
                } else {
                    // Not-loaded case: forced auth on the real server failed. Surface
                    // the error, then relaunch under the normal (non-forced) flow so
                    // the user keeps any unauthenticated capabilities.
                    warn!(target = %target, error = %error, "forced MCP auth failed; reloading under normal flow");
                    self.agent_event_buf
                        .push(AgentEvent::Mcp(McpServerEvent::InitializeError {
                            server_name: target.clone(),
                            error: error.clone(),
                        }));
                    self.reload_mcp_server_normally(&target).await;
                }
            },
            McpServerEvent::Initializing { .. } | McpServerEvent::ToolListChanged { .. } => {
                // Suppress: the shadow must stay invisible. The `authenticating`
                // flag (from GetMcpServerInfo) conveys progress on the target.
            },
        }
        true
    }

    /// Rebuild the BM25 tool index from MCP tool specs
    fn rebuild_tool_search_index(&mut self, mcp_server_tool_specs: &HashMap<String, Vec<ToolSpec>>) {
        self.tool_search_index = ToolIndex::from_tool_specs(mcp_server_tool_specs);
    }

    /// This prepends the embedded user msg to the system prompt field of the agent
    pub fn prepend_embedded_user_msg(&mut self, msg: &str) {
        self.agent_config.set_global_prompt_prefix(msg);
    }

    /// This appends the embedded user msg to the global prompt field of the agent
    pub fn append_embedded_user_msg(&mut self, msg: &str) {
        self.agent_config.set_global_prompt_suffix(msg);
    }

    /// Append a user message to the conversation and emit the log event.
    fn append_user_message(&mut self, id: String, content: Vec<ContentBlock>, meta: Option<MessageMetadata>) {
        let entry = LogEntry::prompt(id, content, meta);
        let index = self.conversation_state.append_log(entry.clone());
        self.agent_event_buf.push(AgentEvent::LogEntryAppended { entry, index });
    }

    /// Append tool results to the conversation and emit the log event.
    fn append_tool_results(&mut self, id: String, content: Vec<ContentBlock>, results: HashMap<String, LogToolResult>) {
        let entry = LogEntry::tool_results(id, content, results);
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
#[allow(clippy::too_many_arguments)]
async fn format_request<T, U, P>(
    mut messages: VecDeque<Message>,
    mut tool_spec: Vec<ToolSpec>,
    agent_config: &LoadedAgentConfig,
    agent_spawn_hooks: T,
    provider: &P,
    latest_summary: Option<String>,
    task_context: Option<String>,
    knowledge_context: Option<String>,
    context_window_size: Option<usize>,
    tool_search_enabled: bool,
    deferred_tools_list: Option<String>,
    model_name: Option<&str>,
) -> SendRequestArgs
where
    T: IntoIterator<Item = U>,
    U: AsRef<str>,
    P: SystemProvider,
{
    enforce_conversation_invariants(&mut messages, &mut tool_spec);

    let ctx_messages = create_context_messages(
        agent_config,
        agent_spawn_hooks,
        latest_summary,
        task_context,
        knowledge_context,
        provider,
        context_window_size,
        tool_search_enabled,
        deferred_tools_list,
        model_name,
    )
    .await;
    for msg in ctx_messages.into_iter().rev() {
        messages.push_front(msg);
    }

    SendRequestArgs::new(
        messages.into(),
        if tool_spec.is_empty() { None } else { Some(tool_spec) },
        agent_config.global_prompt(),
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
#[allow(clippy::too_many_arguments)]
async fn create_context_messages<T, U, P>(
    agent_config: &LoadedAgentConfig,
    agent_spawn_hooks: T,
    latest_summary: Option<String>,
    task_context: Option<String>,
    knowledge_context: Option<String>,
    provider: &P,
    context_window_size: Option<usize>,
    tool_search_enabled: bool,
    deferred_tools_list: Option<String>,
    model_name: Option<&str>,
) -> Vec<Message>
where
    T: IntoIterator<Item = U>,
    U: AsRef<str>,
    P: SystemProvider,
{
    let global_prompt = agent_config.global_prompt();
    let (mut files, skills) = collect_resources(agent_config.resources(), provider).await;

    drop_resources_exceeding_budget(&mut files, context_window_size);

    let content = format_user_context_message(
        global_prompt.as_deref(),
        files.iter().map(|r| (r.file_path.as_str(), r.content.as_str())),
        skills.iter().map(|r| &r.content),
        agent_spawn_hooks,
        latest_summary,
        task_context,
        knowledge_context,
        tool_search_enabled,
        deferred_tools_list.as_deref(),
        model_name,
    );
    if content.is_empty() {
        return vec![];
    }
    let user_msg = Message {
        id: None,
        role: Role::User,
        content: vec![ContentBlock::Text(content)],
        meta: None,
    };
    let assistant_msg = Message {
        id: None,
        role: Role::Assistant,
        content: vec![ContentBlock::Text(
            "I will fully incorporate this information when generating my responses, and explicitly acknowledge relevant parts of the summary when answering questions.".to_string(),
        )],
        meta: None,
    };

    vec![user_msg, assistant_msg]
}

/// Format a steering message using the LIVE STEERING format.
///
/// Generates a `steer-` prefixed message ID and wraps the user's message
/// in the standard steering template that instructs the LLM to incorporate
/// the user's mid-turn guidance.
fn format_steering_message(content: &str) -> String {
    let message_id = format!("steer-{}", Uuid::new_v4().as_simple());
    format!(
        "[LIVE STEERING - New message from user]\n\
         \n\
         The user sent a new message while you are working. As the currently active agent, \
         adjust your approach if necessary based on this guidance.\n\
         \n\
         <user_message id=\"{message_id}\">\n\
         {content}\n\
         </user_message>\n\
         \n\
         IMPORTANT: After completing your work, include a brief note about how you handled \
         this steering message. Use this exact format:\n\
         \n\
         [STEERING {message_id}: <describe what you did or why it wasn't applicable>]"
    )
}

#[allow(clippy::too_many_arguments)]
fn format_user_context_message<'a, T, U, V, W, X>(
    system_prompt: Option<&str>,
    files: T,
    skills: W,
    agent_spawn_hooks: U,
    latest_summary: Option<String>,
    task_context: Option<String>,
    knowledge_context: Option<String>,
    tool_search_enabled: bool,
    deferred_tools_list: Option<&str>,
    model_name: Option<&str>,
) -> String
where
    T: IntoIterator<Item = (&'a str, &'a str)>,
    U: IntoIterator<Item = V>,
    W: IntoIterator<Item = X>,
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

    // Knowledge base listing — matches V1's context injection at position 2 (after summary).
    // This is a discovery mechanism: the model sees what KBs exist and can then use the
    // `knowledge search` tool to retrieve actual content.
    if let Some(kb_ctx) = knowledge_context {
        context_content.push_str(CONTEXT_ENTRY_START_HEADER);
        context_content.push_str(&kb_ctx);
        context_content.push_str(CONTEXT_ENTRY_END_HEADER);
    }

    if let Some(task_ctx) = task_context {
        context_content.push_str(CONTEXT_ENTRY_START_HEADER);
        context_content.push_str(&task_ctx);
        context_content.push_str(CONTEXT_ENTRY_END_HEADER);
    }

    if tool_search_enabled && let Some(tools_list) = deferred_tools_list {
        context_content.push_str(CONTEXT_ENTRY_START_HEADER);
        context_content.push_str(DEFERRED_TOOLS_MESSAGE);
        context_content.push_str(tools_list);
        context_content.push('\n');
        context_content.push_str(CONTEXT_ENTRY_END_HEADER);
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

    for (file_path, content) in files {
        context_content.push_str(CONTEXT_ENTRY_START_HEADER);
        context_content.push_str(&format!("[{}]\n{}", file_path, content));
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

    // Agent prompt placed last for maximum recency bias, matching V1 behavior.
    // V1 injects the prompt after all context files and hooks, ensuring the model
    // prioritizes the agent's instructions over resource content.
    if let Some(prompt) = system_prompt {
        context_content.push_str(&format!("Follow this instruction: {prompt}"));
    }
    if let Some(name) = model_name {
        if name.eq_ignore_ascii_case("auto") {
            context_content.push_str("\nThe current model is Auto (model selected dynamically on the server).\n");
        } else {
            context_content.push_str(&format!("\nThe current model is {name}.\n"));
        }
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
/// - Any tool uses that do not exist in the provided tool specs will have their arguments replaced
///   with dummy content.
pub(super) fn enforce_conversation_invariants(messages: &mut VecDeque<Message>, tools: &mut Vec<ToolSpec>) {
    if messages.is_empty() {
        return;
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

use resource_budget::{
    Resource,
    drop_resources_exceeding_budget,
};

/// Parse skill frontmatter and format as hint
fn format_skill_hint(file_path: &str, content: &str) -> Option<String> {
    let yaml = crate::util::steering::extract_yaml_frontmatter(content)?;

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
    use std::collections::HashSet;

    use glob;

    let mut files = Vec::new();
    let mut skills = Vec::new();
    // Track canonicalized paths already loaded so we don't include the same file
    // twice when multiple URIs resolve to the same path. Examples:
    //   - "file://./AGENTS.md" and "file://AGENTS.md" (one user-declared, one default-injected)
    //   - A literal "file://path/foo.md" plus a glob like "file://**/*.md" that also matches it
    //   - Two paths via different symlink chains pointing at the same target
    // Cardinality is bounded by the number of resource files (typically ~tens, ~100 worst case),
    // so a HashSet of String paths is fine.
    let mut seen_files: HashSet<String> = HashSet::new();
    let mut seen_skills: HashSet<String> = HashSet::new();

    for resource in resources {
        let Ok(kind) = ResourceKind::parse(resource.as_ref(), provider) else {
            continue;
        };
        match kind {
            ResourceKind::File { original, file_path } => {
                let Ok(path) = canonicalize_path_sys(file_path, provider) else {
                    continue;
                };
                if !seen_files.insert(path.clone()) {
                    continue;
                }
                let Ok((content, _)) = read_file_with_max_limit(&path, MAX_RESOURCE_FILE_LENGTH, "...truncated").await
                else {
                    continue;
                };
                files.push(Resource {
                    config_value: original.to_string(),
                    file_path: path.clone(),
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
                        // Canonicalize before deduping so a glob hit collapses with an
                        // explicitly-listed file (which is canonicalized in the `File` arm).
                        // Without this, the same file loads twice whenever the literal glob
                        // path differs from the canonical path (e.g. a symlinked cwd such as
                        // macOS `/tmp` -> `/private/tmp`).
                        let entry_path_str = canonicalize_path_sys(entry.to_string_lossy(), provider)
                            .unwrap_or_else(|_| entry.to_string_lossy().to_string());
                        if !seen_files.insert(entry_path_str.clone()) {
                            continue;
                        }
                        let Ok((content, _)) =
                            read_file_with_max_limit(entry.as_path(), MAX_RESOURCE_FILE_LENGTH, "...truncated").await
                        else {
                            continue;
                        };
                        files.push(Resource {
                            config_value: original.to_string(),
                            file_path: entry_path_str,
                            content,
                        });
                    }
                }
            },
            ResourceKind::Skill { original, file_path } => {
                let Ok(path) = canonicalize_path_sys(&file_path, provider) else {
                    continue;
                };
                if !seen_skills.insert(path.clone()) {
                    continue;
                }
                let Ok((content, _)) = read_file_with_max_limit(&path, MAX_RESOURCE_FILE_LENGTH, "...truncated").await
                else {
                    continue;
                };
                let hint = format_skill_hint(&file_path, &content).unwrap_or(content);
                skills.push(Resource {
                    config_value: original.to_string(),
                    file_path: path.clone(),
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
                        let file_path_str = entry.to_string_lossy().to_string();
                        // Canonicalize the dedup key so a glob hit collapses with an
                        // explicitly-listed skill (canonicalized in the `Skill` arm).
                        let dedup_key =
                            canonicalize_path_sys(&file_path_str, provider).unwrap_or_else(|_| file_path_str.clone());
                        if !seen_skills.insert(dedup_key) {
                            continue;
                        }
                        let Ok((content, _)) =
                            read_file_with_max_limit(entry.as_path(), MAX_RESOURCE_FILE_LENGTH, "...truncated").await
                        else {
                            continue;
                        };
                        let hint = format_skill_hint(&file_path_str, &content).unwrap_or(content);
                        skills.push(Resource {
                            config_value: original.to_string(),
                            file_path: file_path_str,
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
        ToolNameKind::Subagent { .. } => tool.builtin_tool_name() == Some(tools::BuiltInToolName::AgentCrew),
    }
}

/// Pending user message to be appended to the event log only after a successful assistant response.
///
/// User messages are not persisted immediately when sent to the model. Instead, they are held
/// as "pending" until we receive a successful assistant response. This ensures we don't log
/// messages for requests that fail and need to be retried (e.g., due to context window overflow).
#[derive(Debug, Clone)]
pub enum PendingUserMessage {
    /// A user prompt (text, images, etc.)
    Prompt {
        /// Stable id assigned at construction. Reused for both the in-flight
        /// [Message] sent to the model and the persisted [LogEntry] in the
        /// conversation log so they can be correlated.
        id: String,
        content: Vec<ContentBlock>,
        meta: Option<MessageMetadata>,
    },
    /// Tool execution results sent back to the model.
    ToolResults {
        /// Stable id assigned at construction. See [PendingUserMessage::Prompt::id].
        id: String,
        /// The content blocks sent to the model (ToolResultBlock items).
        content: Vec<ContentBlock>,
        /// Metadata for the event log: maps tool_use_id to the parsed tool and execution result.
        /// This provides richer information than `content` alone for logging/debugging.
        results: HashMap<String, LogToolResult>,
    },
}

impl PendingUserMessage {
    /// Constructs a new prompt with a freshly generated id.
    pub fn new_prompt(content: Vec<ContentBlock>, meta: Option<MessageMetadata>) -> Self {
        Self::Prompt {
            id: Uuid::new_v4().to_string(),
            content,
            meta,
        }
    }

    /// Constructs a new tool results message with a freshly generated id.
    pub fn new_tool_results(content: Vec<ContentBlock>, results: HashMap<String, LogToolResult>) -> Self {
        Self::ToolResults {
            id: Uuid::new_v4().to_string(),
            content,
            results,
        }
    }

    /// Returns the stable id of the pending message.
    pub fn id(&self) -> &str {
        match self {
            PendingUserMessage::Prompt { id, .. } => id,
            PendingUserMessage::ToolResults { id, .. } => id,
        }
    }

    /// Returns the content blocks to be sent to the model.
    pub fn content(&self) -> &[ContentBlock] {
        match self {
            PendingUserMessage::Prompt { content, .. } => content,
            PendingUserMessage::ToolResults { content, .. } => content,
        }
    }

    /// Returns the metadata for prompt messages.
    pub fn meta(&self) -> Option<&MessageMetadata> {
        match self {
            PendingUserMessage::Prompt { meta, .. } => meta.as_ref(),
            PendingUserMessage::ToolResults { .. } => None,
        }
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
    ExecutingRequest {
        /// If Some, this request is a retry after successful compaction.
        /// Used to detect when overflow happens on retry, indicating the user message needs
        /// truncation.
        #[serde(default)]
        compaction_retry: Option<CompactionRetry>,
        /// Whether this request is a retry after a previous empty response. Set on the first
        /// retry; if the retry also returns empty, the agent enters the error state instead of
        /// retrying again.
        #[serde(default)]
        empty_response_retried: bool,
        /// User message that triggered this request, to be appended to the event log only after
        /// receiving a successful assistant response. This ensures we don't persist user messages
        /// that fail (e.g., due to ContextWindowOverflow) and need to be retried or truncated.
        #[serde(skip)]
        pending_user_message: Option<PendingUserMessage>,
    },
    /// Agent is executing tools
    ExecutingTools(ExecutingTools),
    /// Agent is compacting conversation history
    Compacting {
        /// The strategy used for compaction
        strategy: CompactStrategy,
        /// User message that caused ContextWindowOverflow, preserved during compaction so it can
        /// be retried afterward. May be truncated if the retry also overflows.
        #[serde(skip)]
        pending_user_message: Option<PendingUserMessage>,
    },
}

/// Tracks state for retrying a request after compaction.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct CompactionRetry {
    /// Whether the user message has been truncated in a previous retry attempt.
    pub is_prompt_truncated: bool,
}

/// Tracks approval state for a single tool use.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApprovalState {
    /// Available permission options for this tool
    pub options: Vec<PermissionOption>,
    /// The option selected by the user, if any
    pub selected: Option<PermissionOptionId>,
    /// Optional rejection reason (user feedback from drill-in edit)
    #[serde(default)]
    pub rejection_reason: Option<String>,
}

/// State for tools waiting for user approval.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WaitingForApproval {
    /// Tools pending approval with their parsed definitions
    pub tools: Vec<(ToolUseBlock, Tool)>,
    /// Approval state keyed by tool use ID
    pub needs_approval: HashMap<String, ApprovalState>,
    /// Pre-built tool_results to merge into the eventual outbound batch — see
    /// ExecutingTools for the rationale.
    #[serde(default)]
    pub pre_built_content: Vec<ContentBlock>,
    #[serde(default)]
    pub pre_built_results: HashMap<String, LogToolResult>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutingTools {
    tools: Vec<ExecutingTool>,
    /// Tool results synthesized before execution started — e.g. parse-error
    /// results for siblings of successfully-parsed tools in the same model
    /// batch. Carried through the executor stages and merged into the
    /// outbound tool_results message in send_tool_results so the model sees
    /// every tool_use from its assistant turn paired with a tool_result,
    /// without short-circuiting the parsed-OK tools.
    #[serde(default)]
    pre_built_content: Vec<ContentBlock>,
    #[serde(default)]
    pre_built_results: HashMap<String, LogToolResult>,
}

impl ExecutingTools {
    fn tools(&self) -> &[ExecutingTool] {
        &self.tools
    }

    fn get_tool(&self, id: &ToolExecutionId) -> Option<&ExecutingTool> {
        self.tools.iter().find(|tool| &tool.id == id)
    }

    fn get_tool_mut(&mut self, id: &ToolExecutionId) -> Option<&mut ExecutingTool> {
        self.tools.iter_mut().find(|tool| &tool.id == id)
    }

    fn all_tools_finished(&self) -> bool {
        self.tools.iter().all(|tool| tool.result.is_some())
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
        /// Granular trust options per tool_use_id from permission evaluation
        trust_options_map: HashMap<String, Vec<protocol::TrustOption>>,
        /// Pre-built tool_results to merge into the eventual outbound batch — see
        /// ExecutingTools for the rationale.
        #[serde(default)]
        pre_built_content: Vec<ContentBlock>,
        #[serde(default)]
        pre_built_results: HashMap<String, LogToolResult>,
    },
    /// Hooks after executing tool uses
    PostToolUse { executing_tools: ExecutingTools },
    /// Hooks when the assistant finishes responding
    Stop {
        /// The [UserTurnMetadata] for the completed user turn
        user_turn_metadata: Box<UserTurnMetadata>,
    },
}

/// Recover a [`Summary`] from a pending (un-executed) `summary` tool use.
///
/// When the summary tool is the last thing a subagent emitted but its turn is
/// torn down by an error before the tool executes, the model's real result is
/// still present in the tool-use input. Returns `Some` only when `name` is the
/// summary tool and its `input` deserializes into a valid [`Summary`].
fn recover_pending_summary(name: &str, input: &serde_json::Value) -> Option<Summary> {
    if name != tools::BuiltInToolName::Summary.to_string() {
        return None;
    }
    match serde_json::from_value::<Summary>(input.clone()) {
        Ok(summary) => Some(summary),
        Err(e) => {
            warn!(?e, "failed to deserialize pending summary tool use during salvage");
            None
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::util::test::TestBase;

    /// A pending summary tool use carries the model's real result in its input;
    /// the salvage path must recover it verbatim so a turn torn down by an error
    /// before the tool executed still delivers the subagent's result.
    #[test]
    fn recover_pending_summary_extracts_result_from_input() {
        let name = tools::BuiltInToolName::Summary.to_string();
        let input = serde_json::json!({
            "taskDescription": "find research papers",
            "contextSummary": "searched internal + external",
            "taskResult": "Found 3 relevant papers on deal recommendation.",
        });

        let summary = recover_pending_summary(&name, &input).expect("summary should be recovered");
        assert_eq!(summary.task_description, "find research papers");
        assert_eq!(summary.context_summary.as_deref(), Some("searched internal + external"));
        assert_eq!(summary.task_result, "Found 3 relevant papers on deal recommendation.");
    }

    /// Non-summary tool uses must never be salvaged as a summary.
    #[test]
    fn recover_pending_summary_ignores_other_tools() {
        let input = serde_json::json!({ "path": "/tmp/foo.txt" });
        assert!(recover_pending_summary("fs_read", &input).is_none());
    }

    /// A summary-named tool use whose input is missing required fields must not
    /// fabricate a partial summary — it returns None so the existing fallback runs.
    #[test]
    fn recover_pending_summary_rejects_malformed_input() {
        let name = tools::BuiltInToolName::Summary.to_string();
        // Missing the required `taskResult` field.
        let input = serde_json::json!({ "taskDescription": "incomplete" });
        assert!(recover_pending_summary(&name, &input).is_none());
    }

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

    /// Two `file://` URIs that canonicalize to the same path (e.g. `./AGENTS.md`
    /// and `AGENTS.md`) should not produce duplicate `Resource` entries. The
    /// `append_default_agent_resources` helper injects the bare-form `file://AGENTS.md`
    /// and a user agent might declare `file://./AGENTS.md` — both end up in the
    /// resources list because `add_resource` only string-compares. The dedup
    /// must happen here, post-canonicalization.
    #[tokio::test]
    async fn test_collect_resources_dedupes_canonical_paths() {
        let test_base = TestBase::new().await.with_file(("AGENTS.md", "# Agents")).await;

        let (resources, _skills) = collect_resources(["file://./AGENTS.md", "file://AGENTS.md"], &test_base).await;

        assert_eq!(
            resources.len(),
            1,
            "expected ./AGENTS.md and AGENTS.md to dedup to one Resource, got: {:?}",
            resources
                .iter()
                .map(|r| (&r.config_value, &r.file_path))
                .collect::<Vec<_>>()
        );
    }

    /// Regression for the 2.7 double-load bug: a file listed explicitly and the
    /// same file matched by an injected absolute-path glob must not load twice.
    /// `append_default_agent_resources` injects steering as a `file://<cwd>/.kiro/steering/**/*.md`
    /// glob; a user agent that also lists a specific steering file would get it twice
    /// because glob entries were deduped by their raw (non-canonical) path while
    /// explicit entries are canonicalized. When the glob reaches the file through a
    /// symlinked path component the two keys differ and dedup fails. Canonicalizing
    /// glob entries before the dedup check fixes it.
    #[tokio::test]
    #[cfg(unix)]
    async fn test_collect_resources_dedupes_glob_through_symlink() {
        let base = TestBase::new()
            .await
            .with_file((".kiro/steering/a.md", "# Steering A"))
            .await;

        let real_steering = base.join(".kiro/steering");
        let link_steering = base.join("linksteer");
        std::os::unix::fs::symlink(&real_steering, &link_steering).unwrap();

        // Explicit canonical file + a glob reaching the same file via the symlink.
        let explicit = format!("file://{}", real_steering.join("a.md").display());
        let glob = format!("file://{}/*.md", link_steering.display());

        let (resources, _skills) = collect_resources([explicit, glob], &base).await;

        assert_eq!(
            resources.len(),
            1,
            "explicit file and glob-through-symlink should dedup to one Resource, got: {:?}",
            resources.iter().map(|r| &r.file_path).collect::<Vec<_>>()
        );
    }

    #[test]
    fn test_format_user_context_message_with_model_name() {
        let content = format_user_context_message(
            None,
            std::iter::empty::<(&str, &str)>(),
            std::iter::empty::<&str>(),
            std::iter::empty::<&str>(),
            None,
            None,
            None,
            false,
            None,
            Some("Claude Sonnet 4"),
        );
        assert!(
            content.contains("The current model is Claude Sonnet 4."),
            "expected model name in context: {content}"
        );
    }

    #[test]
    fn test_format_user_context_message_with_auto_model() {
        let content = format_user_context_message(
            None,
            std::iter::empty::<(&str, &str)>(),
            std::iter::empty::<&str>(),
            std::iter::empty::<&str>(),
            None,
            None,
            None,
            false,
            None,
            Some("auto"),
        );
        assert!(
            content.contains("model selected dynamically"),
            "expected auto wording: {content}"
        );
    }

    #[test]
    fn test_format_user_context_message_without_model_name() {
        let content = format_user_context_message(
            None,
            std::iter::empty::<(&str, &str)>(),
            std::iter::empty::<&str>(),
            std::iter::empty::<&str>(),
            None,
            None,
            None,
            false,
            None,
            None,
        );
        assert!(
            !content.contains("The current model is"),
            "unexpected model name in context: {content}"
        );
    }
}
