use std::collections::HashMap;
use std::io::Write;

use agent::tools::summary::Summary;
use chat_cli_ui::conduit::get_conduit;
use chat_cli_ui::subagent_indicator::SubagentIndicator;
use eyre::Result;
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

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct InvokeSubagent {
    /// The query or task to be handled by the subagent
    query: String,
    /// Optional name of the specific agent to use. If not provided, uses the default agent
    agent_name: Option<String>,
    /// Optional additional context that should be provided to the subagent to help it
    /// understand the task better
    relevant_context: Option<String>,
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

    /// Checks if todo lists are enabled
    pub fn is_enabled(os: &Os) -> bool {
        ExperimentManager::is_enabled(os, ExperimentName::UseSubagent)
    }

    pub fn validate(&self) -> Result<(), String> {
        if let UseSubagent::InvokeSubagents { subagents, .. } = self {
            if subagents.len() > 4 {
                return Err("You can only spawn 4 or fewer subagents at a time".to_string());
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
                let subagents = subagents
                    .iter()
                    .enumerate()
                    .map(|(id, invoke_subagent)| {
                        let mut subagent: Subagent<'_> = invoke_subagent.into();
                        subagent.id = id as u16;
                        subagent
                    })
                    .collect::<Vec<_>>();

                let subagent_indicator = SubagentIndicator::new(
                    &subagents
                        .iter()
                        .map(|subagent| (subagent.agent_name.unwrap_or(DEFAULT_AGENT_NAME), subagent.query))
                        .collect::<Vec<(&str, &str)>>(),
                    view_end,
                );
                let _guard = subagent_indicator.run();

                let parent_conv_id = convo_id.as_deref().unwrap_or_default();
                let (oks, bads) =
                    futures::future::join_all(subagents.into_iter().map(|subagent| {
                        subagent.query(os, input_rx.resubscribe(), control_end.clone(), parent_conv_id)
                    }))
                    .await
                    .into_iter()
                    .partition::<Vec<eyre::Result<Summary>>, _>(|res| res.is_ok());

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

impl<'a> From<&'a InvokeSubagent> for Subagent<'a> {
    fn from(value: &'a InvokeSubagent) -> Self {
        let InvokeSubagent {
            query,
            agent_name,
            relevant_context,
        } = value;

        Subagent {
            id: 0_u16,
            query: query.as_str(),
            agent_name: agent_name.as_deref(),
            embedded_user_msg: relevant_context.as_deref(),
            dangerously_trust_all_tools: false,
        }
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
