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

/// Hard ceiling on loop iterations regardless of what the model requests.
const MAX_LOOP_ITERATIONS_CAP: u32 = 10;
/// Minimum trigger length to avoid accidental matches on common words.
const MIN_TRIGGER_LENGTH: usize = 4;

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub struct LoopConfig {
    /// Name of the stage to loop back to when triggered.
    pub target: String,
    /// Maximum number of loop iterations before stopping (capped at 10).
    pub max_iterations: u32,
    /// Text that, when present in the stage's output, triggers the loop.
    pub trigger: String,
}

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
    /// Optional loop-back configuration: when this stage completes and its output
    /// contains the trigger text, re-run the target stage (up to max_iterations).
    #[serde(default)]
    pub loop_to: Option<LoopConfig>,
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

LOOPS:
- Add loop_to on a stage to create iterative cycles (e.g., reviewer loops back to implementer)
- trigger: text in the stage's output that triggers the loop (e.g., "NEEDS_CHANGES")
- max_iterations: safety cap to prevent infinite loops
- The target stage re-runs with the triggering stage's feedback as context

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
          "model": { "type": "string" },
          "loop_to": {
            "type": "object",
            "description": "Loop back to a target stage when this stage's output contains the trigger text. Useful for review→implement cycles.",
            "properties": {
              "target": { "type": "string", "description": "Name of the stage to loop back to" },
              "max_iterations": { "type": "integer", "description": "Maximum loop iterations (safety cap)" },
              "trigger": { "type": "string", "description": "Text in output that triggers the loop (e.g. 'NEEDS_CHANGES')" }
            },
            "required": ["target", "max_iterations", "trigger"]
          }
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

/// Substitute the `{task}` placeholder in each stage's `prompt_template` with
/// the overall task value, in-place on the JSON model input. The tool schema
/// (TOOL_SCHEMA above) tells the model to write `{task}` literally and let the
/// runtime substitute — backend execution does this for the spawned subagent
/// in `spawn_ready_stages` (line 327, 335). The TUI separately renders the
/// raw model input from `tool_use_block.input`, which would otherwise display
/// `{task}` literally to the user. Substituting on the display copy keeps the
/// rendered prompt identical to what the spawned subagent actually receives.
///
/// Caller responsibility: only invoke for `Tool::AgentCrew(_)` tool calls.
/// Silently no-ops on malformed input (missing `task`, non-array `stages`,
/// non-string `prompt_template`) — display path must never panic.
pub fn substitute_task_placeholder(input: &mut serde_json::Value) {
    let Some(obj) = input.as_object_mut() else { return };
    let Some(task) = obj.get("task").and_then(|v| v.as_str()).map(str::to_string) else {
        return;
    };
    let Some(stages) = obj.get_mut("stages").and_then(|v| v.as_array_mut()) else {
        return;
    };
    for stage in stages {
        let Some(stage_obj) = stage.as_object_mut() else {
            continue;
        };
        let Some(template) = stage_obj.get("prompt_template").and_then(|v| v.as_str()) else {
            continue;
        };
        if template.contains("{task}") {
            let substituted = template.replace("{task}", &task);
            stage_obj.insert("prompt_template".to_string(), serde_json::Value::String(substituted));
        }
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
    /// Loop-back config from the source stage that points to this stage's target.
    #[serde(default)]
    pub loop_config: Option<LoopConfig>,
}

impl AgentCrew {
    pub async fn execute(
        &self,
        tool_use_id: String,
        event_tx: broadcast::Sender<AgentEvent>,
        crew_settings: &AgentCrewSettings,
    ) -> ToolExecutionResult {
        self.validate_roles(crew_settings)?;
        self.validate_loop_configs()?;
        let stages = self.clamp_loop_iterations();
        let group = format!("crew-{}", crate::agent::util::truncate_safe(&self.task, 20));

        let (spawned, pending_specs) = Self::spawn_ready_stages(&stages, &self.task, &group, &event_tx).await?;

        Self::register_pending_stages(&pending_specs, &group, &event_tx).await;

        match self.mode {
            CrewMode::Blocking => Self::await_blocking(&group, stages.len(), &tool_use_id, &event_tx).await,
            #[allow(unreachable_patterns)]
            _ => Ok(Self::non_blocking_summary(&spawned, &pending_specs)),
        }
    }

    // ── Validation ──────────────────────────────────────────────────────

    /// Reject stages whose role isn't in the allowed agent list.
    fn validate_roles(&self, crew_settings: &AgentCrewSettings) -> Result<(), ToolExecutionError> {
        if crew_settings.available_agents.is_empty() {
            return Ok(());
        }
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
        Ok(())
    }

    /// Validate every `loop_to` config: target exists, no self-loops, trigger is
    /// long enough to avoid accidental matches, max_iterations > 0, and no
    /// circular A↔B loop_to pairs.
    fn validate_loop_configs(&self) -> Result<(), ToolExecutionError> {
        let stage_names: std::collections::HashSet<&str> = self.stages.iter().map(|s| s.name.as_str()).collect();

        for stage in &self.stages {
            if let Some(lc) = &stage.loop_to {
                if !stage_names.contains(lc.target.as_str()) {
                    return Err(ToolExecutionError::Custom(format!(
                        "Stage '{}' has loop_to targeting '{}', which does not exist",
                        stage.name, lc.target
                    )));
                }
                if lc.target == stage.name {
                    return Err(ToolExecutionError::Custom(format!(
                        "Stage '{}' cannot loop_to itself",
                        stage.name
                    )));
                }
                if lc.trigger.trim().len() < MIN_TRIGGER_LENGTH {
                    return Err(ToolExecutionError::Custom(format!(
                        "Stage '{}' loop trigger '{}' is too short (min {} chars). Use a distinctive trigger like NEEDS_CHANGES",
                        stage.name, lc.trigger, MIN_TRIGGER_LENGTH
                    )));
                }
                if lc.max_iterations == 0 {
                    return Err(ToolExecutionError::Custom(format!(
                        "Stage '{}' loop max_iterations must be at least 1",
                        stage.name
                    )));
                }
            }
        }

        Self::detect_circular_loops(&self.stages)
    }

    /// Detect mutual loop_to references (A→B and B→A).
    fn detect_circular_loops(stages: &[PipelineStage]) -> Result<(), ToolExecutionError> {
        let loop_targets: std::collections::HashMap<&str, &str> = stages
            .iter()
            .filter_map(|s| s.loop_to.as_ref().map(|lc| (s.name.as_str(), lc.target.as_str())))
            .collect();
        for (src, tgt) in &loop_targets {
            if loop_targets.get(tgt) == Some(src) {
                return Err(ToolExecutionError::Custom(format!(
                    "Circular loop_to detected: '{}' → '{}' → '{}'. Only one direction should have loop_to",
                    src, tgt, src
                )));
            }
        }
        Ok(())
    }

    /// Return a copy of stages with `max_iterations` clamped to the server-side cap.
    fn clamp_loop_iterations(&self) -> Vec<PipelineStage> {
        self.stages
            .iter()
            .map(|s| {
                let mut s = s.clone();
                if let Some(lc) = &mut s.loop_to {
                    lc.max_iterations = lc.max_iterations.min(MAX_LOOP_ITERATIONS_CAP);
                }
                s
            })
            .collect()
    }

    // ── Spawning ────────────────────────────────────────────────────────

    /// Spawn stages with no dependencies immediately. Returns (spawned names, pending specs).
    async fn spawn_ready_stages(
        stages: &[PipelineStage],
        task: &str,
        group: &str,
        event_tx: &broadcast::Sender<AgentEvent>,
    ) -> Result<(Vec<String>, Vec<PendingStageSpec>), ToolExecutionError> {
        let pending_specs: Vec<PendingStageSpec> = stages
            .iter()
            .filter(|s| !s.depends_on.is_empty())
            .map(|s| PendingStageSpec {
                name: s.name.clone(),
                role: s.role.clone(),
                task: s.prompt_template.replace("{task}", task),
                depends_on: s.depends_on.clone(),
                loop_config: s.loop_to.clone(),
            })
            .collect();

        // Fire all spawn requests up front, then await every response. The
        // session manager actor still processes spawn messages one-at-a-time,
        // but batching the SENDS here removes the per-stage agent_crew↔actor
        // round-trip serialization. Without this batching, each iteration
        // pays a full IPC round trip (event_tx → handler → SessionManager →
        // resp_sender) before the next stage even reaches the actor's queue,
        // which staggers the resulting SUBAGENT_LIST_UPDATE notifications
        // across hundreds of milliseconds and makes subagents appear in the
        // TUI footer one-at-a-time. Batching collapses those round trips so
        // the notifications fire in rapid succession (within a single render
        // frame) and the user sees all stages appear together.
        let mut response_rxs = Vec::new();
        let mut spawned = Vec::new();
        for stage in stages.iter().filter(|s| s.depends_on.is_empty()) {
            let stage_task = stage.prompt_template.replace("{task}", task);
            let (response_tx, response_rx) = tokio::sync::oneshot::channel();
            let request = SessionToolRequest {
                request: SessionTool::SpawnSession {
                    agent_name: stage.role.clone(),
                    task: stage_task,
                    name: Some(stage.name.clone()),
                    role: Some(stage.role.clone()),
                    group: Some(group.to_string()),
                    persistent: Some(false),
                },
                response_tx: SessionResponseSender::new(response_tx),
            };
            event_tx
                .send(AgentEvent::SessionToolRequest(request))
                .map_err(|e| ToolExecutionError::Custom(format!("Failed to spawn stage {}: {e}", stage.name)))?;
            response_rxs.push(response_rx);
            spawned.push(stage.name.clone());
        }
        // Await every spawn to ensure all sessions are registered before
        // WaitForGroup runs. Without this, WaitForGroup can race ahead and
        // see an empty group (`.all()` on empty iterator = true), firing
        // immediately.
        for rx in response_rxs {
            let _ = rx.await;
        }

        Ok((spawned, pending_specs))
    }

    /// Register pending stages so the session manager can trigger them when deps complete.
    async fn register_pending_stages(
        pending_specs: &[PendingStageSpec],
        group: &str,
        event_tx: &broadcast::Sender<AgentEvent>,
    ) {
        if pending_specs.is_empty() {
            return;
        }
        let (response_tx, response_rx) = tokio::sync::oneshot::channel();
        let request = SessionToolRequest {
            request: SessionTool::RegisterPendingStages {
                group: group.to_string(),
                pending_stages: pending_specs.to_vec(),
            },
            response_tx: SessionResponseSender::new(response_tx),
        };
        let _ = event_tx.send(AgentEvent::SessionToolRequest(request));
        let _ = response_rx.await;
    }

    // ── Completion ──────────────────────────────────────────────────────

    /// Block until all stages complete, then format consolidated results.
    async fn await_blocking(
        group: &str,
        stage_count: usize,
        tool_use_id: &str,
        event_tx: &broadcast::Sender<AgentEvent>,
    ) -> ToolExecutionResult {
        let _ = event_tx.send(AgentEvent::Update(crate::protocol::UpdateEvent::ToolCallUpdate {
            id: tool_use_id.to_string(),
            content: crate::protocol::ContentChunk::Text(format!(
                "⏳ Running crew pipeline ({stage_count} stages)... Press ctrl+g to monitor progress."
            )),
        }));

        let (response_tx, response_rx) = tokio::sync::oneshot::channel();
        let wait_request = SessionToolRequest {
            request: SessionTool::WaitForGroup {
                group: group.to_string(),
            },
            response_tx: SessionResponseSender::new(response_tx),
        };
        event_tx
            .send(AgentEvent::SessionToolRequest(wait_request))
            .map_err(|e| ToolExecutionError::Custom(format!("Failed to wait for group: {e}")))?;

        let response = response_rx
            .await
            .map_err(|_e| ToolExecutionError::Custom("Group wait channel dropped".to_string()))?
            .map_err(ToolExecutionError::Custom)?;

        let formatted = Self::format_group_results(&response.output);
        Ok(ToolExecutionOutput::new(vec![ToolExecutionOutputItem::Text(format!(
            "Pipeline completed: {stage_count} stages finished.\n\n{formatted}"
        ))]))
    }

    /// Parse the JSON results envelope and format as markdown sections.
    fn format_group_results(output: &ToolExecutionOutput) -> String {
        #[derive(serde::Deserialize)]
        struct GroupResult {
            name: String,
            result: String,
            #[serde(default)]
            loop_iterations_used: u64,
        }
        #[derive(serde::Deserialize)]
        struct GroupResponse {
            #[serde(default)]
            results: Vec<GroupResult>,
        }

        let text = match output.items.first() {
            Some(ToolExecutionOutputItem::Text(t)) => t.as_str(),
            _ => return "No text response".to_string(),
        };
        let response: GroupResponse = serde_json::from_str(text).unwrap_or_else(|_| GroupResponse { results: vec![] });

        if response.results.is_empty() {
            return "No results available".to_string();
        }

        response
            .results
            .iter()
            .map(|r| {
                if r.loop_iterations_used > 0 {
                    format!(
                        "## {} (↻ {} iterations)\n\n{}",
                        r.name, r.loop_iterations_used, r.result
                    )
                } else {
                    format!("## {}\n\n{}", r.name, r.result)
                }
            })
            .collect::<Vec<_>>()
            .join("\n\n---\n\n")
    }

    /// Build a summary for non-blocking mode (fire-and-forget).
    fn non_blocking_summary(spawned: &[String], pending: &[PendingStageSpec]) -> ToolExecutionOutput {
        let pending_names: Vec<&str> = pending.iter().map(|s| s.name.as_str()).collect();
        ToolExecutionOutput::new(vec![ToolExecutionOutputItem::Text(format!(
            "Pipeline started: {} stages spawned immediately: [{}]. {} stages pending dependencies: [{}]. Monitor with ctrl+g.",
            spawned.len(),
            spawned.join(", "),
            pending_names.len(),
            pending_names.join(", ")
        ))])
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

    // ── Helpers for loop_to tests ───────────────────────────────────────

    fn stage(name: &str, loop_to: Option<LoopConfig>) -> PipelineStage {
        PipelineStage {
            name: name.to_string(),
            role: "default".to_string(),
            prompt_template: "{task}".to_string(),
            depends_on: vec![],
            model: None,
            loop_to,
        }
    }

    fn lc(target: &str, max_iterations: u32, trigger: &str) -> Option<LoopConfig> {
        Some(LoopConfig {
            target: target.to_string(),
            max_iterations,
            trigger: trigger.to_string(),
        })
    }

    fn crew(stages: Vec<PipelineStage>) -> AgentCrew {
        AgentCrew {
            task: "test".to_string(),
            stages,
            mode: CrewMode::Blocking,
        }
    }

    // ── validate_loop_configs ───────────────────────────────────────────

    #[test]
    fn validate_loop_configs_target_does_not_exist() {
        let c = crew(vec![stage("A", lc("nonexistent", 3, "RETRY"))]);
        let err = c.validate_loop_configs().unwrap_err().to_string();
        assert!(err.contains("nonexistent"), "expected missing target in: {err}");
    }

    #[test]
    fn validate_loop_configs_self_loop() {
        let c = crew(vec![stage("A", lc("A", 3, "RETRY"))]);
        let err = c.validate_loop_configs().unwrap_err().to_string();
        assert!(
            err.contains("cannot loop_to itself"),
            "expected self-loop error in: {err}"
        );
    }

    #[test]
    fn validate_loop_configs_short_trigger() {
        let c = crew(vec![stage("A", lc("B", 3, "no")), stage("B", None)]);
        let err = c.validate_loop_configs().unwrap_err().to_string();
        assert!(err.contains("too short"), "expected short trigger error in: {err}");
    }

    #[test]
    fn validate_loop_configs_zero_max_iterations() {
        let c = crew(vec![stage("A", lc("B", 0, "RETRY")), stage("B", None)]);
        let err = c.validate_loop_configs().unwrap_err().to_string();
        assert!(err.contains("at least 1"), "expected zero iterations error in: {err}");
    }

    #[test]
    fn validate_loop_configs_valid() {
        let c = crew(vec![stage("A", lc("B", 3, "NEEDS_CHANGES")), stage("B", None)]);
        assert!(c.validate_loop_configs().is_ok());
    }

    #[test]
    fn validate_loop_configs_no_loops() {
        let c = crew(vec![stage("A", None), stage("B", None)]);
        assert!(c.validate_loop_configs().is_ok());
    }

    // ── detect_circular_loops ───────────────────────────────────────────

    #[test]
    fn detect_circular_loops_mutual() {
        let stages = vec![stage("A", lc("B", 3, "RETRY")), stage("B", lc("A", 3, "RETRY"))];
        let err = AgentCrew::detect_circular_loops(&stages).unwrap_err().to_string();
        assert!(err.contains("Circular loop_to"), "expected circular error in: {err}");
    }

    #[test]
    fn detect_circular_loops_one_direction_ok() {
        let stages = vec![stage("A", lc("B", 3, "RETRY")), stage("B", None)];
        assert!(AgentCrew::detect_circular_loops(&stages).is_ok());
    }

    #[test]
    fn detect_circular_loops_none() {
        let stages = vec![stage("A", None), stage("B", None)];
        assert!(AgentCrew::detect_circular_loops(&stages).is_ok());
    }

    // ── clamp_loop_iterations ───────────────────────────────────────────

    #[test]
    fn clamp_loop_iterations_above_cap() {
        let c = crew(vec![stage("A", lc("B", 20, "RETRY")), stage("B", None)]);
        let clamped = c.clamp_loop_iterations();
        assert_eq!(clamped[0].loop_to.as_ref().unwrap().max_iterations, 10);
    }

    #[test]
    fn clamp_loop_iterations_below_cap() {
        let c = crew(vec![stage("A", lc("B", 5, "RETRY")), stage("B", None)]);
        let clamped = c.clamp_loop_iterations();
        assert_eq!(clamped[0].loop_to.as_ref().unwrap().max_iterations, 5);
    }

    #[test]
    fn clamp_loop_iterations_no_loop() {
        let c = crew(vec![stage("A", None)]);
        let clamped = c.clamp_loop_iterations();
        assert!(clamped[0].loop_to.is_none());
    }

    // ── format_group_results ────────────────────────────────────────────

    #[test]
    fn format_group_results_valid_json() {
        let json = serde_json::json!({
            "results": [
                {"name": "Research", "result": "Found 3 papers", "loop_iterations_used": 0},
                {"name": "Code", "result": "Implemented feature", "loop_iterations_used": 0}
            ]
        });
        let output = ToolExecutionOutput::new(vec![ToolExecutionOutputItem::Text(json.to_string())]);
        let formatted = AgentCrew::format_group_results(&output);
        assert!(formatted.contains("## Research"));
        assert!(formatted.contains("Found 3 papers"));
        assert!(formatted.contains("## Code"));
        assert!(formatted.contains("Implemented feature"));
        // No loop annotation when iterations = 0
        assert!(!formatted.contains("↻"));
    }

    #[test]
    fn format_group_results_with_loop_iterations() {
        let json = serde_json::json!({
            "results": [
                {"name": "reviewer", "result": "All good", "loop_iterations_used": 3}
            ]
        });
        let output = ToolExecutionOutput::new(vec![ToolExecutionOutputItem::Text(json.to_string())]);
        let formatted = AgentCrew::format_group_results(&output);
        assert!(formatted.contains("## reviewer (↻ 3 iterations)"));
        assert!(formatted.contains("All good"));
    }

    #[test]
    fn format_group_results_empty_array() {
        let json = serde_json::json!({"results": []});
        let output = ToolExecutionOutput::new(vec![ToolExecutionOutputItem::Text(json.to_string())]);
        let formatted = AgentCrew::format_group_results(&output);
        assert_eq!(formatted, "No results available");
    }

    #[test]
    fn format_group_results_invalid_json() {
        let output = ToolExecutionOutput::new(vec![ToolExecutionOutputItem::Text("not json".to_string())]);
        let formatted = AgentCrew::format_group_results(&output);
        // Invalid JSON falls through to the empty results array default, then empty join
        assert!(formatted.is_empty() || formatted == "No results available" || !formatted.contains("##"));
    }

    #[test]
    fn format_group_results_no_text_item() {
        let output = ToolExecutionOutput::new(vec![ToolExecutionOutputItem::Json(serde_json::json!({}))]);
        let formatted = AgentCrew::format_group_results(&output);
        assert_eq!(formatted, "No text response");
    }

    #[test]
    fn format_group_results_empty_output() {
        let output = ToolExecutionOutput::new(vec![]);
        let formatted = AgentCrew::format_group_results(&output);
        assert_eq!(formatted, "No text response");
    }

    // ── non_blocking_summary ────────────────────────────────────────────

    #[test]
    fn non_blocking_summary_with_pending() {
        let spawned = vec!["research".to_string(), "code".to_string()];
        let pending = vec![PendingStageSpec {
            name: "review".to_string(),
            role: "reviewer".to_string(),
            task: "review code".to_string(),
            depends_on: vec!["code".to_string()],
            loop_config: None,
        }];
        let output = AgentCrew::non_blocking_summary(&spawned, &pending);
        let text = match &output.items[0] {
            ToolExecutionOutputItem::Text(t) => t.as_str(),
            _ => panic!("expected text"),
        };
        assert!(text.contains("2 stages spawned"));
        assert!(text.contains("research"));
        assert!(text.contains("code"));
        assert!(text.contains("1 stages pending"));
        assert!(text.contains("review"));
    }

    #[test]
    fn non_blocking_summary_zero_pending() {
        let spawned = vec!["A".to_string()];
        let output = AgentCrew::non_blocking_summary(&spawned, &[]);
        let text = match &output.items[0] {
            ToolExecutionOutputItem::Text(t) => t.as_str(),
            _ => panic!("expected text"),
        };
        assert!(text.contains("0 stages pending"));
    }

    // ── validate_roles ──────────────────────────────────────────────────

    #[test]
    fn validate_roles_empty_available_allows_all() {
        let c = crew(vec![stage("A", None)]);
        let settings = AgentCrewSettings::default();
        assert!(c.validate_roles(&settings).is_ok());
    }

    #[test]
    fn validate_roles_exact_match_passes() {
        let mut s = stage("A", None);
        s.role = "research".to_string();
        let c = crew(vec![s]);
        let settings: AgentCrewSettings = serde_json::from_value(serde_json::json!({
            "availableAgents": ["research"]
        }))
        .unwrap();
        assert!(c.validate_roles(&settings).is_ok());
    }

    #[test]
    fn validate_roles_glob_match_passes() {
        let mut s = stage("A", None);
        s.role = "test-unit".to_string();
        let c = crew(vec![s]);
        let settings: AgentCrewSettings = serde_json::from_value(serde_json::json!({
            "availableAgents": ["test-*"]
        }))
        .unwrap();
        assert!(c.validate_roles(&settings).is_ok());
    }

    #[test]
    fn validate_roles_denied_returns_error() {
        let mut s = stage("A", None);
        s.role = "hacker".to_string();
        let c = crew(vec![s]);
        let settings: AgentCrewSettings = serde_json::from_value(serde_json::json!({
            "availableAgents": ["research", "code"]
        }))
        .unwrap();
        let err = c.validate_roles(&settings).unwrap_err().to_string();
        assert!(err.contains("hacker"), "expected denied role in: {err}");
    }

    #[test]
    fn validate_roles_multiple_denied() {
        let mut s1 = stage("A", None);
        s1.role = "bad1".to_string();
        let mut s2 = stage("B", None);
        s2.role = "bad2".to_string();
        let c = crew(vec![s1, s2]);
        let settings: AgentCrewSettings = serde_json::from_value(serde_json::json!({
            "availableAgents": ["research"]
        }))
        .unwrap();
        let err = c.validate_roles(&settings).unwrap_err().to_string();
        assert!(err.contains("bad1"));
        assert!(err.contains("bad2"));
    }

    #[test]
    fn validate_roles_partial_match() {
        let mut s1 = stage("A", None);
        s1.role = "research".to_string();
        let mut s2 = stage("B", None);
        s2.role = "evil".to_string();
        let c = crew(vec![s1, s2]);
        let settings: AgentCrewSettings = serde_json::from_value(serde_json::json!({
            "availableAgents": ["research"]
        }))
        .unwrap();
        let err = c.validate_roles(&settings).unwrap_err().to_string();
        assert!(err.contains("evil"));
        assert!(!err.contains("research"));
    }

    // ── validate_loop_configs additional edge cases ─────────────────────

    #[test]
    fn validate_loop_configs_whitespace_trigger() {
        let c = crew(vec![stage("A", lc("B", 3, "   ")), stage("B", None)]);
        let err = c.validate_loop_configs().unwrap_err().to_string();
        assert!(err.contains("too short"));
    }

    #[test]
    fn validate_loop_configs_exactly_min_trigger() {
        // MIN_TRIGGER_LENGTH is 4, so exactly 4 chars should pass
        let c = crew(vec![stage("A", lc("B", 1, "DONE")), stage("B", None)]);
        assert!(c.validate_loop_configs().is_ok());
    }

    // ── Serde roundtrip tests ───────────────────────────────────────────

    #[test]
    fn serde_roundtrip_agent_crew() {
        let c = AgentCrew {
            task: "build feature".to_string(),
            stages: vec![
                PipelineStage {
                    name: "research".to_string(),
                    role: "researcher".to_string(),
                    prompt_template: "Research {task}".to_string(),
                    depends_on: vec![],
                    model: Some("claude-sonnet".to_string()),
                    loop_to: None,
                },
                PipelineStage {
                    name: "implement".to_string(),
                    role: "coder".to_string(),
                    prompt_template: "Implement {task}".to_string(),
                    depends_on: vec!["research".to_string()],
                    model: None,
                    loop_to: Some(LoopConfig {
                        target: "research".to_string(),
                        max_iterations: 3,
                        trigger: "NEEDS_MORE_INFO".to_string(),
                    }),
                },
            ],
            mode: CrewMode::Blocking,
        };
        let json = serde_json::to_string(&c).unwrap();
        let deserialized: AgentCrew = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.task, "build feature");
        assert_eq!(deserialized.stages.len(), 2);
        assert_eq!(deserialized.stages[0].name, "research");
        assert_eq!(deserialized.stages[0].model.as_deref(), Some("claude-sonnet"));
        assert_eq!(deserialized.stages[1].depends_on, vec!["research"]);
        assert_eq!(deserialized.stages[1].loop_to.as_ref().unwrap().target, "research");
        assert_eq!(deserialized.stages[1].loop_to.as_ref().unwrap().max_iterations, 3);
        assert_eq!(deserialized.mode, CrewMode::Blocking);
    }

    #[test]
    fn serde_roundtrip_pending_stage_spec() {
        let spec = PendingStageSpec {
            name: "review".to_string(),
            role: "reviewer".to_string(),
            task: "review the code".to_string(),
            depends_on: vec!["implement".to_string()],
            loop_config: Some(LoopConfig {
                target: "implement".to_string(),
                max_iterations: 5,
                trigger: "NEEDS_CHANGES".to_string(),
            }),
        };
        let json = serde_json::to_string(&spec).unwrap();
        let deserialized: PendingStageSpec = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.name, "review");
        assert_eq!(deserialized.depends_on, vec!["implement"]);
        assert_eq!(deserialized.loop_config.as_ref().unwrap().trigger, "NEEDS_CHANGES");
    }

    #[test]
    fn serde_crew_mode_default() {
        let json = serde_json::json!({"task": "x", "stages": []});
        let c: AgentCrew = serde_json::from_value(json).unwrap();
        assert_eq!(c.mode, CrewMode::Blocking);
    }

    #[test]
    fn serde_loop_config_roundtrip() {
        let lc = LoopConfig {
            target: "stage_a".to_string(),
            max_iterations: 7,
            trigger: "RETRY_NOW".to_string(),
        };
        let json = serde_json::to_string(&lc).unwrap();
        let deserialized: LoopConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.target, "stage_a");
        assert_eq!(deserialized.max_iterations, 7);
        assert_eq!(deserialized.trigger, "RETRY_NOW");
    }

    #[test]
    fn serde_pipeline_stage_minimal() {
        let json = serde_json::json!({
            "name": "s1",
            "role": "r1",
            "prompt_template": "do {task}"
        });
        let s: PipelineStage = serde_json::from_value(json).unwrap();
        assert_eq!(s.name, "s1");
        assert!(s.depends_on.is_empty());
        assert!(s.model.is_none());
        assert!(s.loop_to.is_none());
    }

    // ── BuiltInToolTrait methods ────────────────────────────────────────

    #[test]
    fn builtin_trait_name() {
        assert_eq!(AgentCrew::name(), BuiltInToolName::AgentCrew);
    }

    #[test]
    fn builtin_trait_description_not_empty() {
        let desc = AgentCrew::description();
        assert!(desc.contains("pipeline"));
    }

    #[test]
    fn builtin_trait_input_schema_valid_json() {
        let schema = AgentCrew::input_schema();
        let parsed: serde_json::Value = serde_json::from_str(&schema).unwrap();
        assert_eq!(parsed["type"], "object");
        assert!(
            parsed["required"]
                .as_array()
                .unwrap()
                .contains(&serde_json::json!("task"))
        );
        assert!(
            parsed["required"]
                .as_array()
                .unwrap()
                .contains(&serde_json::json!("stages"))
        );
    }

    #[test]
    fn builtin_trait_aliases() {
        let aliases = AgentCrew::aliases().unwrap();
        assert!(aliases.contains(&"agent_crew"));
    }

    // ── get_canonical_name ──────────────────────────────────────────────

    #[test]
    fn get_canonical_name_returns_builtin() {
        let name = AgentCrew::get_canonical_name();
        assert_eq!(name, CanonicalToolName::BuiltIn(BuiltInToolName::AgentCrew));
    }

    // ── generate_dynamic_tool_spec description content ──────────────────

    #[test]
    fn dynamic_spec_includes_agent_descriptions() {
        let agents = vec![make_agent("research", "Deep research agent")];
        let settings = AgentCrewSettings::default();
        let spec = AgentCrew::generate_dynamic_tool_spec(&agents, &settings);
        let schema = serde_json::Value::Object(spec.input_schema);
        let role_desc = schema
            .pointer("/properties/stages/items/properties/role/description")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("");
        assert!(role_desc.contains("Deep research agent"));
        assert!(role_desc.contains("research"));
    }

    #[test]
    fn dynamic_spec_agent_no_description() {
        let mut cfg = AgentConfigV2025_08_22::default();
        cfg.name = "nodesc".to_string();
        cfg.description = None;
        let agent = LoadedAgentConfig::new(
            AgentConfig::V2025_08_22(cfg),
            ConfigSource::BuiltIn,
            ResolvedGlobalPrompt::None,
        );
        let settings = AgentCrewSettings::default();
        let spec = AgentCrew::generate_dynamic_tool_spec(&[agent], &settings);
        let schema = serde_json::Value::Object(spec.input_schema);
        let role_desc = schema
            .pointer("/properties/stages/items/properties/role/description")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("");
        assert!(role_desc.contains("No description"));
    }

    // ── clamp_loop_iterations at boundary ───────────────────────────────

    #[test]
    fn clamp_loop_iterations_exactly_at_cap() {
        let c = crew(vec![stage("A", lc("B", 10, "RETRY")), stage("B", None)]);
        let clamped = c.clamp_loop_iterations();
        assert_eq!(clamped[0].loop_to.as_ref().unwrap().max_iterations, 10);
    }

    // ── AgentIdentifier glob edge cases via validate_roles ──────────────

    #[test]
    fn validate_roles_glob_star_prefix() {
        let mut s = stage("A", None);
        s.role = "my-research".to_string();
        let c = crew(vec![s]);
        let settings: AgentCrewSettings = serde_json::from_value(serde_json::json!({
            "availableAgents": ["*-research"]
        }))
        .unwrap();
        assert!(c.validate_roles(&settings).is_ok());
    }

    #[test]
    fn validate_roles_glob_middle_star() {
        let mut s = stage("A", None);
        s.role = "test-unit-fast".to_string();
        let c = crew(vec![s]);
        let settings: AgentCrewSettings = serde_json::from_value(serde_json::json!({
            "availableAgents": ["test-*-fast"]
        }))
        .unwrap();
        assert!(c.validate_roles(&settings).is_ok());
    }

    #[test]
    fn validate_roles_glob_no_match() {
        let mut s = stage("A", None);
        s.role = "production".to_string();
        let c = crew(vec![s]);
        let settings: AgentCrewSettings = serde_json::from_value(serde_json::json!({
            "availableAgents": ["test-*"]
        }))
        .unwrap();
        let err = c.validate_roles(&settings).unwrap_err().to_string();
        assert!(err.contains("production"));
    }
}
