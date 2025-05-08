use std::future::Future;
use std::io::Write;
use std::pin::Pin;
use std::process::Command;
use std::{
    env,
    fs,
};

use crossterm::style::Color;
use crossterm::{
    queue,
    style,
};
use tempfile::NamedTempFile;
use tracing::{
    debug,
    error,
};

use super::context_adapter::CommandContextAdapter;
use super::handler::CommandHandler;
use crate::cli::chat::{
    ChatError,
    ChatState,
    QueuedTool,
};

/// Command handler for the `/editor` command
pub struct EditorCommand;

// Create a static instance of the handler
pub static EDITOR_HANDLER: EditorCommand = EditorCommand;

impl Default for EditorCommand {
    fn default() -> Self {
        Self
    }
}

impl EditorCommand {
    #[allow(dead_code)]
    /// Get the default editor from environment or fallback to platform-specific defaults
    fn get_default_editor() -> String {
        if let Ok(editor) = env::var("EDITOR") {
            return editor;
        }

        #[cfg(target_os = "windows")]
        {
            return "notepad.exe".to_string();
        }

        #[cfg(not(target_os = "windows"))]
        {
            // Try to find common editors
            for editor in &["nano", "vim", "vi", "emacs"] {
                if Command::new("which")
                    .arg(editor)
                    .output()
                    .map(|o| o.status.success())
                    .unwrap_or(false)
                {
                    return (*editor).to_string();
                }
            }

            // Fallback to vi which should be available on most Unix systems
            "vi".to_string()
        }
    }
}

impl CommandHandler for EditorCommand {
    fn name(&self) -> &'static str {
        "editor"
    }

    fn description(&self) -> &'static str {
        "Open an external editor for composing prompts"
    }

    fn usage(&self) -> &'static str {
        "/editor [initial_text]"
    }

    fn help(&self) -> String {
        color_print::cformat!(
            r#"
<magenta,em>External Editor</magenta,em>

Opens your default text editor to compose a longer or more complex prompt.

<cyan!>Usage: /editor [initial_text]</cyan!>

<cyan!>Description</cyan!>
  Opens your system's default text editor (as specified by the EDITOR environment variable)
  with optional initial text. After you save and close the editor, the content is sent as
  a prompt to Amazon Q.

<cyan!>Examples</cyan!>
  /editor
  /editor Please help me with the following code:

<cyan!>Notes</cyan!>
• Uses your system's default editor (EDITOR environment variable)
• Common editors include vim, nano, emacs, VS Code, etc.
• Useful for multi-paragraph prompts or code snippets
• All content from the editor is sent as a single prompt
"#
        )
    }

    fn llm_description(&self) -> String {
        r#"
The editor command opens an external text editor for composing longer or more complex prompts.

Usage:
- /editor [initial_text]

This command:
- Opens the user's default text editor (from EDITOR environment variable)
- Pre-populates the editor with initial_text if provided
- Sends the edited content as a prompt to Amazon Q when the editor is closed

This command is useful when:
- The user wants to compose a multi-paragraph prompt
- The user needs to include code snippets with proper formatting
- The user wants to carefully edit their prompt before sending it
- The prompt contains special characters or formatting

The command takes an optional initial text parameter that will be pre-populated in the editor.

Examples:
- "/editor" - Opens an empty editor
- "/editor Please help me with this code:" - Opens editor with initial text
"#
        .to_string()
    }

    fn to_command(&self, args: Vec<&str>) -> Result<crate::cli::chat::command::Command, ChatError> {
        let initial_text = if !args.is_empty() { Some(args.join(" ")) } else { None };

        Ok(crate::cli::chat::command::Command::PromptEditor { initial_text })
    }

    fn execute_command<'a>(
        &'a self,
        command: &'a crate::cli::chat::command::Command,
        ctx: &'a mut CommandContextAdapter<'a>,
        tool_uses: Option<Vec<QueuedTool>>,
        pending_tool_index: Option<usize>,
    ) -> Pin<Box<dyn Future<Output = Result<ChatState, ChatError>> + Send + 'a>> {
        Box::pin(async move {
            if let crate::cli::chat::command::Command::PromptEditor { initial_text } = command {
                // Create a temporary file for editing
                let mut temp_file = match NamedTempFile::new() {
                    Ok(file) => file,
                    Err(e) => {
                        error!("Failed to create temporary file: {}", e);
                        queue!(
                            ctx.output,
                            style::SetForegroundColor(Color::Red),
                            style::Print("Error: Failed to create temporary file for editor.\n"),
                            style::ResetColor
                        )?;
                        return Ok(ChatState::PromptUser {
                            tool_uses,
                            pending_tool_index,
                            skip_printing_tools: true,
                        });
                    },
                };

                // Write initial text to the file if provided
                if let Some(text) = initial_text {
                    if let Err(e) = temp_file.write_all(text.as_bytes()) {
                        error!("Failed to write initial text to temporary file: {}", e);
                        queue!(
                            ctx.output,
                            style::SetForegroundColor(Color::Red),
                            style::Print("Error: Failed to write initial text to editor.\n"),
                            style::ResetColor
                        )?;
                        return Ok(ChatState::PromptUser {
                            tool_uses,
                            pending_tool_index,
                            skip_printing_tools: true,
                        });
                    }
                    // Flush to ensure content is written before editor opens
                    if let Err(e) = temp_file.flush() {
                        error!("Failed to flush temporary file: {}", e);
                    }
                }

                // Get the path to the temporary file
                let temp_path = temp_file.path().to_string_lossy().to_string();
                debug!("Created temporary file for editor: {}", temp_path);

                // Get the editor command
                let editor = Self::get_default_editor();
                debug!("Using editor: {}", editor);

                // Inform the user
                queue!(
                    ctx.output,
                    style::Print(format!("\nOpening editor ({})...\n", editor)),
                    style::Print("Save and close the editor when you're done.\n\n")
                )?;
                ctx.output.flush()?;

                // Open the editor
                let status = Command::new(&editor).arg(&temp_path).status();

                match status {
                    Ok(exit_status) => {
                        if exit_status.success() {
                            // Read the content from the file
                            match fs::read_to_string(&temp_path) {
                                Ok(content) => {
                                    // Process the content (trim, etc.)
                                    let processed_content = content.trim().to_string();

                                    if processed_content.is_empty() {
                                        queue!(
                                            ctx.output,
                                            style::SetForegroundColor(Color::Yellow),
                                            style::Print("Editor returned empty content. No prompt sent.\n"),
                                            style::ResetColor
                                        )?;
                                        return Ok(ChatState::PromptUser {
                                            tool_uses,
                                            pending_tool_index,
                                            skip_printing_tools: true,
                                        });
                                    }

                                    // Return the content as user input
                                    return Ok(ChatState::HandleInput {
                                        input: processed_content,
                                        tool_uses,
                                        pending_tool_index,
                                    });
                                },
                                Err(e) => {
                                    error!("Failed to read content from temporary file: {}", e);
                                    queue!(
                                        ctx.output,
                                        style::SetForegroundColor(Color::Red),
                                        style::Print("Error: Failed to read content from editor.\n"),
                                        style::ResetColor
                                    )?;
                                },
                            }
                        } else {
                            queue!(
                                ctx.output,
                                style::SetForegroundColor(Color::Yellow),
                                style::Print("Editor exited with an error. No prompt sent.\n"),
                                style::ResetColor
                            )?;
                        }
                    },
                    Err(e) => {
                        error!("Failed to start editor: {}", e);
                        queue!(
                            ctx.output,
                            style::SetForegroundColor(Color::Red),
                            style::Print(format!("Error: Failed to start editor '{}': {}\n", editor, e)),
                            style::ResetColor
                        )?;
                    },
                }

                Ok(ChatState::PromptUser {
                    tool_uses,
                    pending_tool_index,
                    skip_printing_tools: true,
                })
            } else {
                Err(ChatError::Custom(
                    "EditorCommand can only execute PromptEditor commands".into(),
                ))
            }
        })
    }

    fn requires_confirmation(&self, _args: &[&str]) -> bool {
        false // Editor command doesn't require confirmation
    }
}
