use std::time::Duration;

use chrono::{
    DateTime,
    Utc,
};
use rand::RngExt as _;
use rand::distr::Alphanumeric;
use serde::{
    Deserialize,
    Serialize,
};
use typeshare::typeshare;
use uuid::Uuid;

use super::agent_loop::protocol::{
    SendRequestArgs,
    UserTurnMetadata,
};
use super::agent_loop::types::Message;
use super::consts::DEFAULT_AGENT_NAME;
use super::event_log::{
    EventLog,
    LogEntry,
};
use super::permissions::RuntimePermissions;
use crate::agent::ExecutionState;
use crate::agent::agent_config::LoadedAgentConfig;
use crate::agent::tools::ToolState;

/// A point-in-time snapshot of an agent's state.
///
/// This includes all serializable state associated with an executing agent, for example:
///
/// * The agent config
/// * Conversation history
/// * State of execution (ie, is the agent idle, executing hooks, receiving a response from the
///   model, etc.)
/// * Agent settings
///
/// and so on.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[typeshare]
pub struct AgentSnapshot {
    /// Agent id
    pub id: AgentId,
    /// Agent config
    #[typeshare(skip)]
    #[serde(default)]
    pub agent_config: LoadedAgentConfig,
    /// Agent conversation state
    pub conversation_state: ConversationState,
    /// Agent conversation metadata
    #[typeshare(skip)]
    pub conversation_metadata: ConversationMetadata,
    /// Agent execution state
    #[typeshare(skip)]
    pub execution_state: ExecutionState,
    /// State associated with the model implementation used by the agent
    pub model_state: Option<serde_json::Value>,
    /// Persistent state required by tools during the conversation
    #[typeshare(skip)]
    pub tool_state: ToolState,
    /// Agent settings
    pub settings: AgentSettings,
    /// Runtime permissions accumulated during the session
    #[typeshare(skip)]
    #[serde(default)]
    pub permissions: RuntimePermissions,
    /// Cached tool specifications (for context size calculation)
    #[typeshare(skip)]
    #[serde(default)]
    pub tool_specs: Vec<super::agent_loop::types::ToolSpec>,
    /// Maps each model-visible tool name to `built-in`, `mcp:<server>`, or `agent:<name>`.
    #[typeshare(skip)]
    #[serde(skip)]
    pub tool_spec_sources: std::collections::BTreeMap<String, String>,
    /// Paths added via /context add during this session
    #[typeshare(skip)]
    #[serde(default)]
    pub session_resource_paths: std::collections::HashSet<String>,
    /// Whether a knowledge provider is configured
    #[typeshare(skip)]
    #[serde(default)]
    pub has_knowledge_provider: bool,
}

impl AgentSnapshot {
    pub fn new_empty(agent_config: LoadedAgentConfig) -> Self {
        Self {
            id: agent_config.name().into(),
            agent_config,
            conversation_state: ConversationState::new(Uuid::new_v4(), Vec::new()),
            conversation_metadata: Default::default(),
            execution_state: Default::default(),
            model_state: Default::default(),
            tool_state: Default::default(),
            settings: Default::default(),
            permissions: Default::default(),
            tool_specs: Default::default(),
            tool_spec_sources: Default::default(),
            session_resource_paths: Default::default(),
            has_knowledge_provider: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompactionSnapshot {
    conversation_state: ConversationState,
    summary: ConversationSummary,
}

/// Represents a summary of a conversation history.
///
/// Generally created by the model to replace a history of messages with a succinct summarization.
/// Summarizations are done to save tokens by capturing the most important bits of context while
/// removing unnecessary information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationSummary {
    /// Identifier for the summary
    pub id: Uuid,
    /// Conversation summary content
    pub content: String,
    /// The conversation that was summarized
    pub summarized_state: ConversationState,
    /// Timestamp for when the summary was generated
    #[serde(with = "chrono::serde::ts_seconds_option")]
    pub timestamp: Option<DateTime<Utc>>,
}

impl ConversationSummary {
    pub fn new(content: String, summarized_state: ConversationState, timestamp: Option<DateTime<Utc>>) -> Self {
        Self {
            id: Uuid::new_v4(),
            content,
            summarized_state,
            timestamp,
        }
    }
}

impl AsRef<str> for ConversationSummary {
    fn as_ref(&self) -> &str {
        &self.content
    }
}

/// Arguments forwarded from the TUI to the ACP backend process.
#[typeshare]
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AcpSpawnArgs {
    /// Name of the agent to use when starting the first session.
    pub agent: Option<String>,
    /// Model ID to use when starting the first session.
    pub model: Option<String>,
    /// Auto-approve all tool permission requests.
    #[serde(default)]
    pub trust_all_tools: bool,
    /// Trust only this set of tools (comma-separated names from CLI).
    #[serde(default)]
    pub trust_tools: Option<Vec<String>>,
    /// Agent engine to use ("rust" or "kas").
    pub agent_engine: Option<String>,
    /// Initial effort level to set (e.g. "low", "medium", "high").
    /// Silently ignored if the resolved model does not support effort.
    pub effort: Option<String>,
}

/// Settings to modify the runtime behavior of the agent.
#[typeshare]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentSettings {
    /// Timeout waiting for MCP servers to initialize during agent initialization.
    pub mcp_init_timeout: Duration,
    /// Disable automatic compaction when context window overflows.
    #[serde(default)]
    pub disable_auto_compact: bool,
    /// When true, all tool permission checks are bypassed (auto-approve everything).
    #[serde(default)]
    pub trust_all_tools: bool,
    /// When false, web_search and web_fetch tools are excluded (governance disabled them).
    #[serde(default = "default_true")]
    pub web_tools_enabled: bool,
    /// When false, MCP servers (user-configured and registry-based) are excluded from the
    /// agent — mirrors the Kiro console `MCP` toggle (governance) for enterprise / API-key users.
    #[serde(default = "default_true")]
    pub mcp_enabled: bool,
    /// Hold session startup until an authorization-pending MCP server has finished
    /// authorizing, rather than starting without its tools. Delegated turns have
    /// nobody to re-prompt once they are under way, so they wait; interactive
    /// sessions proceed and pick the server up when it arrives.
    #[typeshare(skip)]
    #[serde(default)]
    pub mcp_wait_for_authorization: bool,
    /// When true, MCP tools are hidden until activated via search_tools.
    #[serde(default)]
    pub tool_search_enabled: bool,
    /// MCP server names that must always have their tools immediately available
    /// (bypass tool_search deferral). Set via ASBX_KIRO_MANDATORY_MCPS env var.
    #[serde(default)]
    pub mandatory_mcp_names: Vec<String>,
    /// Only activate tool search when MCP tool specs exceed this percentage of context window.
    #[typeshare(skip)]
    #[serde(default = "default_tool_search_min_pct")]
    pub tool_search_min_pct: Option<f64>,
    /// Only activate tool search when MCP tool specs exceed this many tokens.
    #[typeshare(skip)]
    #[serde(default = "default_tool_search_min_tokens")]
    pub tool_search_min_tokens: Option<u64>,
    /// Inter-event stream silence after which a stall warning is emitted while the
    /// response stream stays open. Zero disables the warning.
    #[typeshare(skip)]
    #[serde(default = "default_stream_idle_soft_timeout")]
    pub stream_idle_soft_timeout: Duration,
    /// Inter-event stream silence after which the response stream is cancelled and
    /// surfaced as a stream timeout. Zero disables the watchdog.
    #[typeshare(skip)]
    #[serde(default = "default_stream_idle_hard_timeout")]
    pub stream_idle_hard_timeout: Duration,
    /// Idle window on child progress for a blocking subagent/crew stage. Resets on
    /// genuine child activity; on a full window of inactivity the parent stops
    /// waiting, cancels unfinished children, and continues with partial results.
    /// Zero disables the timer.
    #[typeshare(skip)]
    #[serde(default = "default_subagent_timeout")]
    pub subagent_timeout: Duration,
}

impl AgentSettings {
    const DEFAULT_MCP_INIT_TIMEOUT: Duration = Duration::from_secs(5);
    /// Ceiling for the stream-idle tiers. The settings are read as seconds while
    /// their `api.timeout` sibling is milliseconds, so a value carried across in
    /// the wrong unit (e.g. `300000`) would otherwise silently disable the
    /// watchdog for days; no legitimate idle threshold exceeds an hour.
    pub const STREAM_IDLE_TIMEOUT_CEILING: Duration = Duration::from_secs(3600);

    /// Clamps one stream-idle tier to [`Self::STREAM_IDLE_TIMEOUT_CEILING`], warning
    /// when the configured value exceeds it. Shared with read sites that resolve the
    /// setting outside an `AgentSettings` (V1's chat loop), so the documented ceiling
    /// holds everywhere the setting is honored.
    pub fn clamp_stream_idle_timeout(tier: &str, value: Duration) -> Duration {
        if value > Self::STREAM_IDLE_TIMEOUT_CEILING {
            tracing::warn!(
                tier,
                configured = ?value,
                ceiling = ?Self::STREAM_IDLE_TIMEOUT_CEILING,
                "configured stream-idle timeout exceeds the ceiling (values are seconds, not milliseconds); clamping"
            );
            return Self::STREAM_IDLE_TIMEOUT_CEILING;
        }
        value
    }

    /// Reconciles the stream-idle watchdog tiers once, after user settings are applied.
    ///
    /// Values above [`Self::STREAM_IDLE_TIMEOUT_CEILING`] are clamped to it (with a
    /// logged warning) before the tier-ordering check runs.
    ///
    /// A soft (warning) threshold at or past the hard (cancel) threshold can never fire.
    /// When the soft value was explicitly configured (`soft_explicit`), that is a user
    /// misconfiguration: warn once and disable the warning tier, preserving the
    /// documented "0 disables a tier" semantics. When only the hard cap was tightened
    /// and the colliding soft value is the inherited default, the user never chose it —
    /// keep the warning tier by clamping it to half the hard bound instead.
    pub fn reconcile_stream_idle_tiers(&mut self, soft_explicit: bool) {
        for (name, tier) in [
            ("soft", &mut self.stream_idle_soft_timeout),
            ("hard", &mut self.stream_idle_hard_timeout),
        ] {
            *tier = Self::clamp_stream_idle_timeout(name, *tier);
        }
        let soft = self.stream_idle_soft_timeout;
        let hard = self.stream_idle_hard_timeout;
        if soft.is_zero() || hard.is_zero() || soft < hard {
            return;
        }
        if soft_explicit {
            tracing::warn!(
                ?soft,
                ?hard,
                "configured stream-idle soft timeout is >= the hard timeout; disabling the soft warning tier"
            );
            self.stream_idle_soft_timeout = Duration::ZERO;
        } else {
            self.stream_idle_soft_timeout = hard / 2;
        }
    }
}

/// Reads a stream-idle timeout setting (whole seconds) from its raw settings value.
///
/// A value that is present but unusable — negative, fractional, or not a number —
/// warns and is treated as absent, so the tier keeps its inherited default. The
/// out-of-range and tier-collision cases already warn in
/// [`AgentSettings::reconcile_stream_idle_tiers`]; this closes the one
/// invalid-input path that previously failed silently.
pub fn stream_idle_setting_secs(raw: Option<&serde_json::Value>, setting_name: &str) -> Option<u64> {
    let value = raw?;
    match value.as_i64().and_then(|i| u64::try_from(i).ok()) {
        Some(secs) => Some(secs),
        None => {
            tracing::warn!(
                setting = setting_name,
                value = %value,
                "invalid stream-idle timeout (expected a non-negative whole number of seconds); using the default"
            );
            None
        },
    }
}

fn default_true() -> bool {
    true
}

fn default_tool_search_min_pct() -> Option<f64> {
    Some(5.0)
}

fn default_tool_search_min_tokens() -> Option<u64> {
    Some(50_000)
}

fn default_stream_idle_soft_timeout() -> Duration {
    super::consts::DEFAULT_STREAM_IDLE_SOFT_TIMEOUT
}

fn default_stream_idle_hard_timeout() -> Duration {
    super::consts::DEFAULT_STREAM_IDLE_HARD_TIMEOUT
}

fn default_subagent_timeout() -> Duration {
    // Env-overridable (0 disables): an idle window on child progress for a blocking
    // subagent stage, reset whenever a child reports fresh activity.
    super::consts::subagent_stall_timeout()
}

impl Default for AgentSettings {
    fn default() -> Self {
        Self {
            mcp_init_timeout: Self::DEFAULT_MCP_INIT_TIMEOUT,
            disable_auto_compact: false,
            trust_all_tools: false,
            web_tools_enabled: true,
            mcp_enabled: true,
            mcp_wait_for_authorization: false,
            tool_search_enabled: false,
            mandatory_mcp_names: Vec::new(),
            tool_search_min_pct: default_tool_search_min_pct(),
            tool_search_min_tokens: default_tool_search_min_tokens(),
            stream_idle_soft_timeout: default_stream_idle_soft_timeout(),
            stream_idle_hard_timeout: default_stream_idle_hard_timeout(),
            subagent_timeout: default_subagent_timeout(),
        }
    }
}

/// State associated with a history of messages.
#[typeshare]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationState {
    pub id: Uuid,
    #[serde(default)]
    #[typeshare(skip)]
    event_log: EventLog,
    #[serde(skip)]
    messages_cache: Option<Vec<Message>>,
}

impl ConversationState {
    pub fn new(id: Uuid, entries: Vec<LogEntry>) -> Self {
        let event_log = EventLog::new(entries);
        let messages = event_log.derive_messages();
        Self {
            id,
            event_log,
            messages_cache: Some(messages),
        }
    }

    pub fn messages(&mut self) -> &[Message] {
        if self.messages_cache.is_none() {
            self.messages_cache = Some(self.event_log.derive_messages());
        }
        self.messages_cache.as_ref().unwrap()
    }

    pub fn cached_messages(&self) -> Option<&[Message]> {
        self.messages_cache.as_deref()
    }

    pub fn event_log(&self) -> &EventLog {
        &self.event_log
    }

    /// Append a log entry and return its index. Updates the messages cache.
    pub fn append_log(&mut self, entry: LogEntry) -> usize {
        let messages = self.messages_cache.get_or_insert_with(Vec::new);
        entry.apply(messages, &self.event_log);
        self.event_log.append(entry);
        self.event_log.len() - 1
    }
}

// TODO: Remove Default implementation - ConversationState should require explicit id
impl Default for ConversationState {
    fn default() -> Self {
        Self::new(Uuid::new_v4(), Vec::new())
    }
}

/// A context usage percentage paired with the model that reported it.
///
/// The percentage is relative to the reporting model's context window, so it is only
/// meaningful while that same model is in use. Switching models invalidates it: 100%
/// of a 272K window is roughly 27% of a 1M one.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LastContextUsage {
    pub percentage: f32,
    /// Model that produced this reading, from [`UserTurnMetadata::model`].
    pub model_id: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ConversationMetadata {
    /// History of user turns
    pub user_turn_metadatas: Vec<UserTurnMetadata>,
    /// Most recent context usage reported by the backend, or `None` if unknown.
    ///
    /// This is the operational value used to decide whether to synthesize a context
    /// overflow before dispatching a request. It is deliberately a single current
    /// reading rather than a search over [`Self::user_turn_metadatas`], which is
    /// append-only telemetry history and retains readings from before a compaction.
    ///
    /// Overwritten at the end of every turn with whatever that turn reported,
    /// including overwriting with `None` when the turn reported nothing, and cleared
    /// when compaction shrinks the history. Best effort: when it is unset, no
    /// overflow is synthesized and the request dispatches normally.
    #[serde(default)]
    pub last_context_usage: Option<LastContextUsage>,
    /// The request that started the most recent user turn
    pub user_turn_start_request: Option<SendRequestArgs>,
    /// The most recent request sent
    ///
    /// This is equivalent to user_turn_start_request for the first request of a user turn
    pub last_request: Option<SendRequestArgs>,
}

/// Unique identifier of an agent instance within a session.
///
/// Formatted as: `parent_id/name#rand`
#[typeshare]
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct AgentId {
    /// Name of the agent
    ///
    /// This is the same as the agent name in the agent's config
    name: String,
    /// String-formatted id of the agent's parent, if available.
    ///
    /// If available, this would be the result of [AgentId::to_string].
    parent_id: Option<String>,
    /// Random suffix
    rand: Option<String>,
}

impl AgentId {
    // '/', '#', and '|' are not valid characters for an agent name, hence using these as separators.

    const AGENT_ID_SUFFIX: char = '|';
    const RAND_PART_SEPARATOR: char = '#';

    pub fn new(name: String) -> Self {
        Self {
            name,
            parent_id: None,
            rand: Some(rand::rng().sample_iter(&Alphanumeric).take(5).map(char::from).collect()),
        }
    }

    /// Name of the agent, as written in the agent config
    pub fn name(&self) -> &str {
        &self.name
    }
}

impl Default for AgentId {
    fn default() -> Self {
        Self {
            name: DEFAULT_AGENT_NAME.to_string(),
            parent_id: Default::default(),
            rand: Default::default(),
        }
    }
}

impl std::fmt::Display for AgentId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if let Some(parent) = self.parent_id.as_ref() {
            write!(f, "{parent}|")?;
        }
        write!(f, "{}", self.name)?;
        if let Some(id) = self.rand.as_ref() {
            write!(f, "#{id}")?;
        }
        Ok(())
    }
}

impl<T> From<T> for AgentId
where
    T: AsRef<str>,
{
    fn from(value: T) -> Self {
        let s = value.as_ref();

        let mut parent_part = None;
        let mut rand_part = None;
        if let Some((i, _)) = s.rmatch_indices(Self::AGENT_ID_SUFFIX).next() {
            parent_part = Some((i, s.split_at(i).0.to_string()));
        }
        match (&parent_part, s.rmatch_indices(Self::RAND_PART_SEPARATOR).next()) {
            (Some((i, _)), Some((j, _))) if j > *i => rand_part = Some((j, s.split_at(j + 1).1.to_string())),
            (None, Some((j, _))) => rand_part = Some((j, s.split_at(j + 1).1.to_string())),
            _ => (),
        }
        let name = match (&parent_part, &rand_part) {
            (None, None) => s.split_once(Self::AGENT_ID_SUFFIX).unwrap_or((s, "")).0.to_string(),
            (None, Some((i, _))) => s.split_at(*i).0.to_string(),
            (Some((i, _)), None) => s.split_at(*i + 1).1.to_string(),
            (Some((i, _)), Some((j, _))) => s
                .split_at(*i + 1)
                .1
                .split_at(j.saturating_sub(*i).saturating_sub(1))
                .0
                .to_string(),
        };
        Self {
            name,
            parent_id: parent_part.map(|v| v.1),
            rand: rand_part.map(|v| v.1),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settings_with_tiers(soft: u64, hard: u64) -> AgentSettings {
        AgentSettings {
            stream_idle_soft_timeout: Duration::from_secs(soft),
            stream_idle_hard_timeout: Duration::from_secs(hard),
            ..Default::default()
        }
    }

    #[test]
    fn reconcile_keeps_valid_and_disabled_tiers_untouched() {
        for (soft, hard) in [(60, 300), (0, 300), (60, 0), (0, 0)] {
            for explicit in [false, true] {
                let mut s = settings_with_tiers(soft, hard);
                s.reconcile_stream_idle_tiers(explicit);
                assert_eq!(s.stream_idle_soft_timeout, Duration::from_secs(soft));
                assert_eq!(s.stream_idle_hard_timeout, Duration::from_secs(hard));
            }
        }
    }

    #[test]
    fn reconcile_disables_explicitly_misconfigured_soft_tier() {
        let mut s = settings_with_tiers(300, 60);
        s.reconcile_stream_idle_tiers(true);
        assert_eq!(s.stream_idle_soft_timeout, Duration::ZERO);
        assert_eq!(s.stream_idle_hard_timeout, Duration::from_secs(60));
    }

    /// A tightened hard cap colliding with the *inherited default* soft keeps the
    /// warning tier (clamped below the hard bound) instead of silently losing it.
    #[test]
    fn reconcile_clamps_inherited_default_soft_below_hard() {
        let mut s = settings_with_tiers(60, 30);
        s.reconcile_stream_idle_tiers(false);
        assert_eq!(s.stream_idle_soft_timeout, Duration::from_secs(15));
        assert_eq!(s.stream_idle_hard_timeout, Duration::from_secs(30));
    }

    /// A milliseconds-shaped value (the `api.timeout` unit carried onto the
    /// seconds-based idle keys) is clamped to the ceiling instead of silently
    /// disabling the watchdog for days.
    #[test]
    fn reconcile_clamps_values_above_the_ceiling() {
        let mut s = settings_with_tiers(60, 300_000);
        s.reconcile_stream_idle_tiers(false);
        assert_eq!(s.stream_idle_soft_timeout, Duration::from_secs(60));
        assert_eq!(s.stream_idle_hard_timeout, AgentSettings::STREAM_IDLE_TIMEOUT_CEILING);

        // Both tiers oversized: after clamping they collide at the ceiling, and
        // the ordering rules then apply to the clamped values.
        let mut s = settings_with_tiers(600_000, 300_000);
        s.reconcile_stream_idle_tiers(true);
        assert_eq!(s.stream_idle_soft_timeout, Duration::ZERO);
        assert_eq!(s.stream_idle_hard_timeout, AgentSettings::STREAM_IDLE_TIMEOUT_CEILING);
    }

    /// Present-but-unusable settings values (negative, fractional, non-numeric)
    /// fall back to the inherited default instead of parsing; usable whole
    /// seconds — including an explicit 0 (tier disabled) — pass through.
    #[test]
    fn stream_idle_setting_secs_accepts_only_whole_non_negative_seconds() {
        use serde_json::json;
        assert_eq!(stream_idle_setting_secs(None, "t"), None);
        assert_eq!(stream_idle_setting_secs(Some(&json!(60)), "t"), Some(60));
        assert_eq!(stream_idle_setting_secs(Some(&json!(0)), "t"), Some(0));
        assert_eq!(stream_idle_setting_secs(Some(&json!(-5)), "t"), None);
        assert_eq!(stream_idle_setting_secs(Some(&json!(1.5)), "t"), None);
        assert_eq!(stream_idle_setting_secs(Some(&json!("60")), "t"), None);
    }

    #[test]
    fn test_agent_id_parse() {
        macro_rules! assert_agent_id {
            ($val:expr, $s:expr) => {
                assert_eq!($val.to_string(), $s);
                assert_eq!($val, $s.into());
            };
        }

        // Testing as expected in the app
        let parent = AgentId {
            name: "parent".to_string(),
            parent_id: None,
            rand: None,
        };
        assert_agent_id!(parent, "parent");
        let child = AgentId {
            name: "child".to_string(),
            parent_id: Some(parent.to_string()),
            rand: Some("123".to_string()),
        };
        assert_agent_id!(child, "parent|child#123");
        let grandchild = AgentId {
            name: "grandchild".to_string(),
            parent_id: Some(child.to_string()),
            rand: Some("456".to_string()),
        };
        assert_agent_id!(grandchild, "parent|child#123|grandchild#456");

        // Testing edge cases
        let a1 = AgentId {
            name: "a1".to_string(),
            parent_id: None,
            rand: Some("rand".to_string()),
        };
        assert_agent_id!(a1, "a1#rand");
        let a2 = AgentId {
            name: "a2".to_string(),
            parent_id: Some(a1.to_string()),
            rand: None,
        };
        assert_agent_id!(a2, "a1#rand|a2");
        let a3 = AgentId {
            name: "a3".to_string(),
            parent_id: Some(a2.to_string()),
            rand: None,
        };
        assert_agent_id!(a3, "a1#rand|a2|a3");
    }

    #[test]
    fn test_agent_id_new() {
        let id = AgentId::new("test".to_string());
        assert_eq!(id.name(), "test");
        assert!(id.rand.is_some());
        assert!(id.parent_id.is_none());
    }

    #[test]
    fn test_agent_id_default() {
        let id = AgentId::default();
        assert!(!id.name().is_empty());
    }

    #[test]
    fn test_agent_id_clone_and_eq() {
        let id1 = AgentId::new("test".to_string());
        let id2 = id1.clone();
        assert_eq!(id1, id2);
    }

    #[test]
    fn test_agent_settings_default() {
        let s = AgentSettings::default();
        assert_eq!(s.mcp_init_timeout, Duration::from_secs(5));
        assert!(!s.disable_auto_compact);
        assert!(!s.trust_all_tools);
    }

    #[test]
    fn test_agent_settings_serde() {
        let s = AgentSettings {
            mcp_init_timeout: Duration::from_secs(10),
            disable_auto_compact: true,
            trust_all_tools: true,
            ..AgentSettings::default()
        };
        let json = serde_json::to_string(&s).unwrap();
        let parsed: AgentSettings = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.mcp_init_timeout, s.mcp_init_timeout);
        assert_eq!(parsed.disable_auto_compact, s.disable_auto_compact);
        assert_eq!(parsed.trust_all_tools, s.trust_all_tools);
    }

    #[test]
    fn test_acp_spawn_args_default() {
        let a = AcpSpawnArgs::default();
        assert!(a.agent.is_none());
        assert!(a.model.is_none());
        assert!(!a.trust_all_tools);
        assert!(a.trust_tools.is_none());
    }

    #[test]
    fn test_conversation_state_new() {
        let id = Uuid::new_v4();
        let s = ConversationState::new(id, vec![]);
        assert_eq!(s.id, id);
        assert!(s.cached_messages().unwrap().is_empty());
    }

    #[test]
    fn test_conversation_state_default() {
        let s = ConversationState::default();
        // Should have a fresh id and empty cache
        assert!(s.cached_messages().unwrap().is_empty());
    }

    #[test]
    fn test_conversation_state_messages() {
        let mut s = ConversationState::new(Uuid::new_v4(), vec![]);
        let msgs = s.messages();
        assert!(msgs.is_empty());
    }

    #[test]
    fn test_conversation_state_cached_messages() {
        let s = ConversationState::new(Uuid::new_v4(), vec![]);
        let cached = s.cached_messages();
        assert!(cached.is_some());
        assert!(cached.unwrap().is_empty());
    }

    #[test]
    fn test_conversation_summary_new() {
        let state = ConversationState::new(Uuid::new_v4(), vec![]);
        let now = Some(Utc::now());
        let summary = ConversationSummary::new("summary text".to_string(), state, now);
        assert_eq!(summary.content, "summary text");
        assert!(summary.timestamp.is_some());
    }

    #[test]
    fn test_conversation_summary_as_ref() {
        let state = ConversationState::new(Uuid::new_v4(), vec![]);
        let summary = ConversationSummary::new("text content".to_string(), state, None);
        let s: &str = summary.as_ref();
        assert_eq!(s, "text content");
    }

    #[test]
    fn test_agent_snapshot_default() {
        let s = AgentSnapshot::default();
        assert!(s.tool_specs.is_empty());
        assert!(s.tool_spec_sources.is_empty());
        assert!(s.session_resource_paths.is_empty());
    }

    #[test]
    fn test_agent_snapshot_new_empty() {
        let cfg = LoadedAgentConfig::default();
        let s = AgentSnapshot::new_empty(cfg);
        assert!(s.tool_specs.is_empty());
        assert!(s.tool_spec_sources.is_empty());
    }

    #[test]
    fn test_conversation_state_event_log() {
        let s = ConversationState::new(Uuid::new_v4(), vec![]);
        let log = s.event_log();
        assert_eq!(log.len(), 0);
    }
}
