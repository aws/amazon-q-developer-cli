use std::future::Future;
use std::io::Write;
use std::pin::Pin;

use crossterm::queue;
use crossterm::style::{
    self,
    Color,
};
use eyre::{
    Result,
    eyre,
};
use fig_os_shim::Context;

use crate::cli::chat::commands::CommandHandler;
use crate::cli::chat::context_adapter::ContextExt;
use crate::cli::chat::{
    ChatState,
    QueuedTool,
};

/// Handler for the context remove command
pub struct RemoveContextCommand {
    global: bool,
    paths: Vec<String>,
}

impl RemoveContextCommand {
    pub fn new(global: bool, paths: Vec<&str>) -> Self {
        Self {
            global,
            paths: paths.iter().map(|p| (*p).to_string()).collect(),
        }
    }
}

impl CommandHandler for RemoveContextCommand {
    fn name(&self) -> &'static str {
        "remove"
    }

    fn description(&self) -> &'static str {
        "Remove file(s) from context"
    }

    fn usage(&self) -> &'static str {
        "/context rm [--global] <path1> [path2...]"
    }

    fn help(&self) -> String {
        "Remove files from the context. Use --global to remove from global context.".to_string()
    }

    fn execute<'a>(
        &'a self,
        _args: Vec<&'a str>,
        ctx: &'a Context,
        tool_uses: Option<Vec<QueuedTool>>,
        pending_tool_index: Option<usize>,
    ) -> Pin<Box<dyn Future<Output = Result<ChatState>> + Send + 'a>> {
        Box::pin(async move {
            // Check if paths are provided
            if self.paths.is_empty() {
                return Err(eyre!("No paths specified. Usage: {}", self.usage()));
            }

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

            // Remove the paths from the context
            match context_manager.remove_paths(self.paths.clone(), self.global).await {
                Ok(_) => {
                    // Success message
                    let scope = if self.global { "global" } else { "profile" };
                    queue!(
                        stdout,
                        style::SetForegroundColor(Color::Green),
                        style::Print(format!("Removed path(s) from {} context\n", scope)),
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
        true // Removing context files requires confirmation as it's a destructive operation
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_remove_context_command_no_paths() {
        let command = RemoveContextCommand::new(false, vec![]);
        use crate::cli::chat::commands::test_utils::create_test_context;
        let ctx = create_test_context();
        let result = command.execute(vec![], &ctx, None, None).await;
        assert!(result.is_err());
    }
}
