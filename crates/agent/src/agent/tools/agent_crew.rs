//! agent_crew tool — pipeline orchestrator that spawns sessions via SessionTool.

use schemars::JsonSchema;
use serde::{
    Deserialize,
    Serialize,
};
use tokio::sync::broadcast;

use super::session::{
    SessionResponseSender,
    SessionTool,
    SessionToolRequest,
};
use super::{
    BuiltInToolName,
    BuiltInToolTrait,
    ToolExecutionError,
    ToolExecutionOutput,
    ToolExecutionOutputItem,
    ToolExecutionResult,
};
use crate::agent::agent_config::LoadedAgentConfig;
use crate::agent::agent_config::definitions::{
    AgentCrewSettings,
    AgentIdentifier,
};
use crate::agent::agent_loop::types::ToolSpec;
use crate::agent_config::parse::CanonicalToolName;
use crate::protocol::AgentEvent;

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub struct PipelineStage {
    pub name: String,
    pub role: String,
    pub prompt_template: String,
    #[serde(default)]
    pub depends_on: Vec<String>,
    #[serde(default)]
    pub model: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
#[derive(Default)]
pub enum CrewMode {
    #[default]
    Blocking,
    // TODO: add background mode Here is a prompt that points the agent to the right direction for
    // implementing this feature: currently there is a problem with the crew tool's non block
    // interface: should the model decide to invoke the tool in non-block mode, it is likely to get
    // into an invocation loop with the session tool where it constantly checks for completion
    // until the task is done. Ideally, the model should be made aware that the task is not going
    // to be done right away and that it should not check right away if it is invoked in a
    // non-block mode. The checking should be done by the user (via a keyboard shortcut) and the
    // user should be notified via the UI that a background task is completed. Thus, this task to
    // fix this interface consists of two major parts:
    //
    // a. a tool spec refine in the crew tool to
    // let the model know should the crew tool be invoked in non-block mode to not check right away
    // and instead defer the checking to the user.
    //
    // b. when crew tool completes, emit an event to
    // the UI. And on the UI side, updates the prompt line (i.e. the bottom of the app) with a
    // special symbol that a background task is ready. there could be multiple of these background
    // task in flight but for now let's assume only one can exist at a time and not worry about
    // displaying which one is ready or which result to include.
    //
    // c. an extension in the acp
    // protocol that signals to the session manager, which then signals the appropriate acp
    // session, which then signals to the agent loop to include the pending new info into the
    // conversation.
    // Background,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub struct AgentCrew {
    pub task: String,
    pub stages: Vec<PipelineStage>,
    #[serde(default)]
    pub mode: CrewMode,
}

const TOOL_DESCRIPTION: &str = r#"
Spawn and coordinate multiple AI agents in a pipeline (DAG). Each stage runs as a
persistent session. Stages with no depends_on start immediately in parallel.

MODES:
- background (not yet implemented): Fire-and-forget. Returns immediately, results arrive in inbox.
- blocking (default): Waits for all stages to complete, returns consolidated results.

USE THIS when you need multi-step work with specialized agents:
- Research → Implement → Review pipelines
- Parallel research tracks that feed into a single implementer
- Any workflow where stages have dependencies

Each stage becomes a session you can monitor via ctrl+g in the TUI.
"#;

const TOOL_SCHEMA: &str = r#"
{
  "type": "object",
  "required": ["task", "stages"],
  "properties": {
    "task": { "type": "string", "description": "Overall task description" },
    "mode": { 
      "type": "string", 
      "enum": ["blocking"],
      "description": "Execution mode: 'blocking' (wait for completion)"
    },
    "stages": {
      "type": "array",
      "items": {
        "type": "object",
        "required": ["name", "role", "prompt_template"],
        "properties": {
          "name": { "type": "string" },
          "role": { "type": "string" },
          "prompt_template": { "type": "string", "description": "Task for this stage. Use {task} to reference the overall task." },
          "depends_on": { "type": "array", "items": { "type": "string" } },
          "model": { "type": "string" }
        }
      }
    }
  }
}
"#;

impl BuiltInToolTrait for AgentCrew {
    fn name() -> BuiltInToolName {
        BuiltInToolName::AgentCrew
    }

    fn description() -> std::borrow::Cow<'static, str> {
        TOOL_DESCRIPTION.into()
    }

    fn input_schema() -> std::borrow::Cow<'static, str> {
        TOOL_SCHEMA.into()
    }

    fn aliases() -> Option<&'static [&'static str]> {
        Some(&["agent_crew"])
    }
}

/// Spec for a pending stage passed to the session manager for DAG execution.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub struct PendingStageSpec {
    pub name: String,
    pub role: String,
    pub task: String,
    pub depends_on: Vec<String>,
}

impl AgentCrew {
    pub async fn execute(
        &self,
        tool_use_id: String,
        event_tx: broadcast::Sender<AgentEvent>,
        crew_settings: &AgentCrewSettings,
    ) -> ToolExecutionResult {
        // Validate stage roles against availableAgents before spawning
        if !crew_settings.available_agents.is_empty() {
            let denied: Vec<&str> = self
                .stages
                .iter()
                .filter(|s| !AgentIdentifier::any_matches(&crew_settings.available_agents, &s.role))
                .map(|s| s.role.as_str())
                .collect();
            if !denied.is_empty() {
                return Err(ToolExecutionError::Custom(format!(
                    "Agents not available for crew stages: {}",
                    denied.join(", ")
                )));
            }
        }

        let group = format!("crew-{}", &self.task[..self.task.len().min(20)]);

        // Spawn all stages with no dependencies immediately (parallel)
        let ready: Vec<&PipelineStage> = self.stages.iter().filter(|s| s.depends_on.is_empty()).collect();
        let pending_specs: Vec<PendingStageSpec> = self
            .stages
            .iter()
            .filter(|s| !s.depends_on.is_empty())
            .map(|s| PendingStageSpec {
                name: s.name.clone(),
                role: s.role.clone(),
                task: s.prompt_template.replace("{task}", &self.task),
                depends_on: s.depends_on.clone(),
            })
            .collect();

        let mut spawned = Vec::new();
        for stage in &ready {
            let task = stage.prompt_template.replace("{task}", &self.task);
            let (response_tx, response_rx) = tokio::sync::oneshot::channel();
            let request = SessionToolRequest {
                request: SessionTool::SpawnSession {
                    agent_name: stage.role.clone(),
                    task,
                    name: Some(stage.name.clone()),
                    role: Some(stage.role.clone()),
                    group: Some(group.clone()),
                    persistent: Some(false),
                },
                response_tx: SessionResponseSender::new(response_tx),
            };
            event_tx
                .send(AgentEvent::SessionToolRequest(request))
                .map_err(|e| ToolExecutionError::Custom(format!("Failed to spawn stage {}: {e}", stage.name)))?;
            // Await the response to ensure the session is registered in the session manager
            // before we send WaitForGroup. Without this, WaitForGroup can race ahead and
            // see an empty group (`.all()` on empty iterator = true), firing immediately.
            let _ = response_rx.await;
            spawned.push(stage.name.clone());
        }

        // Register pending stages so the session manager can trigger them when deps complete
        if !pending_specs.is_empty() {
            let (response_tx, response_rx) = tokio::sync::oneshot::channel();
            let register_request = SessionToolRequest {
                request: SessionTool::RegisterPendingStages {
                    group: group.clone(),
                    pending_stages: pending_specs.clone(),
                },
                response_tx: SessionResponseSender::new(response_tx),
            };
            let _ = event_tx.send(AgentEvent::SessionToolRequest(register_request));
            let _ = response_rx.await;
        }

        match self.mode {
            CrewMode::Blocking => {
                // Emit live output so LLM knows it's waiting
                let _ = event_tx.send(AgentEvent::Update(crate::protocol::UpdateEvent::ToolCallUpdate {
                    id: tool_use_id.clone(),
                    content: crate::protocol::ContentChunk::Text(format!(
                        "⏳ Running crew pipeline ({} stages)... Press ctrl+g to monitor progress.",
                        self.stages.len()
                    )),
                }));

                // Wait for all stages to complete via session manager
                let (response_tx, response_rx) = tokio::sync::oneshot::channel();
                let wait_request = SessionToolRequest {
                    request: SessionTool::WaitForGroup { group: group.clone() },
                    response_tx: SessionResponseSender::new(response_tx),
                };
                event_tx
                    .send(AgentEvent::SessionToolRequest(wait_request))
                    .map_err(|e| ToolExecutionError::Custom(format!("Failed to wait for group: {e}")))?;

                // Await completion
                let response = response_rx
                    .await
                    .map_err(|_e| ToolExecutionError::Custom("Group wait channel dropped".to_string()))?
                    .map_err(ToolExecutionError::Custom)?;

                // Parse and format consolidated results
                let response_text = match &response.output.items[0] {
                    ToolExecutionOutputItem::Text(text) => text.clone(),
                    _ => "No text response".to_string(),
                };

                let results: serde_json::Value =
                    serde_json::from_str(&response_text).unwrap_or_else(|_| serde_json::json!({"results": []}));

                let formatted_results = if let Some(results_array) = results.get("results").and_then(|r| r.as_array()) {
                    results_array
                        .iter()
                        .map(|r| {
                            format!(
                                "## {}\n\n{}",
                                r.get("name").and_then(|n| n.as_str()).unwrap_or("Unknown"),
                                r.get("result").and_then(|res| res.as_str()).unwrap_or("No result")
                            )
                        })
                        .collect::<Vec<_>>()
                        .join("\n\n---\n\n")
                } else {
                    "No results available".to_string()
                };

                Ok(ToolExecutionOutput::new(vec![ToolExecutionOutputItem::Text(format!(
                    "Pipeline completed: {} stages finished.\n\n{}",
                    self.stages.len(),
                    formatted_results
                ))]))
            },
            // TODO: enable non-blocking mode
            #[allow(unreachable_patterns)]
            _ => {
                let pending_names: Vec<&str> = pending_specs.iter().map(|s| s.name.as_str()).collect();
                Ok(ToolExecutionOutput::new(vec![ToolExecutionOutputItem::Text(format!(
                    "Pipeline started: {} stages spawned immediately: [{}]. {} stages pending dependencies: [{}]. Monitor with ctrl+g.",
                    spawned.len(),
                    spawned.join(", "),
                    pending_names.len(),
                    pending_names.join(", ")
                ))]))
            },
        }
    }

    pub fn get_canonical_name() -> CanonicalToolName {
        CanonicalToolName::BuiltIn(BuiltInToolName::AgentCrew)
    }

    /// Generate a tool spec with available agent names/descriptions injected into the `role` field.
    ///
    /// When `crew_settings` has a non-empty `available_agents` list, only matching agents are
    /// included in the schema enum shown to the model.
    pub fn generate_dynamic_tool_spec(
        available_agents: &[LoadedAgentConfig],
        crew_settings: &AgentCrewSettings,
    ) -> ToolSpec {
        let base: ToolSpec = super::generate_tool_spec_from_trait::<AgentCrew>();
        if available_agents.is_empty() {
            return base;
        }

        // Filter agents by availableAgents config (empty = all allowed)
        let filtered: Vec<&LoadedAgentConfig> = if crew_settings.available_agents.is_empty() {
            available_agents.iter().collect()
        } else {
            available_agents
                .iter()
                .filter(|a| AgentIdentifier::any_matches(&crew_settings.available_agents, a.name()))
                .collect()
        };

        if filtered.is_empty() {
            return base;
        }

        let mut schema = base.input_schema;

        // Build role field with enum + description listing available agents
        let agent_names: Vec<serde_json::Value> = filtered
            .iter()
            .map(|a| serde_json::Value::String(a.name().to_string()))
            .collect();

        let agent_descriptions: String = filtered
            .iter()
            .map(|a| {
                let desc = a.config().description().unwrap_or("No description");
                format!("- {}: {}", a.name(), desc)
            })
            .collect::<Vec<_>>()
            .join("\n");

        let role_schema = serde_json::json!({
            "type": "string",
            "enum": agent_names,
            "description": format!("Agent to use for this stage. Available agents:\n{}", agent_descriptions)
        });

        // Patch stages.items.properties.role
        if let Some(stages) = schema.get_mut("properties").and_then(|p| p.get_mut("stages"))
            && let Some(items) = stages.get_mut("items")
            && let Some(props) = items.get_mut("properties")
        {
            props["role"] = role_schema;
        }

        ToolSpec {
            input_schema: schema,
            ..base
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::agent_config::definitions::{
        AgentConfigV2025_08_22,
        AgentCrewSettings,
    };
    use crate::agent::agent_config::{
        ConfigSource,
        LoadedAgentConfig,
        ResolvedGlobalPrompt,
    };
    use crate::agent_config::definitions::AgentConfig;

    fn make_agent(name: &str, desc: &str) -> LoadedAgentConfig {
        let mut cfg = AgentConfigV2025_08_22::default();
        cfg.name = name.to_string();
        cfg.description = Some(desc.to_string());
        LoadedAgentConfig::new(
            AgentConfig::V2025_08_22(cfg),
            ConfigSource::BuiltIn,
            ResolvedGlobalPrompt::None,
        )
    }

    fn role_enum(spec: &ToolSpec) -> Vec<String> {
        let schema = serde_json::Value::Object(spec.input_schema.clone());
        schema
            .pointer("/properties/stages/items/properties/role/enum")
            .and_then(serde_json::Value::as_array)
            .map(|arr| {
                arr.iter()
                    .filter_map(serde_json::Value::as_str)
                    .map(String::from)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default()
    }

    // --- AgentCrewSettings deserialization ---

    #[test]
    fn test_crew_settings_deser_empty() {
        let json = serde_json::json!({});
        let s: AgentCrewSettings = serde_json::from_value(json).unwrap();
        assert!(s.available_agents.is_empty());
        assert!(s.trusted_agents.is_empty());
    }

    #[test]
    fn test_crew_settings_deser_exact_names() {
        let json = serde_json::json!({
            "availableAgents": ["research", "code"],
            "trustedAgents": ["research"]
        });
        let s: AgentCrewSettings = serde_json::from_value(json).unwrap();
        assert_eq!(s.available_agents.len(), 2);
        assert!(s.available_agents[0].matches("research"));
        assert!(!s.available_agents[0].matches("code"));
        assert!(s.trusted_agents[0].matches("research"));
    }

    #[test]
    fn test_crew_settings_deser_glob_patterns() {
        let json = serde_json::json!({
            "availableAgents": ["test-*", "research"],
            "trustedAgents": ["test-*"]
        });
        let s: AgentCrewSettings = serde_json::from_value(json).unwrap();
        assert!(s.available_agents[0].matches("test-unit"));
        assert!(s.available_agents[0].matches("test-integration"));
        assert!(!s.available_agents[0].matches("research"));
        assert!(s.available_agents[1].matches("research"));
        assert!(s.trusted_agents[0].matches("test-foo"));
        assert!(!s.trusted_agents[0].matches("research"));
    }

    #[test]
    fn test_crew_settings_in_tools_settings() {
        let json = serde_json::json!({
            "name": "my-agent",
            "toolsSettings": {
                "crew": {
                    "availableAgents": ["a", "b"],
                    "trustedAgents": ["a"]
                }
            }
        });
        let cfg: AgentConfigV2025_08_22 = serde_json::from_value(json).unwrap();
        let ts = cfg.tools_settings.unwrap();
        assert_eq!(ts.crew.available_agents.len(), 2);
        assert_eq!(ts.crew.trusted_agents.len(), 1);
    }

    #[test]
    fn test_crew_settings_alias_agent_crew() {
        let json = serde_json::json!({
            "name": "my-agent",
            "toolsSettings": {
                "agent_crew": {
                    "availableAgents": ["x"]
                }
            }
        });
        let cfg: AgentConfigV2025_08_22 = serde_json::from_value(json).unwrap();
        assert_eq!(cfg.tools_settings.unwrap().crew.available_agents.len(), 1);
    }

    // --- generate_dynamic_tool_spec filtering ---

    #[test]
    fn test_dynamic_spec_no_filter_shows_all() {
        let agents = vec![make_agent("a", "Agent A"), make_agent("b", "Agent B")];
        let settings = AgentCrewSettings::default(); // empty = no filter
        let spec = AgentCrew::generate_dynamic_tool_spec(&agents, &settings);
        let names = role_enum(&spec);
        assert_eq!(names, vec!["a", "b"]);
    }

    #[test]
    fn test_dynamic_spec_filters_by_available() {
        let agents = vec![
            make_agent("research", "Research"),
            make_agent("code", "Code"),
            make_agent("test", "Test"),
        ];
        let settings: AgentCrewSettings = serde_json::from_value(serde_json::json!({
            "availableAgents": ["research", "code"]
        }))
        .unwrap();
        let spec = AgentCrew::generate_dynamic_tool_spec(&agents, &settings);
        let names = role_enum(&spec);
        assert_eq!(names, vec!["research", "code"]);
    }

    #[test]
    fn test_dynamic_spec_filters_by_glob() {
        let agents = vec![
            make_agent("test-unit", "Unit"),
            make_agent("test-integ", "Integ"),
            make_agent("research", "Research"),
        ];
        let settings: AgentCrewSettings = serde_json::from_value(serde_json::json!({
            "availableAgents": ["test-*"]
        }))
        .unwrap();
        let spec = AgentCrew::generate_dynamic_tool_spec(&agents, &settings);
        let names = role_enum(&spec);
        assert_eq!(names, vec!["test-unit", "test-integ"]);
    }

    #[test]
    fn test_dynamic_spec_empty_agents_returns_base() {
        let settings: AgentCrewSettings = serde_json::from_value(serde_json::json!({
            "availableAgents": ["research"]
        }))
        .unwrap();
        let spec = AgentCrew::generate_dynamic_tool_spec(&[], &settings);
        // No role enum when no agents provided
        assert!(role_enum(&spec).is_empty());
    }

    #[test]
    fn test_dynamic_spec_no_match_returns_base() {
        let agents = vec![make_agent("code", "Code")];
        let settings: AgentCrewSettings = serde_json::from_value(serde_json::json!({
            "availableAgents": ["research"]
        }))
        .unwrap();
        let spec = AgentCrew::generate_dynamic_tool_spec(&agents, &settings);
        assert!(role_enum(&spec).is_empty());
    }
}
