use std::collections::VecDeque;

use fig_api_client::model::ChatMessage;

use crate::util::token_counter::TokenCounter;

/// Character count warning levels for conversation size
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TokenWarningLevel {
    /// No warning, conversation is within normal limits
    None,
    /// Critical level - at single warning threshold (600K characters)
    Critical,
}

/// Constants for character-based warning threshold
pub const CONTEXT_WINDOW_SIZE: usize = 200_000; // tokens
pub const MAX_CHARS: usize = TokenCounter::token_to_chars(CONTEXT_WINDOW_SIZE); // Character-based warning threshold

/// State for tracking summarization process
#[derive(Debug, Clone)]
pub struct SummarizationState {
    /// The saved original history
    pub original_history: Option<VecDeque<ChatMessage>>,
    /// Optional custom prompt used for summarization
    pub custom_prompt: Option<String>,
    /// Whether to show the summary after compacting
    pub show_summary: bool,
}

impl SummarizationState {
    #[allow(dead_code)]
    pub fn new() -> Self {
        Self {
            original_history: None,
            custom_prompt: None,
            show_summary: false,
        }
    }

    pub fn with_prompt(prompt: Option<String>) -> Self {
        Self {
            original_history: None,
            custom_prompt: prompt,
            show_summary: false,
        }
    }
}
