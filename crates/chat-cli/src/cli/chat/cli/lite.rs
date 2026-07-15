use clap::Args;
use crossterm::execute;
use crossterm::style::{
    self,
    Stylize,
};

use crate::cli::chat::{
    ChatError,
    ChatSession,
    ChatState,
};
use crate::theme::StyledText;

#[deny(missing_docs)]
#[derive(Debug, PartialEq, Args)]
/// /lite command arguments.
pub struct LiteArgs;

impl LiteArgs {
    pub async fn execute(self, session: &mut ChatSession) -> Result<ChatState, ChatError> {
        let mut stderr = std::io::stderr();

        // Hidden from autocomplete when Lite is unavailable; guard anyway in
        // case it's typed directly.
        if !crate::cli::chat::lite_enabled() {
            execute!(
                stderr,
                StyledText::warning_fg(),
                style::Print("\nThe Lite UI isn't available for your account yet.\n\n"),
                StyledText::reset(),
            )?;
            return Ok(ChatState::PromptUser {
                skip_printing_tools: true,
            });
        }

        execute!(
            stderr,
            StyledText::secondary_fg(),
            style::Print("\nSwitching to the "),
            StyledText::reset(),
            style::Print("Lite UI".bold().to_string()),
            StyledText::secondary_fg(),
            style::Print("... (make it your default from "),
            StyledText::reset(),
            style::Print(&StyledText::command("/settings display")),
            StyledText::secondary_fg(),
            style::Print(" once it opens)\n\n"),
            StyledText::reset(),
        )?;

        session.relaunch_in_lite = true;
        Ok(ChatState::Exit)
    }
}

#[cfg(test)]
mod tests {
    use clap::Parser;

    use crate::cli::chat::cli::SlashCommand;

    #[test]
    fn parses_lite_command() {
        let cmd = SlashCommand::try_parse_from(["slash_command", "lite"]).unwrap();
        assert_eq!(cmd.command_name(), "lite");
    }
}
