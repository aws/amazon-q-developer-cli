use super::token_counter::TokenCounter;

// These limits are the internal undocumented values from the service for each item

pub const MAX_CURRENT_WORKING_DIRECTORY_LEN: usize = 256;

/// Limit to send the number of messages as part of chat.
pub const MAX_CONVERSATION_STATE_HISTORY_LEN: usize = 250;

pub const MAX_TOOL_RESPONSE_SIZE: usize = 800_000;

/// TODO: Use this to gracefully handle user message sizes.
#[allow(dead_code)]
pub const MAX_USER_MESSAGE_SIZE: usize = 600_000;

/// In tokens
pub const CONTEXT_WINDOW_SIZE: usize = 200_000;

pub const CONTEXT_FILES_MAX_SIZE: usize = 150_000;

pub const MAX_CHARS: usize = TokenCounter::token_to_chars(CONTEXT_WINDOW_SIZE); // Character-based warning threshold
