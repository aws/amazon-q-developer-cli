use std::borrow::Cow;
use std::collections::HashMap;
use std::io::Write;

use crossterm::{
    queue,
    style,
};
use eyre::Result;
use rmcp::model::CallToolRequestParam;
use schemars::JsonSchema;
use serde::{
    Deserialize,
    Serialize,
};
use tracing::warn;

use super::InvokeOutput;
use crate::cli::agent::{
    Agent,
    PermissionEvalResult,
};
use crate::cli::chat::CONTINUATION_LINE;
use crate::cli::chat::token_counter::TokenCounter;
use crate::mcp_client::RunningService;
use crate::os::Os;
use crate::util::MCP_SERVER_TOOL_DELIMITER;
use crate::util::pattern_matching::matches_any_pattern;

#[derive(Clone, Serialize, Deserialize, Debug, Eq, PartialEq, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum TransportType {
    /// Standard input/output transport (default)
    Stdio,
    /// HTTP transport for web-based communication
    Http,
}

impl Default for TransportType {
    fn default() -> Self {
        Self::Stdio
    }
}

#[derive(Clone, Serialize, Deserialize, Debug, Eq, PartialEq, JsonSchema)]
pub struct CustomToolConfig {
    /// The type of transport the mcp server is expecting
    #[serde(default)]
    pub r#type: TransportType,
    /// The URL endpoint for HTTP-based MCP servers
    #[serde(default)]
    pub url: String,
    /// HTTP headers to include when communicating with HTTP-based MCP servers
    #[serde(default)]
    pub headers: HashMap<String, String>,
    /// The command string used to initialize the mcp server
    #[serde(default)]
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
    /// A flag to denote whether this is a server from the legacy mcp.json
    #[serde(skip)]
    pub is_from_legacy_mcp_json: bool,
}

pub fn default_timeout() -> u64 {
    120 * 1000
}

/// Represents a custom tool that can be invoked through the Model Context Protocol (MCP).
#[derive(Clone, Debug)]
pub struct CustomTool {
    /// Actual tool name as recognized by its MCP server. This differs from the tool names as they
    /// are seen by the model since they are not prefixed by its MCP server name.
    pub name: String,
    /// The name of the MCP (Model Context Protocol) server that hosts this tool.
    /// This is used to identify which server instance the tool belongs to and is
    /// prefixed to the tool name when presented to the model for disambiguation.
    pub server_name: String,
    /// Reference to the client that manages communication with the tool's server process.
    pub client: RunningService,
    /// Optional parameters to pass to the tool when invoking the method.
    /// Structured as a JSON value to accommodate various parameter types and structures.
    pub params: Option<serde_json::Map<String, serde_json::Value>>,
}

impl CustomTool {
    pub async fn invoke(&self, _os: &Os, _updates: &mut impl Write) -> Result<InvokeOutput> {
        let params = CallToolRequestParam {
            name: Cow::from(self.name.clone()),
            arguments: self.params.clone(),
        };

        let resp = self.client.call_tool(params.clone()).await?;

        if resp.is_error.is_none_or(|v| !v) {
            Ok(InvokeOutput {
                output: super::OutputKind::Json(serde_json::json!(resp)),
            })
        } else {
            warn!("Tool call for {} failed", self.name);
            Ok(InvokeOutput {
                output: super::OutputKind::Json(serde_json::json!(resp)),
            })
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
        TokenCounter::count_tokens(
            &serde_json::to_string(self.params.as_ref().unwrap_or(&serde_json::Map::new())).unwrap_or_default(),
        )
    }

    pub fn eval_perm(&self, _os: &Os, agent: &Agent) -> PermissionEvalResult {
        let Self { name: tool_name, .. } = self;
        let server_name = &self.server_name;

        let server_pattern = format!("@{server_name}");
        if agent.allowed_tools.contains(&server_pattern) {
            return PermissionEvalResult::Allow;
        }

        let tool_pattern = format!("@{server_name}{MCP_SERVER_TOOL_DELIMITER}{tool_name}");
        if matches_any_pattern(&agent.allowed_tools, &tool_pattern) {
            return PermissionEvalResult::Allow;
        }

        PermissionEvalResult::Ask
    }
}
