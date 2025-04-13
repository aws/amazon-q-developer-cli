use std::future::Future;
use std::io::Write;
use std::pin::Pin;

use crossterm::queue;
use crossterm::style::{
    self,
    Color,
};
use eyre::Result;
use fig_os_shim::Context;

use crate::cli::chat::commands::CommandHandler;
use crate::cli::chat::conversation_state::ChatState;
use crate::cli::chat::{
    ContextExt,
    QueuedTool,
};

/// Handler for the context clear command
pub struct ClearContextCommand {
    global: bool,
}

impl ClearContextCommand {
    pub fn new(global: bool) -> Self {
        Self { global }
    }
}

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

    fn execute<'a>(
        &'a self,
        _args: Vec<&'a str>,
        ctx: &'a Context,
        tool_uses: Option<Vec<QueuedTool>>,
        pending_tool_index: Option<usize>,
    ) -> Pin<Box<dyn Future<Output = Result<ChatState>> + Send + 'a>> {
        Box::pin(async move {
            // Get the conversation state from the context
            let mut stdout = ctx.stdout();
            let conversation_state = ctx.get_conversation_state()?;

            // Get the context manager
            let Some(context_manager) = &mut conversation_state.context_manager else {
                queue!(
                    stdout,
                    style::SetForegroundColor(Color::Red),
                    style::Print("Error: Context manager not initialized\n"),
                    style::ResetColor
                )?;
                stdout.flush()?;
                return Ok(ChatState::PromptUser {
                    tool_uses,
                    pending_tool_index,
                    skip_printing_tools: true,
                });
            };

            // Clear the context
            match context_manager.clear(self.global).await {
                Ok(_) => {
                    // Success message
                    let scope = if self.global { "global" } else { "profile" };
                    queue!(
                        stdout,
                        style::SetForegroundColor(Color::Green),
                        style::Print(format!("Cleared all files from {} context\n", scope)),
                        style::ResetColor
                    )?;
                    stdout.flush()?;
                },
                Err(e) => {
                    // Error message
                    queue!(
                        stdout,
                        style::SetForegroundColor(Color::Red),
                        style::Print(format!("Error: {}\n", e)),
                        style::ResetColor
                    )?;
                    stdout.flush()?;
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

    #[tokio::test]
    async fn test_clear_context_command() {
        let command = ClearContextCommand::new(false);
        assert_eq!(command.name(), "clear");
        assert_eq!(command.description(), "Clear all files from current context");
        assert_eq!(command.usage(), "/context clear [--global]");

        // Note: Full testing would require mocking the context manager
    }
}
