//! # MCP (Model Context Protocol) Module
//!
//! This module provides a manager for launching and interacting with multiple MCP servers.
//! It implements a multi-layered architecture with asynchronous communication between components.
//!
//! ## Architecture Overview
//!
//! The module consists of the following key constructs organized in multiple layers:
//!
//! ### Management Layer
//!
//! - **[`McpManager`]**: The central manager that runs in its own async task. It maintains the
//!   lifecycle of multiple MCP server instances and routes requests to the appropriate servers.
//!
//! - **[`McpManagerHandle`]**: A cloneable handle for interacting with the `McpManager` from other
//!   parts of the application. It provides a safe, async API for launching servers, querying tool
//!   specifications, executing tools, and receiving server events.
//!
//! ### Actor Layer
//!
//! - **[`McpServerActor`]** (in [`actor`] module): Individual server actors that manage the
//!   lifecycle of a single MCP server process. Each actor handles initialization, tool execution,
//!   and communication with its associated server.
//!
//! - **[`McpServerActorHandle`]** (in [`actor`] module): A handle for interacting with a specific
//!   `McpServerActor`. Used internally by `McpManager` to communicate with servers.
//!
//! ### Service Layer
//!
//! - **`McpService`** (in `service` module): Implements the `rmcp::Service` trait to handle
//!   server-to-client requests and notifications. Created during server launch and consumed by the
//!   rmcp crate.
//!
//! - **`RunningMcpService`** (in `service` module): A handle to a running MCP server that wraps the
//!   rmcp service. Provides methods for calling tools, listing tools/prompts, and handles
//!   authentication/token refresh for remote servers.
//!
//! - **`rmcp::RunningService`** (from rmcp crate): The underlying service from the rmcp library
//!   that handles the actual MCP protocol communication over stdio (for local servers) or HTTP (for
//!   remote servers).
//!
//! ## Communication Patterns
//!
//! The module uses two primary communication patterns:
//!
//! ### 1. Request/Response Pattern
//!
//! ```text
//! McpManagerHandle      McpManager      McpServerActor    RunningMcpService    rmcp::RunningService
//!       |                    |                 |                  |                     |
//!       |--[LaunchServer]--->|                 |                  |                     |
//!       |                    |----[spawn]----->|                  |                     |
//!       |                    |                 |--[McpService]--->|                     |
//!       |                    |                 |                  |--[serve]----------->|
//!       |<--[response]-------| (initializing)  |                  |                     |
//!       |                    |                 |<--[initialized]--|                     |
//!       |                    |                 |                  |                     |
//!       |--[GetToolSpecs]--->|                 |                  |                     |
//!       |                    |--[get_tools]--->|                  |                     |
//!       |                    |                 | (returns cached) |                     |
//!       |                    |<--[tools]-------|                  |                     |
//!       |<--[tools]----------|                 |                  |                     |
//!       |                    |                 |                  |                     |
//!       |--[ExecuteTool]---->|                 |                  |                     |
//!       |                    |--[execute]----->|                  |                     |
//!       |                    |                 |--[call_tool]---->|                     |
//!       |                    |                 |                  |--[call_tool]------->|
//!       |<--[oneshot rx]-----|                 |                  |                     |
//!       |                    |                 |                  |<--[result]----------|
//!       |                    |                 |<--[result]-------|                     |
//!       |<--[result via rx]------------------------[async]--------|                     |
//! ```
//!
//! ### 2. Event Broadcasting Pattern
//!
//! ```text
//! McpServerActor              McpManager              McpManagerHandle
//!       |                          |                         |
//!       |--[Initialized event]---->|                         |
//!       |                          |--[forward event]------->|
//!       |                          | (moves server from      |
//!       |                          |  initializing_servers   |
//!       |                          |  to servers HashMap)    |
//!       |                          |                         |
//!       |--[OauthRequest event]--->|                         |
//!       |                          |--[forward event]------->|
//!       |                          |                         |
//!       |--[InitializeError]------>|                         |
//!       |                          |--[forward event]------->|
//!       |                          | (removes from           |
//!       |                          |  initializing_servers)  |
//! ```
//!
//! ## Server Lifecycle
//!
//! MCP servers go through the following states:
//!
//! 1. **Not Launched**: Server configuration exists but no actor has been spawned
//! 2. **Initializing**: `McpServerActor` has been spawned and is stored in
//!    `McpManager::initializing_servers`. The actor is establishing connection and fetching initial
//!    metadata (tools, prompts)
//! 3. **Initialized**: Server is ready and stored in `McpManager::servers`. Tools can now be
//!    executed
//! 4. **Error**: Initialization failed, server is removed from `initializing_servers`

pub mod actor;
pub mod oauth_util;
pub mod reconcile;
pub mod registry;
pub(crate) mod service;
pub mod types;

use std::collections::{
    HashMap,
    HashSet,
};
use std::path::PathBuf;
use std::time::Duration;

use actor::{
    McpServerActor,
    McpServerActorError,
    McpServerActorEvent,
    McpServerActorHandle,
};
use futures::future::join_all;
pub use registry::McpRegistry;
use rmcp::model::CallToolResult;
use serde::{
    Deserialize,
    Serialize,
};
use serde_json::Value;
use tokio::sync::broadcast::error::RecvError;
use tokio::sync::{
    broadcast,
    mpsc,
    oneshot,
};
use tracing::{
    debug,
    info,
    warn,
};
use types::Prompt;

use super::agent_loop::types::ToolSpec;
use super::consts::DEFAULT_MCP_CREDENTIAL_PATH;
use super::tools::mcp::McpToolAnnotations;
use super::util::path::expand_path;
use super::util::providers::RealProvider;
use super::util::request_channel::{
    RequestReceiver,
    new_request_channel,
};
use crate::agent::agent_config::definitions::McpServerConfig;
use crate::agent::util::request_channel::{
    RequestSender,
    respond,
};

/// Handle for communicating with an [`McpManager`] actor.
#[derive(Debug)]
pub struct McpManagerHandle {
    /// Sender for sending requests to the tool manager task
    request_tx: RequestSender<McpManagerRequest, McpManagerResponse, McpManagerError>,
    mcp_main_loop_to_handle_server_event_rx: broadcast::Receiver<McpServerActorEvent>,
}

impl Clone for McpManagerHandle {
    fn clone(&self) -> Self {
        Self {
            request_tx: self.request_tx.clone(),
            mcp_main_loop_to_handle_server_event_rx: self.mcp_main_loop_to_handle_server_event_rx.resubscribe(),
        }
    }
}

impl McpManagerHandle {
    fn new(
        request_tx: RequestSender<McpManagerRequest, McpManagerResponse, McpManagerError>,
        mcp_main_loop_to_handle_server_event_rx: broadcast::Receiver<McpServerActorEvent>,
    ) -> Self {
        Self {
            request_tx,
            mcp_main_loop_to_handle_server_event_rx,
        }
    }

    pub async fn launch_server(
        &mut self,
        name: String,
        config: McpServerConfig,
    ) -> Result<oneshot::Receiver<LaunchServerResult>, McpManagerError> {
        match self
            .request_tx
            .send_recv(McpManagerRequest::LaunchServer {
                server_name: name,
                config,
            })
            .await
            .unwrap_or(Err(McpManagerError::Channel))?
        {
            McpManagerResponse::LaunchServer(rx) => Ok(rx),
            other => Err(McpManagerError::Custom(format!(
                "received unexpected response: {other:?}"
            ))),
        }
    }

    /// Terminate and remove a single server by name. Idempotent: stopping a
    /// server that isn't running succeeds (the desired absence already holds).
    pub async fn stop_server(&mut self, name: String) -> Result<(), McpManagerError> {
        match self
            .request_tx
            .send_recv(McpManagerRequest::StopServer { server_name: name })
            .await
            .unwrap_or(Err(McpManagerError::Channel))?
        {
            McpManagerResponse::StopServerAcknowledged => Ok(()),
            other => Err(McpManagerError::Custom(format!(
                "received unexpected response: {other:?}"
            ))),
        }
    }

    pub async fn get_tool_specs(&self, server_name: String) -> Result<Vec<ToolSpec>, McpManagerError> {
        match self
            .request_tx
            .send_recv(McpManagerRequest::GetToolSpecs { server_name })
            .await
            .unwrap_or(Err(McpManagerError::Channel))?
        {
            McpManagerResponse::ToolSpecs(v) => Ok(v),
            other => Err(McpManagerError::Custom(format!(
                "received unexpected response: {other:?}"
            ))),
        }
    }

    /// Look up MCP annotations for a single tool. Returns `Ok(None)` if the
    /// server isn't initialized, the tool doesn't exist, or the gateway
    /// emitted no annotations for it. The agent uses this to populate
    /// `McpTool.annotations` after `Tool::parse`; permissioning policy MUST
    /// treat `Ok(None)` as "no hints", not "false".
    pub async fn get_tool_annotations(
        &self,
        server_name: String,
        tool_name: String,
    ) -> Result<Option<McpToolAnnotations>, McpManagerError> {
        match self
            .request_tx
            .send_recv(McpManagerRequest::GetToolAnnotations { server_name, tool_name })
            .await
            .unwrap_or(Err(McpManagerError::Channel))?
        {
            McpManagerResponse::ToolAnnotations(v) => Ok(v),
            other => Err(McpManagerError::Custom(format!(
                "received unexpected response: {other:?}"
            ))),
        }
    }

    pub async fn get_prompts(&self, server_name: String) -> Result<Vec<Prompt>, McpManagerError> {
        match self
            .request_tx
            .send_recv(McpManagerRequest::GetPrompts { server_name })
            .await
            .unwrap_or(Err(McpManagerError::Channel))?
        {
            McpManagerResponse::Prompts(v) => Ok(v),
            other => Err(McpManagerError::Custom(format!(
                "received unexpected response: {other:?}"
            ))),
        }
    }

    pub async fn get_prompt(
        &self,
        server_name: String,
        name: String,
        arguments: HashMap<String, String>,
    ) -> Result<Vec<serde_json::Value>, McpManagerError> {
        match self
            .request_tx
            .send_recv(McpManagerRequest::GetPrompt {
                server_name,
                name,
                arguments,
            })
            .await
            .unwrap_or(Err(McpManagerError::Channel))?
        {
            McpManagerResponse::Prompt(v) => Ok(v),
            other => Err(McpManagerError::Custom(format!(
                "received unexpected response: {other:?}"
            ))),
        }
    }

    pub async fn execute_tool(
        &self,
        server_name: String,
        tool_name: String,
        args: Option<serde_json::Map<String, Value>>,
    ) -> Result<oneshot::Receiver<ExecuteToolResult>, McpManagerError> {
        match self
            .request_tx
            .send_recv(McpManagerRequest::ExecuteTool {
                server_name,
                tool_name,
                args,
            })
            .await
            .unwrap_or(Err(McpManagerError::Channel))?
        {
            McpManagerResponse::ExecuteTool(rx) => Ok(rx),
            other => Err(McpManagerError::Custom(format!(
                "received unexpected response: {other:?}"
            ))),
        }
    }

    pub async fn recv(&mut self) -> Result<McpServerEvent, RecvError> {
        self.mcp_main_loop_to_handle_server_event_rx
            .recv()
            .await
            .map(|evt| evt.into())
    }

    pub fn terminate(&self) {
        _ = self.request_tx.try_blocking_send_recv(McpManagerRequest::Terminate);
    }

    /// Async version of [`terminate`](Self::terminate) that awaits MCP server shutdown.
    pub async fn shutdown(&self) {
        _ = self.request_tx.send_recv(McpManagerRequest::Terminate).await;
    }
}

/// Actor that manages the lifecycle of multiple MCP servers.
///
/// See the module-level documentation for architecture details.
#[derive(Debug)]
pub struct McpManager {
    request_tx: RequestSender<McpManagerRequest, McpManagerResponse, McpManagerError>,
    request_rx: RequestReceiver<McpManagerRequest, McpManagerResponse, McpManagerError>,
    server_event_tx: mpsc::Sender<McpServerActorEvent>,
    server_event_rx: mpsc::Receiver<McpServerActorEvent>,

    cred_path: PathBuf,

    initializing_servers: HashMap<String, (McpServerActorHandle, oneshot::Sender<LaunchServerResult>)>,
    servers: HashMap<String, McpServerActorHandle>,
    /// Names of servers that failed initialization.
    failed_servers: HashSet<String>,
    event_buf: Vec<McpServerActorEvent>,
}

impl McpManager {
    pub fn new(cred_path: PathBuf) -> Self {
        let (request_tx, request_rx) = new_request_channel();
        let (server_event_tx, server_event_rx) = mpsc::channel::<McpServerActorEvent>(100);

        Self {
            request_tx,
            request_rx,
            server_event_tx,
            server_event_rx,
            cred_path,
            initializing_servers: HashMap::new(),
            servers: HashMap::new(),
            failed_servers: HashSet::new(),
            event_buf: Vec::<McpServerActorEvent>::new(),
        }
    }

    pub fn spawn(self) -> McpManagerHandle {
        let request_tx = self.request_tx.clone();
        let (mcp_main_loop_to_handle_server_event_tx, mcp_main_loop_to_handle_server_event_rx) =
            broadcast::channel::<McpServerActorEvent>(100);

        tokio::spawn(async move {
            self.main_loop(mcp_main_loop_to_handle_server_event_tx).await;
        });

        McpManagerHandle::new(request_tx, mcp_main_loop_to_handle_server_event_rx)
    }

    async fn main_loop(mut self, mcp_main_loop_to_handle_server_event_tx: broadcast::Sender<McpServerActorEvent>) {
        loop {
            self.event_buf
                .drain(..)
                .for_each(|evt| _ = mcp_main_loop_to_handle_server_event_tx.send(evt));

            tokio::select! {
                req = self.request_rx.recv() => {
                    let Some(req) = req else {
                        warn!("Tool manager request channel has closed, exiting");
                        break;
                    };
                    let res = self.handle_mcp_manager_request(req.payload).await;

                    if let Ok(McpManagerResponse::TerminateAcknowledged) = res {
                        respond!(req, res);
                        break;
                    } else {
                        respond!(req, res);
                    }
                },
                res = self.server_event_rx.recv() => {
                    if let Some(evt) = res {
                        self.handle_mcp_actor_event(evt);
                    }
                }
            }
        }
    }

    async fn handle_mcp_manager_request(
        &mut self,
        req: McpManagerRequest,
    ) -> Result<McpManagerResponse, McpManagerError> {
        debug!(?req, "tool manager received new request");
        match req {
            McpManagerRequest::LaunchServer {
                server_name: name,
                config,
            } => {
                if self.initializing_servers.contains_key(&name) {
                    return Err(McpManagerError::ServerCurrentlyInitializing { name });
                } else if self.servers.contains_key(&name) {
                    return Err(McpManagerError::ServerAlreadyLaunched { name });
                }

                self.event_buf.push(McpServerActorEvent::Initializing {
                    server_name: name.clone(),
                });

                let event_tx = self.server_event_tx.clone();
                let handle = McpServerActor::spawn(name.clone(), config, self.cred_path.clone(), event_tx);
                let (tx, rx) = oneshot::channel();

                self.initializing_servers.insert(name, (handle, tx));
                Ok(McpManagerResponse::LaunchServer(rx))
            },
            McpManagerRequest::GetToolSpecs { server_name } => match self.servers.get(&server_name) {
                Some(handle) => Ok(McpManagerResponse::ToolSpecs(handle.get_tool_specs().await?)),
                None if self.failed_servers.contains(&server_name) => {
                    Err(McpManagerError::ServerFailed { name: server_name })
                },
                None if self.initializing_servers.contains_key(&server_name) => {
                    Err(McpManagerError::ServerCurrentlyInitializing { name: server_name })
                },
                None => Err(McpManagerError::ServerNotInitialized { name: server_name }),
            },
            McpManagerRequest::GetToolAnnotations { server_name, tool_name } => match self.servers.get(&server_name) {
                Some(handle) => Ok(McpManagerResponse::ToolAnnotations(
                    handle.get_tool_annotations(tool_name).await?,
                )),
                // Mirror `GetToolSpecs` semantics: failures and not-initialized states get
                // surfaced; the agent's caller decides whether to fall through to "no hints".
                None if self.failed_servers.contains(&server_name) => {
                    Err(McpManagerError::ServerFailed { name: server_name })
                },
                None if self.initializing_servers.contains_key(&server_name) => {
                    Err(McpManagerError::ServerCurrentlyInitializing { name: server_name })
                },
                None => Err(McpManagerError::ServerNotInitialized { name: server_name }),
            },
            McpManagerRequest::GetPrompts { server_name } => match self.servers.get(&server_name) {
                Some(handle) => Ok(McpManagerResponse::Prompts(handle.get_prompts().await?)),
                None => Err(McpManagerError::ServerNotInitialized { name: server_name }),
            },
            McpManagerRequest::GetPrompt {
                server_name,
                name,
                arguments,
            } => match self.servers.get(&server_name) {
                Some(handle) => Ok(McpManagerResponse::Prompt(handle.get_prompt(name, arguments).await?)),
                None => Err(McpManagerError::ServerNotInitialized { name: server_name }),
            },
            McpManagerRequest::ExecuteTool {
                server_name,
                tool_name,
                args,
            } => match self.servers.get(&server_name) {
                Some(handle) => Ok(McpManagerResponse::ExecuteTool(
                    handle.execute_tool(tool_name, args).await?,
                )),
                None => Err(McpManagerError::ServerNotInitialized { name: server_name }),
            },
            McpManagerRequest::Terminate => {
                let futs: Vec<_> = self
                    .servers
                    .iter()
                    .map(|(name, s)| {
                        let name = name.clone();
                        async move {
                            if tokio::time::timeout(Duration::from_secs(4), s.shutdown())
                                .await
                                .is_err()
                            {
                                warn!(server_name = %name, "MCP server did not shut down within timeout");
                            }
                        }
                    })
                    .collect();
                join_all(futs).await;
                Ok(McpManagerResponse::TerminateAcknowledged)
            },
            McpManagerRequest::StopServer { server_name } => {
                // Targeted termination. A server may be fully running or still
                // initializing; handle both. Removing the initializing entry
                // drops its result sender, unblocking any pending launch receiver.
                if let Some(handle) = self.servers.remove(&server_name) {
                    if tokio::time::timeout(Duration::from_secs(4), handle.shutdown())
                        .await
                        .is_err()
                    {
                        warn!(server_name = %server_name, "MCP server did not shut down within timeout");
                    }
                } else if let Some((handle, _result_tx)) = self.initializing_servers.remove(&server_name) {
                    handle.terminate();
                }
                // Clear any failed marker so a subsequent relaunch isn't shadowed.
                self.failed_servers.remove(&server_name);
                Ok(McpManagerResponse::StopServerAcknowledged)
            },
        }
    }

    fn handle_mcp_actor_event(&mut self, evt: McpServerActorEvent) {
        // TODO: keep a record of all the different server events received in this layer?
        match &evt {
            McpServerActorEvent::Initializing { server_name: _ } => { /* noop */ },
            McpServerActorEvent::Initialized {
                server_name,
                serve_duration: _,
                list_tools_duration: _,
                list_prompts_duration: _,
            } => {
                let Some((handle, result_tx)) = self.initializing_servers.remove(server_name) else {
                    warn!(?server_name, ?evt, "event was not from an initializing MCP server");
                    return;
                };

                if let Err(e) = result_tx.send(Ok(())) {
                    warn!(?server_name, ?e, "failed to send server initialized message");
                }

                if self.servers.insert(server_name.clone(), handle).is_some() {
                    warn!(?server_name, "duplicated server. old server dropped");
                }
            },
            McpServerActorEvent::InitializeError { server_name, error } => {
                if let Some((_, result_tx)) = self.initializing_servers.remove(server_name)
                    && let Err(e) = result_tx.send(Err(McpManagerError::Custom(error.clone())))
                {
                    warn!(?server_name, ?e, "failed to send server initialized message");
                }
                self.failed_servers.insert(server_name.clone());
            },
            McpServerActorEvent::OauthRequest { server_name, oauth_url } => {
                info!(?server_name, ?oauth_url, "received oauth request");
            },
            McpServerActorEvent::ToolListChanged { server_name } => {
                info!(?server_name, "MCP server tool list changed");
            },
        }
        self.event_buf.push(evt);
    }
}

impl Default for McpManager {
    fn default() -> Self {
        let expanded_path =
            expand_path(DEFAULT_MCP_CREDENTIAL_PATH, &RealProvider).expect("failed to expand default credential path");
        let default_path = PathBuf::from(expanded_path.as_ref());

        Self::new(default_path)
    }
}

#[derive(Debug, Clone)]
pub enum McpManagerRequest {
    LaunchServer {
        /// Identifier for the server
        server_name: String,
        /// Config to use
        config: McpServerConfig,
    },
    GetToolSpecs {
        server_name: String,
    },
    GetToolAnnotations {
        server_name: String,
        tool_name: String,
    },
    GetPrompts {
        server_name: String,
    },
    GetPrompt {
        server_name: String,
        name: String,
        arguments: HashMap<String, String>,
    },
    ExecuteTool {
        server_name: String,
        tool_name: String,
        args: Option<serde_json::Map<String, Value>>,
    },
    /// Terminate and remove a single server by name. Idempotent: an unknown
    /// name is a no-op. Unlike [`Terminate`](Self::Terminate) — which tears
    /// down every server for shutdown — this targets one server so a changed
    /// desired set can be reconciled without churning unaffected servers.
    StopServer {
        server_name: String,
    },
    Terminate,
}

#[derive(Debug)]
pub enum McpManagerResponse {
    LaunchServer(oneshot::Receiver<LaunchServerResult>),
    ToolSpecs(Vec<ToolSpec>),
    ToolAnnotations(Option<McpToolAnnotations>),
    Prompts(Vec<Prompt>),
    Prompt(Vec<serde_json::Value>),
    ExecuteTool(oneshot::Receiver<ExecuteToolResult>),
    TerminateAcknowledged,
    StopServerAcknowledged,
}

pub type ExecuteToolResult = Result<CallToolResult, McpServerActorError>;

type LaunchServerResult = Result<(), McpManagerError>;

#[derive(Debug, Clone, Serialize, Deserialize, thiserror::Error)]
pub enum McpManagerError {
    #[error("Server with the name {} is not initialized", .name)]
    ServerNotInitialized { name: String },
    #[error("Server with the name {} is currently initializing", .name)]
    ServerCurrentlyInitializing { name: String },
    #[error("Server with the name {} failed to initialize", .name)]
    ServerFailed { name: String },
    #[error("Server with the name {} has already launched", .name)]
    ServerAlreadyLaunched { name: String },
    #[error(transparent)]
    McpActor(#[from] McpServerActorError),
    #[error("The channel has closed")]
    Channel,
    #[error("{}", .0)]
    Custom(String),
}

/// MCP events relevant to agent operations.
/// Provides abstraction over [McpServerActorEvent] to avoid leaking implementation details.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum McpServerEvent {
    /// The MCP server is currently initializing
    Initializing { server_name: String },
    /// The MCP server has launched successfully
    Initialized {
        server_name: String,
        /// Time taken to launch the server
        serve_duration: Duration,
        /// Time taken to list all tools.
        ///
        /// None if the server does not support tools, or there was an error fetching tools.
        list_tools_duration: Option<Duration>,
        /// Time taken to list all prompts
        ///
        /// None if the server does not support prompts, or there was an error fetching prompts.
        list_prompts_duration: Option<Duration>,
    },
    /// The MCP server failed to initialize successfully
    InitializeError { server_name: String, error: String },
    /// An OAuth authentication request from the MCP server
    OauthRequest { server_name: String, oauth_url: String },
    /// The MCP server's tool list has changed
    ToolListChanged { server_name: String },
}

impl From<McpServerActorEvent> for McpServerEvent {
    fn from(value: McpServerActorEvent) -> Self {
        match value {
            McpServerActorEvent::Initializing { server_name } => Self::Initializing { server_name },
            McpServerActorEvent::Initialized {
                server_name,
                serve_duration,
                list_tools_duration,
                list_prompts_duration,
            } => Self::Initialized {
                server_name,
                serve_duration,
                list_tools_duration,
                list_prompts_duration,
            },
            McpServerActorEvent::InitializeError { server_name, error } => Self::InitializeError { server_name, error },
            McpServerActorEvent::OauthRequest { server_name, oauth_url } => {
                Self::OauthRequest { server_name, oauth_url }
            },
            McpServerActorEvent::ToolListChanged { server_name } => Self::ToolListChanged { server_name },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::agent_config::definitions::LocalMcpServerConfig;

    #[test]
    fn test_mcp_manager_error_display() {
        let e = McpManagerError::ServerNotInitialized {
            name: "myserver".to_string(),
        };
        assert!(e.to_string().contains("myserver"));
        assert!(e.to_string().contains("not initialized"));

        let e2 = McpManagerError::ServerCurrentlyInitializing { name: "x".to_string() };
        assert!(e2.to_string().contains("currently initializing"));

        let e3 = McpManagerError::ServerFailed { name: "x".to_string() };
        assert!(e3.to_string().contains("failed to initialize"));

        let e4 = McpManagerError::ServerAlreadyLaunched { name: "x".to_string() };
        assert!(e4.to_string().contains("already launched"));

        let e5 = McpManagerError::Channel;
        assert_eq!(e5.to_string(), "The channel has closed");

        let e6 = McpManagerError::Custom("oops".to_string());
        assert_eq!(e6.to_string(), "oops");
    }

    #[test]
    fn test_mcp_manager_error_serde() {
        let e = McpManagerError::ServerNotInitialized { name: "x".to_string() };
        let json = serde_json::to_string(&e).unwrap();
        let parsed: McpManagerError = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.to_string(), e.to_string());
    }

    #[test]
    fn test_mcp_manager_error_from_actor_error() {
        let actor_err = McpServerActorError::Channel;
        let mgr_err: McpManagerError = actor_err.into();
        assert!(matches!(mgr_err, McpManagerError::McpActor(_)));
    }

    #[test]
    fn test_mcp_server_event_serde_initializing() {
        let e = McpServerEvent::Initializing {
            server_name: "test".to_string(),
        };
        let json = serde_json::to_string(&e).unwrap();
        let parsed: McpServerEvent = serde_json::from_str(&json).unwrap();
        match parsed {
            McpServerEvent::Initializing { server_name } => assert_eq!(server_name, "test"),
            _ => panic!("expected Initializing"),
        }
    }

    #[test]
    fn test_mcp_server_event_serde_initialized() {
        let e = McpServerEvent::Initialized {
            server_name: "test".to_string(),
            serve_duration: Duration::from_secs(1),
            list_tools_duration: Some(Duration::from_millis(100)),
            list_prompts_duration: None,
        };
        let json = serde_json::to_string(&e).unwrap();
        let parsed: McpServerEvent = serde_json::from_str(&json).unwrap();
        match parsed {
            McpServerEvent::Initialized {
                server_name,
                list_tools_duration,
                list_prompts_duration,
                ..
            } => {
                assert_eq!(server_name, "test");
                assert_eq!(list_tools_duration, Some(Duration::from_millis(100)));
                assert!(list_prompts_duration.is_none());
            },
            _ => panic!("expected Initialized"),
        }
    }

    #[test]
    fn test_mcp_server_event_serde_initialize_error() {
        let e = McpServerEvent::InitializeError {
            server_name: "x".to_string(),
            error: "boom".to_string(),
        };
        let json = serde_json::to_string(&e).unwrap();
        let parsed: McpServerEvent = serde_json::from_str(&json).unwrap();
        match parsed {
            McpServerEvent::InitializeError { server_name, error } => {
                assert_eq!(server_name, "x");
                assert_eq!(error, "boom");
            },
            _ => panic!("expected InitializeError"),
        }
    }

    #[test]
    fn test_mcp_server_event_serde_oauth_request() {
        let e = McpServerEvent::OauthRequest {
            server_name: "x".to_string(),
            oauth_url: "https://auth.example.com".to_string(),
        };
        let json = serde_json::to_string(&e).unwrap();
        let parsed: McpServerEvent = serde_json::from_str(&json).unwrap();
        match parsed {
            McpServerEvent::OauthRequest { server_name, oauth_url } => {
                assert_eq!(server_name, "x");
                assert_eq!(oauth_url, "https://auth.example.com");
            },
            _ => panic!("expected OauthRequest"),
        }
    }

    #[test]
    fn test_mcp_server_event_serde_tool_list_changed() {
        let e = McpServerEvent::ToolListChanged {
            server_name: "x".to_string(),
        };
        let json = serde_json::to_string(&e).unwrap();
        let parsed: McpServerEvent = serde_json::from_str(&json).unwrap();
        match parsed {
            McpServerEvent::ToolListChanged { server_name } => assert_eq!(server_name, "x"),
            _ => panic!("expected ToolListChanged"),
        }
    }

    #[test]
    fn test_mcp_server_event_clone() {
        let e = McpServerEvent::Initializing {
            server_name: "x".to_string(),
        };
        let cloned = e.clone();
        match cloned {
            McpServerEvent::Initializing { server_name } => assert_eq!(server_name, "x"),
            _ => panic!("expected Initializing"),
        }
    }

    #[test]
    fn test_mcp_server_event_from_actor_event_all_variants() {
        // Initializing
        let evt = McpServerActorEvent::Initializing {
            server_name: "s1".to_string(),
        };
        let converted: McpServerEvent = evt.into();
        assert!(matches!(converted, McpServerEvent::Initializing { .. }));

        // Initialized
        let evt = McpServerActorEvent::Initialized {
            server_name: "s2".to_string(),
            serve_duration: Duration::from_millis(100),
            list_tools_duration: Some(Duration::from_millis(50)),
            list_prompts_duration: None,
        };
        let converted: McpServerEvent = evt.into();
        match converted {
            McpServerEvent::Initialized {
                server_name,
                serve_duration,
                list_tools_duration,
                list_prompts_duration,
            } => {
                assert_eq!(server_name, "s2");
                assert_eq!(serve_duration, Duration::from_millis(100));
                assert_eq!(list_tools_duration, Some(Duration::from_millis(50)));
                assert!(list_prompts_duration.is_none());
            },
            _ => panic!("expected Initialized"),
        }

        // InitializeError
        let evt = McpServerActorEvent::InitializeError {
            server_name: "s3".to_string(),
            error: "fail".to_string(),
        };
        let converted: McpServerEvent = evt.into();
        match converted {
            McpServerEvent::InitializeError { server_name, error } => {
                assert_eq!(server_name, "s3");
                assert_eq!(error, "fail");
            },
            _ => panic!("expected InitializeError"),
        }

        // OauthRequest
        let evt = McpServerActorEvent::OauthRequest {
            server_name: "s4".to_string(),
            oauth_url: "https://oauth.test".to_string(),
        };
        let converted: McpServerEvent = evt.into();
        match converted {
            McpServerEvent::OauthRequest { server_name, oauth_url } => {
                assert_eq!(server_name, "s4");
                assert_eq!(oauth_url, "https://oauth.test");
            },
            _ => panic!("expected OauthRequest"),
        }

        // ToolListChanged
        let evt = McpServerActorEvent::ToolListChanged {
            server_name: "s5".to_string(),
        };
        let converted: McpServerEvent = evt.into();
        assert!(matches!(converted, McpServerEvent::ToolListChanged { .. }));
    }

    #[test]
    fn test_mcp_manager_new() {
        let mgr = McpManager::new(PathBuf::from("/tmp/creds"));
        assert!(mgr.servers.is_empty());
        assert!(mgr.initializing_servers.is_empty());
        assert!(mgr.failed_servers.is_empty());
        assert!(mgr.event_buf.is_empty());
    }

    #[test]
    #[ignore = "spawns real subprocess; can hang in coverage runs"]
    fn test_mcp_manager_handle_mcp_actor_event_initialized() {
        let mut mgr = McpManager::new(PathBuf::from("/tmp/creds"));
        let (event_tx, _event_rx) = mpsc::channel(10);

        // Simulate a server in initializing state
        let handle = McpServerActor::spawn(
            "test-server".to_string(),
            McpServerConfig::Local(LocalMcpServerConfig {
                command: "false".to_string(),
                args: vec![],
                env: None,
                timeout_ms: 1000,
                disabled: false,
                disabled_tools: vec![],
            }),
            PathBuf::from("/tmp"),
            event_tx,
        );
        let (tx, _rx) = oneshot::channel();
        mgr.initializing_servers.insert("test-server".to_string(), (handle, tx));

        // Handle Initialized event
        mgr.handle_mcp_actor_event(McpServerActorEvent::Initialized {
            server_name: "test-server".to_string(),
            serve_duration: Duration::from_millis(100),
            list_tools_duration: None,
            list_prompts_duration: None,
        });

        assert!(mgr.servers.contains_key("test-server"));
        assert!(!mgr.initializing_servers.contains_key("test-server"));
        assert_eq!(mgr.event_buf.len(), 1);
    }

    #[test]
    #[ignore = "spawns real subprocess; can hang in coverage runs"]
    fn test_mcp_manager_handle_mcp_actor_event_initialize_error() {
        let mut mgr = McpManager::new(PathBuf::from("/tmp/creds"));
        let (event_tx, _event_rx) = mpsc::channel(10);

        let handle = McpServerActor::spawn(
            "fail-server".to_string(),
            McpServerConfig::Local(LocalMcpServerConfig {
                command: "false".to_string(),
                args: vec![],
                env: None,
                timeout_ms: 1000,
                disabled: false,
                disabled_tools: vec![],
            }),
            PathBuf::from("/tmp"),
            event_tx,
        );
        let (tx, _rx) = oneshot::channel();
        mgr.initializing_servers.insert("fail-server".to_string(), (handle, tx));

        mgr.handle_mcp_actor_event(McpServerActorEvent::InitializeError {
            server_name: "fail-server".to_string(),
            error: "connection refused".to_string(),
        });

        assert!(!mgr.initializing_servers.contains_key("fail-server"));
        assert!(mgr.failed_servers.contains("fail-server"));
        assert_eq!(mgr.event_buf.len(), 1);
    }

    #[test]
    fn test_mcp_manager_handle_mcp_actor_event_oauth_request() {
        let mut mgr = McpManager::new(PathBuf::from("/tmp/creds"));
        mgr.handle_mcp_actor_event(McpServerActorEvent::OauthRequest {
            server_name: "oauth-server".to_string(),
            oauth_url: "https://auth.example.com/authorize".to_string(),
        });
        assert_eq!(mgr.event_buf.len(), 1);
    }

    #[test]
    fn test_mcp_manager_handle_mcp_actor_event_tool_list_changed() {
        let mut mgr = McpManager::new(PathBuf::from("/tmp/creds"));
        mgr.handle_mcp_actor_event(McpServerActorEvent::ToolListChanged {
            server_name: "some-server".to_string(),
        });
        assert_eq!(mgr.event_buf.len(), 1);
    }

    #[test]
    fn test_mcp_manager_handle_mcp_actor_event_initializing_noop() {
        let mut mgr = McpManager::new(PathBuf::from("/tmp/creds"));
        mgr.handle_mcp_actor_event(McpServerActorEvent::Initializing {
            server_name: "x".to_string(),
        });
        // Event is still buffered
        assert_eq!(mgr.event_buf.len(), 1);
    }

    #[test]
    fn test_mcp_manager_handle_mcp_actor_event_initialized_unknown_server() {
        let mut mgr = McpManager::new(PathBuf::from("/tmp/creds"));
        // No server in initializing_servers - should just warn and return
        mgr.handle_mcp_actor_event(McpServerActorEvent::Initialized {
            server_name: "unknown".to_string(),
            serve_duration: Duration::from_millis(100),
            list_tools_duration: None,
            list_prompts_duration: None,
        });
        // Event is NOT buffered because we returned early
        assert!(!mgr.servers.contains_key("unknown"));
    }

    #[tokio::test]
    async fn test_mcp_manager_handle_request_get_tools_not_initialized() {
        let mut mgr = McpManager::new(PathBuf::from("/tmp/creds"));
        let result = mgr
            .handle_mcp_manager_request(McpManagerRequest::GetToolSpecs {
                server_name: "nonexistent".to_string(),
            })
            .await;
        assert!(matches!(result, Err(McpManagerError::ServerNotInitialized { .. })));
    }

    #[tokio::test]
    async fn test_mcp_manager_handle_request_get_tools_failed_server() {
        let mut mgr = McpManager::new(PathBuf::from("/tmp/creds"));
        mgr.failed_servers.insert("bad-server".to_string());
        let result = mgr
            .handle_mcp_manager_request(McpManagerRequest::GetToolSpecs {
                server_name: "bad-server".to_string(),
            })
            .await;
        assert!(matches!(result, Err(McpManagerError::ServerFailed { .. })));
    }

    #[tokio::test]
    #[ignore = "spawns real subprocess; can hang in coverage runs"]
    async fn test_mcp_manager_handle_request_get_tools_initializing_server() {
        let mut mgr = McpManager::new(PathBuf::from("/tmp/creds"));
        let (event_tx, _event_rx) = mpsc::channel(10);
        let handle = McpServerActor::spawn(
            "init-server".to_string(),
            McpServerConfig::Local(LocalMcpServerConfig {
                command: "false".to_string(),
                args: vec![],
                env: None,
                timeout_ms: 1000,
                disabled: false,
                disabled_tools: vec![],
            }),
            PathBuf::from("/tmp"),
            event_tx,
        );
        let (tx, _rx) = oneshot::channel();
        mgr.initializing_servers.insert("init-server".to_string(), (handle, tx));

        let result = mgr
            .handle_mcp_manager_request(McpManagerRequest::GetToolSpecs {
                server_name: "init-server".to_string(),
            })
            .await;
        assert!(matches!(
            result,
            Err(McpManagerError::ServerCurrentlyInitializing { .. })
        ));
    }

    #[tokio::test]
    async fn test_mcp_manager_handle_request_get_prompts_not_initialized() {
        let mut mgr = McpManager::new(PathBuf::from("/tmp/creds"));
        let result = mgr
            .handle_mcp_manager_request(McpManagerRequest::GetPrompts {
                server_name: "nonexistent".to_string(),
            })
            .await;
        assert!(matches!(result, Err(McpManagerError::ServerNotInitialized { .. })));
    }

    #[tokio::test]
    async fn test_mcp_manager_handle_request_get_prompt_not_initialized() {
        let mut mgr = McpManager::new(PathBuf::from("/tmp/creds"));
        let result = mgr
            .handle_mcp_manager_request(McpManagerRequest::GetPrompt {
                server_name: "nonexistent".to_string(),
                name: "test".to_string(),
                arguments: HashMap::new(),
            })
            .await;
        assert!(matches!(result, Err(McpManagerError::ServerNotInitialized { .. })));
    }

    #[tokio::test]
    async fn test_mcp_manager_handle_request_execute_tool_not_initialized() {
        let mut mgr = McpManager::new(PathBuf::from("/tmp/creds"));
        let result = mgr
            .handle_mcp_manager_request(McpManagerRequest::ExecuteTool {
                server_name: "nonexistent".to_string(),
                tool_name: "test_tool".to_string(),
                args: None,
            })
            .await;
        assert!(matches!(result, Err(McpManagerError::ServerNotInitialized { .. })));
    }

    #[tokio::test]
    async fn test_mcp_manager_handle_request_terminate() {
        let mut mgr = McpManager::new(PathBuf::from("/tmp/creds"));
        let result = mgr.handle_mcp_manager_request(McpManagerRequest::Terminate).await;
        assert!(matches!(result, Ok(McpManagerResponse::TerminateAcknowledged)));
    }

    #[tokio::test]
    async fn test_mcp_manager_stop_server_unknown_is_noop() {
        // Stopping a server that was never launched succeeds — the desired
        // absence already holds. Keeps the reconcile path idempotent.
        let mut mgr = McpManager::new(PathBuf::from("/tmp/creds"));
        let result = mgr
            .handle_mcp_manager_request(McpManagerRequest::StopServer {
                server_name: "ghost".to_string(),
            })
            .await;
        assert!(matches!(result, Ok(McpManagerResponse::StopServerAcknowledged)));
    }

    #[tokio::test]
    async fn test_mcp_manager_stop_server_removes_only_target() {
        // A running server is dropped from `servers`; unaffected servers stay.
        let mut mgr = McpManager::new(PathBuf::from("/tmp/creds"));
        mgr.servers
            .insert("gone".to_string(), McpServerActorHandle::new_dummy("gone"));
        mgr.servers
            .insert("kept".to_string(), McpServerActorHandle::new_dummy("kept"));

        let result = mgr
            .handle_mcp_manager_request(McpManagerRequest::StopServer {
                server_name: "gone".to_string(),
            })
            .await;
        assert!(matches!(result, Ok(McpManagerResponse::StopServerAcknowledged)));
        assert!(!mgr.servers.contains_key("gone"));
        assert!(mgr.servers.contains_key("kept"));
    }

    #[tokio::test]
    async fn test_mcp_manager_stop_server_removes_initializing_and_clears_failed() {
        // An initializing server is removed, and any stale failed marker is
        // cleared so a later relaunch of the same name isn't shadowed.
        let mut mgr = McpManager::new(PathBuf::from("/tmp/creds"));
        let (tx, _rx) = oneshot::channel();
        mgr.initializing_servers
            .insert("pending".to_string(), (McpServerActorHandle::new_dummy("pending"), tx));
        mgr.failed_servers.insert("pending".to_string());

        let result = mgr
            .handle_mcp_manager_request(McpManagerRequest::StopServer {
                server_name: "pending".to_string(),
            })
            .await;
        assert!(matches!(result, Ok(McpManagerResponse::StopServerAcknowledged)));
        assert!(!mgr.initializing_servers.contains_key("pending"));
        assert!(!mgr.failed_servers.contains("pending"));
    }

    #[tokio::test]
    #[ignore = "spawns real subprocess; can hang in coverage runs"]
    async fn test_mcp_manager_handle_request_launch_duplicate_initializing() {
        let mut mgr = McpManager::new(PathBuf::from("/tmp/creds"));
        let (event_tx, _event_rx) = mpsc::channel(10);
        let config = McpServerConfig::Local(LocalMcpServerConfig {
            command: "false".to_string(),
            args: vec![],
            env: None,
            timeout_ms: 1000,
            disabled: false,
            disabled_tools: vec![],
        });
        let handle = McpServerActor::spawn(
            "dup-server".to_string(),
            config.clone(),
            PathBuf::from("/tmp"),
            event_tx,
        );
        let (tx, _rx) = oneshot::channel();
        mgr.initializing_servers.insert("dup-server".to_string(), (handle, tx));

        let result = mgr
            .handle_mcp_manager_request(McpManagerRequest::LaunchServer {
                server_name: "dup-server".to_string(),
                config,
            })
            .await;
        assert!(matches!(
            result,
            Err(McpManagerError::ServerCurrentlyInitializing { .. })
        ));
    }

    #[tokio::test]
    async fn test_mcp_manager_spawn_and_terminate() {
        let mgr = McpManager::new(PathBuf::from("/tmp/creds"));
        let handle = mgr.spawn();
        handle.shutdown().await;
    }

    #[tokio::test]
    async fn test_mcp_manager_handle_clone() {
        let mgr = McpManager::new(PathBuf::from("/tmp/creds"));
        let handle = mgr.spawn();
        let _handle2 = handle.clone();
        handle.shutdown().await;
    }

    #[tokio::test]
    async fn test_mcp_manager_handle_get_tool_specs_no_server() {
        let mgr = McpManager::new(PathBuf::from("/tmp/creds"));
        let handle = mgr.spawn();
        let result = handle.get_tool_specs("nonexistent".to_string()).await;
        assert!(result.is_err());
        handle.shutdown().await;
    }

    #[tokio::test]
    async fn test_mcp_manager_handle_get_prompts_no_server() {
        let mgr = McpManager::new(PathBuf::from("/tmp/creds"));
        let handle = mgr.spawn();
        let result = handle.get_prompts("nonexistent".to_string()).await;
        assert!(result.is_err());
        handle.shutdown().await;
    }

    #[tokio::test]
    async fn test_mcp_manager_handle_get_prompt_no_server() {
        let mgr = McpManager::new(PathBuf::from("/tmp/creds"));
        let handle = mgr.spawn();
        let result = handle
            .get_prompt("nonexistent".to_string(), "p".to_string(), HashMap::new())
            .await;
        assert!(result.is_err());
        handle.shutdown().await;
    }

    #[tokio::test]
    async fn test_mcp_manager_handle_execute_tool_no_server() {
        let mgr = McpManager::new(PathBuf::from("/tmp/creds"));
        let handle = mgr.spawn();
        let result = handle
            .execute_tool("nonexistent".to_string(), "tool".to_string(), None)
            .await;
        assert!(result.is_err());
        handle.shutdown().await;
    }

    #[tokio::test]
    async fn test_mcp_manager_handle_launch_server() {
        let mgr = McpManager::new(PathBuf::from("/tmp/creds"));
        let mut handle = mgr.spawn();
        let config = McpServerConfig::Local(LocalMcpServerConfig {
            command: "echo".to_string(),
            args: vec![],
            env: None,
            timeout_ms: 1000,
            disabled: false,
            disabled_tools: vec![],
        });
        // Launch returns a oneshot receiver for the result
        let result = handle.launch_server("echo-server".to_string(), config).await;
        assert!(result.is_ok());
        handle.shutdown().await;
    }

    #[test]
    fn test_mcp_manager_request_debug() {
        let req = McpManagerRequest::Terminate;
        let debug_str = format!("{:?}", req);
        assert!(debug_str.contains("Terminate"));
    }

    #[test]
    fn test_mcp_manager_request_clone() {
        let req = McpManagerRequest::GetToolSpecs {
            server_name: "x".to_string(),
        };
        let cloned = req.clone();
        match cloned {
            McpManagerRequest::GetToolSpecs { server_name } => assert_eq!(server_name, "x"),
            _ => panic!("expected GetToolSpecs"),
        }
    }

    #[test]
    fn test_mcp_manager_error_clone() {
        let e = McpManagerError::ServerNotInitialized { name: "x".to_string() };
        let cloned = e.clone();
        assert_eq!(e.to_string(), cloned.to_string());
    }

    // --- Tests using dummy handles (no subprocess spawning) ---

    #[test]
    fn test_handle_actor_event_initialized_with_dummy_handle() {
        let mut mgr = McpManager::new(PathBuf::from("/tmp/creds"));
        let handle = McpServerActorHandle::new_dummy("srv");
        let (tx, mut rx) = oneshot::channel();
        mgr.initializing_servers.insert("srv".to_string(), (handle, tx));

        mgr.handle_mcp_actor_event(McpServerActorEvent::Initialized {
            server_name: "srv".to_string(),
            serve_duration: Duration::from_millis(50),
            list_tools_duration: Some(Duration::from_millis(10)),
            list_prompts_duration: Some(Duration::from_millis(5)),
        });

        assert!(mgr.servers.contains_key("srv"));
        assert!(!mgr.initializing_servers.contains_key("srv"));
        assert_eq!(mgr.event_buf.len(), 1);
        // The oneshot should have received Ok(())
        assert!(rx.try_recv().unwrap().is_ok());
    }

    #[test]
    fn test_handle_actor_event_initialize_error_with_dummy_handle() {
        let mut mgr = McpManager::new(PathBuf::from("/tmp/creds"));
        let handle = McpServerActorHandle::new_dummy("bad");
        let (tx, mut rx) = oneshot::channel();
        mgr.initializing_servers.insert("bad".to_string(), (handle, tx));

        mgr.handle_mcp_actor_event(McpServerActorEvent::InitializeError {
            server_name: "bad".to_string(),
            error: "timeout".to_string(),
        });

        assert!(!mgr.initializing_servers.contains_key("bad"));
        assert!(mgr.failed_servers.contains("bad"));
        assert_eq!(mgr.event_buf.len(), 1);
        // The oneshot should have received an error
        let result = rx.try_recv().unwrap();
        assert!(result.is_err());
    }

    #[test]
    fn test_handle_actor_event_initialize_error_no_matching_server() {
        let mut mgr = McpManager::new(PathBuf::from("/tmp/creds"));
        // No server in initializing_servers
        mgr.handle_mcp_actor_event(McpServerActorEvent::InitializeError {
            server_name: "ghost".to_string(),
            error: "gone".to_string(),
        });
        // Should still track as failed
        assert!(mgr.failed_servers.contains("ghost"));
        assert_eq!(mgr.event_buf.len(), 1);
    }

    #[test]
    fn test_handle_actor_event_initialized_duplicate_server() {
        let mut mgr = McpManager::new(PathBuf::from("/tmp/creds"));
        // Pre-populate servers with a dummy
        mgr.servers
            .insert("dup".to_string(), McpServerActorHandle::new_dummy("dup"));
        // Also put in initializing
        let handle = McpServerActorHandle::new_dummy("dup");
        let (tx, _rx) = oneshot::channel();
        mgr.initializing_servers.insert("dup".to_string(), (handle, tx));

        mgr.handle_mcp_actor_event(McpServerActorEvent::Initialized {
            server_name: "dup".to_string(),
            serve_duration: Duration::from_millis(1),
            list_tools_duration: None,
            list_prompts_duration: None,
        });

        // Old server replaced, still in servers
        assert!(mgr.servers.contains_key("dup"));
        assert!(!mgr.initializing_servers.contains_key("dup"));
    }

    #[test]
    fn test_handle_actor_event_initialized_dropped_receiver() {
        let mut mgr = McpManager::new(PathBuf::from("/tmp/creds"));
        let handle = McpServerActorHandle::new_dummy("dropped");
        let (tx, rx) = oneshot::channel();
        drop(rx); // Drop receiver before sending
        mgr.initializing_servers.insert("dropped".to_string(), (handle, tx));

        // Should not panic even though receiver is dropped
        mgr.handle_mcp_actor_event(McpServerActorEvent::Initialized {
            server_name: "dropped".to_string(),
            serve_duration: Duration::from_millis(1),
            list_tools_duration: None,
            list_prompts_duration: None,
        });

        assert!(mgr.servers.contains_key("dropped"));
    }

    // --- Tests for handle_mcp_manager_request with injected state ---

    #[tokio::test]
    async fn test_request_get_tools_initializing_server_no_spawn() {
        let mut mgr = McpManager::new(PathBuf::from("/tmp/creds"));
        let handle = McpServerActorHandle::new_dummy("init-srv");
        let (tx, _rx) = oneshot::channel();
        mgr.initializing_servers.insert("init-srv".to_string(), (handle, tx));

        let result = mgr
            .handle_mcp_manager_request(McpManagerRequest::GetToolSpecs {
                server_name: "init-srv".to_string(),
            })
            .await;
        assert!(matches!(
            result,
            Err(McpManagerError::ServerCurrentlyInitializing { .. })
        ));
    }

    #[tokio::test]
    async fn test_request_launch_already_in_servers() {
        let mut mgr = McpManager::new(PathBuf::from("/tmp/creds"));
        mgr.servers
            .insert("existing".to_string(), McpServerActorHandle::new_dummy("existing"));

        let config = McpServerConfig::Local(LocalMcpServerConfig {
            command: "echo".to_string(),
            args: vec![],
            env: None,
            timeout_ms: 1000,
            disabled: false,
            disabled_tools: vec![],
        });
        let result = mgr
            .handle_mcp_manager_request(McpManagerRequest::LaunchServer {
                server_name: "existing".to_string(),
                config,
            })
            .await;
        assert!(matches!(result, Err(McpManagerError::ServerAlreadyLaunched { .. })));
    }

    #[tokio::test]
    async fn test_request_launch_already_initializing_no_spawn() {
        let mut mgr = McpManager::new(PathBuf::from("/tmp/creds"));
        let handle = McpServerActorHandle::new_dummy("dup");
        let (tx, _rx) = oneshot::channel();
        mgr.initializing_servers.insert("dup".to_string(), (handle, tx));

        let config = McpServerConfig::Local(LocalMcpServerConfig {
            command: "echo".to_string(),
            args: vec![],
            env: None,
            timeout_ms: 1000,
            disabled: false,
            disabled_tools: vec![],
        });
        let result = mgr
            .handle_mcp_manager_request(McpManagerRequest::LaunchServer {
                server_name: "dup".to_string(),
                config,
            })
            .await;
        assert!(matches!(
            result,
            Err(McpManagerError::ServerCurrentlyInitializing { .. })
        ));
    }

    #[tokio::test]
    async fn test_request_terminate_with_servers() {
        let mut mgr = McpManager::new(PathBuf::from("/tmp/creds"));
        mgr.servers
            .insert("s1".to_string(), McpServerActorHandle::new_dummy("s1"));
        mgr.servers
            .insert("s2".to_string(), McpServerActorHandle::new_dummy("s2"));

        let result = mgr.handle_mcp_manager_request(McpManagerRequest::Terminate).await;
        assert!(matches!(result, Ok(McpManagerResponse::TerminateAcknowledged)));
    }

    // --- event_buf processing tests ---

    #[test]
    fn test_event_buf_accumulates_multiple_events() {
        let mut mgr = McpManager::new(PathBuf::from("/tmp/creds"));
        mgr.handle_mcp_actor_event(McpServerActorEvent::Initializing {
            server_name: "a".to_string(),
        });
        mgr.handle_mcp_actor_event(McpServerActorEvent::OauthRequest {
            server_name: "b".to_string(),
            oauth_url: "https://x".to_string(),
        });
        mgr.handle_mcp_actor_event(McpServerActorEvent::ToolListChanged {
            server_name: "c".to_string(),
        });
        assert_eq!(mgr.event_buf.len(), 3);
    }

    #[test]
    fn test_event_buf_drain() {
        let mut mgr = McpManager::new(PathBuf::from("/tmp/creds"));
        mgr.event_buf.push(McpServerActorEvent::Initializing {
            server_name: "x".to_string(),
        });
        mgr.event_buf.push(McpServerActorEvent::ToolListChanged {
            server_name: "y".to_string(),
        });

        let drained: Vec<_> = mgr.event_buf.drain(..).collect();
        assert_eq!(drained.len(), 2);
        assert!(mgr.event_buf.is_empty());
    }

    // --- failed_servers tracking ---

    #[test]
    fn test_failed_servers_multiple_failures() {
        let mut mgr = McpManager::new(PathBuf::from("/tmp/creds"));
        mgr.handle_mcp_actor_event(McpServerActorEvent::InitializeError {
            server_name: "s1".to_string(),
            error: "err1".to_string(),
        });
        mgr.handle_mcp_actor_event(McpServerActorEvent::InitializeError {
            server_name: "s2".to_string(),
            error: "err2".to_string(),
        });
        // Same server failing again
        mgr.handle_mcp_actor_event(McpServerActorEvent::InitializeError {
            server_name: "s1".to_string(),
            error: "err3".to_string(),
        });
        assert_eq!(mgr.failed_servers.len(), 2);
        assert!(mgr.failed_servers.contains("s1"));
        assert!(mgr.failed_servers.contains("s2"));
    }

    // --- Serde tests for McpServerConfig variants ---

    #[test]
    fn test_serde_local_mcp_server_config() {
        let config = McpServerConfig::Local(LocalMcpServerConfig {
            command: "node".to_string(),
            args: vec!["server.js".to_string()],
            env: Some(HashMap::from([("KEY".to_string(), "VAL".to_string())])),
            timeout_ms: 5000,
            disabled: false,
            disabled_tools: vec!["tool1".to_string()],
        });
        let json = serde_json::to_string(&config).unwrap();
        let parsed: McpServerConfig = serde_json::from_str(&json).unwrap();
        match parsed {
            McpServerConfig::Local(c) => {
                assert_eq!(c.command, "node");
                assert_eq!(c.args, vec!["server.js"]);
                assert_eq!(c.timeout_ms, 5000);
                assert!(!c.disabled);
                assert_eq!(c.disabled_tools, vec!["tool1"]);
            },
            _ => panic!("expected Local"),
        }
    }

    #[test]
    fn test_serde_remote_mcp_server_config() {
        use crate::agent::agent_config::definitions::RemoteMcpServerConfig;
        let config = McpServerConfig::Remote(RemoteMcpServerConfig {
            url: "https://mcp.example.com".to_string(),
            headers: HashMap::from([("Authorization".to_string(), "Bearer tok".to_string())]),
            timeout_ms: 3000,
            oauth_scopes: vec!["read".to_string()],
            oauth: None,
            disabled: true,
            disabled_tools: vec![],
        });
        let json = serde_json::to_string(&config).unwrap();
        let parsed: McpServerConfig = serde_json::from_str(&json).unwrap();
        match parsed {
            McpServerConfig::Remote(c) => {
                assert_eq!(c.url, "https://mcp.example.com");
                assert!(c.disabled);
                assert_eq!(c.oauth_scopes, vec!["read"]);
            },
            _ => panic!("expected Remote"),
        }
    }

    #[test]
    fn test_serde_mcp_server_actor_event_all_variants() {
        let events = vec![
            McpServerActorEvent::Initializing {
                server_name: "s".to_string(),
            },
            McpServerActorEvent::Initialized {
                server_name: "s".to_string(),
                serve_duration: Duration::from_secs(2),
                list_tools_duration: Some(Duration::from_millis(200)),
                list_prompts_duration: None,
            },
            McpServerActorEvent::InitializeError {
                server_name: "s".to_string(),
                error: "e".to_string(),
            },
            McpServerActorEvent::OauthRequest {
                server_name: "s".to_string(),
                oauth_url: "u".to_string(),
            },
            McpServerActorEvent::ToolListChanged {
                server_name: "s".to_string(),
            },
        ];
        for evt in events {
            let json = serde_json::to_string(&evt).unwrap();
            let _parsed: McpServerActorEvent = serde_json::from_str(&json).unwrap();
        }
    }

    // --- McpManagerError additional variants ---

    #[test]
    fn test_mcp_manager_error_all_variants_serde() {
        let errors: Vec<McpManagerError> = vec![
            McpManagerError::ServerNotInitialized { name: "a".to_string() },
            McpManagerError::ServerCurrentlyInitializing { name: "b".to_string() },
            McpManagerError::ServerFailed { name: "c".to_string() },
            McpManagerError::ServerAlreadyLaunched { name: "d".to_string() },
            McpManagerError::McpActor(McpServerActorError::Channel),
            McpManagerError::McpActor(McpServerActorError::Custom("x".to_string())),
            McpManagerError::McpActor(McpServerActorError::Service {
                message: "svc err".to_string(),
                source: None,
            }),
            McpManagerError::Channel,
            McpManagerError::Custom("custom".to_string()),
        ];
        for e in &errors {
            let json = serde_json::to_string(e).unwrap();
            let parsed: McpManagerError = serde_json::from_str(&json).unwrap();
            // Display should round-trip
            assert!(!parsed.to_string().is_empty());
        }
    }

    #[test]
    fn test_mcp_server_actor_error_display() {
        let e = McpServerActorError::Service {
            message: "connection reset".to_string(),
            source: None,
        };
        assert!(e.to_string().contains("connection reset"));

        let e2 = McpServerActorError::Channel;
        assert_eq!(e2.to_string(), "The channel has closed");

        let e3 = McpServerActorError::Custom("custom err".to_string());
        assert_eq!(e3.to_string(), "custom err");
    }

    // --- McpManagerRequest variants ---

    #[test]
    fn test_mcp_manager_request_all_variants_clone_debug() {
        let requests: Vec<McpManagerRequest> = vec![
            McpManagerRequest::LaunchServer {
                server_name: "s".to_string(),
                config: McpServerConfig::Local(LocalMcpServerConfig {
                    command: "x".to_string(),
                    args: vec![],
                    env: None,
                    timeout_ms: 1000,
                    disabled: false,
                    disabled_tools: vec![],
                }),
            },
            McpManagerRequest::GetToolSpecs {
                server_name: "s".to_string(),
            },
            McpManagerRequest::GetPrompts {
                server_name: "s".to_string(),
            },
            McpManagerRequest::GetPrompt {
                server_name: "s".to_string(),
                name: "p".to_string(),
                arguments: HashMap::new(),
            },
            McpManagerRequest::ExecuteTool {
                server_name: "s".to_string(),
                tool_name: "t".to_string(),
                args: None,
            },
            McpManagerRequest::Terminate,
        ];
        for req in &requests {
            let cloned = req.clone();
            let debug = format!("{:?}", cloned);
            assert!(!debug.is_empty());
        }
    }

    // --- McpManager Default ---

    #[test]
    fn test_mcp_manager_default() {
        let mgr = McpManager::default();
        assert!(mgr.servers.is_empty());
        assert!(mgr.failed_servers.is_empty());
    }
}
