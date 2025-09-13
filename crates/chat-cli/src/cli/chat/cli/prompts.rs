use std::collections::{
    HashMap,
    VecDeque,
};

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
use thiserror::Error;
use unicode_width::UnicodeWidthStr;

use crate::cli::chat::tool_manager::PromptBundle;
use crate::cli::chat::{
    ChatError,
    ChatSession,
    ChatState,
};
use crate::mcp_client::McpClientError;

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
    pub async fn execute(self, session: &mut ChatSession) -> Result<ChatState, ChatError> {
        let search_word = match &self.subcommand {
            Some(PromptsSubcommand::List { search_word }) => search_word.clone(),
            _ => None,
        };

        if let Some(subcommand) = self.subcommand {
            if matches!(subcommand, PromptsSubcommand::Get { .. }) {
                return subcommand.execute(session).await;
            }
        }

        let terminal_width = session.terminal_width();
        let prompts = session.conversation.tool_manager.list_prompts().await?;

        // First pass: find longest name and collect filtered prompts
        let mut longest_name = "";
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
        let description_col_width = 40; // Fixed width for descriptions
        let description_pos = prompt_col_width;
        let arguments_pos = description_pos + description_col_width;

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

        // Print header with three columns
        queue!(
            session.stderr,
            style::Print("\n"),
            style::SetAttribute(Attribute::Bold),
            style::Print("Prompt"),
            style::SetAttribute(Attribute::Reset),
            style::Print({
                let padding = description_pos.saturating_sub(UnicodeWidthStr::width("Prompt"));
                " ".repeat(padding)
            }),
            style::SetAttribute(Attribute::Bold),
            style::Print("Description"),
            style::SetAttribute(Attribute::Reset),
            style::Print({
                let padding = arguments_pos.saturating_sub(description_pos + UnicodeWidthStr::width("Description"));
                " ".repeat(padding)
            }),
            style::SetAttribute(Attribute::Bold),
            style::Print("Arguments (* = required)"),
            style::SetAttribute(Attribute::Reset),
            style::Print("\n"),
            style::Print(format!("{}\n", "▔".repeat(terminal_width))),
        )?;

        for (i, (server_name, bundles)) in prompts_by_server.iter_mut().enumerate() {
            bundles.sort_by_key(|bundle| &bundle.prompt_get.name);

            if i > 0 {
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
                    style::SetForegroundColor(Color::DarkGrey),
                    style::Print(&truncated_desc),
                    style::SetForegroundColor(Color::Reset),
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
                                style::SetForegroundColor(Color::DarkGrey),
                                style::Print(match arg.required {
                                    Some(true) => format!("{}*", arg.name),
                                    _ => arg.name.clone(),
                                }),
                                style::SetForegroundColor(Color::Reset),
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
}

impl PromptsSubcommand {
    pub async fn execute(self, session: &mut ChatSession) -> Result<ChatState, ChatError> {
        let PromptsSubcommand::Get {
            orig_input,
            name,
            arguments,
        } = self
        else {
            unreachable!("List has already been parsed out at this point");
        };

        let prompts = match session.conversation.tool_manager.get_prompt(name, arguments).await {
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

    pub fn name(&self) -> &'static str {
        match self {
            PromptsSubcommand::List { .. } => "list",
            PromptsSubcommand::Get { .. } => "get",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
