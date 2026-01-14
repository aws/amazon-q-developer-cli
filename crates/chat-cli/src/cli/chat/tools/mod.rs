pub mod code;
pub mod code_config;
pub mod custom_tool;
pub mod delegate;
pub mod diff_tool;
pub mod execute;
pub mod fs_read;
pub mod fs_write;
pub mod gh_issue;
pub mod glob;
pub mod grep;
pub mod introspect;
pub mod knowledge;
pub mod switch_to_execution;
pub mod thinking;
pub mod todo;
pub mod tool;
pub mod use_aws;
pub mod use_subagent;
pub mod web_fetch;
pub mod web_search;

use std::borrow::{
    Borrow,
    Cow,
};
use std::io::Write;
use std::path::{
    Path,
    PathBuf,
};

use crossterm::{
    queue,
    style,
};
use eyre::Result;
use serde::{
    Deserialize,
    Serialize,
};
pub use tool::{
    Tool,
    ToolMetadata,
    is_native_tool,
};
use tracing::error;

use super::consts::{
    MAX_TOOL_RESPONSE_SIZE,
    USER_AGENT_APP_NAME,
    USER_AGENT_ENV_VAR,
    USER_AGENT_VERSION_KEY,
    USER_AGENT_VERSION_VALUE,
};
use super::util::images::RichImageBlocks;
use crate::os::Os;
use crate::theme::{
    StyledText,
    theme,
};

pub const DEFAULT_APPROVE: [&str; 0] = [];

#[derive(Debug, Clone)]
pub struct ToolInfo {
    /// The name used in the tool specification sent to the model ('name' attribute in
    /// tool_index.json is the source)
    pub spec_name: &'static str,
    /// The preferred alias for agent configuration and UI display (e.g., "shell", "read")
    pub preferred_alias: &'static str,
    /// All valid aliases accepted in agent configuration, including the old Q CLI names and Kiro
    /// Names (preferred alias)
    // (e.g., ["execute_bash", "execute_cmd", "shell"])
    pub aliases: &'static [&'static str],
}

/// A tool specification to be sent to the model as part of a conversation. Maps to
/// [BedrockToolSpecification].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolSpec {
    pub name: String,
    pub description: String,
    #[serde(alias = "inputSchema")]
    pub input_schema: InputSchema,
    #[serde(skip_serializing, default = "tool_origin")]
    pub tool_origin: ToolOrigin,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum ToolOrigin {
    Native,
    McpServer(String),
}

impl std::hash::Hash for ToolOrigin {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        std::mem::discriminant(self).hash(state);
        match self {
            Self::Native => {},
            Self::McpServer(name) => name.hash(state),
        }
    }
}

impl Borrow<str> for ToolOrigin {
    fn borrow(&self) -> &str {
        match self {
            Self::McpServer(name) => name.as_str(),
            Self::Native => "native",
        }
    }
}

impl<'de> Deserialize<'de> for ToolOrigin {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        if s == "native___" {
            Ok(ToolOrigin::Native)
        } else {
            Ok(ToolOrigin::McpServer(s))
        }
    }
}

impl Serialize for ToolOrigin {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        match self {
            ToolOrigin::Native => serializer.serialize_str("native___"),
            ToolOrigin::McpServer(server) => serializer.serialize_str(server),
        }
    }
}

impl std::fmt::Display for ToolOrigin {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ToolOrigin::Native => write!(f, "Built-in"),
            ToolOrigin::McpServer(server) => write!(f, "{server} (MCP)"),
        }
    }
}

fn tool_origin() -> ToolOrigin {
    ToolOrigin::Native
}

#[derive(Debug, Clone)]
pub struct QueuedTool {
    pub id: String,
    pub name: String,
    pub preferred_alias: String,
    pub accepted: bool,
    pub tool: Tool,
    pub tool_input: serde_json::Value,
}

/// The schema specification describing a tool's fields.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InputSchema(pub serde_json::Value);

/// The output received from invoking a [Tool].
#[derive(Debug, Default)]
pub struct InvokeOutput {
    pub output: OutputKind,
}

impl InvokeOutput {
    pub fn as_str(&self) -> Cow<'_, str> {
        match &self.output {
            OutputKind::Text(s) => s.as_str().into(),
            OutputKind::Json(j) => serde_json::to_string(j)
                .map_err(|err| error!(?err, "failed to serialize tool to json"))
                .unwrap_or_default()
                .into(),
            OutputKind::Images(_) => "".into(),
            OutputKind::Mixed { text, .. } => text.as_str().into(), // Return the text part
        }
    }
}

#[non_exhaustive]
#[derive(Debug)]
pub enum OutputKind {
    Text(String),
    Json(serde_json::Value),
    Images(RichImageBlocks),
    Mixed { text: String, images: RichImageBlocks },
}

impl Default for OutputKind {
    fn default() -> Self {
        Self::Text(String::new())
    }
}

/// Performs tilde expansion and other required sanitization modifications for handling tool use
/// path arguments.
///
/// Required since path arguments are defined by the model.
#[allow(dead_code)]
pub fn sanitize_path_tool_arg(os: &Os, path: impl AsRef<Path>) -> PathBuf {
    let mut res = PathBuf::new();
    // Expand `~` only if it is the first part.
    let mut path = path.as_ref().components();
    match path.next() {
        Some(p) if p.as_os_str() == "~" => {
            res.push(os.env.home().unwrap_or_default());
        },
        Some(p) => res.push(p),
        None => return res,
    }
    for p in path {
        res.push(p);
    }
    // For testing scenarios, we need to make sure paths are appropriately handled in chroot test
    // file systems since they are passed directly from the model.
    os.fs.chroot_path(res)
}

/// Converts `path` to a relative path according to the current working directory `cwd`.
fn absolute_to_relative(cwd: impl AsRef<Path>, path: impl AsRef<Path>) -> Result<PathBuf> {
    let cwd = cwd.as_ref().canonicalize()?;
    let path = path.as_ref().canonicalize()?;
    let mut cwd_parts = cwd.components().peekable();
    let mut path_parts = path.components().peekable();

    // Skip common prefix
    while let (Some(a), Some(b)) = (cwd_parts.peek(), path_parts.peek()) {
        if a == b {
            cwd_parts.next();
            path_parts.next();
        } else {
            break;
        }
    }

    // ".." for any uncommon parts, then just append the rest of the path.
    let mut relative = PathBuf::new();
    for _ in cwd_parts {
        relative.push("..");
    }
    for part in path_parts {
        relative.push(part);
    }

    Ok(relative)
}

/// Small helper for formatting the path as a relative path, if able.
fn format_path(cwd: impl AsRef<Path>, path: impl AsRef<Path>) -> String {
    absolute_to_relative(cwd, path.as_ref())
        .map(|p| p.to_string_lossy().to_string())
        // If we have three consecutive ".." then it should probably just stay as an absolute path.
        .map(|p| {
            let three_up = format!("..{}..{}..", std::path::MAIN_SEPARATOR, std::path::MAIN_SEPARATOR);
            if p.starts_with(&three_up) {
                path.as_ref().to_string_lossy().to_string()
            } else {
                p
            }
        })
        .unwrap_or(path.as_ref().to_string_lossy().to_string())
}

fn supports_truecolor() -> bool {
    // Simple override to disable truecolor since shell_color doesn't use Context.
    !crate::util::env_var::is_truecolor_disabled()
        && shell_color::get_color_support().contains(shell_color::ColorSupport::TERM24BIT)
}

/// Helper function to display a purpose if available (for execute commands)
pub fn display_purpose(purpose: Option<&String>, updates: &mut impl Write) -> Result<()> {
    if let Some(purpose) = purpose {
        queue!(
            updates,
            style::Print("Purpose: "),
            style::Print(purpose),
            style::Print("\n"),
        )?;
    }
    Ok(())
}

/// Helper function to display tool usage information in the top right corner
/// This should be called by each tool within their queue_description method
///
/// # Parameters
/// * `tool` - The Tool enum containing all metadata (name, server info, etc.)
/// * `updates` - The output to write to
pub fn display_tool_use(tool: &Tool, updates: &mut impl Write) -> Result<()> {
    display_tool_use_with_args(tool, updates, None)
}

/// Helper function to display tool usage information with optional additional arguments
///
/// # Parameters
/// * `tool` - The Tool enum containing all metadata (name, server info, etc.)
/// * `updates` - The output to write to
/// * `additional_args` - Optional additional text to display after the tool name
pub fn display_tool_use_with_args(tool: &Tool, updates: &mut impl Write, additional_args: Option<&str>) -> Result<()> {
    // Check if this is a custom tool from an MCP server
    if let Tool::Custom(custom_tool) = tool {
        queue!(
            updates,
            StyledText::secondary_fg(),
            style::Print(" (from mcp server: "),
            style::Print(&custom_tool.server_name),
        )?;
        if let Some(args) = additional_args {
            queue!(updates, style::Print(", "), style::Print(args))?;
        }
        queue!(updates, style::Print(")"), StyledText::reset())?;
    } else {
        queue!(
            updates,
            StyledText::secondary_fg(),
            style::Print(" (using tool: "),
            style::Print(tool.display_name()),
        )?;
        if let Some(args) = additional_args {
            queue!(updates, style::Print(", "), style::Print(args))?;
        }
        queue!(updates, style::Print(")"), StyledText::reset())?;
    }
    Ok(())
}

/// Helper function to format function results with consistent styling
///
/// # Parameters
/// * `result` - The result text to display
/// * `updates` - The output to write to
/// * `is_error` - Whether this is an error message (changes formatting)
/// * `use_bullet` - Whether to use a bullet point instead of a tick/exclamation
pub fn queue_function_result(result: &str, updates: &mut impl Write, is_error: bool, use_bullet: bool) -> Result<()> {
    let lines = result.lines().collect::<Vec<_>>();

    // Determine symbol and color
    let (symbol, color) = match (is_error, use_bullet) {
        (true, _) => (super::ERROR_EXCLAMATION, theme().status.error),
        (false, true) => (super::TOOL_BULLET, theme().ui.secondary_text),
        (false, false) => (super::SUCCESS_TICK, theme().status.success),
    };

    queue!(updates, style::Print("\n"))?;

    // Print first line with symbol
    if let Some(first_line) = lines.first() {
        queue!(
            updates,
            style::SetForegroundColor(color),
            style::Print(symbol),
            StyledText::reset(),
            style::Print(first_line),
            style::Print("\n"),
        )?;
    }

    // Print remaining lines with indentation
    for line in lines.iter().skip(1) {
        queue!(
            updates,
            style::Print("   "), // 3 spaces for alignment
            style::Print(line),
            style::Print("\n"),
        )?;
    }

    Ok(())
}

/// Helper function to set up environment variables with user agent metadata for CloudTrail tracking
pub fn env_vars_with_user_agent(os: &Os) -> std::collections::HashMap<String, String> {
    let mut env_vars: std::collections::HashMap<String, String> = crate::util::env_var::get_all_env_vars().collect();

    // Set up additional metadata for the AWS CLI user agent
    let user_agent_metadata_value =
        format!("{USER_AGENT_APP_NAME} {USER_AGENT_VERSION_KEY}/{USER_AGENT_VERSION_VALUE}");

    // Check if the user agent metadata env var already exists using Os
    let existing_value = os.env.get(USER_AGENT_ENV_VAR).ok();

    // If the user agent metadata env var already exists, append to it, otherwise set it
    if let Some(existing_value) = existing_value {
        if !existing_value.is_empty() {
            env_vars.insert(
                USER_AGENT_ENV_VAR.to_string(),
                format!("{existing_value} {user_agent_metadata_value}"),
            );
        } else {
            env_vars.insert(USER_AGENT_ENV_VAR.to_string(), user_agent_metadata_value);
        }
    } else {
        env_vars.insert(USER_AGENT_ENV_VAR.to_string(), user_agent_metadata_value);
    }

    env_vars
}

#[cfg(test)]
mod tests {
    use std::path::MAIN_SEPARATOR;

    use super::*;
    use crate::os::ACTIVE_USER_HOME;

    #[tokio::test]
    async fn test_tilde_path_expansion() {
        let os = Os::new().await.unwrap();

        let actual = sanitize_path_tool_arg(&os, "~");
        let expected_home = os.env.home().unwrap_or_default();
        assert_eq!(actual, os.fs.chroot_path(&expected_home), "tilde should expand");
        let actual = sanitize_path_tool_arg(&os, "~/hello");
        assert_eq!(
            actual,
            os.fs.chroot_path(expected_home.join("hello")),
            "tilde should expand"
        );
        let actual = sanitize_path_tool_arg(&os, "/~");
        assert_eq!(
            actual,
            os.fs.chroot_path("/~"),
            "tilde should not expand when not the first component"
        );
    }

    #[tokio::test]
    async fn test_format_path() {
        async fn assert_paths(cwd: &str, path: &str, expected: &str) {
            let os = Os::new().await.unwrap();
            let cwd = sanitize_path_tool_arg(&os, cwd);
            let path = sanitize_path_tool_arg(&os, path);
            let fs = os.fs;
            fs.create_dir_all(&cwd).await.unwrap();
            fs.create_dir_all(&path).await.unwrap();

            let formatted = format_path(&cwd, &path);

            if Path::new(expected).is_absolute() {
                // If the expected path is relative, we need to ensure it is relative to the cwd.
                let expected = fs.chroot_path_str(expected);

                assert!(formatted == expected, "Expected '{formatted}' to be '{expected}'");

                return;
            }

            assert!(
                formatted.contains(expected),
                "Expected '{formatted}' to be '{expected}'"
            );
        }

        // Test relative path from src to Downloads (sibling directories)
        assert_paths(
            format!("{ACTIVE_USER_HOME}{MAIN_SEPARATOR}src").as_str(),
            format!("{ACTIVE_USER_HOME}{MAIN_SEPARATOR}Downloads").as_str(),
            format!("..{MAIN_SEPARATOR}Downloads").as_str(),
        )
        .await;

        // Test absolute path that should stay absolute (going up too many levels)
        assert_paths(
            format!("{ACTIVE_USER_HOME}{MAIN_SEPARATOR}projects{MAIN_SEPARATOR}some{MAIN_SEPARATOR}project").as_str(),
            format!("{ACTIVE_USER_HOME}{MAIN_SEPARATOR}other").as_str(),
            format!("{ACTIVE_USER_HOME}{MAIN_SEPARATOR}other").as_str(),
        )
        .await;
    }
}
