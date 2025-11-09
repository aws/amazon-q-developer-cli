use chat_cli_ui::protocol::Event;
use clap::Args;
use crossterm::style::{
    self,
    Stylize,
};
use crossterm::{
    cursor,
    execute,
};
use tracing::error;

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

        // BANDAID FIX: Prevent race condition between display and user input
        // Without this synchronization, user input can start before the confirmation prompt
        // is fully rendered, causing corrupted ANSI escape sequences and malformed output.
        // As a bandaid fix (to hold us until we move to the new event loop where everything is in
        // their rightful place), we signal to the UI layer that display is complete and wait for
        // acknowledgment through the conduit system before reading user input.
        session
            .stderr
            .send(Event::MetaEvent(chat_cli_ui::protocol::MetaEvent {
                meta_type: "timing".to_string(),
                payload: serde_json::Value::String("prompt_user".to_string()),
            }))
            .map_err(|_e| ChatError::Custom("Error sending timing event for prompting user".into()))?;

        // Wait for UI acknowledgment with timeout to ensure display is flushed before user input
        if let Err(e) = session.prompt_ack_rx.recv_timeout(std::time::Duration::from_secs(10)) {
            error!("Failed to receive user prompting acknowledgement from UI: {:?}", e);
        }

        // Now safe to read user input - display is guaranteed to be complete
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
