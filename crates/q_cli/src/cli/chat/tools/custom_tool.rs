use std::collections::HashMap;
use std::io::Write;
use std::sync::Arc;

use eyre::Result;
use fig_os_shim::Context;
use mcp_client::{
    Client as McpClient,
    ClientConfig as McpClientConfig,
    JsonRpcStdioTransport,
    ServerCapabilities,
    StdioTransport,
};
use serde::{
    Deserialize,
    Serialize,
};
use crossterm::queue;
use crossterm::style::{
    self,
    Color,
};
use eyre::{
    Result,
    eyre,
};

use super::InvokeOutput;
use super::OutputKind;

#[derive(Clone, Serialize, Deserialize, Debug)]
pub enum CustomToolConfig {
    Stdio {
        command: String,
        args: Vec<String>,
        env: HashMap<String, String>,
    },
}

#[derive(Clone, Debug)]
pub enum CustomToolClient {
    Stdio {
        client: Arc<McpClient<StdioTransport>>,
        server_capabilities: Option<ServerCapabilities>,
    },
}

impl CustomToolClient {
    // TODO: add support for http transport
    pub async fn from_config(config: CustomToolConfig) -> Result<Self> {
        match config {
            // TODO: accomodate for envs specified
            CustomToolConfig::Stdio { command, args, env: _ } => {
                let mcp_client_config = McpClientConfig {
                    tool_name: command.clone(),
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
                };
                let client = McpClient::<JsonRpcStdioTransport>::from_config(mcp_client_config)?;
                let server_capabilities = Some(client.init().await?);
                Ok(CustomToolClient::Stdio {
                    client: Arc::new(client),
                    server_capabilities,
                })
            },
        }
    }

    pub async fn init(&mut self) -> Result<()> {
        match self {
            CustomToolClient::Stdio {
                client,
                server_capabilities,
            } => {
                server_capabilities.replace(client.init().await?);
                Ok(())
            },
        }
    }

    pub async fn request(&self, method: &str, params: Option<serde_json::Value>) -> Result<serde_json::Value> {
        match self {
            CustomToolClient::Stdio { client, .. } => Ok(client.request(method, params).await?),
        }
    }

    pub async fn notify(&self, method: &str, params: Option<serde_json::Value>) -> Result<()> {
        match self {
            CustomToolClient::Stdio { client, .. } => Ok(client.notify(method, params).await?),
        }
    }
}

pub struct CustomTool {
    client: CustomToolClient,
    method: String,
    params: Option<serde_json::Value>,
}

impl CustomTool {
    pub fn from_config(config: CustomToolConfig) -> Result<Self> {
        Ok(Self {
            client: CustomToolClient::from_config(config)
                .map_err(|e| eyre!("Failed to create custom tool client: {}", e))?,
            method: "execute".to_string(), // Default method to execute
            params: None,
        })
    }

    pub async fn invoke(&self, ctx: &Context, updates: &mut impl Write) -> Result<InvokeOutput> {
        queue!(
            updates,
            style::Print("Executing custom tool: "),
            style::SetForegroundColor(Color::Green),
            style::Print(&self.method),
            style::ResetColor,
            style::Print("\n"),
        )?;

        // Send the request to the custom tool
        let result = self.client.request(&self.method, self.params.clone()).await?;

        // Return the result as JSON
        Ok(InvokeOutput {
            output: OutputKind::Json(result),
        })
    }

    pub fn queue_description(&self, updates: &mut impl Write) -> Result<()> {
        queue!(
            updates,
            style::Print("Executing custom tool method: "),
            style::SetForegroundColor(Color::Green),
            style::Print(&self.method),
            style::ResetColor,
        )?;

        if let Some(params) = &self.params {
            queue!(
                updates,
                style::Print(" with params: "),
                style::SetForegroundColor(Color::Green),
                style::Print(serde_json::to_string_pretty(params).unwrap_or_else(|_| "[Invalid 
JSON]".to_string())),
                style::ResetColor,
            )?;
        }

        Ok(())
    }

    pub async fn validate(&mut self, ctx: &Context) -> Result<()> {
          // Initialize the client if needed
          match &mut self.client {
            CustomToolClient::Stdio { client: _, server_capabilities } => {
                if server_capabilities.is_none() {
                    // Initialize the client
                    self.client.init().await?;
                }
            }
        }

        // We could add more validation here like checking if the method exists in server capabilities
        Ok(())
    }
}
