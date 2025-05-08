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

/// Static instance of the prompts help command handler
pub static HELP_PROMPTS_HANDLER: HelpPromptsCommand = HelpPromptsCommand;

/// Handler for the prompts help command
pub struct HelpPromptsCommand;

impl CommandHandler for HelpPromptsCommand {
    fn name(&self) -> &'static str {
        "help"
    }

    fn description(&self) -> &'static str {
        "Show help for prompts command"
    }

    fn usage(&self) -> &'static str {
        "/prompts help"
    }

    fn help(&self) -> String {
        "Show help information for the prompts command.".to_string()
    }

    fn to_command(&self, _args: Vec<&str>) -> Result<Command, ChatError> {
        Ok(Command::Prompts {
            subcommand: Some(PromptsSubcommand::Help),
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
            match command {
                Command::Prompts {
                    subcommand: Some(PromptsSubcommand::Help),
                } => {
                    // Display help information
                    queue!(
                        ctx.output,
                        style::Print("\n"),
                        style::SetForegroundColor(Color::Magenta),
                        style::SetAttribute(crossterm::style::Attribute::Bold),
                        style::Print("Prompts Management\n"),
                        style::SetAttribute(crossterm::style::Attribute::Reset),
                        style::ResetColor,
                        style::Print("\n"),
                        style::Print(
                            "Prompts are reusable templates that help you quickly access common workflows and tasks.\n"
                        ),
                        style::Print(
                            "These templates are provided by the MCP servers you have installed and configured.\n\n"
                        ),
                        style::SetForegroundColor(Color::Cyan),
                        style::SetAttribute(crossterm::style::Attribute::Bold),
                        style::Print("Available commands\n"),
                        style::SetAttribute(crossterm::style::Attribute::Reset),
                        style::ResetColor,
                        style::Print("  "),
                        style::SetAttribute(crossterm::style::Attribute::Italic),
                        style::Print("list [search word]"),
                        style::SetAttribute(crossterm::style::Attribute::Reset),
                        style::Print("                "),
                        style::SetForegroundColor(Color::DarkGrey),
                        style::Print("List available prompts or search for specific ones\n"),
                        style::ResetColor,
                        style::Print("  "),
                        style::SetAttribute(crossterm::style::Attribute::Italic),
                        style::Print("get <prompt name> [args]"),
                        style::SetAttribute(crossterm::style::Attribute::Reset),
                        style::Print("        "),
                        style::SetForegroundColor(Color::DarkGrey),
                        style::Print("Retrieve and use a specific prompt\n"),
                        style::ResetColor,
                        style::Print("  "),
                        style::SetAttribute(crossterm::style::Attribute::Italic),
                        style::Print("help"),
                        style::SetAttribute(crossterm::style::Attribute::Reset),
                        style::Print("                              "),
                        style::SetForegroundColor(Color::DarkGrey),
                        style::Print("Show this help message\n"),
                        style::ResetColor,
                        style::Print("\n"),
                        style::SetForegroundColor(Color::Cyan),
                        style::SetAttribute(crossterm::style::Attribute::Bold),
                        style::Print("Notes\n"),
                        style::SetAttribute(crossterm::style::Attribute::Reset),
                        style::ResetColor,
                        style::Print(
                            "• You can also use @<prompt name> as a shortcut for /prompts get <prompt name>\n"
                        ),
                        style::Print("• Prompts can accept arguments to customize their behavior\n"),
                        style::Print("• Prompts are provided by MCP servers you have installed\n\n")
                    )?;
                    ctx.output.flush()?;
                },
                _ => return Err(ChatError::Custom("Invalid command".into())),
            }

            Ok(ChatState::PromptUser {
                tool_uses,
                pending_tool_index,
                skip_printing_tools: true,
            })
        })
    }

    fn requires_confirmation(&self, _args: &[&str]) -> bool {
        false // Help command doesn't require confirmation
    }
}
