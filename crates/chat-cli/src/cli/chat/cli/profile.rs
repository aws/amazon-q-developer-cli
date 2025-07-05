use clap::Subcommand;
use crossterm::execute;
use crossterm::style::{
    self,
    Attribute,
    Color,
};

use crate::cli::chat::{
    ChatError,
    ChatSession,
    ChatState,
};
use crate::os::Os;
use crate::util::directories::chat_global_agent_path;

#[deny(missing_docs)]
#[derive(Debug, PartialEq, Subcommand)]
#[command(
    before_long_help = "Agents allow you to organize and manage different sets of context files for different projects or tasks.

Notes
• Launch q chat with a specific agent with --agent
• Construct an agent under ~/.aws/amazonq/agents/ (accessible globally) or cwd/.aws/amazonq/agents (accessible in workspace)
• See example config under global directory
• Set default agent to assume with settings by running \"q settings chat.defaultAgent agent_name\"
• Each agent maintains its own set of context and customizations"
)]
pub enum AgentSubcommand {
    /// List all available profiles
    List,
    /// Create a new profile with the specified name
    Create { name: String },
    /// Delete the specified profile
    Delete { name: String },
    /// Switch to the specified profile
    Set { name: String },
    /// Rename a profile
    Rename { old_name: String, new_name: String },
}

impl AgentSubcommand {
    pub async fn execute(self, os: &Os, session: &mut ChatSession) -> Result<ChatState, ChatError> {
        let agents = &session.conversation.agents;

        macro_rules! _print_err {
            ($err:expr) => {
                execute!(
                    session.stderr,
                    style::SetForegroundColor(Color::Red),
                    style::Print(format!("\nError: {}\n\n", $err)),
                    style::SetForegroundColor(Color::Reset)
                )?
            };
        }

        match self {
            Self::List => {
                let profiles = agents.agents.values().collect::<Vec<_>>();
                let active_profile = agents.get_active();

                execute!(session.stderr, style::Print("\n"))?;
                for profile in profiles {
                    if active_profile.is_some_and(|p| p == profile) {
                        execute!(
                            session.stderr,
                            style::SetForegroundColor(Color::Green),
                            style::Print("* "),
                            style::Print(&profile.name),
                            style::SetForegroundColor(Color::Reset),
                            style::Print("\n")
                        )?;
                    } else {
                        execute!(
                            session.stderr,
                            style::Print("  "),
                            style::Print(&profile.name),
                            style::Print("\n")
                        )?;
                    }
                }
                execute!(session.stderr, style::Print("\n"))?;
            },
            Self::Rename { .. } | Self::Set { .. } | Self::Delete { .. } | Self::Create { .. } => {
                // As part of the agent implementation, we are disabling the ability to
                // switch / create profile after a session has started.
                // TODO: perhaps revive this after we have a decision on profile create /
                // switch
                let global_path = if let Ok(path) = chat_global_agent_path(os) {
                    path.to_str().unwrap_or("default global agent path").to_string()
                } else {
                    "default global agent path".to_string()
                };
                execute!(
                    session.stderr,
                    style::SetForegroundColor(Color::Yellow),
                    style::Print(format!(
                        "Agent / Profile persistence has been disabled. To persist any changes on agent / profile, use the default agent under {} as example",
                        global_path
                    )),
                    style::SetAttribute(Attribute::Reset)
                )?;
            },
        }

        Ok(ChatState::PromptUser {
            skip_printing_tools: true,
        })
    }
}
