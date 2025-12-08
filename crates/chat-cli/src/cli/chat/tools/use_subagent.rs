use std::collections::HashMap;
use std::io::Write;
use std::path::PathBuf;

use agent::tools::summary::Summary;
use chat_cli_ui::conduit::get_conduit;
use chat_cli_ui::subagent_indicator::SubagentIndicator;
use eyre::{
    Result,
    bail,
};
use schemars::JsonSchema;
use serde::Deserialize;

use super::{
    InvokeOutput,
    Tool,
    ToolInfo,
};
use crate::agent::Subagent;
use crate::cli::agent::Agents;
use crate::cli::experiment::experiment_manager::{
    ExperimentManager,
    ExperimentName,
};
use crate::constants::DEFAULT_AGENT_NAME;
use crate::os::Os;
use crate::util::paths::PathResolver;

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct InvokeSubagent {
    /// The query or task to be handled by the subagent
    query: String,
    /// Optional name of the specific agent to use. If not provided, uses the default agent
    agent_name: Option<String>,
    /// Optional additional context that should be provided to the subagent to help it
    /// understand the task better
    relevant_context: Option<String>,
    /// Whether to trust all tools without prompting for confirmation.
    /// When set to true, the subagent will execute all tool calls without user approval.
    /// Use with caution as this may execute potentially dangerous operations.
    #[serde(default)]
    pub dangerously_trust_all_tools: bool,
    /// Whether the subagent should run in interactive mode.
    /// When set to true, the subagent will prompt for user input when needed.
    #[serde(default)]
    pub is_interactive: bool,
}

impl InvokeSubagent {
    fn as_subagent<'a>(
        &'a self,
        id: u16,
        local_agent_path: &'a PathBuf,
        global_agent_path: &'a PathBuf,
        local_mcp_path: &'a PathBuf,
        global_mcp_path: &'a PathBuf,
    ) -> Subagent<'a> {
        let InvokeSubagent {
            query,
            agent_name,
            relevant_context,
            dangerously_trust_all_tools,
            is_interactive,
        } = self;

        Subagent {
            id,
            query: query.as_str(),
            agent_name: agent_name.as_deref(),
            task_context: relevant_context.as_deref(),
            dangerously_trust_all_tools: *dangerously_trust_all_tools,
            is_interactive: *is_interactive,
            local_agent_path,
            global_agent_path,
            local_mcp_path,
            global_mcp_path,
        }
    }
}

/// A tool that allows the LLM to delegate tasks to a specialized subagent.
///
/// This enables the main agent to spawn a focused subagent with its own context
/// and capabilities to handle specific queries or tasks.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(tag = "command", content = "content")]
pub enum UseSubagent {
    /// Query for agents available for task delegation
    ListAgents,
    /// Invoke a subagent with the specified agent to complete a task
    InvokeSubagents {
        subagents: Vec<InvokeSubagent>,
        convo_id: Option<String>,
    },
}

impl UseSubagent {
    pub const INFO: ToolInfo = ToolInfo {
        spec_name: "use_subagent",
        preferred_alias: "subagent",
        aliases: &["use_subagent", "subagent"],
    };

    pub fn is_enabled(os: &Os) -> bool {
        ExperimentManager::is_enabled(os, ExperimentName::UseSubagent)
    }

    pub fn validate(&self) -> Result<()> {
        if let UseSubagent::InvokeSubagents { subagents, .. } = self {
            if subagents.len() > 4 {
                bail!("You can only spawn 4 or fewer subagents at a time");
            }
        }

        Ok(())
    }

    pub async fn invoke(&self, os: &Os, agents: &Agents) -> Result<InvokeOutput> {
        match self {
            Self::ListAgents => {
                let descriptions =
                    agents
                        .agents
                        .iter()
                        .fold(HashMap::<String, String>::new(), |mut acc, (name, agent)| {
                            acc.insert(
                                name.clone(),
                                agent
                                    .description
                                    .as_deref()
                                    .unwrap_or("No description provided. Derive meaning from agent name")
                                    .to_string(),
                            );
                            acc
                        });

                let serialized_output = serde_json::to_value(descriptions)?;

                Ok(InvokeOutput {
                    output: super::OutputKind::Json(serialized_output),
                })
            },
            Self::InvokeSubagents { subagents, convo_id } => {
                let (view_end, input_rx, control_end) = get_conduit();
                let resolver = PathResolver::new(os);
                let local_agent_path = resolver.workspace().agents_dir()?;
                let global_agent_path = resolver.global().agents_dir()?;
                let local_mcp_path = resolver.workspace().mcp_config()?;
                let global_mcp_path = resolver.global().mcp_config()?;
                let is_interactive = subagents.iter().any(|agent| agent.is_interactive);
                let subagents = subagents
                    .iter()
                    .enumerate()
                    .map(|(id, invoke_subagent)| {
                        invoke_subagent.as_subagent(
                            id as u16,
                            &local_agent_path,
                            &global_agent_path,
                            &local_mcp_path,
                            &global_mcp_path,
                        )
                    })
                    .collect::<Vec<_>>();

                let subagent_indicator = SubagentIndicator::new(
                    &subagents
                        .iter()
                        .map(|subagent| (subagent.agent_name.unwrap_or(DEFAULT_AGENT_NAME), subagent.query))
                        .collect::<Vec<(&str, &str)>>(),
                    view_end,
                    is_interactive,
                );
                let mut indicator_handle = subagent_indicator.run();

                let parent_conv_id = convo_id.as_deref().unwrap_or_default();
                let (oks, bads) =
                    futures::future::join_all(subagents.into_iter().map(|subagent| {
                        subagent.query(os, input_rx.resubscribe(), control_end.clone(), parent_conv_id)
                    }))
                    .await
                    .into_iter()
                    .partition::<Vec<eyre::Result<Summary>>, _>(|res| res.is_ok());

                _ = indicator_handle.wait_for_clean_screen().await;

                let oks = oks.into_iter().map(|res| res.unwrap()).collect::<Vec<_>>();
                let bads = bads
                    .into_iter()
                    .map(|res| res.err().unwrap().to_string())
                    .collect::<Vec<_>>();
                let oks = serde_json::to_value(oks)?;
                let bads = serde_json::to_value(bads)?;

                let output_serialized = serde_json::json!({
                    "successes": oks,
                    "failures": bads,
                });

                Ok(InvokeOutput {
                    output: super::OutputKind::Json(output_serialized),
                })
            },
        }
    }

    pub fn queue_description(&self, tool: &Tool, output: &mut impl Write) -> Result<()> {
        _ = self;
        super::display_tool_use(tool, output)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_deser() {
        let input = serde_json::json!({
            "command": "InvokeSubagents",
            "content": {
                "subagents": [{
                    "query": "test query",
                    "agent_name": "test_agent",
                    "relevant_context": "test context"
                }],
                "convo_id": "test_convo_id"
            }
        });

        let result: Result<UseSubagent, _> = serde_json::from_value(input);
        assert!(result.is_ok());

        let input = serde_json::json!({
            "command": "ListAgents",
        });

        let result: Result<UseSubagent, _> = serde_json::from_value(input);
        assert!(result.is_ok());
    }
}
