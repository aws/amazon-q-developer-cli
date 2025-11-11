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
use crate::mcp_client::{
    RunningService,
    oauth_util,
};
use crate::os::Os;
use crate::theme::StyledText;
use crate::util::MCP_SERVER_TOOL_DELIMITER;

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
#[serde(rename_all = "camelCase")]
pub struct OAuthConfig {
    /// Custom redirect URI for OAuth flow (e.g., "127.0.0.1:7778")
    /// If not specified, a random available port will be assigned by the OS
    #[serde(skip_serializing_if = "Option::is_none")]
    pub redirect_uri: Option<String>,
    /// Scopes with which oauth is done (new location, preferred over root-level oauth_scopes)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub oauth_scopes: Option<Vec<String>>,
}

#[derive(Clone, Serialize, Deserialize, Debug, Eq, PartialEq, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct CustomToolConfig {
    /// The transport type to use for communication with the MCP server
    #[serde(default)]
    pub r#type: TransportType,
    /// The URL for HTTP-based MCP server communication
    #[serde(default)]
    pub url: String,
    /// HTTP headers to include when communicating with HTTP-based MCP servers
    #[serde(default)]
    pub headers: HashMap<String, String>,
    /// Scopes with which oauth is done (deprecated: use oauth.oauthScopes instead)
    #[serde(default = "get_default_scopes")]
    pub oauth_scopes: Vec<String>,
    /// OAuth configuration for this server
    #[serde(skip_serializing_if = "Option::is_none")]
    pub oauth: Option<OAuthConfig>,
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

impl CustomToolConfig {
    /// Get the effective oauth scopes, preferring the new location inside oauth object
    pub fn get_oauth_scopes(&self) -> Vec<String> {
        self.oauth
            .as_ref()
            .and_then(|o| o.oauth_scopes.clone())
            .unwrap_or_else(|| self.oauth_scopes.clone())
    }
}

pub fn get_default_scopes() -> Vec<String> {
    oauth_util::get_default_scopes()
        .iter()
        .map(|s| (*s).to_string())
        .collect::<Vec<_>>()
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
    /// Returns the full tool name with server prefix in the format @server_name/tool_name
    pub fn namespaced_tool_name(&self) -> String {
        format!("@{}{}{}", self.server_name, MCP_SERVER_TOOL_DELIMITER, self.name)
    }

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

    pub fn queue_description(&self, tool: &super::tool::Tool, output: &mut impl Write) -> Result<()> {
        queue!(
            output,
            style::Print("Running tool "),
            StyledText::brand_fg(),
            style::Print(&self.name),
            StyledText::reset(),
        )?;
        if let Some(params) = &self.params {
            let params = match serde_json::to_string_pretty(params) {
                Ok(params) => params
                    .split("\n")
                    .map(|p| format!("{CONTINUATION_LINE} {p}"))
                    .collect::<Vec<_>>()
                    .join("\n"),
                _ => format!("{params:?}"),
            };
            queue!(output, style::Print(" with the param"),)?;
            super::display_tool_use(tool, output)?;
            queue!(
                output,
                style::Print("\n"),
                style::Print(params),
                style::Print("\n"),
                StyledText::reset(),
            )?;
        } else {
            super::display_tool_use(tool, output)?;
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
        use crate::util::tool_permission_checker::is_tool_in_allowlist;

        if is_tool_in_allowlist(&agent.allowed_tools, &self.name, Some(&self.server_name)) {
            PermissionEvalResult::Allow
        } else {
            PermissionEvalResult::Ask
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_oauth_scopes_backward_compat_old_format() {
        let json = r#"{
            "command": "test",
            "oauthScopes": ["read", "write"]
        }"#;
        let config: CustomToolConfig = serde_json::from_str(json).unwrap();
        assert_eq!(config.get_oauth_scopes(), vec!["read", "write"]);
    }

    #[test]
    fn test_oauth_scopes_backward_compat_new_format() {
        let json = r#"{
            "command": "test",
            "oauth": {
                "redirectUri": "127.0.0.1:8080",
                "oauthScopes": ["read", "write"]
            }
        }"#;
        let config: CustomToolConfig = serde_json::from_str(json).unwrap();
        assert_eq!(config.get_oauth_scopes(), vec!["read", "write"]);
    }

    #[test]
    fn test_oauth_scopes_new_format_takes_precedence() {
        let json = r#"{
            "command": "test",
            "oauthScopes": ["old1", "old2"],
            "oauth": {
                "redirectUri": "127.0.0.1:8080",
                "oauthScopes": ["new1", "new2"]
            }
        }"#;
        let config: CustomToolConfig = serde_json::from_str(json).unwrap();
        assert_eq!(config.get_oauth_scopes(), vec!["new1", "new2"]);
    }
}
