use std::collections::HashMap;
use std::future::Future;
use std::io::Write;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use chat_cli_ui::conduit::get_conduit;
use chat_cli_ui::subagent_indicator::SubagentIndicator;
use eyre::{
    Result,
    bail,
};
use schemars::JsonSchema;
use serde::Deserialize;
use tracing::{
    error,
    warn,
};

use super::{
    InvokeOutput,
    Tool,
    ToolInfo,
};
use crate::agent::{
    StageActivity,
    Subagent,
};
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
    #[allow(clippy::too_many_arguments)]
    fn as_subagent<'a>(
        &'a self,
        id: u16,
        local_mcp_path: &'a PathBuf,
        global_mcp_path: &'a PathBuf,
        parent_tool_use_id: &'a str,
        code_intelligence: Option<std::sync::Arc<tokio::sync::RwLock<code_agent_sdk::CodeIntelligence>>>,
        query_override: Option<&'a str>,
        registry_data: Option<&'a crate::mcp_registry::McpRegistryResponse>,
        web_tools_enabled: bool,
        stage_activity: Arc<StageActivity>,
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
            query: query_override.unwrap_or(query.as_str()),
            agent_name: agent_name.as_deref(),
            task_context: relevant_context.as_deref(),
            dangerously_trust_all_tools: *dangerously_trust_all_tools,
            is_interactive: *is_interactive,
            local_mcp_path,
            global_mcp_path,
            parent_tool_use_id,
            code_intelligence,
            registry_data,
            web_tools_enabled,
            stage_activity: Some(stage_activity),
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
                    PermissionEvalResult::ask()
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
                            return PermissionEvalResult::ask();
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

                    // Second check: runtime trust (user typed 't') or trustedAgents
                    if is_in_allowlist
                        || agents_to_spawn
                            .iter()
                            .all(|agent| trusted_agents.iter().any(|trusted_agent| trusted_agent == *agent))
                    {
                        PermissionEvalResult::Allow
                    } else {
                        PermissionEvalResult::ask()
                    }
                },
                None => {
                    if is_in_allowlist {
                        PermissionEvalResult::Allow
                    } else {
                        PermissionEvalResult::ask()
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

    pub async fn invoke(
        &self,
        os: &Os,
        agents: &Agents,
        code_intelligence: &Option<std::sync::Arc<tokio::sync::RwLock<code_agent_sdk::CodeIntelligence>>>,
        tool_manager: &mut crate::cli::chat::tool_manager::ToolManager,
        registry_data: Option<&crate::mcp_registry::McpRegistryResponse>,
    ) -> Result<InvokeOutput> {
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
                let local_mcp_path = resolver.workspace().mcp_config()?;
                let global_mcp_path = resolver.global().mcp_config()?;
                let is_interactive = subagents.iter().any(|agent| agent.is_interactive);
                let parent_tool_use_id = tool_use_id.as_deref().unwrap_or_default();
                let web_tools_enabled = tool_manager.web_tools_enabled;

                // Resolve @prompt-name references in each subagent query using the parent's
                // tool_manager (which has MCP servers already running). This allows the main
                // agent's LLM to pass `@agent-sop:code-assist task_description="..."` and have
                // the prompt content resolved before the subagent starts.
                let mut resolved_queries: Vec<String> = Vec::with_capacity(subagents.len());
                for invoke_subagent in subagents.iter() {
                    let resolved = resolve_prompt_in_query(&invoke_subagent.query, os, tool_manager).await;
                    resolved_queries.push(resolved);
                }

                // Original (pre-expansion) task text for cancellation summaries: the
                // resolved query can be an entire expanded @prompt SOP, which must not
                // be re-injected into a parent already trying to recover context.
                let task_descriptions: Vec<String> =
                    subagents.iter().map(|s| truncate_task_description(&s.query)).collect();

                // One activity anchor per child, so the idle deadline answers "has
                // THIS child moved recently?" — a busy sibling can neither mask a
                // wedged child nor be cancelled as collateral when it trips.
                let stage_activities: Vec<Arc<StageActivity>> = (0..subagents.len())
                    .map(|_| Arc::new(StageActivity::default()))
                    .collect();
                let subagents = subagents
                    .iter()
                    .enumerate()
                    .map(|(id, invoke_subagent)| {
                        invoke_subagent.as_subagent(
                            id as u16,
                            &local_mcp_path,
                            &global_mcp_path,
                            parent_tool_use_id,
                            code_intelligence.clone(),
                            Some(resolved_queries[id].as_str()),
                            registry_data,
                            web_tools_enabled,
                            stage_activities[id].clone(),
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

                // Children have a per-turn stream watchdog but can still run unbounded
                // many turns; each child races its OWN idle window, which its progress
                // events (and human-wait holds) keep restarting, so an actively-working
                // child runs indefinitely. A child that goes a full window without
                // progress is cancelled alone — its future is dropped (terminating its
                // session) while healthy siblings keep running — and the parent turn
                // continues with real summaries plus a cancellation summary per
                // cancelled child, instead of erroring.
                let deadline = subagent_deadline(os);
                let mut in_flight: futures::stream::FuturesUnordered<_> = subagents
                    .into_iter()
                    .enumerate()
                    .map(|(index, subagent)| {
                        let input_rx = input_rx.resubscribe();
                        let control_end = control_end.clone();
                        let activity = stage_activities[index].clone();
                        async move {
                            let child = subagent.query(os, input_rx, control_end, parent_conv_id);
                            (index, run_child_with_idle_deadline(child, &activity, deadline).await)
                        }
                    })
                    .collect();

                let StageOutcome {
                    completed,
                    child_error,
                    expired,
                } = drain_stage(&mut in_flight, task_descriptions.len()).await;
                drop(in_flight);

                if let Err(e) = indicator_handle.wait_for_clean_screen().await {
                    error!(?e, "failed to wait for clean screen");
                }

                // Telemetry before the fail-fast return: a deadline expiry already
                // happened even if a sibling then errored, and V2 emits at cancel
                // time regardless of what siblings do afterwards.
                let expired_count = expired.iter().filter(|e| **e).count();
                if expired_count > 0 {
                    let deadline = deadline.unwrap_or_default();
                    error!(?deadline, expired_count, "subagent idle deadline expired");
                    // One event per victim (V2 stage-cancel parity), so dashboards
                    // count a multi-victim stall at its true size.
                    for _ in 0..expired_count {
                        os.telemetry
                            .send_subagent_deadline_expired(&os.database, deadline)
                            .await
                            .ok();
                    }
                }

                if let Some(e) = child_error {
                    return Err(e);
                }

                let summaries: Vec<agent::tools::summary::Summary> = completed
                    .into_iter()
                    .zip(task_descriptions)
                    .map(|(summary, task_description)| {
                        summary.unwrap_or_else(|| agent::tools::summary::Summary {
                            task_description,
                            context_summary: None,
                            task_result: format!(
                                "This subagent was cancelled: it made no observable progress for \
                                 a full idle window ({:?}, setting api.subagentTimeout / env \
                                 KIRO_SUBAGENT_STALL_TIMEOUT_MS). Sibling subagents were not \
                                 affected. Work this subagent completed before cancellation (file \
                                 edits, commands) may have partially applied, and a command it \
                                 launched may still be running; verify the state and decide \
                                 whether to retry with a smaller task.",
                                deadline.unwrap_or_default()
                            ),
                            result_type: None,
                        })
                    })
                    .collect();

                let output_serialized = serde_json::json!({ "summaries": summaries });

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
                    // Multiple subagents - display as batch with details
                    queue!(
                        output,
                        style::Print("Invoking "),
                        StyledText::brand_fg(),
                        style::Print(subagents.len()),
                        StyledText::reset(),
                        style::Print(" subagents in parallel"),
                    )?;
                    super::display_tool_use(tool, output)?;
                    for (i, subagent) in subagents.iter().enumerate() {
                        queue!(
                            output,
                            style::Print("\n  "),
                            style::Print(i + 1),
                            style::Print(". "),
                            StyledText::brand_fg(),
                            style::Print(subagent.agent_name.as_deref().unwrap_or(DEFAULT_AGENT_NAME)),
                            StyledText::reset(),
                            style::Print(": "),
                            style::Print(&subagent.query),
                        )?;
                    }
                    queue!(output, style::Print("\n"))?;
                }
            },
        }

        Ok(())
    }
}

/// One child's terminal state after racing its own idle window.
enum ChildOutcome {
    Done(agent::tools::summary::Summary),
    Failed(eyre::Report),
    /// The child's idle window elapsed with no progress; its future was
    /// dropped, terminating its session.
    DeadlineExpired,
}

/// Race a single child against its own idle window (`None` disables it). The
/// window is an idle timer on THIS child's progress alone: its events (and
/// human-wait holds) restart it, so an actively-working child — however long —
/// is never cut off, a busy sibling cannot mask this child's wedge, and this
/// child tripping never cancels a sibling.
async fn run_child_with_idle_deadline(
    child: impl Future<Output = Result<agent::tools::summary::Summary>>,
    activity: &StageActivity,
    deadline: Option<Duration>,
) -> ChildOutcome {
    tokio::pin!(child);
    // Sleep to the current window edge and re-check: a bump while we slept
    // moves the edge forward, so only a full window of silence falls through.
    let idle = async {
        let Some(window) = deadline else {
            return std::future::pending::<()>().await;
        };
        loop {
            // Saturate rather than panic if the edge is unrepresentable — an
            // overflowing expiry simply means "never expires". The resolver
            // also clamps the window, so this is defense in depth.
            let Some(expires_at) = activity.last().checked_add(window) else {
                return std::future::pending::<()>().await;
            };
            if tokio::time::Instant::now() >= expires_at {
                return;
            }
            tokio::time::sleep_until(expires_at).await;
        }
    };
    tokio::select! {
        // Prefer a completed child over the idle trip when both are ready, so a
        // photo-finish completion isn't reported as a cancellation (V2 parity).
        biased;
        res = &mut child => match res {
            Ok(summary) => ChildOutcome::Done(summary),
            Err(e) => ChildOutcome::Failed(e),
        },
        _ = idle => ChildOutcome::DeadlineExpired,
    }
}

struct StageOutcome {
    /// Per-child summary, index-aligned with the invocation order. `None` for
    /// children cancelled by their idle window (or still pending when a
    /// sibling's error ended the stage).
    completed: Vec<Option<agent::tools::summary::Summary>>,
    child_error: Option<eyre::Report>,
    /// Per-child: whether that child's own idle window cancelled it.
    expired: Vec<bool>,
}

/// Drain wrapped children until all resolve or one fails. A child error is
/// fail-fast (remaining futures are dropped), matching the previous
/// `try_join_all` semantics; per-child deadline expiries are not errors.
async fn drain_stage(
    in_flight: &mut (impl futures::Stream<Item = (usize, ChildOutcome)> + Unpin),
    child_count: usize,
) -> StageOutcome {
    use futures::StreamExt as _;

    let mut outcome = StageOutcome {
        completed: (0..child_count).map(|_| None).collect(),
        child_error: None,
        expired: vec![false; child_count],
    };
    while let Some((index, res)) = in_flight.next().await {
        match res {
            ChildOutcome::Done(summary) => outcome.completed[index] = Some(summary),
            ChildOutcome::DeadlineExpired => outcome.expired[index] = true,
            ChildOutcome::Failed(e) => {
                outcome.child_error = Some(e);
                break;
            },
        }
    }
    outcome
}

/// Bound on a cancellation summary's task description: enough to identify the
/// task, never a re-injected multi-kilobyte expanded prompt.
const TASK_DESCRIPTION_MAX_CHARS: usize = 300;

/// Original task text, bounded, for use in cancellation summaries.
fn truncate_task_description(query: &str) -> String {
    if query.chars().count() <= TASK_DESCRIPTION_MAX_CHARS {
        query.to_string()
    } else {
        let truncated: String = query.chars().take(TASK_DESCRIPTION_MAX_CHARS).collect();
        format!("{truncated}…")
    }
}

/// Idle window for a subagent stage: `KIRO_SUBAGENT_STALL_TIMEOUT_MS` (ms) wins
/// over the stored `api.subagentTimeout` setting (seconds), then the shared
/// default — the same env-over-settings precedence and shared ceiling as V2, so
/// identical launch configuration cancels identically on both engines. A
/// resolved `0` disables the deadline (`None`).
fn subagent_deadline(os: &Os) -> Option<Duration> {
    let timeout = agent::consts::env_subagent_stall_timeout().unwrap_or_else(|| {
        match os
            .database
            .settings
            .get_int(crate::database::settings::Setting::ApiSubagentTimeout)
        {
            Some(i) => u64::try_from(i).map_or_else(
                |_| {
                    // Warn instead of silently defaulting, matching the stream-idle tiers.
                    warn!(value = i, "ignoring negative api.subagentTimeout; using the default");
                    agent::consts::DEFAULT_SUBAGENT_TIMEOUT
                },
                Duration::from_secs,
            ),
            None => agent::consts::DEFAULT_SUBAGENT_TIMEOUT,
        }
    });
    let timeout = agent::consts::clamp_subagent_timeout(timeout);
    (!timeout.is_zero()).then_some(timeout)
}

/// Resolve `@prompt-name [args...]` in a subagent query using the shared prompt resolution.
/// Returns the resolved text content, or the original query unchanged on failure.
/// Note: subagents only support text today, so non-text prompt messages are skipped.
// TODO: validate prompt name against known prompts (file + MCP) like the main agent does,
//       to avoid attempting resolution on @file-references or unknown names.
// TODO: detect and warn on file vs MCP prompt name conflicts (main agent shows a warning).
async fn resolve_prompt_in_query(
    query: &str,
    os: &Os,
    tool_manager: &mut crate::cli::chat::tool_manager::ToolManager,
) -> String {
    let Some((prompt_name, arguments)) = crate::cli::chat::cli::prompts::parse_prompt_reference(query) else {
        return query.to_string();
    };

    match crate::cli::chat::cli::prompts::resolve_prompt_reference(&prompt_name, arguments, os, tool_manager).await {
        Ok(messages) => {
            let text = crate::cli::chat::cli::prompts::prompt_messages_to_text(&messages);
            if text.is_empty() { query.to_string() } else { text }
        },
        Err(e) => {
            tracing::warn!(prompt_name, ?e, "failed to resolve @prompt in subagent query");
            query.to_string()
        },
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

    fn summary(result: &str) -> agent::tools::summary::Summary {
        agent::tools::summary::Summary {
            task_description: "task".to_string(),
            context_summary: None,
            task_result: result.to_string(),
            result_type: None,
        }
    }

    /// Precedence of the stage idle window: env override (ms) over stored
    /// setting (s) over the shared default, with `0` disabling via either
    /// path — matching V2's resolution in `acp_agent.rs`. One test function so
    /// the process-env mutation cannot race a sibling test.
    #[tokio::test]
    async fn subagent_deadline_env_and_setting_precedence() {
        // Serialize with every other test that mutates process env vars: this
        // test and the clamp test below both set/remove the shared
        // SUBAGENT_STALL_TIMEOUT_ENV and would race under the parallel harness.
        let _env_lock = crate::util::paths::ENV_MUTATION_TEST_LOCK
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let mut os = Os::new().await.unwrap();
        let env_key = agent::consts::SUBAGENT_STALL_TIMEOUT_ENV;
        unsafe { std::env::remove_var(env_key) };

        // No env, no setting: the shared default.
        assert_eq!(subagent_deadline(&os), Some(agent::consts::DEFAULT_SUBAGENT_TIMEOUT));

        // Stored setting (whole seconds) applies when the env var is absent.
        os.database
            .settings
            .set(crate::database::settings::Setting::ApiSubagentTimeout, 10, None)
            .await
            .unwrap();
        assert_eq!(subagent_deadline(&os), Some(Duration::from_secs(10)));

        // Env override (ms) wins over the stored setting.
        unsafe { std::env::set_var(env_key, "500") };
        assert_eq!(subagent_deadline(&os), Some(Duration::from_millis(500)));

        // Env `0` disables the deadline even with a non-zero stored setting.
        unsafe { std::env::set_var(env_key, "0") };
        assert_eq!(subagent_deadline(&os), None);

        // A malformed env value is ignored; the stored setting applies.
        unsafe { std::env::set_var(env_key, "not-a-number") };
        assert_eq!(subagent_deadline(&os), Some(Duration::from_secs(10)));

        unsafe { std::env::remove_var(env_key) };

        // Setting `0` disables the deadline.
        os.database
            .settings
            .set(crate::database::settings::Setting::ApiSubagentTimeout, 0, None)
            .await
            .unwrap();
        assert_eq!(subagent_deadline(&os), None);

        // A negative setting is invalid: warned and replaced by the default.
        os.database
            .settings
            .set(crate::database::settings::Setting::ApiSubagentTimeout, -5, None)
            .await
            .unwrap();
        assert_eq!(subagent_deadline(&os), Some(agent::consts::DEFAULT_SUBAGENT_TIMEOUT));
    }

    /// Test helper: wrap a child future exactly as `invoke` does, racing it
    /// against its own idle window on its own activity anchor.
    fn wrap_child(
        index: usize,
        activity: Arc<StageActivity>,
        deadline: Option<Duration>,
        child: impl Future<Output = eyre::Result<agent::tools::summary::Summary>>,
    ) -> impl Future<Output = (usize, ChildOutcome)> {
        async move { (index, run_child_with_idle_deadline(child, &activity, deadline).await) }
    }

    const WINDOW: Duration = Duration::from_secs(3600);

    /// A human-blocked wait (approval prompt, OAuth grant) suspends the idle
    /// window: with a hold outstanding the child must not expire no matter how
    /// long the human takes, and release grants a fresh full window.
    #[tokio::test(start_paused = true)]
    async fn child_deadline_suspends_while_hold_outstanding() {
        let activity = Arc::new(StageActivity::default());
        let holder = activity.clone();
        let mut in_flight: futures::stream::FuturesUnordered<_> =
            vec![wrap_child(0, activity.clone(), Some(WINDOW), async move {
                // Approval prompt appears immediately; the human takes ten windows
                // to answer, then the child finishes within its fresh window.
                holder.hold();
                tokio::time::sleep(Duration::from_secs(36000)).await;
                holder.release();
                tokio::time::sleep(Duration::from_secs(1800)).await;
                Ok(summary("approved and finished"))
            })]
            .into_iter()
            .collect();

        let outcome = drain_stage(&mut in_flight, 1).await;

        assert!(!outcome.expired[0], "a held (human-blocked) child must never expire");
        assert_eq!(
            outcome.completed[0].as_ref().map(|s| s.task_result.as_str()),
            Some("approved and finished")
        );
    }

    /// Releasing the hold resumes the idle window: a full window of silence
    /// after the release still expires the child.
    #[tokio::test(start_paused = true)]
    async fn child_deadline_expires_after_hold_released() {
        let activity = Arc::new(StageActivity::default());
        let holder = activity.clone();
        let mut in_flight: futures::stream::FuturesUnordered<_> =
            vec![wrap_child(0, activity.clone(), Some(WINDOW), async move {
                holder.hold();
                tokio::time::sleep(Duration::from_secs(7200)).await;
                holder.release();
                tokio::time::sleep(Duration::from_secs(86400)).await;
                Ok(summary("never"))
            })]
            .into_iter()
            .collect();

        let outcome = drain_stage(&mut in_flight, 1).await;

        assert!(
            outcome.expired[0],
            "silence after the hold is released must still trip the deadline"
        );
        assert!(outcome.completed[0].is_none());
    }

    /// The deadline is per child: a silent child is cancelled on its own
    /// window while a finished sibling keeps its real summary.
    #[tokio::test(start_paused = true)]
    async fn child_deadline_returns_partial_results() {
        let fast_activity = Arc::new(StageActivity::default());
        let slow_activity = Arc::new(StageActivity::default());
        let mut in_flight: futures::stream::FuturesUnordered<_> = vec![
            futures::future::Either::Left(wrap_child(0, fast_activity, Some(WINDOW), async {
                Ok(summary("fast child done"))
            })),
            futures::future::Either::Right(wrap_child(1, slow_activity, Some(WINDOW), async {
                tokio::time::sleep(Duration::from_secs(7200)).await;
                Ok(summary("slow child done"))
            })),
        ]
        .into_iter()
        .collect();

        let outcome = drain_stage(&mut in_flight, 2).await;

        assert!(outcome.child_error.is_none());
        assert_eq!(
            outcome.completed[0].as_ref().map(|s| s.task_result.as_str()),
            Some("fast child done"),
            "finished child keeps its real summary"
        );
        assert!(outcome.expired[1], "the silent child expires on its own window");
        assert!(outcome.completed[1].is_none(), "cancelled child yields no summary");
        assert!(!outcome.expired[0], "the finished child is not marked expired");
    }

    /// A busy sibling neither masks a wedged child nor is cancelled as
    /// collateral: the wedged child is cancelled on ITS window while the busy
    /// sibling keeps working far past it and completes normally.
    #[tokio::test(start_paused = true)]
    async fn busy_sibling_does_not_mask_wedged_child() {
        let busy_activity = Arc::new(StageActivity::default());
        let wedged_activity = Arc::new(StageActivity::default());
        let bumper = busy_activity.clone();
        let mut in_flight: futures::stream::FuturesUnordered<_> = vec![
            futures::future::Either::Left(wrap_child(0, busy_activity, Some(WINDOW), async move {
                // Three hours of real work, pinging progress every 30 minutes.
                for _ in 0..6 {
                    tokio::time::sleep(Duration::from_secs(1800)).await;
                    bumper.bump();
                }
                Ok(summary("busy sibling done"))
            })),
            futures::future::Either::Right(wrap_child(1, wedged_activity, Some(WINDOW), async {
                // Completely silent: wedged from the start.
                tokio::time::sleep(Duration::from_secs(86400)).await;
                Ok(summary("never"))
            })),
        ]
        .into_iter()
        .collect();

        let outcome = drain_stage(&mut in_flight, 2).await;

        assert!(
            outcome.expired[1] && outcome.completed[1].is_none(),
            "the wedged child is cancelled on its own window despite the busy sibling"
        );
        assert_eq!(
            outcome.completed[0].as_ref().map(|s| s.task_result.as_str()),
            Some("busy sibling done"),
            "the busy sibling is never collateral of the wedged child's deadline"
        );
        assert!(!outcome.expired[0]);
    }

    /// Child progress restarts the idle window: a child doing hours of
    /// legitimate work while reporting activity is never cancelled, even
    /// though its total runtime far exceeds the window.
    #[tokio::test(start_paused = true)]
    async fn child_deadline_resets_on_child_progress() {
        let activity = Arc::new(StageActivity::default());
        let bumper = activity.clone();
        let mut in_flight: futures::stream::FuturesUnordered<_> =
            vec![wrap_child(0, activity.clone(), Some(WINDOW), async move {
                // Three hours of work, pinging progress every 30 minutes.
                for _ in 0..6 {
                    tokio::time::sleep(Duration::from_secs(1800)).await;
                    bumper.bump();
                }
                Ok(summary("slow but active child done"))
            })]
            .into_iter()
            .collect();

        let outcome = drain_stage(&mut in_flight, 1).await;

        assert!(!outcome.expired[0], "an actively-working child must never be cut off");
        assert_eq!(
            outcome.completed[0].as_ref().map(|s| s.task_result.as_str()),
            Some("slow but active child done")
        );
    }

    /// Once progress stops, the child expires one full window after the last bump.
    #[tokio::test(start_paused = true)]
    async fn child_deadline_expires_after_progress_stops() {
        let activity = Arc::new(StageActivity::default());
        let bumper = activity.clone();
        let mut in_flight: futures::stream::FuturesUnordered<_> =
            vec![wrap_child(0, activity.clone(), Some(WINDOW), async move {
                // Progress twice, then go silent for good.
                for _ in 0..2 {
                    tokio::time::sleep(Duration::from_secs(1800)).await;
                    bumper.bump();
                }
                tokio::time::sleep(Duration::from_secs(86400)).await;
                Ok(summary("never"))
            })]
            .into_iter()
            .collect();

        let outcome = drain_stage(&mut in_flight, 1).await;

        assert!(outcome.expired[0], "a full window of silence must trip the deadline");
        assert!(outcome.completed[0].is_none());
    }

    /// With the deadline disabled (setting 0), the stage waits for all children.
    #[tokio::test(start_paused = true)]
    async fn stage_without_deadline_waits_for_all_children() {
        let mut in_flight: futures::stream::FuturesUnordered<_> = vec![
            futures::future::Either::Left(wrap_child(0, Arc::new(StageActivity::default()), None, async {
                Ok(summary("first"))
            })),
            futures::future::Either::Right(wrap_child(1, Arc::new(StageActivity::default()), None, async {
                tokio::time::sleep(Duration::from_secs(7200)).await;
                Ok(summary("second"))
            })),
        ]
        .into_iter()
        .collect();

        let outcome = drain_stage(&mut in_flight, 2).await;

        assert!(!outcome.expired.iter().any(|e| *e));
        assert!(outcome.completed.iter().all(Option::is_some));
    }

    /// Children finishing before the deadline end the stage normally.
    #[tokio::test(start_paused = true)]
    async fn stage_completes_before_deadline() {
        let mut in_flight: futures::stream::FuturesUnordered<_> =
            vec![wrap_child(0, Arc::new(StageActivity::default()), Some(WINDOW), async {
                Ok(summary("done"))
            })]
            .into_iter()
            .collect();

        let outcome = drain_stage(&mut in_flight, 1).await;

        assert!(!outcome.expired[0]);
        assert_eq!(
            outcome.completed[0].as_ref().map(|s| s.task_result.as_str()),
            Some("done")
        );
    }

    /// An oversized window (setting or env) is clamped so deadline arithmetic
    /// can never overflow and panic mid-stage; a bounded generous value passes
    /// through untouched.
    #[tokio::test]
    async fn subagent_deadline_clamps_oversized_windows() {
        // Shares SUBAGENT_STALL_TIMEOUT_ENV with the precedence test above.
        let _env_lock = crate::util::paths::ENV_MUTATION_TEST_LOCK
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let mut os = Os::new().await.unwrap();
        let env_key = agent::consts::SUBAGENT_STALL_TIMEOUT_ENV;
        unsafe { std::env::remove_var(env_key) };

        os.database
            .settings
            .set(crate::database::settings::Setting::ApiSubagentTimeout, i64::MAX, None)
            .await
            .unwrap();
        assert_eq!(subagent_deadline(&os), Some(agent::consts::MAX_SUBAGENT_TIMEOUT));

        // Largest accepted value passes through unclamped.
        os.database
            .settings
            .set(
                crate::database::settings::Setting::ApiSubagentTimeout,
                agent::consts::MAX_SUBAGENT_TIMEOUT.as_secs() as i64,
                None,
            )
            .await
            .unwrap();
        assert_eq!(subagent_deadline(&os), Some(agent::consts::MAX_SUBAGENT_TIMEOUT));

        // The env path clamps too.
        unsafe { std::env::set_var(env_key, u64::MAX.to_string()) };
        assert_eq!(subagent_deadline(&os), Some(agent::consts::MAX_SUBAGENT_TIMEOUT));
        unsafe { std::env::remove_var(env_key) };
    }

    /// Cancellation summaries carry a bounded task description, never a
    /// re-injected multi-kilobyte expanded prompt.
    #[test]
    fn task_description_is_bounded() {
        let short = "fix the flaky test";
        assert_eq!(truncate_task_description(short), short);

        let long = "x".repeat(10_000);
        let truncated = truncate_task_description(&long);
        assert!(truncated.chars().count() <= TASK_DESCRIPTION_MAX_CHARS + 1);
        assert!(truncated.ends_with('…'));
    }

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
        assert!(matches!(result, PermissionEvalResult::Ask { .. }));
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
        assert!(matches!(result, PermissionEvalResult::Ask { .. }));
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
        assert!(matches!(result, PermissionEvalResult::Ask { .. }));
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
        assert!(matches!(result, PermissionEvalResult::Ask { .. }));
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
        assert!(matches!(result, PermissionEvalResult::Ask { .. }));
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
        assert!(matches!(result, PermissionEvalResult::Ask { .. }));
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
        assert!(matches!(result, PermissionEvalResult::Ask { .. }));
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
        assert_eq!(tool3.eval_perm(&os, &agent), PermissionEvalResult::ask());
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

    /// Regression test: when toolsSettings exist and the tool has been runtime-trusted
    /// (added to allowed_tools via user typing 't'), InvokeSubagents should return Allow
    /// even if the agent is not in trustedAgents.
    #[tokio::test]
    async fn test_eval_perm_invoke_subagents_runtime_trust_with_settings() {
        let os = Os::new().await.unwrap();
        let mut settings = HashMap::new();
        settings.insert(
            "subagent",
            json!({
                "trustedAgents": ["other-agent"]
            }),
        );
        // Simulate user having typed 't' — tool added to allowed_tools
        let agent = create_test_agent(vec!["use_subagent"], settings);
        let tool = UseSubagent::InvokeSubagents {
            subagents: vec![InvokeSubagent {
                query: "test query".to_string(),
                agent_name: Some("untrusted-agent".to_string()),
                relevant_context: None,
                dangerously_trust_all_tools: false,
                is_interactive: false,
            }],
            convo_id: None,
            tool_use_id: None,
        };

        let result = tool.eval_perm(&os, &agent);
        // Should be Allow because the tool was runtime-trusted via allowlist
        assert_eq!(result, PermissionEvalResult::Allow);
    }

    /// Regression test: runtime trust via allowlist should still respect availableAgents deny.
    #[tokio::test]
    async fn test_eval_perm_invoke_subagents_runtime_trust_still_respects_available_agents() {
        let os = Os::new().await.unwrap();
        let mut settings = HashMap::new();
        settings.insert(
            "subagent",
            json!({
                "availableAgents": ["agent1"],
                "trustedAgents": []
            }),
        );
        // Tool is in allowlist (runtime trusted), but agent is not in availableAgents
        let agent = create_test_agent(vec!["use_subagent"], settings);
        let tool = UseSubagent::InvokeSubagents {
            subagents: vec![InvokeSubagent {
                query: "test query".to_string(),
                agent_name: Some("blocked-agent".to_string()),
                relevant_context: None,
                dangerously_trust_all_tools: false,
                is_interactive: false,
            }],
            convo_id: None,
            tool_use_id: None,
        };

        let result = tool.eval_perm(&os, &agent);
        // availableAgents restriction should still be enforced even with runtime trust
        assert!(matches!(result, PermissionEvalResult::Deny(_)));
    }

    /// Regression test: runtime trust with the "subagent" alias should also work
    /// when toolsSettings exist.
    #[tokio::test]
    async fn test_eval_perm_invoke_subagents_runtime_trust_via_alias_with_settings() {
        let os = Os::new().await.unwrap();
        let mut settings = HashMap::new();
        settings.insert(
            "subagent",
            json!({
                "trustedAgents": ["other-agent"]
            }),
        );
        // Trust via the "subagent" alias instead of "use_subagent"
        let agent = create_test_agent(vec!["subagent"], settings);
        let tool = UseSubagent::InvokeSubagents {
            subagents: vec![InvokeSubagent {
                query: "test query".to_string(),
                agent_name: Some("untrusted-agent".to_string()),
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
        assert!(matches!(result, PermissionEvalResult::Ask { .. }));
    }
}
