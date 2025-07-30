use super::token_counter::TokenCounter;

// These limits are the internal undocumented values from the service for each item

pub const MAX_CURRENT_WORKING_DIRECTORY_LEN: usize = 256;

/// Limit to send the number of messages as part of chat.
pub const MAX_CONVERSATION_STATE_HISTORY_LEN: usize = 250;

/// Actual service limit is 800_000
pub const MAX_TOOL_RESPONSE_SIZE: usize = 400_000;

/// Actual service limit is 600_000
pub const MAX_USER_MESSAGE_SIZE: usize = 400_000;

/// In tokens
pub const CONTEXT_WINDOW_SIZE: usize = 200_000;

pub const CONTEXT_FILES_MAX_SIZE: usize = 150_000;

pub const MAX_CHARS: usize = TokenCounter::token_to_chars(CONTEXT_WINDOW_SIZE); // Character-based warning threshold

pub const DUMMY_TOOL_NAME: &str = "dummy";

pub const MAX_NUMBER_OF_IMAGES_PER_REQUEST: usize = 10;

/// In bytes - 10 MB
pub const MAX_IMAGE_SIZE: usize = 10 * 1024 * 1024;

pub const AGENT_FORMAT_HOOKS_DOC_URL: &str =
    "https://github.com/aws/amazon-q-developer-cli/blob/main/docs/agent-format.md#hooks-field";

pub const AGENT_FORMAT_TOOLS_DOC_URL: &str =
    "https://github.com/aws/amazon-q-developer-cli/blob/main/docs/agent-format.md#tools-field";
