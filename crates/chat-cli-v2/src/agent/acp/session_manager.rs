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
    LoadedAgentConfig,
    load_agents,
};
use agent::consts::DEFAULT_AGENT_NAME;
use agent::tools::session::{
    GroupAction,
    SessionFilter,
};
use agent::util::providers::RealProvider;
use code_agent_sdk::CodeIntelligence;
use sacp::ConnectionTo;
use sacp::schema::SessionId;
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
use crate::agent::acp::orchestration::naming;
use crate::agent::acp::orchestration::permissions::PermissionStore;
use crate::agent::acp::orchestration::types::{
    GroupMembership,
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
    /// Whether MCP is enabled by governance. When `false`, the TUI should warn the user.
    pub mcp_enabled: bool,
    /// When `mcp_enabled=false`, distinguishes admin-disabled (`false`) from
    /// API-failure fail-closed path (`true`). Ignored when `mcp_enabled=true`.
    pub mcp_api_failure: bool,
    /// Whether web tools (web_search, web_fetch) are enabled by governance. When `false`, the TUI
    /// should warn the user. The fail-closed/API-failure distinction reuses `mcp_api_failure`
    /// since both come from the same GetProfile call.
    pub web_tools_enabled: bool,
}

/// Whether a changed path is a config file we care about: an agent config
/// (lives under an `agents`/`cli-agents` directory) or an `mcp.json`. Keeps
/// the watched config directories from triggering reloads on unrelated files.
fn is_relevant_config_path(path: &std::path::Path) -> bool {
    if path.file_name().is_some_and(|n| n == "mcp.json") {
        return true;
    }
    // Agent configs are JSON files under an `agents`/`cli-agents` directory.
    // Require a `.json` extension so unrelated file types in those dirs (logs,
    // editor temp files) don't trigger reloads. Extension-less paths (e.g.
    // directory create/remove events) pass so a newly-created agents dir is
    // still noticed.
    path.extension().is_none_or(|e| e == "json")
        && path
            .components()
            .any(|c| matches!(c.as_os_str().to_str(), Some("agents" | "cli-agents")))
}

/// Resolve the directories to watch for config hot-reload.
///
/// V2 agents live in `.kiro/agents` (workspace) and `~/.kiro/agents` (global);
/// these paths come from `PathResolver` so they can't drift from the loader.
/// The legacy `.amazonq` / `~/.aws/amazonq` "cli-agents" locations are read-only
/// migration fallbacks (Q-CLI), not V2-native, so they are deliberately not
/// watched.
///
/// Kiro owns `~/.kiro`, so the global agents dir is created here and watched
/// live — a fresh install hot-reloads without a restart and without the noisy
/// `$HOME` sentinel the previous design used. The workspace `.kiro/agents` dir
/// is only materialized when its `.kiro` parent already exists, so we never
/// create `.kiro` in an arbitrary working directory. `mcp.json` parents are
/// watched only if they already exist. Returns existing, deduplicated dirs;
/// each is watched non-recursively.
fn resolve_watch_targets(
    workspace_agents_dir: Option<&std::path::Path>,
    global_agents_dir: Option<&std::path::Path>,
    local_mcp_path: Option<&std::path::Path>,
    global_mcp_path: Option<&std::path::Path>,
) -> Vec<PathBuf> {
    let mut targets: Vec<PathBuf> = Vec::new();

    // Global agents dir: kiro-owned, created so it's watched live from a fresh install.
    if let Some(global) = global_agents_dir {
        let _ = std::fs::create_dir_all(global);
        targets.push(global.to_path_buf());
    }

    // Workspace `.kiro/agents`: only materialized when its `.kiro` parent already
    // exists — never create `.kiro` in an arbitrary working directory.
    if let Some(workspace) = workspace_agents_dir {
        if workspace.parent().is_some_and(|kiro| kiro.exists()) {
            let _ = std::fs::create_dir_all(workspace);
        }
        targets.push(workspace.to_path_buf());
    }

    // mcp.json parents — watched only if they already exist.
    for parent in [local_mcp_path, global_mcp_path]
        .into_iter()
        .flatten()
        .filter_map(|p| p.parent())
    {
        targets.push(parent.to_path_buf());
    }

    // Watch only dirs that exist (the kiro-owned ones we just created always
    // will), deduplicated.
    let mut resolved: Vec<PathBuf> = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for t in targets {
        if t.exists() && seen.insert(t.clone()) {
            resolved.push(t);
        }
    }
    resolved
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
            // Load agent configs once at startup. Custom agents inherit default resources unless
            // `chat.disableInheritingDefaultResources` is set (defaults to false → inherit).
            let inherit_default_resources = !os
                .database
                .settings
                .get_bool(Setting::ChatDisableInheritingDefaultResources)
                .unwrap_or(false);
            let (agent_configs, agent_config_errors): (Vec<LoadedAgentConfig>, Vec<AgentConfigLoadError>) =
                match load_agents(&RealProvider, inherit_default_resources).await {
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

            // Fetch MCP and web tools governance in a single GetProfile call for enterprise/API key users.
            // Skip for non-enterprise, non-API-key users (Builder ID, social auth) and test mode —
            // these users are not subject to admin governance, so default MCP enabled / web tools enabled
            // and avoid an unnecessary (and unauthorized) API call.
            // Fail-closed for enterprise/API-key users: MCP disabled and web tools disabled if GetProfile
            // fails.
            let is_enterprise = crate::auth::builder_id::is_enterprise_user(&os.database).await;
            let is_api_key = crate::util::env_var::get_api_key().is_some();
            let (mcp_enabled, mcp_registry_data, mcp_registry_url, web_tools_enabled, mcp_api_failure) = {
                // In debug builds, allow overriding the registry URL for local testing (bypasses governance).
                let registry_override = if cfg!(debug_assertions) {
                    std::env::var("KIRO_MCP_REGISTRY_URL_OVERRIDE").ok()
                } else {
                    None
                };

                if let Some(override_url) = registry_override {
                    tracing::warn!("Using KIRO_MCP_REGISTRY_URL_OVERRIDE={}", override_url);
                    let client = crate::mcp_registry::McpRegistryClient::new();
                    let registry_data = match client.fetch_registry(&override_url).await {
                        Ok(registry) => {
                            info!(
                                servers = registry.servers.len(),
                                "Fetched override registry from {}", override_url
                            );
                            Some(registry)
                        },
                        Err(e) => {
                            error!(%e, "Failed to fetch override registry");
                            None
                        },
                    };
                    (true, registry_data, Some(override_url), true, false)
                } else if std::env::var(KIRO_TEST_MODE).is_ok() || (!is_enterprise && !is_api_key) {
                    // Builder ID / social auth / test mode: no governance applies — MCP on, web tools on.
                    (true, None, None, true, false)
                } else {
                    match os.client.get_governance_config().await {
                        Ok((mcp_enabled, Some(registry_url), web_tools_enabled)) if mcp_enabled => {
                            let client = crate::mcp_registry::McpRegistryClient::new();
                            let registry_data = match client.fetch_registry(&registry_url).await {
                                Ok(registry) => {
                                    info!(
                                        servers = registry.servers.len(),
                                        "Fetched MCP registry from {}", registry_url
                                    );
                                    Some(registry)
                                },
                                Err(first_err) => {
                                    tracing::warn!(%first_err, "Registry fetch failed, retrying in 1s");
                                    tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                                    match client.fetch_registry(&registry_url).await {
                                        Ok(registry) => {
                                            info!(
                                                servers = registry.servers.len(),
                                                "Fetched MCP registry from {} (retry)", registry_url
                                            );
                                            Some(registry)
                                        },
                                        Err(e) => {
                                            error!(%e, "Failed to fetch MCP registry — registry servers disabled for this session");
                                            Some(crate::mcp_registry::McpRegistryResponse { servers: vec![] })
                                        },
                                    }
                                },
                            };
                            (true, registry_data, Some(registry_url), web_tools_enabled, false)
                        },
                        Ok((mcp_enabled, _, web_tools_enabled)) => (mcp_enabled, None, None, web_tools_enabled, false),
                        Err(e) => {
                            error!(%e, "Failed to get governance config from API — MCP disabled, web tools disabled");
                            // Fail closed: treat as MCP disabled so that no user-configured MCP
                            // servers are launched and no registry servers are advertised.
                            // `mcp_api_failure=true` so the TUI surfaces the correct message
                            // ("Failed to retrieve MCP settings") instead of the admin-disabled one.
                            (false, None, None, false, true)
                        },
                    }
                }
            };

            // Enforce MCP governance on every loaded agent config up-front so downstream
            // consumers (session start, /agent switch, session-injected servers) cannot
            // accidentally launch MCP when the console setting is off.
            let mut agent_configs = agent_configs;
            debug!(
                mcp_enabled,
                web_tools_enabled,
                mcp_api_failure,
                is_enterprise,
                is_api_key,
                has_registry = mcp_registry_data.is_some(),
                "MCP governance resolved"
            );
            if !mcp_enabled {
                for cfg in &mut agent_configs {
                    cfg.config_mut().clear_mcp_configs();
                }
                if is_enterprise || is_api_key {
                    warn!(
                        "MCP functionality has been disabled by governance — user-configured and registry MCP servers are suppressed for this session"
                    );
                }
            }

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

            let mandatory_mcp_names: Vec<String> = std::env::var("ASBX_KIRO_MANDATORY_MCPS")
                .ok()
                .filter(|s| !s.is_empty())
                .map(|s| {
                    s.split(',')
                        .map(|n| n.trim().to_string())
                        .filter(|n| !n.is_empty())
                        .collect()
                })
                .unwrap_or_default();

            // Spawn file watcher for agent config directories (debounced reload on change)
            if std::env::var(KIRO_TEST_MODE).is_err() {
                let sm_handle = session_manager_handle_clone.clone();
                let watch_targets = {
                    // Resolve agent dirs via the shared PathResolver (same source
                    // the caller/loader use) so the watcher can't drift. We watch
                    // the V2 kiro location only — `agents_dir_for_create()` returns
                    // `.kiro/agents` without the legacy cli-agents migration fallback.
                    let resolver = crate::util::paths::PathResolver::new(&os);
                    resolve_watch_targets(
                        resolver.workspace().agents_dir_for_create().ok().as_deref(),
                        resolver.global().agents_dir_for_create().ok().as_deref(),
                        local_mcp_path.as_deref(),
                        global_mcp_path.as_deref(),
                    )
                };

                if !watch_targets.is_empty() {
                    // ponytail: the watcher task lives for the process lifetime — there is
                    // no shutdown handle. Fine for today's single, long-lived session
                    // manager; add a cancellation token if managers become reconstructable.
                    tokio::spawn(async move {
                        use notify::{
                            RecursiveMode,
                            Watcher,
                        };

                        let (tx, mut rx) = tokio::sync::mpsc::channel(16);
                        let mut watcher = match notify::RecommendedWatcher::new(
                            move |res: Result<notify::Event, notify::Error>| {
                                if let Ok(event) = res {
                                    // Only trigger on content changes, not metadata
                                    if matches!(
                                        event.kind,
                                        notify::EventKind::Create(_)
                                            | notify::EventKind::Modify(notify::event::ModifyKind::Data(_))
                                            | notify::EventKind::Remove(_)
                                    ) && event.paths.iter().any(|p| is_relevant_config_path(p))
                                    {
                                        // Coalescing signal: the consumer only cares that
                                        // *something* changed and debounces, so a full channel
                                        // is benign. Trace-log the drop to aid debugging.
                                        if let Err(e) = tx.try_send(()) {
                                            tracing::trace!(
                                                ?e,
                                                "config watcher: change-signal channel full, coalescing"
                                            );
                                        }
                                    }
                                }
                            },
                            notify::Config::default(),
                        ) {
                            Ok(w) => w,
                            Err(e) => {
                                warn!(%e, "Failed to create agent config file watcher");
                                return;
                            },
                        };

                        for dir in &watch_targets {
                            // Non-recursive: V2 agent dirs are flat and mcp.json is a
                            // direct child of its watched parent.
                            if let Err(e) = watcher.watch(dir, RecursiveMode::NonRecursive) {
                                warn!(?dir, %e, "Failed to watch config path");
                            } else {
                                info!(?dir, "Watching config path for changes");
                            }
                        }

                        // Debounce: wait 500ms after last event before reloading
                        loop {
                            if rx.recv().await.is_none() {
                                break;
                            }
                            // Drain rapid-fire events and wait for quiescence
                            while let Ok(Some(())) =
                                tokio::time::timeout(std::time::Duration::from_millis(500), rx.recv()).await
                            {
                            }
                            info!("Agent config file change detected, reloading");
                            if let Err(e) = sm_handle.reload_agent_configs().await {
                                error!(%e, "Failed to send agent config reload request");
                            }
                        }
                    });
                }
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
                web_tools_enabled,
                mcp_enabled,
                mcp_api_failure,
                mandatory_mcp_names,
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
/// Result for a single stage in a completed group.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct GroupStageResult {
    pub name: String,
    pub result: Option<String>,
    /// Number of loop iterations this stage completed (0 if no loop).
    pub loop_iterations_used: u32,
}

/// Result delivered to a blocking group waiter: stage results on success, or a
/// human-readable error string when a stage fails (fail-fast).
type GroupCompletionResult = Result<Vec<GroupStageResult>, String>;

/// Sender for group completion notifications.
type GroupCompletionSender = oneshot::Sender<GroupCompletionResult>;

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
    /// Effort level to use for the next session, set via `--effort` CLI flag.
    next_effort: Option<String>,
    /// Shared code intelligence clients - lazily initialized per CWD, shared across sessions
    code_intelligence: HashMap<PathBuf, Arc<RwLock<CodeIntelligence>>>,
    /// When true, all tool permission checks are bypassed for new sessions
    trust_all_tools: bool,
    /// Specific tools to trust for new sessions (from --trust-tools CLI flag)
    trust_tools: Option<Vec<String>>,
    /// ACP client identity from InitializeRequest, propagated to all sessions
    acp_client_info: Option<crate::telemetry::AcpClientInfo>,
    /// Telemetry event store for recording events in test scenarios.
    /// Shared with the IPC server so tests can drain and assert on events. `None` in production.
    telemetry_event_store: Option<TelemetryEventStore>,
    /// Orchestration: permission tracking for messaging
    permission_store: PermissionStore,
    /// Orchestration: metadata about orchestrated sessions
    orchestrated_sessions: HashMap<String, OrchestratedSession>,
    /// Orchestration: session groups
    groups: HashMap<String, SessionGroup>,
    /// Shared TUI connection — cloned into every AcpSession (main + subagent)
    connection_cx: Option<ConnectionTo<sacp::Client>>,
    /// Pending group completion waiters: group_name -> sender
    group_completion_waiters: HashMap<String, GroupCompletionSender>,
    /// Map of group_name -> error message. Records a stage failure that occurs
    /// before the parent registers its blocking waiter; drained by the next
    /// WaitForGroupCompletion so the parent's tool fails fast instead of hanging.
    group_failures: HashMap<String, String>,
    /// V1 session exporter for lazy migration of V1 conversations.
    legacy_session_exporter: Arc<dyn LegacySessionExporter>,
    /// Agent config errors encountered during loading at startup.
    agent_config_errors: Vec<AgentConfigLoadError>,
    /// MCP registry data for enterprise users, fetched once at startup
    mcp_registry_data: Option<crate::mcp_registry::McpRegistryResponse>,
    /// Whether web tools (web_search, web_fetch) are enabled by governance
    web_tools_enabled: bool,
    /// Whether MCP is enabled by governance (Kiro console MCP toggle).
    /// When `false`, all MCP servers (user-configured, legacy, registry, and session-injected)
    /// are suppressed for every session in this process — mirroring Kiro IDE / `--classic`.
    mcp_enabled: bool,
    /// When `mcp_enabled=false`, distinguishes admin-disabled (`false`) from
    /// API-failure fail-closed path (`true`). Forwarded to the TUI so it can show
    /// the correct user-facing message.
    mcp_api_failure: bool,
    /// MCP server names from ASBX_KIRO_MANDATORY_MCPS that must always be loaded,
    /// bypass tool filtering, and survive agent swaps.
    mandatory_mcp_names: Vec<String>,
}

/// An agent config error with optional file path.
#[derive(Debug, Clone)]
pub struct AgentConfigLoadError {
    pub path: Option<String>,
    pub message: String,
}

impl SessionManager {
    /// Max time to wait for a session's graceful `shutdown()` (which tears down its
    /// MCP child processes) before giving up, so a wedged server can't block the
    /// single-threaded SessionManager loop.
    const SHUTDOWN_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(4);

    pub fn builder() -> SessionManagerBuilder {
        Default::default()
    }

    #[allow(clippy::too_many_arguments, clippy::fn_params_excessive_bools)]
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
        web_tools_enabled: bool,
        mcp_enabled: bool,
        mcp_api_failure: bool,
        mandatory_mcp_names: Vec<String>,
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
            next_effort: None,
            code_intelligence: HashMap::new(),
            trust_all_tools,
            trust_tools,
            acp_client_info: None,
            telemetry_event_store,
            permission_store: PermissionStore::new(),
            orchestrated_sessions: HashMap::new(),
            groups: HashMap::new(),
            connection_cx: None,
            group_completion_waiters: HashMap::new(),
            group_failures: HashMap::new(),
            legacy_session_exporter,
            agent_config_errors,
            mcp_registry_data,
            web_tools_enabled,
            mcp_enabled,
            mcp_api_failure,
            mandatory_mcp_names,
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
    /// Select the sessions in `group` that must be actively cancelled when a
    /// stage fails fast: every session in the group that is still running and is
    /// not the failed stage itself. Already-terminated stages have nothing to
    /// cancel, and the failed stage is excluded so it is never targeted twice.
    fn siblings_to_cancel(
        orchestrated_sessions: &HashMap<String, OrchestratedSession>,
        group: &str,
        failed_stage: &str,
    ) -> Vec<SessionId> {
        orchestrated_sessions
            .values()
            .filter(|s| s.group.as_deref() == Some(group))
            .filter(|s| s.name != failed_stage)
            .filter(|s| s.status != SessionStatus::Terminated)
            .map(|s| s.session_id.clone())
            .collect()
    }

    fn collect_group_results(&self, group_name: &str) -> Vec<GroupStageResult> {
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
        // Stages that are loop targets should always be included in results
        // even if they're depended-on, so users can see intermediate outputs.
        let loop_targets: std::collections::HashSet<&str> = group
            .iter()
            .filter_map(|s| s.loop_config.as_ref().map(|lc| lc.target.as_str()))
            .collect();
        group
            .iter()
            .filter(|s| !depended_on.contains(s.name.as_str()) || loop_targets.contains(s.name.as_str()))
            .map(|s| GroupStageResult {
                name: s.name.clone(),
                result: s.result.clone(),
                loop_iterations_used: s.loop_iteration,
            })
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

    /// Gracefully shut down a session handle, bounded by [`Self::SHUTDOWN_TIMEOUT`].
    /// Returns `true` if shutdown completed in time, `false` if it timed out (logs a
    /// warning, with `context` describing the call site).
    async fn shutdown_session(session_id: &SessionId, handle: &AcpSessionHandle, context: &str) -> bool {
        if tokio::time::timeout(Self::SHUTDOWN_TIMEOUT, handle.shutdown())
            .await
            .is_err()
        {
            warn!(?session_id, context, "Session did not shut down within timeout");
            false
        } else {
            true
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

        // The agent applies its stored MCP registry to the swapped-in config in
        // `handle_swap_agent`, so we no longer pre-rewrite the config here.
        session
            .swap_agent(agent_config.clone())
            .await
            .map_err(|e| sacp::util::internal_error(format!("Failed to swap agent: {}", e)))?;

        Ok(())
    }

    async fn handle_reload_agent_configs(&mut self) {
        info!("Reloading agent configs from disk");
        // Custom agents inherit default resources unless
        // `chat.disableInheritingDefaultResources` is set (defaults to false → inherit).
        let inherit_default_resources = !self
            .os
            .database
            .settings
            .get_bool(Setting::ChatDisableInheritingDefaultResources)
            .unwrap_or(false);
        match load_agents(&RealProvider, inherit_default_resources).await {
            Ok((mut configs, errors)) => {
                // Re-apply MCP governance if MCP is disabled
                if !self.mcp_enabled {
                    for cfg in &mut configs {
                        cfg.config_mut().clear_mcp_configs();
                    }
                }
                let loaded_count = configs.len();
                let error_count = errors.len();
                for err in &errors {
                    error!(%err, "Agent config error during reload");
                }
                self.agent_configs = configs;
                self.agent_config_errors = errors
                    .into_iter()
                    .map(|e| match e {
                        agent::agent_config::AgentConfigError::InvalidAgentConfig { path, message } => {
                            AgentConfigLoadError {
                                path: Some(path),
                                message,
                            }
                        },
                        other => AgentConfigLoadError {
                            path: None,
                            message: other.to_string(),
                        },
                    })
                    .collect();

                info!(loaded_count, error_count, "Agent configs reloaded");

                // Push updated configs to active sessions
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
                let mut seen = std::collections::HashSet::new();
                available_agents.retain(|a| seen.insert(a.name.clone()));

                for (session_id, session_handle) in &self.sessions {
                    if let Err(e) = session_handle
                        .refresh_agent_configs(self.agent_configs.clone(), available_agents.clone())
                        .await
                    {
                        warn!(?session_id, %e, "Failed to push agent configs to session");
                    }
                }
            },
            Err(e) => {
                error!(%e, "Failed to reload agent configs");
            },
        }
    }

    async fn handle_refresh_registry(&mut self, registry: crate::mcp_registry::McpRegistryResponse) {
        info!("Refreshing MCP registry data ({} servers)", registry.servers.len());
        self.mcp_registry_data = Some(registry.clone());

        // Build a single adapter and clone the trait object into each session. The
        // adapter holds the response in an Arc, so cloning is cheap. The agent on the
        // other end re-applies the registry to its current config and reloads MCP
        // servers — the host no longer pre-rewrites configs here.
        // Build a single adapter (eagerly loads mcp.json registry-type overrides) and
        // clone the trait object into each session. The adapter holds the response and
        // override map in Arcs, so cloning is cheap.
        let adapter = crate::mcp_registry::RegistryAdapter::new(
            registry,
            self.local_mcp_path.as_ref(),
            self.global_mcp_path.as_ref(),
        )
        .await;
        let registry_template: Box<dyn agent::mcp::McpRegistry> = Box::new(adapter);

        for (session_id, session_handle) in &self.sessions {
            if let Err(e) = session_handle.refresh_mcp_registry(registry_template.clone()).await {
                warn!(?session_id, %e, "Failed to queue registry refresh for session");
            }
        }
    }

    /// Resolve the agent name by precedence: explicit per-session agent >
    /// persisted (`session/load`) > `--agent` flag > `chat.defaultAgent` >
    /// built-in default. `cli_agent` sits below `persisted_agent` so loading a
    /// session restores its own agent rather than the startup flag.
    fn resolve_agent_name(
        initial_agent_name: Option<String>,
        persisted_agent: Option<String>,
        cli_agent: Option<String>,
        default_setting: Option<String>,
    ) -> String {
        initial_agent_name
            .or(persisted_agent)
            .or(cli_agent)
            .or(default_setting)
            .unwrap_or_else(|| agent::consts::DEFAULT_AGENT_NAME.to_string())
    }

    fn resolve_model_id(explicit_model: Option<String>, cli_model: &mut Option<String>) -> Option<String> {
        explicit_model.or_else(|| cli_model.take())
    }

    fn orchestrated_session_config(
        session_id: String,
        cwd: PathBuf,
        parent_session_id: String,
        agent_name: String,
        model_id: Option<String>,
        embedded_msg: String,
    ) -> AcpSessionConfig {
        AcpSessionConfig::new(session_id, cwd)
            .parent_session_id(parent_session_id)
            .initial_agent_name(agent_name)
            .model_id(model_id)
            .user_embedded_msg(embedded_msg)
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

                // If loading an existing session that doesn't exist as V2, try exporting from V1.
                //
                // TODO: route this through `chat _ ensure-session
                // --target-format v2` so the conversion lives in one
                // place. The host (TUI / autocomplete / external ACP
                // client) would call `ensure-session` before
                // `session/load`, and this lazy fallback can be
                // deleted once every known caller is confirmed to do
                // so.
                if config.load
                    && let Ok(sessions_dir) = crate::util::paths::sessions_dir()
                    && !crate::agent::session::session_exists(&sessions_dir, &config.session_id)
                    && let Err(e) = self
                        .legacy_session_exporter
                        .export_session(&config.session_id, &sessions_dir)
                {
                    warn!(session_id = %config.session_id, error = %e, "Failed to export V1 session");
                }

                // Resolve agent name. Precedence (highest first): explicit
                // config > persisted session agent (session/load) > CLI
                // --agent flag > chat.defaultAgent > built-in default.
                //
                // The --agent value is cloned, not taken, so it applies to
                // every new session for the subprocess lifetime. It sits below
                // the persisted agent so loading a session restores its own
                // saved agent rather than the startup flag.
                let persisted_agent = if config.load {
                    let sessions_dir = crate::util::paths::sessions_dir().ok();
                    sessions_dir.and_then(|d| crate::agent::session::peek_agent_name(&d, &config.session_id))
                } else {
                    None
                };
                let agent_name = Self::resolve_agent_name(
                    config.initial_agent_name.clone(),
                    persisted_agent,
                    self.next_agent_name.clone(),
                    self.os.database.settings.get_string(Setting::ChatDefaultAgent),
                );

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

                // If ACP client provided MCP servers, create an ephemeral config with them merged in.
                // When MCP is disabled by governance, drop them entirely to preserve parity with
                // Kiro IDE / kiro-cli --classic.
                let converted_mcp_servers: Vec<_> = if self.mcp_enabled {
                    config
                        .mcp_servers
                        .into_iter()
                        .filter_map(|server| match convert_mcp_server(server) {
                            Ok((name, cfg)) => Some((name, cfg)),
                            Err(e) => {
                                warn!(?e, "Failed to convert MCP server, skipping");
                                None
                            },
                        })
                        .collect()
                } else {
                    if !config.mcp_servers.is_empty() {
                        warn!(
                            count = config.mcp_servers.len(),
                            "Dropping session-injected MCP servers: MCP disabled by governance"
                        );
                    }
                    Vec::new()
                };

                let agent_config_to_use: LoadedAgentConfig = if !converted_mcp_servers.is_empty() {
                    let mut ephemeral = base_agent_config.clone();
                    if let Some(overridden) = ephemeral.add_mcp_servers_with_source(
                        converted_mcp_servers.clone(),
                        agent::agent_config::McpServerConfigSource::AcpInjected,
                    ) {
                        warn!(?overridden, "ACP MCP servers override existing servers in agent config");
                    }

                    ephemeral
                } else {
                    base_agent_config.clone()
                };

                // The agent applies the registry to its config at construction (and on
                // every swap / refresh). The adapter eagerly loads any `"type": "registry"`
                // overrides from mcp.json so env / headers / timeout overrides for
                // registry-managed servers (e.g. `BRAVE_API_KEY` for `npm-brave-search`)
                // surface into the resolved Local/Remote config.
                let mcp_registry: Option<Box<dyn agent::mcp::McpRegistry>> = match self.mcp_registry_data.as_ref() {
                    Some(r) => {
                        let adapter = crate::mcp_registry::RegistryAdapter::new(
                            r.clone(),
                            self.local_mcp_path.as_ref(),
                            self.global_mcp_path.as_ref(),
                        )
                        .await;
                        Some(Box::new(adapter))
                    },
                    None => None,
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
                    .set_as_subagent(
                        config.parent_session_id.is_some()
                            && matches!(
                                config.session_created_reason,
                                crate::agent::session::SessionCreatedReason::Subagent
                            ),
                    )
                    .parent_session_id(config.parent_session_id.clone())
                    .code_intelligence(code_intel)
                    .trust_all_tools(self.trust_all_tools)
                    .trust_tools(self.trust_tools.clone())
                    .web_tools_enabled(self.web_tools_enabled)
                    .mcp_enabled(self.mcp_enabled)
                    .mandatory_mcp_names(self.mandatory_mcp_names.clone())
                    .acp_client_info(self.acp_client_info.clone())
                    .telemetry_event_store(self.telemetry_event_store.clone())
                    .legacy_session_exporter(Arc::clone(&self.legacy_session_exporter))
                    .session_injected_mcp_servers(converted_mcp_servers)
                    .mcp_registry(mcp_registry);

                // Pass client connection to session
                if let Some(cx) = connection_cx {
                    // Main session — store connection for subagents to clone
                    if self.connection_cx.is_none() {
                        self.connection_cx = Some(cx.clone());
                    }
                    builder = builder.connection_cx(cx);
                } else if config.parent_session_id.is_some() {
                    // Subagent or rewind-fork session — clone the stored connection
                    // (any session derived from a parent reuses the parent's connection).
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

                // Explicit per-session model takes precedence over the CLI override.
                let next_model_id = Self::resolve_model_id(config.model_id.clone(), &mut self.next_model_id);
                if let Some(ref model_id) = next_model_id {
                    builder = builder.model_id(Some(model_id.as_str()));
                }

                // Pass CLI --effort override to session builder
                let next_effort = self.next_effort.take();
                if let Some(ref effort) = next_effort {
                    builder = builder.effort(Some(effort.as_str()));
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

                // If this id is already active it can only be a `session/load` re-load
                // (`session/new` always uses a fresh UUID). Shut the old instance down
                // BEFORE building the new one: both map to the same on-disk session files,
                // and the old SessionDb releases its `.lock` on shutdown, so the new owner
                // can acquire cleanly. If the old session doesn't shut down in time, fail
                // the reload rather than risk two live owners of the same files — the
                // client can retry once the previous instance has drained.
                if let Some(old_handle) = self.sessions.remove(&session_id) {
                    warn!(
                        ?session_id,
                        "Reloading active session — shutting down previous instance"
                    );
                    if !Self::shutdown_session(&session_id, &old_handle, "reload").await {
                        _ = resp_sender.send(Err(sacp::util::internal_error(
                            "Previous session is still shutting down; please retry the load",
                        )));
                        return;
                    }
                }

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
                            requested_agent_name,
                            available_agents,
                            available_models,
                            current_model_id,
                            agent_config_errors: self.agent_config_errors.clone(),
                            mcp_enabled: self.mcp_enabled,
                            mcp_api_failure: self.mcp_api_failure,
                            web_tools_enabled: self.web_tools_enabled,
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
                    Self::shutdown_session(&session_id, &handle, "terminate").await;
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
                    .map(|(id, h)| Self::shutdown_session(id, h, "process-shutdown"))
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
            SessionManagerRequestData::SetNextEffort {
                next_effort,
                resp_sender,
            } => {
                self.next_effort = Some(next_effort);
                _ = resp_sender.send(Ok(()));
            },
            SessionManagerRequestData::UpdateSetting {
                key,
                value,
                resp_sender,
            } => {
                let result = self
                    .os
                    .database
                    .settings
                    .set(key, value, None)
                    .await
                    .map_err(|e| sacp::util::internal_error(format!("Failed to update setting: {e}")));
                _ = resp_sender.send(result);
            },
            SessionManagerRequestData::Initialize {
                name,
                version,
                resp_sender,
            } => {
                // Stamp the driving ACP client name so child `aws` CLI invocations carry an
                // `acp-client/<name>` userAgent token for CloudTrail attribution. This is
                // intentionally separate from `KIRO_CLI_CLIENT_APPLICATION`, telemetry, and the
                // SDK user-agent interceptor, none of which are touched here. The sanitize+stamp
                // lives in `stamp_acp_client_name`; telemetry below still receives the RAW name.
                stamp_acp_client_name(&self.os.env, &name);
                self.acp_client_info = Some(crate::telemetry::AcpClientInfo::new(name, version));
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
                                    parent_session_id: None,
                                    session_created_reason: crate::agent::session::SessionCreatedReason::Subagent,
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
                            model: ps.model,
                            loop_config: ps
                                .loop_config
                                .map(|lc| crate::agent::acp::orchestration::types::LoopConfig {
                                    target: lc.target,
                                    max_iterations: lc.max_iterations,
                                    trigger: lc.trigger,
                                }),
                            loop_iteration: 0,
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
                    // A dependency name is "completed" only if the LATEST session
                    // with that name is terminated (not an old iteration still in the map).
                    let mut latest_by_name: std::collections::HashMap<&str, &OrchestratedSession> =
                        std::collections::HashMap::new();
                    for s in self.orchestrated_sessions.values() {
                        if s.group.as_deref() != Some(&gname) {
                            continue;
                        }
                        let entry = latest_by_name.entry(s.name.as_str()).or_insert(s);
                        if s.created_at > entry.created_at {
                            *entry = s;
                        }
                    }
                    let completed: std::collections::HashSet<String> = latest_by_name
                        .into_iter()
                        .filter(|(_, s)| s.status == SessionStatus::Terminated)
                        .map(|(name, _)| name.to_string())
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
                        let task_with_context = {
                            let dep_context: Vec<String> = deps
                                .iter()
                                .filter_map(|dep_name| {
                                    // Find the most recently created session with this name
                                    // (there may be multiple from loop iterations)
                                    self.orchestrated_sessions
                                        .values()
                                        .filter(|s| s.name == *dep_name && s.result.is_some())
                                        .max_by_key(|s| s.created_at)
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
                                stage.model.as_deref(),
                                &task_with_context,
                                Some(&stage.name),
                                Some(&stage.role),
                                Some(&gname),
                                false,
                                deps,
                                stage.loop_config.clone(),
                                stage.loop_iteration,
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
                        // Extract loop data from session before dropping the mutable borrow
                        let loop_data = Self::check_loop_trigger(session);

                        if let Some(data) = loop_data {
                            self.enqueue_loop_iteration(&group, &data);
                            self.spawn_ready_pending_stages(&group).await;
                        }

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
                                let _ = waiter.send(Ok(results));
                            }
                            self.group_failures.remove(&group);
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
                changes_needed,
                resp_sender,
            } => {
                if let Some(session) = self.orchestrated_sessions.get_mut(&sid.to_string()) {
                    session.result = Some(result);
                    session.changes_needed = changes_needed;
                }
                _ = resp_sender.send(());
            },
            SessionManagerRequestData::WaitForGroupCompletion {
                group_name,
                resp_sender,
            } => {
                // A stage may have already failed before this waiter registered.
                if let Some(error) = self.group_failures.remove(&group_name) {
                    let _ = resp_sender.send(Err(error));
                    self.orchestrated_sessions
                        .retain(|_, s| s.group.as_deref() != Some(&group_name));
                    self.groups.remove(&group_name);
                } else {
                    // Check if all sessions in group are already terminated
                    let all_done = self
                        .orchestrated_sessions
                        .values()
                        .filter(|s| s.group.as_deref() == Some(&group_name))
                        .all(|s| s.status == SessionStatus::Terminated);
                    if all_done {
                        let results = self.collect_group_results(&group_name);
                        let _ = resp_sender.send(Ok(results));
                        // Clean up completed group
                        self.orchestrated_sessions
                            .retain(|_, s| s.group.as_deref() != Some(&group_name));
                        self.groups.remove(&group_name);
                    } else {
                        // Store waiter — will be fired when last session terminates
                        self.group_completion_waiters.insert(group_name, resp_sender);
                    }
                }
            },
            SessionManagerRequestData::FailGroup {
                group_name,
                stage_name,
                error,
                resp_sender,
            } => {
                let message = format!("stage '{}' failed: {}", stage_name, error);
                // Surface final state to the TUI before tearing the group down.
                self.send_subagent_list_update().await;
                if let Some(waiter) = self.group_completion_waiters.remove(&group_name) {
                    let _ = waiter.send(Err(message));
                } else if self.groups.contains_key(&group_name) {
                    // Waiter not registered yet, but the group is still live, so
                    // a WaitForGroupCompletion is still expected — record the
                    // failure so it fails fast. If the group is already gone
                    // (e.g. another path already completed it, or this is a
                    // late/duplicate failure), do NOT record: the group key is
                    // reused across crews with the same task, and a stale entry
                    // would poison the next same-task crew.
                    self.group_failures.insert(group_name.clone(), message);
                }
                // Abandon the rest of the group: any sibling stages still
                // running are actively cancelled, so they cannot keep consuming
                // resources or deliver a late result to a parent whose crew tool
                // has already returned.
                let siblings = Self::siblings_to_cancel(&self.orchestrated_sessions, &group_name, &stage_name);
                for sibling_id in siblings {
                    if let Some(handle) = self.sessions.remove(&sibling_id)
                        && tokio::time::timeout(std::time::Duration::from_secs(4), handle.shutdown())
                            .await
                            .is_err()
                    {
                        warn!(
                            ?sibling_id,
                            "Sibling session did not shut down within timeout during group failure"
                        );
                    }
                }
                self.orchestrated_sessions
                    .retain(|_, s| s.group.as_deref() != Some(&group_name));
                self.groups.remove(&group_name);
                _ = resp_sender.send(());
            },
            // --- Orchestration handlers ---
            SessionManagerRequestData::SpawnOrchestratedSession {
                parent_session_id,
                agent_name,
                task,
                model,
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
                        model.as_deref(),
                        &task,
                        name.as_deref(),
                        role.as_deref(),
                        group.as_deref(),
                        persistent,
                        vec![],
                        None,
                        0,
                    )
                    .await;
                if result.is_ok() {
                    self.send_subagent_list_update().await;
                }
                _ = resp_sender.send(result);
            },
            SessionManagerRequestData::ListOrchestratedSessions { filter, resp_sender } => {
                let sessions = self.handle_list_orchestrated(filter);
                _ = resp_sender.send(Ok(sessions));
            },
            SessionManagerRequestData::GetOrchestratedSessionStatus { target, resp_sender } => {
                let result = self.handle_get_orchestrated_status(&target);
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
                action,
                group,
                target,
                role,
                resp_sender,
            } => {
                let result = self.handle_manage_group(action, group.as_deref(), target.as_deref(), role.as_deref());
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
            SessionManagerRequestData::ReloadAgentConfigs { resp_sender } => {
                self.handle_reload_agent_configs().await;
                _ = resp_sender.send(());
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
    ///    - Calls `start_session` with `parent_session_id` set (cloned `connection_cx`).
    ///    - Waits for MCP init (`ready_rx`).
    ///    - Calls `internal_prompt(task)` — blocks until the agent calls the `summary` tool.
    ///    - On success: stores the result on the orchestrated session, marks it `Terminated`,
    ///      terminates it, then calls `trigger_pending_stages` to advance the DAG.
    ///    - On error: fails the group, same cleanup.
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
        model: Option<&str>,
        task: &str,
        name: Option<&str>,
        role: Option<&str>,
        group: Option<&str>,
        persistent: bool,
        depends_on: Vec<String>,
        loop_config: Option<crate::agent::acp::orchestration::types::LoopConfig>,
        loop_iteration: u32,
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
        let group_name_for_task = group_name.clone();
        let orch_session = OrchestratedSession {
            session_id: new_session_id.clone(),
            name: session_name.clone(),
            role: role.map(String::from),
            agent_name: agent_name.to_string(),
            model: model.map(String::from),
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
            loop_config,
            loop_iteration,
            changes_needed: false,
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
        let model_id = model.map(String::from);
        let task_str = task.to_string();
        let session_name_clone = session_name.clone();
        let group_name_clone = group_name_for_task;
        let parent_sid = parent_session_id.clone();
        let embedded_msg = format!(
            "CRITICAL: You MUST call the `summary` tool before ending your turn. Do NOT end with a plain text response — always close out by calling the summary tool with your findings.\n\n\
             You are '{}' — an orchestrated session.\nYour task: {}\n{}\n\
             Reminder: When your task is complete, you MUST call the summary tool (not just respond with text).",
            session_name,
            task,
            role.map(|r| format!("Your role: {}", r)).unwrap_or_default(),
        );
        tokio::spawn(async move {
            let config = Self::orchestrated_session_config(
                new_sid.to_string(),
                std::env::current_dir().unwrap_or_default(),
                parent_sid.to_string(),
                agent_str,
                model_id,
                embedded_msg,
            );
            match session_tx.start_session(&new_sid, config, None).await {
                Ok(result) => {
                    let _ = result.ready_rx.await;
                    match result.handle.internal_prompt(task_str).await {
                        Ok(summary) => {
                            info!(name = %session_name_clone, "Orchestrated session completed task");
                            let changes_needed = summary.result_type.as_deref() == Some("changes_needed");
                            session_tx
                                .store_session_result(&new_sid, summary.task_result, changes_needed)
                                .await;
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
                            if cancelled {
                                // Stamp a placeholder result so the parent's
                                // agent_crew/subagent output shows "[Cancelled
                                // by user]" for this stage instead of an
                                // ambiguous "No result". The orch session
                                // table is read by collect_group_results to
                                // build the final agent_crew output; stages
                                // with `result: None` flow through as "No
                                // result" via the JSON formatting in
                                // session_tool_handler.rs, which historically
                                // looked indistinguishable from a silent
                                // backend failure and tempted the parent
                                // model to re-dispatch the work the user
                                // just killed.
                                session_tx
                                    .store_session_result(&new_sid, "[Cancelled by user]".to_string(), false)
                                    .await;
                            }
                            if persistent {
                                session_tx.update_session_status(&new_sid, SessionStatus::Idle).await;
                            } else {
                                // Mark terminated via terminate_session (which also prunes
                                // dependents). We intentionally do NOT call
                                // update_session_status(Terminated) here: its group-completion
                                // check cannot tell a failed stage (no result) from a
                                // successful one and would fire the waiter with a spurious
                                // "Ok/empty" result before fail_group can report the error.
                                session_tx.terminate_session(&new_sid).await;
                            }
                            // Fail-fast: a stage failed, so the blocking pipeline cannot
                            // complete. Abort the group's wait with an error instead of
                            // leaving the parent's subagent tool hanging on pruned
                            // dependents. Cancellation is driven by the parent itself, so
                            // it does not fail the group.
                            if !cancelled {
                                session_tx
                                    .fail_group(&group_name_clone, &session_name_clone, &e.to_string())
                                    .await;
                            }
                        },
                    }
                },
                Err(e) => {
                    error!("Failed to start orchestrated session {}: {}", new_sid, e);
                    session_tx
                        .update_session_status(&new_sid, SessionStatus::Terminated)
                        .await;
                    session_tx
                        .fail_group(&group_name_clone, &session_name_clone, &e.to_string())
                        .await;
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
                "Session '{}' is {:?} — only terminated sessions can be revived",
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
            model: old_session.model.clone(),
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
            loop_config: old_session.loop_config.clone(),
            loop_iteration: old_session.loop_iteration,
            changes_needed: false,
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
                            format!("{}...", agent::util::truncate_safe(text, 300))
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
                                        format!("{}...", agent::util::truncate_safe(t, 200))
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
                    agent::agent_loop::types::ContentBlock::Thinking(_) => {},
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
            .can_interact(from_session, &target_id)
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
            .can_interact(from_session, &target_id)
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
        action: GroupAction,
        group: Option<&str>,
        target: Option<&str>,
        role: Option<&str>,
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
        }
    }

    // ── Loop handling ─────────────────────────────────────────────────

    /// Check whether a terminated session's output triggers a loop-back.
    ///
    /// Prefers the structured `changes_needed` signal from the summary tool's
    /// `resultType` field. Falls back to text-matching the trigger in the tail
    /// of the session's result for backward compatibility.
    fn check_loop_trigger(
        session: &crate::agent::acp::orchestration::types::OrchestratedSession,
    ) -> Option<crate::agent::acp::orchestration::types::LoopTriggerData> {
        let cfg = session.loop_config.as_ref()?;
        let result = session.result.as_ref()?;

        if session.loop_iteration >= cfg.max_iterations {
            info!(
                stage = %session.name,
                iteration = session.loop_iteration,
                max = cfg.max_iterations,
                "Loop max iterations reached, stopping"
            );
            return None;
        }

        // Prefer structured signal: subagent explicitly set resultType = "changes_needed"
        let triggered = if session.changes_needed {
            true
        } else {
            // Fallback: text-match trigger in the tail of the output
            let check_region = if result.len() > 500 {
                // SAFETY: floor_char_boundary ensures we don't split a multi-byte char
                let start = result.floor_char_boundary(result.len() - 500);
                #[allow(clippy::string_slice)]
                &result[start..]
            } else {
                result.as_str()
            };
            check_region.contains(&cfg.trigger)
        };

        if !triggered {
            return None;
        }

        Some(crate::agent::acp::orchestration::types::LoopTriggerData {
            loop_config: cfg.clone(),
            iteration: session.loop_iteration,
            session_name: session.name.clone(),
            result_text: result.clone(),
            session_task: session.task.clone(),
            session_role: session.role.clone().unwrap_or_default(),
            agent_name: session.agent_name.clone(),
            model: session.model.clone(),
        })
    }

    /// A prior run of the target is authoritative; if the trigger fires before
    /// the target has ever run, its model still lives on its pending stage.
    fn loop_target_model(
        target_session: Option<&crate::agent::acp::orchestration::types::OrchestratedSession>,
        pending_stages: &[crate::agent::acp::orchestration::types::PendingStage],
        target_name: &str,
    ) -> Option<String> {
        target_session.and_then(|s| s.model.clone()).or_else(|| {
            pending_stages
                .iter()
                .find(|ps| ps.name == target_name)
                .and_then(|ps| ps.model.clone())
        })
    }

    /// Re-enqueue the target stage (with feedback) and the triggering stage
    /// (with incremented iteration) into the group's pending list.
    fn enqueue_loop_iteration(&mut self, group: &str, data: &crate::agent::acp::orchestration::types::LoopTriggerData) {
        let cfg = &data.loop_config;

        info!(
            stage = %data.session_name,
            target = %cfg.target,
            iteration = data.iteration + 1,
            max = cfg.max_iterations,
            "Loop triggered, re-enqueuing target stage"
        );

        // Look up the target stage's original task and role from existing sessions
        let target_session = self
            .orchestrated_sessions
            .values()
            .filter(|s| s.name == cfg.target && s.group.as_deref() == Some(group))
            .max_by_key(|s| s.created_at);

        let target_task = target_session.map_or_else(|| cfg.target.clone(), |s| s.task.clone());
        let target_role = target_session
            .and_then(|s| s.role.clone())
            .unwrap_or_else(|| cfg.target.clone());
        let target_model = Self::loop_target_model(
            target_session,
            self.groups.get(group).map_or(&[][..], |g| &g.pending_stages),
            &cfg.target,
        );

        let loop_task = format!(
            "{}\n\n---\n\n## Loop iteration {} (feedback from {})\n\n{}",
            target_task,
            data.iteration + 1,
            data.session_name,
            data.result_text
        );

        if let Some(g) = self.groups.get_mut(group) {
            // Target stage (e.g., implementer): no deps, ready to run immediately
            g.pending_stages
                .push(crate::agent::acp::orchestration::types::PendingStage {
                    name: cfg.target.clone(),
                    role: target_role.clone(),
                    task: loop_task,
                    depends_on: vec![],
                    agent_name: target_role,
                    model: target_model,
                    loop_config: None,
                    loop_iteration: 0,
                });
            // Triggering stage (e.g., reviewer): depends on target, carries loop config forward
            g.pending_stages
                .push(crate::agent::acp::orchestration::types::PendingStage {
                    name: data.session_name.clone(),
                    role: data.session_role.clone(),
                    task: data.session_task.clone(),
                    depends_on: vec![cfg.target.clone()],
                    agent_name: data.agent_name.clone(),
                    model: data.model.clone(),
                    loop_config: Some(crate::agent::acp::orchestration::types::LoopConfig {
                        target: cfg.target.clone(),
                        max_iterations: cfg.max_iterations,
                        trigger: cfg.trigger.clone(),
                    }),
                    loop_iteration: data.iteration + 1,
                });
        }
    }

    /// Drain ready pending stages (those with empty `depends_on`) from a group
    /// and spawn them as new sessions.
    async fn spawn_ready_pending_stages(&mut self, group: &str) {
        let ready: Vec<crate::agent::acp::orchestration::types::PendingStage> =
            if let Some(g) = self.groups.get_mut(group) {
                let r: Vec<_> = g
                    .pending_stages
                    .iter()
                    .filter(|ps| ps.depends_on.is_empty())
                    .cloned()
                    .collect();
                g.pending_stages.retain(|ps| !ps.depends_on.is_empty());
                r
            } else {
                return;
            };

        let parent = self
            .orchestrated_sessions
            .values()
            .find(|s| s.group.as_deref() == Some(group))
            .and_then(|s| s.parent_session.clone());

        let Some(parent_id) = parent else { return };

        for stage in ready {
            let _ = self
                .handle_spawn_orchestrated(
                    &parent_id,
                    &stage.agent_name,
                    stage.model.as_deref(),
                    &stage.task,
                    Some(&stage.name),
                    Some(&stage.role),
                    Some(group),
                    false,
                    stage.depends_on,
                    stage.loop_config,
                    stage.loop_iteration,
                )
                .await;
        }
    }

    /// Builds the `SubagentListUpdateNotification` and sends it to the TUI.
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
                has_loop: s.loop_config.is_some(),
                loop_iteration: s.loop_iteration,
                loop_max_iterations: s.loop_config.as_ref().map_or(0, |lc| lc.max_iterations),
                created_at_ms: s
                    .created_at
                    .duration_since(std::time::UNIX_EPOCH)
                    .map_or(0, |d| d.as_millis() as u64),
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
        connection_cx: Option<ConnectionTo<sacp::Client>>,
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
    SetNextEffort {
        next_effort: String,
        resp_sender: oneshot::Sender<Result<(), sacp::Error>>,
    },
    UpdateSetting {
        key: Setting,
        value: serde_json::Value,
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
        changes_needed: bool,
        resp_sender: oneshot::Sender<()>,
    },
    WaitForGroupCompletion {
        group_name: String,
        resp_sender: oneshot::Sender<GroupCompletionResult>,
    },
    FailGroup {
        group_name: String,
        stage_name: String,
        error: String,
        resp_sender: oneshot::Sender<()>,
    },
    // --- Orchestration requests ---
    SpawnOrchestratedSession {
        parent_session_id: SessionId,
        agent_name: String,
        task: String,
        model: Option<String>,
        name: Option<String>,
        role: Option<String>,
        group: Option<String>,
        persistent: bool,
        resp_sender: oneshot::Sender<Result<SpawnOrchestratedResult, sacp::Error>>,
    },
    ListOrchestratedSessions {
        filter: Option<SessionFilter>,
        resp_sender: oneshot::Sender<Result<Vec<OrchestratedSession>, sacp::Error>>,
    },
    GetOrchestratedSessionStatus {
        target: String,
        resp_sender: oneshot::Sender<Result<OrchestratedSession, sacp::Error>>,
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
        action: GroupAction,
        group: Option<String>,
        target: Option<String>,
        role: Option<String>,
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
    ReloadAgentConfigs {
        resp_sender: oneshot::Sender<()>,
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
        connection_cx: Option<ConnectionTo<sacp::Client>>,
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

    pub async fn set_next_effort(&self, next_effort: String) -> Result<(), sacp::Error> {
        let (resp_sender, rx) = oneshot::channel();
        self.tx
            .send(SessionManagerRequest {
                session_id: SessionId::new(String::new()),
                data: SessionManagerRequestData::SetNextEffort {
                    next_effort,
                    resp_sender,
                },
            })
            .await
            .map_err(|_e| sacp::util::internal_error("Failed to send set_next_effort request"))?;
        rx.await
            .map_err(|_e| sacp::util::internal_error("Failed to receive set_next_effort response"))?
    }

    pub async fn update_setting(&self, key: Setting, value: serde_json::Value) -> Result<(), sacp::Error> {
        let (resp_sender, rx) = oneshot::channel();
        self.tx
            .send(SessionManagerRequest {
                session_id: SessionId::new(String::new()),
                data: SessionManagerRequestData::UpdateSetting {
                    key,
                    value,
                    resp_sender,
                },
            })
            .await
            .map_err(|_e| sacp::util::internal_error("Failed to send update_setting request"))?;
        rx.await
            .map_err(|_e| sacp::util::internal_error("Failed to receive update_setting response"))?
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

    pub async fn store_session_result(&self, session_id: &SessionId, result: String, changes_needed: bool) {
        let (resp_sender, rx) = oneshot::channel();
        let _ = self
            .tx
            .send(SessionManagerRequest {
                session_id: session_id.clone(),
                data: SessionManagerRequestData::StoreSessionResult {
                    session_id: session_id.clone(),
                    result,
                    changes_needed,
                    resp_sender,
                },
            })
            .await;
        let _ = rx.await;
    }

    /// Wait for all sessions in a group to complete. Blocks until all are Terminated.
    pub async fn wait_for_group_completion(&self, group_name: String) -> GroupCompletionResult {
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
        rx.await
            .unwrap_or_else(|_| Err("Group wait channel dropped".to_string()))
    }

    /// Fail-fast: a stage in a blocking group failed. Abort the group's wait
    /// with an error so the parent's `subagent` tool returns instead of hanging
    /// on pruned dependents.
    pub async fn fail_group(&self, group: &str, stage_name: &str, error: &str) {
        let (resp_sender, rx) = oneshot::channel();
        let _ = self
            .tx
            .send(SessionManagerRequest {
                session_id: SessionId::new(String::new()),
                data: SessionManagerRequestData::FailGroup {
                    group_name: group.to_string(),
                    stage_name: stage_name.to_string(),
                    error: error.to_string(),
                    resp_sender,
                },
            })
            .await;
        let _ = rx.await;
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
        self.spawn_orchestrated_session_with_model(
            parent_session_id,
            agent_name,
            task,
            None,
            name,
            role,
            group,
            persistent,
        )
        .await
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn spawn_orchestrated_session_with_model(
        &self,
        parent_session_id: &SessionId,
        agent_name: String,
        task: String,
        model: Option<String>,
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
                    model,
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
    ) -> Result<String, sacp::Error> {
        let (resp_sender, rx) = oneshot::channel();
        self.tx
            .send(SessionManagerRequest {
                session_id: from_session.clone(),
                data: SessionManagerRequestData::ManageOrchestrationGroup {
                    action,
                    group: group.map(String::from),
                    target: target.map(String::from),
                    role: role.map(String::from),
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

    pub async fn reload_agent_configs(&self) -> Result<(), sacp::Error> {
        let (resp_sender, rx) = oneshot::channel();
        self.tx
            .send(SessionManagerRequest {
                session_id: SessionId::new(String::new()),
                data: SessionManagerRequestData::ReloadAgentConfigs { resp_sender },
            })
            .await
            .map_err(|_e| sacp::util::internal_error("Failed to send reload request"))?;
        rx.await
            .map_err(|_e| sacp::util::internal_error("Failed to receive reload response"))
    }
}

/// Stamps the sanitized driving ACP client name into the process environment so
/// that child `aws` CLI invocations carry an `acp-client/<name>` userAgent token
/// for CloudTrail attribution.
///
/// The `name` from `Initialize` is attacker-controlled, so it is sanitized first
/// (length-capped, and stripped of NUL — which would panic `set_var` — plus
/// spaces and `/`, which could otherwise forge extra userAgent tokens). A name
/// with nothing usable left (e.g. `"///"`) clears any previously-set value so a
/// stale name can't persist across a re-`Initialize`. Only
/// `ACP_CLIENT_NAME_ENV_VAR` is touched; telemetry and
/// `KIRO_CLI_CLIENT_APPLICATION` are unaffected.
fn stamp_acp_client_name(env: &crate::os::Env, name: &str) {
    // SAFETY (both arms): called early during session init — same risk profile
    // as the other `set_var` calls in this codebase (e.g.
    // `env_var::publish_session_id`).
    match agent::util::sanitize_acp_client_name(name) {
        Some(sanitized) => unsafe {
            env.set_var(agent::util::consts::env_var::ACP_CLIENT_NAME_ENV_VAR, &sanitized);
        },
        // Nothing usable in `name`: clear any prior value rather than leaving a
        // stale one behind. `Env` exposes no `remove_var`, so we set an empty
        // string; the user-agent formatter omits empty values via its
        // `!is_empty()` guard, making an empty value equivalent to unset.
        None => unsafe {
            env.set_var(agent::util::consts::env_var::ACP_CLIENT_NAME_ENV_VAR, "");
        },
    }
}

#[cfg(test)]
mod tests {
    use agent::util::truncate_safe;

    use super::SessionManager;

    // resolve_agent_name tests.
    //
    // Regression coverage for the ACP `--agent` drop: the CLI flag must apply
    // to every `session/new` (not be consumed after the first), while a loaded
    // session's persisted agent still wins over the startup flag.

    const DEFAULT: &str = agent::consts::DEFAULT_AGENT_NAME;

    #[test]
    fn resolve_agent_prefers_initial_over_everything() {
        let got = SessionManager::resolve_agent_name(
            Some("initial".to_string()),
            Some("persisted".to_string()),
            Some("cli".to_string()),
            Some("setting".to_string()),
        );
        assert_eq!(got, "initial");
    }

    #[test]
    fn resolve_agent_persisted_wins_over_cli_flag_on_load() {
        // session/load: persisted agent must override the startup --agent flag.
        let got = SessionManager::resolve_agent_name(
            None,
            Some("persisted".to_string()),
            Some("cli".to_string()),
            Some("setting".to_string()),
        );
        assert_eq!(got, "persisted");
    }

    #[test]
    fn resolve_agent_cli_flag_used_for_new_session() {
        // session/new: no persisted agent, so the --agent flag applies.
        let got = SessionManager::resolve_agent_name(None, None, Some("cli".to_string()), Some("setting".to_string()));
        assert_eq!(got, "cli");
    }

    #[test]
    fn resolve_agent_cli_flag_applies_repeatedly() {
        // The fix: the same cli value resolves on every call because the caller
        // clones (does not `take`) it. Simulate N sequential new-session calls.
        let cli = Some("cost-ai-agent".to_string());
        for _ in 0..5 {
            let got = SessionManager::resolve_agent_name(None, None, cli.clone(), Some("setting".to_string()));
            assert_eq!(got, "cost-ai-agent");
        }
    }

    #[test]
    fn resolve_agent_falls_back_to_setting() {
        let got = SessionManager::resolve_agent_name(None, None, None, Some("setting".to_string()));
        assert_eq!(got, "setting");
    }

    #[test]
    fn resolve_agent_falls_back_to_default() {
        let got = SessionManager::resolve_agent_name(None, None, None, None);
        assert_eq!(got, DEFAULT);
    }

    #[test]
    fn resolve_model_prefers_explicit_without_consuming_cli_override() {
        let mut cli_model = Some("cli-model".to_string());
        let resolved = SessionManager::resolve_model_id(Some("stage-model".to_string()), &mut cli_model);
        assert_eq!(resolved.as_deref(), Some("stage-model"));
        assert_eq!(cli_model.as_deref(), Some("cli-model"));
    }

    #[test]
    fn resolve_model_uses_cli_override_when_explicit_is_absent() {
        let mut cli_model = Some("cli-model".to_string());
        let resolved = SessionManager::resolve_model_id(None, &mut cli_model);
        assert_eq!(resolved.as_deref(), Some("cli-model"));
        assert!(cli_model.is_none());
    }

    #[test]
    fn orchestrated_session_config_preserves_model_override() {
        let config = SessionManager::orchestrated_session_config(
            "session-id".to_string(),
            std::path::PathBuf::from("/tmp/workspace"),
            "parent-id".to_string(),
            "reviewer".to_string(),
            Some("stage-model".to_string()),
            "review task".to_string(),
        );
        assert_eq!(config.model_id.as_deref(), Some("stage-model"));
        assert_eq!(config.initial_agent_name.as_deref(), Some("reviewer"));
        assert_eq!(config.parent_session_id.as_deref(), Some("parent-id"));
    }

    #[test]
    fn test_is_relevant_config_path() {
        use std::path::Path;
        let rel = super::is_relevant_config_path;
        // mcp.json anywhere is relevant.
        assert!(rel(Path::new("/home/u/.kiro/settings/mcp.json")));
        assert!(rel(Path::new("/proj/.amazonq/mcp.json")));
        // JSON agent configs under agents / cli-agents dirs.
        assert!(rel(Path::new("/home/u/.kiro/agents/my.json")));
        assert!(rel(Path::new("/proj/.amazonq/cli-agents/x.json")));
        // Directory events (no extension) under an agents dir still pass.
        assert!(rel(Path::new("/home/u/.kiro/agents")));
        // Non-JSON files under those dirs are ignored.
        assert!(!rel(Path::new("/home/u/.kiro/agents/notes.txt")));
        assert!(!rel(Path::new("/home/u/.kiro/agents/server.log")));
        // Unrelated paths are ignored.
        assert!(!rel(Path::new("/home/u/.kiro/steering/foo.md")));
        assert!(!rel(Path::new("/proj/src/main.rs")));
    }

    #[test]
    fn test_resolve_watch_targets_create_policy() {
        let tmp = tempfile::tempdir().unwrap();
        let cwd = tmp.path().join("workspace");
        let home = tmp.path().join("home");
        std::fs::create_dir_all(&cwd).unwrap();
        std::fs::create_dir_all(&home).unwrap();

        let ws_agents = cwd.join(".kiro").join("agents");
        let global_agents = home.join(".kiro").join("agents");

        // Bare cwd (no `.kiro`): the workspace agents dir is NOT created (no
        // `.kiro` litter) and is not watched; the kiro-owned global dir IS.
        let targets = super::resolve_watch_targets(Some(&ws_agents), Some(&global_agents), None, None);
        assert!(!cwd.join(".kiro").exists(), "must not create .kiro in a bare cwd");
        assert!(!targets.contains(&ws_agents));
        assert!(global_agents.exists(), "global agents dir should be created");
        assert!(targets.contains(&global_agents));

        // Once the user has opted into `.kiro`, its agents dir is materialized + watched.
        std::fs::create_dir_all(cwd.join(".kiro")).unwrap();
        let targets = super::resolve_watch_targets(Some(&ws_agents), Some(&global_agents), None, None);
        assert!(ws_agents.exists(), "workspace agents dir created when .kiro exists");
        assert!(targets.contains(&ws_agents));
    }

    /// Verifies that truncating multi-byte UTF-8 text does not panic.
    /// Reproduces the byte-slicing bug in handle_get_live_activity where
    /// `&text[..300]` panics when byte 300 is mid-character.
    #[test]
    fn test_live_activity_truncation_handles_multibyte() {
        // "x" + "あ"×200 = 1 + 600 = 601 bytes. Byte 300 falls mid-character.
        let text = format!("x{}", "あ".repeat(200));
        assert!(text.len() > 300);

        // This is what the buggy code does — it panics:
        let result = std::panic::catch_unwind(|| format!("{}...", &text[..300]));
        assert!(result.is_err(), "byte slicing should panic on multi-byte boundary");

        // This is the safe alternative:
        let safe = truncate_safe(&text, 300);
        assert!(safe.len() <= 300);
        assert!(!safe.is_empty());
    }

    // ── check_loop_trigger tests ────────────────────────────────────────

    fn make_session(
        loop_config: Option<crate::agent::acp::orchestration::types::LoopConfig>,
        result: Option<String>,
        loop_iteration: u32,
        changes_needed: bool,
        model: Option<&str>,
    ) -> crate::agent::acp::orchestration::types::OrchestratedSession {
        use std::time::SystemTime;

        use crate::agent::acp::orchestration::types::*;
        OrchestratedSession {
            session_id: sacp::schema::SessionId::new("test".to_string()),
            name: "reviewer".to_string(),
            task: "review code".to_string(),
            agent_name: "review-agent".to_string(),
            model: model.map(String::from),
            role: Some("reviewer".to_string()),
            parent_session: None,
            group: Some("test-group".to_string()),
            status: SessionStatus::Terminated,
            created_at: SystemTime::now(),
            last_activity: SystemTime::now(),
            human_attached: false,
            persistent: false,
            depends_on: vec![],
            result,
            loop_config,
            loop_iteration,
            changes_needed,
        }
    }

    fn loop_cfg() -> Option<crate::agent::acp::orchestration::types::LoopConfig> {
        Some(crate::agent::acp::orchestration::types::LoopConfig {
            target: "implementer".to_string(),
            max_iterations: 3,
            trigger: "NEEDS_CHANGES".to_string(),
        })
    }

    #[test]
    fn check_loop_trigger_changes_needed_signal() {
        let session = make_session(loop_cfg(), Some("All good".to_string()), 0, true, None);
        let data = super::SessionManager::check_loop_trigger(&session);
        assert!(
            data.is_some(),
            "changes_needed=true should trigger loop even without trigger text"
        );
    }

    #[test]
    fn check_loop_trigger_preserves_model_override() {
        let session = make_session(loop_cfg(), Some("All good".to_string()), 0, true, Some("review-model"));
        let data = super::SessionManager::check_loop_trigger(&session).unwrap();
        assert_eq!(data.model.as_deref(), Some("review-model"));
    }

    fn make_pending_stage(name: &str, model: Option<&str>) -> crate::agent::acp::orchestration::types::PendingStage {
        crate::agent::acp::orchestration::types::PendingStage {
            name: name.to_string(),
            role: "implementer".to_string(),
            task: "implement".to_string(),
            depends_on: vec![],
            agent_name: "implementer".to_string(),
            model: model.map(String::from),
            loop_config: None,
            loop_iteration: 0,
        }
    }

    #[test]
    fn loop_target_model_prefers_completed_session() {
        let session = make_session(None, None, 0, false, Some("session-model"));
        let pending = vec![make_pending_stage("reviewer", Some("pending-model"))];
        let model = super::SessionManager::loop_target_model(Some(&session), &pending, "reviewer");
        assert_eq!(model.as_deref(), Some("session-model"));
    }

    #[test]
    fn loop_target_model_falls_back_to_pending_stage() {
        let pending = vec![
            make_pending_stage("other", Some("other-model")),
            make_pending_stage("implementer", Some("impl-model")),
        ];
        let model = super::SessionManager::loop_target_model(None, &pending, "implementer");
        assert_eq!(model.as_deref(), Some("impl-model"));
    }

    #[test]
    fn loop_target_model_none_when_target_unknown() {
        let pending = vec![make_pending_stage("other", Some("other-model"))];
        let model = super::SessionManager::loop_target_model(None, &pending, "implementer");
        assert!(model.is_none());
    }

    #[test]
    fn check_loop_trigger_text_fallback() {
        let session = make_session(
            loop_cfg(),
            Some("Found issues. NEEDS_CHANGES".to_string()),
            0,
            false,
            None,
        );
        let data = super::SessionManager::check_loop_trigger(&session);
        assert!(data.is_some(), "trigger text should still work as fallback");
    }

    #[test]
    fn check_loop_trigger_no_signal_no_text() {
        let session = make_session(loop_cfg(), Some("All good, approved".to_string()), 0, false, None);
        let data = super::SessionManager::check_loop_trigger(&session);
        assert!(data.is_none(), "no signal and no trigger text should not trigger");
    }

    #[test]
    fn check_loop_trigger_max_iterations_reached() {
        let session = make_session(loop_cfg(), Some("NEEDS_CHANGES".to_string()), 3, true, None);
        let data = super::SessionManager::check_loop_trigger(&session);
        assert!(data.is_none(), "should not trigger when max iterations reached");
    }

    #[test]
    fn check_loop_trigger_no_loop_config() {
        let session = make_session(None, Some("NEEDS_CHANGES".to_string()), 0, true, None);
        let data = super::SessionManager::check_loop_trigger(&session);
        assert!(data.is_none(), "no loop_config means no trigger regardless of signals");
    }

    // ── siblings_to_cancel tests ────────────────────────────────────────

    fn orch_in_group(
        id: &str,
        name: &str,
        group: &str,
        status: crate::agent::acp::orchestration::types::SessionStatus,
    ) -> crate::agent::acp::orchestration::types::OrchestratedSession {
        use std::time::SystemTime;

        use crate::agent::acp::orchestration::types::*;
        OrchestratedSession {
            session_id: sacp::schema::SessionId::new(id.to_string()),
            name: name.to_string(),
            task: "t".to_string(),
            agent_name: "a".to_string(),
            model: None,
            role: None,
            parent_session: None,
            group: Some(group.to_string()),
            status,
            created_at: SystemTime::now(),
            last_activity: SystemTime::now(),
            human_attached: false,
            persistent: false,
            depends_on: vec![],
            result: None,
            loop_config: None,
            loop_iteration: 0,
            changes_needed: false,
        }
    }

    #[test]
    fn siblings_to_cancel_returns_running_in_group_siblings_only() {
        use std::collections::HashMap;

        use crate::agent::acp::orchestration::types::SessionStatus;
        let mut sessions = HashMap::new();
        // The failed stage (already terminated by the failure branch).
        sessions.insert(
            "a".to_string(),
            orch_in_group("a", "stage-a", "crew-x", SessionStatus::Terminated),
        );
        // A still-running sibling — must be cancelled.
        sessions.insert(
            "b".to_string(),
            orch_in_group("b", "stage-b", "crew-x", SessionStatus::Busy),
        );
        // A sibling that already finished — nothing to cancel.
        sessions.insert(
            "c".to_string(),
            orch_in_group("c", "stage-c", "crew-x", SessionStatus::Terminated),
        );
        // An unrelated session in another group — must be untouched.
        sessions.insert(
            "d".to_string(),
            orch_in_group("d", "stage-d", "other", SessionStatus::Busy),
        );

        let mut got: Vec<String> = super::SessionManager::siblings_to_cancel(&sessions, "crew-x", "stage-a")
            .iter()
            .map(|s| s.to_string())
            .collect();
        got.sort();
        assert_eq!(
            got,
            vec!["b".to_string()],
            "only the running, in-group, non-failed sibling should be cancelled"
        );
    }

    #[test]
    fn siblings_to_cancel_never_selects_the_failed_stage() {
        use std::collections::HashMap;

        use crate::agent::acp::orchestration::types::SessionStatus;
        let mut sessions = HashMap::new();
        // Failed stage whose status has not yet flipped to Terminated.
        sessions.insert(
            "a".to_string(),
            orch_in_group("a", "stage-a", "crew-x", SessionStatus::Busy),
        );
        sessions.insert(
            "b".to_string(),
            orch_in_group("b", "stage-b", "crew-x", SessionStatus::Busy),
        );

        let got: Vec<String> = super::SessionManager::siblings_to_cancel(&sessions, "crew-x", "stage-a")
            .iter()
            .map(|s| s.to_string())
            .collect();
        assert_eq!(
            got,
            vec!["b".to_string()],
            "the failed stage itself must never be selected for cancellation"
        );
    }

    // ── stamp_acp_client_name tests ─────────────────────────────────────
    // The Initialize path stamps the SANITIZED ACP client name into the env so
    // child `aws` CLI calls carry an `acp-client/<name>` userAgent token. The
    // raw name is left to telemetry (AcpClientInfo); only the env var is
    // sanitized here. A junk-only name clears any previously-set value.

    #[test]
    fn stamp_acp_client_name_sanitizes_multiword_name() {
        use agent::util::consts::env_var::ACP_CLIENT_NAME_ENV_VAR;

        use crate::os::Env;

        let env = Env::from_slice(&[]);
        super::stamp_acp_client_name(&env, "Visual Studio Code");
        assert_eq!(
            env.get(ACP_CLIENT_NAME_ENV_VAR).unwrap(),
            "VisualStudioCode",
            "spaces must be stripped so the name can't forge extra UA tokens"
        );
    }

    #[test]
    fn stamp_acp_client_name_strips_nul_without_panic() {
        use agent::util::consts::env_var::ACP_CLIENT_NAME_ENV_VAR;

        use crate::os::Env;

        // A NUL byte would panic the real `set_var`; it must be dropped, not stored.
        let env = Env::from_slice(&[]);
        super::stamp_acp_client_name(&env, "a\u{0}b");
        assert_eq!(env.get(ACP_CLIENT_NAME_ENV_VAR).unwrap(), "ab");
    }

    #[test]
    fn stamp_acp_client_name_junk_only_clears_var() {
        use agent::util::consts::env_var::ACP_CLIENT_NAME_ENV_VAR;

        use crate::os::Env;

        // Nothing survives the allowlist, so the var is cleared. `Env` has no
        // `remove_var`, so the cleared state is an empty string (or unset); the
        // UA formatter omits both via its `!is_empty()` guard.
        let env = Env::from_slice(&[]);
        super::stamp_acp_client_name(&env, "///");
        assert!(
            env.get(ACP_CLIENT_NAME_ENV_VAR).map(|v| v.is_empty()).unwrap_or(true),
            "junk-only name must clear (empty/unset) the env var"
        );
    }

    #[test]
    fn stamp_acp_client_name_junk_clears_prior_value() {
        use agent::util::consts::env_var::ACP_CLIENT_NAME_ENV_VAR;

        use crate::os::Env;

        // A previously-stamped value must not survive a re-`Initialize` whose
        // name sanitizes to nothing — otherwise stale attribution would leak
        // into later `aws` calls.
        let env = Env::from_slice(&[(ACP_CLIENT_NAME_ENV_VAR, "MeshClaw")]);
        super::stamp_acp_client_name(&env, "///");
        assert!(
            env.get(ACP_CLIENT_NAME_ENV_VAR).map(|v| v.is_empty()).unwrap_or(true),
            "a junk name must clear the previously-stamped value"
        );
    }

    #[test]
    fn stamp_acp_client_name_valid_overwrites_prior_value() {
        use agent::util::consts::env_var::ACP_CLIENT_NAME_ENV_VAR;

        use crate::os::Env;

        // A second `Initialize` with a valid name replaces the prior one
        // (last-writer-wins: a single driving client per process).
        let env = Env::from_slice(&[(ACP_CLIENT_NAME_ENV_VAR, "MeshClaw")]);
        super::stamp_acp_client_name(&env, "kiro-tui");
        assert_eq!(env.get(ACP_CLIENT_NAME_ENV_VAR).unwrap(), "kiro-tui");
    }
}
