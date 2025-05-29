use std::collections::HashMap;
use std::io::Write;
use std::sync::Arc;
use std::sync::atomic::Ordering;

use crossterm::{
    queue,
    style,
};
use eyre::Result;
use serde::{
    Deserialize,
    Serialize,
};
use tokio::sync::RwLock;
use tracing::warn;

use super::InvokeOutput;
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
    SseTransport,
    ToolCallResult,
    TransportType,
};
use crate::platform::Context;

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct CustomToolConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(default = "default_transport_type")]
    pub transport: TransportType,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub env: Option<HashMap<String, String>>,
    #[serde(default = "default_timeout")]
    pub timeout: u64,
}

pub fn default_transport_type() -> TransportType {
    TransportType::Stdio
}

pub fn default_timeout() -> u64 {
    120 * 1000
}

#[derive(Debug)]
pub enum CustomToolClient {
    Stdio {
        server_name: String,
        client: McpClient<StdioTransport>,
        server_capabilities: RwLock<Option<ServerCapabilities>>,
    },
    Sse {
        server_name: String,
        client: McpClient<SseTransport>,
        server_capabilities: RwLock<Option<ServerCapabilities>>,
    },
}

impl CustomToolClient {
    pub fn from_config(server_name: String, config: CustomToolConfig) -> Result<Self> {
        let CustomToolConfig {
            command,
            url,
            transport,
            args,
            env,
            timeout,
        } = config;
        
        match transport {
            TransportType::Stdio => {
                let command = command.ok_or_else(|| eyre::eyre!("command required for STDIO transport"))?;
                let mcp_client_config = McpClientConfig {
                    server_name: server_name.clone(),
                    bin_path: Some(command),
                    server_url: None,
                    transport_type: Some(TransportType::Stdio),
                    args,
                    timeout,
                    client_info: serde_json::json!({
                       "name": "Q CLI Chat",
                       "version": "1.0.0"
                    }),
                    env,
                };
                let client = McpClient::<StdioTransport>::from_config(mcp_client_config)?;
                Ok(CustomToolClient::Stdio {
                    server_name,
                    client,
                    server_capabilities: RwLock::new(None),
                })
            }
            TransportType::Sse => {
                let url = url.ok_or_else(|| eyre::eyre!("url required for SSE transport"))?;
                let mcp_client_config = McpClientConfig {
                    server_name: server_name.clone(),
                    bin_path: None,
                    server_url: Some(url),
                    transport_type: Some(TransportType::Sse),
                    args,
                    timeout,
                    client_info: serde_json::json!({
                       "name": "Q CLI Chat",
                       "version": "1.0.0"
                    }),
                    env,
                };
                let client = tokio::task::block_in_place(|| {
                    tokio::runtime::Handle::current().block_on(async {
                        McpClient::<SseTransport>::from_config(mcp_client_config).await
                    })
                })?;
                Ok(CustomToolClient::Sse {
                    server_name,
                    client,
                    server_capabilities: RwLock::new(None),
                })
            }
            _ => Err(eyre::eyre!("Unsupported transport type")),
        }
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
            CustomToolClient::Sse {
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
            CustomToolClient::Sse { client, .. } => {
                client.messenger = Some(messenger);
            },
        }
    }

    pub fn get_server_name(&self) -> &str {
        match self {
            CustomToolClient::Stdio { server_name, .. } => server_name.as_str(),
            CustomToolClient::Sse { server_name, .. } => server_name.as_str(),
        }
    }

    pub async fn request(&self, method: &str, params: Option<serde_json::Value>) -> Result<JsonRpcResponse> {
        match self {
            CustomToolClient::Stdio { client, .. } => Ok(client.request(method, params).await?),
            CustomToolClient::Sse { client, .. } => Ok(client.request(method, params).await?),
        }
    }

    pub fn list_prompt_gets(&self) -> Arc<std::sync::RwLock<HashMap<String, PromptGet>>> {
        match self {
            CustomToolClient::Stdio { client, .. } => client.prompt_gets.clone(),
            CustomToolClient::Sse { client, .. } => client.prompt_gets.clone(),
        }
    }

    #[allow(dead_code)]
    pub async fn notify(&self, method: &str, params: Option<serde_json::Value>) -> Result<()> {
        match self {
            CustomToolClient::Stdio { client, .. } => Ok(client.notify(method, params).await?),
            CustomToolClient::Sse { client, .. } => Ok(client.notify(method, params).await?),
        }
    }

    pub fn is_prompts_out_of_date(&self) -> bool {
        match self {
            CustomToolClient::Stdio { client, .. } => client.is_prompts_out_of_date.load(Ordering::Relaxed),
            CustomToolClient::Sse { client, .. } => client.is_prompts_out_of_date.load(Ordering::Relaxed),
        }
    }

    pub fn prompts_updated(&self) {
        match self {
            CustomToolClient::Stdio { client, .. } => client.is_prompts_out_of_date.store(false, Ordering::Relaxed),
            CustomToolClient::Sse { client, .. } => client.is_prompts_out_of_date.store(false, Ordering::Relaxed),
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
    pub async fn invoke(&self, _ctx: &Context, _updates: &mut impl Write) -> Result<InvokeOutput> {
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

    pub fn queue_description(&self, updates: &mut impl Write) -> Result<()> {
        queue!(
            updates,
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
                updates,
                style::Print(" with the param:\n"),
                style::Print(params),
                style::ResetColor,
            )?;
        } else {
            queue!(updates, style::Print("\n"))?;
        }
        Ok(())
    }

    pub async fn validate(&mut self, _ctx: &Context) -> Result<()> {
        Ok(())
    }

    pub fn get_input_token_size(&self) -> usize {
        TokenCounter::count_tokens(self.method.as_str())
            + TokenCounter::count_tokens(self.params.as_ref().map_or("", |p| p.as_str().unwrap_or_default()))
    }
}
