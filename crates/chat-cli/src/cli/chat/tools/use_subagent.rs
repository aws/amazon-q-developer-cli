use std::collections::HashMap;
use std::io::Write;

use chat_cli_ui::conduit::get_legacy_conduits;
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
#[serde(tag = "command")]
pub enum UseSubagent {
    /// Query for agents available for task delegation
    ListAgents,
    /// Invoke a subagent with the specified agent to complete a task
    InvokeSubagent(InvokeSubagent),
}

impl UseSubagent {
    pub const INFO: ToolInfo = ToolInfo {
        spec_name: "use_subagent",
        preferred_alias: "subagent",
        aliases: &["use_subagent", "subagent"],
    };

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
            Self::InvokeSubagent(invoke_subagent) => {
                let InvokeSubagent { query, agent_name, .. } = invoke_subagent;
                let subagent: Subagent<'_> = self.try_into()?;
                let (view_end, _byte_receiver, mut control_end_stderr, _control_end_stdout) = get_legacy_conduits(true);
                let agent_name = agent_name.as_deref().unwrap_or("default agent");
                let initial_query = query.as_str();
                let subagent_indicator = SubagentIndicator::new(agent_name, initial_query, view_end);
                let _guard = subagent_indicator.run();

                let output = subagent.query(os, &mut control_end_stderr).await?;
                let output_serialized = serde_json::to_value(output)?;

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

impl<'a> TryFrom<&'a UseSubagent> for Subagent<'a> {
    type Error = eyre::Error;

    fn try_from(value: &'a UseSubagent) -> std::result::Result<Self, Self::Error> {
        if let UseSubagent::InvokeSubagent(InvokeSubagent {
            query,
            agent_name,
            relevant_context,
        }) = value
        {
            Ok(Subagent {
                query: query.as_str(),
                agent_name: agent_name.as_deref(),
                embedded_user_msg: relevant_context.as_deref(),
                dangerously_trust_all_tools: false,
            })
        } else {
            bail!("Incorrect subagent tool call supplied")
        }
    }
}
