use std::collections::HashMap;
use std::io::Write;
use std::sync::Arc;

use crossterm::{
    queue,
    style,
};
use eyre::Result;
use fig_os_shim::Context;
use mcp_client::{Client as McpClient, ClientConfig as McpClientConfig, JsonRpcStdioTransport, JsonRpcSseTransport, ServerCapabilities, StdioTransport, SseTransport};
use serde::{
    Deserialize,
    Serialize,
};
use super::{
    InvokeOutput,
    ToolSpec,
};

// TODO: support http transport type
#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct CustomToolConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub args: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub env: Option<HashMap<String, String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
}

// todo: maybe don't need enum, just an MCP with generic Transport
#[derive(Debug)]
pub enum CustomToolClient {
    Stdio {
        server_name: String,
        client: McpClient<StdioTransport>,
        #[allow(dead_code)]
        server_capabilities: Option<ServerCapabilities>,
    },
    Sse {
        server_name: String,
        client: McpClient<SseTransport>,
        #[allow(dead_code)]
        server_capabilities: Option<ServerCapabilities>,
    }
}

impl CustomToolClient {
    pub async fn from_config(server_name: String, config: CustomToolConfig) -> Result<Self> {
        // TODO: accommodate for envs specified
        let CustomToolConfig { command, args, env: _, url } = config;
        let mcp_client_config = McpClientConfig {
            server_name: server_name.clone(),
            bin_path: command.clone(),
            args,
            url: url.clone(),
            timeout: 120,
            init_params: serde_json::json!({
                 "protocolVersion": "2024-11-05",
                 "capabilities": {},
                 "clientInfo": {
                   "name": "Q CLI Chat",
                   "version": "1.0.0"
                 }
            }),
        };

        if mcp_client_config.bin_path.is_some() {
            let client = McpClient::<JsonRpcStdioTransport>::from_config(mcp_client_config)?;
            let server_capabilities = Some(client.init().await?);
            Ok(CustomToolClient::Stdio {
                server_name,
                client,
                server_capabilities,
            })
        }
        else if mcp_client_config.url.is_some() {
            let client = McpClient::<JsonRpcSseTransport>::from_config(mcp_client_config).await?;
            let server_capabilities = Some(client.init().await?);
            Ok(CustomToolClient::Sse {
                server_name,
                client,
                server_capabilities,
            })
        }
        else {
            return Err(eyre::eyre!("MCP Definition Invalid because neither command or url specified"));
        }
    }

    // todo: reduce duplication
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
            CustomToolClient::Sse {
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
            }
        }
    }

    pub async fn request(&self, method: &str, params: Option<serde_json::Value>) -> Result<serde_json::Value> {
        match self {
            CustomToolClient::Stdio { client, .. } => Ok(client.request(method, params).await?),
            CustomToolClient::Sse { client, .. } => Ok(client.request(method, params).await?),
        }
    }

    #[allow(dead_code)]
    pub async fn notify(&self, method: &str, params: Option<serde_json::Value>) -> Result<()> {
        match self {
            CustomToolClient::Stdio { client, .. } => Ok(client.notify(method, params).await?),
            CustomToolClient::Sse { client, .. } => Ok(client.notify(method, params).await?),
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
        Ok(InvokeOutput {
            output: super::OutputKind::Json(result.clone()),
        })
    }

    pub fn queue_description(&self, updates: &mut impl Write) -> Result<()> {
        queue!(updates, style::Print(format!("Running {}", self.name)),)?;
        if let Some(params) = &self.params {
            queue!(updates, style::Print(format!(" with the param:\n{}", params)),)?;
        }
        queue!(updates, style::Print("\n"),)?;
        Ok(())
    }

    pub async fn validate(&mut self, _ctx: &Context) -> Result<()> {
        Ok(())
    }
}
