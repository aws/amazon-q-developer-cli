use clap::Args;
use crossterm::execute;
use crossterm::style::{
    self,
    Color,
};

use crate::cli::chat::util::clipboard::paste_image_from_clipboard;
use crate::cli::chat::{
    ChatError,
    ChatSession,
    ChatState,
};
use crate::os::Os;

#[derive(Debug, Args, PartialEq)]
pub struct PasteArgs;

impl PasteArgs {
    pub async fn execute(self, _os: &mut Os, session: &mut ChatSession) -> Result<ChatState, ChatError> {
        match paste_image_from_clipboard() {
            Ok(path) => Ok(ChatState::HandleInput {
                input: path.display().to_string(),
            }),
            Err(e) => {
                execute!(
                    session.stderr,
                    style::SetForegroundColor(Color::Red),
                    style::Print("❌ Failed to paste image: "),
                    style::SetForegroundColor(Color::Reset),
                    style::Print(format!("{}\n", e))
                )?;

                Ok(ChatState::PromptUser {
                    skip_printing_tools: false,
                })
            },
        }
    }
}
