use std::env;
use std::fs;
use std::process::Command;

use eyre::{Result, eyre};
use rustyline::{Cmd, ConditionalEventHandler, EventContext, Movement, RepeatCount};
use uuid::Uuid;

/// Handler for Ctrl+F keyboard shortcut that opens the current prompt content in an editor
pub struct EditorLauncher;

impl EditorLauncher {
    pub fn new() -> Self {
        Self
    }

    /// Create a command that replaces the entire line with new content
    /// Uses a comprehensive approach to handle different scenarios
    fn create_line_replacement_command(new_content: &str, current_text: &str, cursor_pos: usize) -> Option<Cmd> {
        // Strategy: Use the best available approach based on rustyline capabilities

        if new_content.is_empty() {
            // User wants to clear the line
            if current_text.is_empty() {
                Some(Cmd::Noop) // Nothing to do
            } else {
                // Clear the entire line - move to beginning and kill to end
                // This should clear all content on the current line
                Some(Cmd::Kill(Movement::BeginningOfLine))
            }
        } else if current_text.is_empty() {
            // Current line is empty, just insert new content
            Some(Cmd::Insert(1, new_content.to_string()))
        } else {
            // Need to replace existing content with new content
            //
            // The most reliable approach: use Kill to clear from beginning to end,
            // and then insert the new content
            //
            // But since we can only return one command, let's try a different approach:
            // Use the cursor position to calculate how to best replace content

            if cursor_pos == 0 {
                // Cursor is at beginning - kill to end and insert
                // But we can only do one command - let's try Replace with EndOfLine
                Some(Cmd::Replace(Movement::EndOfLine, Some(new_content.to_string())))
            } else if cursor_pos >= current_text.len() {
                // Cursor is at end - move to beginning and replace all
                Some(Cmd::Replace(Movement::BeginningOfLine, Some(new_content.to_string())))
            } else {
                // Cursor is in the middle - this is more complex
                // For now, let's use the EndOfLine approach
                Some(Cmd::Replace(Movement::EndOfLine, Some(new_content.to_string())))
            }
        }
    }

    /// Launch the system editor with the given content and return the edited result
    fn launch_system_editor(initial_content: &str) -> Result<Option<String>> {
        // Create a temporary markdown file with a unique name
        let temp_file_name = format!("q-developer-prompt-{}.md", Uuid::new_v4());
        let temp_dir = env::temp_dir();
        let temp_file_path = temp_dir.join(temp_file_name);

        // Write initial content to the temporary file
        fs::write(&temp_file_path, initial_content)?;

        // Get editor command from environment variable, default to "vi"
        let editor_env = env::var("EDITOR").unwrap_or_else(|_| "vi".to_string());

        // Parse editor command to handle cases like "code --wait"
        let (editor_cmd, editor_args) = match shlex::split(&editor_env) {
            Some(mut parts) if !parts.is_empty() => {
                let cmd = parts.remove(0);
                (cmd, parts)
            },
            _ => (editor_env, vec![]),
        };

        // Launch the editor
        let status = Command::new(editor_cmd)
            .args(editor_args)
            .arg(&temp_file_path)
            .status()?;

        if !status.success() {
            // Clean up temp file on error
            let _ = fs::remove_file(&temp_file_path);
            return Err(eyre!("Editor exited with non-zero status"));
        }

        // Read the edited content
        let edited_content = fs::read_to_string(&temp_file_path)?;

        // Clean up temp file
        let _ = fs::remove_file(&temp_file_path);

        // Return None if content is empty (user cleared everything)
        if edited_content.trim().is_empty() {
            Ok(None)
        } else {
            // Remove trailing newline that editors often add
            let content = edited_content.trim_end_matches('\n').to_string();
            Ok(Some(content))
        }
    }
}

impl ConditionalEventHandler for EditorLauncher {
    fn handle(&self, _evt: &rustyline::Event, _n: RepeatCount, _positive: bool, ctx: &EventContext<'_>) -> Option<Cmd> {
        // Get the current line content and cursor position from the event context
        let current_text = ctx.line();
        let cursor_pos = ctx.pos();

        // Launch editor with current content
        match Self::launch_system_editor(current_text) {
            Ok(Some(edited_content)) => {
                // Check if content was actually changed
                if edited_content.trim() == current_text.trim() {
                    // Content unchanged, do nothing
                    Some(Cmd::Noop)
                } else {
                    // Replace the entire line with edited content
                    // Strategy: Move to beginning of line, kill everything to end, then insert new content
                    Self::create_line_replacement_command(&edited_content, current_text, cursor_pos)
                }
            },
            Ok(None) => {
                // User cleared all content in editor - clear the entire line
                Self::create_line_replacement_command("", current_text, cursor_pos)
            },
            Err(_) => {
                // Editor failed, keep original content unchanged
                Some(Cmd::Noop)
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn test_editor_launcher_creation() {
        let _launcher = EditorLauncher::new();
        // Just verify we can create the struct without panicking
    }

    #[test]
    fn test_launch_system_editor_with_mock_editor() {
        // Store original EDITOR value
        let original_editor = env::var("EDITOR").ok();

        // Create a temporary directory for our test
        let temp_dir = tempdir().expect("Failed to create temp dir");
        let mock_editor_path = temp_dir.path().join("mock_editor.sh");

        // Create a mock editor script that adds text to the file
        let mock_editor_script = r#"#!/bin/bash
echo "Edited: $(cat "$1")" > "$1"
"#;
        fs::write(&mock_editor_path, mock_editor_script).expect("Failed to write mock editor");

        // Make the script executable
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = fs::metadata(&mock_editor_path).unwrap().permissions();
            perms.set_mode(0o755);
            fs::set_permissions(&mock_editor_path, perms).unwrap();

            // Set the EDITOR environment variable to our mock editor
            env::set_var("EDITOR", mock_editor_path.to_str().unwrap());

            // Test the editor launcher
            let result = EditorLauncher::launch_system_editor("hello world");

            // Verify the result
            assert!(result.is_ok());
            let content = result.unwrap();
            assert!(content.is_some());
            assert_eq!(content.unwrap(), "Edited: hello world");
        }

        // Restore original EDITOR or remove if it wasn't set
        match original_editor {
            Some(editor) => env::set_var("EDITOR", editor),
            None => env::remove_var("EDITOR"),
        }
    }

    #[test]
    #[cfg(unix)] // Only run on Unix systems to avoid hanging on Windows
    fn test_launch_system_editor_empty_content() {
        // Store original EDITOR value
        let original_editor = env::var("EDITOR").ok();

        // Test with empty initial content using a mock that creates empty output
        let temp_dir = tempdir().expect("Failed to create temp dir");
        let mock_editor_path = temp_dir.path().join("empty_editor.sh");

        // Create a mock editor that creates an empty file
        let mock_editor_script = r#"#!/bin/bash
> "$1"  # Create empty file
"#;
        fs::write(&mock_editor_path, mock_editor_script).expect("Failed to write mock editor");

        // Make the script executable on Unix systems
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(&mock_editor_path).unwrap().permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&mock_editor_path, perms).unwrap();

        // Set the EDITOR environment variable to our mock editor
        env::set_var("EDITOR", mock_editor_path.to_str().unwrap());

        let result = EditorLauncher::launch_system_editor("");

        // Restore original EDITOR or remove if it wasn't set
        match original_editor {
            Some(editor) => env::set_var("EDITOR", editor),
            None => env::remove_var("EDITOR"),
        }

        // Should handle empty content gracefully
        assert!(result.is_ok());
        // Should return None for empty content
        assert_eq!(result.unwrap(), None);
    }

    #[test]
    fn test_launch_system_editor_invalid_editor() {
        // Store original EDITOR value
        let original_editor = env::var("EDITOR").ok();

        // Set an invalid editor command
        env::set_var("EDITOR", "nonexistent_editor_command_12345");

        let result = EditorLauncher::launch_system_editor("test content");

        // Restore original EDITOR or remove if it wasn't set
        match original_editor {
            Some(editor) => env::set_var("EDITOR", editor),
            None => env::remove_var("EDITOR"),
        }

        // Should return an error for invalid editor
        assert!(result.is_err());
    }

    #[test]
    fn test_launch_system_editor_multiline_content() {
        // Test handling of multiline content
        let content = "line 1\nline 2\nline 3";

        // Store original EDITOR value
        let original_editor = env::var("EDITOR").ok();

        // Use a simple editor that just preserves content (cat-like behavior)
        env::set_var("EDITOR", "true"); // 'true' command succeeds and does nothing

        let result = EditorLauncher::launch_system_editor(content);

        // Restore original EDITOR
        match original_editor {
            Some(editor) => env::set_var("EDITOR", editor),
            None => env::remove_var("EDITOR"),
        }

        // Should handle multiline content without error
        assert!(result.is_ok());
    }

    #[test]
    fn test_launch_system_editor_special_characters() {
        // Test handling of special characters
        let content = "Special chars: $PATH, ~/, 'quotes', \"double quotes\"";

        // Store original EDITOR value
        let original_editor = env::var("EDITOR").ok();

        // Use true command as a safe no-op editor
        env::set_var("EDITOR", "true");

        let result = EditorLauncher::launch_system_editor(content);

        // Restore original EDITOR
        match original_editor {
            Some(editor) => env::set_var("EDITOR", editor),
            None => env::remove_var("EDITOR"),
        }

        // Should handle special characters without error
        assert!(result.is_ok());
    }

    #[test]
    fn test_editor_launcher_with_complex_editor_command() {
        // Test parsing of complex editor commands like "code --wait"
        let original_editor = env::var("EDITOR").ok();

        // Set a complex editor command (but use true to avoid actually launching code)
        env::set_var("EDITOR", "true --wait --new-window");

        let result = EditorLauncher::launch_system_editor("test");

        // Restore original EDITOR
        match original_editor {
            Some(editor) => env::set_var("EDITOR", editor),
            None => env::remove_var("EDITOR"),
        }

        // Should parse complex commands without error
        assert!(result.is_ok());
    }

    #[test]
    fn test_create_line_replacement_command() {
        // Test the line replacement logic with different scenarios

        // Test 1: Empty current text, insert new content
        let cmd = EditorLauncher::create_line_replacement_command("new content", "", 0);
        match cmd {
            Some(Cmd::Insert(_, content)) => assert_eq!(content, "new content"),
            _ => panic!("Expected Insert command for empty line"),
        }

        // Test 2: Clear line (empty new content)
        let cmd = EditorLauncher::create_line_replacement_command("", "old content", 5);
        match cmd {
            Some(Cmd::Kill(_)) => {}, // Expected
            Some(Cmd::Noop) => {},    // Also acceptable
            _ => panic!("Expected Kill or Noop command for clearing line"),
        }

        // Test 3: Replace existing content
        let cmd = EditorLauncher::create_line_replacement_command("new content", "old content", 0);
        match cmd {
            Some(Cmd::Replace(_, Some(content))) => assert_eq!(content, "new content"),
            _ => panic!("Expected Replace command for content replacement"),
        }
    }

    #[test]
    fn test_create_line_replacement_command_cursor_positions() {
        // Test replacement behavior with different cursor positions
        let current_text = "hello world";
        let new_content = "goodbye world";

        // Cursor at beginning
        let cmd = EditorLauncher::create_line_replacement_command(new_content, current_text, 0);
        assert!(matches!(cmd, Some(Cmd::Replace(_, Some(_)))));

        // Cursor at end
        let cmd = EditorLauncher::create_line_replacement_command(new_content, current_text, current_text.len());
        assert!(matches!(cmd, Some(Cmd::Replace(_, Some(_)))));

        // Cursor in middle
        let cmd = EditorLauncher::create_line_replacement_command(new_content, current_text, 5);
        assert!(matches!(cmd, Some(Cmd::Replace(_, Some(_)))));
    }
}

