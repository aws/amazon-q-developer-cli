use clap::Args;
use crossterm::style::{
    self,
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
use crate::theme::StyledText;

#[deny(missing_docs)]
#[derive(Debug, PartialEq, Args)]
/// Arguments for the clear command that erases conversation history and context.
pub struct ClearArgs;

impl ClearArgs {
    pub async fn execute(self, session: &mut ChatSession) -> Result<ChatState, ChatError> {
        execute!(
            session.stderr,
            StyledText::secondary_fg(),
            style::Print(
                "\nAre you sure? This will erase the conversation history and context from hooks for the current session. "
            ),
            style::Print("["),
            StyledText::success_fg(),
            style::Print("y"),
            StyledText::secondary_fg(),
            style::Print("/"),
            StyledText::success_fg(),
            style::Print("n"),
            StyledText::secondary_fg(),
            style::Print("]:\n\n"),
            StyledText::reset(),
            cursor::Show,
        )?;

        // Setting `exit_on_single_ctrl_c` for better ux: exit the confirmation dialog rather than the CLI
        let user_input = match session.read_user_input("> ".yellow().to_string().as_str(), true) {
            Some(input) => input,
            None => "".to_string(),
        };

        if ["y", "Y"].contains(&user_input.as_str()) {
            session.conversation.clear();
            if let Some(cm) = session.conversation.context_manager.as_mut() {
                cm.hook_executor.cache.clear();
            }

            // Reset pending tool state to prevent orphaned tool approval prompts
            session.tool_uses.clear();
            session.pending_tool_index = None;
            session.tool_turn_start_time = None;

            execute!(
                session.stderr,
                StyledText::success_fg(),
                style::Print("\nConversation history cleared.\n\n"),
                StyledText::reset(),
            )?;
        }

        Ok(ChatState::default())
    }
}
