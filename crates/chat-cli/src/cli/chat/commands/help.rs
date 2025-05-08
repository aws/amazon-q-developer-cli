use std::future::Future;
use std::io::Write;
use std::pin::Pin;

use super::CommandHandler;
use super::clear::CLEAR_HANDLER;
use super::context_adapter::CommandContextAdapter;
use super::quit::QUIT_HANDLER;
use crate::cli::chat::command::Command;
use crate::cli::chat::{
    ChatError,
    ChatState,
    QueuedTool,
};

/// Static instance of the help command handler
pub static HELP_HANDLER: HelpCommand = HelpCommand {};

/// Help command handler
#[derive(Clone, Copy)]
pub struct HelpCommand;

impl CommandHandler for HelpCommand {
    fn name(&self) -> &'static str {
        "help"
    }

    fn description(&self) -> &'static str {
        "Show help information"
    }

    fn usage(&self) -> &'static str {
        "/help"
    }

    fn help(&self) -> String {
        "Show help information for all commands".to_string()
    }

    fn llm_description(&self) -> String {
        r#"The help command displays information about available commands.

Usage:
- /help                      Show general help information

Examples:
- "/help" - Shows general help information with a list of all available commands"#
            .to_string()
    }

    fn to_command(&self, args: Vec<&str>) -> Result<Command, ChatError> {
        let help_text = if args.is_empty() { None } else { Some(args.join(" ")) };

        Ok(Command::Help { help_text })
    }

    fn execute_command<'a>(
        &'a self,
        command: &'a Command,
        ctx: &'a mut CommandContextAdapter<'a>,
        tool_uses: Option<Vec<QueuedTool>>,
        pending_tool_index: Option<usize>,
    ) -> Pin<Box<dyn Future<Output = Result<ChatState, ChatError>> + Send + 'a>> {
        Box::pin(async move {
            if let Command::Help { help_text } = command {
                // Get the help text to display
                let text = if let Some(topic) = help_text {
                    // If a specific topic was requested, try to get help for that command
                    // Use the Command enum's to_handler method to get the appropriate handler
                    match topic.as_str() {
                        "clear" => CLEAR_HANDLER.help(),
                        "quit" => QUIT_HANDLER.help(),
                        "help" => self.help(),
                        // Add other commands as needed
                        _ => format!("Unknown command: {}", topic),
                    }
                } else {
                    // Otherwise, show general help
                    crate::cli::chat::HELP_TEXT.to_string()
                };

                // Display the help text
                writeln!(ctx.output, "{}", text)?;
                Ok(ChatState::PromptUser {
                    tool_uses,
                    pending_tool_index,
                    skip_printing_tools: true,
                })
            } else {
                // This should never happen if the command system is working correctly
                Err(ChatError::Custom("HelpCommand can only execute Help commands".into()))
            }
        })
    }

    fn requires_confirmation(&self, _args: &[&str]) -> bool {
        false // Help command doesn't require confirmation
    }
}
