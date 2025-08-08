use std::collections::HashMap;
use std::io::Write;
use std::sync::Arc;
use std::sync::atomic::Ordering;

use crossterm::{
    queue,
    style,
};
use eyre::Result;
use regex::Regex;
use schemars::JsonSchema;
use serde::{
    Deserialize,
    Serialize,
};
use tokio::sync::RwLock;
use tracing::warn;

use super::InvokeOutput;
use crate::cli::agent::{
    Agent,
    PermissionEvalResult,
};
use crate::cli::chat::CONTINUATION_LINE;
use crate::cli::chat::token_counter::TokenCounter;
use crate::mcp_client::{
    Client as McpClient,
    ClientConfig as McpClientConfig,
    JsonRpcResponse,
    MessageContent,
    Messenger,
    PromptGet,
    ServerCapabilities,
    StdioTransport,
    ToolCallResult,
};
use crate::os::Os;

// TODO: support http transport type
#[derive(Clone, Serialize, Deserialize, Debug, Eq, PartialEq, JsonSchema)]
pub struct CustomToolConfig {
    /// The command string used to initialize the mcp server
    pub command: String,
    /// A list of arguments to be used to run the command with
    #[serde(default)]
    pub args: Vec<String>,
    /// A list of environment variables to run the command with
    #[serde(skip_serializing_if = "Option::is_none")]
    pub env: Option<HashMap<String, String>>,
    /// Timeout for each mcp request in ms
    #[serde(default = "default_timeout")]
    pub timeout: u64,
    /// A boolean flag to denote whether or not to load this mcp server
    #[serde(default)]
    pub disabled: bool,
    /// Enable MCP sampling support for this server
    #[serde(default)]
    pub sampling_enabled: bool,
    /// A flag to denote whether this is a server from the legacy mcp.json
    #[serde(skip)]
    pub is_from_legacy_mcp_json: bool,
}

pub fn default_timeout() -> u64 {
    120 * 1000
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

#[derive(Debug)]
pub enum CustomToolClient {
    Stdio {
        /// This is the server name as recognized by the model (post sanitized)
        server_name: String,
        client: McpClient<StdioTransport>,
        server_capabilities: RwLock<Option<ServerCapabilities>>,
    },
}

impl CustomToolClient {
    // TODO: add support for http transport
    pub fn from_config(server_name: String, config: CustomToolConfig, os: &crate::os::Os) -> Result<Self> {
        let CustomToolConfig {
            command,
            args,
            env,
            timeout,
            disabled: _,
            sampling_enabled,
            ..
        } = config;

        // Process environment variables if present
        let processed_env = env.map(|mut env_vars| {
            process_env_vars(&mut env_vars, &os.env);
            env_vars
        });

        let mcp_client_config = McpClientConfig {
            server_name: server_name.clone(),
            bin_path: command.clone(),
            args,
            timeout,
            client_info: serde_json::json!({
               "name": "Q CLI Chat",
               "version": "1.0.0"
            }),
            env: processed_env,
            sampling_enabled,
        };
        let client = McpClient::<StdioTransport>::from_config(mcp_client_config, Some(std::sync::Arc::new(os.client.clone())))?;
        Ok(CustomToolClient::Stdio {
            server_name,
            client,
            server_capabilities: RwLock::new(None),
        })
    }

    pub async fn init(&self) -> Result<()> {
        match self {
            CustomToolClient::Stdio {
                client,
                server_capabilities,
                ..
            } => {
                if let Some(messenger) = &client.messenger {
                    let _ = messenger.send_init_msg().await;
                }
                // We'll need to first initialize. This is the handshake every client and server
                // needs to do before proceeding to anything else
                let cap = client.init().await?;
                // We'll be scrapping this for background server load: https://github.com/aws/amazon-q-developer-cli/issues/1466
                // So don't worry about the tidiness for now
                server_capabilities.write().await.replace(cap);
                Ok(())
            },
        }
    }

    pub fn assign_messenger(&mut self, messenger: Box<dyn Messenger>) {
        match self {
            CustomToolClient::Stdio { client, .. } => {
                client.messenger = Some(messenger);
            },
        }
    }

    pub fn get_server_name(&self) -> &str {
        match self {
            CustomToolClient::Stdio { server_name, .. } => server_name.as_str(),
        }
    }

    pub async fn request(&self, method: &str, params: Option<serde_json::Value>) -> Result<JsonRpcResponse> {
        match self {
            CustomToolClient::Stdio { client, .. } => Ok(client.request(method, params).await?),
        }
    }

    pub fn list_prompt_gets(&self) -> Arc<std::sync::RwLock<HashMap<String, PromptGet>>> {
        match self {
            CustomToolClient::Stdio { client, .. } => client.prompt_gets.clone(),
        }
    }

    #[allow(dead_code)]
    pub async fn notify(&self, method: &str, params: Option<serde_json::Value>) -> Result<()> {
        match self {
            CustomToolClient::Stdio { client, .. } => Ok(client.notify(method, params).await?),
        }
    }

    pub fn is_prompts_out_of_date(&self) -> bool {
        match self {
            CustomToolClient::Stdio { client, .. } => client.is_prompts_out_of_date.load(Ordering::Relaxed),
        }
    }

    pub fn prompts_updated(&self) {
        match self {
            CustomToolClient::Stdio { client, .. } => client.is_prompts_out_of_date.store(false, Ordering::Relaxed),
        }
    }
}

/// Represents a custom tool that can be invoked through the Model Context Protocol (MCP).
#[derive(Clone, Debug)]
pub struct CustomTool {
    /// Actual tool name as recognized by its MCP server. This differs from the tool names as they
    /// are seen by the model since they are not prefixed by its MCP server name.
    pub name: String,
    /// Reference to the client that manages communication with the tool's server process.
    pub client: Arc<CustomToolClient>,
    /// The method name to call on the tool's server, following the JSON-RPC convention.
    /// This corresponds to a specific functionality provided by the tool.
    pub method: String,
    /// Optional parameters to pass to the tool when invoking the method.
    /// Structured as a JSON value to accommodate various parameter types and structures.
    pub params: Option<serde_json::Value>,
}

impl CustomTool {
    pub async fn invoke(&self, _os: &Os, _updates: impl Write) -> Result<InvokeOutput> {
        // Assuming a response shape as per https://spec.modelcontextprotocol.io/specification/2024-11-05/server/tools/#calling-tools
        let resp = self.client.request(self.method.as_str(), self.params.clone()).await?;
        let result = match resp.result {
            Some(result) => result,
            None => {
                let failure = resp.error.map_or("Unknown error encountered".to_string(), |err| {
                    serde_json::to_string(&err).unwrap_or_default()
                });
                return Err(eyre::eyre!(failure));
            },
        };

        match serde_json::from_value::<ToolCallResult>(result.clone()) {
            Ok(mut de_result) => {
                for content in &mut de_result.content {
                    if let MessageContent::Image { data, .. } = content {
                        *data = format!("Redacted base64 encoded string of an image of size {}", data.len());
                    }
                }
                Ok(InvokeOutput {
                    output: super::OutputKind::Json(serde_json::json!(de_result)),
                })
            },
            Err(e) => {
                warn!("Tool call result deserialization failed: {:?}", e);
                Ok(InvokeOutput {
                    output: super::OutputKind::Json(result.clone()),
                })
            },
        }
    }

    pub fn queue_description(&self, output: &mut impl Write) -> Result<()> {
        queue!(
            output,
            style::Print("Running "),
            style::SetForegroundColor(style::Color::Green),
            style::Print(&self.name),
            style::ResetColor,
        )?;
        if let Some(params) = &self.params {
            let params = match serde_json::to_string_pretty(params) {
                Ok(params) => params
                    .split("\n")
                    .map(|p| format!("{CONTINUATION_LINE} {p}"))
                    .collect::<Vec<_>>()
                    .join("\n"),
                _ => format!("{:?}", params),
            };
            queue!(
                output,
                style::Print(" with the param:\n"),
                style::Print(params),
                style::Print("\n"),
                style::ResetColor,
            )?;
        } else {
            queue!(output, style::Print("\n"))?;
        }
        Ok(())
    }

    pub async fn validate(&mut self, _os: &Os) -> Result<()> {
        Ok(())
    }

    pub fn get_input_token_size(&self) -> usize {
        TokenCounter::count_tokens(self.method.as_str())
            + TokenCounter::count_tokens(self.params.as_ref().map_or("", |p| p.as_str().unwrap_or_default()))
    }

    pub fn eval_perm(&self, agent: &Agent) -> PermissionEvalResult {
        use crate::util::MCP_SERVER_TOOL_DELIMITER;
        let Self {
            name: tool_name,
            client,
            ..
        } = self;
        let server_name = client.get_server_name();

        if agent.allowed_tools.contains(&format!("@{server_name}"))
            || agent
                .allowed_tools
                .contains(&format!("@{server_name}{MCP_SERVER_TOOL_DELIMITER}{tool_name}"))
        {
            PermissionEvalResult::Allow
        } else {
            PermissionEvalResult::Ask
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
