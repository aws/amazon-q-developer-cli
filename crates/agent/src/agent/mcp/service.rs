use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Stdio;
use std::time::{
    Duration,
    Instant,
};

use rmcp::RoleClient;
use rmcp::model::{
    CallToolRequestParams,
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
use rmcp::service::{
    DynService,
    ServiceExt,
};
use rmcp::transport::{
    ConfigureCommandExt as _,
    TokioChildProcess,
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

use super::actor::{
    McpMessage,
    McpServerActorEvent,
};
use super::oauth_util::{
    AuthClientWrapper,
    HttpServiceBuilder,
};
use super::types::Prompt;
use crate::agent::agent_config::definitions::McpServerConfig;
use crate::agent::agent_loop::types::ToolSpec;
use crate::agent::tools::mcp::McpToolAnnotations;
use crate::agent::util::expand_env_vars;
use crate::agent::util::path::expand_path;
use crate::agent_config::definitions::RemoteMcpServerConfig;
use crate::util::providers::RealProvider;

const SHUTDOWN_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(3);

/// Sentinel message returned when MCP OAuth token refresh and re-authentication both fail.
/// Detected downstream to produce a user-facing error that mentions `/mcp`.
pub const MCP_AUTH_REFRESH_FAILED: &str = "MCP_AUTH_REFRESH_FAILED";
pub const MCP_AUTH_REAUTH_FAILED: &str = "MCP_AUTH_REAUTH_FAILED";

/// This struct is consumed by the [rmcp] crate on server launch. The only purpose of this struct
/// is to handle server-to-client requests. Client-side code will own a [RunningMcpService]
/// instance.
#[derive(Clone, Debug)]
pub struct McpService {
    server_name: String,
    config: McpServerConfig,
    cred_path: PathBuf,
    /// Sender to the related [McpServerActor]
    message_tx: mpsc::Sender<McpMessage>,
}

impl McpService {
    pub fn new(
        server_name: String,
        config: McpServerConfig,
        cred_path: PathBuf,
        message_tx: mpsc::Sender<McpMessage>,
    ) -> Self {
        Self {
            server_name,
            config,
            cred_path,
            message_tx,
        }
    }

    /// Launches the provided MCP server, returning a client handle to the server for sending
    /// requests.
    pub async fn launch(
        self,
        event_tx: &mpsc::Sender<McpServerActorEvent>,
    ) -> eyre::Result<(RunningMcpService, LaunchMetadata)> {
        let serve_time_taken: std::time::Duration;
        let server_name = self.server_name.clone();

        let (service, child_stderr, auth_client) = match &self.config {
            McpServerConfig::Local(config) => {
                // TODO - don't use real provider
                let cmd = expand_path(&config.command, &RealProvider)?;

                let mut env_vars = config.env.clone();

                // On Windows, commands like npx/uvx are actually .cmd batch files.
                // Command::new() doesn't resolve .cmd/.bat extensions, so we need
                // to run them through cmd.exe /C which handles this automatically.
                #[cfg(windows)]
                let cmd = {
                    let cmd_str = cmd.to_string();
                    Command::new("cmd.exe").configure(|cmd| {
                        let mut cmd_args = vec!["/C".to_string(), cmd_str.clone()];
                        cmd_args.extend(config.args.iter().cloned());
                        // Apply shell env first, then config env, so config takes precedence
                        cmd.envs(std::env::vars()).args(&cmd_args);
                        if let Some(envs) = &mut env_vars {
                            expand_env_vars(envs);
                            cmd.envs(envs);
                        }
                    })
                };

                #[cfg(not(windows))]
                let cmd = Command::new(cmd.as_ref() as &str).configure(|cmd| {
                    // Apply shell env first, then config env, so config takes precedence
                    cmd.envs(std::env::vars()).args(&config.args);
                    if let Some(envs) = &mut env_vars {
                        expand_env_vars(envs);
                        cmd.envs(envs);
                    }
                    cmd.process_group(0);
                });
                let (process, stderr) = TokioChildProcess::builder(cmd).stderr(Stdio::piped()).spawn()?;
                let server_name = self.server_name.clone();

                let start_time = Instant::now();
                info!(?server_name, "Launching MCP server");
                let service = self.into_dyn().serve(process).await?;
                serve_time_taken = start_time.elapsed();
                info!(?serve_time_taken, ?server_name, "MCP server launched successfully");

                (service, stderr, None)
            },
            McpServerConfig::Remote(config) => {
                let RemoteMcpServerConfig {
                    url,
                    headers,
                    timeout_ms: timeout,
                    oauth_scopes: scopes,
                    oauth: oauth_config,
                    disabled: _,
                    disabled_tools: _,
                    force_auth,
                } = config;

                // Nested oauth_scopes (more specific) wins; fall back to top-level
                let effective_scopes = oauth_config
                    .as_ref()
                    .and_then(|c| c.oauth_scopes.as_ref())
                    .cloned()
                    .unwrap_or_else(|| scopes.clone());

                let start_time = Instant::now();
                info!(?self.server_name, "Launching MCP server");

                let mut processed_headers = headers.clone();
                expand_env_vars(&mut processed_headers);

                let http_service_builder = HttpServiceBuilder::new(
                    &self.server_name,
                    url,
                    *timeout,
                    &effective_scopes,
                    &processed_headers,
                    oauth_config,
                    event_tx,
                    *force_auth,
                );
                let (service, auth_client) = http_service_builder.try_build(&self, &self.cred_path).await?;
                serve_time_taken = start_time.elapsed();

                (service, None, auth_client)
            },
            McpServerConfig::Registry(_) => {
                eyre::bail!(
                    "Registry server '{}' was not resolved before launch. This is a bug.",
                    self.server_name
                );
            },
        };

        let launch_md = match service.peer_info() {
            Some(info) => {
                debug!(?server_name, ?info, "peer info found");

                // Fetch tools, if we can
                let (tools, tool_annotations, list_tools_duration) = if info.capabilities.tools.is_some() {
                    let start_time = Instant::now();
                    match service.list_all_tools().await {
                        Ok(rmcp_tools) => {
                            let annotations = extract_tool_annotations(&rmcp_tools);
                            (
                                Some(rmcp_tools.into_iter().map(Into::into).collect()),
                                Some(annotations),
                                Some(start_time.elapsed()),
                            )
                        },
                        Err(err) => {
                            error!(?err, "failed to list tools during server initialization");
                            (None, None, None)
                        },
                    }
                } else {
                    (None, None, None)
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
                    tool_annotations,
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
                    tool_annotations: None,
                    list_tools_duration: None,
                    prompts: None,
                    list_prompts_duration: None,
                }
            },
        };

        Ok((
            RunningMcpService::new(server_name, service, child_stderr, auth_client),
            launch_md,
        ))
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
            ServerRequest::CustomRequest(req) => Err(rmcp::ErrorData::new(
                rmcp::model::ErrorCode::METHOD_NOT_FOUND,
                format!("Method not found: {}", req.method),
                None,
            )),
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
                let _ = self.message_tx.send(McpMessage::Tools(tools)).await;
            },
            ServerNotification::PromptListChangedNotification(_) => {
                let prompts = context.peer.list_all_prompts().await;
                let _ = self.message_tx.send(McpMessage::Prompts(prompts)).await;
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
            ServerNotification::ElicitationCompletionNotification(_) => (),
            ServerNotification::CustomNotification(_) => (),
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
            meta: None,
        }
    }
}

/// Metadata about a successfully launched MCP server.
#[derive(Debug, Clone)]
pub struct LaunchMetadata {
    pub serve_time_taken: Duration,
    pub tools: Option<Vec<ToolSpec>>,
    /// Per-tool MCP annotations indexed by tool name. Populated alongside
    /// `tools`. Only tools that the gateway emitted annotations for appear
    /// here; lookups MUST treat absence as "no hints", not "false".
    pub tool_annotations: Option<HashMap<String, McpToolAnnotations>>,
    pub list_tools_duration: Option<Duration>,
    pub prompts: Option<Vec<Prompt>>,
    pub list_prompts_duration: Option<Duration>,
}

/// Extract a `tool-name → annotations` map from the rmcp tool list.
///
/// Tools whose annotations are entirely empty are dropped — there's
/// no point storing structurally-empty entries.
fn extract_tool_annotations(tools: &[RmcpTool]) -> HashMap<String, McpToolAnnotations> {
    tools
        .iter()
        .filter_map(|t| {
            let ann = t.annotations.as_ref()?;
            let ours: McpToolAnnotations = ann.into();
            ours.or_none().map(|a| (t.name.to_string(), a))
        })
        .collect()
}

/// Decorates the method passed in with retry logic, but only if the [RunningService] has an
/// instance of [AuthClientDropGuard].
/// The various methods to interact with the mcp server provided by RMCP supposedly does refresh
/// token once the token expires but that logic would require us to also note down the time at
/// which a token is obtained since the only time related information in the token is the duration
/// for which a token is valid. However, if we do solely rely on the internals of these methods to
/// refresh tokens, we would have no way of knowing when a token is obtained. (Maybe there is a
/// method that would allow us to configure what extra info to include in the token. If you find it,
/// feel free to remove this. That would also enable us to simplify the definition of
/// [RunningService])
macro_rules! decorate_with_auth_retry {
    ($param_type:ty, $method_name:ident, $return_type:ty) => {
        pub async fn $method_name(&self, param: $param_type) -> Result<$return_type, rmcp::ServiceError> {
            // Proactively check token validity before making the call.
            if let Some(auth_client) = self.auth_client.as_ref() {
                if let Err(e) = auth_client.auth_client.get_access_token().await {
                    info!("Token pre-check failed ({e}), attempting re-authentication before call");
                    if let Err(reauth_err) = auth_client.reauthorize().await {
                        error!("Pre-call re-authentication failed: {reauth_err}");
                    }
                }
            }

            let first_attempt = match &self.running_service {
                InnerService::Original(rs) => rs.$method_name(param.clone()).await,
                InnerService::Peer(peer) => peer.$method_name(param.clone()).await,
            };

            match first_attempt {
                Ok(result) => Ok(result),
                Err(e) => {
                    if let Some(auth_client) = self.auth_client.as_ref() {
                        let refresh_result = auth_client.refresh_token().await;
                        match refresh_result {
                            Ok(_) => {
                                info!("Token refreshed");
                                match &self.running_service {
                                    InnerService::Original(rs) => rs.$method_name(param).await,
                                    InnerService::Peer(peer) => peer.$method_name(param).await,
                                }
                            },
                            Err(refresh_err) => {
                                info!("Token refresh failed ({refresh_err}), attempting re-authentication");
                                match auth_client.reauthorize().await {
                                    Ok(_) => {
                                        info!("Reauth initiated");
                                    },
                                    Err(reauth_err) => {
                                        error!("Re-authentication failed: {reauth_err}");
                                        return Err(rmcp::ServiceError::McpError(rmcp::ErrorData::new(
                                            rmcp::model::ErrorCode::INTERNAL_ERROR,
                                            MCP_AUTH_REAUTH_FAILED,
                                            None,
                                        )));
                                    },
                                }

                                Err(rmcp::ServiceError::McpError(rmcp::ErrorData::new(
                                    rmcp::model::ErrorCode::INTERNAL_ERROR,
                                    MCP_AUTH_REFRESH_FAILED,
                                    None,
                                )))
                            },
                        }
                    } else {
                        Err(e)
                    }
                },
            }
        }
    };
    ($method_name:ident, $return_type:ty) => {
        pub async fn $method_name(&self) -> Result<$return_type, rmcp::ServiceError> {
            // Proactively check token validity before making the call.
            if let Some(auth_client) = self.auth_client.as_ref() {
                if let Err(e) = auth_client.auth_client.get_access_token().await {
                    info!("Token pre-check failed ({e}), attempting re-authentication before call");
                    if let Err(reauth_err) = auth_client.reauthorize().await {
                        error!("Pre-call re-authentication failed: {reauth_err}");
                    }
                }
            }

            let first_attempt = match &self.running_service {
                InnerService::Original(rs) => rs.$method_name().await,
                InnerService::Peer(peer) => peer.$method_name().await,
            };

            match first_attempt {
                Ok(result) => Ok(result),
                Err(e) => {
                    if let Some(auth_client) = self.auth_client.as_ref() {
                        let refresh_result = auth_client.refresh_token().await;
                        match refresh_result {
                            Ok(_) => {
                                info!("Token refreshed");
                                match &self.running_service {
                                    InnerService::Original(rs) => rs.$method_name().await,
                                    InnerService::Peer(peer) => peer.$method_name().await,
                                }
                            },
                            Err(refresh_err) => {
                                info!("Token refresh failed ({refresh_err}), attempting re-authentication");
                                match auth_client.reauthorize().await {
                                    Ok(_) => {
                                        info!("Reauth initiated");
                                    },
                                    Err(reauth_err) => {
                                        error!("Re-authentication failed: {reauth_err}");
                                        return Err(rmcp::ServiceError::McpError(rmcp::ErrorData::new(
                                            rmcp::model::ErrorCode::INTERNAL_ERROR,
                                            MCP_AUTH_REAUTH_FAILED,
                                            None,
                                        )));
                                    },
                                }

                                Err(rmcp::ServiceError::McpError(rmcp::ErrorData::new(
                                    rmcp::model::ErrorCode::INTERNAL_ERROR,
                                    MCP_AUTH_REFRESH_FAILED,
                                    None,
                                )))
                            },
                        }
                    } else {
                        Err(e)
                    }
                },
            }
        }
    };
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
    auth_client: Option<AuthClientWrapper>,
}

impl RunningMcpService {
    decorate_with_auth_retry!(CallToolRequestParams, call_tool, CallToolResult);

    decorate_with_auth_retry!(list_all_tools, Vec<RmcpTool>);

    decorate_with_auth_retry!(list_all_prompts, Vec<RmcpPrompt>);

    pub async fn get_prompt(
        &self,
        name: String,
        arguments: HashMap<String, String>,
    ) -> Result<rmcp::model::GetPromptResult, rmcp::ServiceError> {
        use rmcp::model::GetPromptRequestParams;
        let arguments_map: Option<serde_json::Map<String, serde_json::Value>> = if arguments.is_empty() {
            None
        } else {
            Some(
                arguments
                    .into_iter()
                    .map(|(k, v)| (k, serde_json::Value::String(v)))
                    .collect(),
            )
        };

        let params = GetPromptRequestParams {
            name,
            arguments: arguments_map,
            meta: None,
        };

        let first_attempt = match &self.running_service {
            InnerService::Original(rs) => rs.get_prompt(params.clone()).await,
            InnerService::Peer(peer) => peer.get_prompt(params.clone()).await,
        };

        match first_attempt {
            Ok(result) => Ok(result),
            Err(e) => {
                if let Some(auth_client) = self.auth_client.as_ref() {
                    let refresh_result = auth_client.refresh_token().await;
                    match refresh_result {
                        Ok(_) => {
                            info!("Token refreshed");
                            match &self.running_service {
                                InnerService::Original(rs) => rs.get_prompt(params).await,
                                InnerService::Peer(peer) => peer.get_prompt(params).await,
                            }
                        },
                        Err(refresh_err) => {
                            info!("Token refresh failed ({refresh_err}), attempting re-authentication");
                            match auth_client.reauthorize().await {
                                Ok(_) => {
                                    info!("Re-authentication successful, retrying operation");
                                    match &self.running_service {
                                        InnerService::Original(rs) => rs.get_prompt(params).await,
                                        InnerService::Peer(peer) => peer.get_prompt(params).await,
                                    }
                                },
                                Err(reauth_err) => {
                                    error!("Re-authentication failed: {reauth_err}");
                                    Err(e)
                                },
                            }
                        },
                    }
                } else {
                    Err(e)
                }
            },
        }
    }

    fn new(
        server_name: String,
        running_service: rmcp::service::RunningService<RoleClient, Box<dyn DynService<RoleClient>>>,
        child_stderr: Option<ChildStderr>,
        auth_client: Option<AuthClientWrapper>,
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
            auth_client,
        }
    }

    /// Gracefully shuts down the MCP server by calling cancel on the underlying rmcp service.
    ///
    /// For stdio transports, this closes stdin to the child process and waits for it to exit
    /// (with a timeout before sending SIGKILL). Uses `close_with_timeout` to avoid blocking
    /// indefinitely if the serve task cleanup hangs. This should be called before dropping
    /// the service to ensure child processes are properly cleaned up.
    pub async fn cancel(self) {
        match self.running_service {
            InnerService::Original(mut rs) => {
                if let Err(e) = rs.close_with_timeout(SHUTDOWN_TIMEOUT).await {
                    warn!("Failed to shut down MCP service: {e}");
                }
            },
            InnerService::Peer(_) => {},
        }
    }

    /// Returns true if the underlying transport to the MCP server has been closed.
    ///
    /// For stdio transports, this can happen if the server writes non-JSON-RPC output to
    /// stdout, causing a parse error that rmcp interprets as stream closure, which in turn
    /// closes stdin to the server process. For HTTP transports, this indicates the connection
    /// was lost or the server terminated.
    pub fn is_transport_closed(&self) -> bool {
        match &self.running_service {
            InnerService::Original(rs) => rs.is_transport_closed(),
            InnerService::Peer(peer) => peer.is_transport_closed(),
        }
    }

    /// Creates a test-only `RunningMcpService` with a closed transport.
    #[cfg(test)]
    pub(crate) fn new_closed_for_test() -> Self {
        let (ours, _theirs) = tokio::io::duplex(64);
        drop(_theirs);
        let service = McpService {
            server_name: "test".to_string(),
            config: McpServerConfig::Local(crate::agent::agent_config::definitions::LocalMcpServerConfig {
                command: "test".to_string(),
                args: vec![],
                env: None,
                timeout_ms: 1000,
                disabled: false,
                disabled_tools: vec![],
            }),
            cred_path: PathBuf::from("/tmp"),
            message_tx: mpsc::channel(1).0,
        };
        let ct = tokio_util::sync::CancellationToken::new();
        ct.cancel(); // Cancel immediately so transport is closed
        let boxed: Box<dyn DynService<RoleClient>> = Box::new(service);
        let rs = rmcp::service::serve_directly_with_ct(boxed, ours, None, ct);
        Self {
            running_service: InnerService::Original(rs),
            auth_client: None,
        }
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
    Original(rmcp::service::RunningService<RoleClient, Box<dyn DynService<RoleClient>>>),
    Peer(rmcp::service::Peer<RoleClient>),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mcp_auth_constants() {
        assert_eq!(MCP_AUTH_REFRESH_FAILED, "MCP_AUTH_REFRESH_FAILED");
        assert_eq!(MCP_AUTH_REAUTH_FAILED, "MCP_AUTH_REAUTH_FAILED");
    }

    #[test]
    fn test_shutdown_timeout_constant() {
        assert_eq!(SHUTDOWN_TIMEOUT, Duration::from_secs(3));
    }

    #[tokio::test]
    async fn test_mcp_service_new() {
        use crate::agent::agent_config::definitions::LocalMcpServerConfig;

        let cfg = McpServerConfig::Local(LocalMcpServerConfig {
            command: "echo".to_string(),
            args: vec!["hello".to_string()],
            env: None,
            timeout_ms: 30_000,
            disabled: false,
            disabled_tools: vec![],
        });

        let (tx, _rx) = mpsc::channel(8);
        let service = McpService::new("test-server".to_string(), cfg, PathBuf::from("/tmp/cred"), tx);
        assert_eq!(service.server_name, "test-server");
    }

    #[test]
    fn test_mcp_service_clone() {
        use crate::agent::agent_config::definitions::LocalMcpServerConfig;

        let cfg = McpServerConfig::Local(LocalMcpServerConfig {
            command: "echo".to_string(),
            args: vec![],
            env: None,
            timeout_ms: 30_000,
            disabled: false,
            disabled_tools: vec![],
        });

        let (tx, _rx) = mpsc::channel(8);
        let service = McpService::new("test".to_string(), cfg, PathBuf::from("/tmp"), tx);
        let cloned = service.clone();
        assert_eq!(cloned.server_name, "test");
    }

    #[test]
    fn test_mcp_service_debug() {
        use crate::agent::agent_config::definitions::LocalMcpServerConfig;

        let cfg = McpServerConfig::Local(LocalMcpServerConfig {
            command: "echo".to_string(),
            args: vec![],
            env: None,
            timeout_ms: 30_000,
            disabled: false,
            disabled_tools: vec![],
        });

        let (tx, _rx) = mpsc::channel(8);
        let service = McpService::new("debug-test".to_string(), cfg, PathBuf::from("/tmp"), tx);
        let debug_str = format!("{:?}", service);
        assert!(debug_str.contains("debug-test"));
    }

    #[test]
    fn test_launch_metadata_creation() {
        let m = LaunchMetadata {
            serve_time_taken: Duration::from_secs(1),
            tools: None,
            tool_annotations: None,
            list_tools_duration: None,
            prompts: None,
            list_prompts_duration: None,
        };
        assert_eq!(m.serve_time_taken, Duration::from_secs(1));
        assert!(m.tools.is_none());
        assert!(m.prompts.is_none());
    }

    #[test]
    fn test_launch_metadata_with_tools() {
        let m = LaunchMetadata {
            serve_time_taken: Duration::from_millis(500),
            tools: Some(vec![]),
            tool_annotations: Some(HashMap::new()),
            list_tools_duration: Some(Duration::from_millis(100)),
            prompts: Some(vec![]),
            list_prompts_duration: Some(Duration::from_millis(50)),
        };
        assert!(m.tools.is_some());
        assert_eq!(m.list_tools_duration, Some(Duration::from_millis(100)));
        assert!(m.prompts.is_some());
        assert_eq!(m.list_prompts_duration, Some(Duration::from_millis(50)));
    }

    #[test]
    fn test_launch_metadata_clone() {
        let m = LaunchMetadata {
            serve_time_taken: Duration::from_secs(1),
            tools: None,
            tool_annotations: None,
            list_tools_duration: None,
            prompts: None,
            list_prompts_duration: None,
        };
        let m2 = m.clone();
        assert_eq!(m.serve_time_taken, m2.serve_time_taken);
    }

    #[test]
    fn test_extract_tool_annotations_filters_empty() {
        use std::sync::Arc;

        use rmcp::model::ToolAnnotations as RmcpToolAnnotations;
        use serde_json::Map;

        let with_hint = RmcpTool {
            name: "read_only_tool".into(),
            description: None,
            input_schema: Arc::new(Map::new()),
            output_schema: None,
            annotations: Some(RmcpToolAnnotations {
                read_only_hint: Some(true),
                ..Default::default()
            }),
            title: None,
            icons: None,
            execution: None,
            meta: None,
        };
        let title_only = RmcpTool {
            name: "no_hint_tool".into(),
            description: None,
            input_schema: Arc::new(Map::new()),
            output_schema: None,
            annotations: Some(RmcpToolAnnotations {
                title: Some("display only".into()),
                ..Default::default()
            }),
            title: None,
            icons: None,
            execution: None,
            meta: None,
        };
        let no_annotations = RmcpTool {
            name: "bare_tool".into(),
            description: None,
            input_schema: Arc::new(Map::new()),
            output_schema: None,
            annotations: None,
            title: None,
            icons: None,
            execution: None,
            meta: None,
        };

        let map = extract_tool_annotations(&[with_hint, title_only, no_annotations]);
        assert_eq!(map.len(), 1);
        assert_eq!(map.get("read_only_tool").and_then(|a| a.read_only_hint), Some(true));
        assert!(!map.contains_key("no_hint_tool"));
        assert!(!map.contains_key("bare_tool"));
    }

    #[test]
    fn test_launch_metadata_debug() {
        let m = LaunchMetadata {
            serve_time_taken: Duration::from_secs(1),
            tools: None,
            tool_annotations: None,
            list_tools_duration: None,
            prompts: None,
            list_prompts_duration: None,
        };
        let debug_str = format!("{:?}", m);
        assert!(debug_str.contains("LaunchMetadata"));
    }

    #[test]
    fn test_mcp_service_get_info() {
        use crate::agent::agent_config::definitions::LocalMcpServerConfig;

        let cfg = McpServerConfig::Local(LocalMcpServerConfig {
            command: "echo".to_string(),
            args: vec![],
            env: None,
            timeout_ms: 30_000,
            disabled: false,
            disabled_tools: vec![],
        });

        let (tx, _rx) = mpsc::channel(8);
        let service = McpService::new("test".to_string(), cfg, PathBuf::from("/tmp"), tx);

        #[allow(unused_imports)]
        use rmcp::Service;
        let info = rmcp::Service::get_info(&service);
        assert_eq!(info.client_info.name, "Q DEV CLI");
        assert_eq!(info.client_info.version, "1.0.0");
    }

    #[tokio::test]
    async fn test_mcp_service_launch_registry_config_fails() {
        use crate::agent::agent_config::definitions::RegistryMcpServerConfig;

        let cfg = McpServerConfig::Registry(RegistryMcpServerConfig {
            server_type: "registry".to_string(),
            env: None,
            headers: None,
            timeout: None,
            oauth_scopes: vec![],
            oauth: None,
        });

        let (tx, _rx) = mpsc::channel(8);
        let (event_tx, _event_rx) = mpsc::channel(8);
        let service = McpService::new("registry-test".to_string(), cfg, PathBuf::from("/tmp"), tx);
        let result = service.launch(&event_tx).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not resolved"));
    }

    #[tokio::test]
    async fn test_mcp_service_launch_local_invalid_command() {
        use crate::agent::agent_config::definitions::LocalMcpServerConfig;

        let cfg = McpServerConfig::Local(LocalMcpServerConfig {
            command: "/nonexistent/binary/path/xyz123".to_string(),
            args: vec![],
            env: None,
            timeout_ms: 5000,
            disabled: false,
            disabled_tools: vec![],
        });

        let (tx, _rx) = mpsc::channel(8);
        let (event_tx, _event_rx) = mpsc::channel(8);
        let service = McpService::new("bad-cmd".to_string(), cfg, PathBuf::from("/tmp"), tx);
        let result = service.launch(&event_tx).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    #[ignore = "spawns real cat subprocess; hangs waiting for MCP protocol response"]
    async fn test_mcp_service_launch_local_with_env() {
        use crate::agent::agent_config::definitions::LocalMcpServerConfig;

        let cfg = McpServerConfig::Local(LocalMcpServerConfig {
            command: "cat".to_string(),
            args: vec![],
            env: Some(HashMap::from([
                ("MY_VAR".to_string(), "my_value".to_string()),
                ("ANOTHER".to_string(), "$HOME".to_string()),
            ])),
            timeout_ms: 5000,
            disabled: false,
            disabled_tools: vec![],
        });

        let (tx, _rx) = mpsc::channel(8);
        let (event_tx, _event_rx) = mpsc::channel(8);
        let service = McpService::new("env-test".to_string(), cfg, PathBuf::from("/tmp"), tx);
        // cat will start but won't speak MCP protocol, so launch will fail
        let result = service.launch(&event_tx).await;
        assert!(result.is_err());
    }

    #[test]
    fn test_running_mcp_service_clone_is_derived() {
        fn assert_clone<T: Clone>() {}
        fn assert_debug<T: std::fmt::Debug>() {}
        assert_clone::<RunningMcpService>();
        assert_debug::<RunningMcpService>();
    }

    #[test]
    fn test_inner_service_debug_format() {
        fn assert_debug<T: std::fmt::Debug>() {}
        assert_debug::<InnerService>();
    }

    // --- Serde tests for McpServerConfig variants ---

    #[test]
    fn test_serde_local_config() {
        let json = r#"{"command":"echo","args":["hi"],"env":{"K":"V"},"timeoutMs":5000,"disabled":false}"#;
        let cfg: McpServerConfig = serde_json::from_str(json).unwrap();
        assert!(matches!(cfg, McpServerConfig::Local(_)));
        if let McpServerConfig::Local(l) = &cfg {
            assert_eq!(l.command, "echo");
            assert_eq!(l.args, vec!["hi"]);
            assert_eq!(l.env.as_ref().unwrap()["K"], "V");
            assert_eq!(l.timeout_ms, 5000);
            assert!(!l.disabled);
        }
        // roundtrip
        let serialized = serde_json::to_string(&cfg).unwrap();
        let _: McpServerConfig = serde_json::from_str(&serialized).unwrap();
    }

    #[test]
    fn test_serde_local_config_defaults() {
        // minimal: only command required
        let json = r#"{"command":"node"}"#;
        let cfg: McpServerConfig = serde_json::from_str(json).unwrap();
        if let McpServerConfig::Local(l) = cfg {
            assert_eq!(l.command, "node");
            assert!(l.args.is_empty());
            assert!(l.env.is_none());
            assert_eq!(l.timeout_ms, 120_000); // default_timeout
            assert!(!l.disabled);
            assert!(l.disabled_tools.is_empty());
        } else {
            panic!("expected Local");
        }
    }

    #[test]
    fn test_serde_local_config_timeout_alias() {
        // "timeout" alias for "timeoutMs"
        let json = r#"{"command":"x","timeout":9999}"#;
        let cfg: McpServerConfig = serde_json::from_str(json).unwrap();
        if let McpServerConfig::Local(l) = cfg {
            assert_eq!(l.timeout_ms, 9999);
        } else {
            panic!("expected Local");
        }
    }

    #[test]
    fn test_serde_remote_config() {
        let json = r#"{"url":"https://example.com/mcp","headers":{"Auth":"Bearer tok"},"timeoutMs":10000}"#;
        let cfg: McpServerConfig = serde_json::from_str(json).unwrap();
        assert!(matches!(cfg, McpServerConfig::Remote(_)));
        if let McpServerConfig::Remote(r) = &cfg {
            assert_eq!(r.url, "https://example.com/mcp");
            assert_eq!(r.headers["Auth"], "Bearer tok");
            assert_eq!(r.timeout_ms, 10000);
        }
        let serialized = serde_json::to_string(&cfg).unwrap();
        let _: McpServerConfig = serde_json::from_str(&serialized).unwrap();
    }

    #[test]
    fn test_serde_remote_config_defaults() {
        let json = r#"{"url":"http://localhost:8080"}"#;
        let cfg: McpServerConfig = serde_json::from_str(json).unwrap();
        if let McpServerConfig::Remote(r) = cfg {
            assert_eq!(r.url, "http://localhost:8080");
            assert!(r.headers.is_empty());
            assert_eq!(r.timeout_ms, 120_000);
            assert!(r.oauth_scopes.is_empty());
            assert!(r.oauth.is_none());
            assert!(!r.disabled);
        } else {
            panic!("expected Remote");
        }
    }

    #[test]
    fn test_serde_registry_config() {
        let json = r#"{"type":"registry","env":{"A":"B"},"timeout":5000,"oauthScopes":["read"]}"#;
        let cfg: McpServerConfig = serde_json::from_str(json).unwrap();
        assert!(matches!(cfg, McpServerConfig::Registry(_)));
        if let McpServerConfig::Registry(r) = &cfg {
            assert_eq!(r.server_type, "registry");
            assert_eq!(r.env.as_ref().unwrap()["A"], "B");
            assert_eq!(r.timeout, Some(5000));
            assert_eq!(r.oauth_scopes, vec!["read"]);
        }
        let serialized = serde_json::to_string(&cfg).unwrap();
        let _: McpServerConfig = serde_json::from_str(&serialized).unwrap();
    }

    #[test]
    fn test_serde_local_with_disabled_tools() {
        let json = r#"{"command":"srv","disabledTools":["tool_a","tool_b"]}"#;
        let cfg: McpServerConfig = serde_json::from_str(json).unwrap();
        if let McpServerConfig::Local(l) = cfg {
            assert_eq!(l.disabled_tools, vec!["tool_a", "tool_b"]);
        } else {
            panic!("expected Local");
        }
    }

    #[test]
    fn test_serde_remote_with_oauth() {
        let json = r#"{"url":"https://x.com","oauth":{"clientId":"cid","redirectUri":"http://localhost:7778"}}"#;
        let cfg: McpServerConfig = serde_json::from_str(json).unwrap();
        if let McpServerConfig::Remote(r) = cfg {
            let oauth = r.oauth.unwrap();
            assert_eq!(oauth.client_id, Some("cid".to_string()));
            assert_eq!(oauth.redirect_uri, Some("http://localhost:7778".to_string()));
        } else {
            panic!("expected Remote");
        }
    }

    // --- handle_request tests ---

    fn make_service() -> McpService {
        use crate::agent::agent_config::definitions::LocalMcpServerConfig;
        let cfg = McpServerConfig::Local(LocalMcpServerConfig {
            command: "echo".to_string(),
            args: vec![],
            env: None,
            timeout_ms: 30_000,
            disabled: false,
            disabled_tools: vec![],
        });
        let (tx, _rx) = mpsc::channel(8);
        McpService::new("test".to_string(), cfg, PathBuf::from("/tmp"), tx)
    }

    // --- get_info tests ---

    #[test]
    fn test_get_info_protocol_version() {
        let service = make_service();
        let info = rmcp::Service::get_info(&service);
        assert_eq!(info.client_info.name, "Q DEV CLI");
        assert_eq!(info.client_info.version, "1.0.0");
    }

    // --- McpService with Remote config ---

    #[test]
    fn test_mcp_service_new_remote() {
        let cfg = McpServerConfig::Remote(RemoteMcpServerConfig {
            url: "https://example.com".to_string(),
            headers: HashMap::from([("X-Key".to_string(), "val".to_string())]),
            timeout_ms: 60_000,
            oauth_scopes: vec!["scope1".to_string()],
            oauth: None,
            disabled: false,
            disabled_tools: vec![],
            force_auth: false,
        });
        let (tx, _rx) = mpsc::channel(8);
        let service = McpService::new("remote-srv".to_string(), cfg, PathBuf::from("/cred"), tx);
        assert_eq!(service.server_name, "remote-srv");
    }

    #[test]
    fn test_mcp_service_new_registry() {
        use crate::agent::agent_config::definitions::RegistryMcpServerConfig;
        let cfg = McpServerConfig::Registry(RegistryMcpServerConfig {
            server_type: "registry".to_string(),
            env: None,
            headers: None,
            timeout: None,
            oauth_scopes: vec![],
            oauth: None,
        });
        let (tx, _rx) = mpsc::channel(8);
        let service = McpService::new("reg".to_string(), cfg, PathBuf::from("/tmp"), tx);
        assert_eq!(service.server_name, "reg");
    }

    // --- LaunchMetadata with populated fields ---

    #[test]
    fn test_launch_metadata_with_tool_specs() {
        use crate::agent::agent_loop::types::ToolSpec;
        let tools = vec![ToolSpec {
            name: "read_file".to_string(),
            description: "Reads a file".to_string(),
            input_schema: serde_json::Map::new(),
        }];
        let prompts = vec![Prompt {
            name: "summarize".to_string(),
            description: Some("Summarize text".to_string()),
            arguments: Some(vec![]),
        }];
        let m = LaunchMetadata {
            serve_time_taken: Duration::from_millis(200),
            tools: Some(tools),
            tool_annotations: None,
            list_tools_duration: Some(Duration::from_millis(50)),
            prompts: Some(prompts),
            list_prompts_duration: Some(Duration::from_millis(30)),
        };
        assert_eq!(m.tools.as_ref().unwrap().len(), 1);
        assert_eq!(m.tools.as_ref().unwrap()[0].name, "read_file");
        assert_eq!(m.prompts.as_ref().unwrap()[0].name, "summarize");
    }

    // --- Constants ---

    #[test]
    fn test_constants_values() {
        assert_eq!(SHUTDOWN_TIMEOUT, Duration::from_secs(3));
        assert_eq!(MCP_AUTH_REFRESH_FAILED, "MCP_AUTH_REFRESH_FAILED");
        assert_eq!(MCP_AUTH_REAUTH_FAILED, "MCP_AUTH_REAUTH_FAILED");
    }

    // --- McpService cred_path field ---

    #[test]
    fn test_mcp_service_stores_cred_path() {
        use crate::agent::agent_config::definitions::LocalMcpServerConfig;
        let cfg = McpServerConfig::Local(LocalMcpServerConfig {
            command: "x".to_string(),
            args: vec![],
            env: None,
            timeout_ms: 1000,
            disabled: false,
            disabled_tools: vec![],
        });
        let (tx, _rx) = mpsc::channel(8);
        let service = McpService::new("s".to_string(), cfg, PathBuf::from("/my/cred/path"), tx);
        assert_eq!(service.cred_path, PathBuf::from("/my/cred/path"));
    }

    // --- Config field access ---

    #[test]
    fn test_mcp_service_config_field() {
        use crate::agent::agent_config::definitions::LocalMcpServerConfig;
        let cfg = McpServerConfig::Local(LocalMcpServerConfig {
            command: "my-cmd".to_string(),
            args: vec!["--flag".to_string()],
            env: Some(HashMap::from([("K".to_string(), "V".to_string())])),
            timeout_ms: 7000,
            disabled: true,
            disabled_tools: vec!["t1".to_string()],
        });
        let (tx, _rx) = mpsc::channel(8);
        let service = McpService::new("srv".to_string(), cfg, PathBuf::from("/tmp"), tx);
        if let McpServerConfig::Local(l) = &service.config {
            assert_eq!(l.command, "my-cmd");
            assert_eq!(l.args, vec!["--flag"]);
            assert!(l.disabled);
            assert_eq!(l.disabled_tools, vec!["t1"]);
        } else {
            panic!("expected Local");
        }
    }

    // --- RunningMcpService::is_transport_closed ---

    #[tokio::test]
    async fn test_running_mcp_service_is_transport_closed() {
        let svc = RunningMcpService::new_closed_for_test();
        // The transport was closed immediately (peer dropped), so this should be true
        // Give it a moment to detect closure
        tokio::time::sleep(Duration::from_millis(50)).await;
        assert!(svc.is_transport_closed());
    }

    // --- RunningMcpService::cancel ---

    #[tokio::test]
    async fn test_running_mcp_service_cancel() {
        let svc = RunningMcpService::new_closed_for_test();
        // cancel should not panic even on a closed transport
        svc.cancel().await;
    }

    // --- InnerService Clone ---

    #[tokio::test]
    async fn test_inner_service_clone_original_becomes_peer() {
        let svc = RunningMcpService::new_closed_for_test();
        // Cloning should convert Original to Peer
        let cloned = svc.clone();
        let debug = format!("{:?}", cloned.running_service);
        assert!(debug.contains("Peer"));
    }

    // --- InnerService Debug ---

    #[tokio::test]
    async fn test_inner_service_debug_original() {
        let svc = RunningMcpService::new_closed_for_test();
        let debug = format!("{:?}", svc.running_service);
        assert!(debug.contains("Original"));
    }

    // --- RunningMcpService get_prompt with empty/non-empty args ---

    #[tokio::test]
    async fn test_running_mcp_service_get_prompt_empty_args() {
        let svc = RunningMcpService::new_closed_for_test();
        tokio::time::sleep(Duration::from_millis(50)).await;
        // Transport is closed, so this will error, but it exercises the arguments path
        let result = svc.get_prompt("test_prompt".to_string(), HashMap::new()).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_running_mcp_service_get_prompt_with_args() {
        let svc = RunningMcpService::new_closed_for_test();
        tokio::time::sleep(Duration::from_millis(50)).await;
        let mut args = HashMap::new();
        args.insert("key1".to_string(), "value1".to_string());
        args.insert("key2".to_string(), "value2".to_string());
        let result = svc.get_prompt("test_prompt".to_string(), args).await;
        assert!(result.is_err());
    }

    // --- RunningMcpService call_tool / list_all_tools / list_all_prompts ---

    #[tokio::test]
    async fn test_running_mcp_service_call_tool_closed() {
        let svc = RunningMcpService::new_closed_for_test();
        tokio::time::sleep(Duration::from_millis(50)).await;
        let params = CallToolRequestParams {
            name: "test_tool".into(),
            arguments: None,
            meta: None,
            task: None,
        };
        let result = svc.call_tool(params).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_running_mcp_service_list_all_tools_closed() {
        let svc = RunningMcpService::new_closed_for_test();
        tokio::time::sleep(Duration::from_millis(50)).await;
        let result = svc.list_all_tools().await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_running_mcp_service_list_all_prompts_closed() {
        let svc = RunningMcpService::new_closed_for_test();
        tokio::time::sleep(Duration::from_millis(50)).await;
        let result = svc.list_all_prompts().await;
        assert!(result.is_err());
    }

    // --- RunningMcpService debug ---

    #[tokio::test]
    async fn test_running_mcp_service_debug() {
        let svc = RunningMcpService::new_closed_for_test();
        let debug = format!("{:?}", svc);
        assert!(debug.contains("RunningMcpService"));
    }
}
