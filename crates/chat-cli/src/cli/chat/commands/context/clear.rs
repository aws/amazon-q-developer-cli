use std::io::Write;

use crossterm::queue;
use crossterm::style::{
    self,
    Color,
};

use crate::cli::chat::commands::CommandHandler;
use crate::cli::chat::{
    ChatError,
    ChatState,
    QueuedTool,
};

/// Static instance of the clear context command handler
pub static CLEAR_CONTEXT_HANDLER: ClearContextCommand = ClearContextCommand;

/// Handler for the context clear command
pub struct ClearContextCommand;

impl CommandHandler for ClearContextCommand {
    fn name(&self) -> &'static str {
        "clear"
    }

    fn description(&self) -> &'static str {
        "Clear all files from current context"
    }

    fn usage(&self) -> &'static str {
        "/context clear [--global]"
    }

    fn help(&self) -> String {
        "Clear all files from the current context. Use --global to clear global context.".to_string()
    }

    fn to_command(&self, args: Vec<&str>) -> Result<crate::cli::chat::command::Command, ChatError> {
        let global = args.contains(&"--global");

        Ok(crate::cli::chat::command::Command::Context {
            subcommand: crate::cli::chat::command::ContextSubcommand::Clear { global },
        })
    }

    fn execute_command<'a>(
        &'a self,
        command: &'a crate::cli::chat::command::Command,
        ctx: &'a mut crate::cli::chat::commands::context_adapter::CommandContextAdapter<'a>,
        tool_uses: Option<Vec<QueuedTool>>,
        pending_tool_index: Option<usize>,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<ChatState, ChatError>> + Send + 'a>> {
        Box::pin(async move {
            // Extract the parameters from the command
            let global = match command {
                crate::cli::chat::command::Command::Context {
                    subcommand: crate::cli::chat::command::ContextSubcommand::Clear { global },
                } => global,
                _ => return Err(ChatError::Custom("Invalid command".into())),
            };

            // Get the context manager
            let Some(context_manager) = &mut ctx.conversation_state.context_manager else {
                queue!(
                    ctx.output,
                    style::SetForegroundColor(Color::Red),
                    style::Print("Error: Context manager not initialized\n"),
                    style::ResetColor
                )?;
                ctx.output.flush()?;
                return Ok(ChatState::PromptUser {
                    tool_uses,
                    pending_tool_index,
                    skip_printing_tools: true,
                });
            };

            // Clear the context
            match context_manager.clear(*global).await {
                Ok(_) => {
                    // Success message
                    let scope = if *global { "global" } else { "profile" };
                    queue!(
                        ctx.output,
                        style::SetForegroundColor(Color::Green),
                        style::Print(format!("Cleared all files from {} context\n", scope)),
                        style::ResetColor
                    )?;
                    ctx.output.flush()?;
                },
                Err(e) => {
                    // Error message
                    queue!(
                        ctx.output,
                        style::SetForegroundColor(Color::Red),
                        style::Print(format!("Error: {}\n", e)),
                        style::ResetColor
                    )?;
                    ctx.output.flush()?;
                },
            }

            Ok(ChatState::PromptUser {
                tool_uses,
                pending_tool_index,
                skip_printing_tools: true,
            })
        })
    }

    fn requires_confirmation(&self, _args: &[&str]) -> bool {
        true // Clearing context requires confirmation as it's a destructive operation
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::chat::command::{
        Command,
        ContextSubcommand,
    };

    #[test]
    fn test_to_command_with_global() {
        let handler = ClearContextCommand;
        let args = vec!["--global"];

        let command = handler.to_command(args).unwrap();

        match command {
            Command::Context {
                subcommand: ContextSubcommand::Clear { global },
            } => {
                assert!(global);
            },
            _ => panic!("Expected Context Clear command"),
        }
    }

    #[test]
    fn test_to_command_without_global() {
        let handler = ClearContextCommand;
        let args = vec![];

        let command = handler.to_command(args).unwrap();

        match command {
            Command::Context {
                subcommand: ContextSubcommand::Clear { global },
            } => {
                assert!(!global);
            },
            _ => panic!("Expected Context Clear command"),
        }
    }

    #[test]
    fn test_requires_confirmation() {
        let handler = ClearContextCommand;
        assert!(handler.requires_confirmation(&[]));
    }
}
