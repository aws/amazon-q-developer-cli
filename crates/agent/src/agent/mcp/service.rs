use std::process::Stdio;
use std::time::{
    Duration,
    Instant,
};

use rmcp::model::{
    CallToolRequestParam,
    CallToolResult,
    ClientInfo,
    ClientResult,
    Implementation,
    LoggingLevel,
    Prompt as RmcpPrompt,
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
    ServiceExt as _,
};
use tokio::io::AsyncReadExt as _;
use tokio::process::{
    ChildStderr,
    Command,
};
use tokio::sync::mpsc;
use tracing::{
    debug,
    error,
    info,
    trace,
    warn,
};

use super::actor::McpMessage;
use super::types::Prompt;
use crate::agent::agent_config::definitions::McpServerConfig;
use crate::agent::agent_loop::types::ToolSpec;
use crate::agent::util::expand_env_vars;
use crate::agent::util::path::expand_path;

/// This struct is consumed by the [rmcp] crate on server launch. The only purpose of this struct
/// is to handle server-to-client requests. Client-side code will own a [RunningMcpService]
/// instance.
#[derive(Debug)]
pub struct McpService {
    server_name: String,
    config: McpServerConfig,
    /// Sender to the related [McpServerActor]
    message_tx: mpsc::Sender<McpMessage>,
}

impl McpService {
    pub fn new(server_name: String, config: McpServerConfig, message_tx: mpsc::Sender<McpMessage>) -> Self {
        Self {
            server_name,
            config,
            message_tx,
        }
    }

    /// Launches the provided MCP server, returning a client handle to the server for sending
    /// requests.
    pub async fn launch(self) -> eyre::Result<(RunningMcpService, LaunchMetadata)> {
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

                let start_time = Instant::now();
                info!(?server_name, "Launching MCP server");
                let service = self.serve(process).await?;
                let serve_time_taken = start_time.elapsed();
                info!(?serve_time_taken, ?server_name, "MCP server launched successfully");

                let launch_md = match service.peer_info() {
                    Some(info) => {
                        debug!(?server_name, ?info, "peer info found");

                        // Fetch tools, if we can
                        let (tools, list_tools_duration) = if info.capabilities.tools.is_some() {
                            let start_time = Instant::now();
                            match service.list_all_tools().await {
                                Ok(tools) => (
                                    Some(tools.into_iter().map(Into::into).collect()),
                                    Some(start_time.elapsed()),
                                ),
                                Err(err) => {
                                    error!(?err, "failed to list tools during server initialization");
                                    (None, None)
                                },
                            }
                        } else {
                            (None, None)
                        };

                        // Fetch prompts, if we can
                        let (prompts, list_prompts_duration) = if info.capabilities.prompts.is_some() {
                            let start_time = Instant::now();
                            match service.list_all_prompts().await {
                                Ok(prompts) => (
                                    Some(prompts.into_iter().map(Into::into).collect()),
                                    Some(start_time.elapsed()),
                                ),
                                Err(err) => {
                                    error!(?err, "failed to list prompts during server initialization");
                                    (None, None)
                                },
                            }
                        } else {
                            (None, None)
                        };

                        LaunchMetadata {
                            serve_time_taken,
                            tools,
                            list_tools_duration,
                            prompts,
                            list_prompts_duration,
                        }
                    },
                    None => {
                        warn!(?server_name, "no peer info found");
                        LaunchMetadata {
                            serve_time_taken,
                            tools: None,
                            list_tools_duration: None,
                            prompts: None,
                            list_prompts_duration: None,
                        }
                    },
                };

                Ok((RunningMcpService::new(server_name, service, stderr), launch_md))
            },
            McpServerConfig::StreamableHTTP(config) => {
                eyre::bail!("not supported");
            },
        }
    }
}

impl rmcp::Service<RoleClient> for McpService {
    async fn handle_request(
        &self,
        request: <rmcp::RoleClient as rmcp::service::ServiceRole>::PeerReq,
        _context: rmcp::service::RequestContext<RoleClient>,
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
                let tools = context.peer.list_all_tools().await;
                let _ = self.message_tx.send(McpMessage::ToolsResult(tools)).await;
            },
            ServerNotification::PromptListChangedNotification(_) => {
                let prompts = context.peer.list_all_prompts().await;
                let _ = self.message_tx.send(McpMessage::PromptsResult(prompts)).await;
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

/// Metadata about a successfully launched MCP server.
#[derive(Debug, Clone)]
pub struct LaunchMetadata {
    pub serve_time_taken: Duration,
    pub tools: Option<Vec<ToolSpec>>,
    pub list_tools_duration: Option<Duration>,
    pub prompts: Option<Vec<Prompt>>,
    pub list_prompts_duration: Option<Duration>,
}

/// Represents a handle to a running MCP server.
#[derive(Debug, Clone)]
pub struct RunningMcpService {
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

    pub async fn call_tool(&self, param: CallToolRequestParam) -> Result<CallToolResult, ServiceError> {
        self.running_service.peer().call_tool(param).await
    }

    pub async fn list_tools(&self) -> Result<Vec<RmcpTool>, ServiceError> {
        self.running_service.peer().list_all_tools().await
    }

    pub async fn list_prompts(&self) -> Result<Vec<RmcpPrompt>, ServiceError> {
        self.running_service.peer().list_all_prompts().await
    }
}

/// Wrapper around rmcp service types to enable cloning.
///
/// # Context
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
