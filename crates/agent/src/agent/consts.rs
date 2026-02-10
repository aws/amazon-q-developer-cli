pub const DEFAULT_AGENT_NAME: &str = "kiro_default";

/// Resources always included for the default agent
pub const DEFAULT_AGENT_RESOURCES: &[&str] = &[
    "file://AGENTS.md",
    "file://README.md",
    "skill://.kiro/skills/*/SKILL.md",
    "skill://~/.kiro/skills/*/SKILL.md",
];

pub const MAX_CONVERSATION_STATE_HISTORY_LEN: usize = 500;

pub const DUMMY_TOOL_NAME: &str = "dummy";

pub const MAX_RESOURCE_FILE_LENGTH: u64 = 1024 * 10;

pub const RTS_VALID_TOOL_NAME_REGEX: &str = "^[a-zA-Z][a-zA-Z0-9_-]{0,64}$";

pub const MAX_TOOL_NAME_LEN: usize = 64;

/// Threshold for warning about large tool descriptions that may impact performance
pub const LARGE_TOOL_DESCRIPTION_THRESHOLD: usize = 10_000;

pub const DEFAULT_MCP_CREDENTIAL_PATH: &str = "~/.aws/sso/cache";

/// 10 MB
pub const MAX_IMAGE_SIZE_BYTES: u64 = 10 * 1024 * 1024;

pub const TOOL_USE_PURPOSE_FIELD_NAME: &str = "__tool_use_purpose";
pub const TOOL_USE_PURPOSE_FIELD_DESCRIPTION: &str = "A brief explanation why you are making this tool use.";
