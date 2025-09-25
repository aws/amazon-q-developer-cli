use std::collections::{
    HashMap,
    VecDeque,
};
use std::fs;
use std::path::PathBuf;
use std::sync::LazyLock;

use clap::{
    Args,
    Subcommand,
};
use crossterm::style::{
    self,
    Attribute,
    Color,
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
use crate::util::directories::{
    chat_global_prompts_dir,
    chat_local_prompts_dir,
};

/// Maximum allowed length for prompt names
const MAX_PROMPT_NAME_LENGTH: usize = 50;

/// Regex for validating prompt names (alphanumeric, hyphens, underscores only)
static PROMPT_NAME_REGEX: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^[a-zA-Z0-9_-]+$").unwrap());

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
        let local_dir = chat_local_prompts_dir(os).map_err(|e| GetPromptError::General(e.into()))?;
        let global_dir = chat_global_prompts_dir(os).map_err(|e| GetPromptError::General(e.into()))?;

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
    fn load_existing(&self) -> Result<Option<(String, PathBuf)>, GetPromptError> {
        if self.local.exists() {
            let content = self.local.load_content()?;
            Ok(Some((content, self.local.path.clone())))
        } else if self.global.exists() {
            let content = self.global.load_content()?;
            Ok(Some((content, self.global.path.clone())))
        } else {
            Ok(None)
        }
    }

    /// Get all available prompt names from both directories
    fn get_available_names(os: &Os) -> Result<Vec<String>, GetPromptError> {
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
        if let Ok(global_dir) = chat_global_prompts_dir(os) {
            collect_from_dir(global_dir, &mut prompt_names)?;
        }

        // Check local prompts
        if let Ok(local_dir) = chat_local_prompts_dir(os) {
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

        let arg_pos = {
            let optimal_case = UnicodeWidthStr::width(longest_name) + terminal_width / 4;
            if optimal_case > terminal_width {
                terminal_width / 3
            } else {
                optimal_case
            }
        };
        // Add usage guidance at the top
        queue!(
            session.stderr,
            style::Print("\n"),
            style::SetAttribute(Attribute::Bold),
            style::Print("Usage: "),
            style::SetAttribute(Attribute::Reset),
            style::Print("You can use a prompt by typing "),
            style::SetAttribute(Attribute::Bold),
            style::SetForegroundColor(Color::Green),
            style::Print("'@<prompt name> [...args]'"),
            style::SetForegroundColor(Color::Reset),
            style::SetAttribute(Attribute::Reset),
            style::Print("\n\n"),
        )?;
        queue!(
            session.stderr,
            style::Print("\n"),
            style::SetAttribute(Attribute::Bold),
            style::Print("Prompt"),
            style::SetAttribute(Attribute::Reset),
            style::Print({
                let name_width = UnicodeWidthStr::width("Prompt");
                let padding = arg_pos.saturating_sub(name_width);
                " ".repeat(padding)
            }),
            style::SetAttribute(Attribute::Bold),
            style::Print("Arguments (* = required)"),
            style::SetAttribute(Attribute::Reset),
            style::Print("\n"),
            style::Print(format!("{}\n", "▔".repeat(terminal_width))),
        )?;
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

        // Display prompts by category
        let filtered_names: Vec<_> = prompt_names
            .iter()
            .filter(|name| name.contains(search_word.as_deref().unwrap_or("")))
            .collect();

        if !filtered_names.is_empty() {
            // Separate global and local prompts for display
            let _global_dir = chat_global_prompts_dir(os).ok();
            let _local_dir = chat_local_prompts_dir(os).ok();

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
                    style::Print("Global (.aws/amazonq/prompts):"),
                    style::SetAttribute(Attribute::Reset),
                    style::Print("\n"),
                )?;
                for name in &global_prompts {
                    queue!(session.stderr, style::Print("- "), style::Print(name))?;
                    queue!(session.stderr, style::Print("\n"))?;
                }
            }

            if !local_prompts.is_empty() {
                if !global_prompts.is_empty() {
                    queue!(session.stderr, style::Print("\n"))?;
                }
                queue!(
                    session.stderr,
                    style::SetAttribute(Attribute::Bold),
                    style::Print("Local (.amazonq/prompts):"),
                    style::SetAttribute(Attribute::Reset),
                    style::Print("\n"),
                )?;
                for name in &local_prompts {
                    let has_global_version = overridden_globals.contains(name);
                    queue!(session.stderr, style::Print("- "), style::Print(name),)?;
                    if has_global_version {
                        queue!(
                            session.stderr,
                            style::SetForegroundColor(Color::Green),
                            style::Print(" (overrides global)"),
                            style::SetForegroundColor(Color::Reset),
                        )?;
                    }

                    // Show override indicator if this local prompt overrides a global one
                    if overridden_globals.contains(name) {
                        queue!(
                            session.stderr,
                            style::SetForegroundColor(Color::DarkGrey),
                            style::Print(" (overrides global)"),
                            style::SetForegroundColor(Color::Reset),
                        )?;
                    }

                    queue!(session.stderr, style::Print("\n"))?;
                }
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
                style::SetAttribute(Attribute::Reset),
                style::Print("\n"),
            )?;
            for bundle in bundles {
                queue!(
                    session.stderr,
                    style::Print("- "),
                    style::Print(&bundle.prompt_get.name),
                    style::Print({
                        if bundle
                            .prompt_get
                            .arguments
                            .as_ref()
                            .is_some_and(|args| !args.is_empty())
                        {
                            let name_width = UnicodeWidthStr::width(bundle.prompt_get.name.as_str());
                            let padding = arg_pos
                                .saturating_sub(name_width)
                                .saturating_sub(UnicodeWidthStr::width("- "));
                            " ".repeat(padding.max(1))
                        } else {
                            "\n".to_owned()
                        }
                    })
                )?;
                if let Some(args) = bundle.prompt_get.arguments.as_ref() {
                    for (i, arg) in args.iter().enumerate() {
                        queue!(
                            session.stderr,
                            style::SetForegroundColor(Color::DarkGrey),
                            style::Print(match arg.required {
                                Some(true) => format!("{}*", arg.name),
                                _ => arg.name.clone(),
                            }),
                            style::SetForegroundColor(Color::Reset),
                            style::Print(if i < args.len() - 1 { ", " } else { "\n" }),
                        )?;
                    }
                }
            }
        }

        Ok(ChatState::PromptUser {
            skip_printing_tools: true,
        })
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
    /// Create a new prompt
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
    /// Edit an existing prompt
    Edit {
        /// Name of the prompt to edit
        name: String,
        /// Edit global prompt instead of local
        #[arg(long)]
        global: bool,
    },
    /// Remove an existing prompt
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
            PromptsSubcommand::Get {
                orig_input,
                name,
                arguments: _,
            } => Self::execute_get(os, session, orig_input, name).await,
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

    async fn execute_get(
        os: &Os,
        session: &mut ChatSession,
        orig_input: Option<String>,
        name: String,
    ) -> Result<ChatState, ChatError> {
        // First try to find prompt (global or local)
        let prompts = Prompts::new(&name, os).map_err(|e| ChatError::Custom(e.to_string().into()))?;
        if let Some((content, _)) = prompts
            .load_existing()
            .map_err(|e| ChatError::Custom(e.to_string().into()))?
        {
            // Handle local prompt
            session.pending_prompts.clear();

            // Create a PromptMessage from the local prompt content
            let prompt_message = PromptMessage {
                role: PromptMessageRole::User,
                content: PromptMessageContent::Text { text: content.clone() },
            };
            session.pending_prompts.push_back(prompt_message);

            return Ok(ChatState::HandleInput {
                input: orig_input.unwrap_or_default(),
            });
        }

        // If not found locally, try MCP prompts
        let prompts = match session.conversation.tool_manager.get_prompt(name, None).await {
            Ok(resp) => resp,
            Err(e) => {
                match e {
                    GetPromptError::AmbiguousPrompt(prompt_name, alt_msg) => {
                        queue!(
                            session.stderr,
                            style::Print("\n"),
                            style::SetForegroundColor(Color::Yellow),
                            style::Print("Prompt "),
                            style::SetForegroundColor(Color::Cyan),
                            style::Print(prompt_name),
                            style::SetForegroundColor(Color::Yellow),
                            style::Print(" is ambiguous. Use one of the following "),
                            style::SetForegroundColor(Color::Cyan),
                            style::Print(alt_msg),
                            style::SetForegroundColor(Color::Reset),
                        )?;
                    },
                    GetPromptError::PromptNotFound(prompt_name) => {
                        queue!(
                            session.stderr,
                            style::Print("\n"),
                            style::SetForegroundColor(Color::Yellow),
                            style::Print("Prompt "),
                            style::SetForegroundColor(Color::Cyan),
                            style::Print(prompt_name),
                            style::SetForegroundColor(Color::Yellow),
                            style::Print(" not found. Use "),
                            style::SetForegroundColor(Color::Cyan),
                            style::Print("/prompts list"),
                            style::SetForegroundColor(Color::Yellow),
                            style::Print(" to see available prompts.\n"),
                            style::SetForegroundColor(Color::Reset),
                        )?;
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
                style::SetForegroundColor(Color::Red),
                style::Print("❌ Invalid prompt name: "),
                style::Print(validation_error),
                style::Print("\n"),
                style::SetForegroundColor(Color::DarkGrey),
                style::Print("Valid names contain only letters, numbers, hyphens, and underscores (1-50 characters)\n"),
                style::SetForegroundColor(Color::Reset),
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
                style::SetForegroundColor(Color::Yellow),
                style::Print("Prompt "),
                style::SetForegroundColor(Color::Cyan),
                style::Print(&name),
                style::SetForegroundColor(Color::Yellow),
                style::Print(" already exists in "),
                style::Print(location),
                style::Print(" directory. Use "),
                style::SetForegroundColor(Color::Cyan),
                style::Print("/prompts edit "),
                style::Print(&name),
                if global {
                    style::Print(" --global")
                } else {
                    style::Print("")
                },
                style::SetForegroundColor(Color::Yellow),
                style::Print(" to modify it.\n"),
                style::SetForegroundColor(Color::Reset),
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
                style::SetForegroundColor(Color::Yellow),
                style::Print("⚠ Warning: A "),
                style::Print(existing_scope),
                style::Print(" prompt named '"),
                style::SetForegroundColor(Color::Cyan),
                style::Print(&name),
                style::SetForegroundColor(Color::Yellow),
                style::Print("' already exists.\n"),
                style::Print(override_message),
                style::Print("\n"),
                style::SetForegroundColor(Color::Reset),
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
                        style::SetForegroundColor(Color::Green),
                        style::Print("✓ Prompt creation cancelled.\n"),
                        style::SetForegroundColor(Color::Reset),
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
                    style::SetForegroundColor(Color::Green),
                    style::Print("✓ Prompt creation cancelled.\n"),
                    style::SetForegroundColor(Color::Reset),
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
                    style::SetForegroundColor(Color::Green),
                    style::Print("✓ Created "),
                    style::Print(location),
                    style::Print(" prompt "),
                    style::SetForegroundColor(Color::Cyan),
                    style::Print(&name),
                    style::SetForegroundColor(Color::Green),
                    style::Print(" at "),
                    style::SetForegroundColor(Color::DarkGrey),
                    style::Print(target_prompt.path.display().to_string()),
                    style::SetForegroundColor(Color::Reset),
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
                    style::SetForegroundColor(Color::Green),
                    style::Print("Opening editor to create prompt content...\n"),
                    style::SetForegroundColor(Color::Reset),
                )?;

                // Try to open the editor
                match open_editor_file(&target_prompt.path) {
                    Ok(()) => {
                        let location = if global { "global" } else { "local" };
                        queue!(
                            session.stderr,
                            style::SetForegroundColor(Color::Green),
                            style::Print("✓ Created "),
                            style::Print(location),
                            style::Print(" prompt "),
                            style::SetForegroundColor(Color::Cyan),
                            style::Print(&name),
                            style::SetForegroundColor(Color::Green),
                            style::Print(" at "),
                            style::SetForegroundColor(Color::DarkGrey),
                            style::Print(target_prompt.path.display().to_string()),
                            style::SetForegroundColor(Color::Reset),
                            style::Print("\n\n"),
                        )?;
                    },
                    Err(err) => {
                        queue!(
                            session.stderr,
                            style::SetForegroundColor(Color::Red),
                            style::Print("Error opening editor: "),
                            style::Print(err.to_string()),
                            style::SetForegroundColor(Color::Reset),
                            style::Print("\n"),
                            style::SetForegroundColor(Color::DarkGrey),
                            style::Print("Tip: You can edit this file directly: "),
                            style::Print(target_prompt.path.display().to_string()),
                            style::SetForegroundColor(Color::Reset),
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
                style::SetForegroundColor(Color::Red),
                style::Print("❌ Invalid prompt name: "),
                style::Print(validation_error),
                style::Print("\n"),
                style::SetForegroundColor(Color::Reset),
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
                    style::SetForegroundColor(Color::Yellow),
                    style::Print("Global prompt "),
                    style::SetForegroundColor(Color::Cyan),
                    style::Print(&name),
                    style::SetForegroundColor(Color::Yellow),
                    style::Print(" not found.\n"),
                    style::SetForegroundColor(Color::Reset),
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
                style::SetForegroundColor(Color::Yellow),
                style::Print("Local prompt "),
                style::SetForegroundColor(Color::Cyan),
                style::Print(&name),
                style::SetForegroundColor(Color::Yellow),
                style::Print(" not found, but global version exists.\n"),
                style::Print("Use "),
                style::SetForegroundColor(Color::Cyan),
                style::Print("/prompts edit "),
                style::Print(&name),
                style::Print(" --global"),
                style::SetForegroundColor(Color::Yellow),
                style::Print(" to edit the global version, or\n"),
                style::Print("use "),
                style::SetForegroundColor(Color::Cyan),
                style::Print("/prompts create "),
                style::Print(&name),
                style::SetForegroundColor(Color::Yellow),
                style::Print(" to create a local override.\n"),
                style::SetForegroundColor(Color::Reset),
            )?;
            return Ok(ChatState::PromptUser {
                skip_printing_tools: true,
            });
        } else {
            queue!(
                session.stderr,
                style::Print("\n"),
                style::SetForegroundColor(Color::Yellow),
                style::Print("Prompt "),
                style::SetForegroundColor(Color::Cyan),
                style::Print(&name),
                style::SetForegroundColor(Color::Yellow),
                style::Print(" not found.\n"),
                style::SetForegroundColor(Color::Reset),
            )?;
            return Ok(ChatState::PromptUser {
                skip_printing_tools: true,
            });
        };

        let location = if global { "global" } else { "local" };
        queue!(
            session.stderr,
            style::Print("\n"),
            style::SetForegroundColor(Color::Green),
            style::Print("Opening editor for "),
            style::Print(location),
            style::Print(" prompt: "),
            style::SetForegroundColor(Color::Cyan),
            style::Print(&name),
            style::SetForegroundColor(Color::Reset),
            style::Print("\n"),
            style::SetForegroundColor(Color::DarkGrey),
            style::Print("File: "),
            style::Print(target_prompt.path.display().to_string()),
            style::SetForegroundColor(Color::Reset),
            style::Print("\n\n"),
        )?;

        // Try to open the editor
        match open_editor_file(&target_prompt.path) {
            Ok(()) => {
                queue!(
                    session.stderr,
                    style::SetForegroundColor(Color::Green),
                    style::Print("✓ Prompt edited successfully.\n\n"),
                    style::SetForegroundColor(Color::Reset),
                )?;
            },
            Err(err) => {
                queue!(
                    session.stderr,
                    style::SetForegroundColor(Color::Red),
                    style::Print("Error opening editor: "),
                    style::Print(err.to_string()),
                    style::SetForegroundColor(Color::Reset),
                    style::Print("\n"),
                    style::SetForegroundColor(Color::DarkGrey),
                    style::Print("Tip: You can edit this file directly: "),
                    style::Print(target_prompt.path.display().to_string()),
                    style::SetForegroundColor(Color::Reset),
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
                    style::SetForegroundColor(Color::Yellow),
                    style::Print("Global prompt "),
                    style::SetForegroundColor(Color::Cyan),
                    style::Print(&name),
                    style::SetForegroundColor(Color::Yellow),
                    style::Print(" not found.\n"),
                    style::SetForegroundColor(Color::Reset),
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
                style::SetForegroundColor(Color::Yellow),
                style::Print("Local prompt "),
                style::SetForegroundColor(Color::Cyan),
                style::Print(&name),
                style::SetForegroundColor(Color::Yellow),
                style::Print(" not found, but global version exists.\n"),
                style::Print("Use "),
                style::SetForegroundColor(Color::Cyan),
                style::Print("/prompts remove "),
                style::Print(&name),
                style::Print(" --global"),
                style::SetForegroundColor(Color::Yellow),
                style::Print(" to remove the global version.\n"),
                style::SetForegroundColor(Color::Reset),
            )?;
            return Ok(ChatState::PromptUser {
                skip_printing_tools: true,
            });
        } else {
            queue!(
                session.stderr,
                style::Print("\n"),
                style::SetForegroundColor(Color::Yellow),
                style::Print("Prompt "),
                style::SetForegroundColor(Color::Cyan),
                style::Print(&name),
                style::SetForegroundColor(Color::Yellow),
                style::Print(" not found.\n"),
                style::SetForegroundColor(Color::Reset),
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
            style::SetForegroundColor(Color::Yellow),
            style::Print("⚠ Warning: This will permanently remove the "),
            style::Print(location),
            style::Print(" prompt '"),
            style::SetForegroundColor(Color::Cyan),
            style::Print(&name),
            style::SetForegroundColor(Color::Yellow),
            style::Print("'.\n"),
            style::SetForegroundColor(Color::DarkGrey),
            style::Print("File: "),
            style::Print(target_prompt.path.display().to_string()),
            style::SetForegroundColor(Color::Reset),
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
                    style::SetForegroundColor(Color::Green),
                    style::Print("✓ Removal cancelled.\n"),
                    style::SetForegroundColor(Color::Reset),
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
                style::SetForegroundColor(Color::Green),
                style::Print("✓ Removal cancelled.\n"),
                style::SetForegroundColor(Color::Reset),
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
                    style::SetForegroundColor(Color::Green),
                    style::Print("✓ Removed "),
                    style::Print(location),
                    style::Print(" prompt "),
                    style::SetForegroundColor(Color::Cyan),
                    style::Print(&name),
                    style::SetForegroundColor(Color::Green),
                    style::Print(" successfully.\n\n"),
                    style::SetForegroundColor(Color::Reset),
                )?;
            },
            Err(err) => {
                queue!(
                    session.stderr,
                    style::Print("\n"),
                    style::SetForegroundColor(Color::Red),
                    style::Print("Error deleting prompt: "),
                    style::Print(err.to_string()),
                    style::SetForegroundColor(Color::Reset),
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
            PromptsSubcommand::Get { .. } => "get",
            PromptsSubcommand::Create { .. } => "create",
            PromptsSubcommand::Edit { .. } => "edit",
            PromptsSubcommand::Remove { .. } => "remove",
        }
    }
}
#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::TempDir;

    use super::*;

    fn create_prompt_file(dir: &PathBuf, name: &str, content: &str) {
        fs::create_dir_all(dir).unwrap();
        fs::write(dir.join(format!("{}.md", name)), content).unwrap();
    }

    #[tokio::test]
    async fn test_prompt_file_operations() {
        let temp_dir = TempDir::new().unwrap();

        // Create test prompts in temp directory structure
        let global_dir = temp_dir.path().join(".aws/amazonq/prompts");
        let local_dir = temp_dir.path().join(".amazonq/prompts");

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
        let global_dir = temp_dir.path().join(".aws/amazonq/prompts");
        let local_dir = temp_dir.path().join(".amazonq/prompts");

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
}
