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

/// Handler for the context show command
pub struct ShowContextCommand {
    global: bool,
    expand: bool,
}

impl ShowContextCommand {
    pub fn new(global: bool, expand: bool) -> Self {
        Self { global, expand }
    }
}

impl CommandHandler for ShowContextCommand {
    fn name(&self) -> &'static str {
        "show"
    }

    fn description(&self) -> &'static str {
        "Display current context configuration"
    }

    fn usage(&self) -> &'static str {
        "/context show [--global] [--expand]"
    }

    fn help(&self) -> String {
        "Display the current context configuration. Use --global to show only global context. Use --expand to show expanded file contents.".to_string()
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

            // Display current profile
            queue!(
                stdout,
                style::SetForegroundColor(Color::Blue),
                style::Print(format!("Current profile: {}\n", context_manager.current_profile)),
                style::ResetColor
            )?;

            // Always show global context paths
            queue!(
                stdout,
                style::SetForegroundColor(Color::Yellow),
                style::Print("\nGlobal context paths:\n"),
                style::ResetColor
            )?;

            if context_manager.global_config.paths.is_empty() {
                queue!(stdout, style::Print("  (none)\n"))?;
            } else {
                for path in &context_manager.global_config.paths {
                    queue!(stdout, style::Print(format!("  {}\n", path)))?;
                }

                // If expand is requested, show the expanded files
                if self.expand {
                    let expanded_files = context_manager.get_global_context_files(true).await?;
                    queue!(
                        stdout,
                        style::SetForegroundColor(Color::Yellow),
                        style::Print("\nExpanded global context files:\n"),
                        style::ResetColor
                    )?;

                    if expanded_files.is_empty() {
                        queue!(stdout, style::Print("  (none)\n"))?;
                    } else {
                        for (path, _) in expanded_files {
                            queue!(stdout, style::Print(format!("  {}\n", path)))?;
                        }
                    }
                }
            }

            // Display profile-specific context paths if not showing only global
            if !self.global {
                queue!(
                    stdout,
                    style::SetForegroundColor(Color::Yellow),
                    style::Print(format!(
                        "\nProfile '{}' context paths:\n",
                        context_manager.current_profile
                    )),
                    style::ResetColor
                )?;

                if context_manager.profile_config.paths.is_empty() {
                    queue!(stdout, style::Print("  (none)\n"))?;
                } else {
                    for path in &context_manager.profile_config.paths {
                        queue!(stdout, style::Print(format!("  {}\n", path)))?;
                    }

                    // If expand is requested, show the expanded files
                    if self.expand {
                        let expanded_files = context_manager.get_current_profile_context_files(true).await?;
                        queue!(
                            stdout,
                            style::SetForegroundColor(Color::Yellow),
                            style::Print(format!(
                                "\nExpanded profile '{}' context files:\n",
                                context_manager.current_profile
                            )),
                            style::ResetColor
                        )?;

                        if expanded_files.is_empty() {
                            queue!(stdout, style::Print("  (none)\n"))?;
                        } else {
                            for (path, _) in expanded_files {
                                queue!(stdout, style::Print(format!("  {}\n", path)))?;
                            }
                        }
                    }
                }
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
        false // Show command is read-only and doesn't require confirmation
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_show_context_command() {
        let command = ShowContextCommand::new(false, false);
        assert_eq!(command.name(), "show");
        assert_eq!(command.description(), "Display current context configuration");
        assert_eq!(command.usage(), "/context show [--global] [--expand]");

        // Note: Full testing would require mocking the context manager
    }
}
