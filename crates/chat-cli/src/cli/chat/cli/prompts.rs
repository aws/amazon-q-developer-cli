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
        let mut longest_name = "";
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
