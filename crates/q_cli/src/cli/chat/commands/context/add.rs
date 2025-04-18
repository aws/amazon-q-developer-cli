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
use crate::cli::chat::context::ContextExt;
use crate::cli::chat::{
    ChatState,
    QueuedTool,
};

/// Handler for the context add command
pub struct AddContextCommand {
    global: bool,
    force: bool,
    paths: Vec<String>,
}

impl AddContextCommand {
    pub fn new(global: bool, force: bool, paths: Vec<&str>) -> Self {
        Self {
            global,
            force,
            paths: paths.iter().map(|p| (*p).to_string()).collect(),
        }
    }
}

impl CommandHandler for AddContextCommand {
    fn name(&self) -> &'static str {
        "add"
    }

    fn description(&self) -> &'static str {
        "Add file(s) to context"
    }

    fn usage(&self) -> &'static str {
        "/context add [--global] [--force] <path1> [path2...]"
    }

    fn help(&self) -> String {
        "Add files to the context. Use --global to add to global context (available in all profiles). Use --force to add files even if they exceed size limits.".to_string()
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

            // Add the paths to the context
            match context_manager
                .add_paths(self.paths.clone(), self.global, self.force)
                .await
            {
                Ok(_) => {
                    // Success message
                    let scope = if self.global { "global" } else { "profile" };
                    queue!(
                        stdout,
                        style::SetForegroundColor(Color::Green),
                        style::Print(format!("Added {} file(s) to {} context\n", self.paths.len(), scope)),
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

            // Return to prompt
            Ok(ChatState::PromptUser {
                tool_uses,
                pending_tool_index,
                skip_printing_tools: true,
            })
        })
    }

    fn requires_confirmation(&self, _args: &[&str]) -> bool {
        false // Adding context files doesn't require confirmation
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_add_context_command_no_paths() {
        let command = AddContextCommand::new(false, false, vec![]);
        use crate::cli::chat::commands::test_utils::create_test_context;
        let ctx = create_test_context();
        let result = command.execute(vec![], &ctx, None, None).await;
        assert!(result.is_err());
    }
}
