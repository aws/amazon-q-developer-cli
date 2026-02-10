use std::collections::{
    BTreeSet,
    HashMap,
    HashSet,
};
use std::io::Write;

use clap::{
    Args,
    Subcommand,
};
use crossterm::style::Attribute;
use crossterm::{
    queue,
    style,
};

use crate::api_client::model::Tool as FigTool;
use crate::cli::agent::Agent;
use crate::cli::chat::consts::DUMMY_TOOL_NAME;
use crate::cli::chat::token_counter::TokenCounter;
use crate::cli::chat::tools::{
    ToolMetadata,
    ToolOrigin,
};
use crate::cli::chat::{
    ChatError,
    ChatSession,
    ChatState,
    trust_all_text,
};
use crate::constants::help_text::tools_long_help;
use crate::constants::{
    AGENT_FORMAT_TOOLS_DOC_URL,
    DEFAULT_AGENT_NAME,
};
use crate::theme::StyledText;
use crate::util::consts::MCP_SERVER_TOOL_DELIMITER;

/// Formats a token count into a compact human-readable string.
fn format_tokens(tokens: usize) -> String {
    if tokens >= 1000 {
        format!("{:.1}k", tokens as f64 / 1000.0)
    } else {
        tokens.to_string()
    }
}

/// Estimates the token count for a tool specification by serializing it to JSON
/// and using the standard character-based heuristic.
fn estimate_tool_tokens(spec: &crate::cli::chat::tools::ToolSpec) -> usize {
    let json = serde_json::to_string(spec).unwrap_or_default();
    TokenCounter::count_tokens(&json)
}

/// Command-line arguments for managing tools in the chat session
#[deny(missing_docs)]
#[derive(Debug, PartialEq, Args)]
pub struct ToolsArgs {
    #[command(subcommand)]
    subcommand: Option<ToolsSubcommand>,
}

impl ToolsArgs {
    pub async fn execute(self, session: &mut ChatSession, os: &mut crate::os::Os) -> Result<ChatState, ChatError> {
        if let Some(subcommand) = self.subcommand {
            return subcommand.execute(session).await;
        }

        // Ensure MCP data is fresh before displaying tools
        session.ensure_fresh_mcp_data(os).await.ok();

        // Update conversation state to refresh tools after potential server changes
        session.conversation.update_state(false).await;

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
                    .get(&ToolOrigin::Native)
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

        // Pre-compute token estimate strings for all tools
        let schema = &session.conversation.tool_manager.schema;
        let token_strings: HashMap<&str, String> = schema
            .iter()
            .map(|(name, spec)| (name.as_str(), format_tokens(estimate_tool_tokens(spec))))
            .collect();

        let token_col_header = "~Tokens";
        let token_col_width = token_strings
            .values()
            .map(|s| s.len())
            .max()
            .unwrap_or(0)
            .max(token_col_header.len());

        queue!(
            session.stderr,
            style::Print("\n"),
            style::SetAttribute(Attribute::Bold),
            style::Print({
                let pad1 = (longest + 2).saturating_sub("Tool".len()) + 4;
                format!(
                    "Tool{:pad1$}{:>tw$}    Permission",
                    "",
                    token_col_header,
                    pad1 = pad1,
                    tw = token_col_width,
                )
            }),
            StyledText::reset_attributes(),
            style::Print("\n"),
            StyledText::secondary_fg(),
            style::Print("▔".repeat(terminal_width)),
            StyledText::reset(),
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

            let mut origin_total_tokens: usize = 0;
            let to_display = sorted_tools.iter().fold(String::new(), |mut acc, tool_name| {
                // Get preferred alias for native tools, or use original name for MCP tools
                let display_name =
                    ToolMetadata::get_by_spec_name(tool_name).map_or(*tool_name, |info| info.preferred_alias);

                let pad1 = longest.saturating_sub(display_name.len()).saturating_add(4);

                // Look up token estimate - try the model tool name (via tn_map reverse lookup)
                let (token_str, token_count) = session
                    .conversation
                    .tool_manager
                    .tn_map
                    .iter()
                    .find(|(_, info)| info.host_tool_name.as_str() == *tool_name)
                    .and_then(|(model_name, _)| {
                        let spec = schema.get(model_name)?;
                        let token_str = token_strings.get(model_name.as_str())?;
                        Some((token_str.as_str(), estimate_tool_tokens(spec)))
                    })
                    .or_else(|| {
                        token_strings
                            .get(*tool_name)
                            .map(|s| (s.as_str(), schema.get(*tool_name).map_or(0, estimate_tool_tokens)))
                    })
                    .unwrap_or(("-", 0));

                origin_total_tokens += token_count;

                acc.push_str(
                    format!(
                        "- {}{:pad1$}{:>tw$}    {}\n",
                        display_name,
                        "",
                        token_str,
                        session.conversation.agents.display_label(tool_name, origin),
                        pad1 = pad1,
                        tw = token_col_width,
                    )
                    .as_str(),
                );
                acc
            });

            // Format the total for this origin
            let total_str = format_tokens(origin_total_tokens);
            let total_pad = longest.saturating_sub("Total".len()).saturating_add(4);

            let _ = queue!(
                session.stderr,
                style::SetAttribute(Attribute::Bold),
                style::Print(format!("{origin}\n")),
                StyledText::reset_attributes(),
                style::Print(to_display),
                StyledText::secondary_fg(),
                style::Print(format!(
                    "  Total{:total_pad$}{:>tw$}\n",
                    "",
                    total_str,
                    total_pad = total_pad,
                    tw = token_col_width,
                )),
                StyledText::reset(),
                style::Print("\n")
            );
        }

        let loading = session.conversation.tool_manager.pending_clients().await;
        if !loading.is_empty() {
            queue!(
                session.stderr,
                style::SetAttribute(Attribute::Bold),
                style::Print("Servers loading (Some of these might need auth. See "),
                StyledText::success_fg(),
                style::Print("/mcp"),
                StyledText::reset(),
                style::Print(" for details)"),
                StyledText::reset_attributes(),
                style::Print("\n"),
                StyledText::secondary_fg(),
                style::Print("▔".repeat(terminal_width)),
                StyledText::reset(),
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
                StyledText::brand_fg(),
                style::Print(AGENT_FORMAT_TOOLS_DOC_URL),
                StyledText::reset(),
                style::Print("\n"),
                StyledText::reset(),
            )?;
        }

        if !session.conversation.mcp_enabled {
            let message = if session.conversation.mcp_disabled_due_to_api_failure {
                "Failed to retrieve MCP settings; MCP functionality disabled\n\n"
            } else {
                "MCP functionality has been disabled by your administrator.\n\n"
            };

            queue!(
                session.stderr,
                StyledText::warning_fg(),
                style::Print("\n"),
                style::Print("⚠️  WARNING: "),
                StyledText::reset(),
                style::Print(message),
            )?;
        }

        if !session.conversation.tool_manager.web_tools_enabled {
            let message = if session.conversation.tool_manager.web_tools_disabled_due_to_api_failure {
                "Failed to retrieve web tools settings; web tools disabled\n\n"
            } else {
                "Web tools have been disabled by your administrator.\n\n"
            };

            queue!(
                session.stderr,
                StyledText::warning_fg(),
                style::Print("\n"),
                style::Print("⚠️  WARNING: "),
                StyledText::reset(),
                style::Print(message),
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
    before_long_help = tools_long_help()
)]
/// Subcommands for managing tool permissions and configurations
pub enum ToolsSubcommand {
    /// Show the input schema for all available tools
    Schema,
    /// Trust a specific tool or tools for the session
    Trust {
        #[arg(required = true)]
        /// Names of tools to trust
        tool_names: Vec<String>,
    },
    /// Revert a tool or tools to per-request confirmation
    Untrust {
        #[arg(required = true)]
        /// Names of tools to untrust
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
            .get(&ToolOrigin::Native)
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
                        existing_custom_tools.contains(tool_name)
                            || native_tool_names.contains(tool_name)
                            || ToolMetadata::get_by_any_alias(tool_name).is_some()
                    });

                if !invalid_tools.is_empty() {
                    queue!(
                        session.stderr,
                        StyledText::error_fg(),
                        style::Print(format!("\nCannot trust '{}', ", invalid_tools.join("', '"))),
                        if invalid_tools.len() > 1 {
                            style::Print("they do not exist.")
                        } else {
                            style::Print("it does not exist.")
                        },
                        StyledText::reset(),
                    )?;
                }
                if !valid_tools.is_empty() {
                    let tools_to_trust = valid_tools
                        .into_iter()
                        .filter_map(|tool_name| {
                            // Check if it's a native tool by any alias
                            if ToolMetadata::get_by_any_alias(&tool_name).is_some() {
                                Some(tool_name)
                            } else {
                                existing_custom_tools
                                    .get(&tool_name)
                                    .map(|info| format!("@{}{MCP_SERVER_TOOL_DELIMITER}{tool_name}", info.server_name))
                            }
                        })
                        .collect::<Vec<_>>();

                    if !tools_to_trust.is_empty() {
                        queue!(
                            session.stderr,
                            StyledText::success_fg(),
                            if tools_to_trust.len() > 1 {
                                style::Print(format!("\nTools '{}' are ", tools_to_trust.join("', '")))
                            } else {
                                style::Print(format!("\nTool '{}' is ", tools_to_trust[0]))
                            },
                            style::Print("now trusted. I will "),
                            style::SetAttribute(Attribute::Bold),
                            style::Print("not"),
                            StyledText::reset_attributes(),
                            StyledText::success_fg(),
                            style::Print(format!(
                                " ask for confirmation before running {}.",
                                if tools_to_trust.len() > 1 {
                                    "these tools"
                                } else {
                                    "this tool"
                                }
                            )),
                            style::Print("\n"),
                            StyledText::reset(),
                        )?;

                        session.conversation.agents.trust_tools(tools_to_trust);
                    }
                }
            },
            Self::Untrust { tool_names } => {
                let (valid_tools, invalid_tools): (Vec<String>, Vec<String>) =
                    tool_names.into_iter().partition(|tool_name| {
                        existing_custom_tools.contains(tool_name)
                            || native_tool_names.contains(tool_name)
                            || ToolMetadata::get_by_any_alias(tool_name).is_some()
                    });

                if !invalid_tools.is_empty() {
                    queue!(
                        session.stderr,
                        StyledText::error_fg(),
                        style::Print(format!("\nCannot untrust '{}', ", invalid_tools.join("', '"))),
                        if invalid_tools.len() > 1 {
                            style::Print("they do not exist.")
                        } else {
                            style::Print("it does not exist.")
                        },
                        StyledText::reset(),
                    )?;
                }
                if !valid_tools.is_empty() {
                    let tools_to_untrust: Vec<_> = valid_tools
                        .into_iter()
                        .filter_map(|tool_name| {
                            // Check if it's a native tool by any alias
                            if ToolMetadata::get_by_any_alias(&tool_name).is_some() {
                                Some(tool_name)
                            } else {
                                existing_custom_tools
                                    .get(&tool_name)
                                    .map(|info| format!("@{}{MCP_SERVER_TOOL_DELIMITER}{tool_name}", info.server_name))
                            }
                        })
                        .collect();

                    if !tools_to_untrust.is_empty() {
                        // If trust_all_tools is enabled, disable it since user is now managing individual tools
                        if session.conversation.agents.trust_all_tools {
                            session.conversation.agents.trust_all_tools = false;
                        }
                        session.conversation.agents.untrust_tools(&tools_to_untrust);

                        queue!(
                            session.stderr,
                            StyledText::success_fg(),
                            if tools_to_untrust.len() > 1 {
                                style::Print(format!("\nTools '{}' are ", tools_to_untrust.join("', '")))
                            } else {
                                style::Print(format!("\nTool '{}' is ", tools_to_untrust[0]))
                            },
                            style::Print("set to per-request confirmation.\n"),
                            StyledText::reset(),
                        )?;
                    }
                }
            },
            Self::TrustAll => {
                session.conversation.agents.trust_all_tools = true;
                if let Some(agent) = session.conversation.agents.get_active_mut() {
                    agent.add_tools_to_allowed(&session.conversation.tool_manager.schema);
                }
                queue!(session.stderr, style::Print(trust_all_text()))?;
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
                    .is_some_and(|a| a.name.as_str() == DEFAULT_AGENT_NAME)
                {
                    // We only want to reset the tool permission and nothing else
                    if let Some(active_agent) = session.conversation.agents.get_active_mut() {
                        active_agent.allowed_tools = Default::default();
                        active_agent.tools_settings = Default::default();
                    }
                }
                queue!(
                    session.stderr,
                    StyledText::success_fg(),
                    style::Print("\nReset all tools to the permission levels as defined in agent."),
                    StyledText::reset(),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_tokens_small() {
        assert_eq!(format_tokens(0), "0");
        assert_eq!(format_tokens(50), "50");
        assert_eq!(format_tokens(999), "999");
    }

    #[test]
    fn test_format_tokens_thousands() {
        assert_eq!(format_tokens(1000), "1.0k");
        assert_eq!(format_tokens(1200), "1.2k");
        assert_eq!(format_tokens(2500), "2.5k");
        assert_eq!(format_tokens(10000), "10.0k");
        assert_eq!(format_tokens(15700), "15.7k");
    }
}
