use clap::Args;
use crossterm::style::{
    self,
    Stylize,
};
use crossterm::{
    cursor,
    execute,
};

use crate::cli::chat::context::{
    ContextManager,
    calc_max_context_files_size,
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
        // Write the confirmation prompt directly to stderr to avoid a race condition
        // with the conduit's async event processing. Using session.stderr sends bytes
        // through an async channel that may not flush to the terminal before rustyline
        // starts reading input, causing garbled ANSI output (GitHub #5116).
        let mut stderr = std::io::stderr();
        execute!(
            stderr,
            StyledText::secondary_fg(),
            style::Print(
                "\nAre you sure? This will erase the conversation history, context files, and hooks for the current session. "
            ),
            style::Print("["),
            StyledText::current_item_fg(),
            style::Print("y"),
            StyledText::secondary_fg(),
            style::Print("/"),
            StyledText::current_item_fg(),
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

            // Recreate context_manager from the active agent so that skill
            // inclusions (Auto vs Always) are reset to their initial state.
            // Without this, skills that were loaded during the session would
            // remain fully expanded after /clear (GitHub kirodotdev/Kiro#5909).
            if let Some(agent) = session.conversation.agents.get_active() {
                let max_size = calc_max_context_files_size(session.conversation.model_info.as_ref());
                session.conversation.context_manager = ContextManager::from_agent(agent, max_size).ok();
            } else {
                session.conversation.context_manager = None;
            }

            // Reset pending tool state to prevent orphaned tool approval prompts
            session.tool_uses.clear();
            session.pending_tool_index = None;
            session.tool_turn_start_time = None;

            execute!(
                stderr,
                StyledText::success_fg(),
                style::Print("\nConversation history cleared.\n\n"),
                StyledText::reset(),
            )?;
        }

        Ok(ChatState::default())
    }
}
