use serde::{
    Deserialize,
    Serialize,
};

use super::agent_loop::protocol::SendRequestArgs;
use super::agent_loop::types::{
    ContentBlock,
    Message,
    ToolResultContentBlock,
};
use super::types::ConversationState;
use super::util::truncate_safe_in_place;
use super::{
    CONTEXT_ENTRY_END_HEADER,
    CONTEXT_ENTRY_START_HEADER,
};

const TRUNCATED_SUFFIX: &str = "...truncated due to length";
const DEFAULT_MAX_MESSAGE_LEN: usize = 25_000;

/// State associated with an agent compacting its conversation state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompactingState {
    /// The user message that failed to be sent due to the context window overflowing, if
    /// available.
    ///
    /// If this is [Some], then this indicates that auto-compaction was applied. See
    /// [super::types::AgentSettings::auto_compact].
    pub last_user_message: Option<Message>,
    /// Strategy used when creating the compact request.
    pub strategy: CompactStrategy,
    /// The conversation state currently being summarized
    pub conversation: ConversationState,
    // TODO - result sender?
    // #[serde(skip)]
    // pub result_tx: Option<oneshot::Sender<()>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompactStrategy {
    // /// Number of user/assistant pairs to exclude from the history as part of compaction.
    // pub messages_to_exclude: usize,
    /// Whether or not to truncate large messages in the history.
    pub truncate_large_messages: bool,
    /// Maximum allowed size of messages in the conversation history. Only applied when
    /// [Self::truncate_large_messages] is true.
    pub max_message_length: usize,
}

impl CompactStrategy {
    /// Modifies the given request in order to apply the compaction strategy.
    pub fn apply_strategy(&self, request: &mut SendRequestArgs) {
        if self.truncate_large_messages {
            for msg in &mut request.messages {
                // Truncate each content block equally
                let mut total_len = 0;
                let mut total_items = 0;
                // First pass - calculate total length
                for c in &msg.content {
                    match c {
                        ContentBlock::Text(text) => {
                            total_len += text.len();
                            total_items += 1;
                        },
                        ContentBlock::ToolResult(block) => {
                            for c in &block.content {
                                match c {
                                    ToolResultContentBlock::Text(text) => {
                                        total_len += text.len();
                                        total_items += 1;
                                    },
                                    ToolResultContentBlock::Json(value) => {
                                        total_len += serde_json::to_string(value).unwrap_or_default().len();
                                        total_items += 1;
                                    },
                                    ToolResultContentBlock::Image(_) => (),
                                }
                            }
                        },
                        ContentBlock::ToolUse(_) | ContentBlock::Image(_) => (),
                    }
                }
                if total_len <= self.max_message_length {
                    continue;
                }
                // Second pass - perform truncation
                let max_bytes = self.max_message_length / total_items;
                for c in &mut msg.content {
                    match c {
                        ContentBlock::Text(text) => {
                            truncate_safe_in_place(text, max_bytes, TRUNCATED_SUFFIX);
                        },
                        ContentBlock::ToolResult(block) => {
                            for c in &mut block.content {
                                match c {
                                    ToolResultContentBlock::Text(text) => {
                                        truncate_safe_in_place(text, max_bytes, TRUNCATED_SUFFIX);
                                    },
                                    val @ ToolResultContentBlock::Json(_) => {
                                        // For simplicity, convert the JSON to text in order to truncate the
                                        // amount. Otherwise, we'd need to iterate through the JSON
                                        // value itself to find fields to truncate.
                                        let serde_val = if let ToolResultContentBlock::Json(v) = &val {
                                            let mut s = serde_json::to_string(v).unwrap_or_default();
                                            truncate_safe_in_place(&mut s, max_bytes, TRUNCATED_SUFFIX);
                                            s
                                        } else {
                                            String::new()
                                        };
                                        *val = ToolResultContentBlock::Text(serde_val);
                                    },
                                    ToolResultContentBlock::Image(_) => (),
                                }
                            }
                        },
                        ContentBlock::ToolUse(_) | ContentBlock::Image(_) => (),
                    }
                }
            }
        }
    }
}

impl Default for CompactStrategy {
    fn default() -> Self {
        Self {
            truncate_large_messages: false,
            max_message_length: DEFAULT_MAX_MESSAGE_LEN,
        }
    }
}

pub fn create_summary_prompt(custom_prompt: Option<String>, latest_summary: Option<impl AsRef<str>>) -> String {
    let mut summary_content = match custom_prompt {
        Some(custom_prompt) => {
            // Make the custom instructions much more prominent and directive
            format!(
                "[SYSTEM NOTE: This is an automated summarization request, not from the user]\n\n\
                FORMAT REQUIREMENTS: Create a structured, concise summary in bullet-point format. DO NOT respond conversationally. DO NOT address the user directly.\n\n\
                IMPORTANT CUSTOM INSTRUCTION: {}\n\n\
                Your task is to create a structured summary document containing:\n\
                1) A bullet-point list of key topics/questions covered\n\
                2) Bullet points for all significant tools executed and their results\n\
                3) Bullet points for any code or technical information shared\n\
                4) A section of key insights gained\n\n\
                5) REQUIRED: the ID of the currently loaded todo list, if any\n\n\
                FORMAT THE SUMMARY IN THIRD PERSON, NOT AS A DIRECT RESPONSE. Example format:\n\n\
                ## CONVERSATION SUMMARY\n\
                * Topic 1: Key information\n\
                * Topic 2: Key information\n\n\
                ## TOOLS EXECUTED\n\
                * Tool X: Result Y\n\n\
                ## TODO ID\n\
                * <id>\n\n\
                Remember this is a DOCUMENT not a chat response. The custom instruction above modifies what to prioritize.\n\
                FILTER OUT CHAT CONVENTIONS (greetings, offers to help, etc).",
                custom_prompt
            )
        },
        None => {
            // Default prompt
            "[SYSTEM NOTE: This is an automated summarization request, not from the user]\n\n\
                FORMAT REQUIREMENTS: Create a structured, concise summary in bullet-point format. DO NOT respond conversationally. DO NOT address the user directly.\n\n\
                Your task is to create a structured summary document containing:\n\
                1) A bullet-point list of key topics/questions covered\n\
                2) Bullet points for all significant tools executed and their results\n\
                3) Bullet points for any code or technical information shared\n\
                4) A section of key insights gained\n\n\
                5) REQUIRED: the ID of the currently loaded todo list, if any\n\n\
                FORMAT THE SUMMARY IN THIRD PERSON, NOT AS A DIRECT RESPONSE. Example format:\n\n\
                ## CONVERSATION SUMMARY\n\
                * Topic 1: Key information\n\
                * Topic 2: Key information\n\n\
                ## TOOLS EXECUTED\n\
                * Tool X: Result Y\n\n\
                ## TODO ID\n\
                * <id>\n\n\
                Remember this is a DOCUMENT not a chat response.\n\
                FILTER OUT CHAT CONVENTIONS (greetings, offers to help, etc).".to_string()
        },
    };

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
    use super::*;

    const TEST_MESSAGES: &str = r#"
[
    {
        "role": "user",
        "content": [
            {
                "text": "01234567890123456789012345678901234567890123456789"
            },
            {
                "image": {
                    "format": "jpg",
                    "source": {
                        "bytes": "01234567890123456789012345678901234567890123456789"
                    }
                }
            }
        ]
    },
    {
        "role": "assistant",
        "content": [
            {
                "text": "01234567890123456789012345678901234567890123456789"
            }
        ]
    },
    {
        "role": "user",
        "content": [
            {
                "toolResult": {
                    "toolUseId": "testid",
                    "status": "success",
                    "content": [
                        {
                            "text": "01234567890123456789012345678901234567890123456789"
                        },
                        {
                            "json": {
                                "testkey": "01234567890123456789012345678901234567890123456789"
                            }
                        },
                        {
                            "image": {
                                "format": "jpg",
                                "source": {
                                    "bytes": "01234567890123456789012345678901234567890123456789"
                                }
                            }
                        }
                    ]
                }
            }
        ]
    }
]
"#;

    #[test]
    fn test_compact_strategy_truncates_messages() {
        const TRUNCATED_TEXT: &str = "...truncated";

        // GIVEN
        let strategy = CompactStrategy {
            truncate_large_messages: true,
            max_message_length: 40,
        };
        let mut request = SendRequestArgs {
            messages: serde_json::from_str(TEST_MESSAGES).unwrap(),
            tool_specs: None,
            system_prompt: None,
        };

        // WHEN
        strategy.apply_strategy(&mut request);

        // THEN

        // assertions for first user message
        // text should be truncated, image left alone.
        let user_msg = request.messages.first().unwrap();
        let text = user_msg.content[0].text().unwrap();
        assert!(
            text.len() <= strategy.max_message_length,
            "len should be <= {}, instead found: {}",
            strategy.max_message_length,
            text
        );
        assert!(
            text.ends_with(TRUNCATED_SUFFIX),
            "should end with {}, instead found: {}",
            TRUNCATED_SUFFIX,
            text
        );
        user_msg.content[1].image().expect("should be an image");

        // assertions for second user message
        // multiple items are truncated - standard truncated suffix shouldn't entirely fit.
        let tool_result = request.messages[2].content[0].tool_result().unwrap();
        let tool_result_text = tool_result.content[0].text().unwrap();
        assert!(
            tool_result_text.len() <= strategy.max_message_length,
            "len should be <= {}, instead found: {}",
            strategy.max_message_length,
            tool_result_text
        );
        assert!(
            tool_result_text.contains(TRUNCATED_TEXT),
            "expected to find {}, instead found: {}",
            TRUNCATED_TEXT,
            tool_result_text
        );
        let tool_result_json = tool_result.content[1]
            .text()
            .expect("json should have been converted to text");
        assert!(
            tool_result_json.len() <= strategy.max_message_length,
            "len should be <= {}, instead found: {}",
            strategy.max_message_length,
            tool_result_json
        );
        assert!(
            tool_result_json.contains(TRUNCATED_TEXT),
            "expected to find {}, instead found: {}",
            TRUNCATED_TEXT,
            tool_result_json
        );
    }
}
