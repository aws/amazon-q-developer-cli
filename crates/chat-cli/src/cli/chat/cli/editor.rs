use clap::Args;
use crossterm::execute;
use crossterm::style::{
    self,
    Attribute,
    Color,
};
use uuid::Uuid;

use crate::cli::chat::{
    ChatError,
    ChatSession,
    ChatState,
};

#[deny(missing_docs)]
#[derive(Debug, PartialEq, Args)]
/// Command-line arguments for the editor functionality
pub struct EditorArgs {
    /// Initial text to populate in the editor
    pub initial_text: Vec<String>,
}

impl EditorArgs {
    pub async fn execute(self, session: &mut ChatSession) -> Result<ChatState, ChatError> {
        let initial_text = if self.initial_text.is_empty() {
            None
        } else {
            Some(self.initial_text.join(" "))
        };

        let content = match open_editor(initial_text) {
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

        Ok(match content.trim().is_empty() {
            true => {
                execute!(
                    session.stderr,
                    style::SetForegroundColor(Color::Yellow),
                    style::Print("\nEmpty content from editor, not submitting.\n\n"),
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
                    style::SetAttribute(Attribute::Reset),
                    style::SetForegroundColor(Color::Magenta),
                    style::Print("> "),
                    style::SetAttribute(Attribute::Reset),
                    style::Print(&content),
                    style::Print("\n")
                )?;

                // Process the content as user input
                ChatState::HandleInput { input: content }
            },
        })
    }
}

/// Opens the user's preferred editor to compose a prompt
pub fn open_editor(initial_text: Option<String>) -> Result<String, ChatError> {
    // Create a temporary file with a unique name
    let temp_dir = std::env::temp_dir();
    let file_name = format!("q_prompt_{}.md", Uuid::new_v4());
    let temp_file_path = temp_dir.join(file_name);

    // Get the editor from environment variable or use a default
    let editor_cmd = std::env::var("EDITOR").unwrap_or_else(|_| "vi".to_string());

    // Parse the editor command to handle arguments
    let mut parts =
        shlex::split(&editor_cmd).ok_or_else(|| ChatError::Custom("Failed to parse EDITOR command".into()))?;

    if parts.is_empty() {
        return Err(ChatError::Custom("EDITOR environment variable is empty".into()));
    }

    let editor_bin = parts.remove(0);

    // Write initial content to the file if provided
    let initial_content = initial_text.unwrap_or_default();
    std::fs::write(&temp_file_path, &initial_content)
        .map_err(|e| ChatError::Custom(format!("Failed to create temporary file: {}", e).into()))?;

    // Open the editor with the parsed command and arguments
    let mut cmd = std::process::Command::new(editor_bin);
    // Add any arguments that were part of the EDITOR variable
    for arg in parts {
        cmd.arg(arg);
    }
    // Add the file path as the last argument
    let status = cmd
        .arg(&temp_file_path)
        .status()
        .map_err(|e| ChatError::Custom(format!("Failed to open editor: {}", e).into()))?;

    if !status.success() {
        return Err(ChatError::Custom("Editor exited with non-zero status".into()));
    }

    // Read the content back
    let content = std::fs::read_to_string(&temp_file_path)
        .map_err(|e| ChatError::Custom(format!("Failed to read temporary file: {}", e).into()))?;

    // Clean up the temporary file
    let _ = std::fs::remove_file(&temp_file_path);

    Ok(content.trim().to_string())
}
