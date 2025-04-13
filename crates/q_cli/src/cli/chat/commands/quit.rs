use std::future::Future;
use std::pin::Pin;

use eyre::Result;
use fig_os_shim::Context;

use crate::cli::chat::QueuedTool;
use crate::cli::chat::commands::CommandHandler;
use crate::cli::chat::conversation_state::ChatState;

/// Handler for the quit command
pub struct QuitCommand;

impl QuitCommand {
    pub fn new() -> Self {
        Self
    }
}

impl CommandHandler for QuitCommand {
    fn name(&self) -> &'static str {
        "quit"
    }

    fn description(&self) -> &'static str {
        "Exit the application"
    }

    fn usage(&self) -> &'static str {
        "/quit"
    }

    fn help(&self) -> String {
        "Exits the Amazon Q CLI application.".to_string()
    }

    fn execute<'a>(
        &'a self,
        _args: Vec<&'a str>,
        _ctx: &'a Context,
        _tool_uses: Option<Vec<QueuedTool>>,
        _pending_tool_index: Option<usize>,
    ) -> Pin<Box<dyn Future<Output = Result<ChatState>> + Send + 'a>> {
        Box::pin(async move {
            // Return Exit state directly
            Ok(ChatState::Exit)
        })
    }

    fn requires_confirmation(&self, _args: &[&str]) -> bool {
        true // Quitting should require confirmation
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_quit_command() {
        let command = QuitCommand::new();
        assert_eq!(command.name(), "quit");
        assert_eq!(command.description(), "Exit the application");
        assert_eq!(command.usage(), "/quit");
        assert!(command.requires_confirmation(&[]));

        use crate::cli::chat::commands::test_utils::create_test_context;
        let ctx = create_test_context();
        let result = command.execute(vec![], &ctx, None, None).await;
        assert!(result.is_ok());

        if let Ok(state) = result {
            match state {
                ChatState::Exit => {},
                _ => panic!("Expected Exit state"),
            }
        }
    }
}
