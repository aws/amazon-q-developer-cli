//! Session manager actor for coordinating ACP sessions and client communication.

use std::borrow::Cow;
use std::collections::{
    HashMap,
    HashSet,
};
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
use agent::tools::session::{
    GroupAction,
    SessionFilter,
};
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
    info,
    warn,
};

use crate::agent::acp::acp_agent::{
    AcpSessionBuilder,
    AcpSessionConfig,
    AcpSessionHandle,
};
use crate::agent::acp::extensions::SubagentInfo;
use crate::agent::acp::mcp_conversion::convert_mcp_server;
use crate::agent::acp::orchestration::inbox::InboxStore;
use crate::agent::acp::orchestration::naming;
use crate::agent::acp::orchestration::permissions::PermissionStore;
use crate::agent::acp::orchestration::types::{
    GroupMembership,
    InboxMessage,
    OrchestratedSession,
    SessionGroup,
    SessionStatus,
};
use crate::agent::ipc_server::{
    IpcServer,
    TelemetryEventStore,
};
use crate::agent::session::legacy_compat::LegacySessionExporter;
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
    /// The agent name originally requested (before fallback). `None` if no
    /// specific agent was requested or the requested agent was found.
    pub requested_agent_name: Option<String>,
    pub available_agents: Vec<AgentInfo>,
    pub available_models: Vec<ModelInfo>,
    pub current_model_id: String,
    /// Agent config errors encountered during loading.
    pub agent_config_errors: Vec<AgentConfigLoadError>,
    /// The model name originally requested (before fallback). `None` if no
    /// specific model was requested or the requested model was found.
    pub requested_model_name: Option<String>,
}

/// Result returned when spawning an orchestrated session.
#[derive(Debug, Clone)]
pub struct SpawnOrchestratedResult {
    pub session_id: String,
    pub name: String,
}

/// Builder for constructing and spawning a [`SessionManager`] actor.
#[derive(Clone, Default)]
pub struct SessionManagerBuilder {
    os: Option<Os>,
    local_mcp_path: Option<PathBuf>,
    global_mcp_path: Option<PathBuf>,
    trust_all_tools: bool,
    trust_tools: Option<Vec<String>>,
    legacy_session_exporter: Option<Arc<dyn LegacySessionExporter>>,
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

    pub fn trust_tools(mut self, tools: Option<Vec<String>>) -> Self {
        self.trust_tools = tools;
        self
    }

    pub fn legacy_session_exporter(mut self, exporter: Arc<dyn LegacySessionExporter>) -> Self {
        self.legacy_session_exporter = Some(exporter);
        self
    }

    pub fn spawn(self) -> SessionManagerHandle {
        let (tx, mut session_rx) = mpsc::channel::<SessionManagerRequest>(25);
        let Self {
            os,
            local_mcp_path,
            global_mcp_path,
            trust_all_tools,
            trust_tools,
            legacy_session_exporter,
        } = self;
        let os = os.expect("Os not found");
        let legacy_session_exporter = legacy_session_exporter.expect("LegacySessionExporter not set");

        let session_manager_handle = SessionManagerHandle { tx };
        let session_manager_handle_clone = session_manager_handle.clone();

        tokio::spawn(async move {
            // Load agent configs once at startup
            let (agent_configs, agent_config_errors): (Vec<LoadedAgentConfig>, Vec<AgentConfigLoadError>) =
                match load_agents(&RealProvider).await {
                    Ok((configs, errors)) => {
                        let structured: Vec<AgentConfigLoadError> = errors
                            .iter()
                            .map(|e| match e {
                                agent::agent_config::AgentConfigError::InvalidAgentConfig { path, message } => {
                                    AgentConfigLoadError {
                                        path: Some(path.clone()),
                                        message: message.clone(),
                                    }
                                },
                                other => AgentConfigLoadError {
                                    path: None,
                                    message: other.to_string(),
                                },
                            })
                            .collect();
                        for err in &errors {
                            error!(%err, "Failed to load agent config");
                        }
                        (configs, structured)
                    },
                    Err(e) => {
                        error!(%e, "Failed to load agents");
                        (Vec::new(), vec![AgentConfigLoadError {
                            path: None,
                            message: e.to_string(),
                        }])
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

            // Fetch MCP registry data for enterprise users
            // Skip in test mode — no real auth available, and the real API client
            // would fail with Err causing an empty registry that strips all MCP servers.
            let (mcp_registry_data, mcp_registry_url) = if std::env::var(KIRO_TEST_MODE).is_ok() {
                (None, None)
            } else {
                match os.client.get_mcp_config().await {
                    Ok((enabled, Some(registry_url))) if enabled => {
                        let client = crate::mcp_registry::McpRegistryClient::new();
                        match client.fetch_registry(&registry_url).await {
                            Ok(registry) => {
                                info!(
                                    servers = registry.servers.len(),
                                    "Fetched MCP registry from {}", registry_url
                                );
                                (Some(registry), Some(registry_url))
                            },
                            Err(e) => {
                                error!(%e, "Failed to fetch MCP registry — registry servers disabled for this session");
                                // Registry URL was configured but fetch failed — use empty registry
                                // to disable registry-dependent MCP servers (matches V1 behavior)
                                (
                                    Some(crate::mcp_registry::McpRegistryResponse { servers: vec![] }),
                                    Some(registry_url),
                                )
                            },
                        }
                    },
                    Ok(_) => (None, None),
                    Err(e) => {
                        error!(%e, "Failed to get MCP config from API — MCP registry features disabled");
                        // API call failed — we can't determine if registry is configured,
                        // so use empty registry to disable registry-dependent MCP servers
                        (Some(crate::mcp_registry::McpRegistryResponse { servers: vec![] }), None)
                    },
                }
            };

            // Spawn background task to refresh registry every 24 hours
            if mcp_registry_data.is_some()
                && let Some(url) = mcp_registry_url
            {
                let sm_handle = session_manager_handle_clone.clone();
                tokio::spawn(async move {
                    loop {
                        tokio::time::sleep(std::time::Duration::from_secs(24 * 60 * 60)).await;
                        let client = crate::mcp_registry::McpRegistryClient::new();
                        match client.fetch_registry(&url).await {
                            Ok(registry) => {
                                info!(
                                    servers = registry.servers.len(),
                                    "Background registry refresh succeeded"
                                );
                                if let Err(e) = sm_handle.refresh_registry(registry).await {
                                    error!(%e, "Failed to send registry refresh to SessionManager");
                                }
                            },
                            Err(e) => {
                                error!(%e, "Background registry refresh failed — disabling registry servers until next refresh");
                                // Clear registry so new sessions/swaps won't use stale data
                                let empty = crate::mcp_registry::McpRegistryResponse { servers: vec![] };
                                if let Err(e) = sm_handle.refresh_registry(empty).await {
                                    error!(%e, "Failed to send empty registry to SessionManager");
                                }
                            },
                        }
                    }
                });
            }

            let mut session_manager = SessionManager::new(
                agent_configs,
                agent_config_errors,
                os,
                local_mcp_path,
                global_mcp_path,
                session_manager_handle_clone,
                mock_registry,
                trust_all_tools,
                trust_tools,
                telemetry_event_store,
                legacy_session_exporter,
                mcp_registry_data,
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
/// Sender for group completion notifications: Vec of (session_name, optional summary).
type GroupCompletionSender = oneshot::Sender<Vec<(String, Option<String>)>>;

/// Manages session lifecycle (creation, retrieval, termination).
#[derive(Debug)]
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
    /// Specific tools to trust for new sessions (from --trust-tools CLI flag)
    trust_tools: Option<Vec<String>>,
    /// ACP client identity from InitializeRequest, propagated to all sessions
    acp_client_info: Option<crate::telemetry::observer::AcpClientInfo>,
    /// Telemetry event store for recording events in test scenarios.
    /// Shared with the IPC server so tests can drain and assert on events. `None` in production.
    telemetry_event_store: Option<TelemetryEventStore>,
    /// Orchestration: inbox storage for inter-session messaging
    inbox_store: InboxStore,
    /// Orchestration: permission tracking for messaging
    permission_store: PermissionStore,
    /// Orchestration: metadata about orchestrated sessions
    orchestrated_sessions: HashMap<String, OrchestratedSession>,
    /// Orchestration: session groups
    groups: HashMap<String, SessionGroup>,
    /// Shared TUI connection — cloned into every AcpSession (main + subagent)
    connection_cx: Option<JrConnectionCx<AgentToClient>>,
    /// Pending group completion waiters: group_name -> sender
    group_completion_waiters: HashMap<String, GroupCompletionSender>,
    /// V1 session exporter for lazy migration of V1 conversations.
    legacy_session_exporter: Arc<dyn LegacySessionExporter>,
    /// Agent config errors encountered during loading at startup.
    agent_config_errors: Vec<AgentConfigLoadError>,
    /// MCP registry data for enterprise users, fetched once at startup
    mcp_registry_data: Option<crate::mcp_registry::McpRegistryResponse>,
    /// Tracks which agent config each session is using (for registry refresh)
    session_agent_names: HashMap<SessionId, String>,
}

/// An agent config error with optional file path.
#[derive(Debug, Clone)]
pub struct AgentConfigLoadError {
    pub path: Option<String>,
    pub message: String,
}

impl SessionManager {
    pub fn builder() -> SessionManagerBuilder {
        Default::default()
    }

    #[allow(clippy::too_many_arguments)]
    fn new(
        agent_configs: Vec<LoadedAgentConfig>,
        agent_config_errors: Vec<AgentConfigLoadError>,
        os: Os,
        local_mcp_path: Option<PathBuf>,
        global_mcp_path: Option<PathBuf>,
        session_manager_handle: SessionManagerHandle,
        mock_registry: Option<MockResponseRegistryHandle>,
        trust_all_tools: bool,
        trust_tools: Option<Vec<String>>,
        telemetry_event_store: Option<TelemetryEventStore>,
        legacy_session_exporter: Arc<dyn LegacySessionExporter>,
        mcp_registry_data: Option<crate::mcp_registry::McpRegistryResponse>,
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
            trust_tools,
            acp_client_info: None,
            telemetry_event_store,
            inbox_store: InboxStore::new(),
            permission_store: PermissionStore::new(),
            orchestrated_sessions: HashMap::new(),
            groups: HashMap::new(),
            connection_cx: None,
            group_completion_waiters: HashMap::new(),
            legacy_session_exporter,
            agent_config_errors,
            mcp_registry_data,
            session_agent_names: HashMap::new(),
        }
    }

    /// Here we are only collecting the results from the leaf nodes. A special case here is if we
    /// have a partially completed DAG. This would happen if the ancestors of leaf nodes were to
    /// fail. In which case the least ancestral executed nodes would then become the new leaf nodes
    /// (i.e. the "youngest" failed node in the DAG). This is because all subsequent children of a
    /// failed node would not execute.
    ///
    /// If a leaf node has failed, its result, along with its parents results are included in the
    /// group result to be returned by this function. This is to help the main agent retry.
    fn collect_group_results(&self, group_name: &str) -> Vec<(String, Option<String>)> {
        let group: Vec<_> = self
            .orchestrated_sessions
            .values()
            .filter(|s| s.group.as_deref() == Some(group_name))
            .collect();
        let depended_on: std::collections::HashSet<&str> = group
            .iter()
            .filter(|s| s.result.is_some())
            .flat_map(|s| s.depends_on.iter().map(|d| d.as_str()))
            .collect();
        group
            .iter()
            .filter(|s| !depended_on.contains(s.name.as_str()))
            .map(|s| (s.name.clone(), s.result.clone()))
            .collect()
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

        let agent_config = if let Some(ref registry) = self.mcp_registry_data {
            let mut config = agent_config.clone();
            crate::mcp_registry::filter_agent_config_tools_by_registry(&mut config, registry);
            crate::mcp_registry::resolve_registry_servers_for_agent_config(&mut config, registry);
            config
        } else {
            agent_config.clone()
        };

        session
            .swap_agent(agent_config)
            .await
            .map_err(|e| sacp::util::internal_error(format!("Failed to swap agent: {}", e)))?;

        Ok(())
    }

    async fn handle_refresh_registry(&mut self, registry: crate::mcp_registry::McpRegistryResponse) {
        info!("Refreshing MCP registry data ({} servers)", registry.servers.len());
        self.mcp_registry_data = Some(registry.clone());

        for (session_id, session_handle) in &self.sessions {
            let Some(agent_name) = self.session_agent_names.get(session_id) else {
                continue;
            };
            let Some(base_config) = self.agent_configs.iter().find(|c| c.name() == agent_name.as_str()) else {
                continue;
            };

            let mut config = base_config.clone();
            crate::mcp_registry::filter_agent_config_tools_by_registry(&mut config, &registry);
            crate::mcp_registry::resolve_registry_servers_for_agent_config(&mut config, &registry);

            if let Err(e) = session_handle.refresh_mcp_servers(config).await {
                warn!(?session_id, %e, "Failed to queue registry refresh for session");
            }
        }
    }

    async fn handle_request(&mut self, request: SessionManagerRequest) {
        debug!(?request, "session manager received new request");
        let SessionManagerRequest { session_id, data } = request;

        match data {
            SessionManagerRequestData::StartSession {
                config: boxed_config,
                connection_cx,
                resp_sender,
            } => {
                let config = *boxed_config;

                // If loading an existing session that doesn't exist as V2, try exporting from V1
                if config.load
                    && let Ok(sessions_dir) = crate::util::paths::sessions_dir()
                    && !crate::agent::session::session_exists(&sessions_dir, &config.session_id)
                    && let Err(e) = self
                        .legacy_session_exporter
                        .export_session(&config.session_id, &sessions_dir)
                {
                    warn!(session_id = %config.session_id, error = %e, "Failed to export V1 session");
                }

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

                let (base_agent_config, agent_name, requested_agent_name) =
                    match self.agent_configs.iter().find(|c| c.name() == agent_name) {
                        Some(config) => (config, agent_name, None),
                        None => {
                            warn!("Agent '{}' not found, falling back to default", agent_name);
                            let requested = agent_name;
                            (default_agent, DEFAULT_AGENT_NAME.to_string(), Some(requested))
                        },
                    };

                // If ACP client provided MCP servers, create an ephemeral config with them merged in
                let converted_mcp_servers: Vec<_> = config
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

                let agent_config_to_use: LoadedAgentConfig = if !converted_mcp_servers.is_empty() {
                    let mut ephemeral = base_agent_config.config().clone();

                    if let Some(overridden) = ephemeral.add_mcp_servers(converted_mcp_servers.clone()) {
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

                // Resolve registry servers if registry data is available
                let agent_config_to_use = if let Some(ref registry) = self.mcp_registry_data {
                    let mut config = agent_config_to_use;
                    crate::mcp_registry::filter_agent_config_tools_by_registry(&mut config, registry);
                    crate::mcp_registry::resolve_registry_servers_for_agent_config(&mut config, registry);
                    config
                } else {
                    agent_config_to_use
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
                    .trust_tools(self.trust_tools.clone())
                    .acp_client_info(self.acp_client_info.clone())
                    .telemetry_event_store(self.telemetry_event_store.clone())
                    .legacy_session_exporter(Arc::clone(&self.legacy_session_exporter))
                    .session_injected_mcp_servers(converted_mcp_servers);

                // Pass client connection to session
                if let Some(cx) = connection_cx {
                    // Main session — store connection for subagents to clone
                    if self.connection_cx.is_none() {
                        self.connection_cx = Some(cx.clone());
                    }
                    builder = builder.connection_cx(cx);
                } else if config.is_subagent {
                    // Subagent session — clone the stored connection
                    if let Some(cx) = &self.connection_cx {
                        builder = builder.connection_cx(cx.clone());
                    }
                } else {
                    error!("No client connection provided for non-subagent session");
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
                builder = builder.subagent_info(config.subagent_info.clone());

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
                    Ok((handle, ready_rx, initial_model_id, requested_model_name)) => {
                        let current_model_id = initial_model_id.unwrap_or_default();
                        let handle_to_give = handle.clone();
                        self.sessions.insert(session_id.clone(), handle);
                        self.session_agent_names.insert(session_id.clone(), agent_name.clone());
                        _ = resp_sender.send(Ok(StartSessionResult {
                            handle: handle_to_give,
                            ready_rx,
                            current_agent_name: agent_name,
                            requested_agent_name,
                            available_agents,
                            available_models,
                            current_model_id,
                            agent_config_errors: self.agent_config_errors.clone(),
                            requested_model_name,
                        }));

                        // Send SUBAGENT_LIST_UPDATE notification after session creation
                        self.send_subagent_list_update().await;
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
                    self.session_agent_names.remove(&session_id);
                    if tokio::time::timeout(std::time::Duration::from_secs(4), handle.shutdown())
                        .await
                        .is_err()
                    {
                        warn!(?session_id, "Session did not shut down within timeout during terminate");
                    }
                } else {
                    warn!(?session_id, "Attempted to terminate non-existent session");
                }
                if let Some(session) = self.orchestrated_sessions.get_mut(&session_id.to_string()) {
                    session.status = SessionStatus::Terminated;
                    // If this session never produced a result (cancelled/killed), remove
                    // any pending stages that transitively depend on it — they can never
                    // have their dependencies satisfied.
                    if session.result.is_none()
                        && let Some(group_name) = &session.group
                        && let Some(g) = self.groups.get_mut(group_name)
                    {
                        let terminated_name = session.name.clone();
                        let mut removed: std::collections::HashSet<String> = std::collections::HashSet::new();
                        removed.insert(terminated_name);
                        // Iteratively remove stages whose deps overlap with removed set
                        loop {
                            let newly_removed: Vec<String> = g
                                .pending_stages
                                .iter()
                                .filter(|ps| ps.depends_on.iter().any(|d| removed.contains(d)))
                                .map(|ps| ps.name.clone())
                                .collect();
                            if newly_removed.is_empty() {
                                break;
                            }
                            g.pending_stages
                                .retain(|ps| !ps.depends_on.iter().any(|d| removed.contains(d)));
                            removed.extend(newly_removed);
                        }
                    }
                }
                self.send_subagent_list_update().await;
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
                if result.is_ok() {
                    self.session_agent_names.insert(session_id.clone(), mode_id);
                }
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
                let mut result = crate::util::paths::sessions_dir()
                    .map_err(crate::agent::session::SessionError::from)
                    .and_then(|d| crate::agent::session::list_sessions(&d, cwd.as_deref()));

                if let Some(cwd) = cwd {
                    let v1_sessions = match self.legacy_session_exporter.list_sessions(&cwd) {
                        Ok(v) => {
                            debug!(?cwd, v1_sessions_len = v.len(), "found v1 sessions for cwd");
                            v
                        },
                        Err(e) => {
                            warn!(?cwd, ?e, "failed to list v1 sessions for cwd");
                            vec![]
                        },
                    };

                    // Merge V1 sessions that haven't been exported yet
                    if let Ok(v2_sessions) = &mut result
                        && !v1_sessions.is_empty()
                    {
                        let v2_ids: std::collections::HashSet<String> =
                            v2_sessions.iter().map(|s| s.session_id.clone()).collect();
                        for v1 in v1_sessions {
                            if !v2_ids.contains(&v1.conversation_id) {
                                v2_sessions.push(crate::agent::session::SessionDataView {
                                    session_id: v1.conversation_id,
                                    cwd: v1.cwd,
                                    created_at: v1.updated_at,
                                    updated_at: v1.updated_at,
                                    title: v1.title,
                                    message_count: v1.message_count,
                                });
                            }
                        }
                        v2_sessions.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
                    }
                }

                _ = resp_sender.send(result);
            },
            SessionManagerRequestData::GetCodeIntelligence { cwd, resp_sender } => {
                let ci = self.get_or_init_code_intelligence(&cwd);
                _ = resp_sender.send(ci);
            },
            SessionManagerRequestData::GetSubagentSessions { resp_sender } => {
                let subagents = self.get_subagent_sessions();
                _ = resp_sender.send(subagents);
            },
            SessionManagerRequestData::DeliverSubagentResult {
                target_session,
                message,
                resp_sender,
            } => {
                let from_id = SessionId::new("subagent-result".to_string());
                let _ = self
                    .inbox_store
                    .send_message(&target_session, &from_id, "subagent", message, false);
                self.send_inbox_notification(&target_session).await;

                // Auto-wake if idle
                if let Some(orch) = self.orchestrated_sessions.get(&target_session.to_string())
                    && orch.status == SessionStatus::Idle
                    && !orch.human_attached
                    && let Some(handle) = self.sessions.get(&target_session)
                {
                    let wake_msg = "You have new messages in your inbox. Use read_messages to check them.".to_string();
                    let handle_clone = handle.clone();
                    tokio::spawn(async move {
                        let _ = handle_clone.wake_session(wake_msg).await;
                    });
                }

                _ = resp_sender.send(());
            },
            SessionManagerRequestData::RegisterPendingStages {
                group,
                pending_stages,
                resp_sender,
            } => {
                let g =
                    self.groups
                        .entry(group)
                        .or_insert_with(|| crate::agent::acp::orchestration::types::SessionGroup {
                            name: String::new(),
                            series: String::new(),
                            members: vec![],
                            pending_stages: vec![],
                        });
                for ps in pending_stages {
                    g.pending_stages
                        .push(crate::agent::acp::orchestration::types::PendingStage {
                            name: ps.name,
                            role: ps.role.clone(),
                            task: ps.task,
                            depends_on: ps.depends_on,
                            agent_name: ps.role,
                        });
                }
                self.send_subagent_list_update().await;
                _ = resp_sender.send(());
            },
            SessionManagerRequestData::TriggerPendingStages {
                completed_name,
                parent_session_id,
                resp_sender,
            } => {
                let group_name = self
                    .orchestrated_sessions
                    .values()
                    .find(|s| s.name == completed_name)
                    .and_then(|s| s.group.clone());
                if let Some(gname) = group_name {
                    let completed: std::collections::HashSet<String> = self
                        .orchestrated_sessions
                        .values()
                        .filter(|s| s.group.as_deref() == Some(&gname) && s.status == SessionStatus::Terminated)
                        .map(|s| s.name.clone())
                        .collect();
                    let to_spawn: Vec<crate::agent::acp::orchestration::types::PendingStage> =
                        if let Some(g) = self.groups.get(&gname) {
                            g.pending_stages
                                .iter()
                                .filter(|ps| ps.depends_on.iter().all(|dep| completed.contains(dep)))
                                .cloned()
                                .collect()
                        } else {
                            vec![]
                        };
                    if let Some(g) = self.groups.get_mut(&gname) {
                        g.pending_stages
                            .retain(|ps| !to_spawn.iter().any(|s| s.name == ps.name));
                    }
                    for stage in to_spawn {
                        info!(name = %stage.name, "DAG: deps satisfied, spawning stage");
                        let deps = stage.depends_on.clone();

                        // Collect results from completed dependencies stored on OrchestratedSession
                        // This is inbox-independent — works even if parent already read messages
                        let task_with_context = {
                            let dep_context: Vec<String> = deps
                                .iter()
                                .filter_map(|dep_name| {
                                    self.orchestrated_sessions
                                        .values()
                                        .find(|s| s.name == *dep_name)
                                        .and_then(|s| s.result.as_ref())
                                        .map(|r| format!("## Results from {}\n\n{}", dep_name, r))
                                })
                                .collect();
                            if dep_context.is_empty() {
                                stage.task.clone()
                            } else {
                                format!(
                                    "{}\n\n---\n\n## Context from previous stages\n\n{}",
                                    stage.task,
                                    dep_context.join("\n\n---\n\n")
                                )
                            }
                        };

                        let result = self
                            .handle_spawn_orchestrated(
                                &parent_session_id,
                                &stage.agent_name,
                                &task_with_context,
                                Some(&stage.name),
                                Some(&stage.role),
                                Some(&gname),
                                false,
                                deps,
                            )
                            .await;
                        if result.is_ok() {
                            self.send_subagent_list_update().await;
                        }
                    }
                }
                _ = resp_sender.send(());
            },
            SessionManagerRequestData::UpdateSessionStatus {
                session_id: sid,
                status,
                resp_sender,
            } => {
                if let Some(session) = self.orchestrated_sessions.get_mut(&sid.to_string()) {
                    session.status = status;
                    session.last_activity = std::time::SystemTime::now();

                    // After updating status to Terminated, check waiters
                    if session.status == SessionStatus::Terminated
                        && let Some(group) = session.group.clone()
                    {
                        let has_pending = self.groups.get(&group).is_some_and(|g| !g.pending_stages.is_empty());
                        let all_done = !has_pending
                            && self
                                .orchestrated_sessions
                                .values()
                                .filter(|s| s.group.as_deref() == Some(&group))
                                .all(|s| s.status == SessionStatus::Terminated);
                        if all_done {
                            // Send list update before cleanup so TUI sees final state
                            self.send_subagent_list_update().await;
                            if let Some(waiter) = self.group_completion_waiters.remove(&group) {
                                let results = self.collect_group_results(&group);
                                let _ = waiter.send(results);
                            }
                            self.orchestrated_sessions
                                .retain(|_, s| s.group.as_deref() != Some(&group));
                            self.groups.remove(&group);
                        }
                    }
                }
                self.send_subagent_list_update().await;
                _ = resp_sender.send(());
            },
            SessionManagerRequestData::StoreSessionResult {
                session_id: sid,
                result,
                resp_sender,
            } => {
                if let Some(session) = self.orchestrated_sessions.get_mut(&sid.to_string()) {
                    session.result = Some(result);
                }
                _ = resp_sender.send(());
            },
            SessionManagerRequestData::WaitForGroupCompletion {
                group_name,
                resp_sender,
            } => {
                // Check if all sessions in group are already terminated
                let all_done = self
                    .orchestrated_sessions
                    .values()
                    .filter(|s| s.group.as_deref() == Some(&group_name))
                    .all(|s| s.status == SessionStatus::Terminated);
                if all_done {
                    let results = self.collect_group_results(&group_name);
                    let _ = resp_sender.send(results);
                    // Clean up completed group
                    self.orchestrated_sessions
                        .retain(|_, s| s.group.as_deref() != Some(&group_name));
                    self.groups.remove(&group_name);
                } else {
                    // Store waiter — will be fired when last session terminates
                    self.group_completion_waiters.insert(group_name, resp_sender);
                }
            },
            // --- Orchestration handlers ---
            SessionManagerRequestData::SpawnOrchestratedSession {
                parent_session_id,
                agent_name,
                task,
                name,
                role,
                group,
                persistent,
                resp_sender,
            } => {
                let result = self
                    .handle_spawn_orchestrated(
                        &parent_session_id,
                        &agent_name,
                        &task,
                        name.as_deref(),
                        role.as_deref(),
                        group.as_deref(),
                        persistent,
                        vec![],
                    )
                    .await;
                if result.is_ok() {
                    self.send_subagent_list_update().await;
                }
                _ = resp_sender.send(result);
            },
            SessionManagerRequestData::SendOrchestrationMessage {
                from_session,
                target,
                message,
                is_escalation,
                resp_sender,
            } => {
                // Resolve target before consuming it
                let target_id = if let Some(t) = target.as_deref() {
                    self.resolve_target(t).ok()
                } else if is_escalation {
                    self.resolve_escalation_target(&from_session).ok()
                } else {
                    None
                };
                let result =
                    self.handle_send_orchestration_message(&from_session, target.as_deref(), &message, is_escalation);
                // Notify TUI so it can show the notification bar alert
                if result.is_ok()
                    && let Some(tid) = target_id
                {
                    self.send_inbox_notification(&tid).await;
                    // Auto-wake if idle — inject message content directly
                    if let Some(orch) = self.orchestrated_sessions.get(&tid.to_string())
                        && orch.status == SessionStatus::Idle
                        && !orch.human_attached
                        && let Some(handle) = self.sessions.get(&tid)
                    {
                        let wake_msg = format!("You have a new message:\n\n{}", message);
                        let handle_clone = handle.clone();
                        tokio::spawn(async move {
                            let _ = handle_clone.wake_session(wake_msg).await;
                        });
                    }
                }
                _ = resp_sender.send(result.map(|_| ()));
            },
            SessionManagerRequestData::ReadOrchestrationMessages {
                session_id: sid,
                limit,
                resp_sender,
            } => {
                let messages = self.inbox_store.read_messages(&sid, limit);
                _ = resp_sender.send(Ok(messages));
            },
            SessionManagerRequestData::ListOrchestratedSessions { filter, resp_sender } => {
                let sessions = self.handle_list_orchestrated(filter);
                _ = resp_sender.send(Ok(sessions));
            },
            SessionManagerRequestData::GetOrchestratedSessionStatus { target, resp_sender } => {
                let result = self.handle_get_orchestrated_status(&target);
                _ = resp_sender.send(result);
            },
            SessionManagerRequestData::GetOrchestratedSessionById {
                session_id,
                resp_sender,
            } => {
                let result = self.orchestrated_sessions.get(&session_id.to_string()).cloned();
                _ = resp_sender.send(result);
            },
            SessionManagerRequestData::InterruptOrchestratedSession {
                from_session,
                target,
                message,
                resp_sender,
            } => {
                let result = self
                    .handle_interrupt_orchestrated(&from_session, &target, &message)
                    .await;
                _ = resp_sender.send(result);
            },
            SessionManagerRequestData::InjectOrchestrationContext {
                from_session,
                target,
                context,
                resp_sender,
            } => {
                let result = self.handle_inject_context(&from_session, &target, &context).await;
                _ = resp_sender.send(result);
            },
            SessionManagerRequestData::ManageOrchestrationGroup {
                from_session,
                action,
                group,
                target,
                role,
                message,
                resp_sender,
            } => {
                let result = self.handle_manage_group(
                    &from_session,
                    action,
                    group.as_deref(),
                    target.as_deref(),
                    role.as_deref(),
                    message.as_deref(),
                );
                _ = resp_sender.send(result);
            },
            SessionManagerRequestData::ReviveOrchestratedSession {
                parent_session_id,
                target,
                task,
                resp_sender,
            } => {
                let result = self.handle_revive_orchestrated(&parent_session_id, &target, &task);
                _ = resp_sender.send(result);
            },
            SessionManagerRequestData::GetSessionLiveActivity { target, resp_sender } => {
                let activity = self.handle_get_live_activity(&target).await;
                _ = resp_sender.send(activity);
            },
            SessionManagerRequestData::RefreshRegistry { registry, resp_sender } => {
                self.handle_refresh_registry(registry).await;
                _ = resp_sender.send(());
            },
            SessionManagerRequestData::GetRegistryData { resp_sender } => {
                _ = resp_sender.send(self.mcp_registry_data.clone());
            },
        }
    }

    fn get_subagent_sessions(&self) -> Vec<SubagentInfo> {
        self.sessions
            .values()
            .filter_map(|handle| handle._subagent_info.clone())
            .collect()
    }

    // --- Orchestration implementation methods ---

    #[allow(clippy::too_many_arguments)]
    /// Spawn an orchestrated (subagent) session and run its task asynchronously.
    ///
    /// # What it does
    /// 1. Generates a bare UUID session ID (NOT "orch-{uuid}" — that fails DB validation).
    /// 2. Registers the session in `orchestrated_sessions` and permission store.
    /// 3. Spawns a `tokio::task` that:
    ///    - Calls `start_session` with `is_subagent=true` (cloned `connection_cx`).
    ///    - Waits for MCP init (`ready_rx`).
    ///    - Calls `internal_prompt(task)` — blocks until the agent calls the `summary` tool.
    ///    - On success: delivers summary to parent inbox, marks session `Terminated`, terminates
    ///      it, then calls `trigger_pending_stages` to advance the DAG.
    ///    - On error: delivers error message, same cleanup.
    /// 4. Returns immediately (the task runs in background).
    ///
    /// # Invariants
    /// - `persistent=false` → session is terminated after task completes (ephemeral worker).
    /// - `persistent=true` → session goes `Idle` after task (knight, stays alive for
    ///   attach/revive).
    /// - The `group` field enables DAG grouping in the TUI crew monitor.
    /// - Subagent streaming reaches TUI via cloned `connection_cx`.
    async fn handle_spawn_orchestrated(
        &mut self,
        parent_session_id: &SessionId,
        agent_name: &str,
        task: &str,
        name: Option<&str>,
        role: Option<&str>,
        group: Option<&str>,
        persistent: bool,
        depends_on: Vec<String>,
    ) -> Result<SpawnOrchestratedResult, sacp::Error> {
        // Determine group and series
        let group_name = group.unwrap_or("default").to_string();
        let group_entry = self.groups.entry(group_name.clone()).or_insert_with(|| {
            let series = naming::pick_series();
            SessionGroup {
                name: group_name.clone(),
                series: series.to_string(),
                members: vec![],
                pending_stages: vec![],
            }
        });

        // Determine session name
        let used_names: HashSet<String> = group_entry.members.iter().map(|m| m.name.clone()).collect();
        let session_name = name.map_or_else(
            || {
                if persistent {
                    naming::next_name(&group_entry.series, &used_names)
                } else {
                    naming::next_squire_name(&used_names)
                }
            },
            String::from,
        );

        // Create a unique session ID (must be a valid UUID for session DB)
        let new_session_id = SessionId::new(uuid::Uuid::new_v4().to_string());

        // Register parent-child relationship
        self.permission_store.register_child(parent_session_id, &new_session_id);
        self.permission_store.register_group(&new_session_id, &group_name);

        // Add to group
        group_entry.members.push(GroupMembership {
            session_id: new_session_id.clone(),
            name: session_name.clone(),
            role: role.map(String::from),
            joined_at: std::time::SystemTime::now(),
        });

        // Store orchestrated session metadata
        let orch_session = OrchestratedSession {
            session_id: new_session_id.clone(),
            name: session_name.clone(),
            role: role.map(String::from),
            agent_name: agent_name.to_string(),
            task: task.to_string(),
            parent_session: Some(parent_session_id.clone()),
            group: Some(group_name),
            status: SessionStatus::Busy,
            created_at: std::time::SystemTime::now(),
            last_activity: std::time::SystemTime::now(),
            human_attached: false,
            persistent,
            depends_on,
            result: None,
        };
        self.orchestrated_sessions
            .insert(new_session_id.to_string(), orch_session);

        info!(
            session_id = %new_session_id.to_string(),
            name = %session_name,
            agent = agent_name,
            "Orchestrated session spawning"
        );

        // Spawn the ACP session as a subagent
        let session_tx = self.session_manager_handle.clone();
        let new_sid = new_session_id.clone();
        let agent_str = agent_name.to_string();
        let task_str = task.to_string();
        let session_name_clone = session_name.clone();
        let parent_sid = parent_session_id.clone();
        let embedded_msg = format!(
            "You are '{}' — an orchestrated session.\nYour task: {}\n{}\nWhen your task is complete, call the summary tool with your findings.",
            session_name,
            task,
            role.map(|r| format!("Your role: {}", r)).unwrap_or_default(),
        );
        tokio::spawn(async move {
            let config = AcpSessionConfig::new(new_sid.to_string(), std::env::current_dir().unwrap_or_default())
                .is_subagent(true)
                .initial_agent_name(agent_str)
                .user_embedded_msg(embedded_msg);
            match session_tx.start_session(&new_sid, config, None).await {
                Ok(result) => {
                    let _ = result.ready_rx.await;
                    match result.handle.internal_prompt(task_str).await {
                        Ok(summary) => {
                            info!(name = %session_name_clone, "Orchestrated session completed task");
                            let msg = format!("[Results from {}]\n\n{}", session_name_clone, summary.task_result);
                            session_tx.deliver_subagent_result(&parent_sid, &msg).await;
                            session_tx.store_session_result(&new_sid, summary.task_result).await;
                            if persistent {
                                session_tx.update_session_status(&new_sid, SessionStatus::Idle).await;
                            } else {
                                session_tx
                                    .update_session_status(&new_sid, SessionStatus::Terminated)
                                    .await;
                                session_tx.terminate_session(&new_sid).await;
                            }
                            session_tx
                                .trigger_pending_stages(&session_name_clone, &parent_sid)
                                .await;
                        },
                        Err(e) => {
                            let cancelled = e.is_cancelled();
                            error!(name = %session_name_clone, "Orchestrated session task failed: {}", e);
                            if !cancelled {
                                let msg = format!("[{} failed: {}]", session_name_clone, e);
                                session_tx.deliver_subagent_result(&parent_sid, &msg).await;
                            }
                            if persistent {
                                session_tx.update_session_status(&new_sid, SessionStatus::Idle).await;
                            } else {
                                session_tx
                                    .update_session_status(&new_sid, SessionStatus::Terminated)
                                    .await;
                                session_tx.terminate_session(&new_sid).await;
                            }
                            // Only trigger next DAG stages on real failures, not cancellation
                            if !cancelled {
                                session_tx
                                    .trigger_pending_stages(&session_name_clone, &parent_sid)
                                    .await;
                            }
                        },
                    }
                },
                Err(e) => {
                    error!("Failed to start orchestrated session {}: {}", new_sid, e);
                },
            }
        });

        Ok(SpawnOrchestratedResult {
            session_id: new_session_id.to_string(),
            name: session_name,
        })
    }

    fn handle_revive_orchestrated(
        &mut self,
        parent_session_id: &SessionId,
        target: &str,
        task: &str,
    ) -> Result<SpawnOrchestratedResult, sacp::Error> {
        // Find the terminated session by name
        let old_session = self
            .find_session_by_name(target)
            .cloned()
            .ok_or_else(|| sacp::util::internal_error(format!("Session not found: {}", target)))?;

        if old_session.status != SessionStatus::Terminated {
            return Err(sacp::util::internal_error(format!(
                "Session '{}' is {:?}, not terminated — use send_message instead",
                target, old_session.status
            )));
        }

        // Remove old session entry
        self.orchestrated_sessions.remove(&old_session.session_id.to_string());

        // Create new session with same name but new ID (must be a valid UUID)
        let new_session_id = SessionId::new(uuid::Uuid::new_v4().to_string());
        let group = old_session.group.as_deref();
        let role = old_session.role.as_deref();

        // Re-register permissions
        self.permission_store.register_child(parent_session_id, &new_session_id);
        if let Some(g) = group {
            self.permission_store.register_group(&new_session_id, g);
        }

        let orch_session = OrchestratedSession {
            session_id: new_session_id.clone(),
            name: target.to_string(),
            role: role.map(String::from),
            agent_name: old_session.agent_name.clone(),
            task: task.to_string(),
            parent_session: Some(parent_session_id.clone()),
            group: group.map(String::from),
            status: SessionStatus::Busy,
            created_at: std::time::SystemTime::now(),
            last_activity: std::time::SystemTime::now(),
            human_attached: false,
            persistent: old_session.persistent,
            depends_on: old_session.depends_on.clone(),
            result: None,
        };
        self.orchestrated_sessions
            .insert(new_session_id.to_string(), orch_session);

        info!(name = target, "Reviving terminated session");

        Ok(SpawnOrchestratedResult {
            session_id: new_session_id.to_string(),
            name: target.to_string(),
        })
    }

    fn find_session_by_name(&self, name: &str) -> Option<&OrchestratedSession> {
        self.orchestrated_sessions.values().find(|s| s.name == name)
    }

    fn resolve_target(&self, target: &str) -> Result<SessionId, sacp::Error> {
        // Try as session name first
        if let Some(s) = self.find_session_by_name(target) {
            return Ok(s.session_id.clone());
        }
        // Try as session ID
        if self.orchestrated_sessions.contains_key(target) {
            return Ok(SessionId::new(target.to_string()));
        }
        // Try as raw session ID in active sessions (covers the orchestrator)
        if self.sessions.contains_key(&SessionId::new(target.to_string())) {
            return Ok(SessionId::new(target.to_string()));
        }
        Err(sacp::util::internal_error(format!("Session not found: {}", target)))
    }

    async fn handle_get_live_activity(&self, target: &str) -> Option<String> {
        let target_id = self.resolve_target(target).ok()?;
        let handle = self.sessions.get(&target_id)?;
        let agent = handle.get_agent_handle().await?;
        let snapshot = agent.create_snapshot().await.ok()?;

        let mut activity_parts = Vec::new();
        let messages = snapshot.conversation_state.cached_messages()?;

        // Walk backwards through messages to find recent activity
        for msg in messages.iter().rev().take(6) {
            let role = &msg.role;
            for block in &msg.content {
                match block {
                    agent::agent_loop::types::ContentBlock::Text(text) => {
                        let truncated = if text.len() > 300 {
                            format!("{}...", &text[..300])
                        } else {
                            text.clone()
                        };
                        activity_parts.push(format!("[{}] {}", role, truncated));
                    },
                    agent::agent_loop::types::ContentBlock::ToolUse(tool_use) => {
                        activity_parts.push(format!("[tool_call] {}", tool_use.name));
                    },
                    agent::agent_loop::types::ContentBlock::ToolResult(tool_result) => {
                        let result_preview = tool_result
                            .content
                            .first()
                            .map(|c| match c {
                                agent::agent_loop::types::ToolResultContentBlock::Text(t) => {
                                    if t.len() > 200 {
                                        format!("{}...", &t[..200])
                                    } else {
                                        t.clone()
                                    }
                                },
                                _ => "(non-text)".to_string(),
                            })
                            .unwrap_or_default();
                        activity_parts.push(format!("[tool_result] {}", result_preview));
                    },
                    agent::agent_loop::types::ContentBlock::Image(_) => {},
                }
            }
        }

        activity_parts.reverse();
        if activity_parts.is_empty() {
            None
        } else {
            Some(activity_parts.join("\n"))
        }
    }

    fn resolve_sender_name(&self, session_id: &SessionId) -> String {
        self.orchestrated_sessions
            .get(&session_id.to_string())
            .map_or_else(|| session_id.to_string(), |s| s.name.clone())
    }

    fn handle_send_orchestration_message(
        &mut self,
        from_session: &SessionId,
        target: Option<&str>,
        message: &str,
        is_escalation: bool,
    ) -> Result<bool, sacp::Error> {
        // Resolve target: explicit target, or escalation auto-route to parent chain
        let target_id = if let Some(t) = target {
            self.resolve_target(t)?
        } else if is_escalation {
            self.resolve_escalation_target(from_session)?
        } else {
            return Err(sacp::util::internal_error(
                "target is required for non-escalation messages",
            ));
        };

        // Check permissions
        self.permission_store
            .can_message(from_session, &target_id)
            .map_err(sacp::util::internal_error)?;

        // Check rate limit
        self.permission_store
            .check_rate_limit(from_session)
            .map_err(sacp::util::internal_error)?;

        let sender_name = self.resolve_sender_name(from_session);

        self.inbox_store
            .send_message(
                &target_id,
                from_session,
                &sender_name,
                message.to_string(),
                is_escalation,
            )
            .map_err(sacp::util::internal_error)?;

        Ok(true)
    }

    fn resolve_escalation_target(&self, from_session: &SessionId) -> Result<SessionId, sacp::Error> {
        let mut current = from_session.clone();
        let mut visited = std::collections::HashSet::new();
        visited.insert(current.to_string());

        loop {
            let parent = self
                .orchestrated_sessions
                .get(&current.to_string())
                .and_then(|s| s.parent_session.clone());

            match parent {
                Some(pid) => {
                    if !visited.insert(pid.to_string()) {
                        return Err(sacp::util::internal_error("Cycle detected in parent chain"));
                    }
                    // If parent is human-attached, deliver there
                    if self
                        .orchestrated_sessions
                        .get(&pid.to_string())
                        .is_some_and(|s| s.human_attached)
                    {
                        return Ok(pid);
                    }
                    current = pid;
                },
                // No parent — deliver to current (root)
                None => return Ok(current),
            }
        }
    }

    fn handle_list_orchestrated(&self, filter: Option<SessionFilter>) -> Vec<OrchestratedSession> {
        self.orchestrated_sessions
            .values()
            .filter(|s| match filter {
                Some(SessionFilter::Idle) => s.status == SessionStatus::Idle,
                Some(SessionFilter::Busy) => s.status == SessionStatus::Busy,
                Some(SessionFilter::Active) => s.status != SessionStatus::Terminated,
                Some(SessionFilter::All) => true,
                Some(SessionFilter::Terminated) => s.status == SessionStatus::Terminated,
                // Default: hide terminated
                None => s.status != SessionStatus::Terminated,
            })
            .cloned()
            .collect()
    }

    fn handle_get_orchestrated_status(&self, target: &str) -> Result<OrchestratedSession, sacp::Error> {
        let target_id = self.resolve_target(target)?;
        self.orchestrated_sessions
            .get(&target_id.to_string())
            .cloned()
            .ok_or_else(|| sacp::util::internal_error(format!("Session not found: {}", target)))
    }

    async fn handle_interrupt_orchestrated(
        &mut self,
        from_session: &SessionId,
        target: &str,
        message: &str,
    ) -> Result<(), sacp::Error> {
        let target_id = self.resolve_target(target)?;

        self.permission_store
            .can_message(from_session, &target_id)
            .map_err(sacp::util::internal_error)?;

        // Cancel the target session and send new prompt
        if let Some(handle) = self.sessions.get(&target_id) {
            let _ = handle.cancel().await;
            let sender_name = self.resolve_sender_name(from_session);
            let interrupt_msg = format!("[INTERRUPT from {}]: {}", sender_name, message);
            if let Err(e) = handle.internal_prompt(interrupt_msg).await {
                warn!("Failed to send interrupt prompt: {}", e);
            }
        }

        Ok(())
    }

    async fn handle_inject_context(
        &mut self,
        from_session: &SessionId,
        target: &str,
        context: &str,
    ) -> Result<(), sacp::Error> {
        const MAX_INJECT_CONTEXT_SIZE: usize = 4000; // ~1K tokens
        if context.len() > MAX_INJECT_CONTEXT_SIZE {
            return Err(sacp::util::internal_error(format!(
                "Context too large: {} chars (max {})",
                context.len(),
                MAX_INJECT_CONTEXT_SIZE
            )));
        }

        let target_id = self.resolve_target(target)?;

        self.permission_store
            .can_message(from_session, &target_id)
            .map_err(sacp::util::internal_error)?;

        // Inject context via the agent's dynamic context
        if let Some(handle) = self.sessions.get(&target_id) {
            let agent = handle.get_agent_handle().await;
            if let Some(_agent) = agent {
                // Note: set_dynamic_context not available in this version
                // agent.set_dynamic_context(Some(context.to_string())).await;
            }
        }

        Ok(())
    }

    fn handle_manage_group(
        &mut self,
        from_session: &SessionId,
        action: GroupAction,
        group: Option<&str>,
        target: Option<&str>,
        role: Option<&str>,
        message: Option<&str>,
    ) -> Result<String, sacp::Error> {
        match action {
            GroupAction::Create => {
                let name = group.ok_or_else(|| sacp::util::internal_error("Group name required"))?;
                let series = naming::pick_series();
                self.groups.insert(name.to_string(), SessionGroup {
                    name: name.to_string(),
                    series: series.to_string(),
                    members: vec![],
                    pending_stages: vec![],
                });
                Ok(serde_json::json!({"status": "created", "group": name, "series": series}).to_string())
            },
            GroupAction::Add => {
                let group_name = group.ok_or_else(|| sacp::util::internal_error("Group name required"))?;
                let target_name = target.ok_or_else(|| sacp::util::internal_error("Target session required"))?;
                let target_id = self.resolve_target(target_name)?;

                let session_name = self.resolve_sender_name(&target_id);

                let group_entry = self
                    .groups
                    .get_mut(group_name)
                    .ok_or_else(|| sacp::util::internal_error(format!("Group not found: {}", group_name)))?;

                group_entry.members.push(GroupMembership {
                    session_id: target_id.clone(),
                    name: session_name,
                    role: role.map(String::from),
                    joined_at: std::time::SystemTime::now(),
                });
                self.permission_store.register_group(&target_id, group_name);

                Ok(serde_json::json!({"status": "added", "group": group_name}).to_string())
            },
            GroupAction::Remove => Err(sacp::util::internal_error("Group remove not yet implemented")),
            GroupAction::List => {
                let groups: Vec<serde_json::Value> = if let Some(name) = group {
                    self.groups
                        .get(name)
                        .map(|g| {
                            vec![serde_json::json!({
                                "name": g.name,
                                "series": g.series,
                                "members": g.members.iter().map(|m| serde_json::json!({
                                    "name": m.name,
                                    "role": m.role,
                                    "session_id": m.session_id.to_string(),
                                })).collect::<Vec<_>>(),
                            })]
                        })
                        .unwrap_or_default()
                } else {
                    self.groups
                        .values()
                        .map(|g| {
                            serde_json::json!({
                                "name": g.name,
                                "series": g.series,
                                "member_count": g.members.len(),
                            })
                        })
                        .collect()
                };
                Ok(serde_json::json!({"groups": groups}).to_string())
            },
            GroupAction::Broadcast => {
                let group_name = group.ok_or_else(|| sacp::util::internal_error("Group name required"))?;
                let msg = message.ok_or_else(|| sacp::util::internal_error("Message required"))?;

                let group_entry = self
                    .groups
                    .get(group_name)
                    .ok_or_else(|| sacp::util::internal_error(format!("Group not found: {}", group_name)))?;

                let member_ids: Vec<SessionId> = group_entry.members.iter().map(|m| m.session_id.clone()).collect();
                let sender_name = self.resolve_sender_name(from_session);

                let mut delivered = 0;
                for member_id in &member_ids {
                    if member_id.to_string() != from_session.to_string()
                        && self
                            .inbox_store
                            .send_message(member_id, from_session, &sender_name, msg.to_string(), false)
                            .is_ok()
                    {
                        delivered += 1;
                    }
                }

                Ok(serde_json::json!({"status": "broadcast", "delivered": delivered}).to_string())
            },
        }
    }

    /// Send a TUI notification about new inbox messages for a session.
    async fn send_inbox_notification(&self, session_id: &SessionId) {
        let summary = self.inbox_store.get_unread_summary(session_id);
        if let Some(handle) = self.sessions.get(session_id) {
            let senders: Vec<String> = summary.senders.iter().map(|(n, _)| n.clone()).collect();
            let params = serde_json::json!({
                "sessionId": session_id.to_string(),
                "sessionName": self.orchestrated_sessions
                    .get(&session_id.to_string())
                    .map_or("main", |s| s.name.as_str()),
                "messageCount": summary.unread_count,
                "escalationCount": summary.escalation_count,
                "senders": senders,
            });
            handle
                .send_ext_notification_raw(super::extensions::methods::INBOX_NOTIFICATION.to_string(), params)
                .await;
        }
    }

    /// Emit an activity event to the TUI.
    async fn emit_activity(&self, event_type: &str, session_name: &str, description: &str) {
        let params = serde_json::json!({
            "type": event_type,
            "sessionName": session_name,
            "description": description,
            "timestamp": chrono::Utc::now().to_rfc3339(),
        });
        for handle in self.sessions.values() {
            handle
                .send_ext_notification_raw(super::extensions::methods::SESSION_ACTIVITY.to_string(), params.clone())
                .await;
        }
    }

    async fn send_session_list_update(&self) {
        let sessions: Vec<serde_json::Value> = self
            .orchestrated_sessions
            .values()
            .map(|s| {
                let inbox_summary = self.inbox_store.get_unread_summary(&s.session_id);
                serde_json::json!({
                    "sessionId": s.session_id.to_string(),
                    "name": s.name,
                    "role": s.role,
                    "agentName": s.agent_name,
                    "task": s.task,
                    "status": s.status,
                    "group": s.group,
                    "parentSessionId": s.parent_session.as_ref().map(|p| p.to_string()),
                    "inboxCount": inbox_summary.unread_count,
                    "escalationCount": inbox_summary.escalation_count,
                    "persistent": s.persistent,
                })
            })
            .collect();

        let params = serde_json::json!({ "sessions": sessions });
        for handle in self.sessions.values() {
            handle
                .send_ext_notification_raw(
                    super::extensions::methods::SESSION_LIST_UPDATE.to_string(),
                    params.clone(),
                )
                .await;
        }
    }

    /// Broadcast the current subagent list (active + pending) to all TUI sessions.
    ///
    /// Sends `SUBAGENT_LIST_UPDATE` (`kiro.dev/subagent/list_update`) to every active ACP session.
    /// The TUI uses this to update the crew monitor DAG and session list.
    ///
    /// # Payload
    /// ```json
    /// {
    ///   "subagents": [{ "sessionId", "agentName", "initialQuery", "status", "group", "role" }],
    ///   "pendingStages": [{ "name", "role", "group", "dependsOn" }]
    /// }
    /// ```
    ///
    /// Called after: session spawn, status change, DAG stage trigger, session termination.
    async fn send_subagent_list_update(&self) {
        let subagents: Vec<super::extensions::SubagentInfo> = self
            .orchestrated_sessions
            .values()
            .map(|s| super::extensions::SubagentInfo {
                session_id: s.session_id.clone(),
                session_name: s.name.clone(),
                agent_name: s.agent_name.clone(),
                initial_query: s.task.clone(),
                status: match s.status {
                    SessionStatus::Busy => super::extensions::SubagentStatus::Working {
                        message: "Running".to_string(),
                    },
                    SessionStatus::Terminated => super::extensions::SubagentStatus::Terminated,
                    SessionStatus::Idle => super::extensions::SubagentStatus::AwaitingInstruction,
                },
                group: s.group.clone(),
                role: s.role.clone(),
                depends_on: s.depends_on.clone(),
            })
            .collect();

        // Include pending stages so TUI can show full DAG
        let pending_stages: Vec<super::extensions::PendingStageInfo> = self
            .groups
            .values()
            .flat_map(|g| {
                g.pending_stages
                    .iter()
                    .map(move |ps| super::extensions::PendingStageInfo {
                        name: ps.name.clone(),
                        role: ps.role.clone(),
                        group: g.name.clone(),
                        depends_on: ps.depends_on.clone(),
                        agent_name: ps.agent_name.clone(),
                    })
            })
            .collect();

        let params = serde_json::json!({ "subagents": subagents, "pendingStages": pending_stages });
        for handle in self.sessions.values() {
            handle
                .send_ext_notification_raw(
                    super::extensions::methods::SUBAGENT_LIST_UPDATE.to_string(),
                    params.clone(),
                )
                .await;
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
        config: Box<AcpSessionConfig>,
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
    GetSubagentSessions {
        resp_sender: oneshot::Sender<Vec<SubagentInfo>>,
    },
    DeliverSubagentResult {
        target_session: SessionId,
        message: String,
        resp_sender: oneshot::Sender<()>,
    },
    RegisterPendingStages {
        group: String,
        pending_stages: Vec<agent::tools::agent_crew::PendingStageSpec>,
        resp_sender: oneshot::Sender<()>,
    },
    TriggerPendingStages {
        completed_name: String,
        parent_session_id: SessionId,
        resp_sender: oneshot::Sender<()>,
    },
    UpdateSessionStatus {
        session_id: SessionId,
        status: SessionStatus,
        resp_sender: oneshot::Sender<()>,
    },
    StoreSessionResult {
        session_id: SessionId,
        result: String,
        resp_sender: oneshot::Sender<()>,
    },
    WaitForGroupCompletion {
        group_name: String,
        resp_sender: oneshot::Sender<Vec<(String, Option<String>)>>,
    },
    // --- Orchestration requests ---
    SpawnOrchestratedSession {
        parent_session_id: SessionId,
        agent_name: String,
        task: String,
        name: Option<String>,
        role: Option<String>,
        group: Option<String>,
        persistent: bool,
        resp_sender: oneshot::Sender<Result<SpawnOrchestratedResult, sacp::Error>>,
    },
    SendOrchestrationMessage {
        from_session: SessionId,
        target: Option<String>,
        message: String,
        is_escalation: bool,
        resp_sender: oneshot::Sender<Result<(), sacp::Error>>,
    },
    ReadOrchestrationMessages {
        session_id: SessionId,
        limit: usize,
        resp_sender: oneshot::Sender<Result<Vec<InboxMessage>, sacp::Error>>,
    },
    ListOrchestratedSessions {
        filter: Option<SessionFilter>,
        resp_sender: oneshot::Sender<Result<Vec<OrchestratedSession>, sacp::Error>>,
    },
    GetOrchestratedSessionStatus {
        target: String,
        resp_sender: oneshot::Sender<Result<OrchestratedSession, sacp::Error>>,
    },
    GetOrchestratedSessionById {
        session_id: SessionId,
        resp_sender: oneshot::Sender<Option<OrchestratedSession>>,
    },
    InterruptOrchestratedSession {
        from_session: SessionId,
        target: String,
        message: String,
        resp_sender: oneshot::Sender<Result<(), sacp::Error>>,
    },
    InjectOrchestrationContext {
        from_session: SessionId,
        target: String,
        context: String,
        resp_sender: oneshot::Sender<Result<(), sacp::Error>>,
    },
    ManageOrchestrationGroup {
        from_session: SessionId,
        action: GroupAction,
        group: Option<String>,
        target: Option<String>,
        role: Option<String>,
        message: Option<String>,
        resp_sender: oneshot::Sender<Result<String, sacp::Error>>,
    },
    ReviveOrchestratedSession {
        parent_session_id: SessionId,
        target: String,
        task: String,
        resp_sender: oneshot::Sender<Result<SpawnOrchestratedResult, sacp::Error>>,
    },
    GetSessionLiveActivity {
        target: String,
        resp_sender: oneshot::Sender<Option<String>>,
    },
    RefreshRegistry {
        registry: crate::mcp_registry::McpRegistryResponse,
        resp_sender: oneshot::Sender<()>,
    },
    GetRegistryData {
        resp_sender: oneshot::Sender<Option<crate::mcp_registry::McpRegistryResponse>>,
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
                    config: Box::new(config),
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

    pub async fn get_subagent_sessions(&self) -> Vec<SubagentInfo> {
        let (resp_sender, rx) = oneshot::channel();
        if self
            .tx
            .send(SessionManagerRequest {
                session_id: SessionId::new(String::new()),
                data: SessionManagerRequestData::GetSubagentSessions { resp_sender },
            })
            .await
            .is_ok()
        {
            rx.await.unwrap_or_default()
        } else {
            Vec::new()
        }
    }

    /// Deliver a subagent's result to the parent session's inbox.
    ///
    /// Puts `message` into the parent's inbox via `InboxStore`, then sends an
    /// `INBOX_NOTIFICATION` to the TUI so the user sees "Bob finished  r: read".
    ///
    /// Called from the `tokio::spawn` in `handle_spawn_orchestrated` after `internal_prompt`
    /// returns. The message format is: `"[Results from {name}]\n\n{summary}"`.
    pub async fn deliver_subagent_result(&self, target_session: &SessionId, message: &str) {
        let (resp_sender, rx) = oneshot::channel();
        let _ = self
            .tx
            .send(SessionManagerRequest {
                session_id: target_session.clone(),
                data: SessionManagerRequestData::DeliverSubagentResult {
                    target_session: target_session.clone(),
                    message: message.to_string(),
                    resp_sender,
                },
            })
            .await;
        let _ = rx.await;
    }

    /// Update the status of an orchestrated session and notify the TUI.
    ///
    /// After updating, fires `send_subagent_list_update` so the TUI crew monitor reflects the
    /// change.
    ///
    /// Called from `handle_spawn_orchestrated` after task completion:
    /// - Ephemeral sessions: `Terminated` → then `terminate_session` to clean up ACP state.
    /// - Persistent sessions: `Idle` → stays alive for attach/revive.
    pub async fn update_session_status(&self, session_id: &SessionId, status: SessionStatus) {
        let (resp_sender, rx) = oneshot::channel();
        let _ = self
            .tx
            .send(SessionManagerRequest {
                session_id: session_id.clone(),
                data: SessionManagerRequestData::UpdateSessionStatus {
                    session_id: session_id.clone(),
                    status,
                    resp_sender,
                },
            })
            .await;
        let _ = rx.await;
    }

    pub async fn store_session_result(&self, session_id: &SessionId, result: String) {
        let (resp_sender, rx) = oneshot::channel();
        let _ = self
            .tx
            .send(SessionManagerRequest {
                session_id: session_id.clone(),
                data: SessionManagerRequestData::StoreSessionResult {
                    session_id: session_id.clone(),
                    result,
                    resp_sender,
                },
            })
            .await;
        let _ = rx.await;
    }

    /// Wait for all sessions in a group to complete. Blocks until all are Terminated.
    pub async fn wait_for_group_completion(&self, group_name: String) -> Vec<(String, Option<String>)> {
        let (resp_sender, rx) = oneshot::channel();
        let _ = self
            .tx
            .send(SessionManagerRequest {
                session_id: SessionId::new(String::new()),
                data: SessionManagerRequestData::WaitForGroupCompletion {
                    group_name,
                    resp_sender,
                },
            })
            .await;
        rx.await.unwrap_or_default()
    }

    /// Store pending DAG stages for a crew group.
    ///
    /// Called by `agent_crew.execute()` via `RegisterPendingStages` tool event.
    /// Stages with `depends_on` that aren't yet satisfied are stored here until
    /// `trigger_pending_stages` finds their deps complete.
    ///
    /// # Group naming
    /// Group name is `"crew-{task[..20]}"` — consistent between `agent_crew` and `session_manager`.
    pub async fn register_pending_stages(
        &self,
        group: String,
        pending_stages: Vec<agent::tools::agent_crew::PendingStageSpec>,
    ) {
        let (resp_sender, rx) = oneshot::channel();
        let _ = self
            .tx
            .send(SessionManagerRequest {
                session_id: SessionId::new(String::new()),
                data: SessionManagerRequestData::RegisterPendingStages {
                    group,
                    pending_stages,
                    resp_sender,
                },
            })
            .await;
        let _ = rx.await;
    }

    /// Called when a session completes — spawns any pending stages whose deps are now all done.
    ///
    /// # Algorithm
    /// 1. Find the group of `completed_name` in `orchestrated_sessions`.
    /// 2. Build `completed` = set of session names in that group with status `Terminated`.
    /// 3. Find pending stages where ALL `depends_on` names are in `completed`.
    /// 4. Remove those stages from `groups[group].pending_stages`.
    /// 5. Spawn each via `handle_spawn_orchestrated` and fire `send_subagent_list_update`.
    ///
    /// # Important
    /// Uses session **names** (not IDs) for dependency matching. Stage names must be unique
    /// within a group — duplicate names cause premature triggering.
    pub async fn trigger_pending_stages(&self, completed_name: &str, parent_session_id: &SessionId) {
        let (resp_sender, rx) = oneshot::channel();
        let _ = self
            .tx
            .send(SessionManagerRequest {
                session_id: parent_session_id.clone(),
                data: SessionManagerRequestData::TriggerPendingStages {
                    completed_name: completed_name.to_string(),
                    parent_session_id: parent_session_id.clone(),
                    resp_sender,
                },
            })
            .await;
        let _ = rx.await;
    }

    // --- Orchestration handle methods ---

    #[allow(clippy::too_many_arguments)]
    pub async fn spawn_orchestrated_session(
        &self,
        parent_session_id: &SessionId,
        agent_name: String,
        task: String,
        name: Option<String>,
        role: Option<String>,
        group: Option<String>,
        persistent: bool,
    ) -> Result<SpawnOrchestratedResult, sacp::Error> {
        let (resp_sender, rx) = oneshot::channel();
        self.tx
            .send(SessionManagerRequest {
                session_id: parent_session_id.clone(),
                data: SessionManagerRequestData::SpawnOrchestratedSession {
                    parent_session_id: parent_session_id.clone(),
                    agent_name,
                    task,
                    name,
                    role,
                    group,
                    persistent,
                    resp_sender,
                },
            })
            .await
            .map_err(|_e| sacp::util::internal_error("Failed to send spawn request"))?;
        rx.await
            .map_err(|_e| sacp::util::internal_error("Failed to receive spawn response"))?
    }

    pub async fn send_orchestration_message(
        &self,
        from_session: &SessionId,
        target: Option<&str>,
        message: &str,
        is_escalation: bool,
    ) -> Result<(), sacp::Error> {
        let (resp_sender, rx) = oneshot::channel();
        self.tx
            .send(SessionManagerRequest {
                session_id: from_session.clone(),
                data: SessionManagerRequestData::SendOrchestrationMessage {
                    from_session: from_session.clone(),
                    target: target.map(String::from),
                    message: message.to_string(),
                    is_escalation,
                    resp_sender,
                },
            })
            .await
            .map_err(|_e| sacp::util::internal_error("Failed to send message request"))?;
        rx.await
            .map_err(|_e| sacp::util::internal_error("Failed to receive message response"))?
    }

    pub async fn read_orchestration_messages(
        &self,
        session_id: &SessionId,
        limit: usize,
    ) -> Result<Vec<InboxMessage>, sacp::Error> {
        let (resp_sender, rx) = oneshot::channel();
        self.tx
            .send(SessionManagerRequest {
                session_id: session_id.clone(),
                data: SessionManagerRequestData::ReadOrchestrationMessages {
                    session_id: session_id.clone(),
                    limit,
                    resp_sender,
                },
            })
            .await
            .map_err(|_e| sacp::util::internal_error("Failed to send read request"))?;
        rx.await
            .map_err(|_e| sacp::util::internal_error("Failed to receive read response"))?
    }

    pub async fn list_orchestrated_sessions(
        &self,
        filter: Option<SessionFilter>,
    ) -> Result<Vec<OrchestratedSession>, sacp::Error> {
        let (resp_sender, rx) = oneshot::channel();
        self.tx
            .send(SessionManagerRequest {
                session_id: SessionId::new(String::new()),
                data: SessionManagerRequestData::ListOrchestratedSessions { filter, resp_sender },
            })
            .await
            .map_err(|_e| sacp::util::internal_error("Failed to send list request"))?;
        rx.await
            .map_err(|_e| sacp::util::internal_error("Failed to receive list response"))?
    }

    pub async fn get_orchestrated_session_status(&self, target: &str) -> Result<OrchestratedSession, sacp::Error> {
        let (resp_sender, rx) = oneshot::channel();
        self.tx
            .send(SessionManagerRequest {
                session_id: SessionId::new(String::new()),
                data: SessionManagerRequestData::GetOrchestratedSessionStatus {
                    target: target.to_string(),
                    resp_sender,
                },
            })
            .await
            .map_err(|_e| sacp::util::internal_error("Failed to send status request"))?;
        rx.await
            .map_err(|_e| sacp::util::internal_error("Failed to receive status response"))?
    }

    pub async fn get_orchestrated_session_by_id(&self, session_id: &SessionId) -> Option<OrchestratedSession> {
        let (resp_sender, rx) = oneshot::channel();
        let _ = self
            .tx
            .send(SessionManagerRequest {
                session_id: session_id.clone(),
                data: SessionManagerRequestData::GetOrchestratedSessionById {
                    session_id: session_id.clone(),
                    resp_sender,
                },
            })
            .await;
        rx.await.unwrap_or(None)
    }

    pub async fn interrupt_orchestrated_session(
        &self,
        from_session: &SessionId,
        target: &str,
        message: &str,
    ) -> Result<(), sacp::Error> {
        let (resp_sender, rx) = oneshot::channel();
        self.tx
            .send(SessionManagerRequest {
                session_id: from_session.clone(),
                data: SessionManagerRequestData::InterruptOrchestratedSession {
                    from_session: from_session.clone(),
                    target: target.to_string(),
                    message: message.to_string(),
                    resp_sender,
                },
            })
            .await
            .map_err(|_e| sacp::util::internal_error("Failed to send interrupt request"))?;
        rx.await
            .map_err(|_e| sacp::util::internal_error("Failed to receive interrupt response"))?
    }

    pub async fn inject_orchestration_context(
        &self,
        from_session: &SessionId,
        target: &str,
        context: &str,
    ) -> Result<(), sacp::Error> {
        let (resp_sender, rx) = oneshot::channel();
        self.tx
            .send(SessionManagerRequest {
                session_id: from_session.clone(),
                data: SessionManagerRequestData::InjectOrchestrationContext {
                    from_session: from_session.clone(),
                    target: target.to_string(),
                    context: context.to_string(),
                    resp_sender,
                },
            })
            .await
            .map_err(|_e| sacp::util::internal_error("Failed to send inject request"))?;
        rx.await
            .map_err(|_e| sacp::util::internal_error("Failed to receive inject response"))?
    }

    pub async fn manage_orchestration_group(
        &self,
        from_session: &SessionId,
        action: GroupAction,
        group: Option<&str>,
        target: Option<&str>,
        role: Option<&str>,
        message: Option<&str>,
    ) -> Result<String, sacp::Error> {
        let (resp_sender, rx) = oneshot::channel();
        self.tx
            .send(SessionManagerRequest {
                session_id: from_session.clone(),
                data: SessionManagerRequestData::ManageOrchestrationGroup {
                    from_session: from_session.clone(),
                    action,
                    group: group.map(String::from),
                    target: target.map(String::from),
                    role: role.map(String::from),
                    message: message.map(String::from),
                    resp_sender,
                },
            })
            .await
            .map_err(|_e| sacp::util::internal_error("Failed to send group request"))?;
        rx.await
            .map_err(|_e| sacp::util::internal_error("Failed to receive group response"))?
    }

    pub async fn revive_orchestrated_session(
        &self,
        parent_session_id: &SessionId,
        target: &str,
        task: &str,
    ) -> Result<SpawnOrchestratedResult, sacp::Error> {
        let (resp_sender, rx) = oneshot::channel();
        self.tx
            .send(SessionManagerRequest {
                session_id: parent_session_id.clone(),
                data: SessionManagerRequestData::ReviveOrchestratedSession {
                    parent_session_id: parent_session_id.clone(),
                    target: target.to_string(),
                    task: task.to_string(),
                    resp_sender,
                },
            })
            .await
            .map_err(|_e| sacp::util::internal_error("Failed to send revive request"))?;
        rx.await
            .map_err(|_e| sacp::util::internal_error("Failed to receive revive response"))?
    }

    pub async fn get_session_live_activity(&self, target: &str) -> Option<String> {
        let (resp_sender, rx) = oneshot::channel();
        let _ = self
            .tx
            .send(SessionManagerRequest {
                session_id: SessionId::new(String::new()),
                data: SessionManagerRequestData::GetSessionLiveActivity {
                    target: target.to_string(),
                    resp_sender,
                },
            })
            .await;
        rx.await.ok().flatten()
    }

    pub async fn refresh_registry(
        &self,
        registry: crate::mcp_registry::McpRegistryResponse,
    ) -> Result<(), sacp::Error> {
        let (resp_sender, rx) = oneshot::channel();
        self.tx
            .send(SessionManagerRequest {
                session_id: SessionId::new(String::new()),
                data: SessionManagerRequestData::RefreshRegistry { registry, resp_sender },
            })
            .await
            .map_err(|_e| sacp::util::internal_error("Failed to send refresh request"))?;
        rx.await
            .map_err(|_e| sacp::util::internal_error("Failed to receive refresh response"))
    }

    pub async fn get_registry_data(&self) -> Option<crate::mcp_registry::McpRegistryResponse> {
        let (resp_sender, rx) = oneshot::channel();
        self.tx
            .send(SessionManagerRequest {
                session_id: SessionId::new(String::new()),
                data: SessionManagerRequestData::GetRegistryData { resp_sender },
            })
            .await
            .ok()?;
        rx.await.ok()?
    }
}
