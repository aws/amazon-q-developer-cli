use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use rmcp::ServiceError;
use rmcp::model::{
    CallToolRequestParam,
    Prompt as RmcpPrompt,
    Tool as RmcpTool,
};
use serde::{
    Deserialize,
    Serialize,
};
use serde_json::Value;
use tokio::sync::{
    mpsc,
    oneshot,
};
use tracing::{
    debug,
    error,
    warn,
};

use super::ExecuteToolResult;
use super::service::{
    McpService,
    RunningMcpService,
};
use super::types::Prompt;
use crate::agent::agent_config::definitions::McpServerConfig;
use crate::agent::agent_loop::types::ToolSpec;
use crate::agent::util::request_channel::{
    RequestReceiver,
    RequestSender,
    new_request_channel,
    respond,
};

/// Represents a message from an MCP server to the client.
#[derive(Debug)]
pub enum McpMessage {
    Tools(Result<Vec<RmcpTool>, ServiceError>),
    Prompts(Result<Vec<RmcpPrompt>, ServiceError>),
    ExecuteTool { request_id: u32, result: ExecuteToolResult },
}

#[derive(Debug)]
pub struct McpServerActorHandle {
    _server_name: String,
    sender: RequestSender<McpServerActorRequest, McpServerActorResponse, McpServerActorError>,
    event_rx: mpsc::Receiver<McpServerActorEvent>,
}

impl McpServerActorHandle {
    pub async fn recv(&mut self) -> Option<McpServerActorEvent> {
        self.event_rx.recv().await
    }

    pub async fn get_tool_specs(&self) -> Result<Vec<ToolSpec>, McpServerActorError> {
        match self
            .sender
            .send_recv(McpServerActorRequest::GetTools)
            .await
            .unwrap_or(Err(McpServerActorError::Channel))?
        {
            McpServerActorResponse::Tools(tool_specs) => Ok(tool_specs),
            other => Err(McpServerActorError::Custom(format!(
                "received unexpected response: {other:?}"
            ))),
        }
    }

    pub async fn get_prompts(&self) -> Result<Vec<Prompt>, McpServerActorError> {
        match self
            .sender
            .send_recv(McpServerActorRequest::GetPrompts)
            .await
            .unwrap_or(Err(McpServerActorError::Channel))?
        {
            McpServerActorResponse::Prompts(prompts) => Ok(prompts),
            other => Err(McpServerActorError::Custom(format!(
                "received unexpected response: {other:?}"
            ))),
        }
    }

    pub async fn execute_tool(
        &self,
        name: String,
        args: Option<serde_json::Map<String, Value>>,
    ) -> Result<oneshot::Receiver<ExecuteToolResult>, McpServerActorError> {
        match self
            .sender
            .send_recv(McpServerActorRequest::ExecuteTool { name, args })
            .await
            .unwrap_or(Err(McpServerActorError::Channel))?
        {
            McpServerActorResponse::ExecuteTool(rx) => Ok(rx),
            other => Err(McpServerActorError::Custom(format!(
                "received unexpected response: {other:?}"
            ))),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum McpServerActorRequest {
    GetTools,
    GetPrompts,
    ExecuteTool {
        name: String,
        args: Option<serde_json::Map<String, Value>>,
    },
}

#[derive(Debug)]
enum McpServerActorResponse {
    Tools(Vec<ToolSpec>),
    Prompts(Vec<Prompt>),
    ExecuteTool(oneshot::Receiver<ExecuteToolResult>),
}

#[derive(Debug, Clone, Serialize, Deserialize, thiserror::Error)]
pub enum McpServerActorError {
    #[error("An error occurred with the service: {}", .message)]
    Service {
        message: String,
        #[serde(skip)]
        #[source]
        source: Option<Arc<ServiceError>>,
    },
    #[error("The channel has closed")]
    Channel,
    #[error("{}", .0)]
    Custom(String),
}

impl From<ServiceError> for McpServerActorError {
    fn from(value: ServiceError) -> Self {
        Self::Service {
            message: value.to_string(),
            source: Some(Arc::new(value)),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum McpServerActorEvent {
    /// The MCP server has launched successfully
    Initialized {
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
    InitializeError(String),
}

#[derive(Debug)]
pub struct McpServerActor {
    /// Name of the MCP server
    server_name: String,
    /// Config the server was launched with. Kept for debug purposes.
    _config: McpServerConfig,
    /// Tools
    tools: Vec<ToolSpec>,
    /// Prompts
    prompts: Vec<Prompt>,
    /// Handle to an MCP server
    service_handle: RunningMcpService,

    /// Monotonically increasing id for tool executions
    curr_tool_execution_id: u32,
    executing_tools: HashMap<u32, oneshot::Sender<ExecuteToolResult>>,

    /// Receiver for actor requests
    req_rx: RequestReceiver<McpServerActorRequest, McpServerActorResponse, McpServerActorError>,
    /// Sender for actor events
    event_tx: mpsc::Sender<McpServerActorEvent>,
    message_tx: mpsc::Sender<McpMessage>,
    message_rx: mpsc::Receiver<McpMessage>,
}

impl McpServerActor {
    /// Spawns an actor to manage the MCP server, returning a [McpServerActorHandle].
    pub fn spawn(server_name: String, config: McpServerConfig) -> McpServerActorHandle {
        let (event_tx, event_rx) = mpsc::channel(32);
        let (req_tx, req_rx) = new_request_channel();

        let server_name_clone = server_name.clone();
        tokio::spawn(async move { Self::launch(server_name_clone, config, req_rx, event_tx).await });

        McpServerActorHandle {
            _server_name: server_name,
            sender: req_tx,
            event_rx,
        }
    }

    async fn launch(
        server_name: String,
        config: McpServerConfig,
        req_rx: RequestReceiver<McpServerActorRequest, McpServerActorResponse, McpServerActorError>,
        event_tx: mpsc::Sender<McpServerActorEvent>,
    ) {
        let (message_tx, message_rx) = mpsc::channel(32);
        match McpService::new(server_name.clone(), config.clone(), message_tx.clone())
            .launch()
            .await
        {
            Ok((service_handle, launch_md)) => {
                let s = Self {
                    server_name,
                    _config: config,
                    tools: launch_md.tools.unwrap_or_default(),
                    prompts: launch_md.prompts.unwrap_or_default(),
                    service_handle,
                    req_rx,
                    event_tx,
                    message_tx,
                    message_rx,
                    curr_tool_execution_id: Default::default(),
                    executing_tools: Default::default(),
                };
                let _ = s
                    .event_tx
                    .send(McpServerActorEvent::Initialized {
                        serve_duration: launch_md.serve_time_taken,
                        list_tools_duration: launch_md.list_tools_duration,
                        list_prompts_duration: launch_md.list_prompts_duration,
                    })
                    .await;
                s.main_loop().await;
            },
            Err(err) => {
                let _ = event_tx
                    .send(McpServerActorEvent::InitializeError(err.to_string()))
                    .await;
            },
        }
    }

    async fn main_loop(mut self) {
        loop {
            tokio::select! {
                req = self.req_rx.recv() => {
                    let Some(req) = req else {
                        warn!(server_name = &self.server_name, "mcp request receiver channel has closed, exiting");
                        break;
                    };
                    let res = self.handle_actor_request(req.payload).await;
                    respond!(req, res);
                },
                res = self.message_rx.recv() => {
                    self.handle_mcp_message(res).await;
                }
            }
        }
    }

    async fn handle_actor_request(
        &mut self,
        req: McpServerActorRequest,
    ) -> Result<McpServerActorResponse, McpServerActorError> {
        debug!(?self.server_name, ?req, "MCP actor received new request");
        match req {
            McpServerActorRequest::GetTools => Ok(McpServerActorResponse::Tools(self.tools.clone())),
            McpServerActorRequest::GetPrompts => Ok(McpServerActorResponse::Prompts(self.prompts.clone())),
            McpServerActorRequest::ExecuteTool { name, args } => {
                let (tx, rx) = oneshot::channel();
                self.curr_tool_execution_id = self.curr_tool_execution_id.wrapping_add(1);
                let request_id = self.curr_tool_execution_id;
                let service_handle = self.service_handle.clone();
                let message_tx = self.message_tx.clone();
                tokio::spawn(async move {
                    let result = service_handle
                        .call_tool(CallToolRequestParam {
                            name: name.into(),
                            arguments: args,
                        })
                        .await
                        .map_err(McpServerActorError::from);
                    let _ = message_tx.send(McpMessage::ExecuteTool { request_id, result }).await;
                });
                self.executing_tools.insert(self.curr_tool_execution_id, tx);
                Ok(McpServerActorResponse::ExecuteTool(rx))
            },
        }
    }

    async fn handle_mcp_message(&mut self, msg: Option<McpMessage>) {
        debug!(?self.server_name, ?msg, "MCP actor received new message");
        let Some(msg) = msg else {
            warn!("MCP message receiver has closed");
            return;
        };
        match msg {
            McpMessage::Tools(res) => match res {
                Ok(tools) => self.tools = tools.into_iter().map(Into::into).collect(),
                Err(err) => {
                    error!(?err, "failed to list tools");
                },
            },
            McpMessage::Prompts(res) => match res {
                Ok(prompts) => self.prompts = prompts.into_iter().map(Into::into).collect(),
                Err(err) => {
                    error!(?err, "failed to list prompts");
                },
            },
            McpMessage::ExecuteTool { request_id, result } => match self.executing_tools.remove(&request_id) {
                Some(tx) => {
                    let _ = tx.send(result);
                },
                None => {
                    warn!(
                        ?request_id,
                        ?result,
                        "received an execute tool result for an execution that does not exist"
                    );
                },
            },
        }
    }

    /// Asynchronously fetch all tools
    #[allow(dead_code)]
    fn refresh_tools(&self) {
        let service_handle = self.service_handle.clone();
        let tx = self.message_tx.clone();
        tokio::spawn(async move {
            let res = service_handle.list_tools().await;
            let _ = tx.send(McpMessage::Tools(res)).await;
        });
    }

    /// Asynchronously fetch all prompts
    #[allow(dead_code)]
    fn refresh_prompts(&self) {
        let service_handle = self.service_handle.clone();
        let tx = self.message_tx.clone();
        tokio::spawn(async move {
            let res = service_handle.list_prompts().await;
            let _ = tx.send(McpMessage::Prompts(res)).await;
        });
    }
}
