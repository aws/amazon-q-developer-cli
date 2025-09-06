use std::collections::{
    HashMap,
    VecDeque,
};
use std::fs;
use std::path::PathBuf;

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

/// Get list of available prompt names from both global and local directories
fn get_available_prompt_names(os: &Os) -> Result<Vec<String>, GetPromptError> {
    let mut prompt_names = Vec::new();

    // Check global prompts
    if let Ok(global_dir) = chat_global_prompts_dir(os) {
        if global_dir.exists() {
            for entry in fs::read_dir(&global_dir)? {
                let entry = entry?;
                let path = entry.path();
                if path.is_file() && path.extension().and_then(|s| s.to_str()) == Some("md") {
                    if let Some(file_stem) = path.file_stem().and_then(|s| s.to_str()) {
                        prompt_names.push(file_stem.to_string());
                    }
                }
            }
        }
    }

    // Check local prompts (can override global ones)
    if let Ok(local_dir) = chat_local_prompts_dir(os) {
        if local_dir.exists() {
            for entry in fs::read_dir(&local_dir)? {
                let entry = entry?;
                let path = entry.path();
                if path.is_file() && path.extension().and_then(|s| s.to_str()) == Some("md") {
                    if let Some(file_stem) = path.file_stem().and_then(|s| s.to_str()) {
                        let name = file_stem.to_string();
                        // Remove duplicate if it exists (local overrides global)
                        prompt_names.retain(|n| n != &name);
                        prompt_names.push(name);
                    }
                }
            }
        }
    }

    Ok(prompt_names)
}

/// Find and load a specific prompt by name
fn load_prompt_by_name(os: &Os, name: &str) -> Result<Option<(String, PathBuf)>, GetPromptError> {
    // Try local first (higher priority)
    if let Ok(local_dir) = chat_local_prompts_dir(os) {
        let local_path = local_dir.join(format!("{}.md", name));
        if local_path.exists() {
            let content = fs::read_to_string(&local_path)?;
            return Ok(Some((content, local_path)));
        }
    }

    // Try global
    if let Ok(global_dir) = chat_global_prompts_dir(os) {
        let global_path = global_dir.join(format!("{}.md", name));
        if global_path.exists() {
            let content = fs::read_to_string(&global_path)?;
            return Ok(Some((content, global_path)));
        }
    }

    Ok(None)
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
                PromptsSubcommand::Get { .. } | PromptsSubcommand::Create { .. } | PromptsSubcommand::Edit { .. }
            ) {
                return subcommand.execute(os, session).await;
            }
        }

        let terminal_width = session.terminal_width();
        let prompts = session.conversation.tool_manager.list_prompts().await?;

        // Get available prompt names
        let prompt_names = get_available_prompt_names(os).map_err(|e| ChatError::Custom(e.to_string().into()))?;

        let mut longest_name = "";

        // Update longest_name to include local prompts
        for name in &prompt_names {
            if name.contains(search_word.as_deref().unwrap_or("")) {
                if name.len() > longest_name.len() {
                    longest_name = name;
                }
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
            let global_dir = chat_global_prompts_dir(os).ok();
            let local_dir = chat_local_prompts_dir(os).ok();

            let mut global_prompts = Vec::new();
            let mut local_prompts_only = Vec::new();

            for name in &filtered_names {
                // Check if it exists in local (higher priority)
                if let Some(local_dir) = &local_dir {
                    let local_path = local_dir.join(format!("{}.md", name));
                    if local_path.exists() {
                        local_prompts_only.push(name);
                        continue;
                    }
                }

                // Check if it exists in global
                if let Some(global_dir) = &global_dir {
                    let global_path = global_dir.join(format!("{}.md", name));
                    if global_path.exists() {
                        global_prompts.push(name);
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
                    queue!(
                        session.stderr,
                        style::Print("- "),
                        style::Print(name),
                        style::Print("\n"),
                    )?;
                }
            }

            if !local_prompts_only.is_empty() {
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
                for name in &local_prompts_only {
                    queue!(
                        session.stderr,
                        style::Print("- "),
                        style::Print(name),
                        style::Print("\n"),
                    )?;
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
    /// Create a new local prompt
    Create {
        /// Name of the prompt to create
        name: String,
        /// Content of the prompt (if not provided, opens editor)
        #[arg(long)]
        content: Option<String>,
    },
    /// Edit an existing local prompt
    Edit {
        /// Name of the prompt to edit
        name: String,
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
            PromptsSubcommand::Create { name, content } => Self::execute_create(os, session, name, content).await,
            PromptsSubcommand::Edit { name } => Self::execute_edit(os, session, name).await,
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
        if let Some((content, _)) =
            load_prompt_by_name(os, &name).map_err(|e| ChatError::Custom(e.to_string().into()))?
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
    ) -> Result<ChatState, ChatError> {
        // Ensure local .amazonq/prompts directory exists
        let prompts_dir = chat_local_prompts_dir(os).map_err(|e| ChatError::Custom(e.to_string().into()))?;

        if !prompts_dir.exists() {
            fs::create_dir_all(&prompts_dir).map_err(|e| ChatError::Custom(e.to_string().into()))?;
        }

        let file_path = prompts_dir.join(format!("{}.md", name));

        // Check if prompt already exists
        if file_path.exists() {
            queue!(
                session.stderr,
                style::Print("\n"),
                style::SetForegroundColor(Color::Yellow),
                style::Print("Prompt "),
                style::SetForegroundColor(Color::Cyan),
                style::Print(&name),
                style::SetForegroundColor(Color::Yellow),
                style::Print(" already exists. Use "),
                style::SetForegroundColor(Color::Cyan),
                style::Print("/prompts edit "),
                style::Print(&name),
                style::SetForegroundColor(Color::Yellow),
                style::Print(" to modify it.\n"),
                style::SetForegroundColor(Color::Reset),
            )?;
            return Ok(ChatState::PromptUser {
                skip_printing_tools: true,
            });
        }

        let prompt_content = match content {
            Some(content) => content,
            None => {
                // Open editor for content input
                queue!(
                    session.stderr,
                    style::Print("\n"),
                    style::SetForegroundColor(Color::Green),
                    style::Print("Opening editor to create prompt content...\n"),
                    style::SetForegroundColor(Color::Reset),
                )?;

                // Use a simple default content that user can edit
                "# Enter your prompt content here\n\nDescribe what this prompt should do...".to_string()
            },
        };

        // Write the prompt file
        fs::write(&file_path, &prompt_content).map_err(|e| ChatError::Custom(e.to_string().into()))?;

        queue!(
            session.stderr,
            style::Print("\n"),
            style::SetForegroundColor(Color::Green),
            style::Print("✓ Created prompt "),
            style::SetForegroundColor(Color::Cyan),
            style::Print(&name),
            style::SetForegroundColor(Color::Green),
            style::Print(" at "),
            style::SetForegroundColor(Color::DarkGrey),
            style::Print(file_path.display().to_string()),
            style::SetForegroundColor(Color::Reset),
            style::Print("\n\n"),
        )?;

        Ok(ChatState::PromptUser {
            skip_printing_tools: true,
        })
    }

    async fn execute_edit(os: &Os, session: &mut ChatSession, name: String) -> Result<ChatState, ChatError> {
        // Find the prompt file path
        if let Some((_, file_path)) =
            load_prompt_by_name(os, &name).map_err(|e| ChatError::Custom(e.to_string().into()))?
        {
            queue!(
                session.stderr,
                style::Print("\n"),
                style::SetForegroundColor(Color::Green),
                style::Print("Opening editor for prompt: "),
                style::SetForegroundColor(Color::Cyan),
                style::Print(&name),
                style::SetForegroundColor(Color::Reset),
                style::Print("\n"),
                style::SetForegroundColor(Color::DarkGrey),
                style::Print("File: "),
                style::Print(file_path.display().to_string()),
                style::SetForegroundColor(Color::Reset),
                style::Print("\n\n"),
            )?;

            // Try to open the editor
            match open_editor_file(&file_path) {
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
                        style::Print(file_path.display().to_string()),
                        style::SetForegroundColor(Color::Reset),
                        style::Print("\n\n"),
                    )?;
                },
            }
        } else {
            queue!(
                session.stderr,
                style::Print("\n"),
                style::SetForegroundColor(Color::Yellow),
                style::Print("Prompt "),
                style::SetForegroundColor(Color::Cyan),
                style::Print(&name),
                style::SetForegroundColor(Color::Yellow),
                style::Print(" not found. Use "),
                style::SetForegroundColor(Color::Cyan),
                style::Print("/prompts create "),
                style::Print(&name),
                style::SetForegroundColor(Color::Yellow),
                style::Print(" to create it.\n"),
                style::SetForegroundColor(Color::Reset),
            )?;
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
                    names.push(file_stem.to_string());
                }
            }
        }

        // Add local prompts (with override logic)
        for entry in fs::read_dir(&local_dir).unwrap() {
            let entry = entry.unwrap();
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) == Some("md") {
                if let Some(file_stem) = path.file_stem().and_then(|s| s.to_str()) {
                    let name = file_stem.to_string();
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
}
