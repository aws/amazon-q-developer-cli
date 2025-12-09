use std::collections::{
    HashMap,
    VecDeque,
};
use std::fs;
use std::path::{
    Path,
    PathBuf,
};
use std::sync::LazyLock;

use clap::{
    Args,
    Subcommand,
};
use crossterm::style::{
    self,
    Attribute,
};
use crossterm::{
    execute,
    queue,
};
use regex::Regex;
use rmcp::model::{
    PromptMessage,
    PromptMessageContent,
    PromptMessageRole,
};
use serde::{
    Deserialize,
    Serialize,
};
use serde_json::Value;
use serde_yaml;
use thiserror::Error;
use unicode_width::UnicodeWidthStr;

use crate::cli::chat::cli::editor::open_editor_file;
use crate::cli::chat::tool_manager::PromptBundle;
use crate::cli::chat::{
    ChatError,
    ChatSession,
    ChatState,
};
use crate::mcp_client::McpClientError;
use crate::os::Os;
use crate::theme::StyledText;
use crate::util::paths::PathResolver;

/// Maximum allowed length for prompt names
const MAX_PROMPT_NAME_LENGTH: usize = 50;

/// Regex for validating prompt names (alphanumeric, hyphens, underscores only)
static PROMPT_NAME_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[a-zA-Z0-9_-]+$").expect("Invalid prompt name regex"));

/// Regex for detecting {{placeholder}} patterns in prompt content
static PLACEHOLDER_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\{\{(\w+)\}\}").expect("Invalid placeholder regex"));

#[derive(Debug, Error)]
pub enum GetPromptError {
    #[error("Prompt with name {0} does not exist")]
    PromptNotFound(String),
    #[error("Prompt {0} is offered by more than one server. Use one of the following {1}")]
    AmbiguousPrompt(String, String),
    #[error("Missing client")]
    MissingClient,
    #[error("Missing prompt name")]
    MissingPromptName,
    #[error("Missing prompt bundle")]
    MissingPromptInfo,
    #[error(transparent)]
    General(#[from] eyre::Report),
    #[error("Incorrect response type received")]
    IncorrectResponseType,
    #[error("Missing channel")]
    MissingChannel,
    #[error(transparent)]
    McpClient(#[from] McpClientError),
    #[error(transparent)]
    Service(#[from] rmcp::ServiceError),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

// ============================================================================
// ERROR TYPES
// ============================================================================

/// Error types for file-based prompt argument processing
#[derive(Debug, Error)]
enum PromptArgumentError {
    #[error("Missing required arguments: {0}")]
    MissingRequired(String),
    #[error("Invalid YAML frontmatter: {0}")]
    InvalidYaml(String),
    #[error("Invalid argument: {0}")]
    InvalidArgument(String),
}

impl From<PromptArgumentError> for GetPromptError {
    fn from(err: PromptArgumentError) -> Self {
        GetPromptError::General(err.into())
    }
}

// ============================================================================
// YAML FRONTMATTER PARSING
// ============================================================================

/// Metadata structure for file-based prompts with frontmatter
#[derive(Debug, Clone, Deserialize, Serialize)]
struct FilePromptMetadata {
    description: Option<String>,
    arguments: Option<Vec<rmcp::model::PromptArgument>>,
}

/// Parse YAML frontmatter from file content if it exists
fn parse_frontmatter_if_exists(content: &str) -> Result<(String, Option<FilePromptMetadata>), PromptArgumentError> {
    let (body, mut metadata) = if let Some(stripped) = content.strip_prefix("---\n") {
        if let Some(end_pos) = stripped.find("\n---\n") {
            let yaml_content = &stripped[..end_pos];
            let body = &stripped[end_pos + 5..]; // +5 for "\n---\n"

            let metadata: FilePromptMetadata =
                serde_yaml::from_str(yaml_content).map_err(|e| PromptArgumentError::InvalidYaml(e.to_string()))?;

            // Basic validation
            if let Some(args) = &metadata.arguments {
                for arg in args {
                    if arg.name.trim().is_empty() {
                        return Err(PromptArgumentError::InvalidArgument(
                            "Argument name cannot be empty".to_string(),
                        ));
                    }
                }
            }

            (body.to_string(), Some(metadata))
        } else {
            return Err(PromptArgumentError::InvalidYaml(
                "Frontmatter missing closing '---'".to_string(),
            ));
        }
    } else {
        (content.to_string(), None)
    };

    // Auto-detect arguments from {{}} placeholders in the body
    let mut detected_args = std::collections::HashSet::new();
    for cap in PLACEHOLDER_REGEX.captures_iter(&body) {
        if let Some(arg_name) = cap.get(1) {
            detected_args.insert(arg_name.as_str().to_string());
        }
    }

    // Create or update metadata with detected arguments
    if !detected_args.is_empty() {
        let auto_args: Vec<rmcp::model::PromptArgument> = detected_args
            .into_iter()
            .map(|name| {
                rmcp::model::PromptArgument {
                    name,
                    description: None,
                    required: Some(false), // Auto-detected args are optional by default
                    title: None,
                }
            })
            .collect();

        match metadata {
            Some(ref mut meta) => {
                // If frontmatter exists but no arguments declared, use auto-detected ones
                if meta.arguments.is_none() {
                    meta.arguments = Some(auto_args);
                }
            },
            None => {
                // No frontmatter, create metadata with auto-detected arguments
                metadata = Some(FilePromptMetadata {
                    description: None,
                    arguments: Some(auto_args),
                });
            },
        }
    }

    Ok((body, metadata))
}

// ============================================================================
// ARGUMENT VALIDATION & PROCESSING
// ============================================================================

/// Process file arguments with simple string replacement (positional only)
fn process_file_arguments(
    content: &str,
    arguments: &[rmcp::model::PromptArgument],
    provided_args: &[String],
) -> Result<String, PromptArgumentError> {
    // Validate argument names
    for arg in arguments {
        if arg.name.trim().is_empty() || !arg.name.chars().all(|c| c.is_alphanumeric() || c == '_') {
            return Err(PromptArgumentError::InvalidArgument(format!(
                "Invalid argument name: '{}'",
                arg.name
            )));
        }
    }

    // Find required arguments and check if they're provided
    let required_args: Vec<_> = arguments
        .iter()
        .enumerate()
        .filter(|(_, arg)| arg.required == Some(true))
        .collect();

    let missing_required: Vec<_> = required_args
        .iter()
        .filter(|(i, _)| *i >= provided_args.len())
        .map(|(_, arg)| arg.name.clone())
        .collect();

    if !missing_required.is_empty() {
        return Err(PromptArgumentError::MissingRequired(missing_required.join(", ")));
    }

    let mut result = content.to_string();
    let mut replaced_placeholders = std::collections::HashSet::new();

    // Replace provided arguments
    for (i, value) in provided_args.iter().enumerate() {
        if let Some(arg) = arguments.get(i) {
            let placeholder = format!("{{{{{}}}}}", arg.name);
            if result.contains(&placeholder) {
                result = result.replace(&placeholder, value);
                replaced_placeholders.insert(&arg.name);
            }
        }
    }

    // Check for unreplaced placeholders
    let remaining_placeholders: Vec<_> = PLACEHOLDER_REGEX
        .captures_iter(&result)
        .filter_map(|cap| cap.get(1))
        .map(|m| m.as_str())
        .collect();

    if !remaining_placeholders.is_empty() {
        return Err(PromptArgumentError::InvalidArgument(format!(
            "Unreplaced placeholders: {}",
            remaining_placeholders.join(", ")
        )));
    }

    Ok(result)
}

/// Represents a single prompt (local or global)
#[derive(Debug, Clone)]
struct Prompt {
    name: String,
    path: PathBuf,
}

impl std::fmt::Display for Prompt {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.name)
    }
}

impl Prompt {
    /// Create a new prompt with the given name in the specified directory
    fn new(name: &str, base_dir: PathBuf) -> Self {
        let path = base_dir.join(format!("{}.md", name));
        Self {
            name: name.to_string(),
            path,
        }
    }

    /// Check if the prompt file exists
    fn exists(&self) -> bool {
        self.path.exists()
    }

    /// Load the content of the prompt file
    fn load_content(&self) -> Result<String, GetPromptError> {
        fs::read_to_string(&self.path).map_err(GetPromptError::Io)
    }

    /// Save content to the prompt file
    fn save_content(&self, content: &str) -> Result<(), GetPromptError> {
        // Ensure parent directory exists
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent).map_err(GetPromptError::Io)?;
        }
        fs::write(&self.path, content).map_err(GetPromptError::Io)
    }

    /// Delete the prompt file
    fn delete(&self) -> Result<(), GetPromptError> {
        fs::remove_file(&self.path).map_err(GetPromptError::Io)
    }
}

/// Represents both local and global prompts for a given name
#[derive(Debug)]
struct Prompts {
    local: Prompt,
    global: Prompt,
}

impl Prompts {
    /// Create a new Prompts instance for the given name
    fn new(name: &str, os: &Os) -> Result<Self, GetPromptError> {
        let resolver = PathResolver::new(os);
        let local_dir = resolver
            .workspace()
            .prompts_dir()
            .map_err(|e| GetPromptError::General(e.into()))?;
        let global_dir = resolver
            .global()
            .prompts_dir()
            .map_err(|e| GetPromptError::General(e.into()))?;

        Ok(Self {
            local: Prompt::new(name, local_dir),
            global: Prompt::new(name, global_dir),
        })
    }

    /// Check if local prompt overrides a global one (both local and global exist)
    fn has_local_override(&self) -> bool {
        self.local.exists() && self.global.exists()
    }

    /// Find and load existing prompt content (local takes priority)
    fn load_existing(&self) -> Result<Option<(String, PathBuf, Option<FilePromptMetadata>)>, GetPromptError> {
        if self.local.exists() {
            let content = self.local.load_content()?;
            let (body, metadata) = parse_frontmatter_if_exists(&content)?;
            Ok(Some((body, self.local.path.clone(), metadata)))
        } else if self.global.exists() {
            let content = self.global.load_content()?;
            let (body, metadata) = parse_frontmatter_if_exists(&content)?;
            Ok(Some((body, self.global.path.clone(), metadata)))
        } else {
            Ok(None)
        }
    }

    /// Get all available prompt names from both directories
    fn get_available_names(os: &Os) -> Result<Vec<String>, GetPromptError> {
        let resolver = PathResolver::new(os);
        let mut prompt_names = std::collections::HashSet::new();

        // Helper function to collect prompt names from a directory
        let collect_from_dir =
            |dir: PathBuf, names: &mut std::collections::HashSet<String>| -> Result<(), GetPromptError> {
                if dir.exists() {
                    for entry in fs::read_dir(&dir)? {
                        let entry = entry?;
                        let path = entry.path();
                        if path.is_file() && path.extension().and_then(|s| s.to_str()) == Some("md") {
                            if let Some(file_stem) = path.file_stem().and_then(|s| s.to_str()) {
                                let prompt = Prompt::new(file_stem, dir.clone());
                                names.insert(prompt.name);
                            }
                        }
                    }
                }
                Ok(())
            };

        // Check global prompts
        if let Ok(global_dir) = resolver.global().prompts_dir() {
            collect_from_dir(global_dir, &mut prompt_names)?;
        }

        // Check local prompts
        if let Ok(local_dir) = resolver.workspace().prompts_dir() {
            collect_from_dir(local_dir, &mut prompt_names)?;
        }

        Ok(prompt_names.into_iter().collect())
    }
}

/// Validate prompt name to ensure it's safe and follows naming conventions
fn validate_prompt_name(name: &str) -> Result<(), String> {
    // Check for empty name
    if name.trim().is_empty() {
        return Err("Prompt name cannot be empty. Please provide a valid name for your prompt.".to_string());
    }

    // Check length limit
    if name.len() > MAX_PROMPT_NAME_LENGTH {
        return Err(format!(
            "Prompt name must be {} characters or less. Current length: {} characters.",
            MAX_PROMPT_NAME_LENGTH,
            name.len()
        ));
    }

    // Check for valid characters using regex (alphanumeric, hyphens, underscores only)
    if !PROMPT_NAME_REGEX.is_match(name) {
        return Err("Prompt name can only contain letters, numbers, hyphens (-), and underscores (_). Special characters, spaces, and path separators are not allowed.".to_string());
    }

    Ok(())
}

/// Formats a prompt description for display in the prompts list.
///
/// Handles None and empty descriptions by returning a placeholder.
/// For multi-line descriptions, only the first line is returned.
fn format_description(description: Option<&String>) -> String {
    match description {
        Some(desc) if !desc.trim().is_empty() => {
            // Take only the first line for multi-line descriptions
            desc.lines().next().unwrap_or("").to_string()
        },
        _ => "(no description)".to_string(),
    }
}

/// Truncates a description string to the specified maximum length.
///
/// If truncation is needed, adds "..." ellipsis and trims trailing whitespace
/// to ensure clean formatting.
fn truncate_description(text: &str, max_length: usize) -> String {
    if text.len() <= max_length {
        text.to_string()
    } else {
        let truncated = &text[..max_length.saturating_sub(3)];
        format!("{}...", truncated.trim_end())
    }
}

/// Represents parsed MCP error details for generating user-friendly messages.
#[derive(Debug)]
struct McpErrorDetails {
    code: String,
    message: String,
    path: Vec<String>,
}

/// Parses MCP error JSON to extract all validation errors for user-friendly messages.
///
/// Attempts to extract JSON error details from MCP server error strings to provide
/// more specific and user-friendly error messages for all validation failures.
///
/// # Arguments
/// * `error_str` - The raw error string from the MCP server
///
/// # Returns
/// * `Vec<McpErrorDetails>` containing all parsed errors, empty if parsing fails
fn parse_all_mcp_error_details(error_str: &str) -> Vec<McpErrorDetails> {
    // Try to extract JSON from error string - MCP errors often contain JSON in the message
    let json_start = match error_str.find('[') {
        Some(pos) => pos,
        None => return Vec::new(),
    };
    let json_end = match error_str.rfind(']') {
        Some(pos) => pos + 1,
        None => return Vec::new(),
    };
    let json_str = &error_str[json_start..json_end];

    let error_array: Vec<Value> = match serde_json::from_str(json_str) {
        Ok(array) => array,
        Err(_) => return Vec::new(),
    };

    error_array
        .iter()
        .filter_map(|error_val| {
            let error_obj = error_val.as_object()?;
            let code = error_obj.get("code")?.as_str()?;
            let message = error_obj.get("message")?.as_str().unwrap_or("");
            let path = match error_obj.get("path").and_then(|p| p.as_array()) {
                Some(path_array) => path_array
                    .iter()
                    .filter_map(|v| v.as_str())
                    .map(|s| s.to_string())
                    .collect(),
                None => Vec::new(),
            };

            Some(McpErrorDetails {
                code: code.to_string(),
                message: message.to_string(),
                path,
            })
        })
        .collect()
}

/// Handles MCP -32602 (Invalid params) errors with user-friendly messages.
///
/// Parses the error details and displays appropriate error messages based on the
/// specific type of invalid parameter error (missing args, invalid values, etc.).
fn handle_mcp_invalid_params_error(
    name: &str,
    error_str: &str,
    prompts: &HashMap<String, Vec<PromptBundle>>,
    session: &mut ChatSession,
) -> Result<(), ChatError> {
    let all_errors = parse_all_mcp_error_details(error_str);

    if !all_errors.is_empty() {
        // Check if this is a missing required arguments error
        if all_errors.len() == 1
            && all_errors[0].code == "invalid_type"
            && all_errors[0].message == "Required"
            && all_errors[0].path.is_empty()
        {
            display_missing_args_error(name, prompts, session)?;
            return Ok(());
        }

        // Display validation errors
        queue!(
            session.stderr,
            style::Print("\n"),
            StyledText::warning_fg(),
            style::Print("Error: Invalid arguments for prompt '"),
            StyledText::brand_fg(),
            style::Print(name),
            StyledText::warning_fg(),
            style::Print("':\n"),
            StyledText::reset(),
        )?;

        for error in &all_errors {
            if !error.path.is_empty() {
                let param_name = error.path.join(".");
                queue!(
                    session.stderr,
                    style::Print("  - "),
                    StyledText::brand_fg(),
                    style::Print(&param_name),
                    StyledText::warning_fg(),
                    style::Print(": "),
                    StyledText::reset(),
                    style::Print(&error.message),
                    style::Print("\n"),
                )?;
            } else {
                queue!(
                    session.stderr,
                    style::Print("  - "),
                    StyledText::reset(),
                    style::Print(&error.message),
                    style::Print("\n"),
                )?;
            }
        }

        queue!(
            session.stderr,
            style::Print("\n"),
            StyledText::secondary_fg(),
            style::Print("Use '/prompts details "),
            style::Print(name),
            style::Print("' for usage information."),
            StyledText::reset(),
            style::Print("\n"),
        )?;

        execute!(session.stderr)?;
    } else {
        // Fallback for unparsable -32602 errors
        queue!(
            session.stderr,
            style::Print("\n"),
            StyledText::warning_fg(),
            style::Print("Error: Invalid arguments for prompt '"),
            StyledText::brand_fg(),
            style::Print(name),
            StyledText::warning_fg(),
            style::Print("'. Use '/prompts details "),
            style::Print(name),
            style::Print("' for usage information."),
            StyledText::reset(),
            style::Print("\n"),
        )?;
        execute!(session.stderr)?;
    }
    Ok(())
}

/// Handles MCP -32603 (Internal error) errors with user-friendly messages.
///
/// Attempts to parse structured error information from the server response
/// and displays it in a user-friendly format.
fn handle_mcp_internal_error(name: &str, error_str: &str, session: &mut ChatSession) -> Result<(), ChatError> {
    // Try to parse JSON error response
    if let Some(json_start) = error_str.find('{') {
        if let Some(json_end) = error_str.rfind('}') {
            let json_str = &error_str[json_start..=json_end];
            if let Ok(error_obj) = serde_json::from_str::<serde_json::Value>(json_str) {
                if let Some(error_field) = error_obj.get("error") {
                    let message = error_field
                        .get("message")
                        .and_then(|m| m.as_str())
                        .unwrap_or("Internal error");

                    queue!(
                        session.stderr,
                        style::Print("\n"),
                        StyledText::error_fg(),
                        style::Print("Error: "),
                        style::Print(message),
                        StyledText::reset(),
                        style::Print("\n"),
                    )?;

                    if let Some(data) = error_field.get("data") {
                        if let Ok(data_str) = serde_json::to_string_pretty(data) {
                            queue!(
                                session.stderr,
                                style::Print("Details: "),
                                style::Print(data_str),
                                style::Print("\n"),
                            )?;
                        }
                    }
                    execute!(session.stderr)?;
                    return Ok(());
                }
            }
        }
    }

    // Fallback for unparsable internal errors
    queue!(
        session.stderr,
        style::Print("\n"),
        StyledText::error_fg(),
        style::Print("Error: MCP server internal error while processing prompt '"),
        StyledText::brand_fg(),
        style::Print(name),
        StyledText::error_fg(),
        style::Print("'."),
        StyledText::reset(),
        style::Print("\n"),
    )?;
    execute!(session.stderr)?;
    Ok(())
}

/// Displays a user-friendly error message for missing required arguments.
///
/// Shows usage information and lists all required and optional arguments
/// with descriptions when available.
fn display_missing_args_error(
    prompt_name: &str,
    prompts: &HashMap<String, Vec<PromptBundle>>,
    session: &mut ChatSession,
) -> Result<(), ChatError> {
    queue!(
        session.stderr,
        style::Print("\n"),
        StyledText::warning_fg(),
        style::Print("Error: Missing required arguments for prompt "),
        StyledText::brand_fg(),
        style::Print(prompt_name),
        StyledText::reset(),
        style::Print("\n\n"),
    )?;

    // Extract the actual prompt name from server/prompt format if needed
    let actual_prompt_name = if let Some((_, name)) = prompt_name.split_once('/') {
        name
    } else {
        prompt_name
    };

    if let Some(bundles) = prompts.get(actual_prompt_name) {
        if let Some(bundle) = bundles.first() {
            if let Some(args) = &bundle.prompt_get.arguments {
                let required_args: Vec<_> = args.iter().filter(|arg| arg.required == Some(true)).collect();
                let optional_args: Vec<_> = args.iter().filter(|arg| arg.required != Some(true)).collect();

                // Usage line
                queue!(
                    session.stderr,
                    style::Print("Usage: "),
                    StyledText::brand_fg(),
                    style::Print("@"),
                    style::Print(prompt_name),
                )?;

                for arg in &required_args {
                    queue!(
                        session.stderr,
                        style::Print(" <"),
                        style::Print(&arg.name),
                        style::Print(">"),
                    )?;
                }
                for arg in &optional_args {
                    queue!(
                        session.stderr,
                        style::Print(" ["),
                        style::Print(&arg.name),
                        style::Print("]"),
                    )?;
                }

                queue!(session.stderr, StyledText::reset(), style::Print("\n"),)?;

                if !args.is_empty() {
                    queue!(session.stderr, style::Print("\nArguments:\n"),)?;

                    // Show required arguments first
                    for arg in required_args {
                        queue!(
                            session.stderr,
                            style::Print("  "),
                            StyledText::error_fg(),
                            style::Print("(required) "),
                            StyledText::brand_fg(),
                            style::Print(&arg.name),
                            StyledText::reset(),
                        )?;
                        if let Some(desc) = &arg.description {
                            if !desc.trim().is_empty() {
                                queue!(session.stderr, style::Print(" - "), style::Print(desc),)?;
                            }
                        }
                        queue!(session.stderr, style::Print("\n"))?;
                    }

                    // Then show optional arguments
                    for arg in optional_args {
                        queue!(
                            session.stderr,
                            style::Print("  "),
                            StyledText::secondary_fg(),
                            style::Print("(optional) "),
                            StyledText::brand_fg(),
                            style::Print(&arg.name),
                            StyledText::reset(),
                        )?;
                        if let Some(desc) = &arg.description {
                            if !desc.trim().is_empty() {
                                queue!(session.stderr, style::Print(" - "), style::Print(desc),)?;
                            }
                        }
                        queue!(session.stderr, style::Print("\n"))?;
                    }
                }
            }
        }
    }

    execute!(session.stderr)?;
    Ok(())
}

/// Command-line arguments for prompt operations
#[deny(missing_docs)]
#[derive(Debug, PartialEq, Args)]
#[command(color = clap::ColorChoice::Always,
    before_long_help = color_print::cstr!{"Prompts are reusable templates that help you quickly access common workflows and tasks. 
These templates are provided by the mcp servers you have installed and configured.

To actually retrieve a prompt, directly start with the following command (without prepending /prompt get):
  <em>@<<prompt name>> [arg]</em>                             <black!>Retrieve prompt specified</black!>
Or if you prefer the long way:
  <em>/prompts get <<prompt name>> [arg]</em>                 <black!>Retrieve prompt specified</black!>"
})]
pub struct PromptsArgs {
    #[command(subcommand)]
    subcommand: Option<PromptsSubcommand>,
}

impl PromptsArgs {
    pub async fn execute(self, os: &Os, session: &mut ChatSession) -> Result<ChatState, ChatError> {
        let search_word = match &self.subcommand {
            Some(PromptsSubcommand::List { search_word }) => search_word.clone(),
            _ => None,
        };

        if let Some(subcommand) = self.subcommand {
            if matches!(
                subcommand,
                PromptsSubcommand::Get { .. }
                    | PromptsSubcommand::Details { .. }
                    | PromptsSubcommand::Create { .. }
                    | PromptsSubcommand::Edit { .. }
                    | PromptsSubcommand::Remove { .. }
            ) {
                return subcommand.execute(os, session).await;
            }
        }

        let terminal_width = session.terminal_width();
        let prompts = session.conversation.tool_manager.list_prompts().await?;

        // Get available prompt names
        let prompt_names = Prompts::get_available_names(os).map_err(|e| ChatError::Custom(e.to_string().into()))?;

        let mut longest_name = "";

        // Update longest_name to include local prompts
        for name in &prompt_names {
            if name.contains(search_word.as_deref().unwrap_or("")) && name.len() > longest_name.len() {
                longest_name = name;
            }
        }
        let mut prompts_by_server: Vec<_> = prompts
            .iter()
            .fold(
                HashMap::<&String, Vec<&PromptBundle>>::new(),
                |mut acc, (prompt_name, bundles)| {
                    if prompt_name.contains(search_word.as_deref().unwrap_or("")) {
                        if prompt_name.len() > longest_name.len() {
                            longest_name = prompt_name.as_str();
                        }
                        for bundle in bundles {
                            acc.entry(&bundle.server_name)
                                .and_modify(|b| b.push(bundle))
                                .or_insert(vec![bundle]);
                        }
                    }
                    acc
                },
            )
            .into_iter()
            .collect();
        prompts_by_server.sort_by_key(|(server_name, _)| server_name.as_str());

        // Calculate positions for three-column layout: Prompt | Description | Arguments
        let prompt_col_width = (UnicodeWidthStr::width(longest_name) + 4).max(20); // Min 20 chars for "Prompt"
        let description_col_width = 41; // Fixed width for descriptions
        let description_pos = prompt_col_width;
        let arguments_pos = description_pos + description_col_width;

        // Add usage guidance at the top
        queue!(
            session.stderr,
            style::Print("\n"),
            style::SetAttribute(Attribute::Bold),
            style::Print("Usage: "),
            StyledText::reset_attributes(),
            style::Print("You can use a prompt by typing "),
            style::SetAttribute(Attribute::Bold),
            StyledText::success_fg(),
            style::Print("'@<prompt name> [...args]'"),
            StyledText::reset(),
            StyledText::reset_attributes(),
            style::Print("\n\n"),
        )?;

        // Print header with three columns
        queue!(
            session.stderr,
            style::Print("\n"),
            style::SetAttribute(Attribute::Bold),
            style::Print("Prompt"),
            StyledText::reset_attributes(),
            style::Print({
                let padding = description_pos.saturating_sub(UnicodeWidthStr::width("Prompt"));
                " ".repeat(padding)
            }),
            style::SetAttribute(Attribute::Bold),
            style::Print("Description"),
            StyledText::reset_attributes(),
            style::Print({
                let padding = arguments_pos.saturating_sub(description_pos + UnicodeWidthStr::width("Description"));
                " ".repeat(padding)
            }),
            style::SetAttribute(Attribute::Bold),
            style::Print("Arguments (* = required)"),
            StyledText::reset_attributes(),
            style::Print("\n"),
            style::Print(format!("{}\n", "▔".repeat(terminal_width))),
        )?;

        // Display prompts by category
        let filtered_names: Vec<_> = prompt_names
            .iter()
            .filter(|name| name.contains(search_word.as_deref().unwrap_or("")))
            .collect();

        if !filtered_names.is_empty() {
            // Separate global and local prompts for display
            let _global_dir = PathResolver::new(os).global().prompts_dir().ok();
            let _local_dir = PathResolver::new(os).workspace().prompts_dir().ok();

            let mut global_prompts = Vec::new();
            let mut local_prompts = Vec::new();
            let mut overridden_globals = Vec::new();

            for name in &filtered_names {
                // Use the Prompts struct to check for conflicts
                if let Ok(prompts) = Prompts::new(name, os) {
                    let (local_exists, global_exists) = (prompts.local.exists(), prompts.global.exists());

                    if global_exists {
                        global_prompts.push(name);
                    }

                    if local_exists {
                        local_prompts.push(name);
                        // Check for overrides using has_local_override method
                        if global_exists {
                            overridden_globals.push(name);
                        }
                    }
                }
            }

            if !global_prompts.is_empty() {
                queue!(
                    session.stderr,
                    style::SetAttribute(Attribute::Bold),
                    style::Print(&format!("Global ({}):", crate::util::paths::global::PROMPTS_DIR)),
                    StyledText::reset_attributes(),
                    style::Print("\n"),
                )?;
                Self::display_file_prompts_table(&global_prompts, os, session, description_pos, arguments_pos, false)?;
            }

            if !local_prompts.is_empty() {
                if !global_prompts.is_empty() {
                    queue!(session.stderr, style::Print("\n"))?;
                }
                queue!(
                    session.stderr,
                    style::SetAttribute(Attribute::Bold),
                    style::Print(&format!("Local ({}):", crate::util::paths::workspace::PROMPTS_DIR)),
                    StyledText::reset_attributes(),
                    style::Print("\n"),
                )?;
                Self::display_file_prompts_table(&local_prompts, os, session, description_pos, arguments_pos, true)?;
            }
        }

        for (i, (server_name, bundles)) in prompts_by_server.iter_mut().enumerate() {
            bundles.sort_by_key(|bundle| &bundle.prompt_get.name);

            if i > 0 || !filtered_names.is_empty() {
                queue!(session.stderr, style::Print("\n"))?;
            }
            queue!(
                session.stderr,
                style::SetAttribute(Attribute::Bold),
                style::Print(server_name),
                style::Print(" (MCP):"),
                StyledText::reset_attributes(),
                style::Print("\n"),
            )?;

            for bundle in bundles {
                let prompt_name = &bundle.prompt_get.name;
                let description = format_description(bundle.prompt_get.description.as_ref());
                let truncated_desc = truncate_description(&description, 40);

                // Print prompt name
                queue!(session.stderr, style::Print("- "), style::Print(prompt_name),)?;

                // Print description with proper alignment
                let name_width = UnicodeWidthStr::width(prompt_name.as_str()) + 2; // +2 for "- "
                let description_padding = description_pos.saturating_sub(name_width);
                queue!(
                    session.stderr,
                    style::Print(" ".repeat(description_padding)),
                    StyledText::secondary_fg(),
                    style::Print(&truncated_desc),
                    StyledText::reset(),
                )?;

                // Print arguments if they exist
                if let Some(args) = bundle.prompt_get.arguments.as_ref() {
                    if !args.is_empty() {
                        let current_pos = description_pos + UnicodeWidthStr::width(truncated_desc.as_str());
                        let arguments_padding = arguments_pos.saturating_sub(current_pos);
                        queue!(session.stderr, style::Print(" ".repeat(arguments_padding)))?;

                        for (i, arg) in args.iter().enumerate() {
                            queue!(
                                session.stderr,
                                StyledText::secondary_fg(),
                                style::Print(match arg.required {
                                    Some(true) => format!("{}*", arg.name),
                                    _ => arg.name.clone(),
                                }),
                                StyledText::reset(),
                                style::Print(if i < args.len() - 1 { ", " } else { "" }),
                            )?;
                        }
                    }
                }
                queue!(session.stderr, style::Print("\n"))?;
            }
        }

        Ok(ChatState::PromptUser {
            skip_printing_tools: true,
        })
    }

    fn display_file_prompts_table(
        prompt_names: &[&&String],
        os: &Os,
        session: &mut ChatSession,
        description_pos: usize,
        arguments_pos: usize,
        show_overrides: bool,
    ) -> Result<(), ChatError> {
        let overridden_globals: Vec<_> = if show_overrides {
            prompt_names
                .iter()
                .filter(|name| {
                    if let Ok(prompts) = Prompts::new(name, os) {
                        prompts.local.exists() && prompts.global.exists()
                    } else {
                        false
                    }
                })
                .map(|s| s.as_str())
                .collect()
        } else {
            Vec::new()
        };

        for name in prompt_names {
            // Load metadata for this prompt
            let metadata = if let Ok(prompts) = Prompts::new(name, os) {
                if let Ok(Some((_content, _path, metadata))) = prompts.load_existing() {
                    metadata
                } else {
                    None
                }
            } else {
                None
            };

            // Display prompt name
            queue!(session.stderr, style::Print("- "), style::Print(name))?;

            // Add override indicator if needed
            if show_overrides && overridden_globals.contains(&name.as_str()) {
                queue!(
                    session.stderr,
                    StyledText::success_fg(),
                    style::Print(" (overrides global)"),
                    StyledText::reset(),
                )?;
            }

            // Calculate current position and add description
            let name_width = UnicodeWidthStr::width(name.as_str()) + 2; // +2 for "- "
            let override_width = if show_overrides && overridden_globals.contains(&name.as_str()) {
                UnicodeWidthStr::width(" (overrides global)")
            } else {
                0
            };
            let current_pos = name_width + override_width;

            if let Some(ref meta) = metadata {
                let desc_width = if let Some(ref desc) = meta.description {
                    let truncated_desc = truncate_description(desc, 40);
                    let description_padding = description_pos.saturating_sub(current_pos);
                    queue!(
                        session.stderr,
                        style::Print(" ".repeat(description_padding)),
                        StyledText::secondary_fg(),
                        style::Print(&truncated_desc),
                        StyledText::reset(),
                    )?;
                    UnicodeWidthStr::width(truncated_desc.as_str())
                } else {
                    // No description, just add padding to description column
                    let description_padding = description_pos.saturating_sub(current_pos);
                    queue!(session.stderr, style::Print(" ".repeat(description_padding)))?;
                    0
                };

                // Display arguments if they exist
                if let Some(ref args) = meta.arguments {
                    if !args.is_empty() {
                        let arguments_padding = arguments_pos.saturating_sub(description_pos + desc_width);
                        queue!(session.stderr, style::Print(" ".repeat(arguments_padding)))?;

                        let mut arg_parts = Vec::new();
                        for arg in args {
                            if arg.required.unwrap_or(false) {
                                arg_parts.push(format!("{}*", arg.name));
                            } else {
                                arg_parts.push(arg.name.clone());
                            }
                        }
                        let args_display = arg_parts.join(", ");
                        queue!(
                            session.stderr,
                            StyledText::secondary_fg(),
                            style::Print(&args_display),
                            StyledText::reset(),
                        )?;
                    }
                }
            }

            queue!(session.stderr, style::Print("\n"))?;
        }
        Ok(())
    }

    pub fn subcommand_name(&self) -> Option<&'static str> {
        self.subcommand.as_ref().map(|s| s.name())
    }
}

/// Subcommands for prompt operations
#[deny(missing_docs)]
#[derive(Clone, Debug, PartialEq, Subcommand)]
pub enum PromptsSubcommand {
    /// List available prompts from a tool or show all available prompt
    List {
        /// Optional search word to filter prompts
        search_word: Option<String>,
    },
    /// Show detailed information about a specific prompt
    Details {
        /// Name of the prompt to show details for
        name: String,
    },
    /// Get a specific prompt by name
    Get {
        #[arg(long, hide = true)]
        /// Original input string (hidden)
        orig_input: Option<String>,
        /// Name of the prompt to retrieve
        name: String,
        /// Optional arguments for the prompt
        arguments: Option<Vec<String>>,
    },
    /// Create a new local prompt
    Create {
        /// Name of the prompt to create
        #[arg(short = 'n', long)]
        name: String,
        /// Content of the prompt (if not provided, opens editor)
        #[arg(long)]
        content: Option<String>,
        /// Create in global directory instead of local
        #[arg(long)]
        global: bool,
    },
    /// Edit an existing local prompt
    Edit {
        /// Name of the prompt to edit
        name: String,
        /// Edit global prompt instead of local
        #[arg(long)]
        global: bool,
    },
    /// Remove an existing local prompt
    Remove {
        /// Name of the prompt to remove
        name: String,
        /// Remove global prompt instead of local
        #[arg(long)]
        global: bool,
    },
}

impl PromptsSubcommand {
    pub async fn execute(self, os: &Os, session: &mut ChatSession) -> Result<ChatState, ChatError> {
        match self {
            PromptsSubcommand::Details { name } => Self::execute_details(name, os, session).await,
            PromptsSubcommand::Get {
                orig_input,
                name,
                arguments,
            } => Self::execute_get(os, session, orig_input, name, arguments).await,
            PromptsSubcommand::Create { name, content, global } => {
                Self::execute_create(os, session, name, content, global).await
            },
            PromptsSubcommand::Edit { name, global } => Self::execute_edit(os, session, name, global).await,
            PromptsSubcommand::Remove { name, global } => Self::execute_remove(os, session, name, global).await,
            PromptsSubcommand::List { .. } => {
                unreachable!("List has already been parsed out at this point");
            },
        }
    }

    async fn execute_details(name: String, os: &Os, session: &mut ChatSession) -> Result<ChatState, ChatError> {
        // First try to find file-based prompt (global or local)
        let file_prompts = Prompts::new(&name, os).map_err(|e| ChatError::Custom(e.to_string().into()))?;
        if let Some((content, source, metadata)) = file_prompts
            .load_existing()
            .map_err(|e| ChatError::Custom(e.to_string().into()))?
        {
            // Check if there's also an MCP prompt with the same name (conflict)
            let mcp_prompts = session.conversation.tool_manager.list_prompts().await?;
            if mcp_prompts.contains_key(&name) {
                // Show conflict warning
                queue!(
                    session.stderr,
                    style::Print("\n"),
                    StyledText::warning_fg(),
                    style::Print("⚠ Warning: Both file-based and MCP prompts named '"),
                    StyledText::brand_fg(),
                    style::Print(&name),
                    StyledText::warning_fg(),
                    style::Print("' exist. Showing file-based prompt.\n"),
                    style::Print("To see MCP prompt, specify server: "),
                    StyledText::brand_fg(),
                    style::Print("/prompts details <server>/"),
                    style::Print(&name),
                    StyledText::reset(),
                    style::Print("\n"),
                )?;
                execute!(session.stderr)?;
            }

            // Display file-based prompt details
            Self::display_file_prompt_details(&name, &content, &source, &metadata, session)?;
            execute!(session.stderr, style::Print("\n"))?;
            return Ok(ChatState::PromptUser {
                skip_printing_tools: true,
            });
        }

        // If not found as file-based prompt, try MCP prompts
        let prompts = session.conversation.tool_manager.list_prompts().await?;

        // Parse server/prompt format if provided
        let (server_filter, prompt_name) = if let Some((server, prompt)) = name.split_once('/') {
            (Some(server), prompt)
        } else {
            (None, name.as_str())
        };

        // Find matching prompts
        let matching_bundles: Vec<&PromptBundle> = prompts
            .get(prompt_name)
            .map(|bundles| {
                if let Some(server) = server_filter {
                    bundles.iter().filter(|b| b.server_name == server).collect()
                } else {
                    bundles.iter().collect()
                }
            })
            .unwrap_or_default();

        match matching_bundles.len() {
            0 => {
                queue!(
                    session.stderr,
                    style::Print("\n"),
                    StyledText::warning_fg(),
                    style::Print("Prompt "),
                    StyledText::brand_fg(),
                    style::Print(&name),
                    StyledText::warning_fg(),
                    style::Print(" not found. Use "),
                    StyledText::brand_fg(),
                    style::Print("/prompts list"),
                    StyledText::warning_fg(),
                    style::Print(" to see available prompts.\n"),
                    StyledText::reset(),
                )?;
            },
            1 => {
                let bundle = matching_bundles[0];
                Self::display_prompt_details(bundle, session)?;
            },
            _ => {
                let alt_names: Vec<String> = matching_bundles
                    .iter()
                    .map(|b| format!("- @{}/{}", b.server_name, prompt_name))
                    .collect();
                let alt_msg = format!("\n{}\n", alt_names.join("\n"));

                queue!(
                    session.stderr,
                    style::Print("\n"),
                    StyledText::warning_fg(),
                    style::Print("Prompt "),
                    StyledText::brand_fg(),
                    style::Print(&name),
                    StyledText::warning_fg(),
                    style::Print(" is ambiguous. Use one of the following:"),
                    StyledText::brand_fg(),
                    style::Print(alt_msg),
                    StyledText::reset(),
                )?;
            },
        }

        execute!(session.stderr, style::Print("\n"))?;
        Ok(ChatState::PromptUser {
            skip_printing_tools: true,
        })
    }

    fn display_prompt_details(bundle: &PromptBundle, session: &mut ChatSession) -> Result<(), ChatError> {
        let prompt = &bundle.prompt_get;
        let terminal_width = session.terminal_width();

        // Display header
        queue!(
            session.stderr,
            style::Print("\n"),
            style::SetAttribute(Attribute::Bold),
            style::Print("Prompt Details"),
            StyledText::reset_attributes(),
            style::Print("\n"),
            style::Print("▔".repeat(terminal_width)),
            style::Print("\n\n"),
        )?;

        // Display basic information
        queue!(
            session.stderr,
            style::SetAttribute(Attribute::Bold),
            style::Print("Name: "),
            StyledText::reset_attributes(),
            style::Print(&prompt.name),
            style::Print("\n"),
            style::SetAttribute(Attribute::Bold),
            style::Print("Server: "),
            StyledText::reset_attributes(),
            style::Print(&bundle.server_name),
            style::Print("\n\n"),
        )?;

        // Display description
        queue!(
            session.stderr,
            style::SetAttribute(Attribute::Bold),
            style::Print("Description:"),
            StyledText::reset_attributes(),
            style::Print("\n"),
        )?;

        match &prompt.description {
            Some(desc) if !desc.trim().is_empty() => {
                for line in desc.lines() {
                    queue!(
                        session.stderr,
                        style::Print("  "),
                        style::Print(line),
                        style::Print("\n")
                    )?;
                }
            },
            _ => {
                queue!(
                    session.stderr,
                    StyledText::secondary_fg(),
                    style::Print("  (no description available)"),
                    StyledText::reset(),
                    style::Print("\n"),
                )?;
            },
        }

        // Display usage example
        queue!(
            session.stderr,
            style::Print("\n"),
            style::SetAttribute(Attribute::Bold),
            style::Print("Usage: "),
            StyledText::reset_attributes(),
            StyledText::brand_fg(),
            style::Print("@"),
            style::Print(&prompt.name),
        )?;

        if let Some(args) = &prompt.arguments {
            for arg in args {
                match arg.required {
                    Some(true) => {
                        queue!(
                            session.stderr,
                            style::Print(" <"),
                            style::Print(&arg.name),
                            style::Print(">"),
                        )?;
                    },
                    _ => {
                        queue!(
                            session.stderr,
                            style::Print(" ["),
                            style::Print(&arg.name),
                            style::Print("]"),
                        )?;
                    },
                }
            }
        }

        queue!(session.stderr, StyledText::reset(), style::Print("\n"),)?;

        // Display arguments
        queue!(
            session.stderr,
            style::Print("\n"),
            style::SetAttribute(Attribute::Bold),
            style::Print("Arguments:"),
            StyledText::reset_attributes(),
            style::Print("\n"),
        )?;

        if let Some(args) = &prompt.arguments {
            if args.is_empty() {
                queue!(
                    session.stderr,
                    StyledText::secondary_fg(),
                    style::Print("  (no arguments)"),
                    StyledText::reset(),
                    style::Print("\n"),
                )?;
            } else {
                let required_args: Vec<_> = args.iter().filter(|arg| arg.required == Some(true)).collect();
                let optional_args: Vec<_> = args.iter().filter(|arg| arg.required != Some(true)).collect();

                // Show required arguments first
                for arg in required_args {
                    queue!(
                        session.stderr,
                        style::Print("  "),
                        StyledText::error_fg(),
                        style::Print("(required) "),
                        StyledText::brand_fg(),
                        style::Print(&arg.name),
                        StyledText::reset(),
                    )?;

                    // Show argument description if available
                    if let Some(desc) = &arg.description {
                        if !desc.trim().is_empty() {
                            queue!(session.stderr, style::Print(" - "), style::Print(desc),)?;
                        }
                    }

                    queue!(session.stderr, style::Print("\n"))?;
                }

                // Then show optional arguments
                for arg in optional_args {
                    queue!(
                        session.stderr,
                        style::Print("  "),
                        StyledText::secondary_fg(),
                        style::Print("(optional) "),
                        StyledText::brand_fg(),
                        style::Print(&arg.name),
                        StyledText::reset(),
                    )?;

                    // Show argument description if available
                    if let Some(desc) = &arg.description {
                        if !desc.trim().is_empty() {
                            queue!(session.stderr, style::Print(" - "), style::Print(desc),)?;
                        }
                    }

                    queue!(session.stderr, style::Print("\n"))?;
                }
            }
        } else {
            queue!(
                session.stderr,
                StyledText::secondary_fg(),
                style::Print("  (no arguments)"),
                StyledText::reset(),
                style::Print("\n"),
            )?;
        }

        Ok(())
    }

    fn display_file_prompt_details(
        name: &str,
        content: &str,
        source: &Path,
        metadata: &Option<FilePromptMetadata>,
        session: &mut ChatSession,
    ) -> Result<(), ChatError> {
        let terminal_width = session.terminal_width();

        // Display header
        queue!(
            session.stderr,
            style::Print("\n"),
            style::SetAttribute(Attribute::Bold),
            style::Print("Prompt Details"),
            StyledText::reset_attributes(),
            style::Print("\n"),
            style::Print("▔".repeat(terminal_width)),
            style::Print("\n\n"),
        )?;

        // Display basic information
        queue!(
            session.stderr,
            style::SetAttribute(Attribute::Bold),
            style::Print("Name: "),
            StyledText::reset_attributes(),
            style::Print(name),
            style::Print("\n"),
            style::SetAttribute(Attribute::Bold),
            style::Print("Source: "),
            StyledText::reset_attributes(),
            StyledText::secondary_fg(),
            style::Print(source.display().to_string()),
            StyledText::reset(),
            style::Print("\n\n"),
        )?;

        // Display description if available
        if let Some(meta) = metadata {
            if let Some(desc) = &meta.description {
                queue!(
                    session.stderr,
                    style::SetAttribute(Attribute::Bold),
                    style::Print("Description:"),
                    style::SetAttribute(Attribute::Reset),
                    style::Print("\n"),
                    style::Print("  "),
                    style::Print(desc),
                    style::Print("\n\n"),
                )?;
            }
        }

        // Display usage example
        queue!(
            session.stderr,
            style::SetAttribute(Attribute::Bold),
            style::Print("Usage: "),
            StyledText::reset_attributes(),
            StyledText::success_fg(),
            style::Print("@"),
            style::Print(name),
        )?;

        // Show arguments in usage if available
        if let Some(meta) = metadata {
            if let Some(args) = &meta.arguments {
                for arg in args {
                    match arg.required {
                        Some(true) => {
                            queue!(
                                session.stderr,
                                style::Print(" <"),
                                style::Print(&arg.name),
                                style::Print(">")
                            )?;
                        },
                        _ => {
                            queue!(
                                session.stderr,
                                style::Print(" ["),
                                style::Print(&arg.name),
                                style::Print("]")
                            )?;
                        },
                    }
                }
            }
        }

        queue!(session.stderr, StyledText::reset(), style::Print("\n\n"),)?;

        // Display arguments section if available
        if let Some(meta) = metadata {
            if let Some(args) = &meta.arguments {
                queue!(
                    session.stderr,
                    style::SetAttribute(Attribute::Bold),
                    style::Print("Arguments:"),
                    style::SetAttribute(Attribute::Reset),
                    style::Print("\n"),
                )?;

                for arg in args {
                    queue!(
                        session.stderr,
                        style::Print("  "),
                        if arg.required == Some(true) {
                            StyledText::error_fg()
                        } else {
                            StyledText::secondary_fg()
                        },
                        style::Print(if arg.required == Some(true) {
                            "(required) "
                        } else {
                            "(optional) "
                        }),
                        StyledText::brand_fg(),
                        style::Print(&arg.name),
                        StyledText::reset(),
                    )?;

                    if let Some(desc) = &arg.description {
                        queue!(session.stderr, style::Print(" - "), style::Print(desc))?;
                    }
                    queue!(session.stderr, style::Print("\n"))?;
                }
                queue!(session.stderr, style::Print("\n"))?;
            }
        }

        // Display usage example
        queue!(
            session.stderr,
            style::SetAttribute(Attribute::Bold),
            style::Print("Usage: "),
            StyledText::reset_attributes(),
            StyledText::success_fg(),
            style::Print("@"),
            style::Print(name),
            StyledText::reset(),
            style::Print("\n\n"),
        )?;

        // Display content preview (first few lines)
        queue!(
            session.stderr,
            style::SetAttribute(Attribute::Bold),
            style::Print("Content Preview:"),
            StyledText::reset_attributes(),
            style::Print("\n"),
        )?;

        let lines: Vec<&str> = content.lines().collect();
        let preview_lines = lines.iter().take(5);
        for line in preview_lines {
            queue!(
                session.stderr,
                StyledText::secondary_fg(),
                style::Print("  "),
                style::Print(line),
                StyledText::reset(),
                style::Print("\n"),
            )?;
        }

        if lines.len() > 5 {
            queue!(
                session.stderr,
                StyledText::secondary_fg(),
                style::Print("  ... ("),
                style::Print((lines.len() - 5).to_string()),
                style::Print(" more lines)"),
                StyledText::reset(),
                style::Print("\n"),
            )?;
        }

        Ok(())
    }

    async fn execute_get(
        os: &Os,
        session: &mut ChatSession,
        orig_input: Option<String>,
        name: String,
        arguments: Option<Vec<String>>,
    ) -> Result<ChatState, ChatError> {
        // First try to find prompt (global or local)
        let prompts = Prompts::new(&name, os).map_err(|e| ChatError::Custom(e.to_string().into()))?;
        if let Some((content, _, metadata)) = prompts
            .load_existing()
            .map_err(|e| ChatError::Custom(e.to_string().into()))?
        {
            // Check if there's also an MCP prompt with the same name (conflict)
            let mcp_prompts = session.conversation.tool_manager.list_prompts().await?;
            if mcp_prompts.contains_key(&name) {
                // Show conflict warning
                queue!(
                    session.stderr,
                    style::Print("\n"),
                    StyledText::warning_fg(),
                    style::Print("⚠ Warning: Both file-based and MCP prompts named '"),
                    StyledText::brand_fg(),
                    style::Print(&name),
                    StyledText::warning_fg(),
                    style::Print("' exist. Using file-based prompt.\n"),
                    style::Print("To use MCP prompt, specify server: "),
                    StyledText::brand_fg(),
                    style::Print("@<server>/"),
                    style::Print(&name),
                    StyledText::reset(),
                    style::Print("\n"),
                )?;
                execute!(session.stderr)?;
            }

            // Process arguments if metadata and arguments exist
            let final_content = if let (Some(meta), Some(args)) = (&metadata, &arguments) {
                if let Some(prompt_args) = &meta.arguments {
                    process_file_arguments(&content, prompt_args, args)
                        .map_err(|e| ChatError::Custom(e.to_string().into()))?
                } else {
                    content
                }
            } else {
                content
            };

            // Display the file-based prompt content to the user
            display_file_prompt_content(&name, &final_content, session)?;

            // Handle local prompt
            session.pending_prompts.clear();

            // Create a PromptMessage from the processed prompt content
            let prompt_message = PromptMessage {
                role: PromptMessageRole::User,
                content: PromptMessageContent::Text { text: final_content },
            };
            session.pending_prompts.push_back(prompt_message);

            return Ok(ChatState::HandleInput {
                input: orig_input.unwrap_or_default(),
            });
        }

        // If not found locally, try MCP prompts
        let prompts = match session
            .conversation
            .tool_manager
            .get_prompt(name.clone(), arguments)
            .await
        {
            Ok(resp) => {
                // Display the fetched prompt content to the user
                display_prompt_content(&name, &resp.messages, session)?;
                resp
            },
            Err(e) => {
                match e {
                    GetPromptError::AmbiguousPrompt(prompt_name, alt_msg) => {
                        queue!(
                            session.stderr,
                            style::Print("\n"),
                            StyledText::warning_fg(),
                            style::Print("Prompt "),
                            StyledText::brand_fg(),
                            style::Print(prompt_name),
                            StyledText::warning_fg(),
                            style::Print(" is ambiguous. Use one of the following "),
                            StyledText::brand_fg(),
                            style::Print(alt_msg),
                            StyledText::reset(),
                        )?;
                    },
                    GetPromptError::PromptNotFound(prompt_name) => {
                        queue!(
                            session.stderr,
                            style::Print("\n"),
                            StyledText::warning_fg(),
                            style::Print("Prompt "),
                            StyledText::brand_fg(),
                            style::Print(prompt_name),
                            StyledText::warning_fg(),
                            style::Print(" not found. Use "),
                            StyledText::brand_fg(),
                            style::Print("/prompts list"),
                            StyledText::warning_fg(),
                            style::Print(" to see available prompts.\n"),
                            StyledText::reset(),
                        )?;
                    },
                    GetPromptError::McpClient(_) | GetPromptError::Service(_) => {
                        let error_str = e.to_string();

                        // Check for specific MCP error codes in the error string
                        if error_str.contains("-32602") {
                            // Invalid params error
                            let prompts_list = session
                                .conversation
                                .tool_manager
                                .list_prompts()
                                .await
                                .unwrap_or_default();
                            handle_mcp_invalid_params_error(&name, &error_str, &prompts_list, session)?;
                        } else if error_str.contains("-32603") {
                            // Internal server error
                            handle_mcp_internal_error(&name, &error_str, session)?;
                        } else {
                            // Other MCP errors - show generic message
                            queue!(
                                session.stderr,
                                style::Print("\n"),
                                StyledText::warning_fg(),
                                style::Print("Error: Failed to execute prompt "),
                                StyledText::brand_fg(),
                                style::Print(&name),
                                StyledText::warning_fg(),
                                style::Print(". "),
                                style::Print(&error_str),
                                StyledText::reset(),
                                style::Print("\n"),
                            )?;
                            execute!(session.stderr)?;
                        }
                    },
                    _ => return Err(ChatError::Custom(e.to_string().into())),
                }
                execute!(session.stderr, style::Print("\n"))?;
                return Ok(ChatState::PromptUser {
                    skip_printing_tools: true,
                });
            },
        };

        session.pending_prompts.clear();
        session.pending_prompts.append(&mut VecDeque::from(prompts.messages));

        Ok(ChatState::HandleInput {
            input: orig_input.unwrap_or_default(),
        })
    }

    async fn execute_create(
        os: &Os,
        session: &mut ChatSession,
        name: String,
        content: Option<String>,
        global: bool,
    ) -> Result<ChatState, ChatError> {
        // Create prompts instance and validate name
        let mut prompts = Prompts::new(&name, os).map_err(|e| ChatError::Custom(e.to_string().into()))?;

        if let Err(validation_error) = validate_prompt_name(&name) {
            queue!(
                session.stderr,
                style::Print("\n"),
                StyledText::error_fg(),
                style::Print("❌ Invalid prompt name: "),
                style::Print(validation_error),
                style::Print("\n"),
                StyledText::secondary_fg(),
                style::Print("Valid names contain only letters, numbers, hyphens, and underscores (1-50 characters)\n"),
                StyledText::reset(),
            )?;
            return Ok(ChatState::PromptUser {
                skip_printing_tools: true,
            });
        }

        // Check if prompt already exists in target location
        let (local_exists, global_exists) = (prompts.local.exists(), prompts.global.exists());
        let target_exists = if global { global_exists } else { local_exists };

        if target_exists {
            let location = if global { "global" } else { "local" };
            queue!(
                session.stderr,
                style::Print("\n"),
                StyledText::warning_fg(),
                style::Print("Prompt "),
                StyledText::brand_fg(),
                style::Print(&name),
                StyledText::warning_fg(),
                style::Print(" already exists in "),
                style::Print(location),
                style::Print(" directory. Use "),
                StyledText::brand_fg(),
                style::Print("/prompts edit "),
                style::Print(&name),
                if global {
                    style::Print(" --global")
                } else {
                    style::Print("")
                },
                StyledText::warning_fg(),
                style::Print(" to modify it.\n"),
                StyledText::reset(),
            )?;
            return Ok(ChatState::PromptUser {
                skip_printing_tools: true,
            });
        }

        // Check if creating this prompt will cause or involve a conflict
        let opposite_exists = if global { local_exists } else { global_exists };

        if prompts.has_local_override() || opposite_exists {
            let (existing_scope, _creating_scope, override_message) = if !global {
                (
                    "global",
                    "local",
                    "Creating this local prompt will override the global one.",
                )
            } else {
                (
                    "local",
                    "global",
                    "The local prompt will continue to override this global one.",
                )
            };

            queue!(
                session.stderr,
                style::Print("\n"),
                StyledText::warning_fg(),
                style::Print("⚠ Warning: A "),
                style::Print(existing_scope),
                style::Print(" prompt named '"),
                StyledText::brand_fg(),
                style::Print(&name),
                StyledText::warning_fg(),
                style::Print("' already exists.\n"),
                style::Print(override_message),
                style::Print("\n"),
                StyledText::reset(),
            )?;

            // Flush stderr to ensure the warning is displayed before asking for input
            execute!(session.stderr)?;

            // Ask for user confirmation
            let user_input = match crate::util::input("Do you want to continue? (y/n): ", None) {
                Ok(input) => input.trim().to_lowercase(),
                Err(_) => {
                    queue!(
                        session.stderr,
                        style::Print("\n"),
                        StyledText::success_fg(),
                        style::Print("✓ Prompt creation cancelled.\n"),
                        StyledText::reset(),
                    )?;
                    return Ok(ChatState::PromptUser {
                        skip_printing_tools: true,
                    });
                },
            };

            if user_input != "y" && user_input != "yes" {
                queue!(
                    session.stderr,
                    style::Print("\n"),
                    StyledText::success_fg(),
                    style::Print("✓ Prompt creation cancelled.\n"),
                    StyledText::reset(),
                )?;
                return Ok(ChatState::PromptUser {
                    skip_printing_tools: true,
                });
            }
        }

        match content {
            Some(content) => {
                // Write the prompt file with provided content
                let target_prompt = if global {
                    &mut prompts.global
                } else {
                    &mut prompts.local
                };

                target_prompt
                    .save_content(&content)
                    .map_err(|e| ChatError::Custom(e.to_string().into()))?;

                let location = if global { "global" } else { "local" };
                queue!(
                    session.stderr,
                    style::Print("\n"),
                    StyledText::success_fg(),
                    style::Print("✓ Created "),
                    style::Print(location),
                    style::Print(" prompt "),
                    StyledText::brand_fg(),
                    style::Print(&name),
                    StyledText::success_fg(),
                    style::Print(" at "),
                    StyledText::secondary_fg(),
                    style::Print(target_prompt.path.display().to_string()),
                    StyledText::reset(),
                    style::Print("\n\n"),
                )?;
            },
            None => {
                // Create file with default template and open editor
                let default_content = "# Enter your prompt content here\n\nDescribe what this prompt should do...";
                let target_prompt = if global {
                    &mut prompts.global
                } else {
                    &mut prompts.local
                };

                target_prompt
                    .save_content(default_content)
                    .map_err(|e| ChatError::Custom(e.to_string().into()))?;

                queue!(
                    session.stderr,
                    style::Print("\n"),
                    StyledText::success_fg(),
                    style::Print("Opening editor to create prompt content...\n"),
                    StyledText::reset(),
                )?;

                // Try to open the editor
                match open_editor_file(&target_prompt.path) {
                    Ok(()) => {
                        let location = if global { "global" } else { "local" };
                        queue!(
                            session.stderr,
                            StyledText::success_fg(),
                            style::Print("✓ Created "),
                            style::Print(location),
                            style::Print(" prompt "),
                            StyledText::brand_fg(),
                            style::Print(&name),
                            StyledText::success_fg(),
                            style::Print(" at "),
                            StyledText::secondary_fg(),
                            style::Print(target_prompt.path.display().to_string()),
                            StyledText::reset(),
                            style::Print("\n\n"),
                        )?;
                    },
                    Err(err) => {
                        queue!(
                            session.stderr,
                            StyledText::error_fg(),
                            style::Print("Error opening editor: "),
                            style::Print(err.to_string()),
                            StyledText::reset(),
                            style::Print("\n"),
                            StyledText::secondary_fg(),
                            style::Print("Tip: You can edit this file directly: "),
                            style::Print(target_prompt.path.display().to_string()),
                            StyledText::reset(),
                            style::Print("\n\n"),
                        )?;
                    },
                }
            },
        };

        Ok(ChatState::PromptUser {
            skip_printing_tools: true,
        })
    }

    async fn execute_edit(
        os: &Os,
        session: &mut ChatSession,
        name: String,
        global: bool,
    ) -> Result<ChatState, ChatError> {
        // Validate prompt name
        if let Err(validation_error) = validate_prompt_name(&name) {
            queue!(
                session.stderr,
                style::Print("\n"),
                StyledText::error_fg(),
                style::Print("❌ Invalid prompt name: "),
                style::Print(validation_error),
                style::Print("\n"),
                StyledText::reset(),
            )?;
            return Ok(ChatState::PromptUser {
                skip_printing_tools: true,
            });
        }

        let prompts = Prompts::new(&name, os).map_err(|e| ChatError::Custom(e.to_string().into()))?;
        let (local_exists, global_exists) = (prompts.local.exists(), prompts.global.exists());

        // Find the target prompt to edit
        let target_prompt = if global {
            if !global_exists {
                queue!(
                    session.stderr,
                    style::Print("\n"),
                    StyledText::warning_fg(),
                    style::Print("Global prompt "),
                    StyledText::brand_fg(),
                    style::Print(&name),
                    StyledText::warning_fg(),
                    style::Print(" not found.\n"),
                    StyledText::reset(),
                )?;
                return Ok(ChatState::PromptUser {
                    skip_printing_tools: true,
                });
            }
            &prompts.global
        } else if local_exists {
            &prompts.local
        } else if global_exists {
            // Found global prompt, but user wants to edit local
            queue!(
                session.stderr,
                style::Print("\n"),
                StyledText::warning_fg(),
                style::Print("Local prompt "),
                StyledText::brand_fg(),
                style::Print(&name),
                StyledText::warning_fg(),
                style::Print(" not found, but global version exists.\n"),
                style::Print("Use "),
                StyledText::brand_fg(),
                style::Print("/prompts edit "),
                style::Print(&name),
                style::Print(" --global"),
                StyledText::warning_fg(),
                style::Print(" to edit the global version, or\n"),
                style::Print("use "),
                StyledText::brand_fg(),
                style::Print("/prompts create "),
                style::Print(&name),
                StyledText::warning_fg(),
                style::Print(" to create a local override.\n"),
                StyledText::reset(),
            )?;
            return Ok(ChatState::PromptUser {
                skip_printing_tools: true,
            });
        } else {
            queue!(
                session.stderr,
                style::Print("\n"),
                StyledText::warning_fg(),
                style::Print("Prompt "),
                StyledText::brand_fg(),
                style::Print(&name),
                StyledText::warning_fg(),
                style::Print(" not found.\n"),
                StyledText::reset(),
            )?;
            return Ok(ChatState::PromptUser {
                skip_printing_tools: true,
            });
        };

        let location = if global { "global" } else { "local" };
        queue!(
            session.stderr,
            style::Print("\n"),
            StyledText::success_fg(),
            style::Print("Opening editor for "),
            style::Print(location),
            style::Print(" prompt: "),
            StyledText::brand_fg(),
            style::Print(&name),
            StyledText::reset(),
            style::Print("\n"),
            StyledText::secondary_fg(),
            style::Print("File: "),
            style::Print(target_prompt.path.display().to_string()),
            StyledText::reset(),
            style::Print("\n\n"),
        )?;

        // Try to open the editor
        match open_editor_file(&target_prompt.path) {
            Ok(()) => {
                queue!(
                    session.stderr,
                    StyledText::success_fg(),
                    style::Print("✓ Prompt edited successfully.\n\n"),
                    StyledText::reset(),
                )?;
            },
            Err(err) => {
                queue!(
                    session.stderr,
                    StyledText::error_fg(),
                    style::Print("Error opening editor: "),
                    style::Print(err.to_string()),
                    StyledText::reset(),
                    style::Print("\n"),
                    StyledText::secondary_fg(),
                    style::Print("Tip: You can edit this file directly: "),
                    style::Print(target_prompt.path.display().to_string()),
                    StyledText::reset(),
                    style::Print("\n\n"),
                )?;
            },
        }

        Ok(ChatState::PromptUser {
            skip_printing_tools: true,
        })
    }

    async fn execute_remove(
        os: &Os,
        session: &mut ChatSession,
        name: String,
        global: bool,
    ) -> Result<ChatState, ChatError> {
        let prompts = Prompts::new(&name, os).map_err(|e| ChatError::Custom(e.to_string().into()))?;
        let (local_exists, global_exists) = (prompts.local.exists(), prompts.global.exists());

        // Find the target prompt to remove
        let target_prompt = if global {
            if !global_exists {
                queue!(
                    session.stderr,
                    style::Print("\n"),
                    StyledText::warning_fg(),
                    style::Print("Global prompt "),
                    StyledText::brand_fg(),
                    style::Print(&name),
                    StyledText::warning_fg(),
                    style::Print(" not found.\n"),
                    StyledText::reset(),
                )?;
                return Ok(ChatState::PromptUser {
                    skip_printing_tools: true,
                });
            }
            &prompts.global
        } else if local_exists {
            &prompts.local
        } else if global_exists {
            queue!(
                session.stderr,
                style::Print("\n"),
                StyledText::warning_fg(),
                style::Print("Local prompt "),
                StyledText::brand_fg(),
                style::Print(&name),
                StyledText::warning_fg(),
                style::Print(" not found, but global version exists.\n"),
                style::Print("Use "),
                StyledText::brand_fg(),
                style::Print("/prompts remove "),
                style::Print(&name),
                style::Print(" --global"),
                StyledText::warning_fg(),
                style::Print(" to remove the global version.\n"),
                StyledText::reset(),
            )?;
            return Ok(ChatState::PromptUser {
                skip_printing_tools: true,
            });
        } else {
            queue!(
                session.stderr,
                style::Print("\n"),
                StyledText::warning_fg(),
                style::Print("Prompt "),
                StyledText::brand_fg(),
                style::Print(&name),
                StyledText::warning_fg(),
                style::Print(" not found.\n"),
                StyledText::reset(),
            )?;
            return Ok(ChatState::PromptUser {
                skip_printing_tools: true,
            });
        };

        let location = if global { "global" } else { "local" };

        // Ask for confirmation
        queue!(
            session.stderr,
            style::Print("\n"),
            StyledText::warning_fg(),
            style::Print("⚠ Warning: This will permanently remove the "),
            style::Print(location),
            style::Print(" prompt '"),
            StyledText::brand_fg(),
            style::Print(&name),
            StyledText::warning_fg(),
            style::Print("'.\n"),
            StyledText::secondary_fg(),
            style::Print("File: "),
            style::Print(target_prompt.path.display().to_string()),
            StyledText::reset(),
            style::Print("\n"),
        )?;

        // Flush stderr to ensure the warning is displayed before asking for input
        execute!(session.stderr)?;

        // Ask for user confirmation
        let user_input = match crate::util::input("Are you sure you want to remove this prompt? (y/n): ", None) {
            Ok(input) => input.trim().to_lowercase(),
            Err(_) => {
                queue!(
                    session.stderr,
                    style::Print("\n"),
                    StyledText::success_fg(),
                    style::Print("✓ Removal cancelled.\n"),
                    StyledText::reset(),
                )?;
                return Ok(ChatState::PromptUser {
                    skip_printing_tools: true,
                });
            },
        };

        if user_input != "y" && user_input != "yes" {
            queue!(
                session.stderr,
                style::Print("\n"),
                StyledText::success_fg(),
                style::Print("✓ Removal cancelled.\n"),
                StyledText::reset(),
            )?;
            return Ok(ChatState::PromptUser {
                skip_printing_tools: true,
            });
        }

        // Remove the file
        match target_prompt.delete() {
            Ok(()) => {
                queue!(
                    session.stderr,
                    style::Print("\n"),
                    StyledText::success_fg(),
                    style::Print("✓ Removed "),
                    style::Print(location),
                    style::Print(" prompt "),
                    StyledText::brand_fg(),
                    style::Print(&name),
                    StyledText::success_fg(),
                    style::Print(" successfully.\n\n"),
                    StyledText::reset(),
                )?;
            },
            Err(err) => {
                queue!(
                    session.stderr,
                    style::Print("\n"),
                    StyledText::error_fg(),
                    style::Print("Error deleting prompt: "),
                    style::Print(err.to_string()),
                    StyledText::reset(),
                    style::Print("\n\n"),
                )?;
            },
        }

        Ok(ChatState::PromptUser {
            skip_printing_tools: true,
        })
    }

    pub fn name(&self) -> &'static str {
        match self {
            PromptsSubcommand::List { .. } => "list",
            PromptsSubcommand::Details { .. } => "details",
            PromptsSubcommand::Get { .. } => "get",
            PromptsSubcommand::Create { .. } => "create",
            PromptsSubcommand::Edit { .. } => "edit",
            PromptsSubcommand::Remove { .. } => "remove",
        }
    }
}

/// Display fetched prompt content to the user before AI processing
fn display_prompt_content(
    _prompt_name: &str,
    messages: &[PromptMessage],
    session: &mut ChatSession,
) -> Result<(), ChatError> {
    fn stringify_prompt_message_content(content: &PromptMessageContent) -> String {
        match content {
            PromptMessageContent::Text { text } => text.clone(),
            PromptMessageContent::Image { image } => image.raw.data.clone(),
            PromptMessageContent::Resource { resource } => match &resource.raw.resource {
                rmcp::model::ResourceContents::TextResourceContents {
                    uri, mime_type, text, ..
                } => {
                    let mime_type = mime_type.as_deref().unwrap_or("unknown");
                    format!("Text resource of uri: {uri}, mime_type: {mime_type}, text: {text}")
                },
                rmcp::model::ResourceContents::BlobResourceContents { uri, mime_type, .. } => {
                    let mime_type = mime_type.as_deref().unwrap_or("unknown");
                    format!("Blob resource of uri: {uri}, mime_type: {mime_type}")
                },
            },
            PromptMessageContent::ResourceLink { link } => {
                format!("Resource link with uri: {}, name: {}", link.raw.uri, link.raw.name)
            },
        }
    }

    queue!(session.stderr, style::Print("\n"),)?;

    for message in messages {
        let content = stringify_prompt_message_content(&message.content);
        if !content.trim().is_empty() {
            queue!(
                session.stderr,
                StyledText::secondary_fg(),
                style::Print(content),
                StyledText::reset(),
                style::Print("\n"),
            )?;
        }
    }

    queue!(session.stderr, style::Print("\n"))?;
    execute!(session.stderr)?;
    Ok(())
}

/// Display file-based prompt content to the user before AI processing
fn display_file_prompt_content(_prompt_name: &str, content: &str, session: &mut ChatSession) -> Result<(), ChatError> {
    queue!(session.stderr, style::Print("\n"),)?;

    if !content.trim().is_empty() {
        queue!(
            session.stderr,
            StyledText::secondary_fg(),
            style::Print(content),
            StyledText::reset(),
            style::Print("\n"),
        )?;
    }

    queue!(session.stderr, style::Print("\n"))?;
    execute!(session.stderr)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::fs;

    use rmcp::model::PromptArgument;
    use tempfile::TempDir;

    use super::*;

    #[test]
    fn test_parse_frontmatter_if_exists() {
        // Test with frontmatter
        let content_with_frontmatter = r#"---
description: "Test prompt"
arguments:
  - name: "city"
    description: "City name"
    required: true
---

Get weather for {{city}}."#;

        let (body, metadata) = parse_frontmatter_if_exists(content_with_frontmatter).unwrap();
        assert_eq!(body.trim(), "Get weather for {{city}}.");
        assert!(metadata.is_some());
        let meta = metadata.unwrap();
        assert_eq!(meta.description, Some("Test prompt".to_string()));
        assert!(meta.arguments.is_some());

        // Test without frontmatter
        let content_without = "Simple prompt content";
        let (body, metadata) = parse_frontmatter_if_exists(content_without).unwrap();
        assert_eq!(body, "Simple prompt content");
        assert!(metadata.is_none());

        // Test invalid frontmatter
        let invalid_frontmatter = "---\ninvalid: yaml: content\n---\nBody";
        assert!(parse_frontmatter_if_exists(invalid_frontmatter).is_err());
    }

    #[test]
    fn test_auto_detect_arguments_from_placeholders() {
        let content = "Write a {{language}} function that uses {{framework}} with {{style}} coding practices.";
        let (body, metadata) = parse_frontmatter_if_exists(content).unwrap();

        assert_eq!(body, content);
        assert!(metadata.is_some());

        let meta = metadata.unwrap();
        assert!(meta.description.is_none());
        assert!(meta.arguments.is_some());

        let args = meta.arguments.unwrap();
        assert_eq!(args.len(), 3);

        let arg_names: std::collections::HashSet<_> = args.iter().map(|a| &a.name).collect();
        assert!(arg_names.contains(&"language".to_string()));
        assert!(arg_names.contains(&"framework".to_string()));
        assert!(arg_names.contains(&"style".to_string()));

        // All auto-detected args should be optional
        for arg in &args {
            assert_eq!(arg.required, Some(false));
            assert!(arg.description.is_none());
        }
    }

    #[test]
    fn test_plain_text_with_placeholders() {
        let content =
            "Today's date in {{format}} format is: {{date}}\n\nPlease use {{timezone}} timezone for the calculation.";
        let (body, metadata) = parse_frontmatter_if_exists(content).unwrap();

        assert_eq!(body, content);
        assert!(metadata.is_some());

        let meta = metadata.unwrap();
        assert!(meta.description.is_none()); // Description should be blank/None
        assert!(meta.arguments.is_some());

        let args = meta.arguments.unwrap();
        assert_eq!(args.len(), 3);

        let arg_names: std::collections::HashSet<_> = args.iter().map(|a| &a.name).collect();
        assert!(arg_names.contains(&"format".to_string()));
        assert!(arg_names.contains(&"date".to_string()));
        assert!(arg_names.contains(&"timezone".to_string()));

        // All should be optional with no description
        for arg in &args {
            assert_eq!(arg.required, Some(false));
            assert!(arg.description.is_none());
        }
    }

    #[test]
    fn test_process_file_arguments() {
        let content = "Weather for {{city}} in {{units}}.";
        let args = vec![
            PromptArgument {
                name: "city".to_string(),
                description: Some("City name".to_string()),
                required: Some(true),
                title: None,
            },
            PromptArgument {
                name: "units".to_string(),
                description: Some("Temperature units".to_string()),
                required: Some(false),
                title: None,
            },
        ];
        let provided = vec!["Tokyo".to_string(), "celsius".to_string()];

        let result = process_file_arguments(content, &args, &provided);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "Weather for Tokyo in celsius.");

        // Test missing required arguments
        let insufficient_args = vec![];
        let result = process_file_arguments(content, &args, &insufficient_args);
        assert!(result.is_err());
        if let Err(e) = result {
            assert!(e.to_string().contains("Missing required arguments: city"));
        }

        // Test invalid argument name
        let invalid_args = vec![PromptArgument {
            name: "invalid name!".to_string(),
            description: None,
            required: Some(false),
            title: None,
        }];
        let result = process_file_arguments("test", &invalid_args, &vec![]);
        assert!(result.is_err());
        if let Err(e) = result {
            assert!(e.to_string().contains("Invalid argument name"));
        }
    }

    #[test]
    fn test_file_prompt_with_arguments() {
        let temp_dir = TempDir::new().unwrap();
        let local_dir = temp_dir.path().join(".amazonq/prompts");

        let content = r#"---
description: "Weather prompt"
arguments:
  - name: "city"
    description: "City name"
    required: true
---

Weather for {{city}}."#;

        create_prompt_file(&local_dir, "weather", content);

        // Test that file can be parsed
        let file_content = fs::read_to_string(local_dir.join("weather.md")).unwrap();
        let (body, metadata) = parse_frontmatter_if_exists(&file_content).unwrap();

        assert!(metadata.is_some());
        let meta = metadata.unwrap();
        assert_eq!(meta.description, Some("Weather prompt".to_string()));
        assert!(meta.arguments.is_some());
        assert_eq!(body.trim(), "Weather for {{city}}.");
    }

    #[test]
    fn test_backward_compatibility() {
        let temp_dir = TempDir::new().unwrap();
        let local_dir = temp_dir.path().join(".amazonq/prompts");

        // Test file without frontmatter (existing behavior)
        create_prompt_file(&local_dir, "simple", "Simple prompt content");

        let file_content = fs::read_to_string(local_dir.join("simple.md")).unwrap();
        let (body, metadata) = parse_frontmatter_if_exists(&file_content).unwrap();

        assert_eq!(body, "Simple prompt content");
        assert!(metadata.is_none());
    }

    #[test]
    fn test_frontmatter_edge_cases() {
        // Test empty frontmatter - needs proper format with newlines
        let empty_frontmatter = "---\n\n---\nContent";
        let (body, metadata) = parse_frontmatter_if_exists(empty_frontmatter).unwrap();
        assert_eq!(body.trim(), "Content");
        assert!(metadata.is_some());

        // Test frontmatter without closing - should error
        let incomplete = "---\ndescription: test\nContent";
        let result = parse_frontmatter_if_exists(incomplete);
        assert!(result.is_err());

        // Test content with placeholders but no frontmatter - should auto-detect
        let no_frontmatter = "Hello {{name}}, welcome to {{place}}!";
        let (body, metadata) = parse_frontmatter_if_exists(no_frontmatter).unwrap();
        assert_eq!(body, no_frontmatter);
        assert!(metadata.is_some());
        let meta = metadata.unwrap();
        assert!(meta.arguments.is_some());
        assert_eq!(meta.arguments.unwrap().len(), 2);
    }

    #[test]
    fn test_debug_actual_file() {
        let content = "# Get date \n\nShow only date in {{time}}\nformat: YYYYMMDD\n\nDon't show anything except date";
        let (body, metadata) = parse_frontmatter_if_exists(content).unwrap();

        println!("Body: {:?}", body);
        println!("Metadata: {:?}", metadata);

        assert_eq!(body, content);
        assert!(metadata.is_some());

        let meta = metadata.unwrap();
        assert!(meta.description.is_none());
        assert!(meta.arguments.is_some());

        let args = meta.arguments.unwrap();
        assert_eq!(args.len(), 1);
        assert_eq!(args[0].name, "time");
    }

    #[test]
    fn test_argument_validation() {
        // Test empty argument name
        let invalid_content = r#"---
description: "Test prompt"
arguments:
  - name: ""
    description: "Empty name"
    required: true
---
Content"#;

        let result = parse_frontmatter_if_exists(invalid_content);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("Argument name cannot be empty")
        );
    }

    fn create_prompt_file(dir: &PathBuf, name: &str, content: &str) {
        fs::create_dir_all(dir).unwrap();
        fs::write(dir.join(format!("{}.md", name)), content).unwrap();
    }

    #[tokio::test]
    async fn test_prompt_file_operations() {
        let temp_dir = TempDir::new().unwrap();

        // Create test prompts in temp directory structure
        let global_dir = temp_dir.path().join(crate::util::paths::global::PROMPTS_DIR);
        let local_dir = temp_dir.path().join(crate::util::paths::workspace::PROMPTS_DIR);

        create_prompt_file(&global_dir, "global_only", "Global content");
        create_prompt_file(&global_dir, "shared", "Global shared");
        create_prompt_file(&local_dir, "local_only", "Local content");
        create_prompt_file(&local_dir, "shared", "Local shared");

        // Test that we can read the files directly
        assert_eq!(
            fs::read_to_string(global_dir.join("global_only.md")).unwrap(),
            "Global content"
        );
        assert_eq!(fs::read_to_string(local_dir.join("shared.md")).unwrap(), "Local shared");
    }

    #[test]
    fn test_local_prompts_override_global() {
        let temp_dir = TempDir::new().unwrap();

        // Create global and local directories
        let global_dir = temp_dir.path().join(crate::util::paths::global::PROMPTS_DIR);
        let local_dir = temp_dir.path().join(crate::util::paths::workspace::PROMPTS_DIR);

        // Create prompts: one with same name in both directories, one unique to each
        create_prompt_file(&global_dir, "shared", "Global version");
        create_prompt_file(&global_dir, "global_only", "Global only");
        create_prompt_file(&local_dir, "shared", "Local version");
        create_prompt_file(&local_dir, "local_only", "Local only");

        // Simulate the priority logic from get_available_prompt_names()
        let mut names = Vec::new();

        // Add global prompts first
        for entry in fs::read_dir(&global_dir).unwrap() {
            let entry = entry.unwrap();
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) == Some("md") {
                if let Some(file_stem) = path.file_stem().and_then(|s| s.to_str()) {
                    let prompt = Prompt::new(file_stem, global_dir.clone());
                    names.push(prompt.name);
                }
            }
        }

        // Add local prompts (with override logic)
        for entry in fs::read_dir(&local_dir).unwrap() {
            let entry = entry.unwrap();
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) == Some("md") {
                if let Some(file_stem) = path.file_stem().and_then(|s| s.to_str()) {
                    let prompt = Prompt::new(file_stem, local_dir.clone());
                    let name = prompt.name;
                    // Remove duplicate if it exists (local overrides global)
                    names.retain(|n| n != &name);
                    names.push(name);
                }
            }
        }

        // Verify: should have 3 unique prompts (shared, global_only, local_only)
        assert_eq!(names.len(), 3);
        assert!(names.contains(&"shared".to_string()));
        assert!(names.contains(&"global_only".to_string()));
        assert!(names.contains(&"local_only".to_string()));

        // Verify only one "shared" exists (local overrode global)
        let shared_count = names.iter().filter(|&name| name == "shared").count();
        assert_eq!(shared_count, 1);

        // Simulate load_prompt_by_name() priority: local first, then global
        let shared_content = if local_dir.join("shared.md").exists() {
            fs::read_to_string(local_dir.join("shared.md")).unwrap()
        } else {
            fs::read_to_string(global_dir.join("shared.md")).unwrap()
        };

        // Verify local version was loaded
        assert_eq!(shared_content, "Local version");
    }

    #[test]
    fn test_validate_prompt_name() {
        // Empty name
        assert!(validate_prompt_name("").is_err());
        assert!(validate_prompt_name("   ").is_err());

        // Too long name (over 50 characters)
        let long_name = "a".repeat(51);
        assert!(validate_prompt_name(&long_name).is_err());

        // Exactly 50 characters should be valid
        let max_name = "a".repeat(50);
        assert!(validate_prompt_name(&max_name).is_ok());

        // Valid names with allowed characters
        assert!(validate_prompt_name("valid_name").is_ok());
        assert!(validate_prompt_name("valid-name-v2").is_ok());

        // Invalid characters (spaces, special chars, path separators)
        assert!(validate_prompt_name("invalid name").is_err()); // space
        assert!(validate_prompt_name("path/name").is_err()); // forward slash
        assert!(validate_prompt_name("path\\name").is_err()); // backslash
        assert!(validate_prompt_name("name.ext").is_err()); // dot
        assert!(validate_prompt_name("name@host").is_err()); // at symbol
        assert!(validate_prompt_name("name#tag").is_err()); // hash
        assert!(validate_prompt_name("name$var").is_err()); // dollar sign
        assert!(validate_prompt_name("name%percent").is_err()); // percent
        assert!(validate_prompt_name("name&and").is_err()); // ampersand
        assert!(validate_prompt_name("name*star").is_err()); // asterisk
        assert!(validate_prompt_name("name+plus").is_err()); // plus
        assert!(validate_prompt_name("name=equals").is_err()); // equals
        assert!(validate_prompt_name("name?question").is_err()); // question mark
        assert!(validate_prompt_name("name[bracket]").is_err()); // brackets
        assert!(validate_prompt_name("name{brace}").is_err()); // braces
        assert!(validate_prompt_name("name(paren)").is_err()); // parentheses
        assert!(validate_prompt_name("name<angle>").is_err()); // angle brackets
        assert!(validate_prompt_name("name|pipe").is_err()); // pipe
        assert!(validate_prompt_name("name;semicolon").is_err()); // semicolon
        assert!(validate_prompt_name("name:colon").is_err()); // colon
        assert!(validate_prompt_name("name\"quote").is_err()); // double quote
        assert!(validate_prompt_name("name'apostrophe").is_err()); // single quote
        assert!(validate_prompt_name("name`backtick").is_err()); // backtick
        assert!(validate_prompt_name("name~tilde").is_err()); // tilde
        assert!(validate_prompt_name("name!exclamation").is_err()); // exclamation
    }

    #[test]
    fn test_format_description() {
        // Test normal description
        let desc = Some("This is a test description".to_string());
        assert_eq!(format_description(desc.as_ref()), "This is a test description");

        // Test None description
        assert_eq!(format_description(None), "(no description)");

        // Test empty description
        let empty_desc = Some("".to_string());
        assert_eq!(format_description(empty_desc.as_ref()), "(no description)");

        // Test whitespace-only description
        let whitespace_desc = Some("   \n\t  ".to_string());
        assert_eq!(format_description(whitespace_desc.as_ref()), "(no description)");

        // Test multi-line description (should take first line)
        let multiline_desc = Some("First line\nSecond line\nThird line".to_string());
        assert_eq!(format_description(multiline_desc.as_ref()), "First line");
    }

    #[test]
    fn test_truncate_description() {
        // Test normal length
        let short = "Short description";
        assert_eq!(truncate_description(short, 40), "Short description");

        // Test truncation
        let long =
            "This is a very long description that should be truncated because it exceeds the maximum length limit";
        let result = truncate_description(long, 40);
        assert!(result.len() <= 40);
        assert!(result.ends_with("..."));
        // Length may be less than 40 due to trim_end() removing trailing spaces
        assert!(result.len() >= 37); // At least max_length - 3 chars

        // Test exact length
        let exact = "A".repeat(40);
        assert_eq!(truncate_description(&exact, 40), exact);

        // Test very short max length
        let result = truncate_description("Hello world", 5);
        assert_eq!(result, "He...");
        assert_eq!(result.len(), 5);

        // Test space trimming before ellipsis
        let with_space = "Prompt to explain available tools and how";
        let result = truncate_description(with_space, 40);
        assert!(!result.contains(" ..."));
        assert!(result.ends_with("..."));
        assert_eq!(result, "Prompt to explain available tools and...");
    }

    #[test]
    fn test_parse_all_mcp_error_details() {
        // Test parsing multiple validation errors
        let error_str = r#"MCP error -32602: Invalid arguments for prompt validation-test: [
  {
    "validation": "regex",
    "code": "invalid_string",
    "message": "Must be a valid email ending in .com",
    "path": [
      "email"
    ]
  },
  {
    "validation": "regex",
    "code": "invalid_string",
    "message": "Must be a positive number",
    "path": [
      "count"
    ]
  }
]"#;

        let errors = parse_all_mcp_error_details(error_str);
        assert_eq!(errors.len(), 2);

        // First error
        assert_eq!(errors[0].code, "invalid_string");
        assert_eq!(errors[0].message, "Must be a valid email ending in .com");
        assert_eq!(errors[0].path, vec!["email"]);

        // Second error
        assert_eq!(errors[1].code, "invalid_string");
        assert_eq!(errors[1].message, "Must be a positive number");
        assert_eq!(errors[1].path, vec!["count"]);

        // Test empty array
        let empty_error = "MCP error -32602: Invalid arguments for prompt test: []";
        let empty_errors = parse_all_mcp_error_details(empty_error);
        assert_eq!(empty_errors.len(), 0);

        // Test invalid JSON
        let invalid_error = "Not a valid MCP error";
        let invalid_errors = parse_all_mcp_error_details(invalid_error);
        assert_eq!(invalid_errors.len(), 0);
    }

    #[test]
    fn test_parse_32603_error_with_data() {
        // Test parsing -32603 error with data object
        let error_str = r#"MCP error -32603: {
            "jsonrpc": "2.0",
            "id": 1,
            "error": {
                "code": -32603,
                "message": "Tool execution failed",
                "data": {
                    "tool": "get_weather",
                    "reason": "API service unavailable"
                }
            }
        }"#;

        // Extract JSON part
        let json_start = error_str.find('{').unwrap();
        let json_end = error_str.rfind('}').unwrap();
        let json_str = &error_str[json_start..=json_end];

        let error_obj: serde_json::Value = serde_json::from_str(json_str).unwrap();
        let error_field = error_obj.get("error").unwrap();

        let message = error_field.get("message").unwrap().as_str().unwrap();
        assert_eq!(message, "Tool execution failed");

        let data = error_field.get("data").unwrap();
        assert_eq!(data.get("tool").unwrap().as_str().unwrap(), "get_weather");
        assert_eq!(data.get("reason").unwrap().as_str().unwrap(), "API service unavailable");
    }

    #[test]
    fn test_parse_32603_error_without_data() {
        // Test parsing -32603 error without data object
        let error_str = r#"MCP error -32603: {
            "jsonrpc": "2.0",
            "id": 5,
            "error": {
                "code": -32603,
                "message": "Internal error"
            }
        }"#;

        let json_start = error_str.find('{').unwrap();
        let json_end = error_str.rfind('}').unwrap();
        let json_str = &error_str[json_start..=json_end];

        let error_obj: serde_json::Value = serde_json::from_str(json_str).unwrap();
        let error_field = error_obj.get("error").unwrap();

        let message = error_field.get("message").unwrap().as_str().unwrap();
        assert_eq!(message, "Internal error");

        // Data field should not exist
        assert!(error_field.get("data").is_none());
    }

    #[test]
    fn test_parse_32603_error_with_complex_data() {
        // Test parsing -32603 error with complex data object
        let error_str = r#"MCP error -32603: {
            "jsonrpc": "2.0",
            "id": 3,
            "error": {
                "code": -32603,
                "message": "Database connection failed",
                "data": {
                    "details": "Connection timeout",
                    "timestamp": "2025-09-13T20:18:59.742Z",
                    "retry_count": 3,
                    "config": {
                        "host": "localhost",
                        "port": 5432
                    }
                }
            }
        }"#;

        let json_start = error_str.find('{').unwrap();
        let json_end = error_str.rfind('}').unwrap();
        let json_str = &error_str[json_start..=json_end];

        let error_obj: serde_json::Value = serde_json::from_str(json_str).unwrap();
        let error_field = error_obj.get("error").unwrap();

        let message = error_field.get("message").unwrap().as_str().unwrap();
        assert_eq!(message, "Database connection failed");

        let data = error_field.get("data").unwrap();
        assert_eq!(data.get("details").unwrap().as_str().unwrap(), "Connection timeout");
        assert_eq!(data.get("retry_count").unwrap().as_u64().unwrap(), 3);

        let config = data.get("config").unwrap();
        assert_eq!(config.get("host").unwrap().as_str().unwrap(), "localhost");
        assert_eq!(config.get("port").unwrap().as_u64().unwrap(), 5432);
    }

    #[test]
    fn test_prompts_subcommand_name() {
        assert_eq!(PromptsSubcommand::List { search_word: None }.name(), "list");
        assert_eq!(
            PromptsSubcommand::Details {
                name: "test".to_string()
            }
            .name(),
            "details"
        );
        assert_eq!(
            PromptsSubcommand::Get {
                orig_input: None,
                name: "test".to_string(),
                arguments: None
            }
            .name(),
            "get"
        );
    }

    #[test]
    fn test_prompts_subcommand_parsing() {
        // Test that Details variant can be created
        let details_cmd = PromptsSubcommand::Details {
            name: "test_prompt".to_string(),
        };
        assert_eq!(details_cmd.name(), "details");

        // Test equality
        let details_cmd2 = PromptsSubcommand::Details {
            name: "test_prompt".to_string(),
        };
        assert_eq!(details_cmd, details_cmd2);
    }

    #[test]
    fn test_server_prompt_name_parsing() {
        // Test parsing server/prompt format
        let name = "server1/my_prompt";
        let (server_filter, prompt_name) = if let Some((server, prompt)) = name.split_once('/') {
            (Some(server), prompt)
        } else {
            (None, name)
        };
        assert_eq!(server_filter, Some("server1"));
        assert_eq!(prompt_name, "my_prompt");

        // Test parsing prompt name only
        let name = "my_prompt";
        let (server_filter, prompt_name) = if let Some((server, prompt)) = name.split_once('/') {
            (Some(server), prompt)
        } else {
            (None, name)
        };
        assert_eq!(server_filter, None);
        assert_eq!(prompt_name, "my_prompt");
    }

    #[test]
    fn test_prompt_bundle_filtering() {
        // Create mock prompt bundles
        let prompt1 = rmcp::model::Prompt {
            name: "test_prompt".to_string(),
            description: Some("Test description".to_string()),
            title: Some("Test Prompt".to_string()),
            icons: None,
            arguments: Some(vec![
                PromptArgument {
                    name: "arg1".to_string(),
                    description: Some("First argument".to_string()),
                    title: Some("Argument 1".to_string()),
                    required: Some(true),
                },
                PromptArgument {
                    name: "arg2".to_string(),
                    title: Some("Argument 2".to_string()),
                    description: None,
                    required: Some(false),
                },
            ]),
        };

        let bundle1 = PromptBundle {
            server_name: "server1".to_string(),
            prompt_get: prompt1.clone(),
        };

        let bundle2 = PromptBundle {
            server_name: "server2".to_string(),
            prompt_get: prompt1,
        };

        let bundles = vec![&bundle1, &bundle2];

        // Test filtering by server
        let filtered: Vec<&PromptBundle> = bundles.iter().filter(|b| b.server_name == "server1").copied().collect();
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].server_name, "server1");

        // Test no filtering (all bundles)
        let all: Vec<&PromptBundle> = bundles.iter().copied().collect();
        assert_eq!(all.len(), 2);
    }

    #[test]
    fn test_ambiguous_prompt_message_generation() {
        // Test generating disambiguation message
        let prompt_name = "test_prompt";
        let server_names = vec!["server1", "server2", "server3"];

        let alt_names: Vec<String> = server_names
            .iter()
            .map(|s| format!("- @{}/{}", s, prompt_name))
            .collect();
        let alt_msg = format!("\n{}\n", alt_names.join("\n"));

        assert_eq!(
            alt_msg,
            "\n- @server1/test_prompt\n- @server2/test_prompt\n- @server3/test_prompt\n"
        );
    }

    #[test]
    fn test_extract_prompt_name_from_qualified_name() {
        // Test extracting prompt name from server/prompt format
        let qualified_name = "server1/my_prompt";
        let actual_prompt_name = if let Some((_, name)) = qualified_name.split_once('/') {
            name
        } else {
            qualified_name
        };
        assert_eq!(actual_prompt_name, "my_prompt");

        // Test with unqualified name
        let unqualified_name = "my_prompt";
        let actual_prompt_name = if let Some((_, name)) = unqualified_name.split_once('/') {
            name
        } else {
            unqualified_name
        };
        assert_eq!(actual_prompt_name, "my_prompt");
    }
}
