use std::future::Future;
use std::io::Write;
use std::pin::Pin;

use crossterm::queue;
use crossterm::style::{
    self,
};

use crate::cli::chat::command::{
    Command,
    ToolsSubcommand,
};
use crate::cli::chat::commands::context_adapter::CommandContextAdapter;
use crate::cli::chat::commands::handler::CommandHandler;
use crate::cli::chat::{
    ChatError,
    ChatState,
    QueuedTool,
};

/// Static instance of the tools help command handler
pub static HELP_TOOLS_HANDLER: HelpToolsCommand = HelpToolsCommand;

/// Handler for the tools help command
pub struct HelpToolsCommand;

impl Default for HelpToolsCommand {
    fn default() -> Self {
        Self
    }
}

impl CommandHandler for HelpToolsCommand {
    fn name(&self) -> &'static str {
        "tools help"
    }

    fn description(&self) -> &'static str {
        "Display help information for the tools command"
    }

    fn usage(&self) -> &'static str {
        "/tools help"
    }

    fn help(&self) -> String {
        "Displays help information for the tools command and its subcommands.".to_string()
    }

    fn to_command(&self, _args: Vec<&str>) -> Result<Command, ChatError> {
        Ok(Command::Tools {
            subcommand: Some(ToolsSubcommand::Help),
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
            if let Command::Tools {
                subcommand: Some(ToolsSubcommand::Help),
            } = command
            {
                // Display the help text from the ToolsSubcommand enum
                let help_text = ToolsSubcommand::help_text();
                queue!(
                    ctx.output,
                    style::Print("\n"),
                    style::Print(help_text),
                    style::Print("\n\n")
                )?;
                ctx.output.flush()?;

                Ok(ChatState::PromptUser {
                    tool_uses,
                    pending_tool_index,
                    skip_printing_tools: true,
                })
            } else {
                Err(ChatError::Custom(
                    "HelpToolsCommand can only execute Help commands".into(),
                ))
            }
        })
    }

    fn requires_confirmation(&self, _args: &[&str]) -> bool {
        false
    }
}
