use clap::Subcommand;
use crossterm::queue;
use crossterm::style::{
    self,
    Color,
};
use eyre::Result;

use crate::cli::chat::{
    ChatError,
    ChatSession,
    ChatState,
};
use crate::os::Os;
use crate::util::command_manager::CommandManager;

/// Custom commands management
#[derive(Clone, Debug, PartialEq, Eq, Subcommand)]
pub enum CommandsSubcommand {
    /// Add a new custom command
    Add { name: String },
}

impl CommandsSubcommand {
    pub async fn execute(self, os: &Os, session: &mut ChatSession) -> Result<ChatState, ChatError> {
        if !CommandManager::is_enabled(os) {
            Self::write_feature_disabled_message(session)?;
            return Ok(Self::default_chat_state());
        }

        let result = self.execute_operation(os, session).await;
        Self::write_operation_result(session, result)?;
        Ok(Self::default_chat_state())
    }

    fn write_feature_disabled_message(session: &mut ChatSession) -> Result<(), std::io::Error> {
        queue!(
            session.stderr,
            style::SetForegroundColor(Color::Red),
            style::Print("\n❌ Commands tool is disabled. Enable it with: q settings chat.enableCommands true\n\n"),
            style::SetForegroundColor(Color::Reset)
        )
    }

    fn default_chat_state() -> ChatState {
        ChatState::PromptUser {
            skip_printing_tools: true,
        }
    }

    async fn execute_operation(&self, os: &Os, _session: &mut ChatSession) -> OperationResult {
        match self {
            CommandsSubcommand::Add { name } => Self::handle_add(os, name).await,
        }
    }

    async fn handle_add(os: &Os, name: &str) -> OperationResult {
        let mut manager = match CommandManager::new(os) {
            Ok(manager) => manager,
            Err(e) => return OperationResult::Error(format!("Failed to initialize command manager: {}", e)),
        };

        match manager.add_command(name, os) {
            Ok(message) => OperationResult::Success(message),
            Err(e) => OperationResult::Error(format!("Failed to add command: {}", e)),
        }
    }

    fn write_operation_result(session: &mut ChatSession, result: OperationResult) -> Result<(), std::io::Error> {
        match result {
            OperationResult::Success(msg) => {
                queue!(
                    session.stderr,
                    style::SetForegroundColor(Color::Green),
                    style::Print(format!("\n{}\n\n", msg)),
                    style::SetForegroundColor(Color::Reset)
                )
            },
            OperationResult::Error(msg) => {
                queue!(
                    session.stderr,
                    style::SetForegroundColor(Color::Red),
                    style::Print(format!("\n❌ Error: {}\n\n", msg)),
                    style::SetForegroundColor(Color::Reset)
                )
            },
        }
    }
}

#[derive(Debug)]
enum OperationResult {
    Success(String),
    Error(String),
}
