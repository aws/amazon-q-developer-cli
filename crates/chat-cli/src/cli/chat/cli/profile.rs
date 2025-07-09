use std::io::Write;

use clap::Subcommand;
use crossterm::style::{
    self,
    Attribute,
    Color,
};
use crossterm::{
    execute,
    queue,
};
use syntect::easy::HighlightLines;
use syntect::highlighting::{
    Style,
    ThemeSet,
};
use syntect::parsing::SyntaxSet;
use syntect::util::{
    LinesWithEndings,
    as_24_bit_terminal_escaped,
};

use crate::cli::agent::Agent;
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
    /// Show agent config schema
    Schema,
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
            Self::Schema => {
                use schemars::schema_for;

                let schema = schema_for!(Agent);
                let pretty = serde_json::to_string_pretty(&schema)
                    .map_err(|e| ChatError::Custom(format!("Failed to convert agent schema to string: {e}").into()))?;
                highlight_json(&mut session.stderr, pretty.as_str())
                    .map_err(|e| ChatError::Custom(format!("Error printing agent schema: {e}").into()))?;
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

fn highlight_json(output: &mut impl Write, json_str: &str) -> eyre::Result<()> {
    let ps = SyntaxSet::load_defaults_newlines();
    let ts = ThemeSet::load_defaults();

    let syntax = ps.find_syntax_by_extension("json").unwrap();
    let mut h = HighlightLines::new(syntax, &ts.themes["base16-ocean.dark"]);

    for line in LinesWithEndings::from(json_str) {
        let ranges: Vec<(Style, &str)> = h.highlight_line(line, &ps)?;
        let escaped = as_24_bit_terminal_escaped(&ranges[..], false);
        queue!(output, style::Print(escaped))?;
    }

    Ok(execute!(output, style::ResetColor)?)
}
