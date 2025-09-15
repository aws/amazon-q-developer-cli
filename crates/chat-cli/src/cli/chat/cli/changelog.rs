use clap::Args;
use eyre::Result;

use crate::cli::chat::{
    ChatError,
    ChatSession,
    ChatState,
};
use crate::util::ui;

#[derive(Debug, PartialEq, Args)]
pub struct ChangelogArgs {}

impl ChangelogArgs {
    pub async fn execute(self, session: &mut ChatSession) -> Result<ChatState, ChatError> {
        // Use the shared rendering function from util::ui
        ui::render_changelog_content(&mut session.stderr).map_err(|e| ChatError::Std(std::io::Error::other(e)))?;

        Ok(ChatState::PromptUser {
            skip_printing_tools: true,
        })
    }
}
