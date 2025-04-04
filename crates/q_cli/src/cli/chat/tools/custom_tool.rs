use std::collections::HashMap;
use std::io::Write;
use std::sync::Arc;

use crossterm::{
    queue,
    style,
};
use eyre::Result;
use fig_os_shim::Context;
use mcp_client::{
    Client as McpClient,
    ClientConfig as McpClientConfig,
    JsonRpcStdioTransport,
    MessageContent,
    ServerCapabilities,
    StdioTransport,
    ToolCallResult,
};
use serde::{
    Deserialize,
    Serialize,
};
use tracing::{
    debug,
    warn,
};

use super::{
    InvokeOutput,
    ToolSpec,
};

// TODO: support http transport type
#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct CustomToolConfig {
    pub command: String,
    pub args: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub env: Option<HashMap<String, String>>,
}

#[derive(Debug)]
pub enum CustomToolClient {
    Stdio {
        server_name: String,
        client: McpClient<StdioTransport>,
        #[allow(dead_code)]
        server_capabilities: Option<ServerCapabilities>,
    },
}

impl CustomToolClient {
    // TODO: add support for http transport
    pub async fn from_config(server_name: String, config: CustomToolConfig) -> Result<Self> {
        let CustomToolConfig { command, args, env } = config;
        let mcp_client_config = McpClientConfig {
            server_name: server_name.clone(),
            bin_path: command.clone(),
            args,
            timeout: 120,
            init_params: serde_json::json!({
                 "protocolVersion": "2024-11-05",
                 "capabilities": {},
                 "clientInfo": {
                   "name": "Q CLI Chat",
                   "version": "1.0.0"
                 }
            }),
            env,
        };
        let client = McpClient::<JsonRpcStdioTransport>::from_config(mcp_client_config)?;
        let server_capabilities = Some(client.init().await?);
        Ok(CustomToolClient::Stdio {
            server_name,
            client,
            server_capabilities,
        })
    }

    pub async fn get_tool_spec(&self) -> Result<(String, Vec<ToolSpec>)> {
        match self {
            CustomToolClient::Stdio {
                client, server_name, ..
            } => {
                let resp = client.request("tools/list", None).await?;
                // Assuming a shape of return as per https://spec.modelcontextprotocol.io/specification/2024-11-05/server/tools/#listing-tools
                let result = resp
                    .get("result")
                    .ok_or(eyre::eyre!("Failed to retrieve result for custom tool {}", server_name))?;
                let tools = result.get("tools").ok_or(eyre::eyre!(
                    "Failed to retrieve tools from result for custom tool {}",
                    server_name
                ))?;
                let tools = serde_json::from_value::<Vec<ToolSpec>>(tools.clone())?;
                Ok((server_name.clone(), tools))
            },
        }
    }

    pub async fn request(&self, method: &str, params: Option<serde_json::Value>) -> Result<serde_json::Value> {
        match self {
            CustomToolClient::Stdio { client, .. } => Ok(client.request(method, params).await?),
        }
    }

    #[allow(dead_code)]
    pub async fn notify(&self, method: &str, params: Option<serde_json::Value>) -> Result<()> {
        match self {
            CustomToolClient::Stdio { client, .. } => Ok(client.notify(method, params).await?),
        }
    }
}

#[derive(Clone, Debug)]
pub struct CustomTool {
    pub name: String,
    pub client: Arc<CustomToolClient>,
    pub method: String,
    pub params: Option<serde_json::Value>,
}

impl CustomTool {
    pub async fn invoke(&self, _ctx: &Context, _updates: &mut impl Write) -> Result<InvokeOutput> {
        // Assuming a response shape as per https://spec.modelcontextprotocol.io/specification/2024-11-05/server/tools/#calling-tools
        let resp = self.client.request(self.method.as_str(), self.params.clone()).await?;
        let result = resp
            .get("result")
            .ok_or(eyre::eyre!("{} invocation failed to produce a result", self.name))?;

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
            queue!(
                updates,
                style::Print(" with the param:\n"),
                style::SetForegroundColor(style::Color::Yellow),
                style::Print(serde_json::to_string_pretty(params).unwrap_or_else(|_| format!("{:?}", params))),
                style::ResetColor,
            )?;
        }
        queue!(updates, style::Print("\n"),)?;
        Ok(())
    }

    pub async fn validate(&mut self, _ctx: &Context) -> Result<()> {
        Ok(())
    }
}
