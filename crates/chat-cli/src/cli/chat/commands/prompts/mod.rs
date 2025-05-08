use std::future::Future;
use std::pin::Pin;

use crate::cli::chat::command::{
    Command,
    PromptsGetCommand,
    PromptsSubcommand,
};
use crate::cli::chat::commands::{
    CommandContextAdapter,
    CommandHandler,
};
use crate::cli::chat::{
    ChatError,
    ChatState,
    QueuedTool,
};

mod get;
mod help;
mod list;

// Static handlers for prompts subcommands
pub use get::GET_PROMPTS_HANDLER;
pub use help::HELP_PROMPTS_HANDLER;
pub use list::LIST_PROMPTS_HANDLER;

/// Handler for the prompts command
pub struct PromptsCommand;

impl PromptsCommand {
    pub fn new() -> Self {
        Self
    }
}

impl Default for PromptsCommand {
    fn default() -> Self {
        Self::new()
    }
}

impl CommandHandler for PromptsCommand {
    fn name(&self) -> &'static str {
        "prompts"
    }

    fn description(&self) -> &'static str {
        "Manage and use reusable prompts"
    }

    fn usage(&self) -> &'static str {
        "/prompts [subcommand]"
    }

    fn help(&self) -> String {
        r#"
Prompts Management

Prompts are reusable templates that help you quickly access common workflows and tasks.
These templates are provided by the mcp servers you have installed and configured.

Available commands:
  list [search word]                List available prompts or search for specific ones
  get <prompt name> [args]          Retrieve and use a specific prompt
  help                              Show this help message

Notes:
• You can also use @<prompt name> as a shortcut for /prompts get <prompt name>
• Prompts can accept arguments to customize their behavior
• Prompts are provided by MCP servers you have installed
"#
        .to_string()
    }

    fn llm_description(&self) -> String {
        r#"Prompts are reusable templates that help you quickly access common workflows and tasks. 
These templates are provided by the mcp servers you have installed and configured.

To actually retrieve a prompt, directly start with the following command (without prepending /prompt get):
  @<prompt name> [arg]                                   Retrieve prompt specified
Or if you prefer the long way:
  /prompts get <prompt name> [arg]                       Retrieve prompt specified

Usage: /prompts [SUBCOMMAND]

Description:
  Show the current set of reusable prompts from the current fleet of mcp servers.

Available subcommands:
  help                                                   Show an explanation for the prompts command
  list [search word]                                     List available prompts from a tool or show all available prompts"#.to_string()
    }

    fn to_command(&self, args: Vec<&str>) -> Result<Command, ChatError> {
        if args.is_empty() {
            // Default to showing the list when no subcommand is provided
            return Ok(Command::Prompts {
                subcommand: Some(PromptsSubcommand::List { search_word: None }),
            });
        }

        // Check if this is a help request
        if args.len() == 1 && args[0] == "help" {
            return Ok(Command::Prompts {
                subcommand: Some(PromptsSubcommand::Help),
            });
        }

        // Parse arguments to determine the subcommand
        let subcommand = if let Some(first_arg) = args.first() {
            match *first_arg {
                "list" => {
                    let search_word = args.get(1).map(|s| (*s).to_string());
                    Some(PromptsSubcommand::List { search_word })
                },
                "get" => {
                    if args.len() < 2 {
                        return Err(ChatError::Custom("Expected prompt name".into()));
                    }

                    let name = args[1].to_string();
                    let arguments = if args.len() > 2 {
                        Some(args[2..].iter().map(|s| (*s).to_string()).collect())
                    } else {
                        None
                    };

                    let params = crate::cli::chat::command::PromptsGetParam { name, arguments };

                    let get_command = PromptsGetCommand {
                        orig_input: Some(args[1..].join(" ")),
                        params,
                    };

                    Some(PromptsSubcommand::Get { get_command })
                },
                "help" => {
                    // This case is handled above, but we'll include it here for completeness
                    Some(PromptsSubcommand::Help)
                },
                _ => {
                    // For unknown subcommands, show help
                    return Ok(Command::Help {
                        help_text: Some(PromptsSubcommand::help_text()),
                    });
                },
            }
        } else {
            None // Default to list if no arguments (should not happen due to earlier check)
        };

        Ok(Command::Prompts { subcommand })
    }

    fn execute_command<'a>(
        &'a self,
        command: &'a Command,
        ctx: &'a mut CommandContextAdapter<'a>,
        tool_uses: Option<Vec<QueuedTool>>,
        pending_tool_index: Option<usize>,
    ) -> Pin<Box<dyn Future<Output = Result<ChatState, ChatError>> + Send + 'a>> {
        Box::pin(async move {
            match command {
                Command::Prompts { subcommand: None } => {
                    // Default behavior is to list prompts
                    LIST_PROMPTS_HANDLER
                        .execute_command(command, ctx, tool_uses, pending_tool_index)
                        .await
                },
                Command::Prompts {
                    subcommand: Some(subcommand),
                } => {
                    // Delegate to the appropriate subcommand handler
                    subcommand
                        .to_handler()
                        .execute_command(command, ctx, tool_uses, pending_tool_index)
                        .await
                },
                _ => Err(ChatError::Custom(
                    "PromptsCommand can only execute Prompts commands".into(),
                )),
            }
        })
    }

    fn requires_confirmation(&self, _args: &[&str]) -> bool {
        false // Prompts commands don't require confirmation
    }
}

impl PromptsSubcommand {
    pub fn to_handler(&self) -> &'static dyn CommandHandler {
        match self {
            PromptsSubcommand::List { .. } => &LIST_PROMPTS_HANDLER,
            PromptsSubcommand::Get { .. } => &GET_PROMPTS_HANDLER,
            PromptsSubcommand::Help => &HELP_PROMPTS_HANDLER,
        }
    }
}
