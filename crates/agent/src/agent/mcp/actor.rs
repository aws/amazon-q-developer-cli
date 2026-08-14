use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use rmcp::ServiceError;
use rmcp::model::{
    CallToolRequestParams,
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

use super::service::{
    McpService,
    RunningMcpService,
};
use super::types::Prompt;
use super::{
    ExecuteToolResult,
    estimate_tool_spec_tokens,
};
use crate::agent::agent_config::definitions::McpServerConfig;
use crate::agent::agent_loop::types::ToolSpec;
use crate::agent::tools::mcp::McpToolAnnotations;
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
    /// `None` only for test handles constructed without a spawned task.
    abort_handle: Option<tokio::task::AbortHandle>,
}

impl McpServerActorHandle {
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

    /// Look up annotations for a single tool by name. Returns `Ok(None)` if
    /// the tool exists but has no annotations, or the tool name is unknown.
    pub async fn get_tool_annotations(
        &self,
        tool_name: String,
    ) -> Result<Option<McpToolAnnotations>, McpServerActorError> {
        match self
            .sender
            .send_recv(McpServerActorRequest::GetToolAnnotations { tool_name })
            .await
            .unwrap_or(Err(McpServerActorError::Channel))?
        {
            McpServerActorResponse::ToolAnnotations(ann) => Ok(ann),
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

    pub async fn get_prompt(
        &self,
        name: String,
        arguments: HashMap<String, String>,
    ) -> Result<Vec<serde_json::Value>, McpServerActorError> {
        match self
            .sender
            .send_recv(McpServerActorRequest::GetPrompt { name, arguments })
            .await
            .unwrap_or(Err(McpServerActorError::Channel))?
        {
            McpServerActorResponse::Prompt(messages) => Ok(messages),
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

    pub fn terminate(&self) {
        _ = self.sender.try_blocking_send_recv(McpServerActorRequest::Terminate);
    }

    /// Async version of [`terminate`](Self::terminate) that awaits server cleanup.
    pub async fn shutdown(&self) {
        _ = self.sender.send_recv(McpServerActorRequest::Terminate).await;
    }

    /// Forcibly abort the spawned actor task (cancels an in-flight launch, e.g. one
    /// blocked on an OAuth redirect, tearing down its loopback).
    pub fn abort(&self) {
        if let Some(handle) = &self.abort_handle {
            handle.abort();
        }
    }

    /// Create a dummy handle for testing without spawning a subprocess.
    #[cfg(test)]
    pub(super) fn new_dummy(name: &str) -> Self {
        let (tx, _rx) = crate::agent::util::request_channel::new_request_channel();
        Self {
            _server_name: name.to_string(),
            sender: tx,
            abort_handle: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum McpServerActorRequest {
    GetTools,
    GetToolAnnotations {
        tool_name: String,
    },
    GetPrompts,
    GetPrompt {
        name: String,
        arguments: HashMap<String, String>,
    },
    ExecuteTool {
        name: String,
        args: Option<serde_json::Map<String, Value>>,
    },
    Terminate,
}

#[derive(Debug)]
enum McpServerActorResponse {
    Tools(Vec<ToolSpec>),
    ToolAnnotations(Option<McpToolAnnotations>),
    Prompts(Vec<Prompt>),
    Prompt(Vec<serde_json::Value>),
    ExecuteTool(oneshot::Receiver<ExecuteToolResult>),
    TerminateAcknowledged,
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
    /// The MCP server is currently initializing
    Initializing { server_name: String },
    /// The MCP server has launched successfully
    Initialized {
        server_name: String,
        /// Time taken to launch the server
        serve_duration: Duration,
        /// Time taken to list all tools.
        ///
        /// None if the server does not advertise tool support.
        list_tools_duration: Option<Duration>,
        /// Time taken to list all prompts
        ///
        /// None if the server does not support prompts, or there was an error fetching prompts.
        list_prompts_duration: Option<Duration>,
        #[serde(default)]
        tool_token_count_estimate: u64,
    },
    /// The MCP server failed to initialize successfully
    InitializeError { server_name: String, error: String },
    /// An OAuth authentication request from the MCP server
    OauthRequest { server_name: String, oauth_url: String },
    /// The MCP server's tool list has changed
    ToolListChanged { server_name: String },
}

#[derive(Debug)]
pub struct McpServerActor {
    /// Name of the MCP server
    server_name: String,
    /// Config the server was launched with. Kept for debug purposes.
    _config: McpServerConfig,
    /// Tools
    tools: Vec<ToolSpec>,
    /// MCP tool annotations indexed by tool name. Populated alongside
    /// `tools`; refreshed on `ToolListChanged`. See [`McpToolAnnotations`].
    tool_annotations: HashMap<String, McpToolAnnotations>,
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
    pub fn spawn(
        server_name: String,
        config: McpServerConfig,
        cred_path: PathBuf,
        event_tx: mpsc::Sender<McpServerActorEvent>,
    ) -> McpServerActorHandle {
        let (req_tx, req_rx) = new_request_channel();

        let server_name_clone = server_name.clone();
        let join_handle =
            tokio::spawn(async move { Self::launch(server_name_clone, config, cred_path, req_rx, event_tx).await });

        McpServerActorHandle {
            _server_name: server_name,
            sender: req_tx,
            abort_handle: Some(join_handle.abort_handle()),
        }
    }

    async fn launch(
        server_name: String,
        config: McpServerConfig,
        cred_path: PathBuf,
        req_rx: RequestReceiver<McpServerActorRequest, McpServerActorResponse, McpServerActorError>,
        event_tx: mpsc::Sender<McpServerActorEvent>,
    ) {
        let (message_tx, message_rx) = mpsc::channel(32);
        match McpService::new(server_name.clone(), config.clone(), cred_path, message_tx.clone())
            .launch(&event_tx)
            .await
        {
            Ok((service_handle, launch_md)) => {
                let tool_token_count_estimate =
                    estimate_tool_spec_tokens(launch_md.tools.as_deref().unwrap_or_default());
                let s = Self {
                    server_name: server_name.clone(),
                    _config: config,
                    tools: launch_md.tools.unwrap_or_default(),
                    tool_annotations: launch_md.tool_annotations.unwrap_or_default(),
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
                        server_name,
                        serve_duration: launch_md.serve_time_taken,
                        list_tools_duration: launch_md.list_tools_duration,
                        list_prompts_duration: launch_md.list_prompts_duration,
                        tool_token_count_estimate,
                    })
                    .await;
                s.main_loop().await;
            },
            Err(err) => {
                let _ = event_tx
                    .send(McpServerActorEvent::InitializeError {
                        server_name,
                        error: err.to_string(),
                    })
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

                    if let Ok(McpServerActorResponse::TerminateAcknowledged) = res {
                        self.service_handle.cancel().await;
                        respond!(req, res);
                        break;
                    } else {
                        respond!(req, res);
                    }
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
            McpServerActorRequest::GetToolAnnotations { tool_name } => Ok(McpServerActorResponse::ToolAnnotations(
                self.tool_annotations.get(&tool_name).cloned(),
            )),
            McpServerActorRequest::GetPrompts => Ok(McpServerActorResponse::Prompts(self.prompts.clone())),
            McpServerActorRequest::GetPrompt { name, arguments } => {
                if self.service_handle.is_transport_closed() {
                    warn!(
                        server_name = &self.server_name,
                        "Transport closed before prompt execution"
                    );
                    let detail = match &self._config {
                        McpServerConfig::Local(_) => format!(
                            "Transport to MCP server '{}' is closed. The server may have written \
                             non-JSON-RPC output to stdout which caused the connection to close.",
                            self.server_name
                        ),
                        McpServerConfig::Remote(_) | McpServerConfig::Registry(_) => format!(
                            "Transport to MCP server '{}' is closed. The server may have \
                             terminated or the connection was lost.",
                            self.server_name
                        ),
                    };
                    return Err(McpServerActorError::Custom(detail));
                }

                let result = self.service_handle.get_prompt(name, arguments).await?;
                let messages: Vec<serde_json::Value> = result
                    .messages
                    .into_iter()
                    .map(|msg| serde_json::to_value(msg).unwrap_or(serde_json::Value::Null))
                    .collect();
                Ok(McpServerActorResponse::Prompt(messages))
            },
            McpServerActorRequest::ExecuteTool { name, args } => {
                // Check transport health before executing the tool call. The rmcp library's
                // serve loop exits (closing stdin) if it encounters a parse error on the MCP
                // server's stdout (for stdio transports). Once closed, all subsequent calls
                // fail with "Transport closed". Detect this early with an actionable error.
                if self.service_handle.is_transport_closed() {
                    warn!(
                        server_name = &self.server_name,
                        "Transport closed before tool execution"
                    );
                    let detail = match &self._config {
                        McpServerConfig::Local(_) => format!(
                            "Transport to MCP server '{}' is closed. The server may have written \
                             non-JSON-RPC output to stdout which caused the connection to close.",
                            self.server_name
                        ),
                        McpServerConfig::Remote(_) | McpServerConfig::Registry(_) => format!(
                            "Transport to MCP server '{}' is closed. The server may have \
                             terminated or the connection was lost.",
                            self.server_name
                        ),
                    };
                    return Err(McpServerActorError::Custom(detail));
                }
                let (tx, rx) = oneshot::channel();
                self.curr_tool_execution_id = self.curr_tool_execution_id.wrapping_add(1);
                let request_id = self.curr_tool_execution_id;
                let service_handle = self.service_handle.clone();
                let message_tx = self.message_tx.clone();
                tokio::spawn(async move {
                    let result = service_handle
                        .call_tool({
                            let mut params = CallToolRequestParams::new(name);
                            params.arguments = args;
                            params
                        })
                        .await
                        .map_err(McpServerActorError::from);
                    let _ = message_tx.send(McpMessage::ExecuteTool { request_id, result }).await;
                });
                self.executing_tools.insert(self.curr_tool_execution_id, tx);
                Ok(McpServerActorResponse::ExecuteTool(rx))
            },
            McpServerActorRequest::Terminate => Ok(McpServerActorResponse::TerminateAcknowledged),
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
                Ok(tools) => {
                    // Refresh annotations alongside tools so a ToolListChanged
                    // notification doesn't leave the annotation cache stale.
                    self.tool_annotations = tools
                        .iter()
                        .filter_map(|t| {
                            let ann = t.annotations.as_ref()?;
                            let ours: McpToolAnnotations = ann.into();
                            ours.or_none().map(|a| (t.name.to_string(), a))
                        })
                        .collect();
                    self.tools = tools.into_iter().map(Into::into).collect();
                    let _ = self
                        .event_tx
                        .send(McpServerActorEvent::ToolListChanged {
                            server_name: self.server_name.clone(),
                        })
                        .await;
                },
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
            let res = service_handle.list_all_tools().await;
            let _ = tx.send(McpMessage::Tools(res)).await;
        });
    }

    /// Asynchronously fetch all prompts
    #[allow(dead_code)]
    fn refresh_prompts(&self) {
        let service_handle = self.service_handle.clone();
        let tx = self.message_tx.clone();
        tokio::spawn(async move {
            let res = service_handle.list_all_prompts().await;
            let _ = tx.send(McpMessage::Prompts(res)).await;
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mcp_server_actor_error_display() {
        let e = McpServerActorError::Service {
            message: "boom".to_string(),
            source: None,
        };
        assert!(e.to_string().contains("boom"));

        let e2 = McpServerActorError::Channel;
        assert_eq!(e2.to_string(), "The channel has closed");

        let e3 = McpServerActorError::Custom("oops".to_string());
        assert_eq!(e3.to_string(), "oops");
    }

    #[test]
    fn test_mcp_server_actor_error_serde() {
        let e = McpServerActorError::Channel;
        let json = serde_json::to_string(&e).unwrap();
        let parsed: McpServerActorError = serde_json::from_str(&json).unwrap();
        assert!(matches!(parsed, McpServerActorError::Channel));

        let e2 = McpServerActorError::Custom("x".into());
        let json2 = serde_json::to_string(&e2).unwrap();
        let parsed2: McpServerActorError = serde_json::from_str(&json2).unwrap();
        match parsed2 {
            McpServerActorError::Custom(s) => assert_eq!(s, "x"),
            _ => panic!("expected Custom"),
        }
    }

    #[test]
    fn test_mcp_server_actor_error_from_service_error() {
        let se = ServiceError::McpError(rmcp::ErrorData::new(
            rmcp::model::ErrorCode::INTERNAL_ERROR,
            "test error",
            None,
        ));
        let actor_err: McpServerActorError = se.into();
        match actor_err {
            McpServerActorError::Service { message, source } => {
                assert!(message.contains("test error"));
                assert!(source.is_some());
            },
            _ => panic!("expected Service variant"),
        }
    }

    #[test]
    fn test_mcp_server_actor_error_service_serde() {
        let e = McpServerActorError::Service {
            message: "connection lost".to_string(),
            source: None,
        };
        let json = serde_json::to_string(&e).unwrap();
        let parsed: McpServerActorError = serde_json::from_str(&json).unwrap();
        match parsed {
            McpServerActorError::Service { message, source } => {
                assert_eq!(message, "connection lost");
                // source is skipped in serde
                assert!(source.is_none());
            },
            _ => panic!("expected Service"),
        }
    }

    #[test]
    fn test_mcp_server_actor_error_clone() {
        let e = McpServerActorError::Custom("test".to_string());
        let cloned = e.clone();
        assert_eq!(e.to_string(), cloned.to_string());
    }

    #[test]
    fn test_mcp_server_actor_event_serde_initializing() {
        let e = McpServerActorEvent::Initializing {
            server_name: "test".to_string(),
        };
        let json = serde_json::to_string(&e).unwrap();
        let parsed: McpServerActorEvent = serde_json::from_str(&json).unwrap();
        match parsed {
            McpServerActorEvent::Initializing { server_name } => assert_eq!(server_name, "test"),
            _ => panic!("expected Initializing"),
        }
    }

    #[test]
    fn test_mcp_server_actor_event_serde_initialize_error() {
        let e = McpServerActorEvent::InitializeError {
            server_name: "x".to_string(),
            error: "boom".to_string(),
        };
        let json = serde_json::to_string(&e).unwrap();
        let parsed: McpServerActorEvent = serde_json::from_str(&json).unwrap();
        match parsed {
            McpServerActorEvent::InitializeError { server_name, error } => {
                assert_eq!(server_name, "x");
                assert_eq!(error, "boom");
            },
            _ => panic!("expected InitializeError"),
        }
    }

    #[test]
    fn test_mcp_server_actor_event_serde_oauth_request() {
        let e = McpServerActorEvent::OauthRequest {
            server_name: "x".to_string(),
            oauth_url: "https://x.com".to_string(),
        };
        let json = serde_json::to_string(&e).unwrap();
        let _: McpServerActorEvent = serde_json::from_str(&json).unwrap();
    }

    #[test]
    fn test_mcp_server_actor_event_serde_tool_list_changed() {
        let e = McpServerActorEvent::ToolListChanged {
            server_name: "x".to_string(),
        };
        let json = serde_json::to_string(&e).unwrap();
        let _: McpServerActorEvent = serde_json::from_str(&json).unwrap();
    }

    #[test]
    fn test_mcp_server_actor_event_clone() {
        let e = McpServerActorEvent::Initializing {
            server_name: "x".to_string(),
        };
        let _cloned = e.clone();
    }

    #[test]
    fn test_mcp_server_actor_event_initialized_serde() {
        let e = McpServerActorEvent::Initialized {
            server_name: "x".to_string(),
            serve_duration: Duration::from_secs(1),
            list_tools_duration: Some(Duration::from_millis(500)),
            list_prompts_duration: None,
            tool_token_count_estimate: 42,
        };
        let json = serde_json::to_string(&e).unwrap();
        let parsed: McpServerActorEvent = serde_json::from_str(&json).unwrap();
        match parsed {
            McpServerActorEvent::Initialized {
                serve_duration,
                list_tools_duration,
                list_prompts_duration,
                tool_token_count_estimate,
                ..
            } => {
                assert_eq!(serve_duration, Duration::from_secs(1));
                assert_eq!(list_tools_duration, Some(Duration::from_millis(500)));
                assert!(list_prompts_duration.is_none());
                assert_eq!(tool_token_count_estimate, 42);
            },
            _ => panic!("expected Initialized"),
        }
    }

    #[test]
    fn test_mcp_message_debug() {
        let msg = McpMessage::Tools(Ok(vec![]));
        let debug_str = format!("{:?}", msg);
        assert!(debug_str.contains("Tools"));

        let msg2 = McpMessage::Prompts(Err(ServiceError::McpError(rmcp::ErrorData::new(
            rmcp::model::ErrorCode::INTERNAL_ERROR,
            "err",
            None,
        ))));
        let debug_str2 = format!("{:?}", msg2);
        assert!(debug_str2.contains("Prompts"));

        let msg3 = McpMessage::ExecuteTool {
            request_id: 42,
            result: Err(McpServerActorError::Channel),
        };
        let debug_str3 = format!("{:?}", msg3);
        assert!(debug_str3.contains("ExecuteTool"));
        assert!(debug_str3.contains("42"));
    }

    #[test]
    fn test_mcp_server_actor_request_serde() {
        let req = McpServerActorRequest::GetTools;
        let json = serde_json::to_string(&req).unwrap();
        let parsed: McpServerActorRequest = serde_json::from_str(&json).unwrap();
        assert!(matches!(parsed, McpServerActorRequest::GetTools));

        let req2 = McpServerActorRequest::GetPrompts;
        let json2 = serde_json::to_string(&req2).unwrap();
        let parsed2: McpServerActorRequest = serde_json::from_str(&json2).unwrap();
        assert!(matches!(parsed2, McpServerActorRequest::GetPrompts));

        let req3 = McpServerActorRequest::Terminate;
        let json3 = serde_json::to_string(&req3).unwrap();
        let parsed3: McpServerActorRequest = serde_json::from_str(&json3).unwrap();
        assert!(matches!(parsed3, McpServerActorRequest::Terminate));
    }

    #[test]
    fn test_mcp_server_actor_request_get_prompt_serde() {
        let req = McpServerActorRequest::GetPrompt {
            name: "test_prompt".to_string(),
            arguments: HashMap::from([("key".to_string(), "value".to_string())]),
        };
        let json = serde_json::to_string(&req).unwrap();
        let parsed: McpServerActorRequest = serde_json::from_str(&json).unwrap();
        match parsed {
            McpServerActorRequest::GetPrompt { name, arguments } => {
                assert_eq!(name, "test_prompt");
                assert_eq!(arguments.get("key"), Some(&"value".to_string()));
            },
            _ => panic!("expected GetPrompt"),
        }
    }

    #[test]
    fn test_mcp_server_actor_request_execute_tool_serde() {
        let mut args = serde_json::Map::new();
        args.insert("param".to_string(), Value::String("val".to_string()));
        let req = McpServerActorRequest::ExecuteTool {
            name: "my_tool".to_string(),
            args: Some(args),
        };
        let json = serde_json::to_string(&req).unwrap();
        let parsed: McpServerActorRequest = serde_json::from_str(&json).unwrap();
        match parsed {
            McpServerActorRequest::ExecuteTool { name, args } => {
                assert_eq!(name, "my_tool");
                assert!(args.is_some());
                assert_eq!(args.unwrap().get("param").unwrap(), "val");
            },
            _ => panic!("expected ExecuteTool"),
        }
    }

    #[test]
    fn test_mcp_server_actor_request_execute_tool_no_args_serde() {
        let req = McpServerActorRequest::ExecuteTool {
            name: "tool".to_string(),
            args: None,
        };
        let json = serde_json::to_string(&req).unwrap();
        let parsed: McpServerActorRequest = serde_json::from_str(&json).unwrap();
        match parsed {
            McpServerActorRequest::ExecuteTool { name, args } => {
                assert_eq!(name, "tool");
                assert!(args.is_none());
            },
            _ => panic!("expected ExecuteTool"),
        }
    }

    #[test]
    fn test_mcp_server_actor_request_clone() {
        let req = McpServerActorRequest::ExecuteTool {
            name: "t".to_string(),
            args: None,
        };
        let cloned = req.clone();
        match cloned {
            McpServerActorRequest::ExecuteTool { name, .. } => assert_eq!(name, "t"),
            _ => panic!("expected ExecuteTool"),
        }
    }

    #[tokio::test]
    #[ignore = "spawns real subprocess; requires environment with cat available, hangs in coverage"]
    async fn test_mcp_server_actor_handle_terminate() {
        use crate::agent::agent_config::definitions::LocalMcpServerConfig;

        let (event_tx, _event_rx) = mpsc::channel(10);
        let config = McpServerConfig::Local(LocalMcpServerConfig {
            command: "cat".to_string(),
            args: vec![],
            env: None,
            timeout_ms: 5000,
            disabled: false,
            disabled_tools: vec![],
        });
        let handle = McpServerActor::spawn("cat-server".to_string(), config, PathBuf::from("/tmp"), event_tx);
        // Give it a moment to start
        tokio::time::sleep(Duration::from_millis(200)).await;
        handle.shutdown().await;
    }

    #[test]
    #[ignore = "spawns real subprocess via false binary, can hang"]
    fn test_mcp_server_actor_handle_debug() {
        use crate::agent::agent_config::definitions::LocalMcpServerConfig;

        let (event_tx, _event_rx) = mpsc::channel(10);
        let config = McpServerConfig::Local(LocalMcpServerConfig {
            command: "false".to_string(),
            args: vec![],
            env: None,
            timeout_ms: 1000,
            disabled: false,
            disabled_tools: vec![],
        });
        let handle = McpServerActor::spawn("debug-server".to_string(), config, PathBuf::from("/tmp"), event_tx);
        let debug_str = format!("{:?}", handle);
        assert!(debug_str.contains("McpServerActorHandle"));
    }

    // --- Tests that exercise McpServerActorHandle methods without spawning ---

    /// Helper: create a handle backed by a mock responder task.
    fn make_test_handle() -> (
        McpServerActorHandle,
        mpsc::Receiver<
            crate::agent::util::request_channel::Request<
                McpServerActorRequest,
                McpServerActorResponse,
                McpServerActorError,
            >,
        >,
    ) {
        let (tx, rx) = crate::agent::util::request_channel::new_request_channel();
        let handle = McpServerActorHandle {
            _server_name: "test-server".to_string(),
            sender: tx,
            abort_handle: None,
        };
        (handle, rx)
    }

    #[test]
    fn test_handle_debug_without_spawn() {
        let (handle, _rx) = make_test_handle();
        let debug_str = format!("{:?}", handle);
        assert!(debug_str.contains("McpServerActorHandle"));
        assert!(debug_str.contains("test-server"));
    }

    #[tokio::test]
    async fn test_handle_get_tool_specs_success() {
        let (handle, mut rx) = make_test_handle();
        tokio::spawn(async move {
            if let Some(req) = rx.recv().await {
                let tools = vec![ToolSpec {
                    name: "my_tool".to_string(),
                    description: "desc".to_string(),
                    input_schema: serde_json::Map::new(),
                }];
                req.respond(Ok(McpServerActorResponse::Tools(tools))).await;
            }
        });
        let result = handle.get_tool_specs().await.unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].name, "my_tool");
    }

    #[tokio::test]
    async fn test_handle_get_tool_specs_channel_closed() {
        let (handle, rx) = make_test_handle();
        drop(rx);
        let result = handle.get_tool_specs().await;
        assert!(matches!(result, Err(McpServerActorError::Channel)));
    }

    #[tokio::test]
    async fn test_handle_get_tool_specs_unexpected_response() {
        let (handle, mut rx) = make_test_handle();
        tokio::spawn(async move {
            if let Some(req) = rx.recv().await {
                req.respond(Ok(McpServerActorResponse::TerminateAcknowledged)).await;
            }
        });
        let result = handle.get_tool_specs().await;
        match result {
            Err(McpServerActorError::Custom(msg)) => assert!(msg.contains("unexpected response")),
            _ => panic!("expected Custom error"),
        }
    }

    #[tokio::test]
    async fn test_handle_get_tool_specs_service_error() {
        let (handle, mut rx) = make_test_handle();
        tokio::spawn(async move {
            if let Some(req) = rx.recv().await {
                req.respond(Err(McpServerActorError::Service {
                    message: "service down".to_string(),
                    source: None,
                }))
                .await;
            }
        });
        let result = handle.get_tool_specs().await;
        match result {
            Err(McpServerActorError::Service { message, .. }) => assert!(message.contains("service down")),
            _ => panic!("expected Service error"),
        }
    }

    #[tokio::test]
    async fn test_handle_get_prompts_success() {
        let (handle, mut rx) = make_test_handle();
        tokio::spawn(async move {
            if let Some(req) = rx.recv().await {
                let prompts = vec![Prompt {
                    name: "p1".to_string(),
                    description: Some("desc".to_string()),
                    arguments: None,
                }];
                req.respond(Ok(McpServerActorResponse::Prompts(prompts))).await;
            }
        });
        let result = handle.get_prompts().await.unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].name, "p1");
    }

    #[tokio::test]
    async fn test_handle_get_prompts_channel_closed() {
        let (handle, rx) = make_test_handle();
        drop(rx);
        let result = handle.get_prompts().await;
        assert!(matches!(result, Err(McpServerActorError::Channel)));
    }

    #[tokio::test]
    async fn test_handle_get_prompts_unexpected_response() {
        let (handle, mut rx) = make_test_handle();
        tokio::spawn(async move {
            if let Some(req) = rx.recv().await {
                req.respond(Ok(McpServerActorResponse::TerminateAcknowledged)).await;
            }
        });
        let result = handle.get_prompts().await;
        match result {
            Err(McpServerActorError::Custom(msg)) => assert!(msg.contains("unexpected response")),
            _ => panic!("expected Custom error"),
        }
    }

    #[tokio::test]
    async fn test_handle_get_prompt_success() {
        let (handle, mut rx) = make_test_handle();
        tokio::spawn(async move {
            if let Some(req) = rx.recv().await {
                let messages = vec![serde_json::json!({"role": "user", "content": "hello"})];
                req.respond(Ok(McpServerActorResponse::Prompt(messages))).await;
            }
        });
        let result = handle.get_prompt("test".to_string(), HashMap::new()).await.unwrap();
        assert_eq!(result.len(), 1);
    }

    #[tokio::test]
    async fn test_handle_get_prompt_channel_closed() {
        let (handle, rx) = make_test_handle();
        drop(rx);
        let result = handle.get_prompt("test".to_string(), HashMap::new()).await;
        assert!(matches!(result, Err(McpServerActorError::Channel)));
    }

    #[tokio::test]
    async fn test_handle_get_prompt_unexpected_response() {
        let (handle, mut rx) = make_test_handle();
        tokio::spawn(async move {
            if let Some(req) = rx.recv().await {
                req.respond(Ok(McpServerActorResponse::Tools(vec![]))).await;
            }
        });
        let result = handle.get_prompt("test".to_string(), HashMap::new()).await;
        match result {
            Err(McpServerActorError::Custom(msg)) => assert!(msg.contains("unexpected response")),
            _ => panic!("expected Custom error"),
        }
    }

    #[tokio::test]
    async fn test_handle_execute_tool_success() {
        let (handle, mut rx) = make_test_handle();
        tokio::spawn(async move {
            if let Some(req) = rx.recv().await {
                let (tx, rx_inner) = oneshot::channel();
                req.respond(Ok(McpServerActorResponse::ExecuteTool(rx_inner))).await;
                let _ = tx.send(Err(McpServerActorError::Channel));
            }
        });
        let rx = handle.execute_tool("tool".to_string(), None).await.unwrap();
        let result = rx.await.unwrap();
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_handle_execute_tool_channel_closed() {
        let (handle, rx) = make_test_handle();
        drop(rx);
        let result = handle.execute_tool("tool".to_string(), None).await;
        assert!(matches!(result, Err(McpServerActorError::Channel)));
    }

    #[tokio::test]
    async fn test_handle_execute_tool_unexpected_response() {
        let (handle, mut rx) = make_test_handle();
        tokio::spawn(async move {
            if let Some(req) = rx.recv().await {
                req.respond(Ok(McpServerActorResponse::TerminateAcknowledged)).await;
            }
        });
        let result = handle.execute_tool("tool".to_string(), None).await;
        match result {
            Err(McpServerActorError::Custom(msg)) => assert!(msg.contains("unexpected response")),
            _ => panic!("expected Custom error"),
        }
    }

    #[tokio::test]
    async fn test_handle_shutdown_channel_closed() {
        let (handle, rx) = make_test_handle();
        drop(rx);
        handle.shutdown().await;
    }

    #[tokio::test]
    async fn test_handle_shutdown_success() {
        let (handle, mut rx) = make_test_handle();
        tokio::spawn(async move {
            if let Some(req) = rx.recv().await {
                req.respond(Ok(McpServerActorResponse::TerminateAcknowledged)).await;
            }
        });
        handle.shutdown().await;
    }

    #[test]
    fn test_handle_terminate_channel_closed() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let (handle, rx) = make_test_handle();
            drop(rx);
            handle.terminate();
        });
    }

    #[test]
    fn test_mcp_server_actor_response_debug() {
        let resp = McpServerActorResponse::Tools(vec![]);
        assert!(format!("{:?}", resp).contains("Tools"));

        let resp = McpServerActorResponse::Prompts(vec![]);
        assert!(format!("{:?}", resp).contains("Prompts"));

        let resp = McpServerActorResponse::Prompt(vec![serde_json::json!("hi")]);
        assert!(format!("{:?}", resp).contains("Prompt"));

        let resp = McpServerActorResponse::TerminateAcknowledged;
        assert!(format!("{:?}", resp).contains("TerminateAcknowledged"));

        let (_tx, rx) = oneshot::channel::<ExecuteToolResult>();
        let resp = McpServerActorResponse::ExecuteTool(rx);
        assert!(format!("{:?}", resp).contains("ExecuteTool"));
    }

    #[test]
    fn test_mcp_server_actor_error_source_skipped_in_serde() {
        let se = ServiceError::McpError(rmcp::ErrorData::new(
            rmcp::model::ErrorCode::INTERNAL_ERROR,
            "serde test",
            None,
        ));
        let e = McpServerActorError::from(se);
        let json = serde_json::to_string(&e).unwrap();
        assert!(!json.contains("source"));
        let parsed: McpServerActorError = serde_json::from_str(&json).unwrap();
        match parsed {
            McpServerActorError::Service { message, source } => {
                assert!(message.contains("serde test"));
                assert!(source.is_none());
            },
            _ => panic!("expected Service"),
        }
    }

    #[test]
    fn test_mcp_server_actor_error_debug() {
        let e = McpServerActorError::Service {
            message: "test".to_string(),
            source: None,
        };
        assert!(format!("{:?}", e).contains("Service"));

        let e2 = McpServerActorError::Channel;
        assert!(format!("{:?}", e2).contains("Channel"));

        let e3 = McpServerActorError::Custom("custom msg".to_string());
        assert!(format!("{:?}", e3).contains("custom msg"));
    }

    #[test]
    fn test_mcp_server_actor_event_debug_all() {
        let events: Vec<McpServerActorEvent> = vec![
            McpServerActorEvent::Initializing {
                server_name: "s".to_string(),
            },
            McpServerActorEvent::Initialized {
                server_name: "s".to_string(),
                serve_duration: Duration::from_secs(1),
                list_tools_duration: None,
                list_prompts_duration: Some(Duration::from_millis(50)),
                tool_token_count_estimate: 0,
            },
            McpServerActorEvent::InitializeError {
                server_name: "s".to_string(),
                error: "err".to_string(),
            },
            McpServerActorEvent::OauthRequest {
                server_name: "s".to_string(),
                oauth_url: "url".to_string(),
            },
            McpServerActorEvent::ToolListChanged {
                server_name: "s".to_string(),
            },
        ];
        for e in &events {
            let _ = format!("{:?}", e);
        }
    }

    #[test]
    fn test_mcp_server_actor_request_debug_all() {
        let reqs: Vec<McpServerActorRequest> = vec![
            McpServerActorRequest::GetTools,
            McpServerActorRequest::GetPrompts,
            McpServerActorRequest::GetPrompt {
                name: "p".to_string(),
                arguments: HashMap::new(),
            },
            McpServerActorRequest::ExecuteTool {
                name: "t".to_string(),
                args: None,
            },
            McpServerActorRequest::Terminate,
        ];
        for r in &reqs {
            let _ = format!("{:?}", r);
        }
    }

    #[test]
    fn test_mcp_server_actor_event_clone_all_variants() {
        let events = vec![
            McpServerActorEvent::Initializing {
                server_name: "a".to_string(),
            },
            McpServerActorEvent::Initialized {
                server_name: "b".to_string(),
                serve_duration: Duration::from_millis(100),
                list_tools_duration: Some(Duration::from_millis(50)),
                list_prompts_duration: Some(Duration::from_millis(25)),
                tool_token_count_estimate: 0,
            },
            McpServerActorEvent::InitializeError {
                server_name: "c".to_string(),
                error: "e".to_string(),
            },
            McpServerActorEvent::OauthRequest {
                server_name: "d".to_string(),
                oauth_url: "u".to_string(),
            },
            McpServerActorEvent::ToolListChanged {
                server_name: "e".to_string(),
            },
        ];
        for e in events {
            let _ = e.clone();
        }
    }

    #[test]
    fn test_mcp_server_actor_event_serde_initialized_all_durations() {
        let e = McpServerActorEvent::Initialized {
            server_name: "full".to_string(),
            serve_duration: Duration::from_secs(2),
            list_tools_duration: Some(Duration::from_millis(100)),
            list_prompts_duration: Some(Duration::from_millis(200)),
            tool_token_count_estimate: 42,
        };
        let json = serde_json::to_string(&e).unwrap();
        let parsed: McpServerActorEvent = serde_json::from_str(&json).unwrap();
        match parsed {
            McpServerActorEvent::Initialized {
                list_tools_duration,
                list_prompts_duration,
                tool_token_count_estimate,
                ..
            } => {
                assert_eq!(list_tools_duration, Some(Duration::from_millis(100)));
                assert_eq!(list_prompts_duration, Some(Duration::from_millis(200)));
                assert_eq!(tool_token_count_estimate, 42);
            },
            _ => panic!("expected Initialized"),
        }
    }

    #[test]
    fn test_mcp_server_actor_request_get_prompt_empty_args_serde() {
        let req = McpServerActorRequest::GetPrompt {
            name: "empty".to_string(),
            arguments: HashMap::new(),
        };
        let json = serde_json::to_string(&req).unwrap();
        let parsed: McpServerActorRequest = serde_json::from_str(&json).unwrap();
        match parsed {
            McpServerActorRequest::GetPrompt { name, arguments } => {
                assert_eq!(name, "empty");
                assert!(arguments.is_empty());
            },
            _ => panic!("expected GetPrompt"),
        }
    }

    #[test]
    fn test_mcp_message_execute_tool_ok_variant() {
        use rmcp::model::CallToolResult;
        let result: ExecuteToolResult = Ok(CallToolResult::success(vec![]));
        let msg = McpMessage::ExecuteTool { request_id: 1, result };
        assert!(format!("{:?}", msg).contains("ExecuteTool"));
    }

    #[test]
    fn test_mcp_server_actor_error_service_with_source_clone() {
        let se = ServiceError::McpError(rmcp::ErrorData::new(
            rmcp::model::ErrorCode::INTERNAL_ERROR,
            "clone test",
            None,
        ));
        let e = McpServerActorError::from(se);
        let cloned = e.clone();
        match cloned {
            McpServerActorError::Service { message, source } => {
                assert!(message.contains("clone test"));
                assert!(source.is_some());
            },
            _ => panic!("expected Service"),
        }
    }

    #[test]
    fn test_mcp_server_actor_error_source() {
        use std::error::Error;
        let se = ServiceError::McpError(rmcp::ErrorData::new(
            rmcp::model::ErrorCode::INTERNAL_ERROR,
            "source test",
            None,
        ));
        let e = McpServerActorError::from(se);
        assert!(e.source().is_some());

        let e2 = McpServerActorError::Channel;
        assert!(e2.source().is_none());
        let e3 = McpServerActorError::Custom("x".to_string());
        assert!(e3.source().is_none());
    }

    #[test]
    fn test_mcp_server_actor_error_from_transport_closed() {
        let se = ServiceError::TransportClosed;
        let e: McpServerActorError = se.into();
        match e {
            McpServerActorError::Service { message, source } => {
                assert!(message.contains("Transport closed"));
                assert!(source.is_some());
            },
            _ => panic!("expected Service variant"),
        }
    }

    // --- Tests for handle_mcp_message and handle_actor_request using a real actor ---

    /// Helper to construct a test McpServerActor with a closed-transport service handle.
    fn make_test_actor() -> (McpServerActor, mpsc::Receiver<McpServerActorEvent>) {
        let (req_tx, req_rx) = new_request_channel();
        let _ = req_tx; // keep sender alive implicitly via actor's req_rx
        let (event_tx, event_rx) = mpsc::channel(32);
        let (message_tx, message_rx) = mpsc::channel(32);
        let service_handle = RunningMcpService::new_closed_for_test();
        let actor = McpServerActor {
            server_name: "test-actor".to_string(),
            _config: McpServerConfig::Local(crate::agent::agent_config::definitions::LocalMcpServerConfig {
                command: "test".to_string(),
                args: vec![],
                env: None,
                timeout_ms: 1000,
                disabled: false,
                disabled_tools: vec![],
            }),
            tools: vec![ToolSpec {
                name: "existing_tool".to_string(),
                description: "desc".to_string(),
                input_schema: serde_json::Map::new(),
            }],
            prompts: vec![Prompt {
                name: "existing_prompt".to_string(),
                description: Some("desc".to_string()),
                arguments: None,
            }],
            service_handle,
            curr_tool_execution_id: 0,
            executing_tools: HashMap::new(),
            tool_annotations: HashMap::new(),
            req_rx,
            event_tx,
            message_tx,
            message_rx,
        };
        (actor, event_rx)
    }

    #[tokio::test]
    async fn test_handle_actor_request_get_tools() {
        let (mut actor, _event_rx) = make_test_actor();
        let res = actor.handle_actor_request(McpServerActorRequest::GetTools).await;
        match res.unwrap() {
            McpServerActorResponse::Tools(tools) => {
                assert_eq!(tools.len(), 1);
                assert_eq!(tools[0].name, "existing_tool");
            },
            _ => panic!("expected Tools response"),
        }
    }

    #[tokio::test]
    async fn test_handle_actor_request_get_prompts() {
        let (mut actor, _event_rx) = make_test_actor();
        let res = actor.handle_actor_request(McpServerActorRequest::GetPrompts).await;
        match res.unwrap() {
            McpServerActorResponse::Prompts(prompts) => {
                assert_eq!(prompts.len(), 1);
                assert_eq!(prompts[0].name, "existing_prompt");
            },
            _ => panic!("expected Prompts response"),
        }
    }

    #[tokio::test]
    async fn test_handle_actor_request_terminate() {
        let (mut actor, _event_rx) = make_test_actor();
        let res = actor.handle_actor_request(McpServerActorRequest::Terminate).await;
        assert!(matches!(res.unwrap(), McpServerActorResponse::TerminateAcknowledged));
    }

    #[tokio::test]
    async fn test_handle_actor_request_execute_tool_transport_closed() {
        let (mut actor, _event_rx) = make_test_actor();
        // Give the serve loop time to detect cancellation and close
        for _ in 0..10 {
            tokio::task::yield_now().await;
        }
        let res = actor
            .handle_actor_request(McpServerActorRequest::ExecuteTool {
                name: "tool".to_string(),
                args: None,
            })
            .await;
        match res {
            Err(McpServerActorError::Custom(msg)) => {
                assert!(msg.contains("Transport to MCP server"));
                assert!(msg.contains("closed"));
            },
            _ => panic!("expected Custom error about transport closed, got: {res:?}"),
        }
    }

    #[tokio::test]
    async fn test_handle_actor_request_get_prompt_transport_closed() {
        let (mut actor, _event_rx) = make_test_actor();
        for _ in 0..10 {
            tokio::task::yield_now().await;
        }
        let res = actor
            .handle_actor_request(McpServerActorRequest::GetPrompt {
                name: "p".to_string(),
                arguments: HashMap::new(),
            })
            .await;
        match res {
            Err(McpServerActorError::Custom(msg)) => {
                assert!(msg.contains("Transport to MCP server"));
                assert!(msg.contains("closed"));
            },
            _ => panic!("expected Custom error about transport closed"),
        }
    }

    #[tokio::test]
    async fn test_handle_actor_request_execute_tool_transport_closed_remote_config() {
        let (req_tx, req_rx) = new_request_channel();
        let _ = req_tx;
        let (event_tx, _event_rx) = mpsc::channel(32);
        let (message_tx, message_rx) = mpsc::channel(32);
        let service_handle = RunningMcpService::new_closed_for_test();
        let mut actor = McpServerActor {
            server_name: "remote-actor".to_string(),
            _config: McpServerConfig::Remote(crate::agent::agent_config::definitions::RemoteMcpServerConfig {
                url: "https://example.com".to_string(),
                headers: HashMap::new(),
                timeout_ms: 30000,
                disabled: false,
                disabled_tools: vec![],
                oauth_scopes: vec![],
                oauth: None,
                force_auth: false,
            }),
            tools: vec![],
            prompts: vec![],
            service_handle,
            curr_tool_execution_id: 0,
            executing_tools: HashMap::new(),
            tool_annotations: HashMap::new(),
            req_rx,
            event_tx,
            message_tx,
            message_rx,
        };
        for _ in 0..10 {
            tokio::task::yield_now().await;
        }
        let res = actor
            .handle_actor_request(McpServerActorRequest::ExecuteTool {
                name: "tool".to_string(),
                args: None,
            })
            .await;
        match res {
            Err(McpServerActorError::Custom(msg)) => {
                assert!(msg.contains("terminated or the connection was lost"));
            },
            _ => panic!("expected Custom error for remote transport closed"),
        }
    }

    #[tokio::test]
    async fn test_handle_actor_request_get_prompt_transport_closed_remote_config() {
        let (req_tx, req_rx) = new_request_channel();
        let _ = req_tx;
        let (event_tx, _event_rx) = mpsc::channel(32);
        let (message_tx, message_rx) = mpsc::channel(32);
        let service_handle = RunningMcpService::new_closed_for_test();
        let mut actor = McpServerActor {
            server_name: "remote-actor".to_string(),
            _config: McpServerConfig::Remote(crate::agent::agent_config::definitions::RemoteMcpServerConfig {
                url: "https://example.com".to_string(),
                headers: HashMap::new(),
                timeout_ms: 30000,
                disabled: false,
                disabled_tools: vec![],
                oauth_scopes: vec![],
                oauth: None,
                force_auth: false,
            }),
            tools: vec![],
            prompts: vec![],
            service_handle,
            curr_tool_execution_id: 0,
            executing_tools: HashMap::new(),
            tool_annotations: HashMap::new(),
            req_rx,
            event_tx,
            message_tx,
            message_rx,
        };
        for _ in 0..10 {
            tokio::task::yield_now().await;
        }
        let res = actor
            .handle_actor_request(McpServerActorRequest::GetPrompt {
                name: "p".to_string(),
                arguments: HashMap::new(),
            })
            .await;
        match res {
            Err(McpServerActorError::Custom(msg)) => {
                assert!(msg.contains("terminated or the connection was lost"));
            },
            _ => panic!("expected Custom error for remote transport closed"),
        }
    }

    #[tokio::test]
    async fn test_handle_mcp_message_none() {
        let (mut actor, _event_rx) = make_test_actor();
        // None message means channel closed - should just return
        actor.handle_mcp_message(None).await;
    }

    #[tokio::test]
    async fn test_handle_mcp_message_tools_ok() {
        let (mut actor, mut event_rx) = make_test_actor();
        let tools = vec![RmcpTool::new("new_tool", "new desc", Arc::new(serde_json::Map::new()))];
        actor.handle_mcp_message(Some(McpMessage::Tools(Ok(tools)))).await;
        assert_eq!(actor.tools.len(), 1);
        assert_eq!(actor.tools[0].name, "new_tool");
        // Should have sent ToolListChanged event
        let event = event_rx.recv().await.unwrap();
        assert!(matches!(event, McpServerActorEvent::ToolListChanged { .. }));
    }

    #[tokio::test]
    async fn test_handle_mcp_message_tools_err() {
        let (mut actor, _event_rx) = make_test_actor();
        let err = ServiceError::McpError(rmcp::ErrorData::new(
            rmcp::model::ErrorCode::INTERNAL_ERROR,
            "list tools failed",
            None,
        ));
        // Should not panic, just log error
        actor.handle_mcp_message(Some(McpMessage::Tools(Err(err)))).await;
        // Tools should remain unchanged
        assert_eq!(actor.tools.len(), 1);
        assert_eq!(actor.tools[0].name, "existing_tool");
    }

    #[tokio::test]
    async fn test_handle_mcp_message_prompts_ok() {
        let (mut actor, _event_rx) = make_test_actor();
        let prompts = vec![RmcpPrompt::new("new_prompt", Some("new desc"), None)];
        actor.handle_mcp_message(Some(McpMessage::Prompts(Ok(prompts)))).await;
        assert_eq!(actor.prompts.len(), 1);
        assert_eq!(actor.prompts[0].name, "new_prompt");
    }

    #[tokio::test]
    async fn test_handle_mcp_message_prompts_err() {
        let (mut actor, _event_rx) = make_test_actor();
        let err = ServiceError::McpError(rmcp::ErrorData::new(
            rmcp::model::ErrorCode::INTERNAL_ERROR,
            "list prompts failed",
            None,
        ));
        actor.handle_mcp_message(Some(McpMessage::Prompts(Err(err)))).await;
        // Prompts should remain unchanged
        assert_eq!(actor.prompts.len(), 1);
        assert_eq!(actor.prompts[0].name, "existing_prompt");
    }

    #[tokio::test]
    async fn test_handle_mcp_message_execute_tool_found() {
        let (mut actor, _event_rx) = make_test_actor();
        let (tx, rx) = oneshot::channel();
        actor.executing_tools.insert(42, tx);
        let result: ExecuteToolResult = Ok(rmcp::model::CallToolResult::success(vec![]));
        actor
            .handle_mcp_message(Some(McpMessage::ExecuteTool { request_id: 42, result }))
            .await;
        // The oneshot should have received the result
        let received = rx.await.unwrap();
        assert!(received.is_ok());
        assert!(actor.executing_tools.is_empty());
    }

    #[tokio::test]
    async fn test_handle_mcp_message_execute_tool_not_found() {
        let (mut actor, _event_rx) = make_test_actor();
        // No matching request_id in executing_tools
        let result: ExecuteToolResult = Err(McpServerActorError::Channel);
        actor
            .handle_mcp_message(Some(McpMessage::ExecuteTool {
                request_id: 999,
                result,
            }))
            .await;
        // Should just warn and not panic
    }

    #[tokio::test]
    async fn test_handle_actor_request_execute_tool_transport_closed_registry_config() {
        let (req_tx, req_rx) = new_request_channel();
        let _ = req_tx;
        let (event_tx, _event_rx) = mpsc::channel(32);
        let (message_tx, message_rx) = mpsc::channel(32);
        let service_handle = RunningMcpService::new_closed_for_test();
        let mut actor = McpServerActor {
            server_name: "registry-actor".to_string(),
            _config: McpServerConfig::Registry(crate::agent::agent_config::definitions::RegistryMcpServerConfig {
                server_type: "registry".to_string(),
                env: None,
                headers: None,
                timeout: None,
                oauth_scopes: vec![],
                oauth: None,
            }),
            tools: vec![],
            prompts: vec![],
            service_handle,
            curr_tool_execution_id: 0,
            executing_tools: HashMap::new(),
            tool_annotations: HashMap::new(),
            req_rx,
            event_tx,
            message_tx,
            message_rx,
        };
        for _ in 0..10 {
            tokio::task::yield_now().await;
        }
        let res = actor
            .handle_actor_request(McpServerActorRequest::ExecuteTool {
                name: "tool".to_string(),
                args: None,
            })
            .await;
        match res {
            Err(McpServerActorError::Custom(msg)) => {
                assert!(msg.contains("terminated or the connection was lost"));
            },
            _ => panic!("expected Custom error for registry transport closed"),
        }
    }

    #[tokio::test]
    async fn test_handle_actor_request_get_prompt_transport_closed_registry_config() {
        let (req_tx, req_rx) = new_request_channel();
        let _ = req_tx;
        let (event_tx, _event_rx) = mpsc::channel(32);
        let (message_tx, message_rx) = mpsc::channel(32);
        let service_handle = RunningMcpService::new_closed_for_test();
        let mut actor = McpServerActor {
            server_name: "registry-actor".to_string(),
            _config: McpServerConfig::Registry(crate::agent::agent_config::definitions::RegistryMcpServerConfig {
                server_type: "registry".to_string(),
                env: None,
                headers: None,
                timeout: None,
                oauth_scopes: vec![],
                oauth: None,
            }),
            tools: vec![],
            prompts: vec![],
            service_handle,
            curr_tool_execution_id: 0,
            executing_tools: HashMap::new(),
            tool_annotations: HashMap::new(),
            req_rx,
            event_tx,
            message_tx,
            message_rx,
        };
        for _ in 0..10 {
            tokio::task::yield_now().await;
        }
        let res = actor
            .handle_actor_request(McpServerActorRequest::GetPrompt {
                name: "p".to_string(),
                arguments: HashMap::new(),
            })
            .await;
        match res {
            Err(McpServerActorError::Custom(msg)) => {
                assert!(msg.contains("terminated or the connection was lost"));
            },
            _ => panic!("expected Custom error for registry transport closed"),
        }
    }

    #[tokio::test]
    async fn test_main_loop_request_channel_closed() {
        // When req_rx channel closes, main_loop should exit
        let (req_tx, req_rx) = new_request_channel();
        let (event_tx, _event_rx) = mpsc::channel(32);
        let (message_tx, message_rx) = mpsc::channel(32);
        let service_handle = RunningMcpService::new_closed_for_test();
        let actor = McpServerActor {
            server_name: "loop-test".to_string(),
            _config: McpServerConfig::Local(crate::agent::agent_config::definitions::LocalMcpServerConfig {
                command: "test".to_string(),
                args: vec![],
                env: None,
                timeout_ms: 1000,
                disabled: false,
                disabled_tools: vec![],
            }),
            tools: vec![],
            prompts: vec![],
            service_handle,
            curr_tool_execution_id: 0,
            executing_tools: HashMap::new(),
            tool_annotations: HashMap::new(),
            req_rx,
            event_tx,
            message_tx,
            message_rx,
        };
        // Drop the sender to close the channel
        drop(req_tx);
        // main_loop should exit immediately since channel is closed
        actor.main_loop().await;
    }

    #[tokio::test]
    async fn test_main_loop_terminate_request() {
        let (req_tx, req_rx) = new_request_channel();
        let (event_tx, _event_rx) = mpsc::channel(32);
        let (message_tx, message_rx) = mpsc::channel(32);
        let service_handle = RunningMcpService::new_closed_for_test();
        let actor = McpServerActor {
            server_name: "loop-terminate".to_string(),
            _config: McpServerConfig::Local(crate::agent::agent_config::definitions::LocalMcpServerConfig {
                command: "test".to_string(),
                args: vec![],
                env: None,
                timeout_ms: 1000,
                disabled: false,
                disabled_tools: vec![],
            }),
            tools: vec![],
            prompts: vec![],
            service_handle,
            curr_tool_execution_id: 0,
            executing_tools: HashMap::new(),
            tool_annotations: HashMap::new(),
            req_rx,
            event_tx,
            message_tx,
            message_rx,
        };
        // Send terminate request then run main_loop
        tokio::spawn(async move {
            let res = req_tx.send_recv(McpServerActorRequest::Terminate).await;
            assert!(res.is_some());
            match res.unwrap() {
                Ok(McpServerActorResponse::TerminateAcknowledged) => {},
                other => panic!("expected TerminateAcknowledged, got {other:?}"),
            }
        });
        actor.main_loop().await;
    }

    #[tokio::test]
    async fn test_main_loop_handles_message_from_channel() {
        let (req_tx, req_rx) = new_request_channel();
        let (event_tx, mut event_rx) = mpsc::channel(32);
        let (message_tx, message_rx) = mpsc::channel(32);
        let service_handle = RunningMcpService::new_closed_for_test();
        let actor = McpServerActor {
            server_name: "loop-msg".to_string(),
            _config: McpServerConfig::Local(crate::agent::agent_config::definitions::LocalMcpServerConfig {
                command: "test".to_string(),
                args: vec![],
                env: None,
                timeout_ms: 1000,
                disabled: false,
                disabled_tools: vec![],
            }),
            tools: vec![],
            prompts: vec![],
            service_handle,
            curr_tool_execution_id: 0,
            executing_tools: HashMap::new(),
            tool_annotations: HashMap::new(),
            req_rx,
            event_tx,
            message_tx: message_tx.clone(),
            message_rx,
        };
        // Send a tools message then terminate
        let req_tx_clone = req_tx.clone();
        tokio::spawn(async move {
            // Send a tools update message
            message_tx
                .send(McpMessage::Tools(Ok(vec![RmcpTool::new(
                    "dynamic_tool",
                    "dynamic",
                    Arc::new(serde_json::Map::new()),
                )])))
                .await
                .unwrap();
            // Give actor time to process the message
            tokio::time::sleep(Duration::from_millis(50)).await;
            // Then terminate
            let _ = req_tx_clone.send_recv(McpServerActorRequest::Terminate).await;
        });
        actor.main_loop().await;
        // Should have received ToolListChanged event
        let event = event_rx.recv().await.unwrap();
        assert!(matches!(event, McpServerActorEvent::ToolListChanged { .. }));
        drop(req_tx);
    }

    #[tokio::test]
    async fn test_main_loop_get_tools_request() {
        let (req_tx, req_rx) = new_request_channel();
        let (event_tx, _event_rx) = mpsc::channel(32);
        let (message_tx, message_rx) = mpsc::channel(32);
        let service_handle = RunningMcpService::new_closed_for_test();
        let actor = McpServerActor {
            server_name: "loop-get-tools".to_string(),
            _config: McpServerConfig::Local(crate::agent::agent_config::definitions::LocalMcpServerConfig {
                command: "test".to_string(),
                args: vec![],
                env: None,
                timeout_ms: 1000,
                disabled: false,
                disabled_tools: vec![],
            }),
            tools: vec![ToolSpec {
                name: "t1".to_string(),
                description: "d".to_string(),
                input_schema: serde_json::Map::new(),
            }],
            prompts: vec![],
            service_handle,
            curr_tool_execution_id: 0,
            executing_tools: HashMap::new(),
            tool_annotations: HashMap::new(),
            req_rx,
            event_tx,
            message_tx,
            message_rx,
        };
        tokio::spawn(async move {
            let res = req_tx.send_recv(McpServerActorRequest::GetTools).await;
            match res.unwrap().unwrap() {
                McpServerActorResponse::Tools(tools) => assert_eq!(tools[0].name, "t1"),
                _ => panic!("expected Tools"),
            }
            let _ = req_tx.send_recv(McpServerActorRequest::Terminate).await;
        });
        actor.main_loop().await;
    }
}
