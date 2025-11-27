#[cfg(test)]
mod overflow_tests {
    use std::collections::{HashMap, VecDeque};
    use crate::api_client::model::Tool;
    use crate::cli::chat::consts::MAX_CONVERSATION_STATE_HISTORY_LEN;
    use crate::cli::chat::conversation::{HistoryEntry, enforce_conversation_invariants};
    use crate::cli::chat::message::{AssistantMessage, UserMessage};
    use crate::cli::chat::tools::ToolOrigin;

    fn create_history_entry(content: &str) -> HistoryEntry {
        HistoryEntry {
            user: UserMessage::new_prompt(content.to_string(), None),
            assistant: AssistantMessage::new_response(None, content.to_string()),
            request_metadata: None,
        }
    }

    #[test]
    fn test_overflow_at_boundary() {
        let mut history = VecDeque::new();
        let mut next_message = None;
        let tools: HashMap<ToolOrigin, Vec<Tool>> = HashMap::new();

        // Create exactly 5000 history entries (10000 messages when flattened)
        for i in 0..5000 {
            history.push_back(create_history_entry(&format!("msg{}", i)));
        }

        let (start, end) = enforce_conversation_invariants(&mut history, &mut next_message, &tools);
        let valid_history_len = end - start;
        let total_messages = valid_history_len * 2;

        // BUG: This should fail but doesn't - we're at exactly the limit
        assert!(
            total_messages <= MAX_CONVERSATION_STATE_HISTORY_LEN,
            "Expected total messages {} to be <= {}, but overflow detection didn't trigger",
            total_messages,
            MAX_CONVERSATION_STATE_HISTORY_LEN
        );
    }

    #[test]
    #[should_panic(expected = "Expected overflow detection to trigger")]
    fn test_overflow_one_past_boundary() {
        let mut history = VecDeque::new();
        let mut next_message = None;
        let tools: HashMap<ToolOrigin, Vec<Tool>> = HashMap::new();

        // Create 5001 history entries (10002 messages when flattened)
        for i in 0..5001 {
            history.push_back(create_history_entry(&format!("msg{}", i)));
        }

        let (start, end) = enforce_conversation_invariants(&mut history, &mut next_message, &tools);
        let valid_history_len = end - start;
        let total_messages = valid_history_len * 2;

        // BUG: This WILL overflow (10002 > 10000) but trimming happens too late
        if total_messages > MAX_CONVERSATION_STATE_HISTORY_LEN {
            panic!("Expected overflow detection to trigger before sending {} messages (limit: {})",
                   total_messages, MAX_CONVERSATION_STATE_HISTORY_LEN);
        }
    }

    #[test]
    fn test_overflow_with_context_buffer() {
        let mut history = VecDeque::new();
        let mut next_message = None;
        let tools: HashMap<ToolOrigin, Vec<Tool>> = HashMap::new();

        // Create 4998 entries (9996 messages) - should be safe with 6-message buffer
        for i in 0..4998 {
            history.push_back(create_history_entry(&format!("msg{}", i)));
        }

        let (start, end) = enforce_conversation_invariants(&mut history, &mut next_message, &tools);
        let valid_history_len = end - start;
        let total_messages = valid_history_len * 2;

        // With 6 context messages, total would be 9996 + 6 = 10002 (OVERFLOW!)
        let total_with_context = total_messages + 6;
        assert!(
            total_with_context <= MAX_CONVERSATION_STATE_HISTORY_LEN,
            "BUG: Total messages with context {} exceeds limit {} - trimming should have occurred earlier",
            total_with_context,
            MAX_CONVERSATION_STATE_HISTORY_LEN
        );
    }

    #[test]
    fn test_trimming_threshold() {
        let mut history = VecDeque::new();
        let mut next_message = None;
        let tools: HashMap<ToolOrigin, Vec<Tool>> = HashMap::new();

        // Test the exact threshold where trimming should occur
        // Current buggy condition: (history.len() * 2) > MAX_CONVERSATION_STATE_HISTORY_LEN - 6
        // This means: history.len() > 4997
        
        // At 4997 entries: 4997 * 2 = 9994, which is NOT > 9994, so no trimming
        for i in 0..4997 {
            history.push_back(create_history_entry(&format!("msg{}", i)));
        }
        
        let initial_len = history.len();
        let (start, end) = enforce_conversation_invariants(&mut history, &mut next_message, &tools);
        let trimmed = start > 0;
        
        assert!(
            !trimmed,
            "BUG: At {} entries (9994 messages), trimming should not occur yet, but it did",
            initial_len
        );

        // At 4998 entries: 4998 * 2 = 9996, which IS > 9994, so trimming occurs
        history.push_back(create_history_entry("msg4997"));
        let (start, end) = enforce_conversation_invariants(&mut history, &mut next_message, &tools);
        let trimmed = start > 0;
        
        assert!(
            trimmed,
            "At 4998 entries (9996 messages), trimming should occur"
        );
        
        // But by now we've already exceeded the safe limit with context messages!
        let valid_history_len = end - start;
        let total_with_context = (valid_history_len * 2) + 6;
        assert!(
            total_with_context <= MAX_CONVERSATION_STATE_HISTORY_LEN,
            "BUG: After trimming, total {} still exceeds limit {}",
            total_with_context,
            MAX_CONVERSATION_STATE_HISTORY_LEN
        );
    }

    #[test]
    fn test_proactive_trimming_needed() {
        // This test demonstrates what the threshold SHOULD be
        let safe_threshold = (MAX_CONVERSATION_STATE_HISTORY_LEN - 100) / 2; // Leave 100 message buffer
        
        let mut history = VecDeque::new();
        let mut next_message = None;
        let tools: HashMap<ToolOrigin, Vec<Tool>> = HashMap::new();

        // Fill to the safe threshold
        for i in 0..safe_threshold {
            history.push_back(create_history_entry(&format!("msg{}", i)));
        }

        let total_messages = history.len() * 2;
        let buffer_remaining = MAX_CONVERSATION_STATE_HISTORY_LEN - total_messages;
        
        assert!(
            buffer_remaining >= 100,
            "Safe threshold should leave at least 100 messages of buffer, but only {} remaining",
            buffer_remaining
        );
    }
}
