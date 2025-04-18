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

/// Handler for the profile set command
pub struct SetProfileCommand {
    name: String,
}

impl SetProfileCommand {
    pub fn new(name: &str) -> Self {
        Self { name: name.to_string() }
    }
}

impl CommandHandler for SetProfileCommand {
    fn name(&self) -> &'static str {
        "set"
    }

    fn description(&self) -> &'static str {
        "Switch to a profile"
    }

    fn usage(&self) -> &'static str {
        "/profile set <name>"
    }

    fn help(&self) -> String {
        "Switch to a profile with the specified name. The profile must exist.".to_string()
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

            // Check if we're already on the requested profile
            if context_manager.current_profile == self.name {
                queue!(
                    stdout,
                    style::SetForegroundColor(Color::Yellow),
                    style::Print(format!("\nAlready on profile: {}\n\n", self.name)),
                    style::ResetColor
                )?;
                stdout.flush()?;
                return Ok(ChatState::PromptUser {
                    tool_uses,
                    pending_tool_index,
                    skip_printing_tools: true,
                });
            }

            // Switch to the profile
            match context_manager.switch_profile(&self.name).await {
                Ok(_) => {
                    // Success message
                    queue!(
                        stdout,
                        style::SetForegroundColor(Color::Green),
                        style::Print(format!("\nSwitched to profile: {}\n\n", self.name)),
                        style::ResetColor
                    )?;
                },
                Err(e) => {
                    // Error message
                    queue!(
                        stdout,
                        style::SetForegroundColor(Color::Red),
                        style::Print(format!("\nError switching to profile: {}\n\n", e)),
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
        false // Set command doesn't require confirmation
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
    async fn test_set_profile_command() {
        let command = SetProfileCommand::new("test");
        assert_eq!(command.name(), "set");
        assert_eq!(command.description(), "Switch to a profile");
        assert_eq!(command.usage(), "/profile set <name>");

        // Note: Full testing would require mocking the context manager
    }
}
