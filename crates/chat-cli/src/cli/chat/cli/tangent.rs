use clap::Args;
use crossterm::execute;
use crossterm::style::{self, Color};

use crate::cli::chat::{ChatError, ChatSession, ChatState};
use crate::os::Os;

#[derive(Debug, PartialEq, Args)]
pub struct TangentArgs;

impl TangentArgs {
    pub async fn execute(self, os: &Os, session: &mut ChatSession) -> Result<ChatState, ChatError> {
        if session.conversation.is_in_tangent_mode() {
            session.conversation.exit_tangent_mode();
            execute!(
                session.stderr,
                style::SetForegroundColor(Color::DarkGrey),
                style::Print("Restored conversation from checkpoint ("),
                style::SetForegroundColor(Color::Yellow),
                style::Print("↯"),
                style::SetForegroundColor(Color::DarkGrey),
                style::Print("). - Returned to main conversation.\n"),
                style::SetForegroundColor(Color::Reset)
            )?;
        } else {
            session.conversation.enter_tangent_mode();

            // Get the configured tangent mode key for display
            let tangent_key_char = match os
                .database
                .settings
                .get_string(crate::database::settings::Setting::TangentModeKey)
            {
                Some(key) if key.len() == 1 => key.chars().next().unwrap_or('t'),
                _ => 't', // Default to 't' if setting is missing or invalid
            };
            let tangent_key_display = format!("ctrl + {}", tangent_key_char.to_lowercase());

            execute!(
                session.stderr,
                style::SetForegroundColor(Color::DarkGrey),
                style::Print("Created a conversation checkpoint ("),
                style::SetForegroundColor(Color::Yellow),
                style::Print("↯"),
                style::SetForegroundColor(Color::DarkGrey),
                style::Print("). Use "),
                style::SetForegroundColor(Color::Green),
                style::Print(&tangent_key_display),
                style::SetForegroundColor(Color::DarkGrey),
                style::Print(" or "),
                style::SetForegroundColor(Color::Green),
                style::Print("/tangent"),
                style::SetForegroundColor(Color::DarkGrey),
                style::Print(" to restore the conversation later.\n"),
                style::Print(
                    "Note: this functionality is experimental and may change or be removed in the future.\n"
                ),
                style::SetForegroundColor(Color::Reset)
            )?;
        }

        Ok(ChatState::PromptUser {
            skip_printing_tools: false,
        })
    }
}
