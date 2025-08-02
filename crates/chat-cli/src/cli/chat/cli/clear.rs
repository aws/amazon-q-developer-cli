use clap::Args;
use crossterm::style::{
    self,
    Color,
    Stylize,
};
use crossterm::{
    cursor,
    execute,
};

use crate::cli::chat::{
    ChatError,
    ChatSession,
    ChatState,
};

#[deny(missing_docs)]
#[derive(Debug, PartialEq, Args)]
pub struct ClearArgs;

impl ClearArgs {
    pub async fn execute(self, session: &mut ChatSession) -> Result<ChatState, ChatError> {
        execute!(
            session.stderr,
            style::SetForegroundColor(Color::DarkGrey),
            style::Print(
                "\nAre you sure? This will erase the conversation history and context from hooks for the current session. "
            ),
            style::Print("["),
            style::SetForegroundColor(Color::Green),
            style::Print("y"),
            style::SetForegroundColor(Color::DarkGrey),
            style::Print("/"),
            style::SetForegroundColor(Color::Green),
            style::Print("n"),
            style::SetForegroundColor(Color::DarkGrey),
            style::Print("]:\n\n"),
            style::SetForegroundColor(Color::Reset),
            cursor::Show,
        )?;

        // Setting `exit_on_single_ctrl_c` for better ux: exit the confirmation dialog rather than the CLI
        let user_input = match session.read_user_input("> ".yellow().to_string().as_str(), true).await {
            Some(input) => input,
            None => "".to_string(),
        };

        if ["y", "Y"].contains(&user_input.as_str()) {
            session.conversation.clear(true);
            if let Some(cm) = session.conversation.context_manager.as_mut() {
                cm.hook_executor.cache.clear();
            }
            execute!(
                session.stderr,
                style::SetForegroundColor(Color::Green),
                style::Print("\nConversation history cleared.\n\n"),
                style::SetForegroundColor(Color::Reset)
            )?;
        }

        Ok(ChatState::default())
    }
}
