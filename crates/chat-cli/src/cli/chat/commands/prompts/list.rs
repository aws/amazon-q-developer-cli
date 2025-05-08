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

/// Static instance of the prompts list command handler
pub static LIST_PROMPTS_HANDLER: ListPromptsCommand = ListPromptsCommand;

/// Handler for the prompts list command
pub struct ListPromptsCommand;

impl CommandHandler for ListPromptsCommand {
    fn name(&self) -> &'static str {
        "list"
    }

    fn description(&self) -> &'static str {
        "List available prompts"
    }

    fn usage(&self) -> &'static str {
        "/prompts list [search_word]"
    }

    fn help(&self) -> String {
        "List available prompts or search for specific ones.".to_string()
    }

    fn to_command(&self, args: Vec<&str>) -> Result<Command, ChatError> {
        let search_word = args.first().map(|s| (*s).to_string());

        Ok(Command::Prompts {
            subcommand: Some(PromptsSubcommand::List { search_word }),
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
            // Extract the search word from the command
            let search_word = match command {
                Command::Prompts {
                    subcommand: Some(PromptsSubcommand::List { search_word }),
                } => search_word.clone(),
                _ => return Err(ChatError::Custom("Invalid command".into())),
            };

            // In a real implementation, we would query the MCP servers for available prompts
            // For now, we'll just display a placeholder message
            queue!(
                ctx.output,
                style::Print("\nAvailable Prompts:\n\n"),
                style::SetForegroundColor(Color::Yellow),
                style::Print("No MCP servers with prompts are currently available.\n\n"),
                style::ResetColor,
                style::Print(
                    "To use prompts, you need to install and configure MCP servers that provide prompt templates.\n\n"
                )
            )?;

            if let Some(word) = search_word {
                queue!(
                    ctx.output,
                    style::Print(format!("Search term: \"{}\"\n", word)),
                    style::Print("No matching prompts found.\n\n")
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
        false // List command doesn't require confirmation
    }
}
