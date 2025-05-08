use std::future::Future;
use std::pin::Pin;

use super::{
    CommandContextAdapter,
    CommandHandler,
};
use crate::cli::chat::command::Command;
use crate::cli::chat::{
    ChatError,
    ChatState,
    QueuedTool,
};

/// Static instance of the quit command handler
pub static QUIT_HANDLER: QuitCommand = QuitCommand;

/// Quit command handler
#[derive(Clone, Copy)]
pub struct QuitCommand;

impl CommandHandler for QuitCommand {
    fn name(&self) -> &'static str {
        "quit"
    }

    fn description(&self) -> &'static str {
        "Quit the application"
    }

    fn usage(&self) -> &'static str {
        "/quit"
    }

    fn help(&self) -> String {
        "Exit the Amazon Q chat application".to_string()
    }

    fn llm_description(&self) -> String {
        r#"The quit command exits the Amazon Q chat application.

Usage:
- /quit                      Exit the application

This command will prompt for confirmation before exiting.

Examples of statements that may trigger this command:
- "Bye!"
- "Let's quit the application"
- "Exit"
- "Adios"
- "I want to exit"
- "Close the chat"
- "End this session"
Common quit commands from other tools that users might try, that SHOULD also trigger this command:
- ":q" or ":wq" or ":q!" (vi/vim)
- "exit" (shell, Python REPL)
- "quit" (many REPLs)
- "quit()" (Python REPL)
- "logout" (shells)
- "bye" (some interactive tools)"#
            .to_string()
    }

    fn to_command(&self, _args: Vec<&str>) -> Result<Command, ChatError> {
        Ok(Command::Quit)
    }

    fn execute_command<'a>(
        &'a self,
        _command: &'a Command,
        _ctx: &'a mut CommandContextAdapter<'a>,
        _tool_uses: Option<Vec<QueuedTool>>,
        _pending_tool_index: Option<usize>,
    ) -> Pin<Box<dyn Future<Output = Result<ChatState, ChatError>> + Send + 'a>> {
        Box::pin(async move { Ok(ChatState::Exit) })
    }

    // Override the default execute implementation since this command
    // returns ChatState::Exit instead of ChatState::ExecuteCommand
    fn execute<'a>(
        &'a self,
        _args: Vec<&'a str>,
        _ctx: &'a mut CommandContextAdapter<'a>,
        _tool_uses: Option<Vec<QueuedTool>>,
        _pending_tool_index: Option<usize>,
    ) -> Pin<Box<dyn Future<Output = Result<ChatState, ChatError>> + Send + 'a>> {
        Box::pin(async move { Ok(ChatState::Exit) })
    }

    fn requires_confirmation(&self, _args: &[&str]) -> bool {
        true // Quit command requires confirmation
    }
}
