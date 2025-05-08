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
    ToolsSubcommand,
};
use crate::cli::chat::commands::context_adapter::CommandContextAdapter;
use crate::cli::chat::commands::handler::CommandHandler;
use crate::cli::chat::{
    ChatError,
    ChatState,
    QueuedTool,
};

/// Static instance of the tools reset command handler
pub static RESET_TOOLS_HANDLER: ResetToolsCommand = ResetToolsCommand;

/// Handler for the tools reset command
pub struct ResetToolsCommand;

impl Default for ResetToolsCommand {
    fn default() -> Self {
        Self
    }
}

impl CommandHandler for ResetToolsCommand {
    fn name(&self) -> &'static str {
        "tools reset"
    }

    fn description(&self) -> &'static str {
        "Reset all tool permissions to their default state"
    }

    fn usage(&self) -> &'static str {
        "/tools reset"
    }

    fn help(&self) -> String {
        "Resets all tool permissions to their default state. This will clear any previously granted permissions."
            .to_string()
    }

    fn to_command(&self, _args: Vec<&str>) -> Result<Command, ChatError> {
        Ok(Command::Tools {
            subcommand: Some(ToolsSubcommand::Reset),
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
                subcommand: Some(ToolsSubcommand::Reset),
            } = command
            {
                // Reset all tool permissions
                ctx.tool_permissions.reset();

                queue!(
                    ctx.output,
                    style::SetForegroundColor(Color::Green),
                    style::Print("\nAll tool permissions have been reset to their default state.\n\n"),
                    style::ResetColor
                )?;
                ctx.output.flush()?;

                Ok(ChatState::PromptUser {
                    tool_uses,
                    pending_tool_index,
                    skip_printing_tools: true,
                })
            } else {
                Err(ChatError::Custom(
                    "ResetToolsCommand can only execute Reset commands".into(),
                ))
            }
        })
    }

    fn requires_confirmation(&self, _args: &[&str]) -> bool {
        true // Reset is destructive, so require confirmation
    }
}
