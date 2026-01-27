use std::collections::HashMap;
use std::io::Write;
use std::path::PathBuf;

use chat_cli_ui::conduit::get_conduit;
use chat_cli_ui::subagent_indicator::SubagentIndicator;
use eyre::{
    Result,
    bail,
};
use schemars::JsonSchema;
use serde::Deserialize;
use tracing::error;

use super::{
    InvokeOutput,
    Tool,
    ToolInfo,
};
use crate::agent::Subagent;
use crate::cli::Agent;
use crate::cli::agent::{
    Agents,
    PermissionEvalResult,
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
        parent_tool_use_id: &'a str,
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
            parent_tool_use_id,
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
        tool_use_id: Option<String>,
    },
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Settings {
    #[serde(default, alias = "trustedAgents")]
    trusted_agents: Vec<AgentIdentifier>,
    #[serde(default)]
    available_agents: Vec<AgentIdentifier>,
}

#[derive(Debug)]
enum AgentIdentifier {
    ExactName(String),
    NameGlob(regex::Regex, String),
}

impl PartialEq for AgentIdentifier {
    fn eq(&self, other: &AgentIdentifier) -> bool {
        match (self, other) {
            (AgentIdentifier::NameGlob(_, self_pattern), AgentIdentifier::NameGlob(_, other_pattern)) => {
                self_pattern == other_pattern
            },
            (AgentIdentifier::ExactName(self_name), AgentIdentifier::ExactName(other_name)) => self_name == other_name,
            (_, _) => false,
        }
    }
}

impl PartialEq<str> for AgentIdentifier {
    fn eq(&self, other: &str) -> bool {
        match self {
            AgentIdentifier::NameGlob(r, _) => r.is_match(other),
            AgentIdentifier::ExactName(name) => name == other,
        }
    }
}

impl<'de> Deserialize<'de> for AgentIdentifier {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        if s.contains("*") {
            let r = regex::Regex::new(s.as_str()).map_err(serde::de::Error::custom)?;
            Ok(AgentIdentifier::NameGlob(r, s))
        } else {
            Ok(AgentIdentifier::ExactName(s))
        }
    }
}

impl UseSubagent {
    pub const INFO: ToolInfo = ToolInfo {
        spec_name: "use_subagent",
        preferred_alias: "subagent",
        aliases: &["use_subagent", "subagent"],
    };

    pub fn validate(&self) -> Result<()> {
        if let UseSubagent::InvokeSubagents { subagents, .. } = self
            && subagents.len() > 4
        {
            bail!("You can only spawn 4 or fewer subagents at a time");
        }

        Ok(())
    }

    pub fn eval_perm(&self, _os: &Os, agent: &Agent) -> PermissionEvalResult {
        use crate::util::tool_permission_checker::is_tool_in_allowlist;

        let is_in_allowlist = Self::INFO
            .aliases
            .iter()
            .any(|alias| is_tool_in_allowlist(&agent.allowed_tools, alias, None));
        let tool_setting = Self::INFO
            .aliases
            .iter()
            .find_map(|alias| agent.tools_settings.get(*alias));

        match self {
            UseSubagent::ListAgents => {
                if is_in_allowlist || tool_setting.is_some() {
                    PermissionEvalResult::Allow
                } else {
                    PermissionEvalResult::Ask
                }
            },
            UseSubagent::InvokeSubagents { subagents, .. } => match tool_setting.cloned() {
                Some(settings) => {
                    let Settings {
                        trusted_agents,
                        available_agents,
                    } = match serde_json::from_value::<Settings>(settings) {
                        Ok(settings) => settings,
                        Err(e) => {
                            error!("Failed to deserialize tool settings for subagent: {:?}", e);
                            return PermissionEvalResult::Ask;
                        },
                    };

                    let agents_to_spawn = subagents
                        .iter()
                        .map(|invoke| invoke.agent_name.as_deref().unwrap_or(DEFAULT_AGENT_NAME))
                        .collect::<Vec<_>>();

                    // First check: availableAgents (if configured)
                    if !available_agents.is_empty() {
                        let denied_agents: Vec<String> = agents_to_spawn
                            .iter()
                            .filter(|agent| {
                                !available_agents
                                    .iter()
                                    .any(|available_agent| available_agent == **agent)
                            })
                            .map(|agent| format!("Agent '{}' is not available to be used as SubAgent", agent))
                            .collect();

                        if !denied_agents.is_empty() {
                            return PermissionEvalResult::Deny(denied_agents);
                        }
                    }

                    // Second check: trustedAgents
                    if agents_to_spawn
                        .iter()
                        .all(|agent| trusted_agents.iter().any(|trusted_agent| trusted_agent == *agent))
                    {
                        PermissionEvalResult::Allow
                    } else {
                        PermissionEvalResult::Ask
                    }
                },
                None => {
                    if is_in_allowlist {
                        PermissionEvalResult::Allow
                    } else {
                        PermissionEvalResult::Ask
                    }
                },
            },
        }
    }

    fn filter_agents(agents: &HashMap<String, Agent>, available_agents: &[AgentIdentifier]) -> HashMap<String, String> {
        agents
            .iter()
            .filter(|(name, _)| {
                available_agents.is_empty() || available_agents.iter().any(|pattern| pattern == name.as_str())
            })
            .map(|(name, agent)| {
                (
                    name.clone(),
                    agent
                        .description
                        .as_deref()
                        .unwrap_or("No description provided. Derive meaning from agent name")
                        .to_string(),
                )
            })
            .collect()
    }

    pub async fn invoke(&self, os: &Os, agents: &Agents) -> Result<InvokeOutput> {
        match self {
            Self::ListAgents => {
                // Get available_agents setting from active agent
                let active_agent = agents.get_active();
                let tool_setting = active_agent.and_then(|agent| {
                    Self::INFO
                        .aliases
                        .iter()
                        .find_map(|alias| agent.tools_settings.get(*alias))
                });

                let available_agents = match tool_setting {
                    Some(settings) => match serde_json::from_value::<Settings>(settings.clone()) {
                        Ok(Settings { available_agents, .. }) => available_agents,
                        Err(e) => {
                            error!("Failed to deserialize tool settings for subagent: {:?}", e);
                            vec![]
                        },
                    },
                    None => vec![],
                };

                let descriptions = Self::filter_agents(&agents.agents, &available_agents);

                let serialized_output = serde_json::to_value(descriptions)?;

                Ok(InvokeOutput {
                    output: super::OutputKind::Json(serialized_output),
                })
            },
            Self::InvokeSubagents {
                subagents,
                convo_id,
                tool_use_id,
            } => {
                let (view_end, input_rx, control_end) = get_conduit();
                let resolver = os.path_resolver();
                let local_agent_path = resolver.workspace().agents_dir()?;
                let global_agent_path = resolver.global().agents_dir()?;
                let local_mcp_path = resolver.workspace().mcp_config()?;
                let global_mcp_path = resolver.global().mcp_config()?;
                let is_interactive = subagents.iter().any(|agent| agent.is_interactive);
                let parent_tool_use_id = tool_use_id.as_deref().unwrap_or_default();
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
                            parent_tool_use_id,
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
                let res =
                    futures::future::try_join_all(subagents.into_iter().map(|subagent| {
                        subagent.query(os, input_rx.resubscribe(), control_end.clone(), parent_conv_id)
                    }))
                    .await;

                if let Err(e) = indicator_handle.wait_for_clean_screen().await {
                    error!(?e, "failed to wait for clean screen");
                }

                let summaries = res?;

                let output_serialized = serde_json::json!({
                    "summaries": summaries,
                });

                Ok(InvokeOutput {
                    output: super::OutputKind::Json(output_serialized),
                })
            },
        }
    }

    pub fn queue_description(&self, tool: &Tool, output: &mut impl Write) -> Result<()> {
        use crossterm::{
            queue,
            style,
        };

        use crate::theme::StyledText;

        match self {
            Self::ListAgents => {
                queue!(output, style::Print("Querying available agents for task delegation"),)?;
                super::display_tool_use(tool, output)?;
            },
            Self::InvokeSubagents {
                subagents,
                convo_id: _,
                tool_use_id: _,
            } => {
                if subagents.len() == 1 {
                    // Single subagent - display without batch prefix
                    let subagent = &subagents[0];
                    queue!(
                        output,
                        style::Print("Invoking subagent: "),
                        StyledText::brand_fg(),
                        style::Print(subagent.agent_name.as_deref().unwrap_or(DEFAULT_AGENT_NAME)),
                        StyledText::reset(),
                        style::Print(" with query: "),
                        StyledText::brand_fg(),
                        style::Print(&subagent.query),
                        StyledText::reset(),
                    )?;
                    super::display_tool_use(tool, output)?;
                } else {
                    // Multiple subagents - display as batch
                    queue!(
                        output,
                        style::Print("Invoking "),
                        StyledText::brand_fg(),
                        style::Print(subagents.len()),
                        StyledText::reset(),
                        style::Print(" subagents in parallel"),
                    )?;
                    super::display_tool_use(tool, output)?;
                    queue!(output, style::Print("\n"))?;
                }
            },
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use serde_json::json;

    use super::*;
    use crate::cli::agent::{
        PermissionEvalResult,
        ToolSettingTarget,
    };

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

    // Helper function to create a minimal Agent for testing
    fn create_test_agent(allowed_tools: Vec<&str>, tools_settings: HashMap<&str, serde_json::Value>) -> Agent {
        Agent {
            name: "test_agent".to_string(),
            allowed_tools: allowed_tools.into_iter().map(|s| s.to_string()).collect(),
            tools_settings: tools_settings
                .into_iter()
                .map(|(k, v)| (ToolSettingTarget(k.to_string()), v))
                .collect(),
            ..Default::default()
        }
    }

    #[tokio::test]
    async fn test_eval_perm_list_agents_in_allowlist() {
        let os = Os::new().await.unwrap();
        let agent = create_test_agent(vec!["use_subagent"], HashMap::new());
        let tool = UseSubagent::ListAgents;

        let result = tool.eval_perm(&os, &agent);
        assert_eq!(result, PermissionEvalResult::Allow);
    }

    #[tokio::test]
    async fn test_eval_perm_list_agents_with_settings() {
        let os = Os::new().await.unwrap();
        let mut settings = HashMap::new();
        settings.insert(
            "subagent",
            json!({
                "trustedAgents": ["agent1", "agent2"]
            }),
        );
        let agent = create_test_agent(vec![], settings);
        let tool = UseSubagent::ListAgents;

        let result = tool.eval_perm(&os, &agent);
        assert_eq!(result, PermissionEvalResult::Allow);
    }

    #[tokio::test]
    async fn test_eval_perm_list_agents_no_permission() {
        let os = Os::new().await.unwrap();
        let agent = create_test_agent(vec![], HashMap::new());
        let tool = UseSubagent::ListAgents;

        let result = tool.eval_perm(&os, &agent);
        assert_eq!(result, PermissionEvalResult::Ask);
    }

    #[tokio::test]
    async fn test_eval_perm_invoke_subagents_in_allowlist() {
        let os = Os::new().await.unwrap();
        let agent = create_test_agent(vec!["use_subagent"], HashMap::new());
        let tool = UseSubagent::InvokeSubagents {
            subagents: vec![InvokeSubagent {
                query: "test query".to_string(),
                agent_name: Some("test_agent".to_string()),
                relevant_context: None,
                dangerously_trust_all_tools: false,
                is_interactive: false,
            }],
            convo_id: None,
            tool_use_id: None,
        };

        let result = tool.eval_perm(&os, &agent);
        assert_eq!(result, PermissionEvalResult::Allow);
    }

    #[tokio::test]
    async fn test_eval_perm_invoke_subagents_exact_name_match_allowed() {
        let os = Os::new().await.unwrap();
        let mut settings = HashMap::new();
        settings.insert(
            "subagent",
            json!({
                "trustedAgents": ["agent1", "agent2"]
            }),
        );
        let agent = create_test_agent(vec![], settings);
        let tool = UseSubagent::InvokeSubagents {
            subagents: vec![InvokeSubagent {
                query: "test query".to_string(),
                agent_name: Some("agent1".to_string()),
                relevant_context: None,
                dangerously_trust_all_tools: false,
                is_interactive: false,
            }],
            convo_id: None,
            tool_use_id: None,
        };

        let result = tool.eval_perm(&os, &agent);
        assert_eq!(result, PermissionEvalResult::Allow);
    }

    #[tokio::test]
    async fn test_eval_perm_invoke_subagents_exact_name_match_denied() {
        let os = Os::new().await.unwrap();
        let mut settings = HashMap::new();
        settings.insert(
            "subagent",
            json!({
                "trustedAgents": ["agent1", "agent2"]
            }),
        );
        let agent = create_test_agent(vec![], settings);
        let tool = UseSubagent::InvokeSubagents {
            subagents: vec![InvokeSubagent {
                query: "test query".to_string(),
                agent_name: Some("agent3".to_string()),
                relevant_context: None,
                dangerously_trust_all_tools: false,
                is_interactive: false,
            }],
            convo_id: None,
            tool_use_id: None,
        };

        let result = tool.eval_perm(&os, &agent);
        assert_eq!(result, PermissionEvalResult::Ask);
    }

    #[tokio::test]
    async fn test_eval_perm_invoke_subagents_glob_pattern_match() {
        let os = Os::new().await.unwrap();
        let mut settings = HashMap::new();
        settings.insert(
            "subagent",
            json!({
                "trustedAgents": ["test-*"]
            }),
        );
        let agent = create_test_agent(vec![], settings);
        let tool = UseSubagent::InvokeSubagents {
            subagents: vec![InvokeSubagent {
                query: "test query".to_string(),
                agent_name: Some("test-agent-1".to_string()),
                relevant_context: None,
                dangerously_trust_all_tools: false,
                is_interactive: false,
            }],
            convo_id: None,
            tool_use_id: None,
        };

        let result = tool.eval_perm(&os, &agent);
        assert_eq!(result, PermissionEvalResult::Allow);
    }

    #[tokio::test]
    async fn test_eval_perm_invoke_subagents_glob_pattern_no_match() {
        let os = Os::new().await.unwrap();
        let mut settings = HashMap::new();
        settings.insert(
            "subagent",
            json!({
                "trustedAgents": ["test-*"]
            }),
        );
        let agent = create_test_agent(vec![], settings);
        let tool = UseSubagent::InvokeSubagents {
            subagents: vec![InvokeSubagent {
                query: "test query".to_string(),
                agent_name: Some("production-agent".to_string()),
                relevant_context: None,
                dangerously_trust_all_tools: false,
                is_interactive: false,
            }],
            convo_id: None,
            tool_use_id: None,
        };

        let result = tool.eval_perm(&os, &agent);
        assert_eq!(result, PermissionEvalResult::Ask);
    }

    #[tokio::test]
    async fn test_eval_perm_invoke_multiple_subagents_all_allowed() {
        let os = Os::new().await.unwrap();
        let mut settings = HashMap::new();
        settings.insert(
            "subagent",
            json!({
                "trustedAgents": ["agent1", "agent2", "agent3"]
            }),
        );
        let agent = create_test_agent(vec![], settings);
        let tool = UseSubagent::InvokeSubagents {
            subagents: vec![
                InvokeSubagent {
                    query: "query 1".to_string(),
                    agent_name: Some("agent1".to_string()),
                    relevant_context: None,
                    dangerously_trust_all_tools: false,
                    is_interactive: false,
                },
                InvokeSubagent {
                    query: "query 2".to_string(),
                    agent_name: Some("agent2".to_string()),
                    relevant_context: None,
                    dangerously_trust_all_tools: false,
                    is_interactive: false,
                },
            ],
            convo_id: None,
            tool_use_id: None,
        };

        let result = tool.eval_perm(&os, &agent);
        assert_eq!(result, PermissionEvalResult::Allow);
    }

    #[tokio::test]
    async fn test_eval_perm_invoke_multiple_subagents_some_denied() {
        let os = Os::new().await.unwrap();
        let mut settings = HashMap::new();
        settings.insert(
            "subagent",
            json!({
                "trustedAgents": ["agent1", "agent2"]
            }),
        );
        let agent = create_test_agent(vec![], settings);
        let tool = UseSubagent::InvokeSubagents {
            subagents: vec![
                InvokeSubagent {
                    query: "query 1".to_string(),
                    agent_name: Some("agent1".to_string()),
                    relevant_context: None,
                    dangerously_trust_all_tools: false,
                    is_interactive: false,
                },
                InvokeSubagent {
                    query: "query 2".to_string(),
                    agent_name: Some("agent3".to_string()),
                    relevant_context: None,
                    dangerously_trust_all_tools: false,
                    is_interactive: false,
                },
            ],
            convo_id: None,
            tool_use_id: None,
        };

        let result = tool.eval_perm(&os, &agent);
        assert_eq!(result, PermissionEvalResult::Ask);
    }

    #[tokio::test]
    async fn test_eval_perm_invoke_subagents_default_agent_name() {
        let os = Os::new().await.unwrap();
        let mut settings = HashMap::new();
        settings.insert(
            "subagent",
            json!({
                "trustedAgents": [DEFAULT_AGENT_NAME]
            }),
        );
        let agent = create_test_agent(vec![], settings);
        let tool = UseSubagent::InvokeSubagents {
            subagents: vec![InvokeSubagent {
                query: "test query".to_string(),
                agent_name: None, // Should use DEFAULT_AGENT_NAME
                relevant_context: None,
                dangerously_trust_all_tools: false,
                is_interactive: false,
            }],
            convo_id: None,
            tool_use_id: None,
        };

        let result = tool.eval_perm(&os, &agent);
        assert_eq!(result, PermissionEvalResult::Allow);
    }

    #[tokio::test]
    async fn test_eval_perm_invoke_subagents_invalid_settings() {
        let os = Os::new().await.unwrap();
        let mut settings = HashMap::new();
        // Invalid settings - not matching expected schema
        settings.insert("subagent", json!("invalid"));
        let agent = create_test_agent(vec![], settings);
        let tool = UseSubagent::InvokeSubagents {
            subagents: vec![InvokeSubagent {
                query: "test query".to_string(),
                agent_name: Some("agent1".to_string()),
                relevant_context: None,
                dangerously_trust_all_tools: false,
                is_interactive: false,
            }],
            convo_id: None,
            tool_use_id: None,
        };

        let result = tool.eval_perm(&os, &agent);
        // Should fall back to Ask when settings are invalid
        assert_eq!(result, PermissionEvalResult::Ask);
    }

    #[tokio::test]
    async fn test_eval_perm_invoke_subagents_empty_allowed_agents() {
        let os = Os::new().await.unwrap();
        let mut settings = HashMap::new();
        settings.insert(
            "subagent",
            json!({
                "trustedAgents": []
            }),
        );
        let agent = create_test_agent(vec![], settings);
        let tool = UseSubagent::InvokeSubagents {
            subagents: vec![InvokeSubagent {
                query: "test query".to_string(),
                agent_name: Some("agent1".to_string()),
                relevant_context: None,
                dangerously_trust_all_tools: false,
                is_interactive: false,
            }],
            convo_id: None,
            tool_use_id: None,
        };

        let result = tool.eval_perm(&os, &agent);
        assert_eq!(result, PermissionEvalResult::Ask);
    }

    #[tokio::test]
    async fn test_eval_perm_invoke_subagents_no_settings_no_allowlist() {
        let os = Os::new().await.unwrap();
        let agent = create_test_agent(vec![], HashMap::new());
        let tool = UseSubagent::InvokeSubagents {
            subagents: vec![InvokeSubagent {
                query: "test query".to_string(),
                agent_name: Some("agent1".to_string()),
                relevant_context: None,
                dangerously_trust_all_tools: false,
                is_interactive: false,
            }],
            convo_id: None,
            tool_use_id: None,
        };

        let result = tool.eval_perm(&os, &agent);
        assert_eq!(result, PermissionEvalResult::Ask);
    }

    #[tokio::test]
    async fn test_eval_perm_mixed_glob_and_exact_patterns() {
        let os = Os::new().await.unwrap();
        let mut settings = HashMap::new();
        settings.insert(
            "subagent",
            json!({
                "trustedAgents": ["exact-agent", "test-.*"]
            }),
        );
        let agent = create_test_agent(vec![], settings);

        // Test exact match
        let tool1 = UseSubagent::InvokeSubagents {
            subagents: vec![InvokeSubagent {
                query: "test query".to_string(),
                agent_name: Some("exact-agent".to_string()),
                relevant_context: None,
                dangerously_trust_all_tools: false,
                is_interactive: false,
            }],
            convo_id: None,
            tool_use_id: None,
        };
        assert_eq!(tool1.eval_perm(&os, &agent), PermissionEvalResult::Allow);

        // Test glob match
        let tool2 = UseSubagent::InvokeSubagents {
            subagents: vec![InvokeSubagent {
                query: "test query".to_string(),
                agent_name: Some("test-123".to_string()),
                relevant_context: None,
                dangerously_trust_all_tools: false,
                is_interactive: false,
            }],
            convo_id: None,
            tool_use_id: None,
        };
        assert_eq!(tool2.eval_perm(&os, &agent), PermissionEvalResult::Allow);

        // Test no match
        let tool3 = UseSubagent::InvokeSubagents {
            subagents: vec![InvokeSubagent {
                query: "test query".to_string(),
                agent_name: Some("other-agent".to_string()),
                relevant_context: None,
                dangerously_trust_all_tools: false,
                is_interactive: false,
            }],
            convo_id: None,
            tool_use_id: None,
        };
        assert_eq!(tool3.eval_perm(&os, &agent), PermissionEvalResult::Ask);
    }

    #[tokio::test]
    async fn test_eval_perm_invoke_available_agents_deny() {
        let os = Os::new().await.unwrap();
        let mut settings = HashMap::new();
        settings.insert(
            "subagent",
            json!({
                "availableAgents": ["agent1", "agent2"],
                "trustedAgents": ["agent1", "agent2", "agent3"]
            }),
        );
        let agent = create_test_agent(vec![], settings);
        let tool = UseSubagent::InvokeSubagents {
            subagents: vec![InvokeSubagent {
                query: "test query".to_string(),
                agent_name: Some("agent3".to_string()),
                relevant_context: None,
                dangerously_trust_all_tools: false,
                is_interactive: false,
            }],
            convo_id: None,
            tool_use_id: None,
        };

        let result = tool.eval_perm(&os, &agent);
        assert!(matches!(result, PermissionEvalResult::Deny(_)));
    }

    #[tokio::test]
    async fn test_eval_perm_invoke_available_agents_allow() {
        let os = Os::new().await.unwrap();
        let mut settings = HashMap::new();
        settings.insert(
            "subagent",
            json!({
                "availableAgents": ["agent1", "agent2"],
                "trustedAgents": ["agent1"]
            }),
        );
        let agent = create_test_agent(vec![], settings);
        let tool = UseSubagent::InvokeSubagents {
            subagents: vec![InvokeSubagent {
                query: "test query".to_string(),
                agent_name: Some("agent1".to_string()),
                relevant_context: None,
                dangerously_trust_all_tools: false,
                is_interactive: false,
            }],
            convo_id: None,
            tool_use_id: None,
        };

        let result = tool.eval_perm(&os, &agent);
        assert_eq!(result, PermissionEvalResult::Allow);
    }

    #[tokio::test]
    async fn test_eval_perm_invoke_available_agents_glob_deny() {
        let os = Os::new().await.unwrap();
        let mut settings = HashMap::new();
        settings.insert(
            "subagent",
            json!({
                "availableAgents": ["test-.*"],
                "trustedAgents": ["test-.*", "production-agent"]
            }),
        );
        let agent = create_test_agent(vec![], settings);
        let tool = UseSubagent::InvokeSubagents {
            subagents: vec![InvokeSubagent {
                query: "test query".to_string(),
                agent_name: Some("production-agent".to_string()),
                relevant_context: None,
                dangerously_trust_all_tools: false,
                is_interactive: false,
            }],
            convo_id: None,
            tool_use_id: None,
        };

        let result = tool.eval_perm(&os, &agent);
        assert!(matches!(result, PermissionEvalResult::Deny(_)));
    }

    #[tokio::test]
    async fn test_eval_perm_invoke_no_available_agents_uses_allowed() {
        let os = Os::new().await.unwrap();
        let mut settings = HashMap::new();
        settings.insert(
            "subagent",
            json!({
                "trustedAgents": ["agent1"]
            }),
        );
        let agent = create_test_agent(vec![], settings);
        let tool = UseSubagent::InvokeSubagents {
            subagents: vec![InvokeSubagent {
                query: "test query".to_string(),
                agent_name: Some("agent1".to_string()),
                relevant_context: None,
                dangerously_trust_all_tools: false,
                is_interactive: false,
            }],
            convo_id: None,
            tool_use_id: None,
        };

        let result = tool.eval_perm(&os, &agent);
        assert_eq!(result, PermissionEvalResult::Allow);
    }

    #[tokio::test]
    async fn test_eval_perm_invoke_visible_but_not_allowed() {
        let os = Os::new().await.unwrap();
        let mut settings = HashMap::new();
        settings.insert(
            "subagent",
            json!({
                "availableAgents": ["agent1", "agent2"],
                "trustedAgents": ["agent1"]
            }),
        );
        let agent = create_test_agent(vec![], settings);
        let tool = UseSubagent::InvokeSubagents {
            subagents: vec![InvokeSubagent {
                query: "test query".to_string(),
                agent_name: Some("agent2".to_string()),
                relevant_context: None,
                dangerously_trust_all_tools: false,
                is_interactive: false,
            }],
            convo_id: None,
            tool_use_id: None,
        };

        let result = tool.eval_perm(&os, &agent);
        // Visible but not allowed → Ask
        assert_eq!(result, PermissionEvalResult::Ask);
    }
}
