use std::borrow::Cow;
use std::collections::HashMap;
use std::process::Stdio;

use regex::Regex;
use rmcp::model::{
    ErrorCode,
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
use tokio::process::Command;
use tokio::task::JoinHandle;
use tracing::error;

use super::messenger::Messenger;
use crate::cli::chat::server_messenger::ServerMessenger;
use crate::cli::chat::tools::custom_tool::CustomToolConfig;
use crate::os::Os;

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
}

pub type RunningService = rmcp::service::RunningService<RoleClient, UninitMcpClient>;

#[derive(Debug)]
pub struct UninitMcpClient {
    pub config: CustomToolConfig,
    server_name: String,
    messenger: ServerMessenger,
}

impl UninitMcpClient {
    pub fn new(server_name: String, config: CustomToolConfig, messenger: ServerMessenger) -> Self {
        Self {
            server_name,
            config,
            messenger,
        }
    }

    pub async fn init(mut self, os: &Os) -> Result<InitializedMcpClient, McpClientError> {
        let os_clone = os.clone();

        let handle: JoinHandle<Result<RunningService, McpClientError>> = tokio::spawn(async move {
            let CustomToolConfig {
                command: command_as_str,
                args,
                env: config_envs,
                ..
            } = &mut self.config;

            let command = Command::new(command_as_str).configure(|cmd| {
                if let Some(envs) = config_envs {
                    process_env_vars(envs, &os_clone.env);
                    cmd.envs(envs);
                }
                cmd.envs(std::env::vars()).args(args).process_group(0);
            });

            let messenger_clone = self.messenger.duplicate();
            let server_name = self.server_name.clone();

            let result: Result<_, McpClientError> = async {
                // Spawn the child process with stderr piped
                let (tokio_child_process, child_stderr) =
                    TokioChildProcess::builder(command).stderr(Stdio::piped()).spawn()?;

                // Attempt to serve the process
                let service = self
                    .serve::<TokioChildProcess, _, _>(tokio_child_process)
                    .await
                    .map_err(Box::new)?;

                Ok((service, child_stderr))
            }
            .await;

            let (service, _child_stderr) = match result {
                Ok((service, stderr)) => (service, stderr),
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

            Ok(service)
        });

        Ok(InitializedMcpClient::Pending(handle))
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
        let _timeout = self.config.timeout;

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
        let _timeout = self.config.timeout;

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

    fn on_cancelled(
        &self,
        _params: rmcp::model::CancelledNotificationParam,
        _context: NotificationContext<RoleClient>,
    ) {
        self.messenger.send_deinit_msg();
    }
}

impl Service<RoleClient> for UninitMcpClient {
    async fn handle_request(
        &self,
        request: <RoleClient as rmcp::service::ServiceRole>::PeerReq,
        _context: rmcp::service::RequestContext<RoleClient>,
    ) -> Result<<RoleClient as rmcp::service::ServiceRole>::Resp, rmcp::ErrorData> {
        match request {
            ServerRequest::PingRequest(_) => Err(rmcp::ErrorData::method_not_found::<rmcp::model::PingRequestMethod>()),
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
            ServerNotification::CancelledNotification(notification) => self.on_cancelled(notification.params, context),
            // TODO: support these
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
            },
        }
    }
}

#[derive(Debug)]
pub enum InitializedMcpClient {
    Pending(JoinHandle<Result<RunningService, McpClientError>>),
    Ready(RunningService),
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
}
