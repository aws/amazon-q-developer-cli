use std::future::Future;
use std::io::Write;
use std::pin::Pin;

use crossterm::queue;
use crossterm::style::{
    self,
    Color,
};

use crate::cli::chat::command::{
    Command,
    PromptsSubcommand,
};
use crate::cli::chat::commands::context_adapter::CommandContextAdapter;
use crate::cli::chat::commands::handler::CommandHandler;
use crate::cli::chat::{
    ChatError,
    ChatState,
    QueuedTool,
};

/// Static instance of the prompts get command handler
pub static GET_PROMPTS_HANDLER: GetPromptsCommand = GetPromptsCommand;

/// Handler for the prompts get command
pub struct GetPromptsCommand;

impl CommandHandler for GetPromptsCommand {
    fn name(&self) -> &'static str {
        "get"
    }

    fn description(&self) -> &'static str {
        "Retrieve and use a specific prompt"
    }

    fn usage(&self) -> &'static str {
        "/prompts get <prompt_name> [args]"
    }

    fn help(&self) -> String {
        "Retrieve and use a specific prompt template.".to_string()
    }

    fn to_command(&self, args: Vec<&str>) -> Result<Command, ChatError> {
        if args.is_empty() {
            return Err(ChatError::Custom("Expected prompt name".into()));
        }

        let name = args[0].to_string();
        let arguments = if args.len() > 1 {
            Some(args[1..].iter().map(|s| (*s).to_string()).collect())
        } else {
            None
        };

        let params = crate::cli::chat::command::PromptsGetParam { name, arguments };

        let get_command = crate::cli::chat::command::PromptsGetCommand {
            orig_input: Some(args.join(" ")),
            params,
        };

        Ok(Command::Prompts {
            subcommand: Some(PromptsSubcommand::Get { get_command }),
        })
    }

    fn execute_command<'a>(
        &'a self,
        command: &'a Command,
        ctx: &'a mut CommandContextAdapter<'a>,
        tool_uses: Option<Vec<QueuedTool>>,
        pending_tool_index: Option<usize>,
    ) -> Pin<Box<dyn Future<Output = Result<ChatState, ChatError>> + Send + 'a>> {
        Box::pin(async move {
            // Extract the get command from the command
            let get_command = match command {
                Command::Prompts {
                    subcommand: Some(PromptsSubcommand::Get { get_command }),
                } => get_command,
                _ => return Err(ChatError::Custom("Invalid command".into())),
            };

            // In a real implementation, we would query the MCP servers for the prompt
            // For now, we'll just display a placeholder message
            queue!(
                ctx.output,
                style::Print("\n"),
                style::SetForegroundColor(Color::Yellow),
                style::Print(format!("Prompt '{}' not found.\n\n", get_command.params.name)),
                style::ResetColor,
                style::Print(
                    "To use prompts, you need to install and configure MCP servers that provide prompt templates.\n\n"
                )
            )?;

            if let Some(args) = &get_command.params.arguments {
                queue!(
                    ctx.output,
                    style::Print(format!("Arguments provided: {}\n\n", args.join(", ")))
                )?;
            }

            ctx.output.flush()?;

            Ok(ChatState::PromptUser {
                tool_uses,
                pending_tool_index,
                skip_printing_tools: true,
            })
        })
    }

    fn requires_confirmation(&self, _args: &[&str]) -> bool {
        false // Get command doesn't require confirmation
    }
}
