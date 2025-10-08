mod service;

use std::collections::HashMap;
use std::process::Stdio;

use futures::stream::FuturesUnordered;
use rmcp::model::{
    CallToolRequestParam,
    CallToolResult,
    ClientInfo,
    ClientResult,
    Implementation,
    LoggingLevel,
    Prompt as RmcpPrompt,
    PromptArgument as RmcpPromptArgument,
    ServerNotification,
    ServerRequest,
    Tool as RmcpTool,
};
use rmcp::transport::{
    ConfigureCommandExt as _,
    TokioChildProcess,
};
use rmcp::{
    RoleClient,
    ServiceError,
    ServiceExt,
};
use serde::{
    Deserialize,
    Serialize,
};
use tokio::io::AsyncReadExt as _;
use tokio::process::{
    ChildStderr,
    Command,
};
use tokio::sync::{
    mpsc,
    oneshot,
};
use tokio_stream::StreamExt as _;
use tracing::{
    debug,
    error,
    info,
    trace,
    warn,
};

use super::agent_config::parse::CanonicalToolName;
use super::agent_loop::types::ToolSpec;
use super::util::request_channel::{
    RequestReceiver,
    new_request_channel,
};
// use crate::chat::EventSender;
use crate::agent::agent_config::AgentConfig;
use crate::agent::agent_config::definitions::{
    LocalMcpServerConfig,
    McpServerConfig,
};
use crate::agent::util::expand_env_vars;
use crate::agent::util::path::expand_path;
use crate::agent::util::request_channel::{
    RequestSender,
    respond,
};

enum McpClient {
    Pending,
    Ready,
}

#[derive(Debug)]
struct McpServerActorHandle {
    server_name: String,
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
                "received unexpected response: {:?}",
                other
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
                "received unexpected response: {:?}",
                other
            ))),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum McpServerActorRequest {
    GetTools,
    GetPrompts,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
enum McpServerActorResponse {
    Tools(Vec<ToolSpec>),
    Prompts(Vec<Prompt>),
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize, thiserror::Error)]
enum McpServerActorError {
    #[error("The channel has closed")]
    Channel,
    #[error("{}", .0)]
    Custom(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum McpServerActorEvent {
    Initialized,
    /// The MCP server failed to initialize successfully
    InitializeError(String),
}

#[derive(Debug)]
struct McpServerActor {
    /// Name of the MCP server
    server_name: String,
    /// Config the server was launched with
    config: McpServerConfig,
    /// Tools
    tools: Vec<ToolSpec>,
    /// Prompts
    prompts: Vec<Prompt>,
    /// Handle to an MCP server
    service_handle: RunningMcpService,

    req_rx: RequestReceiver<McpServerActorRequest, McpServerActorResponse, McpServerActorError>,
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
            server_name,
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
            Ok(service_handle) => {
                let s = Self {
                    server_name,
                    config,
                    tools: vec![],
                    prompts: vec![],
                    service_handle,
                    req_rx,
                    event_tx,
                    message_tx,
                    message_rx,
                };
                let _ = s.event_tx.send(McpServerActorEvent::Initialized).await;
                s.refresh_tools();
                s.refresh_prompts();
                s.main_loop().await;
            },
            Err(err) => {
                // todo - how to handle error here?
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
        debug!(?req, "MCP actor received new request");
        match req {
            McpServerActorRequest::GetTools => Ok(McpServerActorResponse::Tools(self.tools.clone())),
            McpServerActorRequest::GetPrompts => Ok(McpServerActorResponse::Prompts(self.prompts.clone())),
        }
    }

    async fn handle_mcp_message(&mut self, msg: Option<McpMessage>) {
        let Some(msg) = msg else {
            warn!("MCP message receiver has closed");
            return;
        };
        match msg {
            McpMessage::ToolsResult(res) => match res {
                Ok(tools) => self.tools = tools.into_iter().map(Into::into).collect(),
                Err(err) => {
                    error!(?err, "failed to list tools");
                },
            },
            McpMessage::PromptsResult(res) => match res {
                Ok(prompts) => self.prompts = prompts.into_iter().map(Into::into).collect(),
                Err(err) => {
                    error!(?err, "failed to list prompts");
                },
            },
        }
    }

    /// Asynchronously fetch all tools
    fn refresh_tools(&self) {
        let service_handle = self.service_handle.clone();
        let tx = self.message_tx.clone();
        tokio::spawn(async move {
            let res = service_handle.list_tools().await;
            let _ = tx.send(McpMessage::ToolsResult(res)).await;
        });
    }

    /// Asynchronously fetch all prompts
    fn refresh_prompts(&self) {
        let service_handle = self.service_handle.clone();
        let tx = self.message_tx.clone();
        tokio::spawn(async move {
            let res = service_handle.list_prompts().await;
            let _ = tx.send(McpMessage::PromptsResult(res)).await;
        });
    }
}

/// Represents a message from an MCP server to the client.
#[derive(Debug)]
enum McpMessage {
    ToolsResult(Result<Vec<RmcpTool>, ServiceError>),
    PromptsResult(Result<Vec<RmcpPrompt>, ServiceError>),
}

/// Represents a handle to a running MCP server.
#[derive(Debug, Clone)]
struct RunningMcpService {
    /// Handle to an rmcp MCP server from which we can send client requests (list tools, list
    /// prompts, etc.)
    ///
    /// TODO - maybe replace RunningMcpService with just InnerService? Probably not, once OAuth is
    /// implemented since that may require holding an auth guard.
    running_service: InnerService,
}

impl RunningMcpService {
    fn new(
        server_name: String,
        running_service: rmcp::service::RunningService<RoleClient, McpService>,
        child_stderr: Option<ChildStderr>,
    ) -> Self {
        // We need to read from the child process stderr - otherwise, ?? will happen
        if let Some(mut stderr) = child_stderr {
            let server_name_clone = server_name.clone();
            tokio::spawn(async move {
                let mut buf = [0u8; 1024];
                loop {
                    match stderr.read(&mut buf).await {
                        Ok(0) => {
                            info!(target: "mcp", "{server_name_clone} stderr listening process exited due to EOF");
                            break;
                        },
                        Ok(size) => {
                            info!(target: "mcp", "{server_name_clone} logged to its stderr: {}", String::from_utf8_lossy(&buf[0..size]));
                        },
                        Err(e) => {
                            info!(target: "mcp", "{server_name_clone} stderr listening process exited due to error: {e}");
                            break; // Error reading
                        },
                    }
                }
            });
        }

        Self {
            running_service: InnerService::Original(running_service),
        }
    }

    async fn call_tool(&self, param: CallToolRequestParam) -> Result<CallToolResult, ServiceError> {
        self.running_service.peer().call_tool(param).await
    }

    async fn list_tools(&self) -> Result<Vec<RmcpTool>, ServiceError> {
        self.running_service.peer().list_all_tools().await
    }

    async fn list_prompts(&self) -> Result<Vec<RmcpPrompt>, ServiceError> {
        self.running_service.peer().list_all_prompts().await
    }
}

/// Wrapper around rmcp service types to enable cloning.
///
/// This exists because [rmcp::service::RunningService] is not directly cloneable as it is a
/// pointer type to `Peer<C>`. This enum allows us to hold either the original service or its
/// peer representation, enabling cloning by converting the original service to a peer when needed.
pub enum InnerService {
    Original(rmcp::service::RunningService<RoleClient, McpService>),
    Peer(rmcp::service::Peer<RoleClient>),
}

impl InnerService {
    fn peer(&self) -> &rmcp::Peer<RoleClient> {
        match self {
            InnerService::Original(service) => service.peer(),
            InnerService::Peer(peer) => peer,
        }
    }
}

impl std::fmt::Debug for InnerService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            InnerService::Original(_) => f.debug_tuple("Original").field(&"RunningService<..>").finish(),
            InnerService::Peer(peer) => f.debug_tuple("Peer").field(peer).finish(),
        }
    }
}

impl Clone for InnerService {
    fn clone(&self) -> Self {
        match self {
            InnerService::Original(rs) => InnerService::Peer((*rs).clone()),
            InnerService::Peer(peer) => InnerService::Peer(peer.clone()),
        }
    }
}

/// This struct is consumed by the [rmcp] crate on server launch. The only purpose of this struct
/// is to handle server-to-client requests. Client-side code will own a [RunningMcpService]
/// instance.
#[derive(Debug)]
struct McpService {
    server_name: String,
    config: McpServerConfig,
    /// Sender to the related [McpServerActor]
    message_tx: mpsc::Sender<McpMessage>,
}

impl McpService {
    fn new(server_name: String, config: McpServerConfig, message_tx: mpsc::Sender<McpMessage>) -> Self {
        Self {
            server_name,
            config,
            message_tx,
        }
    }

    async fn launch(self) -> eyre::Result<RunningMcpService> {
        match &self.config {
            McpServerConfig::Local(config) => {
                let cmd = expand_path(&config.command)?;
                let mut env_vars = config.env.clone();
                let cmd = Command::new(cmd.as_ref() as &str).configure(|cmd| {
                    if let Some(envs) = &mut env_vars {
                        expand_env_vars(envs);
                        cmd.envs(envs);
                    }
                    cmd.envs(std::env::vars()).args(&config.args);

                    // Launch the MCP process in its own process group so that sigints won't kill
                    // the server process.
                    #[cfg(not(windows))]
                    cmd.process_group(0);
                });
                let (process, stderr) = TokioChildProcess::builder(cmd).stderr(Stdio::piped()).spawn().unwrap();
                let server_name = self.server_name.clone();
                info!(?server_name, "About to serve");
                let r = self.serve(process).await.unwrap();
                info!(?server_name, "Serve completed successfully");
                Ok(RunningMcpService::new(server_name, r, stderr))
            },
            McpServerConfig::StreamableHTTP(config) => todo!(),
        }
    }
}

impl rmcp::Service<RoleClient> for McpService {
    async fn handle_request(
        &self,
        request: <rmcp::RoleClient as rmcp::service::ServiceRole>::PeerReq,
        context: rmcp::service::RequestContext<RoleClient>,
    ) -> Result<<RoleClient as rmcp::service::ServiceRole>::Resp, rmcp::ErrorData> {
        match request {
            ServerRequest::PingRequest(_) => Ok(ClientResult::empty(())),
            ServerRequest::CreateMessageRequest(_) => Err(rmcp::ErrorData::method_not_found::<
                rmcp::model::CreateMessageRequestMethod,
            >()),
            ServerRequest::ListRootsRequest(_) => {
                Err(rmcp::ErrorData::method_not_found::<rmcp::model::ListRootsRequestMethod>())
            },
            ServerRequest::CreateElicitationRequest(_) => Err(rmcp::ErrorData::method_not_found::<
                rmcp::model::ElicitationCreateRequestMethod,
            >()),
        }
    }

    async fn handle_notification(
        &self,
        notification: <RoleClient as rmcp::service::ServiceRole>::PeerNot,
        context: rmcp::service::NotificationContext<RoleClient>,
    ) -> Result<(), rmcp::ErrorData> {
        match notification {
            ServerNotification::ToolListChangedNotification(_) => {
                let tools = context.peer.list_all_tools().await.unwrap();
            },
            ServerNotification::LoggingMessageNotification(notif) => {
                let level = notif.params.level;
                let data = notif.params.data;
                let server_name = &self.server_name;
                match level {
                    LoggingLevel::Error | LoggingLevel::Critical | LoggingLevel::Emergency | LoggingLevel::Alert => {
                        error!(target: "mcp", "{}: {}", server_name, data);
                    },
                    LoggingLevel::Warning => {
                        warn!(target: "mcp", "{}: {}", server_name, data);
                    },
                    LoggingLevel::Info => {
                        info!(target: "mcp", "{}: {}", server_name, data);
                    },
                    LoggingLevel::Debug => {
                        debug!(target: "mcp", "{}: {}", server_name, data);
                    },
                    LoggingLevel::Notice => {
                        trace!(target: "mcp", "{}: {}", server_name, data);
                    },
                }
            },
            ServerNotification::PromptListChangedNotification(_) => {},
            // TODO: support these
            ServerNotification::CancelledNotification(_) => (),
            ServerNotification::ResourceUpdatedNotification(_) => (),
            ServerNotification::ResourceListChangedNotification(_) => (),
            ServerNotification::ProgressNotification(_) => (),
        }
        Ok(())
    }

    fn get_info(&self) -> <RoleClient as rmcp::service::ServiceRole>::Info {
        // send from client to server, so that the server knows what capabilities we support.
        ClientInfo {
            protocol_version: Default::default(),
            capabilities: Default::default(),
            client_info: Implementation {
                name: "Q DEV CLI".to_string(),
                version: "1.0.0".to_string(),
                ..Default::default()
            },
        }
    }
}

async fn test_rmcp(config: LocalMcpServerConfig) {
    let cmd = config.command;
    let cmd = Command::new(cmd);
    let (process, stderr) = TokioChildProcess::builder(cmd).stderr(Stdio::piped()).spawn().unwrap();
    info!("About to serve");
    let r = ().serve(process).await.unwrap();
    info!("Serve complete");
    if let Some(info) = r.peer_info() {
        info!(?info, "peer info");
    }
    let tools = r.list_all_tools().await.unwrap();
    info!(?tools, "got tools");
    let prompts = r.list_all_prompts().await.unwrap();
    info!(?prompts, "got prompts");
}

impl From<RmcpTool> for ToolSpec {
    fn from(value: RmcpTool) -> Self {
        Self {
            name: value.name.to_string(),
            description: value.description.map(String::from).unwrap_or_default(),
            input_schema: (*value.input_schema).clone(),
        }
    }
}

/// A prompt that can be used to generate text from a model
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Prompt {
    /// The name of the prompt
    pub name: String,
    /// Optional description of what the prompt does
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Optional arguments that can be passed to customize the prompt
    #[serde(skip_serializing_if = "Option::is_none")]
    pub arguments: Option<Vec<PromptArgument>>,
}

/// Represents a prompt argument that can be passed to customize the prompt
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PromptArgument {
    /// The name of the argument
    pub name: String,
    /// A description of what the argument is used for
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Whether this argument is required
    #[serde(skip_serializing_if = "Option::is_none")]
    pub required: Option<bool>,
}

impl From<RmcpPrompt> for Prompt {
    fn from(value: RmcpPrompt) -> Self {
        Self {
            name: value.name,
            description: value.description,
            arguments: value.arguments.map(|v| v.into_iter().map(Into::into).collect()),
        }
    }
}

impl From<RmcpPromptArgument> for PromptArgument {
    fn from(value: RmcpPromptArgument) -> Self {
        Self {
            name: value.name,
            description: value.description,
            required: value.required,
        }
    }
}

#[derive(Debug, Clone)]
pub struct McpManagerHandle {
    /// Sender for sending requests to the tool manager task
    sender: RequestSender<McpManagerRequest, McpManagerResponse, McpManagerError>,
}

impl McpManagerHandle {
    fn new(sender: RequestSender<McpManagerRequest, McpManagerResponse, McpManagerError>) -> Self {
        Self { sender }
    }

    pub async fn launch_server(&self, name: String, config: McpServerConfig) -> Result<(), McpManagerError> {
        match self
            .sender
            .send_recv(McpManagerRequest::LaunchServer { name, config })
            .await
            .unwrap_or(Err(McpManagerError::Channel))?
        {
            McpManagerResponse::ToolSpecs(tool_specs) => todo!(),
            McpManagerResponse::LaunchServer(receiver) => todo!(),
        }
    }

    pub async fn get_tool_specs(&self, config: AgentConfig) -> Vec<ToolSpec> {
        Vec::new()
    }

    pub async fn generate_tool_spec(&self, name: &CanonicalToolName) -> Result<ToolSpec, McpManagerError> {
        todo!()
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
            McpManagerRequest::LaunchServer { name, config } => {
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
            McpManagerRequest::GetToolSpecs { config } => {
                todo!();
            },
            McpManagerRequest::RefreshMcpServers => todo!(),
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
            McpServerActorEvent::Initialized => {
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
        name: String,
        /// Config to use
        config: McpServerConfig,
    },
    /// Gets a valid tool specification according to the given agent config.
    GetToolSpecs {
        /// The agent config to use when generating the tool specs.
        config: AgentConfig,
    },
    RefreshMcpServers,
}

#[derive(Debug)]
pub enum McpManagerResponse {
    LaunchServer(oneshot::Receiver<LaunchServerResult>),
    ToolSpecs(Vec<ToolSpec>),
}

type LaunchServerResult = Result<(), McpManagerError>;

#[derive(Debug, Clone, Serialize, Deserialize, thiserror::Error)]
pub enum McpManagerError {
    #[error("Server with the name {} is currently initializing", .name)]
    ServerCurrentlyInitializing { name: String },
    #[error("Server with the name {} has already launched", .name)]
    ServerAlreadyLaunched { name: String },
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
    async fn test_mcp() {
        let _ = tracing_subscriber::fmt::try_init();
        test_rmcp(serde_json::from_str(LOCAL_CONFIG).unwrap()).await;
    }

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
