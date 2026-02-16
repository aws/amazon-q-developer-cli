use std::borrow::Cow;
use std::collections::{
    HashMap,
    HashSet,
};
use std::io::Write;

use crossterm::{
    queue,
    style,
};
use eyre::Result;
use rmcp::model::CallToolRequestParams;
use schemars::JsonSchema;
use serde::{
    Deserialize,
    Serialize,
};
use strsim::jaro_winkler;
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
use crate::util::ui::wrap_text_to_lines;

#[derive(Clone, Serialize, Deserialize, Debug, Eq, PartialEq, JsonSchema)]
#[serde(rename_all = "camelCase")]
#[derive(Default)]
pub enum TransportType {
    /// Standard input/output transport (default)
    #[default]
    Stdio,
    /// HTTP transport for web-based communication
    Http,
    /// Registry-based server (loaded from MCP registry)
    Registry,
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
    /// Transport type: "stdio", "http", or "registry"
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub transport_type: Option<String>,
    /// The URL for HTTP-based MCP server communication
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub url: String,
    /// HTTP headers to include when communicating with HTTP-based MCP servers
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub headers: HashMap<String, String>,
    /// Scopes with which oauth is done (deprecated: use oauth.oauthScopes instead)
    #[serde(default = "get_default_scopes", skip_serializing_if = "is_default_oauth_scopes")]
    pub oauth_scopes: Vec<String>,
    /// OAuth configuration for this server
    #[serde(skip_serializing_if = "Option::is_none")]
    pub oauth: Option<OAuthConfig>,
    /// The command string used to initialize the mcp server
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub command: String,
    /// A list of arguments to be used to run the command with
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub args: Vec<String>,
    /// A list of environment variables to run the command with
    #[serde(skip_serializing_if = "Option::is_none")]
    pub env: Option<HashMap<String, String>>,
    /// Timeout for each mcp request in ms
    #[serde(default = "default_timeout", skip_serializing_if = "is_default_timeout")]
    pub timeout: u64,
    /// A boolean flag to denote whether or not to load this mcp server
    #[serde(default, skip_serializing_if = "is_false")]
    pub disabled: bool,
    /// List of tool names from this server to disable
    #[serde(default, skip_serializing_if = "is_empty_vec")]
    pub disabled_tools: Vec<String>,
    /// A flag to denote whether this is a server from the legacy mcp.json
    #[serde(skip)]
    pub is_from_legacy_mcp_json: bool,
}

impl CustomToolConfig {
    /// Infer the transport type based on which fields are present
    pub fn inferred_type(&self) -> TransportType {
        // Check explicit type field first
        if let Some(ref type_str) = self.transport_type {
            match type_str.as_str() {
                "registry" => return TransportType::Registry,
                "http" => return TransportType::Http,
                "stdio" => return TransportType::Stdio,
                _ => {}, // Fall through to inference
            }
        }

        // Infer from fields
        if !self.command.is_empty() {
            TransportType::Stdio
        } else if !self.url.is_empty() {
            TransportType::Http
        } else {
            TransportType::Stdio
        }
    }

    /// Check if this is a registry-type server
    pub fn is_registry_type(&self) -> bool {
        matches!(self.inferred_type(), TransportType::Registry)
    }

    /// Get the effective oauth scopes, preferring the new location inside oauth object
    pub fn get_oauth_scopes(&self) -> Vec<String> {
        self.oauth
            .as_ref()
            .and_then(|o| o.oauth_scopes.clone())
            .unwrap_or_else(|| self.oauth_scopes.clone())
    }

    /// Create a minimal registry server config with only type: "registry"
    pub fn minimal_registry() -> Self {
        Self {
            transport_type: Some("registry".to_string()),
            ..Default::default()
        }
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

// Helper functions for serde skip_serializing_if
fn is_default_timeout(timeout: &u64) -> bool {
    *timeout == default_timeout()
}

fn is_false(b: &bool) -> bool {
    !b
}

fn is_empty_vec<T>(v: &[T]) -> bool {
    v.is_empty()
}

fn is_default_oauth_scopes(scopes: &Vec<String>) -> bool {
    *scopes == get_default_scopes()
}

// MCP-specific tool validation constants
const MAX_SIMILAR_SUGGESTIONS: usize = 3;
const SIMILARITY_THRESHOLD: f64 = 0.6;
const MAX_TOOLS_TO_DISPLAY: usize = 10;
const WARNING_LINE_WIDTH: usize = 80;

/// Finds tools similar to `missing_tool` from `available_tools` using Jaro-Winkler string
/// similarity. Returns up to `max_suggestions` tool names scoring at or above `threshold`.
fn find_similar_tools(
    missing_tool: &str,
    available_tools: &[String],
    max_suggestions: usize,
    threshold: f64,
) -> Vec<String> {
    let mut scored: Vec<(f64, &String)> = available_tools
        .iter()
        .map(|tool| (jaro_winkler(missing_tool, tool), tool))
        .filter(|(score, _)| *score >= threshold)
        .collect();

    scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
    scored
        .into_iter()
        .take(max_suggestions)
        .map(|(_, tool)| tool.clone())
        .collect()
}

/// Extracts a meaningful prefix from a tool name for grouping related tools.
///
/// Supports two naming conventions:
/// - **CamelCase**: Finds the boundary where an uppercase letter is followed by a lowercase letter,
///   and returns everything before that boundary. For example, `"DatabaseQuery"` → `"Database"`,
///   `"HTTPClient"` → `"HTTP"`.
/// - **snake_case**: Returns the segment before the first underscore. For example,
///   `"database_query"` → `"database"`.
///
/// Returns `None` if no meaningful prefix can be extracted (e.g., single-word lowercase names).
fn extract_tool_prefix(tool_name: &str) -> Option<String> {
    let chars: Vec<char> = tool_name.chars().collect();
    for i in 2..chars.len() {
        if chars[i - 1].is_uppercase() && chars[i].is_lowercase() {
            return Some(chars[..i - 1].iter().collect());
        }
    }

    if tool_name.contains('_') {
        tool_name.split('_').next().map(String::from)
    } else {
        None
    }
}

fn build_fuzzy_suggestions(missing_tools: &[String], available_tools: &[String]) -> Vec<String> {
    let mut lines = Vec::new();

    for missing in missing_tools {
        let similar = find_similar_tools(missing, available_tools, MAX_SIMILAR_SUGGESTIONS, SIMILARITY_THRESHOLD);
        if !similar.is_empty() {
            let formatted = format!(
                "  Did you mean: {}?",
                similar
                    .iter()
                    .map(|s| format!("\x1b[32m{}\x1b[0m", s))
                    .collect::<Vec<_>>()
                    .join(", ")
            );
            lines.push(formatted);
        }
    }

    lines
}

fn filter_available_tools(missing_tool: &str, available_tools: &[String]) -> (Vec<String>, String) {
    if let Some(prefix) = extract_tool_prefix(missing_tool) {
        let filtered: Vec<String> = available_tools
            .iter()
            .filter(|tool| tool.starts_with(&prefix))
            .cloned()
            .collect();
        if !filtered.is_empty() {
            return (filtered, prefix);
        }
    }

    if let Some(first_char) = missing_tool.chars().next() {
        let first_letter = first_char.to_lowercase().next().unwrap_or('_');
        let filtered: Vec<String> = available_tools
            .iter()
            .filter(|tool| tool.chars().next().unwrap_or('_').to_lowercase().next().unwrap_or('_') == first_letter)
            .cloned()
            .collect();
        if !filtered.is_empty() {
            return (filtered, first_letter.to_string());
        }
    }

    let filtered: Vec<String> = available_tools.iter().take(MAX_TOOLS_TO_DISPLAY).cloned().collect();
    (filtered, String::new())
}

/// Validates that requested tools from an agent configuration are available from an MCP server.
/// Returns a formatted warning message if any tools are missing, or None if all tools are
/// available.
pub(crate) fn validate_requested_tools(
    requested_tools: &HashSet<String>,
    available_tools: &[String],
    server_name: &str,
) -> Option<String> {
    use crossterm::style;

    let available_tools_set: HashSet<String> = available_tools.iter().cloned().collect();
    let mut missing_tools: Vec<String> = requested_tools
        .iter()
        .filter(|tool_name| !available_tools_set.contains(*tool_name))
        .cloned()
        .collect();
    missing_tools.sort();

    if missing_tools.is_empty() {
        return None;
    }

    warn!(
        server = %server_name,
        requested_tools = ?requested_tools.iter().collect::<Vec<_>>(),
        available_tools = ?available_tools,
        missing_tools = ?missing_tools,
        "Agent config specifies tools not available from MCP server @{}: {} missing",
        server_name,
        missing_tools.len()
    );

    let mut buffer = Vec::new();
    let tools_str = missing_tools.join(", ");
    let indent = "  ";

    let wrapped = wrap_text_to_lines(&tools_str, WARNING_LINE_WIDTH - indent.len());
    for (i, line) in wrapped.iter().enumerate() {
        if i == 0 {
            let _ = queue!(
                buffer,
                StyledText::warning_fg(),
                style::Print("WARNING: "),
                StyledText::reset(),
                style::Print("Agent config specifies "),
                StyledText::warning_fg(),
                style::Print(missing_tools.len()),
                StyledText::reset(),
                style::Print(format!(
                    " unavailable {} from ",
                    if missing_tools.len() == 1 { "tool" } else { "tools" }
                )),
                StyledText::info_fg(),
                style::Print(format!("@{}", server_name)),
                StyledText::reset(),
                style::Print(": "),
                StyledText::warning_fg(),
                style::Print(line),
                StyledText::reset(),
                style::Print("\n")
            );
        } else {
            let _ = queue!(
                buffer,
                style::Print(indent),
                StyledText::warning_fg(),
                style::Print(line),
                StyledText::reset(),
                style::Print("\n")
            );
        }
    }

    let suggestion_lines = build_fuzzy_suggestions(&missing_tools, available_tools);

    for suggestion in &suggestion_lines {
        let _ = queue!(buffer, style::Print(suggestion), style::Print("\n"));
    }

    if suggestion_lines.is_empty() {
        let (filtered_tools, filter_desc) = filter_available_tools(&missing_tools[0], available_tools);

        if !filtered_tools.is_empty() {
            let header_text = if filter_desc.is_empty() {
                format!("  Available tools from @{}: ", server_name)
            } else {
                format!("  Available tools from @{} matching '{}': ", server_name, filter_desc)
            };
            let header_len = header_text.len();

            let tools_str = filtered_tools.join(", ");
            let indent = "  ";

            let first_line_width = WARNING_LINE_WIDTH.saturating_sub(header_len);
            let continuation_width = WARNING_LINE_WIDTH - indent.len();

            let mut wrapped = Vec::new();
            let mut current_line = String::new();
            let mut current_width = first_line_width;

            for word in tools_str.split_whitespace() {
                let word_with_space = if current_line.is_empty() {
                    word.to_string()
                } else {
                    format!(" {}", word)
                };

                if current_line.len() + word_with_space.len() <= current_width {
                    current_line.push_str(&word_with_space);
                } else {
                    wrapped.push(current_line);
                    current_line = word.to_string();
                    current_width = continuation_width;
                }
            }
            if !current_line.is_empty() {
                wrapped.push(current_line);
            }

            for (i, line) in wrapped.iter().enumerate() {
                if i == 0 {
                    let _ = queue!(
                        buffer,
                        style::Print(indent),
                        style::Print("Available tools from "),
                        StyledText::info_fg(),
                        style::Print(format!("@{}", server_name)),
                        StyledText::reset()
                    );

                    if !filter_desc.is_empty() {
                        let _ = queue!(buffer, style::Print(format!(" matching '{}'", filter_desc)));
                    }

                    let _ = queue!(buffer, style::Print(": "), style::Print(line), style::Print("\n"));
                } else {
                    let _ = queue!(buffer, style::Print(indent), style::Print(line), style::Print("\n"));
                }
            }
        }
    }

    Some(String::from_utf8_lossy(&buffer).to_string())
}

impl Default for CustomToolConfig {
    fn default() -> Self {
        Self {
            transport_type: None,
            url: String::new(),
            headers: std::collections::HashMap::new(),
            oauth_scopes: get_default_scopes(),
            oauth: None,
            command: String::new(),
            args: Vec::new(),
            env: None,
            timeout: default_timeout(),
            disabled: false,
            disabled_tools: Vec::new(),
            is_from_legacy_mcp_json: false,
        }
    }
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
        let params = CallToolRequestParams {
            name: Cow::from(self.name.clone()),
            arguments: self.params.clone(),
            meta: None,
            task: None,
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
            PermissionEvalResult::ask()
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

    #[test]
    fn test_minimal_registry_config() {
        let config = CustomToolConfig::minimal_registry();
        assert_eq!(config.transport_type, Some("registry".to_string()));
        assert_eq!(config.timeout, default_timeout());
        assert!(config.headers.is_empty());
        assert!(config.env.is_none());
        assert!(!config.disabled);
        assert!(config.disabled_tools.is_empty());
    }

    #[test]
    fn test_registry_serialization_skips_defaults() {
        let config = CustomToolConfig::minimal_registry();
        let json = serde_json::to_string(&config).unwrap();

        // Should only contain type: "registry", other fields should be skipped
        assert!(json.contains(r#""type":"registry""#));
        assert!(!json.contains("url")); // Should be skipped (empty string)
        assert!(!json.contains("oauthScopes")); // Should be skipped (default scopes)
        assert!(!json.contains("command")); // Should be skipped (empty string)
        assert!(!json.contains("args")); // Should be skipped (empty vec)
        assert!(!json.contains("timeout")); // Should be skipped (default value)
        assert!(!json.contains("headers")); // Should be skipped (empty)
        assert!(!json.contains("env")); // Should be skipped (None)
        assert!(!json.contains("disabled")); // Should be skipped (false)

        // The final JSON should be very minimal
        assert_eq!(json, r#"{"type":"registry"}"#);
    }
}
