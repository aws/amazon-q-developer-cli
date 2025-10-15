use serde::{
    Deserialize,
    Serialize,
};

use super::agent_loop::types::Message;
use super::types::ConversationState;
use super::{
    CONTEXT_ENTRY_END_HEADER,
    CONTEXT_ENTRY_START_HEADER,
};

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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompactStrategy {
    /// Number of user/assistant pairs to exclude from the history as part of compaction.
    pub messages_to_exclude: usize,
    /// Whether or not to truncate large messages in the history.
    pub truncate_large_messages: bool,
    /// Maximum allowed size of messages in the conversation history.
    pub max_message_length: usize,
}

impl Default for CompactStrategy {
    fn default() -> Self {
        Self {
            messages_to_exclude: 0,
            truncate_large_messages: false,
            max_message_length: 25_000,
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
