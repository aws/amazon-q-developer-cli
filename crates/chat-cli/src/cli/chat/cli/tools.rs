use std::collections::{
    BTreeSet,
    HashSet,
};
use std::io::Write;

use clap::{
    Args,
    Subcommand,
};
use crossterm::style::{
    Attribute,
    Color,
};
use crossterm::{
    queue,
    style,
};

use crate::api_client::model::Tool as FigTool;
use crate::cli::agent::Agent;
use crate::cli::chat::consts::{
    AGENT_FORMAT_TOOLS_DOC_URL,
    DUMMY_TOOL_NAME,
};
use crate::cli::chat::tools::ToolOrigin;
use crate::cli::chat::{
    ChatError,
    ChatSession,
    ChatState,
    TRUST_ALL_TEXT,
};
use crate::util::consts::MCP_SERVER_TOOL_DELIMITER;

#[deny(missing_docs)]
#[derive(Debug, PartialEq, Args)]
pub struct ToolsArgs {
    #[command(subcommand)]
    subcommand: Option<ToolsSubcommand>,
}

impl ToolsArgs {
    pub async fn execute(self, session: &mut ChatSession) -> Result<ChatState, ChatError> {
        if let Some(subcommand) = self.subcommand {
            return subcommand.execute(session).await;
        }

        // No subcommand - print the current tools and their permissions.
        // Determine how to format the output nicely.
        let terminal_width = session.terminal_width();
        let longest = session
            .conversation
            .tool_manager
            .tn_map
            .values()
            .map(|info| info.host_tool_name.len())
            .max()
            .unwrap_or(0)
            .max(
                session
                    .conversation
                    .tools
                    .get("native")
                    .and_then(|tools| {
                        tools
                            .iter()
                            .map(|tool| {
                                let FigTool::ToolSpecification(t) = tool;
                                t.name.len()
                            })
                            .max()
                    })
                    .unwrap_or(0),
            );

        queue!(
            session.stderr,
            style::Print("\n"),
            style::SetAttribute(Attribute::Bold),
            style::Print({
                // Adding 2 because of "- " preceding every tool name
                let width = (longest + 2).saturating_sub("Tool".len()) + 4;
                format!("Tool{:>width$}Permission", "", width = width)
            }),
            style::SetAttribute(Attribute::Reset),
            style::Print("\n"),
            style::Print("▔".repeat(terminal_width)),
        )?;

        let mut origin_tools: Vec<_> = session.conversation.tools.iter().collect();

        // Built in tools always appear first.
        origin_tools.sort_by(|(origin_a, _), (origin_b, _)| match (origin_a, origin_b) {
            (ToolOrigin::Native, _) => std::cmp::Ordering::Less,
            (_, ToolOrigin::Native) => std::cmp::Ordering::Greater,
            (ToolOrigin::McpServer(name_a), ToolOrigin::McpServer(name_b)) => name_a.cmp(name_b),
        });

        for (origin, tools) in origin_tools.iter() {
            // Note that Tool is model facing and thus would have names recognized by model.
            // Here we need to convert them to their host / user facing counter part.
            let tn_map = &session.conversation.tool_manager.tn_map;
            let sorted_tools = tools
                .iter()
                .filter_map(|FigTool::ToolSpecification(spec)| {
                    if spec.name == DUMMY_TOOL_NAME {
                        return None;
                    }

                    tn_map
                        .get(&spec.name)
                        .map_or(Some(spec.name.as_str()), |info| Some(info.host_tool_name.as_str()))
                })
                .collect::<BTreeSet<_>>();

            let to_display = sorted_tools.iter().fold(String::new(), |mut acc, tool_name| {
                let width = longest - tool_name.len() + 4;
                acc.push_str(
                    format!(
                        "- {}{:>width$}{}\n",
                        tool_name,
                        "",
                        session.conversation.agents.display_label(tool_name, origin),
                        width = width
                    )
                    .as_str(),
                );
                acc
            });

            let _ = queue!(
                session.stderr,
                style::SetAttribute(Attribute::Bold),
                style::Print(format!("{}:\n", origin)),
                style::SetAttribute(Attribute::Reset),
                style::Print(to_display),
                style::Print("\n")
            );
        }

        let loading = session.conversation.tool_manager.pending_clients().await;
        if !loading.is_empty() {
            queue!(
                session.stderr,
                style::SetAttribute(Attribute::Bold),
                style::Print("Servers still loading"),
                style::SetAttribute(Attribute::Reset),
                style::Print("\n"),
                style::Print("▔".repeat(terminal_width)),
            )?;
            for client in loading {
                queue!(session.stderr, style::Print(format!(" - {client}")), style::Print("\n"))?;
            }
        }

        if origin_tools.is_empty() {
            queue!(
                session.stderr,
                style::Print(
                    "\nNo tools are currently enabled.\n\nRefer to the documentation for how to add tools to your agent: "
                ),
                style::SetForegroundColor(Color::Green),
                style::Print(AGENT_FORMAT_TOOLS_DOC_URL),
                style::SetForegroundColor(Color::Reset),
                style::Print("\n"),
                style::SetForegroundColor(Color::Reset),
            )?;
        }

        Ok(ChatState::default())
    }

    pub fn subcommand_name(&self) -> Option<&'static str> {
        self.subcommand.as_ref().map(|s| s.name())
    }
}

#[deny(missing_docs)]
#[derive(Debug, PartialEq, Subcommand)]
#[command(
    before_long_help = "By default, Amazon Q will ask for your permission to use certain tools. You can control which tools you
trust so that no confirmation is required.

Refer to the documentation for how to configure tools with your agent: https://github.com/aws/amazon-q-developer-cli/blob/main/docs/agent-format.md#tools-field"
)]
pub enum ToolsSubcommand {
    /// Show the input schema for all available tools
    Schema,
    /// Trust a specific tool or tools for the session
    Trust {
        #[arg(required = true)]
        tool_names: Vec<String>,
    },
    /// Revert a tool or tools to per-request confirmation
    Untrust {
        #[arg(required = true)]
        tool_names: Vec<String>,
    },
    /// Trust all tools (equivalent to deprecated /acceptall)
    TrustAll,
    /// Reset all tools to default permission levels
    Reset,
}

impl ToolsSubcommand {
    pub async fn execute(self, session: &mut ChatSession) -> Result<ChatState, ChatError> {
        // Here we need to obtain the list of host tool names
        let existing_custom_tools = session
            .conversation
            .tool_manager
            .tn_map
            .values()
            .cloned()
            .collect::<HashSet<_>>();

        // We also need to obtain a list of native tools since tn_map from ToolManager does not
        // contain native tools
        let native_tool_names = session
            .conversation
            .tools
            .get("native")
            .map(|tools| {
                tools
                    .iter()
                    .filter_map(|tool| match tool {
                        FigTool::ToolSpecification(t) if t.name != DUMMY_TOOL_NAME => Some(t.name.clone()),
                        FigTool::ToolSpecification(_) => None,
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();

        match self {
            Self::Schema => {
                let schema_json = serde_json::to_string_pretty(&session.conversation.tool_manager.schema)
                    .map_err(|e| ChatError::Custom(format!("Error converting tool schema to string: {e}").into()))?;
                queue!(session.stderr, style::Print(schema_json), style::Print("\n"))?;
            },
            Self::Trust { tool_names } => {
                let (valid_tools, invalid_tools): (Vec<String>, Vec<String>) =
                    tool_names.into_iter().partition(|tool_name| {
                        existing_custom_tools.contains(tool_name) || native_tool_names.contains(tool_name)
                    });

                if !invalid_tools.is_empty() {
                    queue!(
                        session.stderr,
                        style::SetForegroundColor(Color::Red),
                        style::Print(format!("\nCannot trust '{}', ", invalid_tools.join("', '"))),
                        if invalid_tools.len() > 1 {
                            style::Print("they do not exist.")
                        } else {
                            style::Print("it does not exist.")
                        },
                        style::SetForegroundColor(Color::Reset),
                    )?;
                }
                if !valid_tools.is_empty() {
                    let tools_to_trust = valid_tools
                        .into_iter()
                        .filter_map(|tool_name| {
                            if native_tool_names.contains(&tool_name) {
                                Some(tool_name)
                            } else {
                                existing_custom_tools
                                    .get(&tool_name)
                                    .map(|info| format!("@{}{MCP_SERVER_TOOL_DELIMITER}{tool_name}", info.server_name))
                            }
                        })
                        .collect::<Vec<_>>();

                    queue!(
                        session.stderr,
                        style::SetForegroundColor(Color::Green),
                        if tools_to_trust.len() > 1 {
                            style::Print(format!("\nTools '{}' are ", tools_to_trust.join("', '")))
                        } else {
                            style::Print(format!("\nTool '{}' is ", tools_to_trust[0]))
                        },
                        style::Print("now trusted. I will "),
                        style::SetAttribute(Attribute::Bold),
                        style::Print("not"),
                        style::SetAttribute(Attribute::Reset),
                        style::SetForegroundColor(Color::Green),
                        style::Print(format!(
                            " ask for confirmation before running {}.",
                            if tools_to_trust.len() > 1 {
                                "these tools"
                            } else {
                                "this tool"
                            }
                        )),
                        style::Print("\n"),
                        style::SetForegroundColor(Color::Reset),
                    )?;

                    session.conversation.agents.trust_tools(tools_to_trust);
                }
            },
            Self::Untrust { tool_names } => {
                let (valid_tools, invalid_tools): (Vec<String>, Vec<String>) =
                    tool_names.into_iter().partition(|tool_name| {
                        existing_custom_tools.contains(tool_name) || native_tool_names.contains(tool_name)
                    });

                if !invalid_tools.is_empty() {
                    queue!(
                        session.stderr,
                        style::SetForegroundColor(Color::Red),
                        style::Print(format!("\nCannot untrust '{}', ", invalid_tools.join("', '"))),
                        if invalid_tools.len() > 1 {
                            style::Print("they do not exist.")
                        } else {
                            style::Print("it does not exist.")
                        },
                        style::SetForegroundColor(Color::Reset),
                    )?;
                }
                if !valid_tools.is_empty() {
                    let tools_to_untrust = valid_tools
                        .into_iter()
                        .filter_map(|tool_name| {
                            if native_tool_names.contains(&tool_name) {
                                Some(tool_name)
                            } else {
                                existing_custom_tools
                                    .get(&tool_name)
                                    .map(|info| format!("@{}{MCP_SERVER_TOOL_DELIMITER}{tool_name}", info.server_name))
                            }
                        })
                        .collect::<Vec<_>>();

                    session.conversation.agents.untrust_tools(&tools_to_untrust);

                    queue!(
                        session.stderr,
                        style::SetForegroundColor(Color::Green),
                        if tools_to_untrust.len() > 1 {
                            style::Print(format!("\nTools '{}' are ", tools_to_untrust.join("', '")))
                        } else {
                            style::Print(format!("\nTool '{}' is ", tools_to_untrust[0]))
                        },
                        style::Print("set to per-request confirmation.\n"),
                        style::SetForegroundColor(Color::Reset),
                    )?;
                }
            },
            Self::TrustAll => {
                session.conversation.agents.trust_all_tools = true;
                queue!(session.stderr, style::Print(TRUST_ALL_TEXT))?;
            },
            Self::Reset => {
                session.conversation.agents.trust_all_tools = false;

                let active_agent_path = session.conversation.agents.get_active().and_then(|a| a.path.clone());
                if let Some(path) = active_agent_path {
                    let result = async {
                        let content = tokio::fs::read(&path).await?;
                        let orig_agent = serde_json::from_slice::<Agent>(&content)?;
                        // since all we're doing here is swapping the tool list, it's okay if we
                        // don't thaw it here
                        Ok::<Agent, Box<dyn std::error::Error>>(orig_agent)
                    }
                    .await;

                    if let (Ok(orig_agent), Some(active_agent)) = (result, session.conversation.agents.get_active_mut())
                    {
                        active_agent.allowed_tools = orig_agent.allowed_tools;
                    }
                } else if session
                    .conversation
                    .agents
                    .get_active()
                    .is_some_and(|a| a.name.as_str() == "default")
                {
                    // We only want to reset the tool permission and nothing else
                    if let Some(active_agent) = session.conversation.agents.get_active_mut() {
                        active_agent.allowed_tools = Default::default();
                        active_agent.tools_settings = Default::default();
                    }
                }
                queue!(
                    session.stderr,
                    style::SetForegroundColor(Color::Green),
                    style::Print("\nReset all tools to the permission levels as defined in agent."),
                    style::SetForegroundColor(Color::Reset),
                )?;
            },
        };

        session.stderr.flush()?;

        Ok(ChatState::PromptUser {
            skip_printing_tools: true,
        })
    }

    pub fn name(&self) -> &'static str {
        match self {
            ToolsSubcommand::Schema => "schema",
            ToolsSubcommand::Trust { .. } => "trust",
            ToolsSubcommand::Untrust { .. } => "untrust",
            ToolsSubcommand::TrustAll => "trust-all",
            ToolsSubcommand::Reset => "reset",
        }
    }
}
