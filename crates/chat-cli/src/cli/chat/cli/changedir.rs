use std::path::PathBuf;

use clap::Args;
use crossterm::{
    execute,
    style,
};

use crate::cli::chat::{
    ChatError,
    ChatSession,
    ChatState,
};
use crate::theme::StyledText;

#[derive(Debug, PartialEq, Args)]
/// Arguments for the changedir command.
pub struct ChangedirArgs {
    /// The directory to switch to. Defaults to $HOME if not provided.
    pub path: Option<PathBuf>,
}

impl ChangedirArgs {
    pub async fn execute(self, session: &mut ChatSession) -> Result<ChatState, ChatError> {
        let target = match self.path {
            Some(p) => p,
            None => dirs::home_dir().ok_or_else(|| ChatError::Custom("Could not determine home directory".into()))?,
        };

        let target = if target.is_relative() {
            std::env::current_dir()
                .map_err(|e| ChatError::Custom(e.to_string().into()))?
                .join(&target)
        } else {
            target
        };

        std::env::set_current_dir(&target)
            .map_err(|e| ChatError::Custom(format!("Failed to change directory: {e}").into()))?;

        execute!(
            session.stderr,
            StyledText::success_fg(),
            style::Print(format!("Working directory changed to: {}\n", target.display())),
            style::Print("Run /code init to reinitialize code intelligence for this directory.\n"),
            StyledText::reset(),
        )?;

        Ok(ChatState::PromptUser {
            skip_printing_tools: true,
        })
    }
}
