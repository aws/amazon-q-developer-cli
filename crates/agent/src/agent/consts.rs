pub const DEFAULT_AGENT_NAME: &str = "kiro_default";
pub const PLANNER_AGENT_NAME: &str = "kiro_planner";

/// Resources always included for the default agent
pub const DEFAULT_AGENT_RESOURCES: &[&str] = &[
    "file://AGENTS.md",
    "file://README.md",
    "skill://.kiro/skills/*/SKILL.md",
    "skill://~/.kiro/skills/*/SKILL.md",
];

pub const DUMMY_TOOL_NAME: &str = "dummy";

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
