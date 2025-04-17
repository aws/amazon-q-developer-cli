use super::token_counter::TokenCounter;

pub const MAX_TOOL_RESPONSE_SIZE: usize = 800_000;

/// TODO: Use this to gracefully handle user message sizes.
#[allow(dead_code)]
pub const MAX_USER_MESSAGE_SIZE: usize = 600_000;

/// In tokens
pub const CONTEXT_WINDOW_SIZE: usize = 200_000;

pub const MAX_CHARS: usize = TokenCounter::token_to_chars(CONTEXT_WINDOW_SIZE); // Character-based warning threshold
