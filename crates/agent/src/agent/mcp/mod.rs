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
mod service;
pub mod types;

use std::collections::HashMap;
use std::path::PathBuf;

use actor::{
    McpServerActor,
    McpServerActorError,
    McpServerActorEvent,
    McpServerActorHandle,
};
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
    error,
    warn,
};
use types::Prompt;

use super::agent_loop::types::ToolSpec;
use super::consts::DEFAULT_MCP_CREDENTIAL_PATH;
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

#[derive(Debug)]
pub struct McpManagerHandle {
    /// Sender for sending requests to the tool manager task
    request_tx: RequestSender<McpManagerRequest, McpManagerResponse, McpManagerError>,
    server_to_handle_server_event_tx: mpsc::Sender<McpServerActorEvent>,
    mcp_main_loop_to_handle_server_event_rx: broadcast::Receiver<McpServerActorEvent>,
}

impl Clone for McpManagerHandle {
    fn clone(&self) -> Self {
        Self {
            request_tx: self.request_tx.clone(),
            server_to_handle_server_event_tx: self.server_to_handle_server_event_tx.clone(),
            mcp_main_loop_to_handle_server_event_rx: self.mcp_main_loop_to_handle_server_event_rx.resubscribe(),
        }
    }
}

impl McpManagerHandle {
    fn new(
        request_tx: RequestSender<McpManagerRequest, McpManagerResponse, McpManagerError>,
        server_to_handle_server_event_tx: mpsc::Sender<McpServerActorEvent>,
        mcp_main_loop_to_handle_server_event_rx: broadcast::Receiver<McpServerActorEvent>,
    ) -> Self {
        Self {
            request_tx,
            server_to_handle_server_event_tx,
            mcp_main_loop_to_handle_server_event_rx,
        }
    }

    pub async fn launch_server(
        &mut self,
        name: String,
        config: McpServerConfig,
    ) -> Result<McpManagerResponse, McpManagerError> {
        let server_event_sender = self.server_to_handle_server_event_tx.clone();

        self.request_tx
            .send_recv(McpManagerRequest::LaunchServer {
                server_name: name,
                server_event_sender,
                config,
            })
            .await
            .unwrap_or(Err(McpManagerError::Channel))
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
                "received unexpected response: {:?}",
                other
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
                "received unexpected response: {:?}",
                other
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
                "received unexpected response: {:?}",
                other
            ))),
        }
    }

    pub async fn recv(&mut self) -> Result<McpServerActorEvent, RecvError> {
        self.mcp_main_loop_to_handle_server_event_rx.recv().await
    }
}

#[derive(Debug)]
pub struct McpManager {
    request_tx: RequestSender<McpManagerRequest, McpManagerResponse, McpManagerError>,
    request_rx: RequestReceiver<McpManagerRequest, McpManagerResponse, McpManagerError>,
    server_event_tx: mpsc::Sender<McpServerActorEvent>,
    server_event_rx: mpsc::Receiver<McpServerActorEvent>,

    cred_path: PathBuf,

    initializing_servers: HashMap<String, McpServerActorHandle>,
    servers: HashMap<String, McpServerActorHandle>,
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
        }
    }

    pub fn spawn(self) -> McpManagerHandle {
        let request_tx = self.request_tx.clone();
        let server_to_handle_server_event_tx = self.server_event_tx.clone();
        let (mcp_main_loop_to_handle_server_event_tx, mcp_main_loop_to_handle_server_event_rx) =
            broadcast::channel::<McpServerActorEvent>(100);

        tokio::spawn(async move {
            self.main_loop(mcp_main_loop_to_handle_server_event_tx).await;
        });

        McpManagerHandle::new(
            request_tx,
            server_to_handle_server_event_tx,
            mcp_main_loop_to_handle_server_event_rx,
        )
    }

    async fn main_loop(mut self, mcp_main_loop_to_handle_server_event_tx: broadcast::Sender<McpServerActorEvent>) {
        loop {
            tokio::select! {
                req = self.request_rx.recv() => {
                    let Some(req) = req else {
                        warn!("Tool manager request channel has closed, exiting");
                        break;
                    };
                    let res = self.handle_mcp_manager_request(req.payload).await;
                    respond!(req, res);
                },
                res = self.server_event_rx.recv() => {
                    if let Some(evt) = res {
                        self.handle_mcp_actor_event(evt, &mcp_main_loop_to_handle_server_event_tx);
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
                server_event_sender: event_tx,
            } => {
                if self.initializing_servers.contains_key(&name) {
                    return Err(McpManagerError::ServerCurrentlyInitializing { name });
                } else if self.servers.contains_key(&name) {
                    return Err(McpManagerError::ServerAlreadyLaunched { name });
                }
                let handle = McpServerActor::spawn(name.clone(), config, self.cred_path.clone(), event_tx);
                self.initializing_servers.insert(name, handle);
                Ok(McpManagerResponse::LaunchServer)
            },
            McpManagerRequest::GetToolSpecs { server_name } => match self.servers.get(&server_name) {
                Some(handle) => Ok(McpManagerResponse::ToolSpecs(handle.get_tool_specs().await?)),
                None => Err(McpManagerError::ServerNotInitialized { name: server_name }),
            },
            McpManagerRequest::GetPrompts { server_name } => match self.servers.get(&server_name) {
                Some(handle) => Ok(McpManagerResponse::Prompts(handle.get_prompts().await?)),
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
        }
    }

    fn handle_mcp_actor_event(
        &mut self,
        evt: McpServerActorEvent,
        mcp_main_loop_to_handle_server_event_tx: &broadcast::Sender<McpServerActorEvent>,
    ) {
        // TODO: keep a record of all the different server events received in this layer?
        match &evt {
            McpServerActorEvent::Initialized {
                server_name,
                serve_duration: _,
                list_tools_duration: _,
                list_prompts_duration: _,
            } => {
                let Some(handle) = self.initializing_servers.remove(server_name) else {
                    warn!(?server_name, ?evt, "event was not from an initializing MCP server");
                    return;
                };

                if self.servers.insert(server_name.clone(), handle).is_some() {
                    warn!(?server_name, "duplicated server. old server dropped");
                }
            },
            McpServerActorEvent::InitializeError { server_name, error: _ } => {
                self.initializing_servers.remove(server_name);
            },
            McpServerActorEvent::OauthRequest { server_name, oauth_url } => {
                tracing::info!(?server_name, ?oauth_url, "received oauth request");
            },
        }
        let _ = mcp_main_loop_to_handle_server_event_tx.send(evt);
    }
}

impl Default for McpManager {
    fn default() -> Self {
        let expanded_path =
            shellexpand::full(DEFAULT_MCP_CREDENTIAL_PATH).expect("failed to expand default credential path");
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
        /// Channel for sending server events back to the manager
        server_event_sender: mpsc::Sender<McpServerActorEvent>,
    },
    GetToolSpecs {
        server_name: String,
    },
    GetPrompts {
        server_name: String,
    },
    ExecuteTool {
        server_name: String,
        tool_name: String,
        args: Option<serde_json::Map<String, Value>>,
    },
}

#[derive(Debug)]
pub enum McpManagerResponse {
    LaunchServer,
    ToolSpecs(Vec<ToolSpec>),
    Prompts(Vec<Prompt>),
    ExecuteTool(oneshot::Receiver<ExecuteToolResult>),
}

pub type ExecuteToolResult = Result<CallToolResult, McpServerActorError>;

#[derive(Debug, Clone, Serialize, Deserialize, thiserror::Error)]
pub enum McpManagerError {
    #[error("Server with the name {} is not initialized", .name)]
    ServerNotInitialized { name: String },
    #[error("Server with the name {} is currently initializing", .name)]
    ServerCurrentlyInitializing { name: String },
    #[error("Server with the name {} has already launched", .name)]
    ServerAlreadyLaunched { name: String },
    #[error(transparent)]
    McpActor(#[from] McpServerActorError),
    #[error("The channel has closed")]
    Channel,
    #[error("{}", .0)]
    Custom(String),
}
