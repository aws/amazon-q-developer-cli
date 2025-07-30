use std::borrow::Cow;
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

use crate::cli::agent::{
    Agent,
    Agents,
    create_agent,
};
use crate::cli::chat::{
    ChatError,
    ChatSession,
    ChatState,
};
use crate::database::settings::Setting;
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
    /// List all available agents
    List,
    /// Create a new agent with the specified name
    Create {
        /// Name of the agent to be created
        #[arg(long, short)]
        name: String,
        /// The directory where the agent will be saved. If not provided, the agent will be saved in
        /// the global agent directory
        #[arg(long, short)]
        directory: Option<String>,
        /// The name of an agent that shall be used as the starting point for the agent creation
        #[arg(long, short)]
        from: Option<String>,
    },
    /// Delete the specified agent
    #[command(hide = true)]
    Delete { name: String },
    /// Switch to the specified agent
    #[command(hide = true)]
    Set { name: String },
    /// Show agent config schema
    Schema,
    /// Define a default agent to use when q chat launches
    SetDefault {
        #[arg(long, short)]
        name: String,
    },
}

impl AgentSubcommand {
    pub async fn execute(self, os: &mut Os, session: &mut ChatSession) -> Result<ChatState, ChatError> {
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

                for (i, profile) in profiles.iter().enumerate() {
                    if active_profile.is_some_and(|p| p == *profile) {
                        queue!(
                            session.stderr,
                            style::SetForegroundColor(Color::Green),
                            style::Print("* "),
                            style::Print(&profile.name),
                            style::SetForegroundColor(Color::Reset),
                        )?;
                    } else {
                        queue!(session.stderr, style::Print("  "), style::Print(&profile.name),)?;
                    }

                    if i < profiles.len().saturating_sub(1) {
                        queue!(session.stderr, style::Print("\n"))?;
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
            Self::Create { name, directory, from } => {
                let mut agents = Agents::load(os, None, true, &mut session.stderr).await.0;
                let path_with_file_name = create_agent(os, &mut agents, name.clone(), directory, from)
                    .await
                    .map_err(|e| ChatError::Custom(Cow::Owned(e.to_string())))?;
                let editor_cmd = std::env::var("EDITOR").unwrap_or_else(|_| "vi".to_string());
                let mut cmd = std::process::Command::new(editor_cmd);

                let status = cmd.arg(&path_with_file_name).status()?;
                if !status.success() {
                    return Err(ChatError::Custom("Editor process did not exit with success".into()));
                }

                let new_agent = Agent::load(os, &path_with_file_name, &mut None).await;
                match new_agent {
                    Ok(agent) => {
                        session.conversation.agents.agents.insert(agent.name.clone(), agent);
                    },
                    Err(e) => {
                        execute!(
                            session.stderr,
                            style::SetForegroundColor(Color::Red),
                            style::Print("Error: "),
                            style::ResetColor,
                            style::Print(&e),
                            style::Print("\n"),
                        )?;

                        return Err(ChatError::Custom(
                            format!("Post write validation failed for agent '{name}'. Malformed config detected: {e}")
                                .into(),
                        ));
                    },
                }

                execute!(
                    session.stderr,
                    style::SetForegroundColor(Color::Green),
                    style::Print("Agent "),
                    style::SetForegroundColor(Color::Cyan),
                    style::Print(name),
                    style::SetForegroundColor(Color::Green),
                    style::Print(" has been created successfully"),
                    style::SetForegroundColor(Color::Reset),
                    style::Print("\n"),
                    style::SetForegroundColor(Color::Yellow),
                    style::Print("Changes take effect on next launch"),
                    style::SetForegroundColor(Color::Reset)
                )?;
            },
            Self::Set { .. } | Self::Delete { .. } => {
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
                        "To make changes or create agents, please do so via create the corresponding config in {}, where you would also find an example config for your reference.\nTo switch agent, launch another instance of q chat with --agent.\n\n",
                        global_path
                    )),
                    style::SetAttribute(Attribute::Reset)
                )?;
            },
            Self::SetDefault { name } => match session.conversation.agents.agents.get(&name) {
                Some(agent) => {
                    os.database
                        .settings
                        .set(Setting::ChatDefaultAgent, agent.name.clone())
                        .await
                        .map_err(|e| ChatError::Custom(e.to_string().into()))?;

                    execute!(
                        session.stderr,
                        style::SetForegroundColor(Color::Green),
                        style::Print("✓ Default agent set to '"),
                        style::Print(&agent.name),
                        style::Print("'. This will take effect the next time q chat is launched.\n"),
                        style::ResetColor,
                    )?;
                },
                None => {
                    execute!(
                        session.stderr,
                        style::SetForegroundColor(Color::Red),
                        style::Print("Error: "),
                        style::ResetColor,
                        style::Print(format!("No agent with name {name} found\n")),
                    )?;
                },
            },
        }

        Ok(ChatState::PromptUser {
            skip_printing_tools: true,
        })
    }

    pub fn name(&self) -> &'static str {
        match self {
            Self::List => "list",
            Self::Create { .. } => "create",
            Self::Delete { .. } => "delete",
            Self::Set { .. } => "set",
            Self::Schema => "schema",
            Self::SetDefault { .. } => "set_default",
        }
    }
}

fn highlight_json(output: &mut impl Write, json_str: &str) -> eyre::Result<()> {
    let ps = SyntaxSet::load_defaults_newlines();
    let ts = ThemeSet::load_defaults();

    let syntax = ps
        .find_syntax_by_extension("json")
        .ok_or(eyre::eyre!("No syntax found by extension"))?;
    let mut h = HighlightLines::new(syntax, &ts.themes["base16-ocean.dark"]);

    for line in LinesWithEndings::from(json_str) {
        let ranges: Vec<(Style, &str)> = h.highlight_line(line, &ps)?;
        let escaped = as_24_bit_terminal_escaped(&ranges[..], false);
        queue!(output, style::Print(escaped))?;
    }

    Ok(execute!(output, style::ResetColor)?)
}
