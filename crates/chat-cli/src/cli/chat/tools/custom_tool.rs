use std::borrow::Cow;
use std::collections::HashMap;
use std::io::Write;

use crossterm::{
    queue,
    style,
};
use eyre::Result;
use rmcp::RoleClient;
use rmcp::model::{
    CallToolRequestParam,
    RawContent,
};
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
use crate::os::Os;
use crate::util::MCP_SERVER_TOOL_DELIMITER;
use crate::util::pattern_matching::matches_any_pattern;

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
    pub client: rmcp::Peer<RoleClient>,
    /// Optional parameters to pass to the tool when invoking the method.
    /// Structured as a JSON value to accommodate various parameter types and structures.
    pub params: Option<serde_json::Map<String, serde_json::Value>>,
}

impl CustomTool {
    pub async fn invoke(&self, _os: &Os, _updates: impl Write) -> Result<InvokeOutput> {
        let params = CallToolRequestParam {
            name: Cow::from(self.name.clone()),
            arguments: self.params.clone(),
        };

        let mut resp = self.client.call_tool(params).await?;

        if resp.is_error.is_none_or(|v| !v) {
            for content in &mut resp.content {
                if let RawContent::Image(content) = &mut content.raw {
                    content.data = format!(
                        "Redacted base64 encoded string of an image of size {}",
                        content.data.len()
                    );
                }
            }
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
