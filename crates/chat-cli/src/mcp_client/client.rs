use std::borrow::Cow;
use std::collections::HashMap;
use std::process::Stdio;

use regex::Regex;
use rmcp::model::{
    CallToolRequestParam,
    CallToolResult,
    ClientResult,
    ErrorCode,
    GetPromptRequestParam,
    GetPromptResult,
    Implementation,
    InitializeRequestParam,
    ListPromptsResult,
    ListToolsResult,
    LoggingLevel,
    LoggingMessageNotificationParam,
    PaginatedRequestParam,
    ServerNotification,
    ServerRequest,
};
use rmcp::service::{
    ClientInitializeError,
    DynService,
    NotificationContext,
};
use rmcp::transport::{
    ConfigureCommandExt,
    TokioChildProcess,
};
use rmcp::{
    ErrorData,
    RoleClient,
    Service,
    ServiceError,
    ServiceExt,
};
use tokio::io::AsyncReadExt as _;
use tokio::process::{
    ChildStderr,
    Command,
};
use tokio::task::JoinHandle;
use tracing::{
    error,
    info,
};

use super::messenger::Messenger;
use super::{
    AuthClientWrapper,
    HttpServiceBuilder,
    OauthUtilError,
};
use crate::cli::chat::server_messenger::ServerMessenger;
use crate::cli::chat::tools::custom_tool::{
    CustomToolConfig,
    TransportType,
};
use crate::os::Os;
use crate::util::paths::DirectoryError;

/// Fetches all pages of specified resources from a server
macro_rules! paginated_fetch {
    (
        final_result_type: $final_result_type:ty,
        content_type: $content_type:ty,
        service_method: $service_method:ident,
        result_field: $result_field:ident,
        messenger_method: $messenger_method:ident,
        service: $service:expr,
        messenger: $messenger:expr,
        server_name: $server_name:expr
    ) => {
        {
            let mut cursor = None::<String>;
            let mut final_result = Ok(<$final_result_type>::with_all_items(Default::default()));
            let mut content = Vec::<$content_type>::new();

            loop {
                let param = Some(PaginatedRequestParam { cursor: cursor.clone() });
                match $service.$service_method(param).await {
                    Ok(mut result) => {
                        if let Some(s) = result.next_cursor {
                            cursor.replace(s);
                        }
                        content.append(&mut result.$result_field);
                    },
                    Err(e) => {
                        final_result = Err(e);
                        break;
                    },
                }
                if cursor.is_none() {
                    break;
                }
            }

            if let Ok(final_result) = &mut final_result {
                final_result.$result_field.append(&mut content);
            }

            if let Err(e) = $messenger.$messenger_method(final_result, Some($service)).await {
                error!(target: "mcp", "Initial {} result failed to send for server {}: {}",
                       stringify!($result_field), $server_name, e);
            }
        }
    };
}

/// Substitutes environment variables in the format ${env:VAR_NAME} with their actual values
fn substitute_env_vars(input: &str, env: &crate::os::Env) -> String {
    // Create a regex to match ${env:VAR_NAME} pattern
    let re = Regex::new(r"\$\{env:([^}]+)\}").unwrap();

    re.replace_all(input, |caps: &regex::Captures<'_>| {
        let var_name = &caps[1];
        env.get(var_name).unwrap_or_else(|_| format!("${{{}}}", var_name))
    })
    .to_string()
}

/// Process a HashMap of environment variables, substituting any ${env:VAR_NAME} patterns
/// with their actual values from the environment
fn process_env_vars(env_vars: &mut HashMap<String, String>, env: &crate::os::Env) {
    for (_, value) in env_vars.iter_mut() {
        *value = substitute_env_vars(value, env);
    }
}

#[derive(Debug, thiserror::Error)]
pub enum McpClientError {
    #[error(transparent)]
    ClientInitializeError(#[from] Box<ClientInitializeError>),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    JoinError(#[from] tokio::task::JoinError),
    #[error("Client has not finished initializing")]
    NotReady,
    #[error(transparent)]
    Directory(#[from] DirectoryError),
    #[error(transparent)]
    OauthUtil(#[from] OauthUtilError),
    #[error(transparent)]
    Parse(#[from] url::ParseError),
    #[error(transparent)]
    Auth(#[from] crate::auth::AuthError),
    #[error("{0}")]
    MalformedConfig(&'static str),
    #[error(transparent)]
    LookUp(#[from] shellexpand::LookupError<std::env::VarError>),
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
            let first_attempt = match &self.inner_service {
                InnerService::Original(rs) => rs.$method_name(param.clone()).await,
                InnerService::Peer(peer) => peer.$method_name(param.clone()).await,
            };

            match first_attempt {
                Ok(result) => Ok(result),
                Err(e) => {
                    // TODO: discern error type prior to retrying
                    // Not entirely sure what is thrown when auth is required
                    if let Some(auth_client) = self.auth_client.as_ref() {
                        let refresh_result = auth_client.refresh_token().await;
                        match refresh_result {
                            Ok(_) => {
                                info!("Token refreshed");
                                // Retry the operation after token refresh
                                match &self.inner_service {
                                    InnerService::Original(rs) => rs.$method_name(param).await,
                                    InnerService::Peer(peer) => peer.$method_name(param).await,
                                }
                            },
                            Err(_) => {
                                // If refresh fails, return the original error
                                // Currently our event loop just does not allow us easy ways to
                                // reauth entirely once a session starts since this would mean
                                // swapping of transport (which also means swapping of client)
                                Err(e)
                            },
                        }
                    } else {
                        // No auth client available, return original error
                        Err(e)
                    }
                },
            }
        }
    };
}

/// Wrapper around rmcp service types to enable cloning.
///
/// This exists because `rmcp::service::RunningService` is not directly cloneable as it is a
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

/// A wrapper around MCP (Model Context Protocol) service instances that manages
/// authentication and enables cloning functionality.
///
/// This struct holds either an original `RunningService` or its peer representation,
/// along with an optional authentication drop guard for managing OAuth tokens.
/// The authentication drop guard handles token lifecycle and cleanup when the
/// service is dropped.
///
/// # Fields
/// * `inner_service` - The underlying MCP service instance (original or peer)
/// * `auth_dropguard` - Optional authentication manager for OAuth token handling
#[derive(Debug)]
pub struct RunningService {
    pub inner_service: InnerService,
    auth_client: Option<AuthClientWrapper>,
}

impl Clone for RunningService {
    fn clone(&self) -> Self {
        RunningService {
            inner_service: self.inner_service.clone(),
            auth_client: self.auth_client.clone(),
        }
    }
}

impl RunningService {
    decorate_with_auth_retry!(CallToolRequestParam, call_tool, CallToolResult);

    decorate_with_auth_retry!(GetPromptRequestParam, get_prompt, GetPromptResult);
}

/// This struct implements the [Service] trait from rmcp. It is within this trait the logic of
/// server driven data flow (i.e. requests and notifications that are sent from the server) are
/// handled.
#[derive(Clone, Debug)]
pub struct McpClientService {
    pub config: CustomToolConfig,
    server_name: String,
    messenger: ServerMessenger,
}

impl McpClientService {
    pub fn new(server_name: String, config: CustomToolConfig, messenger: ServerMessenger) -> Self {
        Self {
            server_name,
            config,
            messenger,
        }
    }

    pub async fn init(self, os: &Os) -> Result<InitializedMcpClient, McpClientError> {
        let os_clone = os.clone();

        let handle: JoinHandle<Result<RunningService, McpClientError>> = tokio::spawn(async move {
            let messenger_clone = self.messenger.clone();
            let server_name = self.server_name.clone();

            let (service, child_stderr, auth_dropguard) = match self.into_service(&os_clone, &messenger_clone).await {
                Ok((service, stderr, auth_dg)) => (service, stderr, auth_dg),
                Err(e) => {
                    let msg = e.to_string();
                    let error_data = ErrorData {
                        code: ErrorCode::RESOURCE_NOT_FOUND,
                        message: Cow::from(msg),
                        data: None,
                    };
                    let err = ServiceError::McpError(error_data);

                    if let Err(send_err) = messenger_clone.send_tools_list_result(Err(err), None).await {
                        error!("Error sending tool result for {server_name}: {send_err}");
                    }

                    return Err(e);
                },
            };

            if let Some(mut stderr) = child_stderr {
                let server_name_clone = server_name.clone();
                tokio::spawn(async move {
                    let mut buf = [0u8; 1024];
                    loop {
                        match stderr.read(&mut buf).await {
                            Ok(0) => {
                                tracing::info!(target: "mcp", "{server_name_clone} stderr listening process exited due to EOF");
                                break;
                            },
                            Ok(size) => {
                                tracing::info!(target: "mcp", "{server_name_clone} logged to its stderr: {}", String::from_utf8_lossy(&buf[0..size]));
                            },
                            Err(e) => {
                                tracing::info!(target: "mcp", "{server_name_clone} stderr listening process exited due to error: {e}");
                                break; // Error reading
                            },
                        }
                    }
                });
            }

            let service_clone = service.clone();
            tokio::spawn(async move {
                let result: Result<(), Box<dyn std::error::Error + Send + Sync>> = async {
                    let init_result = service_clone.peer_info();
                    if let Some(init_result) = init_result {
                        if init_result.capabilities.tools.is_some() {
                            paginated_fetch! {
                                final_result_type: ListToolsResult,
                                content_type: rmcp::model::Tool,
                                service_method: list_tools,
                                result_field: tools,
                                messenger_method: send_tools_list_result,
                                service: service_clone.clone(),
                                messenger: messenger_clone,
                                server_name: server_name
                            };
                        }

                        if init_result.capabilities.prompts.is_some() {
                            paginated_fetch! {
                                final_result_type: ListPromptsResult,
                                content_type: rmcp::model::Prompt,
                                service_method: list_prompts,
                                result_field: prompts,
                                messenger_method: send_prompts_list_result,
                                service: service_clone,
                                messenger: messenger_clone,
                                server_name: server_name
                            };
                        }
                    }
                    Ok(())
                }
                .await;

                if let Err(e) = result {
                    error!(target: "mcp", "Error in MCP client initialization: {}", e);
                }
            });

            Ok(RunningService {
                inner_service: InnerService::Original(service),
                auth_client: auth_dropguard,
            })
        });

        Ok(InitializedMcpClient::Pending(handle))
    }

    async fn into_service(
        mut self,
        os: &Os,
        messenger: &dyn Messenger,
    ) -> Result<
        (
            rmcp::service::RunningService<RoleClient, Box<dyn DynService<RoleClient>>>,
            Option<ChildStderr>,
            Option<AuthClientWrapper>,
        ),
        McpClientError,
    > {
        let CustomToolConfig {
            r#type,
            url,
            command: command_as_str,
            ..
        } = &self.config;

        let is_malformed_http = matches!(r#type, TransportType::Http) && url.is_empty();
        let is_malformed_stdio = matches!(r#type, TransportType::Stdio) && command_as_str.is_empty();

        if is_malformed_http {
            return Err(McpClientError::MalformedConfig(
                "MCP config is malformed: transport type is specified to be http but url is empty",
            ));
        } else if is_malformed_stdio {
            return Err(McpClientError::MalformedConfig(
                "MCP config is malformed: transport type is specified to be stdio but command is empty",
            ));
        }

        match r#type {
            TransportType::Stdio => {
                let CustomToolConfig {
                    command: command_as_str,
                    args,
                    env: config_envs,
                    ..
                } = &mut self.config;

                let context = |input: &str| Ok(os.env.get(input).ok());
                let home_dir = || os.env.home().map(|p| p.to_string_lossy().to_string());
                let expanded_cmd = shellexpand::full_with_context(command_as_str, home_dir, context)?;

                let command = Command::new(expanded_cmd.as_ref() as &str).configure(|cmd| {
                    if let Some(envs) = config_envs {
                        process_env_vars(envs, &os.env);
                        cmd.envs(envs);
                    }
                    cmd.envs(std::env::vars()).args(args);

                    #[cfg(not(windows))]
                    cmd.process_group(0);
                });

                let (tokio_child_process, child_stderr) =
                    TokioChildProcess::builder(command).stderr(Stdio::piped()).spawn()?;

                let service = self
                    .into_dyn()
                    .serve::<TokioChildProcess, _, _>(tokio_child_process)
                    .await
                    .map_err(Box::new)?;

                Ok((service, child_stderr, None))
            },
            TransportType::Http => {
                let CustomToolConfig {
                    url,
                    headers,
                    oauth_scopes: scopes,
                    oauth,
                    timeout,
                    ..
                } = &self.config;

                // Process environment variables in headers
                let mut processed_headers = headers.clone();
                for (_, value) in processed_headers.iter_mut() {
                    *value = substitute_env_vars(value, &os.env);
                }

                let http_service_builder =
                    HttpServiceBuilder::new(url, os, url, *timeout, scopes, &processed_headers, oauth, messenger);

                let (service, auth_client_wrapper) = http_service_builder.try_build(&self).await?;

                Ok((service, None, auth_client_wrapper))
            },
        }
    }

    async fn on_logging_message(
        &self,
        params: LoggingMessageNotificationParam,
        _context: NotificationContext<RoleClient>,
    ) {
        let level = params.level;
        let data = params.data;
        let server_name = &self.server_name;

        match level {
            LoggingLevel::Error | LoggingLevel::Critical | LoggingLevel::Emergency | LoggingLevel::Alert => {
                tracing::error!(target: "mcp", "{}: {}", server_name, data);
            },
            LoggingLevel::Warning => {
                tracing::warn!(target: "mcp", "{}: {}", server_name, data);
            },
            LoggingLevel::Info => {
                tracing::info!(target: "mcp", "{}: {}", server_name, data);
            },
            LoggingLevel::Debug => {
                tracing::debug!(target: "mcp", "{}: {}", server_name, data);
            },
            LoggingLevel::Notice => {
                tracing::trace!(target: "mcp", "{}: {}", server_name, data);
            },
        }
    }

    async fn on_tool_list_changed(&self, context: NotificationContext<RoleClient>) {
        let NotificationContext { peer, .. } = context;

        paginated_fetch! {
            final_result_type: ListToolsResult,
            content_type: rmcp::model::Tool,
            service_method: list_tools,
            result_field: tools,
            messenger_method: send_tools_list_result,
            service: peer,
            messenger: self.messenger,
            server_name: self.server_name
        };
    }

    async fn on_prompt_list_changed(&self, context: NotificationContext<RoleClient>) {
        let NotificationContext { peer, .. } = context;

        paginated_fetch! {
            final_result_type: ListPromptsResult,
            content_type: rmcp::model::Prompt,
            service_method: list_prompts,
            result_field: prompts,
            messenger_method: send_prompts_list_result,
            service: peer,
            messenger: self.messenger,
            server_name: self.server_name
        };
    }
}

impl Service<RoleClient> for McpClientService {
    async fn handle_request(
        &self,
        request: <RoleClient as rmcp::service::ServiceRole>::PeerReq,
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
        context: NotificationContext<RoleClient>,
    ) -> Result<(), rmcp::ErrorData> {
        match notification {
            ServerNotification::ToolListChangedNotification(_) => self.on_tool_list_changed(context).await,
            ServerNotification::LoggingMessageNotification(notification) => {
                self.on_logging_message(notification.params, context).await;
            },
            ServerNotification::PromptListChangedNotification(_) => self.on_prompt_list_changed(context).await,
            // TODO: support these
            ServerNotification::CancelledNotification(_) => (),
            ServerNotification::ResourceUpdatedNotification(_) => (),
            ServerNotification::ResourceListChangedNotification(_) => (),
            ServerNotification::ProgressNotification(_) => (),
        };
        Ok(())
    }

    fn get_info(&self) -> <RoleClient as rmcp::service::ServiceRole>::Info {
        InitializeRequestParam {
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

/// InitializedMcpClient is the return of [McpClientService::init].
/// This is necessitated by the fact that [Service::serve], the command to spawn the process, is
/// async and does not resolve immediately. This delay can be significant and causes long perceived
/// latency during start up. However, our current architecture still requires the main chat loop to
/// have ownership of [RunningService].  
/// The solution chosen here is to instead spawn a task and have [Service::serve] called there and
/// return the handle to said task, stored in the [InitializedMcpClient::Pending] variant. This
/// enum is then flipped lazily (if applicable) when a [RunningService] is needed.
pub enum InitializedMcpClient {
    Pending(JoinHandle<Result<RunningService, McpClientError>>),
    Ready(RunningService),
}

impl std::fmt::Debug for InitializedMcpClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            InitializedMcpClient::Pending(_) => f.debug_tuple("Pending").field(&"JoinHandle<..>").finish(),
            InitializedMcpClient::Ready(_) => f.debug_tuple("Ready").field(&"RunningService<..>").finish(),
        }
    }
}

impl InitializedMcpClient {
    pub async fn get_running_service(&mut self) -> Result<&RunningService, McpClientError> {
        match self {
            InitializedMcpClient::Pending(handle) if handle.is_finished() => {
                let running_service = handle.await??;
                *self = InitializedMcpClient::Ready(running_service);
                let InitializedMcpClient::Ready(running_service) = self else {
                    unreachable!()
                };

                Ok(running_service)
            },
            InitializedMcpClient::Ready(running_service) => Ok(running_service),
            InitializedMcpClient::Pending(_) => Err(McpClientError::NotReady),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_substitute_env_vars() {
        // Set a test environment variable
        let os = Os::new().await.unwrap();
        unsafe {
            os.env.set_var("TEST_VAR", "test_value");
        }

        // Test basic substitution
        assert_eq!(
            substitute_env_vars("Value is ${env:TEST_VAR}", &os.env),
            "Value is test_value"
        );

        // Test multiple substitutions
        assert_eq!(
            substitute_env_vars("${env:TEST_VAR} and ${env:TEST_VAR}", &os.env),
            "test_value and test_value"
        );

        // Test non-existent variable
        assert_eq!(
            substitute_env_vars("${env:NON_EXISTENT_VAR}", &os.env),
            "${NON_EXISTENT_VAR}"
        );

        // Test mixed content
        assert_eq!(
            substitute_env_vars("Prefix ${env:TEST_VAR} suffix", &os.env),
            "Prefix test_value suffix"
        );
    }

    #[tokio::test]
    async fn test_process_env_vars() {
        let os = Os::new().await.unwrap();
        unsafe {
            os.env.set_var("TEST_VAR", "test_value");
        }

        let mut env_vars = HashMap::new();
        env_vars.insert("KEY1".to_string(), "Value is ${env:TEST_VAR}".to_string());
        env_vars.insert("KEY2".to_string(), "No substitution".to_string());

        process_env_vars(&mut env_vars, &os.env);

        assert_eq!(env_vars.get("KEY1").unwrap(), "Value is test_value");
        assert_eq!(env_vars.get("KEY2").unwrap(), "No substitution");
    }

    #[tokio::test]
    async fn test_http_headers_env_var_processing() {
        let os = Os::new().await.unwrap();
        unsafe {
            os.env.set_var("GITHUB_TOKEN", "github_pat_test123");
            os.env.set_var("API_KEY", "secret_key_456");
        }

        // Simulate HTTP headers with environment variables
        let mut headers = HashMap::new();
        headers.insert("Authorization".to_string(), "Bearer ${env:GITHUB_TOKEN}".to_string());
        headers.insert("X-API-Key".to_string(), "${env:API_KEY}".to_string());
        headers.insert("Content-Type".to_string(), "application/json".to_string());

        // Process headers (same logic as in HTTP transport)
        let mut processed_headers = headers.clone();
        for (_, value) in processed_headers.iter_mut() {
            *value = substitute_env_vars(value, &os.env);
        }

        // Verify environment variables were substituted
        assert_eq!(
            processed_headers.get("Authorization").unwrap(),
            "Bearer github_pat_test123"
        );
        assert_eq!(processed_headers.get("X-API-Key").unwrap(), "secret_key_456");
        assert_eq!(processed_headers.get("Content-Type").unwrap(), "application/json");
    }
}
