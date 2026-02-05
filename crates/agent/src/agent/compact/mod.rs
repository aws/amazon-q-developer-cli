mod facts;

use std::collections::VecDeque;

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
    enforce_conversation_invariants,
};

const TRUNCATED_SUFFIX: &str = "...truncated due to length";
const DEFAULT_MAX_MESSAGE_LEN: usize = 25_000;
/// Default (User, Assistant) message pairs to exclude from compaction.
pub const DEFAULT_MESSAGE_PAIRS_TO_EXCLUDE: usize = 2;
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompactStrategy {
    /// Number of (User, Assistant) message pairs to exclude from the history as part of compaction.
    pub message_pairs_to_exclude: usize,
    /// Percentage of context window to exclude from compaction.
    pub context_window_percent_to_exclude: usize,
    /// Whether or not to truncate large messages in the history.
    pub truncate_large_messages: bool,
    /// Maximum allowed size of messages in the conversation history. Only applied when
    /// [Self::truncate_large_messages] is true.
    pub max_message_length: usize,
}

impl CompactStrategy {
    /// Returns the number of messages to exclude from the end of the history.
    ///
    /// The returned value ensures that `messages[..messages.len() - return_value]` consists of
    /// (User, Assistant) pairs.
    fn effective_messages_to_exclude(&self, messages: &[Message], context_window_size: usize) -> usize {
        if messages.is_empty() {
            return 0;
        }

        // If the last message is from the user, exclude it from pair counting
        let trailing_user = messages.last().is_some_and(|m| m.role == Role::User);
        let pair_messages = &messages[..messages.len().saturating_sub(trailing_user as usize)];

        // Calculate how many tokens we want to preserve based on the percentage
        let tokens_to_exclude = context_window_size * self.context_window_percent_to_exclude / 100;
        let target_bytes = tokens_to_exclude * BYTES_PER_TOKEN;

        // Count message pairs from the end until we exceed the byte budget.
        let token_based_pair_count = {
            let mut total_bytes = 0;
            let mut pair_count = 0;
            for pair in pair_messages.rchunks(2) {
                pair_count += 1;
                total_bytes += pair.iter().map(|m| m.byte_len()).sum::<usize>();
                if total_bytes >= target_bytes {
                    break;
                }
            }
            pair_count
        };

        let exclude_count = self.message_pairs_to_exclude.max(token_based_pair_count) * 2;
        let exclude_count = if trailing_user {
            exclude_count + 1
        } else {
            exclude_count
        };

        // Ensure we leave at least 2 messages to summarize
        let max_exclude = messages.len().saturating_sub(2);
        exclude_count.min(max_exclude)
    }
}

impl CompactStrategy {
    /// Default compaction strategy that preserves message content.
    ///
    /// Excludes 2 message pairs from compaction and does not truncate large messages.
    /// Use this as the first attempt when compacting conversation history.
    pub fn default_strategy() -> Self {
        Self {
            message_pairs_to_exclude: DEFAULT_MESSAGE_PAIRS_TO_EXCLUDE,
            context_window_percent_to_exclude: DEFAULT_CONTEXT_WINDOW_PERCENT_TO_EXCLUDE,
            truncate_large_messages: false,
            max_message_length: DEFAULT_MAX_MESSAGE_LEN,
        }
    }

    /// Aggressive compaction strategy that truncates large messages.
    ///
    /// Use this as a fallback when the default strategy fails due to context window overflow.
    pub fn aggressive_strategy() -> Self {
        Self {
            message_pairs_to_exclude: DEFAULT_MESSAGE_PAIRS_TO_EXCLUDE,
            context_window_percent_to_exclude: DEFAULT_CONTEXT_WINDOW_PERCENT_TO_EXCLUDE,
            truncate_large_messages: true,
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

    let mut messages = VecDeque::from(history);
    let mut tools = Vec::new();
    enforce_conversation_invariants(&mut messages, &mut tools);

    SendRequestArgs {
        messages: messages.into(),
        tool_specs: if tools.is_empty() { None } else { Some(tools) },
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
    let messages_snapshot: Vec<Message> = messages[drain_end..].to_vec();

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
    let entry = LogEntry::compaction(summary, *strategy, messages_snapshot);
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
    use crate::agent::agent_loop::types::{
        ToolResultBlock,
        ToolResultContentBlock,
        ToolResultStatus,
        ToolUseBlock,
    };
    use crate::agent::detect_invariant_violations;
    use crate::agent::event_log::{
        LogEntry,
        LogEntryV1,
    };

    fn assert_valid_conversation(messages: &[Message]) {
        let violations = detect_invariant_violations(messages);
        assert!(
            violations.is_valid(),
            "invalid conversation: {:?}\nmessages: {:#?}",
            violations,
            messages
        );
    }

    fn append_user(conv: &mut ConversationState, text: &str) {
        conv.append_log(LogEntry::prompt(Uuid::new_v4().to_string(), vec![ContentBlock::Text(
            text.into(),
        )]));
    }

    fn append_assistant(conv: &mut ConversationState, text: &str) {
        conv.append_log(LogEntry::assistant_message(Uuid::new_v4().to_string(), vec![
            ContentBlock::Text(text.into()),
        ]));
    }

    fn append_assistant_tool_use(conv: &mut ConversationState, tool_use_id: &str) {
        conv.append_log(LogEntry::assistant_message(Uuid::new_v4().to_string(), vec![
            ContentBlock::ToolUse(ToolUseBlock {
                tool_use_id: tool_use_id.into(),
                name: "test_tool".into(),
                input: serde_json::json!({}),
            }),
        ]));
    }

    fn append_user_tool_result(conv: &mut ConversationState, tool_use_id: &str) {
        conv.append_log(LogEntry::prompt(Uuid::new_v4().to_string(), vec![
            ContentBlock::ToolResult(ToolResultBlock {
                tool_use_id: tool_use_id.into(),
                content: vec![ToolResultContentBlock::Text("result".into())],
                status: ToolResultStatus::Success,
            }),
        ]));
    }

    /// Creates a (User, Assistant) message pair with specific content sizes (in chars).
    fn create_message_pair(user_chars: usize, assistant_chars: usize) -> [Message; 2] {
        [
            Message::new(Role::User, vec![ContentBlock::Text("x".repeat(user_chars))], None),
            Message::new(
                Role::Assistant,
                vec![ContentBlock::Text("y".repeat(assistant_chars))],
                None,
            ),
        ]
    }

    const TEST_CONTEXT_WINDOW: usize = 128_000;

    #[test]
    fn test_effective_messages_to_exclude_percentage_based() {
        // Each pair: 4000 chars user + 4000 chars assistant = 8000 chars = ~2000 tokens
        let messages: Vec<Message> = (0..5).flat_map(|_| create_message_pair(4000, 4000)).collect();

        // Use 4% of 128K = 5120 tokens = ~20480 chars
        let strategy = CompactStrategy {
            message_pairs_to_exclude: 1,
            context_window_percent_to_exclude: 4,
            ..CompactStrategy::default_strategy()
        };

        let effective = strategy.effective_messages_to_exclude(&messages, TEST_CONTEXT_WINDOW);
        // 4% of 128K = 5120 tokens = 20480 chars. Each pair is 8000 chars.
        // 1st pair: 8000 (under), 2nd: 16000 (under), 3rd: 24000 (crosses threshold)
        // max(1, 3) = 3 pairs = 6 messages
        assert_eq!(effective, 6);
    }

    #[test]
    fn test_effective_messages_to_exclude_message_based_wins() {
        let messages: Vec<Message> = (0..5).flat_map(|_| create_message_pair(50, 50)).collect();

        let strategy = CompactStrategy {
            message_pairs_to_exclude: 4,
            context_window_percent_to_exclude: 0,
            ..CompactStrategy::default_strategy()
        };

        let effective = strategy.effective_messages_to_exclude(&messages, TEST_CONTEXT_WINDOW);
        // Token-based gives 0, message_pairs_to_exclude is 4, max(4, 0) = 4 pairs = 8 messages
        assert_eq!(effective, 8);
    }

    #[test]
    fn test_effective_messages_to_exclude_empty_history() {
        let messages: Vec<Message> = vec![];
        let strategy = CompactStrategy::default_strategy();

        let effective = strategy.effective_messages_to_exclude(&messages, TEST_CONTEXT_WINDOW);
        assert_eq!(effective, 0);
    }

    #[test]
    fn test_effective_messages_to_exclude_small_history_leaves_messages_to_summarize() {
        // 3 messages: User, Assistant, User (trailing)
        let messages = vec![
            Message::new(Role::User, vec![ContentBlock::Text("x".repeat(100000))], None),
            Message::new(Role::Assistant, vec![ContentBlock::Text("y".repeat(100))], None),
            Message::new(Role::User, vec![ContentBlock::Text("z".repeat(100))], None),
        ];

        let strategy = CompactStrategy {
            message_pairs_to_exclude: 1,
            context_window_percent_to_exclude: 2,
            ..CompactStrategy::default_strategy()
        };

        let effective = strategy.effective_messages_to_exclude(&messages, TEST_CONTEXT_WINDOW);
        assert_eq!(effective, 1);
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

    #[test]
    fn test_compaction_full_flow() {
        let mut conv = ConversationState::new(Uuid::new_v4(), Vec::new());

        append_user(&mut conv, "hello");
        append_assistant_tool_use(&mut conv, "tool_1");
        append_user_tool_result(&mut conv, "tool_1");
        append_assistant_tool_use(&mut conv, "tool_2");
        append_user_tool_result(&mut conv, "tool_2");

        assert_valid_conversation(conv.messages());

        let strategy = CompactStrategy {
            message_pairs_to_exclude: 1,
            context_window_percent_to_exclude: 0,
            ..CompactStrategy::default_strategy()
        };

        let request = create_compaction_request(conv.messages(), &strategy, Some(100_000), None::<String>, None);
        assert_valid_conversation(&request.messages);

        let model_response = Message::new(Role::Assistant, vec![ContentBlock::Text("Summary".into())], None);

        let (_entry, _index) = finalize_compaction(&mut conv, model_response, &strategy, Some(100_000));
        let messages = conv.messages();

        // After compaction with message_pairs_to_exclude=1:
        // Original: [U(hello), A(tool_1), U(result_1), A(tool_2), U(result_2)]
        // Excluded: 1 pair (2) + trailing user (1) = 3
        // Kept: messages[2..] = [U(result_1), A(tool_2), U(result_2)]
        assert_eq!(
            messages.len(),
            3,
            "expected 3 messages after compaction, got {}",
            messages.len()
        );
        assert_eq!(messages[0].role, Role::User);
        assert_eq!(messages[1].role, Role::Assistant);
        assert_eq!(messages[2].role, Role::User);

        // The first message should have tool_1 result (orphaned, will be fixed at request time)
        assert!(
            messages[0].tool_results_iter().any(|tr| tr.tool_use_id == "tool_1"),
            "expected tool_1 result in first message"
        );

        // The last message should have tool_2 result
        assert!(
            messages[2].tool_results_iter().any(|tr| tr.tool_use_id == "tool_2"),
            "expected tool_2 result in last message"
        );
    }

    #[test]
    fn test_default_compaction_strategy() {
        let mut conv = ConversationState::new(Uuid::new_v4(), Vec::new());

        // Add 3 user/assistant pairs
        for i in 0..3 {
            append_user(&mut conv, &format!("user {i}"));
            append_assistant(&mut conv, &format!("assistant {i}"));
        }

        assert_eq!(conv.messages().len(), 6);
        assert_valid_conversation(conv.messages());

        let strategy = CompactStrategy::default_strategy();
        let request = create_compaction_request(conv.messages(), &strategy, Some(100_000), None::<String>, None);
        assert_valid_conversation(&request.messages);

        // default excludes 2 pairs (4 messages), keeps 2 + compaction prompt = 3
        assert_eq!(request.messages.len(), 3);

        let model_response = Message::new(Role::Assistant, vec![ContentBlock::Text("Summary".into())], None);
        let (entry, index) = finalize_compaction(&mut conv, model_response, &strategy, Some(100_000));

        // After compaction: 4 messages remain (2 pairs excluded)
        assert_eq!(conv.messages().len(), 4);
        assert_valid_conversation(conv.messages());

        match &entry {
            LogEntry::V1(LogEntryV1::Compaction { summary, .. }) => {
                assert!(summary.contains("Summary"));
            },
            _ => panic!("Expected compaction entry"),
        }
        assert!(index > 0);
    }
}
