mod actor;
mod service;
mod types;

use std::collections::HashMap;

use actor::{
    McpServerActor,
    McpServerActorError,
    McpServerActorEvent,
    McpServerActorHandle,
};
use futures::stream::FuturesUnordered;
use rmcp::model::CallToolResult;
use serde::{
    Deserialize,
    Serialize,
};
use serde_json::Value;
use tokio::sync::oneshot;
use tokio_stream::StreamExt as _;
use tracing::{
    debug,
    error,
    warn,
};

use super::agent_loop::types::ToolSpec;
use super::util::request_channel::{
    RequestReceiver,
    new_request_channel,
};
use crate::agent::agent_config::definitions::McpServerConfig;
use crate::agent::util::request_channel::{
    RequestSender,
    respond,
};

#[derive(Debug, Clone)]
pub struct McpManagerHandle {
    /// Sender for sending requests to the tool manager task
    sender: RequestSender<McpManagerRequest, McpManagerResponse, McpManagerError>,
}

impl McpManagerHandle {
    fn new(sender: RequestSender<McpManagerRequest, McpManagerResponse, McpManagerError>) -> Self {
        Self { sender }
    }

    pub async fn launch_server(
        &self,
        name: String,
        config: McpServerConfig,
    ) -> Result<oneshot::Receiver<LaunchServerResult>, McpManagerError> {
        match self
            .sender
            .send_recv(McpManagerRequest::LaunchServer {
                server_name: name,
                config,
            })
            .await
            .unwrap_or(Err(McpManagerError::Channel))?
        {
            McpManagerResponse::LaunchServer(rx) => Ok(rx),
            other => Err(McpManagerError::Custom(format!(
                "received unexpected response: {:?}",
                other
            ))),
        }
    }

    pub async fn get_tool_specs(&self, server_name: String) -> Result<Vec<ToolSpec>, McpManagerError> {
        match self
            .sender
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

    pub async fn execute_tool(
        &self,
        server_name: String,
        tool_name: String,
        args: Option<serde_json::Map<String, Value>>,
    ) -> Result<oneshot::Receiver<ExecuteToolResult>, McpManagerError> {
        match self
            .sender
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
}

#[derive(Debug)]
pub struct McpManager {
    request_tx: RequestSender<McpManagerRequest, McpManagerResponse, McpManagerError>,
    request_rx: RequestReceiver<McpManagerRequest, McpManagerResponse, McpManagerError>,

    initializing_servers: HashMap<String, (McpServerActorHandle, oneshot::Sender<LaunchServerResult>)>,
    servers: HashMap<String, McpServerActorHandle>,
}

impl McpManager {
    pub fn new() -> Self {
        let (request_tx, request_rx) = new_request_channel();
        Self {
            request_tx,
            request_rx,
            initializing_servers: HashMap::new(),
            servers: HashMap::new(),
        }
    }

    pub fn spawn(self) -> McpManagerHandle {
        let request_tx = self.request_tx.clone();

        tokio::spawn(async move {
            self.main_loop().await;
        });

        McpManagerHandle::new(request_tx)
    }

    async fn main_loop(mut self) {
        loop {
            let mut initializing_servers = FuturesUnordered::new();
            for (name, (handle, _)) in &mut self.initializing_servers {
                let name_clone = name.clone();
                initializing_servers.push(async { (name_clone, handle.recv().await) });
            }
            let mut initialized_servers = FuturesUnordered::new();
            for (name, handle) in &mut self.servers {
                let name_clone = name.clone();
                initialized_servers.push(async { (name_clone, handle.recv().await) });
            }

            tokio::select! {
                req = self.request_rx.recv() => {
                    std::mem::drop(initializing_servers);
                    std::mem::drop(initialized_servers);
                    let Some(req) = req else {
                        warn!("Tool manager request channel has closed, exiting");
                        break;
                    };
                    let res = self.handle_mcp_manager_request(req.payload).await;
                    respond!(req, res);
                },
                res = initializing_servers.next(), if !initializing_servers.is_empty() => {
                    std::mem::drop(initializing_servers);
                    std::mem::drop(initialized_servers);
                    if let Some((name, evt)) = res {
                        self.handle_initializing_mcp_actor_event(name, evt).await;
                    }
                },
                res = initialized_servers.next(), if !initialized_servers.is_empty() => {
                    std::mem::drop(initializing_servers);
                    std::mem::drop(initialized_servers);
                    if let Some((name, evt)) = res {
                        self.handle_mcp_actor_event(name, evt).await;
                    }
                },
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
                let (tx, rx) = oneshot::channel();
                let handle = McpServerActor::spawn(name.clone(), config);
                self.initializing_servers.insert(name, (handle, tx));
                Ok(McpManagerResponse::LaunchServer(rx))
            },
            McpManagerRequest::GetToolSpecs { server_name: name } => match self.servers.get(&name) {
                Some(handle) => Ok(McpManagerResponse::ToolSpecs(handle.get_tool_specs().await?)),
                None => Err(McpManagerError::ServerNotInitialized { name }),
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

    async fn handle_mcp_actor_event(&mut self, server_name: String, evt: Option<McpServerActorEvent>) {
        debug!(?server_name, ?evt, "Received event from an MCP actor");
        debug_assert!(self.servers.contains_key(&server_name));
    }

    async fn handle_initializing_mcp_actor_event(&mut self, server_name: String, evt: Option<McpServerActorEvent>) {
        debug!(?server_name, ?evt, "Received event from initializing MCP actor");
        debug_assert!(self.initializing_servers.contains_key(&server_name));

        let Some((handle, tx)) = self.initializing_servers.remove(&server_name) else {
            warn!(?server_name, ?evt, "event was not from an initializing MCP server");
            return;
        };

        // Event should always exist, otherwise indicates a bug with the initialization logic.
        let Some(evt) = evt else {
            let _ = tx.send(Err(McpManagerError::Custom("Server channel closed".to_string())));
            self.initializing_servers.remove(&server_name);
            return;
        };

        // First event from an initializing server should only be either of these Initialize variants.
        match evt {
            McpServerActorEvent::Initialized { .. } => {
                let _ = tx.send(Ok(()));
                self.servers.insert(server_name, handle);
            },
            McpServerActorEvent::InitializeError(msg) => {
                let _ = tx.send(Err(McpManagerError::Custom(msg)));
                self.initializing_servers.remove(&server_name);
            },
        }
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
        /// Server name
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
    LaunchServer(oneshot::Receiver<LaunchServerResult>),
    ToolSpecs(Vec<ToolSpec>),
    ExecuteTool(oneshot::Receiver<ExecuteToolResult>),
}

pub type ExecuteToolResult = Result<CallToolResult, McpServerActorError>;

type LaunchServerResult = Result<(), McpManagerError>;

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

#[cfg(test)]
mod tests {
    use super::*;

    const MCP_CONFIG: &str = r#"
{
    "mcpServers": {
        "amazon-internal-mcp-server": {
            "command": "amzn-mcp",
            "args": [],
            "env": {}
        },
        "aws-knowledge-mcp-server": {
            "type": "http",
            "url": "https://knowledge-mcp.global.api.aws"
        },
        "github": {
            "type": "http",
            "url": "https://api.githubcopilot.com/mcp/"
        }
    }
}
"#;

    const LOCAL_CONFIG: &str = r#"
{
    "command": "amzn-mcp",
    "args": [],
    "env": {}
}
"#;

    #[tokio::test]
    async fn test_mcp_actor() {
        let mut handle = McpServerActor::spawn("Amazon MCP".to_string(), serde_json::from_str(LOCAL_CONFIG).unwrap());
        let res = handle.recv().await;
        println!("Got res: {:?}", res);
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        let tools = handle.get_tool_specs().await;
        println!("Got tools: {:?}", tools);
        let prompts = handle.get_prompts().await;
        println!("Got prompts: {:?}", prompts);
    }
}
