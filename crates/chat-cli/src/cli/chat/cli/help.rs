use clap::{
    Args,
    CommandFactory,
};
use crossterm::{
    execute,
    style,
};

use super::SlashCommand;
use crate::cli::chat::{
    ChatError,
    ChatSession,
    ChatState,
};
use crate::constants::HELP_AGENT_NAME;
use crate::os::Os;

#[derive(Debug, PartialEq, Args)]
#[command(disable_help_flag = true)]
pub struct HelpArgs {
    /// Show classic help text instead of interactive help agent
    #[arg(long)]
    pub legacy: bool,

    /// Optional question to ask the help agent
    #[arg(allow_hyphen_values = true)]
    pub question: Vec<String>,
}

impl HelpArgs {
    pub async fn execute(self, _os: &Os, session: &mut ChatSession) -> Result<ChatState, ChatError> {
        // If --legacy flag is set, show classic help text
        if self.legacy {
            let mut cmd = SlashCommand::command();

            // If a subcommand is specified, show help for that subcommand
            if !self.question.is_empty() {
                for arg in &self.question {
                    if let Some(subcmd) = cmd.find_subcommand(arg) {
                        cmd = subcmd.clone();
                    } else {
                        break;
                    }
                }
            }

            let help_text = cmd.render_long_help().to_string();
            execute!(session.stderr, style::Print(&help_text), style::Print("\n"))?;

            return Ok(ChatState::PromptUser {
                skip_printing_tools: true,
            });
        }

        // Otherwise, use interactive help agent
        let swap_state = session.input_source.agent_swap_state();
        let current_agent = swap_state.get_current_agent();

        if current_agent == HELP_AGENT_NAME {
            if !self.question.is_empty() {
                let question_text = self.question.join(" ");
                session.conversation.append_user_transcript(&question_text);
                return Ok(ChatState::HandleInput { input: question_text });
            } else {
                // Toggle back to previous agent
                swap_state.toggle_to_previous_agent(None);
                return Ok(ChatState::PromptUser {
                    skip_printing_tools: false,
                });
            }
        }

        let prompt_option = if self.question.is_empty() {
            None
        } else {
            Some(self.question.join(" "))
        };
        swap_state.trigger_swap(HELP_AGENT_NAME, prompt_option);

        Ok(ChatState::PromptUser {
            skip_printing_tools: false,
        })
    }
}

#[cfg(test)]
mod tests {
    use clap::Parser;

    use super::*;

    #[test]
    fn test_help_accepts_dash_h() {
        // Test that -h is accepted as a question argument, not treated as help flag
        let result = SlashCommand::try_parse_from(["slash_command", "help", "-h"]);
        assert!(result.is_ok());
        if let SlashCommand::Help(args) = result.unwrap() {
            assert_eq!(args.question, vec!["-h"]);
        } else {
            panic!("Expected Help variant");
        }
    }

    #[test]
    fn test_help_accepts_multiple_args() {
        let result = SlashCommand::try_parse_from(["slash_command", "help", "how", "do", "I", "use", "-h"]);
        assert!(result.is_ok());
        if let SlashCommand::Help(args) = result.unwrap() {
            assert_eq!(args.question, vec!["how", "do", "I", "use", "-h"]);
        } else {
            panic!("Expected Help variant");
        }
    }

    #[test]
    fn test_other_commands_show_help() {
        // Test that other commands still show help with -h
        let result = SlashCommand::try_parse_from(["slash_command", "compact", "-h"]);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().kind(), clap::error::ErrorKind::DisplayHelp);
    }
}
