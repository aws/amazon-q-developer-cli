//! agent_crew tool — pipeline orchestrator that spawns sessions via SessionTool.

use schemars::JsonSchema;
use serde::{
    Deserialize,
    Serialize,
};
use tokio::sync::broadcast;
use tracing::{
    error,
    warn,
};

use super::session::{
    SessionResponseSender,
    SessionTool,
    SessionToolRequest,
    SessionToolResponse,
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
/// Upper bound on waiting for partial results after a deadline cancel, so a slow or
/// absent session manager cannot re-strand the parent turn we just freed.
const CANCEL_COLLECT_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);
/// Bound on a single group-activity query so an unanswered probe (no session
/// manager) resolves to "no progress" instead of hanging the stall loop. Kept
/// small because it is spent serially on every poll of an unresponsive manager,
/// yet ample for an in-process actor oneshot round-trip.
const STALL_QUERY_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(2);
/// Floor on the re-check interval so a child whose stall is a hair under the
/// window cannot spin the poll loop.
const STALL_POLL_MIN: std::time::Duration = std::time::Duration::from_millis(50);
/// Ceiling on the re-check interval so detection latency stays within roughly one
/// window even for a large deadline: without it we would sleep the whole remaining
/// window and could miss that a mid-window stall already began.
const STALL_POLL_MAX: std::time::Duration = std::time::Duration::from_secs(30);
/// Consecutive failed probes (no answer, timeout, malformed body) required before
/// "no answer" is allowed to trip the stall. A single transient failure — a
/// momentarily busy session manager — must not cancel a healthy child, while a
/// sustained failure (manager gone) still trips within a few polls.
const STALL_PROBE_FAILURE_TOLERANCE: u32 = 3;
/// Below this window, probe latency (up to [`STALL_QUERY_TIMEOUT`] per poll) is
/// comparable to the window itself, so detection accuracy degrades; warn rather
/// than clamp, since sub-second windows are a deliberate test/tuning escape hatch.
const STALL_WINDOW_ACCURACY_FLOOR: std::time::Duration = std::time::Duration::from_secs(5);
/// Minimum trigger length to avoid accidental matches on common words.
const MIN_TRIGGER_LENGTH: usize = 4;

/// Activity reported by an answered group probe: the freshest timestamp across
/// the group plus one entry per live stage, so each stage can race its own
/// idle window.
#[derive(Debug, Clone)]
struct GroupActivity {
    /// Freshest activity across the group, or `None` when nothing is live.
    freshest: Option<std::time::SystemTime>,
    /// Per-stage `(name, effective last activity)` for live stages.
    stages: Vec<(String, std::time::SystemTime)>,
}

/// One group-activity probe's outcome, distinguishing "the manager answered"
/// from "no usable answer": a transiently failed probe must not be mistaken
/// for a stalled child.
#[derive(Debug, Clone)]
enum ActivityProbe {
    /// The manager replied with the group's per-stage activity.
    Answered(GroupActivity),
    /// Send failure, probe timeout, or malformed response body.
    Failed,
}

/// The manager's answer to a stage-cancel request, distinguishing a confirmed
/// verdict from no usable answer: a cancel that was lost, timed out, or came
/// back malformed must be retried, never recorded as done.
#[derive(Debug, Clone, Copy)]
enum StageCancelAck {
    /// The manager replied: `true` = stage terminated by this cancel,
    /// `false` = no live stage by that name (it already finished).
    Confirmed(bool),
    /// Send failure, response timeout, or malformed response body.
    Unconfirmed,
}

/// Human-readable deadline: whole seconds for a normal (>=1s) window, milliseconds
/// for the sub-second windows used in tests/tuning, so neither renders as "0s".
fn format_deadline(d: std::time::Duration) -> String {
    if d < std::time::Duration::from_secs(1) {
        format!("{}ms", d.as_millis())
    } else {
        format!("{}s", d.as_secs())
    }
}

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
- background (not yet implemented): Fire-and-forget. Returns immediately.
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
          "model": { "type": "string", "description": "Optional model override for this stage" },
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
    #[serde(default)]
    pub model: Option<String>,
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
        deadline: Option<std::time::Duration>,
    ) -> ToolExecutionResult {
        self.validate_roles(crew_settings)?;
        self.validate_unique_stage_names()?;
        self.validate_loop_configs()?;
        let stages = self.clamp_loop_iterations();
        let group = format!("crew-{}", crate::agent::util::truncate_safe(&self.task, 20));

        let (spawned, pending_specs) = Self::spawn_ready_stages(&stages, &self.task, &group, &event_tx).await?;

        Self::register_pending_stages(&pending_specs, &group, &event_tx).await;

        match self.mode {
            CrewMode::Blocking => Self::await_blocking(&group, stages.len(), &tool_use_id, &event_tx, deadline).await,
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

    /// Reject duplicate stage names: the DAG (dependencies, loop targets), the
    /// stall watchdog's per-stage tracks, and stage cancellation all address
    /// stages by name, so two live same-name sessions would make a cancel
    /// ambiguous and let the un-cancelled twin evade the watchdog entirely.
    fn validate_unique_stage_names(&self) -> Result<(), ToolExecutionError> {
        let mut seen: std::collections::HashSet<&str> = std::collections::HashSet::new();
        let duplicates: Vec<&str> = self
            .stages
            .iter()
            .map(|s| s.name.as_str())
            .filter(|name| !seen.insert(name))
            .collect();
        if !duplicates.is_empty() {
            return Err(ToolExecutionError::Custom(format!(
                "Duplicate stage names are not allowed: {}. Give each stage a unique name.",
                duplicates.join(", ")
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
                model: s.model.clone(),
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
                    model: stage.model.clone(),
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
        deadline: Option<std::time::Duration>,
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

        // Stall timer: the deadline is an idle window on genuine child progress, not
        // a flat cap. Children ping the SessionManager on every UpdateEvent (assistant
        // tokens, tool calls, tool results); we poll the group's freshest activity and
        // reset the window whenever it advances, so an actively-working child — even
        // one long turn — is never cancelled. Only true inactivity for the full window
        // trips it, cancelling the still-running children and continuing with partial
        // results. A zero/None deadline disables the timer and pends on the group result.
        let deadline = deadline.filter(|d| !d.is_zero());
        let mut response_rx = response_rx;
        let Some(deadline) = deadline else {
            let response = response_rx
                .await
                .map_err(|_e| ToolExecutionError::Custom("Group wait channel dropped".to_string()))?
                .map_err(ToolExecutionError::Custom)?;
            let formatted = Self::format_group_results(&response.output);
            return Ok(ToolExecutionOutput::new(vec![ToolExecutionOutputItem::Text(format!(
                "Pipeline completed: {stage_count} stages finished.\n\n{formatted}"
            ))]));
        };

        if deadline < STALL_WINDOW_ACCURACY_FLOOR {
            tracing::warn!(
                deadline = %format_deadline(deadline),
                "subagent stall window is smaller than the probe round-trip budget; \
                 detection timing will be approximate"
            );
        }

        // Per-stage tracking so each stage races its OWN idle window: a busy
        // sibling can neither mask a wedged stage nor be cancelled as collateral
        // when it trips. Each track uses the same reported-timestamp anchor plus
        // monotonic clamp as the group-level fallback below. A track opens with
        // a fresh monotonic anchor on first observation (the clamp deliberately
        // distrusts cross-process wall-clock deltas), so a stage already silent
        // before its first probe is detected within ~2 windows of its last
        // genuine progress rather than ~1.5.
        struct StallTrack {
            last_seen: std::time::SystemTime,
            last_progress: tokio::time::Instant,
        }
        let mut stage_tracks: std::collections::HashMap<String, StallTrack> = Default::default();
        let mut cancelled_stages: std::collections::BTreeSet<String> = Default::default();

        // Group-level anchor: the fallback for a wholly unresponsive backend
        // (sustained probe failure) or a group left with nothing live to probe.
        // `last_seen` holds the freshest reported timestamp; `last_progress`
        // marks (on the monotonic clock) when we observed it advance.
        let mut last_progress = tokio::time::Instant::now();
        let mut last_seen: Option<std::time::SystemTime> = None;
        let mut failed_probes: u32 = 0;
        let response = loop {
            let probe = Self::query_group_activity(group, event_tx).await;
            let mut probe_answered = false;
            let mut live_stages = 0usize;
            match probe {
                ActivityProbe::Answered(activity) => {
                    probe_answered = true;
                    failed_probes = 0;
                    live_stages = activity.stages.len();
                    if let Some(freshest) = activity.freshest
                        && last_seen.is_none_or(|prev| freshest > prev)
                    {
                        last_seen = Some(freshest);
                        last_progress = tokio::time::Instant::now();
                    }
                    // Drop tracks for stages that finished; advance or open the rest.
                    let now = tokio::time::Instant::now();
                    stage_tracks.retain(|name, _| activity.stages.iter().any(|(n, _)| n == name));
                    for (name, seen) in activity.stages {
                        match stage_tracks.get_mut(&name) {
                            Some(track) => {
                                if seen > track.last_seen {
                                    track.last_seen = seen;
                                    track.last_progress = now;
                                }
                            },
                            None => {
                                stage_tracks.insert(name, StallTrack {
                                    last_seen: seen,
                                    last_progress: now,
                                });
                            },
                        }
                    }
                    // Cancel each individually-stalled stage; siblings keep running
                    // and the group completes with a cancellation note per victim.
                    for (name, track) in &stage_tracks {
                        if cancelled_stages.contains(name) {
                            continue;
                        }
                        let stage_stall = std::time::SystemTime::now()
                            .duration_since(track.last_seen)
                            .unwrap_or(std::time::Duration::ZERO)
                            .min(track.last_progress.elapsed());
                        if stage_stall >= deadline {
                            error!(
                                deadline = %format_deadline(deadline),
                                group,
                                stage = %name,
                                "crew stage stalled: cancelling it; sibling stages keep running"
                            );
                            // Record (and report) a cancellation only once the manager
                            // confirms it. An unconfirmed cancel (lost send, timeout)
                            // leaves the stage tracked so the next probe retries — a
                            // stage marked cancelled on faith would disarm both the
                            // per-stage retry and the whole-group fallback, re-stranding
                            // the parent on a live child. A refused cancel means the
                            // stage finished first: defer to the pending group
                            // completion instead of reporting finished work as killed.
                            match Self::cancel_group_stage(group, name, track.last_seen, event_tx).await {
                                StageCancelAck::Confirmed(true) => {
                                    // One telemetry event per victim, so multi-victim
                                    // stalls are not undercounted.
                                    let _ = event_tx.send(AgentEvent::Internal(
                                        crate::protocol::InternalEvent::SubagentDeadlineExpired { deadline },
                                    ));
                                    cancelled_stages.insert(name.clone());
                                },
                                StageCancelAck::Confirmed(false) => {},
                                StageCancelAck::Unconfirmed => {},
                            }
                        }
                    }
                },
                ActivityProbe::Failed => failed_probes = failed_probes.saturating_add(1),
            }
            // Anchor on the reported timestamp, not our observation time, and poll at
            // most every half window: a stall that began mid-window is then caught
            // within ~1.5 windows of the last genuine progress rather than up to two.
            // A backward wall-clock step saturates to zero, and clamping by the
            // monotonic elapsed-since-observation bounds a forward step or
            // suspend/resume, so neither clock move can cancel healthy children or
            // fire prematurely.
            let stall = match last_seen {
                Some(seen) => std::time::SystemTime::now()
                    .duration_since(seen)
                    .unwrap_or(std::time::Duration::ZERO)
                    .min(last_progress.elapsed()),
                None => last_progress.elapsed(),
            };
            // The whole-group trip is only the fallback now: a transient probe
            // failure is not evidence of a stall (trip on sustained failure only),
            // and an answered probe trips it only when nothing live remains to
            // probe yet the waiter never resolves (e.g. a pending stage whose
            // dependency can never be satisfied). Individually-stalled live stages
            // were already cancelled above without disturbing their siblings.
            let stalled = stall >= deadline
                && ((probe_answered && live_stages == 0) || failed_probes >= STALL_PROBE_FAILURE_TOLERANCE);
            let poll_cap = (deadline / 2).clamp(STALL_POLL_MIN, STALL_POLL_MAX);
            let remaining = deadline.saturating_sub(stall);
            let sleep_for = if stalled {
                std::time::Duration::ZERO
            } else if remaining.is_zero() {
                // Past the window but no trip condition holds (e.g. an unconfirmed
                // stage cancel awaiting retry): keep polling at the normal cadence
                // instead of collapsing to the 50ms floor and hot-looping the manager.
                poll_cap
            } else {
                remaining.min(poll_cap).max(STALL_POLL_MIN)
            };
            tokio::select! {
                // Prefer a completed group over the stall trip when both are ready in the
                // same poll, so a photo-finish completion isn't reported as a timeout
                // with empty partials.
                biased;
                res = &mut response_rx => {
                    break res
                        .map_err(|_e| ToolExecutionError::Custom("Group wait channel dropped".to_string()))?
                        .map_err(ToolExecutionError::Custom)?;
                },
                _ = tokio::time::sleep(sleep_for) => {
                    if stalled {
                        error!(
                            deadline = %format_deadline(deadline),
                            group, "crew group stalled: no live stages and no completion within the deadline window"
                        );
                        if cancelled_stages.is_empty() {
                            let _ = event_tx.send(AgentEvent::Internal(
                                crate::protocol::InternalEvent::SubagentDeadlineExpired { deadline },
                            ));
                        }
                        return Self::cancel_and_summarize(group, stage_count, event_tx, deadline).await;
                    }
                },
            }
        };

        let formatted = Self::format_group_results(&response.output);
        let header = if cancelled_stages.is_empty() {
            format!("Pipeline completed: {stage_count} stages finished.")
        } else {
            format!(
                "Pipeline finished with {} stage(s) cancelled by the idle deadline ({}): {}. A cancelled \
                 stage made no observable progress for a full window; stages depending on it were \
                 skipped, and sibling stages were unaffected. Work a cancelled stage did before \
                 cancellation (file edits, commands) may have partially applied, and a command it \
                 launched may still be running — verify before relying on it.",
                cancelled_stages.len(),
                format_deadline(deadline),
                cancelled_stages.iter().cloned().collect::<Vec<_>>().join(", ")
            )
        };
        Ok(ToolExecutionOutput::new(vec![ToolExecutionOutputItem::Text(format!(
            "{header}\n\n{formatted}"
        ))]))
    }

    /// Cancel a single stalled stage, bounded: the manager replies immediately
    /// (its teardown runs off the request loop), and a lost response must not
    /// stall the poll loop — it degrades to [`StageCancelAck::Unconfirmed`] so
    /// the caller retries on a later probe instead of assuming success. The
    /// observed timestamp lets the manager refuse a cancel the stage outran.
    async fn cancel_group_stage(
        group: &str,
        stage: &str,
        observed_last_seen: std::time::SystemTime,
        event_tx: &broadcast::Sender<AgentEvent>,
    ) -> StageCancelAck {
        let (response_tx, response_rx) = tokio::sync::oneshot::channel();
        let request = SessionToolRequest {
            request: SessionTool::CancelGroupStage {
                group: group.to_string(),
                name: stage.to_string(),
                observed_last_activity_ms: observed_last_seen
                    .duration_since(std::time::UNIX_EPOCH)
                    .ok()
                    .map(|d| d.as_millis() as u64),
            },
            response_tx: SessionResponseSender::new(response_tx),
        };
        if event_tx.send(AgentEvent::SessionToolRequest(request)).is_err() {
            return StageCancelAck::Unconfirmed;
        }
        let response: SessionToolResponse = match tokio::time::timeout(STALL_QUERY_TIMEOUT, response_rx).await {
            Ok(Ok(Ok(resp))) => resp,
            _ => return StageCancelAck::Unconfirmed,
        };
        let text = match response.output.items.first() {
            Some(ToolExecutionOutputItem::Text(t)) => t.as_str(),
            _ => return StageCancelAck::Unconfirmed,
        };
        #[derive(serde::Deserialize)]
        struct CancelResponse {
            cancelled: bool,
        }
        match serde_json::from_str::<CancelResponse>(text) {
            Ok(parsed) => StageCancelAck::Confirmed(parsed.cancelled),
            Err(e) => {
                error!(group, stage, body = text, error = %e, "unparseable stage-cancel response");
                StageCancelAck::Unconfirmed
            },
        }
    }

    /// Probe the SessionManager for the freshest child activity timestamp across
    /// the group. `Answered(None)` means the manager replied but knows no live
    /// activity for the group (group gone); `Failed` means no usable answer at
    /// all (send failure, [`STALL_QUERY_TIMEOUT`], or a malformed body) — the
    /// stall loop tolerates transient failures but trips on sustained ones.
    async fn query_group_activity(group: &str, event_tx: &broadcast::Sender<AgentEvent>) -> ActivityProbe {
        let (response_tx, response_rx) = tokio::sync::oneshot::channel();
        let request = SessionToolRequest {
            request: SessionTool::GroupLastActivity {
                group: group.to_string(),
            },
            response_tx: SessionResponseSender::new(response_tx),
        };
        if event_tx.send(AgentEvent::SessionToolRequest(request)).is_err() {
            return ActivityProbe::Failed;
        }
        // Both non-answer shapes count toward the fallback tolerance (an
        // unobservable parent channel also leaves the group waiter unresolvable),
        // but they are logged distinctly: a locally-dropped probe (parent event
        // channel lagged and discarded the request) is not evidence the manager
        // itself is unreachable.
        let response: SessionToolResponse = match tokio::time::timeout(STALL_QUERY_TIMEOUT, response_rx).await {
            Ok(Ok(Ok(resp))) => resp,
            Ok(Ok(Err(e))) => {
                warn!(group, error = %e, "group-activity probe answered with an error");
                return ActivityProbe::Failed;
            },
            Ok(Err(_)) => {
                warn!(
                    group,
                    "group-activity probe dropped before reaching the manager (parent event channel lag)"
                );
                return ActivityProbe::Failed;
            },
            Err(_) => {
                warn!(
                    group,
                    "group-activity probe not answered within budget (manager unresponsive)"
                );
                return ActivityProbe::Failed;
            },
        };
        let text = match response.output.items.first() {
            Some(ToolExecutionOutputItem::Text(t)) => t.as_str(),
            _ => return ActivityProbe::Failed,
        };
        #[derive(serde::Deserialize)]
        struct StageActivityDto {
            name: String,
            last_activity_ms: u64,
        }
        #[derive(serde::Deserialize)]
        struct ActivityResponse {
            #[serde(default)]
            last_activity_ms: Option<u64>,
            #[serde(default)]
            stages: Vec<StageActivityDto>,
        }
        // Checked: a parseable-but-absurd timestamp must degrade to one failed
        // probe like any other malformed body, not panic the watchdog task.
        let from_ms = |ms: u64| std::time::UNIX_EPOCH.checked_add(std::time::Duration::from_millis(ms));
        match serde_json::from_str::<ActivityResponse>(text) {
            Ok(parsed) => {
                let freshest = match parsed.last_activity_ms {
                    Some(ms) => match from_ms(ms) {
                        Some(t) => Some(t),
                        None => {
                            error!(group, ms, "out-of-range group-activity timestamp");
                            return ActivityProbe::Failed;
                        },
                    },
                    None => None,
                };
                let mut stages = Vec::with_capacity(parsed.stages.len());
                for s in parsed.stages {
                    let Some(t) = from_ms(s.last_activity_ms) else {
                        error!(group, stage = %s.name, ms = s.last_activity_ms, "out-of-range stage-activity timestamp");
                        return ActivityProbe::Failed;
                    };
                    stages.push((s.name, t));
                }
                ActivityProbe::Answered(GroupActivity { freshest, stages })
            },
            Err(e) => {
                // Loud: a silent schema drift here would degrade the idle window back
                // into the flat "no observed progress" behavior it replaces.
                error!(group, body = text, error = %e, "unparseable group-activity probe response");
                ActivityProbe::Failed
            },
        }
    }

    /// Cancel every still-running session in the group and return whatever partial
    /// results completed, so the parent turn can continue after the deadline expired.
    async fn cancel_and_summarize(
        group: &str,
        stage_count: usize,
        event_tx: &broadcast::Sender<AgentEvent>,
        deadline: std::time::Duration,
    ) -> ToolExecutionResult {
        let (response_tx, response_rx) = tokio::sync::oneshot::channel();
        let cancel_request = SessionToolRequest {
            request: SessionTool::CancelGroup {
                group: group.to_string(),
            },
            response_tx: SessionResponseSender::new(response_tx),
        };
        event_tx
            .send(AgentEvent::SessionToolRequest(cancel_request))
            .map_err(|e| ToolExecutionError::Custom(format!("Failed to cancel group: {e}")))?;

        // Bound the collect wait: cancellation is best-effort cleanup, so a slow or
        // absent session manager must not re-strand the parent we just freed. But
        // only an acknowledged response proves anything was torn down — this path
        // fires precisely when the manager is unobservable, so an unanswered cancel
        // must be reported as unconfirmed, not as done.
        let acked = match tokio::time::timeout(CANCEL_COLLECT_TIMEOUT, response_rx).await {
            Ok(Ok(Ok(response))) => Some(Self::format_group_results(&response.output)),
            _ => None,
        };

        let text = match acked {
            Some(partial) => format!(
                "Pipeline stopped: the crew deadline ({}) expired before all {stage_count} stages \
                 finished. Unfinished stages were cancelled; any work they completed (file edits, \
                 commands) may have partially applied, and commands they launched may still be \
                 running. Verify the state and retry with smaller stages if needed.\n\nPartial \
                 results:\n\n{partial}",
                format_deadline(deadline)
            ),
            None => format!(
                "Pipeline stopped: the crew deadline ({}) expired before all {stage_count} stages \
                 finished, and the cancellation request was NOT acknowledged by the session \
                 manager — child sessions may still be running and consuming resources. The \
                 cancel request remains queued and will apply if the manager recovers. No partial \
                 results were available; verify the state before retrying.",
                format_deadline(deadline)
            ),
        };
        Ok(ToolExecutionOutput::new(vec![ToolExecutionOutputItem::Text(text)]))
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
    use super::super::session::SessionToolResponse;
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

    // ── validate_unique_stage_names ─────────────────────────────────────

    /// Duplicate stage names would make name-addressed cancellation ambiguous:
    /// the watchdog cancels one twin and skips the name thereafter, so the
    /// other twin would evade both the per-stage retry and the group fallback.
    #[test]
    fn validate_unique_stage_names_rejects_duplicates() {
        let c = crew(vec![stage("worker", None), stage("worker", None)]);
        let err = c.validate_unique_stage_names().unwrap_err().to_string();
        assert!(err.contains("worker"), "expected the duplicate name in: {err}");

        let ok = crew(vec![stage("a", None), stage("b", None)]);
        ok.validate_unique_stage_names().expect("unique names must pass");
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
            model: None,
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
            model: Some("claude-sonnet".to_string()),
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
        assert_eq!(deserialized.model.as_deref(), Some("claude-sonnet"));
        assert_eq!(deserialized.loop_config.as_ref().unwrap().trigger, "NEEDS_CHANGES");
    }

    #[tokio::test]
    async fn spawn_ready_stages_preserves_model_overrides() {
        let mut immediate = stage("research", None);
        immediate.role = "researcher".to_string();
        immediate.model = Some("claude-opus".to_string());

        let mut delayed = stage("implement", None);
        delayed.role = "coder".to_string();
        delayed.depends_on = vec!["research".to_string()];
        delayed.model = Some("claude-sonnet".to_string());

        let (event_tx, mut event_rx) = broadcast::channel(4);
        let spawn_task = tokio::spawn(async move {
            AgentCrew::spawn_ready_stages(&[immediate, delayed], "build feature", "test-group", &event_tx).await
        });

        let AgentEvent::SessionToolRequest(request) = event_rx.recv().await.unwrap() else {
            panic!("expected session tool request");
        };
        let SessionToolRequest { request, response_tx } = request;
        assert!(matches!(
            request,
            SessionTool::SpawnSession { model: Some(model), .. } if model == "claude-opus"
        ));
        response_tx
            .send(Ok(crate::agent::tools::session::SessionToolResponse {
                output: ToolExecutionOutput::new(vec![]),
            }))
            .await
            .unwrap();

        let (_, pending) = spawn_task.await.unwrap().unwrap();
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].model.as_deref(), Some("claude-sonnet"));
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
        assert_eq!(
            parsed["properties"]["stages"]["items"]["properties"]["model"]["type"],
            "string"
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

    #[tokio::test(start_paused = true)]
    async fn blocking_stage_deadline_cancels_and_returns_partial() {
        // With no session manager responding to WaitForGroup, the deadline must fire,
        // emit SubagentDeadlineExpired, issue a CancelGroup, and return a partial
        // result summary rather than hanging. A concurrent collector drains the event
        // channel (as the real SessionManager does) and holds every request so their
        // response senders stay alive; it never answers, so both the deadline and the
        // cancel-collect wait elapse. The paused clock auto-advances through the 3600s
        // deadline and CANCEL_COLLECT_TIMEOUT, so this runs instantly.
        let (tx, _rx) = broadcast::channel(64);
        let deadline = std::time::Duration::from_secs(3600);

        let mut sub = tx.subscribe();
        let collector = tokio::spawn(async move {
            let mut held = Vec::new();
            let mut saw_deadline_event = false;
            let mut saw_cancel_request = false;
            while let Ok(event) = sub.recv().await {
                match event {
                    AgentEvent::Internal(crate::protocol::InternalEvent::SubagentDeadlineExpired { .. }) => {
                        saw_deadline_event = true;
                    },
                    AgentEvent::SessionToolRequest(req) => {
                        if matches!(req.request, SessionTool::CancelGroup { .. }) {
                            saw_cancel_request = true;
                        }
                        // Keep the request (and its response sender) alive but unanswered.
                        held.push(req);
                    },
                    _ => {},
                }
                // The CancelGroup is the last event await_blocking emits, so stop once
                // both are seen; await_blocking then times out its collect wait.
                if saw_deadline_event && saw_cancel_request {
                    break;
                }
            }
            (saw_deadline_event, saw_cancel_request)
        });

        let result = AgentCrew::await_blocking("crew-test", 1, "tool-1", &tx, Some(deadline))
            .await
            .expect("await_blocking should return Ok with partial results");

        let text = match result.items.first() {
            Some(ToolExecutionOutputItem::Text(t)) => t.clone(),
            _ => panic!("expected text output"),
        };
        assert!(text.contains("deadline"), "output should explain the deadline: {text}");

        let (saw_deadline_event, saw_cancel_request) = collector.await.expect("collector task panicked");
        assert!(saw_deadline_event, "expected a SubagentDeadlineExpired event");
        assert!(saw_cancel_request, "expected a CancelGroup request");
    }

    #[tokio::test(start_paused = true)]
    async fn blocking_stage_completes_before_deadline() {
        // When the group result arrives before the deadline, await_blocking returns the
        // consolidated success output and never fires the deadline path (no cancel, no
        // SubagentDeadlineExpired). Guards the biased select's completion-wins branch.
        let (tx, mut rx) = broadcast::channel(64);
        let deadline = std::time::Duration::from_secs(3600);

        // Subscribe before await_blocking so its WaitForGroup send cannot be missed.
        let mut sub = tx.subscribe();
        let responder = tokio::spawn(async move {
            while let Ok(event) = sub.recv().await {
                if let AgentEvent::SessionToolRequest(req) = event
                    && matches!(req.request, SessionTool::WaitForGroup { .. })
                {
                    let _ = req
                        .response_tx
                        .send(Ok(SessionToolResponse {
                            output: ToolExecutionOutput::new(vec![]),
                        }))
                        .await;
                    break;
                }
            }
        });

        let result = AgentCrew::await_blocking("crew-done", 2, "tool-3", &tx, Some(deadline))
            .await
            .expect("await_blocking should return Ok on group completion");
        responder.await.ok();

        let text = match result.items.first() {
            Some(ToolExecutionOutputItem::Text(t)) => t.clone(),
            _ => panic!("expected text output"),
        };
        assert!(text.contains("completed"), "output should report completion: {text}");

        while let Ok(event) = rx.try_recv() {
            assert!(
                !matches!(
                    event,
                    AgentEvent::Internal(crate::protocol::InternalEvent::SubagentDeadlineExpired { .. })
                ) && !matches!(
                    &event,
                    AgentEvent::SessionToolRequest(req) if matches!(req.request, SessionTool::CancelGroup { .. })
                ),
                "completion path must not fire the deadline/cancel branch"
            );
        }
    }

    #[tokio::test]
    async fn blocking_stage_stall_timer_resets_on_child_progress() {
        // The stall window is an idle timer, not a flat cap: a child that keeps
        // reporting fresh activity past several windows must NOT be cancelled. A
        // responder answers GroupLastActivity with an ever-advancing timestamp for
        // longer than the deadline, then completes the group — the deadline/cancel
        // branch must never fire. Real clock (not paused) because the stall anchor is
        // wall-clock `SystemTime`, which a paused tokio clock would not advance. The
        // window is kept comfortably wide so a starved CI runner's scheduling jitter
        // between polls cannot be mistaken for a stall. NB: a bug where the timer
        // ignored progress would cancel at ~one window and fail this test.
        let (tx, mut rx) = broadcast::channel(64);
        let deadline = std::time::Duration::from_millis(750);

        let mut sub = tx.subscribe();
        let responder = tokio::spawn(async move {
            let start = std::time::Instant::now();
            let mut wait_responder: Option<SessionResponseSender> = None;
            while let Ok(event) = sub.recv().await {
                if let AgentEvent::SessionToolRequest(req) = event {
                    match &req.request {
                        SessionTool::WaitForGroup { .. } => {
                            wait_responder = Some(req.response_tx.clone());
                        },
                        SessionTool::GroupLastActivity { .. } => {
                            // Always report "just now" so the observed activity keeps
                            // advancing and the window resets on every poll.
                            let now_ms = std::time::SystemTime::now()
                                .duration_since(std::time::UNIX_EPOCH)
                                .map_or(0u64, |d| d.as_millis() as u64);
                            let output = ToolExecutionOutput::new(vec![ToolExecutionOutputItem::Text(
                                serde_json::json!({ "last_activity_ms": now_ms }).to_string(),
                            )]);
                            let _ = req.response_tx.send(Ok(SessionToolResponse { output })).await;
                            // After ~4 windows of steady progress, complete the group.
                            if start.elapsed() >= deadline * 4
                                && let Some(w) = wait_responder.take()
                            {
                                let _ = w
                                    .send(Ok(SessionToolResponse {
                                        output: ToolExecutionOutput::new(vec![]),
                                    }))
                                    .await;
                                break;
                            }
                        },
                        _ => {},
                    }
                }
            }
        });

        let result = AgentCrew::await_blocking("crew-progress", 1, "tool-2", &tx, Some(deadline))
            .await
            .expect("await_blocking should return Ok on completion after sustained progress");
        responder.await.ok();

        let text = match result.items.first() {
            Some(ToolExecutionOutputItem::Text(t)) => t.clone(),
            _ => panic!("expected text output"),
        };
        assert!(
            text.contains("completed"),
            "sustained progress should complete, not stall: {text}"
        );

        while let Ok(event) = rx.try_recv() {
            assert!(
                !matches!(
                    event,
                    AgentEvent::Internal(crate::protocol::InternalEvent::SubagentDeadlineExpired { .. })
                ) && !matches!(
                    &event,
                    AgentEvent::SessionToolRequest(req) if matches!(req.request, SessionTool::CancelGroup { .. })
                ),
                "steady child progress must reset the stall timer, never trip it"
            );
        }
    }

    #[tokio::test]
    async fn blocking_stage_stall_timer_trips_when_activity_stays_stale() {
        // Inverse of the reset test: a child whose reported activity never advances
        // (a fixed, stale timestamp) is genuinely stalled, so the window must elapse
        // and cancel it. Proves the reset guard keys on *advancing* activity, not the
        // mere presence of a response. Real clock with a short window; CancelGroup is
        // answered so cancel_and_summarize returns promptly.
        let (tx, mut rx) = broadcast::channel(64);
        let deadline = std::time::Duration::from_millis(150);
        let stale_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0u64, |d| d.as_millis() as u64);

        let mut sub = tx.subscribe();
        let responder = tokio::spawn(async move {
            while let Ok(event) = sub.recv().await {
                if let AgentEvent::SessionToolRequest(req) = event {
                    match &req.request {
                        SessionTool::GroupLastActivity { .. } => {
                            let output = ToolExecutionOutput::new(vec![ToolExecutionOutputItem::Text(
                                serde_json::json!({ "last_activity_ms": stale_ms }).to_string(),
                            )]);
                            let _ = req.response_tx.send(Ok(SessionToolResponse { output })).await;
                        },
                        SessionTool::CancelGroup { .. } => {
                            let _ = req
                                .response_tx
                                .send(Ok(SessionToolResponse {
                                    output: ToolExecutionOutput::new(vec![]),
                                }))
                                .await;
                            break;
                        },
                        _ => {},
                    }
                }
            }
        });

        let result = AgentCrew::await_blocking("crew-stale", 1, "tool-4", &tx, Some(deadline))
            .await
            .expect("await_blocking should return Ok with partial results on stall");
        responder.await.ok();

        let text = match result.items.first() {
            Some(ToolExecutionOutputItem::Text(t)) => t.clone(),
            _ => panic!("expected text output"),
        };
        assert!(
            text.contains("deadline"),
            "stale activity should trip the stall timer: {text}"
        );

        let mut saw_deadline_event = false;
        let mut saw_cancel_request = false;
        while let Ok(event) = rx.try_recv() {
            match event {
                AgentEvent::Internal(crate::protocol::InternalEvent::SubagentDeadlineExpired { .. }) => {
                    saw_deadline_event = true;
                },
                AgentEvent::SessionToolRequest(req) if matches!(req.request, SessionTool::CancelGroup { .. }) => {
                    saw_cancel_request = true;
                },
                _ => {},
            }
        }
        assert!(
            saw_deadline_event,
            "expected a SubagentDeadlineExpired event on stale stall"
        );
        assert!(saw_cancel_request, "expected a CancelGroup request on stale stall");
    }

    #[tokio::test]
    async fn stall_timer_tolerates_transient_probe_failures() {
        // Probes scripted by index: one fresh answer, then two malformed bodies
        // straddling the window boundary, then fresh answers and completion.
        // Counting a failed probe as "no progress" (the old behavior) would
        // cancel right at the boundary; the failure tolerance must ride out the
        // blip and let the group complete. Real clock: the stall anchor is
        // wall-clock SystemTime.
        let (tx, mut rx) = broadcast::channel(64);
        let deadline = std::time::Duration::from_millis(400);

        let mut sub = tx.subscribe();
        let responder = tokio::spawn(async move {
            let mut probe_count = 0usize;
            let mut wait_responder: Option<SessionResponseSender> = None;
            while let Ok(event) = sub.recv().await {
                if let AgentEvent::SessionToolRequest(req) = event {
                    match &req.request {
                        SessionTool::WaitForGroup { .. } => {
                            wait_responder = Some(req.response_tx.clone());
                        },
                        SessionTool::GroupLastActivity { .. } => {
                            probe_count += 1;
                            let payload = if (2..=3).contains(&probe_count) {
                                "definitely not json".to_string()
                            } else {
                                let now_ms = std::time::SystemTime::now()
                                    .duration_since(std::time::UNIX_EPOCH)
                                    .map_or(0u64, |d| d.as_millis() as u64);
                                serde_json::json!({ "last_activity_ms": now_ms }).to_string()
                            };
                            let output = ToolExecutionOutput::new(vec![ToolExecutionOutputItem::Text(payload)]);
                            let _ = req.response_tx.send(Ok(SessionToolResponse { output })).await;
                            // Once probes are answering fresh again, complete the group.
                            if probe_count >= 5
                                && let Some(w) = wait_responder.take()
                            {
                                let _ = w
                                    .send(Ok(SessionToolResponse {
                                        output: ToolExecutionOutput::new(vec![]),
                                    }))
                                    .await;
                                break;
                            }
                        },
                        _ => {},
                    }
                }
            }
        });

        let result = AgentCrew::await_blocking("crew-blip", 1, "tool-5", &tx, Some(deadline))
            .await
            .expect("await_blocking should return Ok on completion despite the probe blip");
        responder.await.ok();

        let text = match result.items.first() {
            Some(ToolExecutionOutputItem::Text(t)) => t.clone(),
            _ => panic!("expected text output"),
        };
        assert!(
            text.contains("completed"),
            "a transient probe blip must not cancel a healthy group: {text}"
        );

        while let Ok(event) = rx.try_recv() {
            assert!(
                !matches!(
                    event,
                    AgentEvent::Internal(crate::protocol::InternalEvent::SubagentDeadlineExpired { .. })
                ) && !matches!(
                    &event,
                    AgentEvent::SessionToolRequest(req) if matches!(req.request, SessionTool::CancelGroup { .. })
                ),
                "transient probe failures below the tolerance must never trip the stall"
            );
        }
    }

    #[tokio::test]
    async fn stall_timer_trips_on_sustained_probe_failure() {
        // Every probe returns a malformed body: a manager that never answers
        // usefully must still trip the deadline (after the failure tolerance),
        // otherwise a dead backend would strand the parent forever.
        let (tx, mut rx) = broadcast::channel(64);
        let deadline = std::time::Duration::from_millis(150);

        let mut sub = tx.subscribe();
        let responder = tokio::spawn(async move {
            while let Ok(event) = sub.recv().await {
                if let AgentEvent::SessionToolRequest(req) = event {
                    match &req.request {
                        SessionTool::GroupLastActivity { .. } => {
                            let output = ToolExecutionOutput::new(vec![ToolExecutionOutputItem::Text(
                                "definitely not json".to_string(),
                            )]);
                            let _ = req.response_tx.send(Ok(SessionToolResponse { output })).await;
                        },
                        SessionTool::CancelGroup { .. } => {
                            let _ = req
                                .response_tx
                                .send(Ok(SessionToolResponse {
                                    output: ToolExecutionOutput::new(vec![]),
                                }))
                                .await;
                            break;
                        },
                        _ => {},
                    }
                }
            }
        });

        let result = AgentCrew::await_blocking("crew-dead-probe", 1, "tool-6", &tx, Some(deadline))
            .await
            .expect("await_blocking should return Ok with partial results on sustained probe failure");
        responder.await.ok();

        let text = match result.items.first() {
            Some(ToolExecutionOutputItem::Text(t)) => t.clone(),
            _ => panic!("expected text output"),
        };
        assert!(
            text.contains("deadline"),
            "sustained probe failure must still trip the stall timer: {text}"
        );

        let mut saw_cancel_request = false;
        while let Ok(event) = rx.try_recv() {
            if let AgentEvent::SessionToolRequest(req) = &event
                && matches!(req.request, SessionTool::CancelGroup { .. })
            {
                saw_cancel_request = true;
            }
        }
        assert!(
            saw_cancel_request,
            "expected a CancelGroup request on sustained failure"
        );
    }

    #[tokio::test]
    async fn group_fallback_reports_unconfirmed_when_cancel_unacknowledged() {
        // The whole-group fallback fires precisely when the manager is
        // unobservable, so an unanswered CancelGroup must be reported as
        // unconfirmed (children may still be running), never as completed.
        let (tx, _rx) = broadcast::channel(64);
        let deadline = std::time::Duration::from_millis(120);

        let mut sub = tx.subscribe();
        let responder = tokio::spawn(async move {
            while let Ok(event) = sub.recv().await {
                if let AgentEvent::SessionToolRequest(req) = event {
                    match &req.request {
                        SessionTool::GroupLastActivity { .. } => {
                            let output = ToolExecutionOutput::new(vec![ToolExecutionOutputItem::Text(
                                "definitely not json".to_string(),
                            )]);
                            let _ = req.response_tx.send(Ok(SessionToolResponse { output })).await;
                        },
                        SessionTool::CancelGroup { .. } => {
                            // Drop the request without replying: no acknowledgment.
                            break;
                        },
                        _ => {},
                    }
                }
            }
        });

        let result = AgentCrew::await_blocking("crew-unacked-cancel", 1, "tool-10", &tx, Some(deadline))
            .await
            .expect("await_blocking should return Ok even when the cancel is unacknowledged");
        responder.await.ok();

        let text = match result.items.first() {
            Some(ToolExecutionOutputItem::Text(t)) => t.clone(),
            _ => panic!("expected text output"),
        };
        assert!(
            text.contains("NOT acknowledged") && text.contains("may still be running"),
            "an unanswered cancel must be reported as unconfirmed: {text}"
        );
    }

    #[tokio::test]
    async fn stall_timer_cancels_only_the_stalled_stage() {
        // Two live stages: one keeps reporting fresh activity, one is frozen at
        // a stale timestamp. Only the frozen stage may be cancelled — via
        // CancelGroupStage, never a whole-group CancelGroup — and the busy
        // sibling keeps running until the group completes normally. The final
        // output names the cancelled stage.
        let (tx, mut rx) = broadcast::channel(64);
        let deadline = std::time::Duration::from_millis(200);
        let stale_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0u64, |d| d.as_millis() as u64);

        let mut sub = tx.subscribe();
        let responder = tokio::spawn(async move {
            let mut wait_responder: Option<SessionResponseSender> = None;
            let mut cancelled_stage: Option<String> = None;
            while let Ok(event) = sub.recv().await {
                if let AgentEvent::SessionToolRequest(req) = event {
                    match &req.request {
                        SessionTool::WaitForGroup { .. } => {
                            wait_responder = Some(req.response_tx.clone());
                        },
                        SessionTool::GroupLastActivity { .. } => {
                            let now_ms = std::time::SystemTime::now()
                                .duration_since(std::time::UNIX_EPOCH)
                                .map_or(0u64, |d| d.as_millis() as u64);
                            let payload = if cancelled_stage.is_none() {
                                serde_json::json!({
                                    "last_activity_ms": now_ms,
                                    "stages": [
                                        { "name": "busy", "last_activity_ms": now_ms },
                                        { "name": "frozen", "last_activity_ms": stale_ms },
                                    ],
                                })
                            } else {
                                serde_json::json!({
                                    "last_activity_ms": now_ms,
                                    "stages": [
                                        { "name": "busy", "last_activity_ms": now_ms },
                                    ],
                                })
                            };
                            let output =
                                ToolExecutionOutput::new(vec![ToolExecutionOutputItem::Text(payload.to_string())]);
                            let _ = req.response_tx.send(Ok(SessionToolResponse { output })).await;
                            // After the frozen stage was cancelled, "busy" finishes and
                            // the group completes with real results.
                            if cancelled_stage.is_some()
                                && let Some(w) = wait_responder.take()
                            {
                                let results = serde_json::json!({
                                    "results": [
                                        { "name": "busy", "result": "busy finished fine", "loop_iterations_used": 0 },
                                        { "name": "frozen", "result": "Cancelled by the idle deadline", "loop_iterations_used": 0 },
                                    ],
                                });
                                let _ = w
                                    .send(Ok(SessionToolResponse {
                                        output: ToolExecutionOutput::new(vec![ToolExecutionOutputItem::Text(
                                            results.to_string(),
                                        )]),
                                    }))
                                    .await;
                                break;
                            }
                        },
                        SessionTool::CancelGroupStage {
                            name,
                            observed_last_activity_ms,
                            ..
                        } => {
                            // The cancel must carry the observation that authorized it,
                            // so the manager can refuse one the stage outran.
                            assert_eq!(
                                *observed_last_activity_ms,
                                Some(stale_ms),
                                "cancel must carry the stale timestamp the watchdog observed"
                            );
                            cancelled_stage = Some(name.clone());
                            let output = ToolExecutionOutput::new(vec![ToolExecutionOutputItem::Text(
                                serde_json::json!({ "cancelled": true }).to_string(),
                            )]);
                            let _ = req.response_tx.send(Ok(SessionToolResponse { output })).await;
                        },
                        _ => {},
                    }
                }
            }
            cancelled_stage
        });

        let result = AgentCrew::await_blocking("crew-per-stage", 2, "tool-7", &tx, Some(deadline))
            .await
            .expect("await_blocking should complete after cancelling only the frozen stage");
        let cancelled_stage = responder.await.expect("responder task panicked");

        assert_eq!(
            cancelled_stage.as_deref(),
            Some("frozen"),
            "only the frozen stage may be cancelled"
        );
        let text = match result.items.first() {
            Some(ToolExecutionOutputItem::Text(t)) => t.clone(),
            _ => panic!("expected text output"),
        };
        assert!(
            text.contains("frozen") && text.contains("idle deadline"),
            "output must name the cancelled stage: {text}"
        );
        assert!(
            text.contains("busy finished fine"),
            "the busy sibling's real result must survive: {text}"
        );

        let mut deadline_events = 0;
        while let Ok(event) = rx.try_recv() {
            match &event {
                AgentEvent::Internal(crate::protocol::InternalEvent::SubagentDeadlineExpired { .. }) => {
                    deadline_events += 1;
                },
                AgentEvent::SessionToolRequest(req) => {
                    assert!(
                        !matches!(req.request, SessionTool::CancelGroup { .. }),
                        "a single stalled stage must never trigger a whole-group cancel"
                    );
                },
                _ => {},
            }
        }
        assert_eq!(deadline_events, 1, "exactly one deadline event for the stalled stage");
    }

    #[tokio::test]
    async fn stall_timer_retries_unconfirmed_stage_cancel() {
        // The first CancelGroupStage gets no reply (the manager dropped it), so
        // the stage must NOT be recorded as cancelled: a later probe retries the
        // cancel, and only the confirmed second attempt counts — once, in both
        // the output header and telemetry.
        let (tx, mut rx) = broadcast::channel(64);
        let deadline = std::time::Duration::from_millis(200);
        let stale_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0u64, |d| d.as_millis() as u64);

        let mut sub = tx.subscribe();
        let responder = tokio::spawn(async move {
            let mut wait_responder: Option<SessionResponseSender> = None;
            let mut cancel_attempts = 0usize;
            while let Ok(event) = sub.recv().await {
                if let AgentEvent::SessionToolRequest(req) = event {
                    match &req.request {
                        SessionTool::WaitForGroup { .. } => {
                            wait_responder = Some(req.response_tx.clone());
                        },
                        SessionTool::GroupLastActivity { .. } => {
                            let payload = if cancel_attempts < 2 {
                                serde_json::json!({
                                    "last_activity_ms": stale_ms,
                                    "stages": [ { "name": "frozen", "last_activity_ms": stale_ms } ],
                                })
                            } else {
                                serde_json::json!({ "last_activity_ms": null, "stages": [] })
                            };
                            let output =
                                ToolExecutionOutput::new(vec![ToolExecutionOutputItem::Text(payload.to_string())]);
                            let _ = req.response_tx.send(Ok(SessionToolResponse { output })).await;
                        },
                        SessionTool::CancelGroupStage { .. } => {
                            cancel_attempts += 1;
                            if cancel_attempts == 1 {
                                // Drop the request without replying: an unconfirmed cancel.
                                continue;
                            }
                            let output = ToolExecutionOutput::new(vec![ToolExecutionOutputItem::Text(
                                serde_json::json!({ "cancelled": true }).to_string(),
                            )]);
                            let _ = req.response_tx.send(Ok(SessionToolResponse { output })).await;
                            if let Some(w) = wait_responder.take() {
                                let results = serde_json::json!({
                                    "results": [
                                        { "name": "frozen", "result": "Cancelled by the idle deadline", "loop_iterations_used": 0 },
                                    ],
                                });
                                let _ = w
                                    .send(Ok(SessionToolResponse {
                                        output: ToolExecutionOutput::new(vec![ToolExecutionOutputItem::Text(
                                            results.to_string(),
                                        )]),
                                    }))
                                    .await;
                                break;
                            }
                        },
                        _ => {},
                    }
                }
            }
            cancel_attempts
        });

        let result = AgentCrew::await_blocking("crew-cancel-retry", 1, "tool-8", &tx, Some(deadline))
            .await
            .expect("await_blocking should complete after the retried cancel is confirmed");
        let cancel_attempts = responder.await.expect("responder task panicked");

        assert!(
            cancel_attempts >= 2,
            "an unconfirmed cancel must be retried, got {cancel_attempts} attempt(s)"
        );
        let text = match result.items.first() {
            Some(ToolExecutionOutputItem::Text(t)) => t.clone(),
            _ => panic!("expected text output"),
        };
        assert!(
            text.contains("1 stage(s) cancelled") && text.contains("frozen"),
            "the confirmed cancel must be reported exactly once: {text}"
        );

        let mut deadline_events = 0;
        while let Ok(event) = rx.try_recv() {
            if let AgentEvent::Internal(crate::protocol::InternalEvent::SubagentDeadlineExpired { .. }) = &event {
                deadline_events += 1;
            }
        }
        assert_eq!(
            deadline_events, 1,
            "only the confirmed cancel may emit deadline telemetry"
        );
    }

    #[tokio::test]
    async fn stall_timer_defers_to_completion_on_refused_cancel() {
        // The manager refuses the cancel (`cancelled: false`): the stage finished
        // just before the cancel landed. Its real completion must be reported —
        // no cancellation header, no deadline telemetry.
        let (tx, mut rx) = broadcast::channel(64);
        let deadline = std::time::Duration::from_millis(200);
        let stale_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0u64, |d| d.as_millis() as u64);

        let mut sub = tx.subscribe();
        let responder = tokio::spawn(async move {
            let mut wait_responder: Option<SessionResponseSender> = None;
            while let Ok(event) = sub.recv().await {
                if let AgentEvent::SessionToolRequest(req) = event {
                    match &req.request {
                        SessionTool::WaitForGroup { .. } => {
                            wait_responder = Some(req.response_tx.clone());
                        },
                        SessionTool::GroupLastActivity { .. } => {
                            let payload = serde_json::json!({
                                "last_activity_ms": stale_ms,
                                "stages": [ { "name": "photo", "last_activity_ms": stale_ms } ],
                            });
                            let output =
                                ToolExecutionOutput::new(vec![ToolExecutionOutputItem::Text(payload.to_string())]);
                            let _ = req.response_tx.send(Ok(SessionToolResponse { output })).await;
                        },
                        SessionTool::CancelGroupStage { .. } => {
                            let output = ToolExecutionOutput::new(vec![ToolExecutionOutputItem::Text(
                                serde_json::json!({ "cancelled": false }).to_string(),
                            )]);
                            let _ = req.response_tx.send(Ok(SessionToolResponse { output })).await;
                            // The stage had already finished: complete the group with
                            // its genuine result.
                            if let Some(w) = wait_responder.take() {
                                let results = serde_json::json!({
                                    "results": [
                                        { "name": "photo", "result": "finished at the wire", "loop_iterations_used": 0 },
                                    ],
                                });
                                let _ = w
                                    .send(Ok(SessionToolResponse {
                                        output: ToolExecutionOutput::new(vec![ToolExecutionOutputItem::Text(
                                            results.to_string(),
                                        )]),
                                    }))
                                    .await;
                                break;
                            }
                        },
                        _ => {},
                    }
                }
            }
        });

        let result = AgentCrew::await_blocking("crew-cancel-refused", 1, "tool-9", &tx, Some(deadline))
            .await
            .expect("await_blocking should complete normally when the cancel is refused");
        responder.await.expect("responder task panicked");

        let text = match result.items.first() {
            Some(ToolExecutionOutputItem::Text(t)) => t.clone(),
            _ => panic!("expected text output"),
        };
        assert!(
            text.contains("Pipeline completed") && text.contains("finished at the wire"),
            "a refused cancel must report the stage's real completion: {text}"
        );

        while let Ok(event) = rx.try_recv() {
            assert!(
                !matches!(
                    event,
                    AgentEvent::Internal(crate::protocol::InternalEvent::SubagentDeadlineExpired { .. })
                ),
                "a refused cancel must not emit deadline telemetry"
            );
        }
    }
}
