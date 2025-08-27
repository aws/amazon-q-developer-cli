use std::collections::HashMap;

use regex::Regex;
use rmcp::model::{
    ListPromptsResult,
    ListToolsResult,
    LoggingLevel,
    LoggingMessageNotificationParam,
    PaginatedRequestParam,
};
use rmcp::service::{
    ClientInitializeError,
    NotificationContext,
    RunningService,
};
use rmcp::transport::TokioChildProcess;
use rmcp::{
    ClientHandler,
    RoleClient,
    ServiceExt,
};
use tokio::process::Command;
use tracing::error;

use super::messenger::Messenger;
use crate::cli::chat::tools::custom_tool::CustomToolConfig;
use crate::os::Os;

pub type RunningClient<M> = RunningService<RoleClient, McpClient<M>>;

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
}

#[derive(Debug)]
pub struct McpClient<M> {
    pub config: CustomToolConfig,
    server_name: String,
    messenger: M,
}

impl<M> McpClient<M>
where
    M: Messenger,
{
    pub fn new(server_name: String, config: CustomToolConfig, messenger: M) -> Self {
        Self {
            server_name,
            config,
            messenger,
        }
    }

    pub async fn init(mut self, os: &Os) -> Result<RunningService<RoleClient, McpClient<M>>, McpClientError> {
        let CustomToolConfig {
            command: command_as_str,
            args,
            env: config_envs,
            ..
        } = &mut self.config;
        let mut command = Command::new(command_as_str);

        command.envs(std::env::vars()).args(args);
        if let Some(envs) = config_envs {
            process_env_vars(envs, &os.env);
            command.envs(envs);
        }

        let messenger_clone = self.messenger.duplicate();
        let server_name = self.server_name.clone();

        let service = self.serve(TokioChildProcess::new(command)?).await.map_err(Box::new)?;

        // list tools here as per our existing protocol
        let service_clone = service.clone();
        tokio::spawn(async move {
            paginated_fetch! {
                final_result_type: ListToolsResult,
                content_type: rmcp::model::Tool,
                service_method: list_tools,
                result_field: tools,
                messenger_method: send_tools_list_result,
                service: service_clone,
                messenger: messenger_clone,
                server_name: server_name
            };
        });

        Ok(service)
    }
}

impl<M> ClientHandler for McpClient<M>
where
    M: Messenger,
{
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
