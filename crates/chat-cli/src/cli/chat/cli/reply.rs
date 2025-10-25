use clap::Args;
use crossterm::execute;
use crossterm::style::{
    self,
};

use super::editor::open_editor;
use crate::cli::chat::{
    ChatError,
    ChatSession,
    ChatState,
};
use crate::theme::StyledText;

/// Arguments to the `/reply` command.
#[deny(missing_docs)]
#[derive(Debug, PartialEq, Args)]
pub struct ReplyArgs {}

impl ReplyArgs {
    pub async fn execute(self, session: &mut ChatSession) -> Result<ChatState, ChatError> {
        // Get the most recent assistant message from transcript
        let last_assistant_message = session
            .conversation
            .transcript
            .iter()
            .rev()
            .find(|msg| !msg.starts_with("> "))
            .cloned();

        let initial_text = match last_assistant_message {
            Some(msg) => {
                // Format with > prefix for each line
                msg.lines()
                    .map(|line| format!("> {line}"))
                    .collect::<Vec<_>>()
                    .join("\n")
            },
            None => {
                execute!(
                    session.stderr,
                    StyledText::warning_fg(),
                    style::Print("\nNo assistant message found to reply to.\n\n"),
                    StyledText::reset(),
                )?;

                return Ok(ChatState::PromptUser {
                    skip_printing_tools: true,
                });
            },
        };

        let content = match open_editor(Some(initial_text.clone())) {
            Ok(content) => content,
            Err(err) => {
                execute!(
                    session.stderr,
                    StyledText::error_fg(),
                    style::Print(format!("\nError opening editor: {err}\n\n")),
                    StyledText::reset(),
                )?;

                return Ok(ChatState::PromptUser {
                    skip_printing_tools: true,
                });
            },
        };

        Ok(
            match content.trim().is_empty() || content.trim() == initial_text.trim() {
                true => {
                    execute!(
                        session.stderr,
                        StyledText::warning_fg(),
                        style::Print("\nNo changes made in editor, not submitting.\n\n"),
                        StyledText::reset(),
                    )?;

                    ChatState::PromptUser {
                        skip_printing_tools: true,
                    }
                },
                false => {
                    execute!(
                        session.stderr,
                        StyledText::success_fg(),
                        style::Print("\nContent loaded from editor. Submitting prompt...\n\n"),
                        StyledText::reset(),
                    )?;

                    // Display the content as if the user typed it
                    execute!(
                        session.stderr,
                        StyledText::reset_attributes(),
                        StyledText::emphasis_fg(),
                        style::Print("> "),
                        StyledText::reset_attributes(),
                        style::Print(&content),
                        style::Print("\n")
                    )?;

                    // Process the content as user input
                    ChatState::HandleInput { input: content }
                },
            },
        )
    }
}
