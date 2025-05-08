use std::future::Future;
use std::pin::Pin;
use std::process::Command as ProcessCommand;

use crossterm::{
    queue,
    style,
};

use crate::cli::chat::command::Command;
use crate::cli::chat::commands::context_adapter::CommandContextAdapter;
use crate::cli::chat::commands::handler::CommandHandler;
use crate::cli::chat::{
    ChatError,
    ChatState,
    QueuedTool,
};

/// Static instance of the execute command handler
pub static EXECUTE_HANDLER: ExecuteCommand = ExecuteCommand;

/// Handler for the execute command
pub struct ExecuteCommand;

impl CommandHandler for ExecuteCommand {
    fn name(&self) -> &'static str {
        "execute"
    }

    fn description(&self) -> &'static str {
        "Execute a shell command"
    }

    fn usage(&self) -> &'static str {
        "!<command>"
    }

    fn help(&self) -> String {
        "Execute a shell command directly from the chat interface.".to_string()
    }

    fn llm_description(&self) -> String {
        r#"
Execute a shell command directly from the chat interface.

Usage:
!<command>

Examples:
- "!ls -la" - List files in the current directory
- "!echo Hello, world!" - Print a message
- "!git status" - Check git status

This command allows you to run any shell command without leaving the chat interface.
"#
        .to_string()
    }

    fn to_command(&self, args: Vec<&str>) -> Result<Command, ChatError> {
        let command = args.join(" ");
        Ok(Command::Execute { command })
    }

    fn execute_command<'a>(
        &'a self,
        command: &'a Command,
        ctx: &'a mut CommandContextAdapter<'a>,
        tool_uses: Option<Vec<QueuedTool>>,
        pending_tool_index: Option<usize>,
    ) -> Pin<Box<dyn Future<Output = Result<ChatState, ChatError>> + Send + 'a>> {
        Box::pin(async move {
            if let Command::Execute { command } = command {
                queue!(ctx.output, style::Print('\n'))?;
                ProcessCommand::new("bash").args(["-c", command]).status().ok();
                queue!(ctx.output, style::Print('\n'))?;

                Ok(ChatState::PromptUser {
                    tool_uses,
                    pending_tool_index,
                    skip_printing_tools: true,
                })
            } else {
                Err(ChatError::Custom(
                    "ExecuteCommand can only execute Execute commands".into(),
                ))
            }
        })
    }

    fn requires_confirmation(&self, _args: &[&str]) -> bool {
        true // Execute commands require confirmation for security
    }
}
