/// Name of the default agent.
pub const BUILTIN_VIBER_AGENT_NAME: &str = "cli_default";
pub const BUILTIN_PLANNER_AGENT_NAME: &str = "cli_planner";

pub const MAX_CONVERSATION_STATE_HISTORY_LEN: usize = 500;

pub const DUMMY_TOOL_NAME: &str = "dummy";

pub const MAX_RESOURCE_FILE_LENGTH: u64 = 1024 * 10;

pub const RTS_VALID_TOOL_NAME_REGEX: &str = "^[a-zA-Z][a-zA-Z0-9_-]{0,64}$";

pub const MAX_TOOL_NAME_LEN: usize = 64;

pub const MAX_TOOL_SPEC_DESCRIPTION_LEN: usize = 10_004;

/// 10 MB
pub const MAX_IMAGE_SIZE_BYTES: u64 = 10 * 1024 * 1024;
