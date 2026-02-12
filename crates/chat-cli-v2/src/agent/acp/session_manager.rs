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
    info,
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
#[derive(Debug, Clone, serde::Serialize)]
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
    local_mcp_path: Option<PathBuf>,
    global_mcp_path: Option<PathBuf>,
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

    pub fn spawn(self) -> SessionManagerHandle {
        let (tx, mut session_rx) = mpsc::channel::<SessionManagerRequest>(25);
        let Self {
            os,
            local_mcp_path,
            global_mcp_path,
        } = self;
        let os = os.expect("Os not found");

        let session_manager_handle = SessionManagerHandle { tx };
        let session_manager_handle_clone = session_manager_handle.clone();

        tokio::spawn(async move {
            // Load agent configs once at startup
            let agent_configs: Vec<LoadedAgentConfig> =
                load_agents(&RealProvider).await.map(|(c, _)| c).unwrap_or_default();

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
    /// Shared code intelligence client - lazily initialized, shared across all sessions
    code_intelligence: Option<Arc<RwLock<CodeIntelligence>>>,
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
            agent_configs,
            os,
            local_mcp_path,
            global_mcp_path,
            session_manager_handle,
            mock_registry,
            next_agent_name: None,
            code_intelligence: None,
        }
    }

    /// Get or initialize the shared CodeIntelligence client
    fn get_or_init_code_intelligence(&mut self, cwd: &Path) -> Option<Arc<RwLock<CodeIntelligence>>> {
        if self.code_intelligence.is_none() {
            match CodeIntelligence::builder()
                .workspace_root(cwd.to_path_buf())
                .auto_detect_languages()
                .build()
            {
                Ok(client) => {
                    info!("Initialized shared CodeIntelligence client");
                    self.code_intelligence = Some(Arc::new(RwLock::new(client)));
                },
                Err(e) => {
                    error!(
                        "Failed to initialize CodeIntelligence: {}. Code tool will be unavailable.",
                        e
                    );
                },
            }
        }
        self.code_intelligence.clone()
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
        debug!(?request, "session manager received new request");
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
                    .or_else(|| self.next_agent_name.take())
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

                    LoadedAgentConfig::new(ephemeral, ConfigSource::BuiltIn)
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
                    .code_intelligence(code_intel);

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
                    })
                    .collect();
                // Dedupe by name (keep first occurrence)
                let mut seen = std::collections::HashSet::new();
                available_agents.retain(|a| seen.insert(a.name.clone()));

                builder = builder.available_agents(available_agents.clone());
                builder = builder.agent_configs(self.agent_configs.clone());
                builder = builder.current_agent_name(agent_name.clone());

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
                if self.sessions.remove(&session_id).is_none() {
                    warn!(?session_id, "Attempted to terminate non-existent session");
                }
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
    SetMode {
        mode_id: String,
        resp_sender: oneshot::Sender<Result<(), sacp::Error>>,
    },
    SetNextAgentName {
        next_agent_name: String,
        resp_sender: oneshot::Sender<Result<(), sacp::Error>>,
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
}
