mod facts;

pub use facts::{
    CommandEntry,
    CompactionFacts,
    FileEntry,
    extract_compaction_facts,
};
use serde::{
    Deserialize,
    Serialize,
};

use super::agent_loop::protocol::SendRequestArgs;
use super::agent_loop::types::{
    ContentBlock,
    Message,
    Role,
};
use super::event_log::LogEntry;
use super::types::ConversationState;
use super::{
    CONTEXT_ENTRY_END_HEADER,
    CONTEXT_ENTRY_START_HEADER,
};

const TRUNCATED_SUFFIX: &str = "...truncated due to length";
const DEFAULT_MAX_MESSAGE_LEN: usize = 25_000;
/// Default message pairs to exclude from compaction.
pub const DEFAULT_MESSAGES_TO_EXCLUDE: usize = 2;
/// Default percentage of context window to exclude from compaction.
pub const DEFAULT_CONTEXT_WINDOW_PERCENT_TO_EXCLUDE: usize = 2;
/// Approximate bytes per token for estimation.
const BYTES_PER_TOKEN: usize = 4;
/// Default context window size used when none is provided.
const DEFAULT_CONTEXT_WINDOW_SIZE: usize = 100_000;

/// State associated with an agent compacting its conversation state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompactingState {
    /// The user message that failed to be sent due to the context window overflowing, if
    /// available.
    ///
    /// If this is [Some], then this indicates that auto-compaction was applied. See
    /// [super::types::AgentSettings::disable_auto_compact].
    pub last_user_message: Option<Message>,
    /// Strategy used when creating the compact request.
    pub strategy: CompactStrategy,
    /// The conversation state currently being summarized
    pub conversation: ConversationState,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompactStrategy {
    /// Number of user/assistant pairs to exclude from the history as part of compaction.
    pub messages_to_exclude: usize,
    /// Percentage of context window to exclude from compaction.
    pub context_window_percent_to_exclude: usize,
    /// Whether or not to truncate large messages in the history.
    pub truncate_large_messages: bool,
    /// Maximum allowed size of messages in the conversation history. Only applied when
    /// [Self::truncate_large_messages] is true.
    pub max_message_length: usize,
}

impl CompactStrategy {
    /// Returns max(messages_to_exclude, messages fitting in context_window_percent_to_exclude).
    fn effective_messages_to_exclude(&self, messages: &[Message], context_window_size: usize) -> usize {
        // Calculate how many tokens we want to preserve based on the percentage
        let tokens_to_exclude = context_window_size * self.context_window_percent_to_exclude / 100;
        let target_bytes = tokens_to_exclude * BYTES_PER_TOKEN;

        // Count messages from the end until we exceed the byte budget.
        // This tells us how many recent messages fit within the percentage-based limit.
        let token_based_count = {
            let mut total_bytes = 0;
            let mut count = 0;
            for msg in messages.iter().rev() {
                count += 1;
                total_bytes += msg.byte_len();
                if total_bytes >= target_bytes {
                    break;
                }
            }
            count
        };

        self.messages_to_exclude.max(token_based_count)
    }
}

impl Default for CompactStrategy {
    fn default() -> Self {
        Self {
            messages_to_exclude: DEFAULT_MESSAGES_TO_EXCLUDE,
            context_window_percent_to_exclude: DEFAULT_CONTEXT_WINDOW_PERCENT_TO_EXCLUDE,
            truncate_large_messages: false,
            max_message_length: DEFAULT_MAX_MESSAGE_LEN,
        }
    }
}

/// Creates a request to generate a conversation summary.
pub fn create_compaction_request(
    messages: &[Message],
    strategy: &CompactStrategy,
    context_window_size: Option<usize>,
    custom_prompt: Option<String>,
    latest_summary: Option<&str>,
) -> SendRequestArgs {
    let effective_exclude =
        strategy.effective_messages_to_exclude(messages, context_window_size.unwrap_or(DEFAULT_CONTEXT_WINDOW_SIZE));

    // Take messages to summarize (exclude recent ones)
    let end_idx = messages.len().saturating_sub(effective_exclude);
    let mut history: Vec<Message> = messages[..end_idx].to_vec();

    if strategy.truncate_large_messages {
        for msg in &mut history {
            msg.truncate(strategy.max_message_length, Some(TRUNCATED_SUFFIX));
        }
    }

    let summary_prompt = create_summary_prompt(custom_prompt, latest_summary);
    history.push(Message::new(Role::User, vec![ContentBlock::Text(summary_prompt)], None));

    SendRequestArgs {
        messages: history,
        tool_specs: None,
        system_prompt: None,
    }
}

/// Finalizes compaction by extracting facts from history, combining with LLM summary,
/// and appending the compaction log entry to the conversation state.
///
/// Returns the (log_entry, index) that was appended.
pub fn finalize_compaction(
    conversation_state: &mut ConversationState,
    model_response: Message,
    strategy: &CompactStrategy,
    context_window_size: Option<usize>,
) -> (LogEntry, usize) {
    let messages = conversation_state.messages();
    let context_window = context_window_size.unwrap_or(DEFAULT_CONTEXT_WINDOW_SIZE);
    let effective_exclude = strategy.effective_messages_to_exclude(messages, context_window);

    // Extract facts before we lose access to the full history
    let facts = extract_compaction_facts(messages, effective_exclude);

    // Calculate messages to keep (the ones excluded from compaction)
    let drain_end = messages.len().saturating_sub(effective_exclude);
    let messages_snapshot = messages[drain_end..].to_vec();

    // Extract LLM summary from response
    let llm_summary = model_response
        .content
        .iter()
        .filter_map(|c| c.text())
        .collect::<Vec<_>>()
        .join("\n");

    // Combine LLM summary with facts
    let summary = if facts.is_empty() {
        llm_summary
    } else {
        format!("{llm_summary}\n\n## FACTUAL RECORD\n\n{facts}")
    };

    // Create and append the log entry
    let entry = LogEntry::compaction(summary, strategy.clone(), messages_snapshot);
    let index = conversation_state.append_log(entry.clone());

    (entry, index)
}

fn create_summary_prompt(custom_prompt: Option<String>, latest_summary: Option<impl AsRef<str>>) -> String {
    const COMPACTION_PROMPT: &str = include_str!("compaction_prompt.md");

    let custom_instruction = custom_prompt
        .map(|p| format!("IMPORTANT CUSTOM INSTRUCTION: {}\n\n", p))
        .unwrap_or_default();
    let mut summary_content = COMPACTION_PROMPT.replace("{{CUSTOM_INSTRUCTION}}\n", &custom_instruction);

    if let Some(summary) = latest_summary {
        summary_content.push_str("\n\n");
        summary_content.push_str(CONTEXT_ENTRY_START_HEADER);
        summary_content.push_str("This summary contains ALL relevant information from our previous conversation including tool uses, results, code analysis, and file operations. YOU MUST be sure to include this information when creating your summarization document.\n\n");
        summary_content.push_str("SUMMARY CONTENT:\n");
        summary_content.push_str(summary.as_ref());
        summary_content.push('\n');
        summary_content.push_str(CONTEXT_ENTRY_END_HEADER);
    }

    summary_content
}

#[cfg(test)]
mod tests {
    use uuid::Uuid;

    use super::*;
    use crate::agent::event_log::{
        LogEntry,
        LogEntryV1,
    };

    #[test]
    fn test_finalize_compaction_drains_old_messages() {
        let mut conversation_state = ConversationState::new(Uuid::new_v4(), Vec::new());

        // Add 5 user/assistant pairs via append_log
        for i in 0..5 {
            conversation_state.append_log(LogEntry::prompt(format!("msg-{}", i * 2), vec![ContentBlock::Text(
                format!("user {i}"),
            )]));
            conversation_state.append_log(LogEntry::assistant_message(format!("msg-{}", i * 2 + 1), vec![
                ContentBlock::Text(format!("assistant {i}")),
            ]));
        }

        assert_eq!(conversation_state.messages().len(), 10);

        let strategy = CompactStrategy {
            messages_to_exclude: 2,
            context_window_percent_to_exclude: 0,
            ..Default::default()
        };

        let model_response = Message::new(
            Role::Assistant,
            vec![ContentBlock::Text("This is the summary".to_string())],
            None,
        );

        let (entry, index) = finalize_compaction(&mut conversation_state, model_response, &strategy, Some(100_000));

        // After compaction, should have only the excluded messages
        assert_eq!(conversation_state.messages().len(), 2);

        // Entry should contain the summary
        match &entry {
            LogEntry::V1(LogEntryV1::Compaction { summary, .. }) => {
                assert!(summary.contains("This is the summary"));
            },
            _ => panic!("Expected compaction entry"),
        }

        // Index should be valid
        assert!(index > 0);
    }

    #[test]
    fn test_create_compaction_request_excludes_recent() {
        let messages: Vec<Message> = (0..6)
            .map(|i| {
                let role = if i % 2 == 0 { Role::User } else { Role::Assistant };
                Message::new(role, vec![ContentBlock::Text(format!("msg{}", i))], None)
            })
            .collect();

        let strategy = CompactStrategy {
            messages_to_exclude: 2,
            context_window_percent_to_exclude: 0,
            ..Default::default()
        };

        let request = create_compaction_request(&messages, &strategy, Some(100_000), None::<String>, None);

        // Should have 4 old messages + 1 summary prompt = 5
        assert_eq!(request.messages.len(), 5);
        assert_eq!(request.messages.last().unwrap().role, Role::User);
    }

    #[test]
    fn test_create_summary_prompt_includes_latest_summary() {
        let prompt = create_summary_prompt(None, Some("Previous summary"));
        assert!(prompt.contains("Previous summary"));
    }

    #[test]
    fn test_create_summary_prompt_with_custom_prompt() {
        let prompt = create_summary_prompt(Some("Focus on code".to_string()), None::<&str>);
        assert!(prompt.contains("Focus on code"));
    }
}
