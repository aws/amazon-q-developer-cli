//! Session manager actor for coordinating ACP sessions and client communication.

use std::borrow::Cow;
use std::collections::HashMap;
use std::path::{
    Path,
    PathBuf,
};
use std::sync::Arc;

use agent::agent_config::{
    ConfigSource,
    LoadedAgentConfig,
    ResolvedGlobalPrompt,
    load_agents,
};
use agent::consts::DEFAULT_AGENT_NAME;
use agent::util::providers::RealProvider;
use code_agent_sdk::CodeIntelligence;
use sacp::schema::SessionId;
use sacp::{
    AgentToClient,
    JrConnectionCx,
};
use tokio::sync::{
    RwLock,
    mpsc,
    oneshot,
};
use tracing::{
    debug,
    error,
    warn,
};

use crate::agent::acp::acp_agent::{
    AcpSessionBuilder,
    AcpSessionConfig,
    AcpSessionHandle,
};
use crate::agent::acp::mcp_conversion::convert_mcp_server;
use crate::agent::ipc_server::{
    IpcServer,
    TelemetryEventStore,
};
use crate::api_client::{
    ApiClient,
    MockResponseRegistryHandle,
};
use crate::cli::chat::legacy::model::{
    ModelInfo,
    get_available_models,
};
use crate::database::settings::Setting;
use crate::os::Os;
use crate::util::consts::env_var::KIRO_TEST_MODE;

/// Metadata about an available agent configuration.
#[derive(Debug, Clone, serde::Serialize)]
pub struct AgentInfo {
    pub name: String,
    pub description: Option<String>,
    pub source: String,
    pub welcome_message: Option<String>,
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
    local_mcp_path: Option<PathBuf>,
    global_mcp_path: Option<PathBuf>,
    trust_all_tools: bool,
}

impl SessionManagerBuilder {
    pub fn os(mut self, os: Os) -> Self {
        self.os = Some(os);
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

    pub fn trust_all_tools(mut self, trust: bool) -> Self {
        self.trust_all_tools = trust;
        self
    }

    pub fn spawn(self) -> SessionManagerHandle {
        let (tx, mut session_rx) = mpsc::channel::<SessionManagerRequest>(25);
        let Self {
            os,
            local_mcp_path,
            global_mcp_path,
            trust_all_tools,
        } = self;
        let os = os.expect("Os not found");

        let session_manager_handle = SessionManagerHandle { tx };
        let session_manager_handle_clone = session_manager_handle.clone();

        tokio::spawn(async move {
            // Load agent configs once at startup
            let agent_configs: Vec<LoadedAgentConfig> = match load_agents(&RealProvider).await {
                Ok((configs, errors)) => {
                    for err in &errors {
                        error!(%err, "Failed to load agent config");
                    }
                    configs
                },
                Err(e) => {
                    error!(%e, "Failed to load agents");
                    Vec::new()
                },
            };

            // In test mode, spawn IpcServer and MockResponseRegistry
            let (mock_registry, telemetry_event_store) = if std::env::var(KIRO_TEST_MODE).is_ok() {
                let registry = MockResponseRegistryHandle::spawn();
                let capture = TelemetryEventStore::default();
                if let Err(e) = IpcServer::spawn(registry.clone(), capture.clone()) {
                    error!("Failed to spawn IPC server: {}", e);
                }
                (Some(registry), Some(capture))
            } else {
                (None, None)
            };

            let mut session_manager = SessionManager::new(
                agent_configs,
                os,
                local_mcp_path,
                global_mcp_path,
                session_manager_handle_clone,
                mock_registry,
                trust_all_tools,
                telemetry_event_store,
            );

            loop {
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
/// Manages session lifecycle (creation, retrieval, termination).
#[derive(Clone, Debug)]
pub struct SessionManager {
    sessions: HashMap<SessionId, AcpSessionHandle>,
    agent_configs: Vec<LoadedAgentConfig>,
    os: Os,
    local_mcp_path: Option<PathBuf>,
    global_mcp_path: Option<PathBuf>,
    session_manager_handle: SessionManagerHandle,
    mock_registry: Option<MockResponseRegistryHandle>,
    /// The agent name to use when creating the next session.
    ///
    /// # Context
    ///
    /// Why is this required? In an ACP integration, we want to support launching the CLI with a
    /// `--agent` flag so that a session can be initialized using an agent (ie, ACP mode).
    ///
    /// If session/new supports a `mode` parameter when creating/loading a session, this could
    /// likely be removed.
    next_agent_name: Option<String>,
    /// Model ID to use for the next session, set via `--model` CLI flag.
    next_model_id: Option<String>,
    /// Shared code intelligence clients - lazily initialized per CWD, shared across sessions
    code_intelligence: HashMap<PathBuf, Arc<RwLock<CodeIntelligence>>>,
    /// When true, all tool permission checks are bypassed for new sessions
    trust_all_tools: bool,
    /// ACP client identity from InitializeRequest, propagated to all sessions
    acp_client_info: Option<crate::telemetry::observer::AcpClientInfo>,
    /// Telemetry event store for recording events in test scenarios.
    /// Shared with the IPC server so tests can drain and assert on events. `None` in production.
    telemetry_event_store: Option<TelemetryEventStore>,
}

impl SessionManager {
    pub fn builder() -> SessionManagerBuilder {
        Default::default()
    }

    #[allow(clippy::too_many_arguments)]
    fn new(
        agent_configs: Vec<LoadedAgentConfig>,
        os: Os,
        local_mcp_path: Option<PathBuf>,
        global_mcp_path: Option<PathBuf>,
        session_manager_handle: SessionManagerHandle,
        mock_registry: Option<MockResponseRegistryHandle>,
        trust_all_tools: bool,
        telemetry_event_store: Option<TelemetryEventStore>,
    ) -> Self {
        Self {
            sessions: HashMap::new(),
            agent_configs,
            os,
            local_mcp_path,
            global_mcp_path,
            session_manager_handle,
            mock_registry,
            next_agent_name: None,
            next_model_id: None,
            code_intelligence: HashMap::new(),
            trust_all_tools,
            acp_client_info: None,
            telemetry_event_store,
        }
    }

    /// Get or initialize a CodeIntelligence client for the given CWD.
    /// If `lsp.json` exists, automatically initializes LSP servers in the background.
    fn get_or_init_code_intelligence(&mut self, cwd: &Path) -> Option<Arc<RwLock<CodeIntelligence>>> {
        if let Some(ci) = self.code_intelligence.get(cwd) {
            return Some(ci.clone());
        }
        match CodeIntelligence::builder()
            .workspace_root(cwd.to_path_buf())
            .auto_detect_languages()
            .build()
        {
            Ok(client) => {
                let should_init = client.should_auto_initialize();
                debug!("Initialized CodeIntelligence client for {}", cwd.display());
                let ci = Arc::new(RwLock::new(client));
                if should_init {
                    let ci_clone = ci.clone();
                    tokio::spawn(async move {
                        let mut guard = ci_clone.write().await;
                        if let Err(e) = guard.initialize().await {
                            warn!("Failed to auto-initialize code intelligence: {}", e);
                        }
                    });
                }
                self.code_intelligence.insert(cwd.to_path_buf(), ci.clone());
                Some(ci)
            },
            Err(e) => {
                error!(
                    "Failed to initialize CodeIntelligence for {}: {}. Code tool will be unavailable.",
                    cwd.display(),
                    e
                );
                None
            },
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
            .swap_agent(agent_config.clone())
            .await
            .map_err(|e| sacp::util::internal_error(format!("Failed to swap agent: {}", e)))?;

        Ok(())
    }

    async fn handle_request(&mut self, request: SessionManagerRequest) {
        debug!(?request, "session manager received new request");
        let SessionManagerRequest { session_id, data } = request;

        match data {
            SessionManagerRequestData::StartSession {
                config,
                connection_cx,
                resp_sender,
            } => {
                // Resolve agent name: explicit config > CLI --agent flag > persisted session agent > setting >
                // default
                let persisted_agent = if config.load {
                    let sessions_dir = crate::util::paths::sessions_dir().ok();
                    sessions_dir.and_then(|d| crate::agent::session::peek_agent_name(&d, &config.session_id))
                } else {
                    None
                };
                let agent_name = config
                    .initial_agent_name
                    .clone()
                    .or_else(|| self.next_agent_name.take())
                    .or(persisted_agent)
                    .or_else(|| self.os.database.settings.get_string(Setting::ChatDefaultAgent))
                    .unwrap_or_else(|| agent::consts::DEFAULT_AGENT_NAME.to_string());

                let default_agent = self
                    .agent_configs
                    .iter()
                    .find(|c| c.name() == DEFAULT_AGENT_NAME)
                    .expect("missing default agent");

                let (base_agent_config, agent_name) = match self.agent_configs.iter().find(|c| c.name() == agent_name) {
                    Some(config) => (config, agent_name),
                    None => {
                        warn!("Agent '{}' not found, falling back to default", agent_name);
                        (default_agent, DEFAULT_AGENT_NAME.to_string())
                    },
                };

                // If ACP client provided MCP servers, create an ephemeral config with them merged in
                let agent_config_to_use: LoadedAgentConfig = if !config.mcp_servers.is_empty() {
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

                    // Preserve the resolved global prompt from the base config
                    let resolved_prompt = base_agent_config
                        .global_prompt()
                        .map_or(ResolvedGlobalPrompt::None, ResolvedGlobalPrompt::Resolved);

                    LoadedAgentConfig::new(ephemeral, ConfigSource::BuiltIn, resolved_prompt)
                } else {
                    base_agent_config.clone()
                };

                // Initialize or get shared code intelligence client
                let code_intel = self.get_or_init_code_intelligence(&config.cwd);

                let mut builder = AcpSessionBuilder::default()
                    .os(self.os.clone())
                    .session_id(config.session_id)
                    .cwd(config.cwd.clone())
                    .load(config.load)
                    .local_mcp_path(self.local_mcp_path.as_ref())
                    .global_mcp_path(self.global_mcp_path.as_ref())
                    .initial_agent_config(Cow::Owned(agent_config_to_use))
                    .user_embedded_msg(config.user_embedded_msg.as_deref())
                    .session_tx(self.session_manager_handle.clone())
                    .set_as_subagent(config.is_subagent)
                    .code_intelligence(code_intel)
                    .trust_all_tools(self.trust_all_tools)
                    .acp_client_info(self.acp_client_info.clone())
                    .telemetry_event_store(self.telemetry_event_store.clone());

                // Pass client connection to session (required)
                if let Some(cx) = connection_cx {
                    builder = builder.connection_cx(cx);
                } else {
                    error!("No client connection provided for session");
                    _ = resp_sender.send(Err(sacp::util::internal_error("Missing client connection")));
                    return;
                }

                if let Some(ref registry) = self.mock_registry {
                    builder = builder.mock_registry(registry.clone());
                }

                let mut available_agents: Vec<AgentInfo> = self
                    .agent_configs
                    .iter()
                    .map(|c| AgentInfo {
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
                // Dedupe by name (keep first occurrence)
                let mut seen = std::collections::HashSet::new();
                available_agents.retain(|a| seen.insert(a.name.clone()));

                builder = builder.available_agents(available_agents.clone());
                builder = builder.agent_configs(self.agent_configs.clone());
                builder = builder.current_agent_name(agent_name.clone());

                // Pass CLI --model override to session builder
                let next_model_id = self.next_model_id.take();
                if let Some(ref model_id) = next_model_id {
                    builder = builder.model_id(Some(model_id.as_str()));
                }

                // Fetch available models (use mock client in test mode to avoid network calls)
                let available_models = if let Some(ref registry) = self.mock_registry {
                    let mock_client = ApiClient::new_ipc_mock(registry.clone());
                    mock_client
                        .list_available_models_cached()
                        .await
                        .map(|r| r.models.iter().map(ModelInfo::from_api_model).collect())
                        .unwrap_or_default()
                } else {
                    match get_available_models(&self.os.client).await {
                        Ok((models, _)) => models,
                        Err(e) => {
                            warn!("Failed to fetch available models: {}", e);
                            vec![]
                        },
                    }
                };

                match builder
                    .start_session()
                    .await
                    .map_err(|e| sacp::util::internal_error(format!("Failed to start session: {}", e)))
                {
                    Ok((handle, ready_rx, initial_model_id)) => {
                        let current_model_id = initial_model_id.unwrap_or_default();
                        let handle_to_give = handle.clone();
                        self.sessions.insert(session_id.clone(), handle);
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
                if let Some(handle) = self.sessions.remove(&session_id) {
                    if tokio::time::timeout(std::time::Duration::from_secs(4), handle.shutdown())
                        .await
                        .is_err()
                    {
                        warn!(?session_id, "Session did not shut down within timeout during terminate");
                    }
                } else {
                    warn!(?session_id, "Attempted to terminate non-existent session");
                }
            },
            SessionManagerRequestData::Shutdown { resp_sender } => {
                // Terminate all sessions' agents so MCP child processes are cleaned up
                // before the tokio runtime exits. Each session gets its own timeout so
                // a stuck session doesn't block the others.
                let sessions: Vec<_> = self.sessions.drain().collect();
                let futs: Vec<_> = sessions
                    .iter()
                    .map(|(id, h)| {
                        let id = id.clone();
                        async move {
                            if tokio::time::timeout(std::time::Duration::from_secs(4), h.shutdown())
                                .await
                                .is_err()
                            {
                                warn!(?id, "Session did not shut down within timeout");
                            }
                        }
                    })
                    .collect();
                futures::future::join_all(futs).await;
                _ = resp_sender.send(());
            },
            SessionManagerRequestData::SetMode { mode_id, resp_sender } => {
                let result = self.handle_set_mode(&session_id, &mode_id).await;
                _ = resp_sender.send(result);
            },
            SessionManagerRequestData::SetNextAgentName {
                next_agent_name,
                resp_sender,
            } => {
                self.next_agent_name = Some(next_agent_name);
                _ = resp_sender.send(Ok(()));
            },
            SessionManagerRequestData::SetNextModelId {
                next_model_id,
                resp_sender,
            } => {
                self.next_model_id = Some(next_model_id);
                _ = resp_sender.send(Ok(()));
            },
            SessionManagerRequestData::Initialize {
                name,
                version,
                resp_sender,
            } => {
                self.acp_client_info = Some(crate::telemetry::observer::AcpClientInfo::new(name, version));
                _ = resp_sender.send(Ok(()));
            },
            SessionManagerRequestData::ListSessions { cwd, resp_sender } => {
                let result = crate::agent::session::list_sessions(cwd.as_deref());
                _ = resp_sender.send(result);
            },
            SessionManagerRequestData::GetCodeIntelligence { cwd, resp_sender } => {
                let ci = self.get_or_init_code_intelligence(&cwd);
                _ = resp_sender.send(ci);
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
    Shutdown {
        resp_sender: oneshot::Sender<()>,
    },
    SetMode {
        mode_id: String,
        resp_sender: oneshot::Sender<Result<(), sacp::Error>>,
    },
    SetNextAgentName {
        next_agent_name: String,
        resp_sender: oneshot::Sender<Result<(), sacp::Error>>,
    },
    SetNextModelId {
        next_model_id: String,
        resp_sender: oneshot::Sender<Result<(), sacp::Error>>,
    },
    Initialize {
        name: String,
        version: String,
        resp_sender: oneshot::Sender<Result<(), sacp::Error>>,
    },
    ListSessions {
        cwd: Option<PathBuf>,
        resp_sender:
            oneshot::Sender<Result<Vec<crate::agent::session::SessionDataView>, crate::agent::session::SessionError>>,
    },
    GetCodeIntelligence {
        cwd: PathBuf,
        resp_sender: oneshot::Sender<Option<Arc<RwLock<CodeIntelligence>>>>,
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

    pub async fn terminate_session(&self, session_id: &SessionId) {
        let _ = self
            .tx
            .send(SessionManagerRequest {
                session_id: session_id.clone(),
                data: SessionManagerRequestData::TerminateSession,
            })
            .await;
    }

    /// Gracefully shut down all sessions, awaiting MCP server cleanup.
    pub async fn shutdown(&self) {
        let (resp_sender, rx) = oneshot::channel();
        let _ = self
            .tx
            .send(SessionManagerRequest {
                session_id: SessionId::new(String::new()),
                data: SessionManagerRequestData::Shutdown { resp_sender },
            })
            .await;
        _ = rx.await;
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

    pub async fn set_next_agent_name(&self, next_agent_name: String) -> Result<(), sacp::Error> {
        let (resp_sender, rx) = oneshot::channel();
        self.tx
            .send(SessionManagerRequest {
                // TODO - refactor request type to just be an enum, and move session_id into each
                // enum variant
                session_id: SessionId::new(String::new()),
                data: SessionManagerRequestData::SetNextAgentName {
                    next_agent_name,
                    resp_sender,
                },
            })
            .await
            .map_err(|_e| sacp::util::internal_error("Failed to send set_next_agent_name request"))?;
        rx.await
            .map_err(|_e| sacp::util::internal_error("Failed to receive set_next_agent_name response"))?
    }

    pub async fn set_next_model_id(&self, next_model_id: String) -> Result<(), sacp::Error> {
        let (resp_sender, rx) = oneshot::channel();
        self.tx
            .send(SessionManagerRequest {
                session_id: SessionId::new(String::new()),
                data: SessionManagerRequestData::SetNextModelId {
                    next_model_id,
                    resp_sender,
                },
            })
            .await
            .map_err(|_e| sacp::util::internal_error("Failed to send set_next_model_id request"))?;
        rx.await
            .map_err(|_e| sacp::util::internal_error("Failed to receive set_next_model_id response"))?
    }

    pub async fn initialize(&self, name: String, version: String) -> Result<(), sacp::Error> {
        let (resp_sender, rx) = oneshot::channel();
        self.tx
            .send(SessionManagerRequest {
                session_id: SessionId::new(String::new()), // session-agnostic request; empty ID is intentional
                data: SessionManagerRequestData::Initialize {
                    name,
                    version,
                    resp_sender,
                },
            })
            .await
            .map_err(|_e| sacp::util::internal_error("Failed to send initialize request"))?;
        rx.await
            .map_err(|_e| sacp::util::internal_error("Failed to receive initialize response"))?
    }

    /// Lists available sessions, filtered by cwd if provided.
    pub async fn list_sessions(
        &self,
        cwd: Option<PathBuf>,
    ) -> Result<Vec<crate::agent::session::SessionDataView>, sacp::Error> {
        let (resp_sender, rx) = oneshot::channel();
        self.tx
            .send(SessionManagerRequest {
                session_id: SessionId::new(String::new()),
                data: SessionManagerRequestData::ListSessions { cwd, resp_sender },
            })
            .await
            .map_err(|_e| sacp::util::internal_error("Failed to send list_sessions request"))?;
        rx.await
            .map_err(|_e| sacp::util::internal_error("Failed to receive list_sessions response"))?
            .map_err(|e| sacp::util::internal_error(format!("Failed to list sessions: {}", e)))
    }

    pub async fn get_code_intelligence(&self, cwd: PathBuf) -> Option<Arc<RwLock<CodeIntelligence>>> {
        let (resp_sender, rx) = oneshot::channel();
        self.tx
            .send(SessionManagerRequest {
                session_id: SessionId::new(String::new()),
                data: SessionManagerRequestData::GetCodeIntelligence { cwd, resp_sender },
            })
            .await
            .ok()?;
        rx.await.ok()?
    }
}
