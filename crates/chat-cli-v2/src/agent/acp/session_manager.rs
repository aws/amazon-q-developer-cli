//! Session manager actor for coordinating ACP sessions and client communication.

use std::borrow::Cow;
use std::collections::HashMap;
use std::path::PathBuf;

use agent::AgentHandle;
use agent::agent_config::{
    ConfigSource,
    LoadedAgentConfig,
    load_agents,
};
use agent::agent_loop::types::ToolUseBlock;
use agent::consts::DEFAULT_AGENT_NAME;
use sacp::schema::{
    SessionId,
    SessionNotification,
    SessionUpdate,
    ToolCallId,
    ToolCallUpdate,
};
use sacp::{
    AgentToClient,
    JrConnectionCx,
};
use tokio::sync::{
    mpsc,
    oneshot,
};
use tracing::{
    error,
    warn,
};

use crate::agent::acp::acp_agent::{
    AcpSessionBuilder,
    AcpSessionConfig,
    AcpSessionHandle,
};
use crate::agent::acp::mcp_conversion::convert_mcp_server;
use crate::agent::ipc_server::IpcServer;
use crate::api_client::MockResponseRegistryHandle;
use crate::cli::chat::legacy::model::{
    ModelInfo,
    get_available_models,
};
use crate::database::settings::Setting;
use crate::os::Os;

/// Metadata about an available agent configuration.
#[derive(Debug, Clone)]
pub struct AgentInfo {
    pub name: String,
    pub description: Option<String>,
}

/// Result returned when starting or loading a session.
#[derive(Debug)]
pub struct StartSessionResult {
    pub handle: AcpSessionHandle,
    /// Resolves when the session is ready to accept prompts.
    pub ready_rx: oneshot::Receiver<()>,
    pub current_agent_name: String,
    pub available_agents: Vec<AgentInfo>,
    pub available_models: Vec<ModelInfo>,
    pub current_model_id: String,
}

/// Builder for constructing and spawning a [`SessionManager`] actor.
#[derive(Clone, Debug, Default)]
pub struct SessionManagerBuilder {
    os: Option<Os>,
    local_agent_path: Option<PathBuf>,
    global_agent_path: Option<PathBuf>,
    local_mcp_path: Option<PathBuf>,
    global_mcp_path: Option<PathBuf>,
}

impl SessionManagerBuilder {
    pub fn os(mut self, os: Os) -> Self {
        self.os = Some(os);
        self
    }

    pub fn local_agent_path(mut self, path: Option<PathBuf>) -> Self {
        self.local_agent_path = path;
        self
    }

    pub fn global_agent_path(mut self, path: Option<PathBuf>) -> Self {
        self.global_agent_path = path;
        self
    }

    pub fn local_mcp_path(mut self, path: Option<PathBuf>) -> Self {
        self.local_mcp_path = path;
        self
    }

    pub fn global_mcp_path(mut self, path: Option<PathBuf>) -> Self {
        self.global_mcp_path = path;
        self
    }

    pub fn spawn(self) -> SessionManagerHandle {
        let (tx, mut session_rx) = mpsc::channel::<SessionManagerRequest>(25);
        let Self {
            os,
            local_agent_path,
            global_agent_path,
            local_mcp_path,
            global_mcp_path,
        } = self;
        let os = os.expect("Os not found");

        let session_manager_handle = SessionManagerHandle { tx };
        let session_manager_handle_clone = session_manager_handle.clone();

        tokio::spawn(async move {
            // Load agent configs once at startup
            let agent_configs: Vec<LoadedAgentConfig> = match (local_agent_path.as_ref(), global_agent_path.as_ref()) {
                (Some(local), Some(global)) => load_agents(local, global).await.map(|(c, _)| c).unwrap_or_default(),
                _ => Vec::new(),
            };

            // In test mode, spawn IpcServer and MockResponseRegistry
            let mock_registry = if std::env::var("KIRO_TEST_MODE").is_ok() {
                let registry = MockResponseRegistryHandle::spawn();
                if let Err(e) = IpcServer::spawn(registry.clone()) {
                    error!("Failed to spawn IPC server: {}", e);
                }
                Some(registry)
            } else {
                None
            };

            let mut session_manager = SessionManager::new(
                agent_configs,
                os,
                local_mcp_path,
                global_mcp_path,
                session_manager_handle_clone,
                mock_registry,
            );

            loop {
                // Flush any buffered notifications at the start of each iteration
                session_manager.flush_notifications();

                tokio::select! {
                    req = session_rx.recv() => {
                        let Some(request) = req else {
                            error!("Failed to receive session manager request");
                            break;
                        };
                        session_manager.handle_request(request).await;
                    }
                }
            }
        });

        session_manager_handle
    }
}

/// Central coordinator that owns all active ACP sessions.
///
/// Routes requests to the appropriate session and sends notifications to the ACP client.
#[derive(Clone, Debug)]
pub struct SessionManager {
    sessions: HashMap<SessionId, AcpSessionHandle>,
    client_connection: Option<JrConnectionCx<AgentToClient>>,
    agent_configs: Vec<LoadedAgentConfig>,
    os: Os,
    local_mcp_path: Option<PathBuf>,
    global_mcp_path: Option<PathBuf>,
    session_manager_handle: SessionManagerHandle,
    mock_registry: Option<MockResponseRegistryHandle>,
    /// Buffer for notifications to be sent to the client at the start of the next loop iteration
    notification_buf: Vec<SessionNotification>,
    /// Per-session set of trusted tool names (tools that user selected "Trust" for)
    trusted_tools: HashMap<SessionId, std::collections::HashSet<String>>,
}

impl SessionManager {
    pub fn builder() -> SessionManagerBuilder {
        Default::default()
    }

    fn new(
        agent_configs: Vec<LoadedAgentConfig>,
        os: Os,
        local_mcp_path: Option<PathBuf>,
        global_mcp_path: Option<PathBuf>,
        session_manager_handle: SessionManagerHandle,
        mock_registry: Option<MockResponseRegistryHandle>,
    ) -> Self {
        Self {
            sessions: HashMap::new(),
            client_connection: None,
            agent_configs,
            os,
            local_mcp_path,
            global_mcp_path,
            session_manager_handle,
            mock_registry,
            notification_buf: Vec::new(),
            trusted_tools: HashMap::new(),
        }
    }

    /// Drains and sends all buffered notifications to the client.
    fn flush_notifications(&mut self) {
        if let Some(ref cx) = self.client_connection {
            for notification in self.notification_buf.drain(..) {
                if let Err(e) = cx.send_notification(notification) {
                    error!(?e, "Failed to send buffered notification");
                }
            }
        } else {
            // No client connection, just clear the buffer
            self.notification_buf.clear();
        }
    }

    async fn handle_set_mode(&self, session_id: &SessionId, mode_id: &str) -> Result<(), sacp::Error> {
        let session = self
            .sessions
            .get(session_id)
            .ok_or_else(|| sacp::util::internal_error("Session not found"))?;

        let agent_config = self
            .agent_configs
            .iter()
            .find(|c| c.name() == mode_id)
            .ok_or_else(|| sacp::util::internal_error(format!("Mode '{}' not found", mode_id)))?;

        session
            .swap_agent(agent_config.config().clone())
            .await
            .map_err(|e| sacp::util::internal_error(format!("Failed to swap agent: {}", e)))?;

        Ok(())
    }

    async fn handle_request(&mut self, request: SessionManagerRequest) {
        let SessionManagerRequest { session_id, data } = request;

        match data {
            SessionManagerRequestData::StartSession {
                config,
                connection_cx,
                resp_sender,
            } => {
                // Resolve agent name: explicit > setting > default
                let agent_name = config
                    .initial_agent_name
                    .clone()
                    .or_else(|| self.os.database.settings.get_string(Setting::ChatDefaultAgent))
                    .unwrap_or_else(|| agent::consts::DEFAULT_AGENT_NAME.to_string());

                let default_agent = self
                    .agent_configs
                    .iter()
                    .find(|c| c.name() == DEFAULT_AGENT_NAME)
                    .expect("missing default agent");

                let base_agent_config = self
                    .agent_configs
                    .iter()
                    .find(|c| c.name() == agent_name)
                    .unwrap_or(default_agent);

                // If ACP client provided MCP servers, create an ephemeral config with them merged in
                let agent_config_to_use: Cow<'_, LoadedAgentConfig> = if !config.mcp_servers.is_empty() {
                    let mut ephemeral = base_agent_config.config().clone();

                    // Convert ACP MCP servers to agent configs
                    let converted: Vec<_> = config
                        .mcp_servers
                        .into_iter()
                        .filter_map(|server| match convert_mcp_server(server) {
                            Ok((name, cfg)) => Some((name, cfg)),
                            Err(e) => {
                                warn!(?e, "Failed to convert MCP server, skipping");
                                None
                            },
                        })
                        .collect();

                    if let Some(overridden) = ephemeral.add_mcp_servers(converted) {
                        warn!(?overridden, "ACP MCP servers override existing servers in agent config");
                    }

                    let loaded = LoadedAgentConfig::new(ephemeral, ConfigSource::BuiltIn);

                    Cow::Owned(loaded)
                } else {
                    Cow::Borrowed(base_agent_config)
                };

                let mut builder = AcpSessionBuilder::default()
                    .os(self.os.clone())
                    .session_id(config.session_id)
                    .cwd(config.cwd)
                    .load(config.load)
                    .local_mcp_path(self.local_mcp_path.as_ref())
                    .global_mcp_path(self.global_mcp_path.as_ref())
                    .initial_agent_config(agent_config_to_use)
                    .user_embedded_msg(config.user_embedded_msg.as_deref())
                    .session_tx(self.session_manager_handle.clone())
                    .set_as_subagent(config.is_subagent);

                if let Some(ref registry) = self.mock_registry {
                    builder = builder.mock_registry(registry.clone());
                }

                let available_agents: Vec<AgentInfo> = self
                    .agent_configs
                    .iter()
                    .map(|c| AgentInfo {
                        name: c.name().to_string(),
                        description: c.config().description().map(|s| s.to_string()),
                    })
                    .collect();

                // Fetch available models
                let available_models = match get_available_models(&self.os).await {
                    Ok((models, _)) => models,
                    Err(e) => {
                        warn!("Failed to fetch available models: {}", e);
                        vec![]
                    },
                };

                match builder
                    .start_session()
                    .await
                    .map_err(|e| sacp::util::internal_error(format!("Failed to start session: {}", e)))
                {
                    Ok((handle, ready_rx)) => {
                        let current_model_id = handle.get_model_id().await.unwrap_or_default();
                        let handle_to_give = handle.clone();
                        self.sessions.insert(session_id.clone(), handle);
                        // Register the client connection if provided
                        if let Some(cx) = connection_cx
                            && self.client_connection.is_none()
                        {
                            self.client_connection.replace(cx);
                        }
                        _ = resp_sender.send(Ok(StartSessionResult {
                            handle: handle_to_give,
                            ready_rx,
                            current_agent_name: agent_name,
                            available_agents,
                            available_models,
                            current_model_id,
                        }));
                    },
                    Err(e) => {
                        _ = resp_sender.send(Err(e));
                    },
                }
            },
            SessionManagerRequestData::GetSessionHandle { resp_sender } => {
                let maybe_session = self
                    .sessions
                    .get(&session_id)
                    .ok_or(sacp::util::internal_error("No session found with id"));
                match maybe_session {
                    Ok(handle) => {
                        let handle_to_give = handle.clone();
                        _ = resp_sender.send(Ok(handle_to_give));
                    },
                    Err(e) => _ = resp_sender.send(Err(e)),
                }
            },
            SessionManagerRequestData::TerminateSession => {
                if self.sessions.remove(&session_id).is_none() {
                    warn!(?session_id, "Attempted to terminate non-existent session");
                }
            },
            SessionManagerRequestData::CancelSession => {
                if let Some(session_handle) = self.sessions.get(&session_id) {
                    // Cancel the session - the agent handles all internal cancellation logic
                    let _ = session_handle.cancel().await;
                } else {
                    warn!(?session_id, "Attempted to cancel non-existent session");
                }
            },
            SessionManagerRequestData::SendNotification { update } => {
                if let Some(cx) = self.client_connection.as_ref() {
                    if let Err(e) = cx.send_notification(SessionNotification::new(session_id.clone(), *update)) {
                        warn!("Failed to send notification to {}: {}", session_id, e);
                    }
                } else {
                    warn!("No client connection found for session {}", session_id);
                }
            },
            SessionManagerRequestData::SendMetadata {
                context_usage_percentage,
            } => {
                if let Some(cx) = self.client_connection.as_ref() {
                    let notification = super::schema::MetadataNotification {
                        session_id: session_id.to_string(),
                        context_usage_percentage,
                    };
                    if let Err(e) = cx.send_notification(notification) {
                        warn!("Failed to send metadata notification: {}", e);
                    }
                }
            },
            SessionManagerRequestData::SendExtNotification { method, params } => {
                if let Some(cx) = self.client_connection.as_ref() {
                    let params_raw = match serde_json::value::to_raw_value(&params) {
                        Ok(raw) => raw,
                        Err(e) => {
                            warn!("Failed to serialize extension notification params: {}", e);
                            return;
                        },
                    };
                    let ext_notification = sacp::schema::ExtNotification::new(method, std::sync::Arc::from(params_raw));
                    let notification = sacp::schema::AgentNotification::ExtNotification(ext_notification);
                    if let Err(e) = cx.send_notification(notification) {
                        warn!("Failed to send extension notification: {}", e);
                    }
                } else {
                    warn!("No client connection found for extension notification");
                }
            },
            SessionManagerRequestData::ToolUseApprovalRequest {
                tool_use_id,
                tool_use,
                agent_handle,
            } => {
                let Some(cx) = self.client_connection.as_ref() else {
                    warn!(?session_id, "No client connection found for tool approval request");
                    return;
                };

                // Check if this tool is already trusted for this session
                let is_trusted = self
                    .trusted_tools
                    .get(&session_id)
                    .is_some_and(|tools| tools.contains(&tool_use.name));

                if is_trusted {
                    // Auto-approve trusted tools
                    if let Err(e) = agent_handle
                        .send_tool_use_approval_result(agent::protocol::SendApprovalResultArgs {
                            id: tool_use_id,
                            result: agent::protocol::ApprovalResult::Approve,
                        })
                        .await
                    {
                        error!("Failed to send auto-approval for trusted tool: {}", e);
                    }
                } else {
                    // Create a channel for the approval handler to notify us of trust decisions
                    let (trust_tx, mut trust_rx) = mpsc::channel::<String>(1);
                    let session_manager_handle = self.session_manager_handle.clone();
                    let session_id_clone = session_id.clone();

                    // Spawn a task to handle trust notifications
                    tokio::spawn(async move {
                        if let Some(tool_name) = trust_rx.recv().await {
                            session_manager_handle
                                .add_trusted_tool(&session_id_clone, tool_name)
                                .await;
                        }
                    });

                    tokio::spawn(handle_approval_request(
                        tool_use_id,
                        tool_use,
                        agent_handle,
                        cx.clone(),
                        session_id,
                        trust_tx,
                    ));
                }
            },
            SessionManagerRequestData::SetMode { mode_id, resp_sender } => {
                let result = self.handle_set_mode(&session_id, &mode_id).await;
                _ = resp_sender.send(result);
            },
            SessionManagerRequestData::AddTrustedTool { tool_name } => {
                self.trusted_tools.entry(session_id).or_default().insert(tool_name);
            },
        }
    }
}

/// Messages that can be sent to a [`SessionManager`] actor.
#[derive(Debug)]
pub(crate) struct SessionManagerRequest {
    pub session_id: SessionId,
    pub data: SessionManagerRequestData,
}

/// Payload variants for [`SessionManagerRequest`].
#[derive(Debug)]
pub(crate) enum SessionManagerRequestData {
    StartSession {
        config: AcpSessionConfig,
        connection_cx: Option<JrConnectionCx<AgentToClient>>,
        resp_sender: oneshot::Sender<Result<StartSessionResult, sacp::Error>>,
    },
    GetSessionHandle {
        resp_sender: oneshot::Sender<Result<AcpSessionHandle, sacp::Error>>,
    },
    TerminateSession,
    CancelSession,
    SendNotification {
        update: Box<SessionUpdate>,
    },
    SendMetadata {
        context_usage_percentage: Option<f32>,
    },
    SendExtNotification {
        method: String,
        params: serde_json::Value,
    },
    ToolUseApprovalRequest {
        tool_use_id: String,
        tool_use: ToolUseBlock,
        agent_handle: AgentHandle,
    },
    SetMode {
        mode_id: String,
        resp_sender: oneshot::Sender<Result<(), sacp::Error>>,
    },
    AddTrustedTool {
        tool_name: String,
    },
}

/// Handle for communicating with a [`SessionManager`] actor.
#[derive(Clone, Debug)]
pub struct SessionManagerHandle {
    tx: mpsc::Sender<SessionManagerRequest>,
}

impl SessionManagerHandle {
    pub async fn start_session(
        &self,
        session_id: &SessionId,
        config: AcpSessionConfig,
        connection_cx: Option<JrConnectionCx<AgentToClient>>,
    ) -> Result<StartSessionResult, sacp::Error> {
        let (resp_sender, rx) = oneshot::channel();
        self.tx
            .send(SessionManagerRequest {
                session_id: session_id.clone(),
                data: SessionManagerRequestData::StartSession {
                    config,
                    connection_cx,
                    resp_sender,
                },
            })
            .await
            .map_err(|_e| sacp::util::internal_error("Failed to send session request"))?;
        rx.await
            .map_err(|_e| sacp::util::internal_error("Failed to receive session response"))?
    }

    pub async fn get_session_handle(&self, session_id: &SessionId) -> Result<AcpSessionHandle, sacp::Error> {
        let (resp_sender, rx) = oneshot::channel();
        self.tx
            .send(SessionManagerRequest {
                session_id: session_id.clone(),
                data: SessionManagerRequestData::GetSessionHandle { resp_sender },
            })
            .await
            .map_err(|_e| sacp::util::internal_error("Failed to send session request"))?;
        rx.await
            .map_err(|_e| sacp::util::internal_error("Failed to receive session response"))?
    }

    pub async fn send_notification(&self, update: SessionUpdate, session_id: SessionId) -> Result<(), sacp::Error> {
        self.tx
            .send(SessionManagerRequest {
                session_id,
                data: SessionManagerRequestData::SendNotification {
                    update: Box::new(update),
                },
            })
            .await
            .map_err(|_e| sacp::util::internal_error("Failed to send notification"))?;
        Ok(())
    }

    pub async fn send_turn_metadata(
        &self,
        session_id: SessionId,
        metadata: &agent::agent_loop::protocol::UserTurnMetadata,
    ) -> Result<(), sacp::Error> {
        self.tx
            .send(SessionManagerRequest {
                session_id,
                data: SessionManagerRequestData::SendMetadata {
                    context_usage_percentage: metadata.context_usage_percentage,
                },
            })
            .await
            .map_err(|_e| sacp::util::internal_error("Failed to send metadata"))?;
        Ok(())
    }

    pub async fn send_ext_notification<T: serde::Serialize>(
        &self,
        method: &str,
        params: T,
        session_id: SessionId,
    ) -> Result<(), sacp::Error> {
        let params_value = serde_json::to_value(params)
            .map_err(|e| sacp::util::internal_error(format!("Failed to serialize params: {}", e)))?;
        self.tx
            .send(SessionManagerRequest {
                session_id,
                data: SessionManagerRequestData::SendExtNotification {
                    method: method.to_string(),
                    params: params_value,
                },
            })
            .await
            .map_err(|_e| sacp::util::internal_error("Failed to send extension notification"))?;
        Ok(())
    }

    pub async fn terminate_session(&self, session_id: &SessionId) {
        let _ = self
            .tx
            .send(SessionManagerRequest {
                session_id: session_id.clone(),
                data: SessionManagerRequestData::TerminateSession,
            })
            .await;
    }

    pub async fn cancel_session(&self, session_id: &SessionId) {
        let _ = self
            .tx
            .send(SessionManagerRequest {
                session_id: session_id.clone(),
                data: SessionManagerRequestData::CancelSession,
            })
            .await;
    }

    pub async fn request_permission(
        &self,
        tool_use_id: String,
        tool_use: ToolUseBlock,
        session_id: SessionId,
        agent_handle: AgentHandle,
    ) {
        let _ = self
            .tx
            .send(SessionManagerRequest {
                session_id,
                data: SessionManagerRequestData::ToolUseApprovalRequest {
                    tool_use_id,
                    tool_use,
                    agent_handle,
                },
            })
            .await;
    }

    pub async fn set_mode(&self, session_id: &SessionId, mode_id: String) -> Result<(), sacp::Error> {
        let (resp_sender, rx) = oneshot::channel();
        self.tx
            .send(SessionManagerRequest {
                session_id: session_id.clone(),
                data: SessionManagerRequestData::SetMode { mode_id, resp_sender },
            })
            .await
            .map_err(|_e| sacp::util::internal_error("Failed to send set_mode request"))?;
        rx.await
            .map_err(|_e| sacp::util::internal_error("Failed to receive set_mode response"))?
    }

    pub async fn add_trusted_tool(&self, session_id: &SessionId, tool_name: String) {
        let _ = self
            .tx
            .send(SessionManagerRequest {
                session_id: session_id.clone(),
                data: SessionManagerRequestData::AddTrustedTool { tool_name },
            })
            .await;
    }
}

// Permission option IDs for tool approval
mod permission_options {
    pub const ALLOW_ONCE: &str = "allow_once";
    pub const REJECT_ONCE: &str = "reject_once";
    pub const ALLOW_ALWAYS: &str = "allow_always";
}

async fn handle_approval_request(
    id: String,
    tool_use: ToolUseBlock,
    agent: AgentHandle,
    cx: JrConnectionCx<AgentToClient>,
    session_id: SessionId,
    trust_tx: mpsc::Sender<String>,
) -> Result<(), sacp::Error> {
    use sacp::schema::{
        PermissionOption,
        PermissionOptionKind,
        RequestPermissionRequest,
        ToolCallUpdateFields,
    };

    let tool_name = tool_use.name.clone();

    // Send permission request to client
    let response = cx
        .send_request(RequestPermissionRequest::new(
            session_id,
            ToolCallUpdate::new(
                ToolCallId::new(id.clone()),
                ToolCallUpdateFields::new().title(Some(tool_use.name.clone())),
            ),
            vec![
                PermissionOption::new(permission_options::ALLOW_ONCE, "Yes", PermissionOptionKind::AllowOnce),
                PermissionOption::new(permission_options::REJECT_ONCE, "No", PermissionOptionKind::RejectOnce),
                PermissionOption::new(
                    permission_options::ALLOW_ALWAYS,
                    "Trust",
                    PermissionOptionKind::AllowAlways,
                ),
            ],
        ))
        .block_task()
        .await;

    let res = match response {
        Ok(resp) => resp,
        Err(e) => {
            error!("Failed to get permission response: {:?}", e);
            return Ok(());
        },
    };

    let (approval_result, should_trust) = match res.outcome {
        sacp::schema::RequestPermissionOutcome::Selected(selected) => match selected.option_id.0.as_ref() {
            id if id == permission_options::REJECT_ONCE => (
                agent::protocol::ApprovalResult::Deny {
                    reason: Some("User denied tool execution".to_string()),
                },
                false,
            ),
            id if id == permission_options::ALLOW_ALWAYS => (agent::protocol::ApprovalResult::Approve, true),
            _ => (agent::protocol::ApprovalResult::Approve, false),
        },
        sacp::schema::RequestPermissionOutcome::Cancelled => (
            agent::protocol::ApprovalResult::Deny {
                reason: Some("User cancelled".to_string()),
            },
            false,
        ),
        _ => (
            agent::protocol::ApprovalResult::Deny {
                reason: Some("Unknown response".to_string()),
            },
            false,
        ),
    };

    // If user selected "Trust", notify the session manager to add this tool to trusted list
    if should_trust {
        let _ = trust_tx.send(tool_name).await;
    }

    agent
        .send_tool_use_approval_result(agent::protocol::SendApprovalResultArgs {
            id,
            result: approval_result,
        })
        .await
        .map_err(|e| sacp::util::internal_error(format!("Failed to send approval result: {}", e)))
}
