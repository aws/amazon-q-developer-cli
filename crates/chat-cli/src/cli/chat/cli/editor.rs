use clap::Args;
use crossterm::execute;
use crossterm::style::{
    self,
};
use uuid::Uuid;

use crate::cli::chat::{
    ChatError,
    ChatSession,
    ChatState,
};
use crate::theme::StyledText;

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
                    StyledText::error_fg(),
                    style::Print(format!("\nError opening editor: {err}\n\n")),
                    StyledText::reset(),
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
                    StyledText::warning_fg(),
                    style::Print("\nEmpty content from editor, not submitting.\n\n"),
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
        })
    }
}

/// Launch the user's preferred editor with the given file path
fn launch_editor(file_path: &std::path::Path) -> Result<(), ChatError> {
    // Get the editor from environment variable or use a default
    let editor_cmd = std::env::var("EDITOR").unwrap_or_else(|_| "vi".to_string());

    // Parse the editor command to handle arguments
    let mut parts =
        shlex::split(&editor_cmd).ok_or_else(|| ChatError::Custom("Failed to parse EDITOR command".into()))?;

    if parts.is_empty() {
        return Err(ChatError::Custom("EDITOR environment variable is empty".into()));
    }

    let editor_bin = parts.remove(0);

    // Open the editor with the parsed command and arguments
    let mut cmd = std::process::Command::new(editor_bin);
    // Add any arguments that were part of the EDITOR variable
    for arg in parts {
        cmd.arg(arg);
    }
    // Add the file path as the last argument
    let status = cmd
        .arg(file_path)
        .status()
        .map_err(|e| ChatError::Custom(format!("Failed to open editor: {e}").into()))?;

    if !status.success() {
        return Err(ChatError::Custom("Editor exited with non-zero status".into()));
    }

    Ok(())
}

/// Opens the user's preferred editor to edit an existing file
pub fn open_editor_file(file_path: &std::path::Path) -> Result<(), ChatError> {
    launch_editor(file_path)
}

/// Opens the user's preferred editor to compose a prompt
pub fn open_editor(initial_text: Option<String>) -> Result<String, ChatError> {
    // Create a temporary file with a unique name
    let temp_dir = std::env::temp_dir();
    let file_name = format!("q_prompt_{}.md", Uuid::new_v4());
    let temp_file_path = temp_dir.join(file_name);

    // Write initial content to the file if provided
    let initial_content = initial_text.unwrap_or_default();
    std::fs::write(&temp_file_path, &initial_content)
        .map_err(|e| ChatError::Custom(format!("Failed to create temporary file: {e}").into()))?;

    // Launch the editor
    launch_editor(&temp_file_path)?;

    // Read the content back
    let content = std::fs::read_to_string(&temp_file_path)
        .map_err(|e| ChatError::Custom(format!("Failed to read temporary file: {e}").into()))?;

    // Clean up the temporary file
    let _ = std::fs::remove_file(&temp_file_path);

    Ok(content.trim().to_string())
}
