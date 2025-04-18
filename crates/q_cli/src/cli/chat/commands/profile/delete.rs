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
use crate::cli::chat::context::ContextExt;
use crate::cli::chat::{
    ChatState,
    QueuedTool,
};

/// Handler for the profile delete command
pub struct DeleteProfileCommand {
    name: String,
}

impl DeleteProfileCommand {
    pub fn new(name: &str) -> Self {
        Self { name: name.to_string() }
    }
}

impl CommandHandler for DeleteProfileCommand {
    fn name(&self) -> &'static str {
        "delete"
    }

    fn description(&self) -> &'static str {
        "Delete a profile"
    }

    fn usage(&self) -> &'static str {
        "/profile delete <name>"
    }

    fn help(&self) -> String {
        "Delete a profile with the specified name. You cannot delete the default profile or the currently active profile.".to_string()
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

            // Delete the profile
            match context_manager.delete_profile(&self.name).await {
                Ok(_) => {
                    // Success message
                    queue!(
                        stdout,
                        style::SetForegroundColor(Color::Green),
                        style::Print(format!("\nDeleted profile: {}\n\n", self.name)),
                        style::ResetColor
                    )?;
                },
                Err(e) => {
                    // Error message
                    queue!(
                        stdout,
                        style::SetForegroundColor(Color::Red),
                        style::Print(format!("\nError deleting profile: {}\n\n", e)),
                        style::ResetColor
                    )?;
                },
            }

            stdout.flush()?;

            Ok(ChatState::PromptUser {
                tool_uses,
                pending_tool_index,
                skip_printing_tools: true,
            })
        })
    }

    fn requires_confirmation(&self, _args: &[&str]) -> bool {
        true // Delete command requires confirmation as it's a destructive operation
    }

    fn parse_args<'a>(&self, args: Vec<&'a str>) -> Result<Vec<&'a str>> {
        Ok(args)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::chat::commands::test_utils::create_test_context;

    #[tokio::test]
    async fn test_delete_profile_command() {
        let command = DeleteProfileCommand::new("test");
        assert_eq!(command.name(), "delete");
        assert_eq!(command.description(), "Delete a profile");
        assert_eq!(command.usage(), "/profile delete <name>");

        // Note: Full testing would require mocking the context manager
    }
}
