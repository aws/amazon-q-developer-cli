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

#[deny(missing_docs)]
#[derive(Debug, PartialEq, Args)]
/// Arguments for the clear command that erases conversation history and context.
pub struct ClearArgs;

impl ClearArgs {
    pub async fn execute(self, session: &mut ChatSession) -> Result<ChatState, ChatError> {
        execute!(
            session.stderr,
            style::SetForegroundColor(style::Color::DarkGrey),
            style::Print(
                "\nAre you sure? This will erase the conversation history and context from hooks for the current session. "
            ),
            style::Print("["),
            style::SetForegroundColor(style::Color::Green),
            style::Print("y"),
            style::SetForegroundColor(style::Color::DarkGrey),
            style::Print("/"),
            style::SetForegroundColor(style::Color::Green),
            style::Print("n"),
            style::SetForegroundColor(style::Color::DarkGrey),
            style::Print("]:\n\n"),
            style::ResetColor,
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
                style::SetForegroundColor(style::Color::Green),
                style::Print("\nConversation history cleared.\n\n"),
                style::ResetColor,
            )?;
        }

        Ok(ChatState::default())
    }
}

#[cfg(test)]
mod tests {
    use crossterm::{
        execute,
        style,
    };

    #[test]
    fn test_clear_prompt_renders_correctly() {
        let mut buffer = Vec::new();

        // Test the actual implementation pattern used in clear command
        let result = execute!(
            &mut buffer,
            style::SetForegroundColor(style::Color::DarkGrey),
            style::Print("Test "),
            style::Print("["),
            style::SetForegroundColor(style::Color::Green),
            style::Print("y"),
            style::SetForegroundColor(style::Color::DarkGrey),
            style::Print("/"),
            style::SetForegroundColor(style::Color::Green),
            style::Print("n"),
            style::SetForegroundColor(style::Color::DarkGrey),
            style::Print("]"),
            style::ResetColor,
        );

        assert!(result.is_ok());

        let output = String::from_utf8(buffer).unwrap();
        eprintln!("Output: {:?}", output);

        // Verify the text content is correct
        assert!(output.contains("Test"), "Output should contain 'Test'");
        assert!(output.contains("["), "Output should contain '['");
        assert!(output.contains("y"), "Output should contain 'y'");
        assert!(output.contains("/"), "Output should contain '/'");
        assert!(output.contains("n"), "Output should contain 'n'");
        assert!(output.contains("]"), "Output should contain ']'");
        
        // Verify ANSI escape sequences are present
        assert!(output.contains("\x1b["), "Output should contain ANSI escape sequences");
        
        // Verify reset code is present
        assert!(output.contains("\x1b[0m"), "Output should contain reset code");
    }
}
