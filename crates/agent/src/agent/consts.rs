pub const DEFAULT_AGENT_NAME: &str = "kiro_default";
pub const PLANNER_AGENT_NAME: &str = "kiro_planner";

/// Resources always included for the default agent.
///
/// These are static workspace-relative entries. The global
/// `$KIRO_HOME/skills/*/SKILL.md` pattern is added dynamically by
/// `build_default_agent` so it honors the `KIRO_HOME` override.
pub const DEFAULT_AGENT_RESOURCES: &[&str] = &[
    "file://AGENTS.md",
    "file://README.md",
    "skill://.kiro/skills/*/SKILL.md",
];

pub const DUMMY_TOOL_NAME: &str = "dummy";

/// Instructional tool_result returned when the model invokes the placeholder
/// [`DUMMY_TOOL_NAME`] tool.
///
/// `enforce_conversation_invariants` advertises the `dummy` tool whenever
/// history references a tool the current agent can't dispatch (e.g. an executor
/// tool left in shared history after a `kiro_planner` plan/execute handoff).
/// Because `dummy` is never registered in the tool map, the model calling it
/// used to hard-fail with `NameDoesNotExist`, which drove a tight
/// unavailable-tool retry loop. Instead we hand back this guidance so the model
/// can self-correct.
pub const DUMMY_TOOL_RESULT_MESSAGE: &str = "The 'dummy' tool is a placeholder for a tool that is not available to the current agent and cannot be called. The tool you attempted to use may belong to a different agent; switch to an agent that provides it if you need it.";

/// Maximum number of consecutive agent-loop turns that yield no executable tool
/// calls (only parse errors and/or `dummy` placeholder calls) before the agent
/// stops auto-resending and ends the turn. Guards against an unbounded
/// request/response loop when the model repeatedly calls an unavailable tool.
pub const MAX_CONSECUTIVE_UNEXECUTABLE_TOOL_TURNS: usize = 3;

/// Assistant message surfaced when [`MAX_CONSECUTIVE_UNEXECUTABLE_TOOL_TURNS`]
/// is reached and the turn is force-ended.
pub const REPEATED_UNEXECUTABLE_TOOL_MESSAGE: &str = "Stopped after repeated attempts to call tools that aren't available. The required tools may belong to a different agent -- consider switching agents, or rephrase your request.";

/// Safety cap to prevent loading extremely large files into memory.
/// The actual context budget is enforced separately in create_context_messages.
pub const MAX_RESOURCE_FILE_LENGTH: u64 = 5 * 1024 * 1024;

/// Approximate bytes per token for estimation.
pub const BYTES_PER_TOKEN: usize = 4;

/// Default context window size (in tokens) when the model doesn't report one.
pub const DEFAULT_CONTEXT_WINDOW_SIZE: usize = 200_000;

pub const RTS_VALID_TOOL_NAME_REGEX: &str = "^[a-zA-Z][a-zA-Z0-9_-]{0,64}$";

pub const MAX_TOOL_NAME_LEN: usize = 64;

/// Threshold for warning about large tool descriptions that may impact performance
pub const LARGE_TOOL_DESCRIPTION_THRESHOLD: usize = 10_000;

pub const DEFAULT_MCP_CREDENTIAL_PATH: &str = "~/.aws/sso/cache";

/// 10 MB
pub const MAX_IMAGE_SIZE_BYTES: u64 = 10 * 1024 * 1024;

pub const TOOL_USE_PURPOSE_FIELD_NAME: &str = "__tool_use_purpose";
pub const TOOL_USE_PURPOSE_FIELD_DESCRIPTION: &str = "A brief explanation why you are making this tool use.";

pub const CONTEXT_ENTRY_START_HEADER: &str = "--- CONTEXT ENTRY BEGIN ---\n";
pub const CONTEXT_ENTRY_END_HEADER: &str = "--- CONTEXT ENTRY END ---\n\n";
pub const SKILL_FILES_MESSAGE: &str = "The following file entries contain: name, filepath, and description. You SHOULD decide when to read the full file using the filepath based on its description:\n\n";
pub const DEFERRED_TOOLS_MESSAGE: &str = "The following tool entries contain: tool_id (server_name::tool_name) and description. You SHOULD call tool_search with the tool_id to load a tool before using it, based on its description:\n\n";
