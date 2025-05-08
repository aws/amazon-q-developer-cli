use std::future::Future;
use std::io::Write;
use std::pin::Pin;

use super::CommandHandler;
use super::context_adapter::CommandContextAdapter;
use crate::cli::chat::command::Command;
use crate::cli::chat::{
    ChatError,
    ChatState,
    QueuedTool,
};

/// Static instance of the clear command handler
pub static CLEAR_HANDLER: ClearCommand = ClearCommand;

/// Clear command handler
#[derive(Clone, Copy)]
pub struct ClearCommand;

impl CommandHandler for ClearCommand {
    fn name(&self) -> &'static str {
        "clear"
    }

    fn description(&self) -> &'static str {
        "Clear the conversation history"
    }

    fn usage(&self) -> &'static str {
        "/clear"
    }

    fn help(&self) -> String {
        "Clear the conversation history and context from hooks for the current session".to_string()
    }

    fn llm_description(&self) -> String {
        r#"The clear command erases the conversation history and context from hooks for the current session.

Usage:
- /clear                     Clear the conversation history

This command will prompt for confirmation before clearing the history.

Examples of statements that may trigger this command:
- "Clear the conversation"
- "Start fresh"
- "Reset our chat"
- "Clear the chat history"
- "I want to start over"
- "Erase our conversation"
- "Let's start with a clean slate"
- "Clear everything"
- "Reset the context"
- "Wipe the conversation history""#
            .to_string()
    }

    fn to_command(&self, _args: Vec<&str>) -> Result<Command, ChatError> {
        Ok(Command::Clear)
    }

    fn execute_command<'a>(
        &'a self,
        command: &'a Command,
        ctx: &'a mut CommandContextAdapter<'a>,
        tool_uses: Option<Vec<QueuedTool>>,
        pending_tool_index: Option<usize>,
    ) -> Pin<Box<dyn Future<Output = Result<ChatState, ChatError>> + Send + 'a>> {
        Box::pin(async move {
            if let Command::Clear = command {
                // Clear the conversation history
                ctx.conversation_state.clear(false);
                writeln!(ctx.output, "Conversation history cleared.")?;
                Ok(ChatState::PromptUser {
                    tool_uses,
                    pending_tool_index,
                    skip_printing_tools: true,
                })
            } else {
                Err(ChatError::Custom("ClearCommand can only execute Clear commands".into()))
            }
        })
    }

    fn requires_confirmation(&self, _args: &[&str]) -> bool {
        true // Clear command requires confirmation
    }
}
