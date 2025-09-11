use clap::Args;
use crossterm::execute;
use crossterm::style::{self, Color};

use super::editor::open_editor;
use crate::cli::chat::{ChatError, ChatSession, ChatState};

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
                    .map(|line| format!("> {}", line))
                    .collect::<Vec<_>>()
                    .join("\n")
            },
            None => {
                execute!(
                    session.stderr,
                    style::SetForegroundColor(Color::Yellow),
                    style::Print("\nNo assistant message found to reply to.\n\n"),
                    style::SetForegroundColor(Color::Reset)
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
                    style::SetForegroundColor(Color::Red),
                    style::Print(format!("\nError opening editor: {}\n\n", err)),
                    style::SetForegroundColor(Color::Reset)
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
                        style::SetForegroundColor(Color::Yellow),
                        style::Print("\nNo changes made in editor, not submitting.\n\n"),
                        style::SetForegroundColor(Color::Reset)
                    )?;

                    ChatState::PromptUser {
                        skip_printing_tools: true,
                    }
                },
                false => {
                    execute!(
                        session.stderr,
                        style::SetForegroundColor(Color::Green),
                        style::Print("\nContent loaded from editor. Submitting prompt...\n\n"),
                        style::SetForegroundColor(Color::Reset)
                    )?;

                    // Display the content as if the user typed it
                    execute!(
                        session.stderr,
                        style::SetAttribute(style::Attribute::Reset),
                        style::SetForegroundColor(Color::Magenta),
                        style::Print("> "),
                        style::SetAttribute(style::Attribute::Reset),
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
