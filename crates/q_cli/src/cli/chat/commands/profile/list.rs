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

/// Handler for the profile list command
pub struct ListProfilesCommand;

impl ListProfilesCommand {
    pub fn new() -> Self {
        Self
    }
}

impl CommandHandler for ListProfilesCommand {
    fn name(&self) -> &'static str {
        "list"
    }

    fn description(&self) -> &'static str {
        "List available profiles"
    }

    fn usage(&self) -> &'static str {
        "/profile list"
    }

    fn help(&self) -> String {
        "List all available profiles. The current profile is marked with an asterisk.".to_string()
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
            let Some(context_manager) = &conversation_state.context_manager else {
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

            // Get the list of profiles
            let profiles = match context_manager.list_profiles().await {
                Ok(profiles) => profiles,
                Err(e) => {
                    queue!(
                        stdout,
                        style::SetForegroundColor(Color::Red),
                        style::Print(format!("Error listing profiles: {}\n", e)),
                        style::ResetColor
                    )?;
                    stdout.flush()?;
                    return Ok(ChatState::PromptUser {
                        tool_uses,
                        pending_tool_index,
                        skip_printing_tools: true,
                    });
                },
            };

            // Display the profiles
            queue!(
                stdout,
                style::SetForegroundColor(Color::Yellow),
                style::Print("\nAvailable profiles:\n"),
                style::ResetColor
            )?;

            for profile in profiles {
                if profile == context_manager.current_profile {
                    queue!(
                        stdout,
                        style::SetForegroundColor(Color::Green),
                        style::Print("* "),
                        style::Print(&profile),
                        style::ResetColor,
                        style::Print(" (current)\n")
                    )?;
                } else {
                    queue!(stdout, style::Print("  "), style::Print(&profile), style::Print("\n"))?;
                }
            }

            queue!(stdout, style::Print("\n"))?;
            stdout.flush()?;

            Ok(ChatState::PromptUser {
                tool_uses,
                pending_tool_index,
                skip_printing_tools: true,
            })
        })
    }

    fn requires_confirmation(&self, _args: &[&str]) -> bool {
        false // List command is read-only and doesn't require confirmation
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
    async fn test_list_profiles_command() {
        let command = ListProfilesCommand::new();
        assert_eq!(command.name(), "list");
        assert_eq!(command.description(), "List available profiles");
        assert_eq!(command.usage(), "/profile list");

        // Note: Full testing would require mocking the context manager
    }
}
