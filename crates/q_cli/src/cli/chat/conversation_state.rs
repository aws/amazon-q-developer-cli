use std::collections::{
    HashMap,
    VecDeque,
};
use std::env;

use fig_api_client::model::{
    AssistantResponseMessage,
    ChatMessage,
    ConversationState as FigConversationState,
    EnvState,
    ShellState,
    Tool,
    ToolInputSchema,
    ToolResult,
    ToolResultContentBlock,
    ToolSpecification,
    UserInputMessage,
    UserInputMessageContext,
};
use fig_util::Shell;
use rand::distributions::{
    Alphanumeric,
    DistString,
};
use tracing::{
    debug,
    error,
    info,
    trace,
    warn,
};

use super::tools::ToolSpec;
use crate::cli::chat::tools::{
    InputSchema,
    InvokeOutput,
    serde_value_to_document,
};

// Max constants for length of strings and lists, use these to truncate elements
// to ensure the API request is valid

// These limits are the internal undocumented values from the service for each item
const MAX_CURRENT_WORKING_DIRECTORY_LEN: usize = 256;

/// Limit to send the number of messages as part of chat.
const MAX_CONVERSATION_STATE_HISTORY_LEN: usize = 100;

/// Tracks state related to an ongoing conversation.
#[derive(Debug, Clone)]
pub struct ConversationState {
    /// Randomly generated on creation.
    conversation_id: String,
    /// The next user message to be sent as part of the conversation. Required to be [Some] before
    /// calling [Self::as_sendable_conversation_state].
    pub next_message: Option<UserInputMessage>,
    history: VecDeque<ChatMessage>,
    tools: Vec<Tool>,
}

impl ConversationState {
    pub fn new(tool_config: HashMap<String, ToolSpec>) -> Self {
        let conversation_id = Alphanumeric.sample_string(&mut rand::thread_rng(), 9);
        info!(?conversation_id, "Generated new conversation id");
        Self {
            conversation_id,
            next_message: None,
            history: VecDeque::new(),
            tools: tool_config
                .into_values()
                .map(|v| {
                    Tool::ToolSpecification(ToolSpecification {
                        name: v.name,
                        description: v.description,
                        input_schema: v.input_schema.into(),
                    })
                })
                .collect(),
        }
    }

    /// Clears the conversation history.
    pub fn clear(&mut self) {
        self.next_message = None;
        self.history.clear();
    }

    pub async fn append_new_user_message(&mut self, input: String) {
        debug_assert!(self.next_message.is_none(), "next_message should not exist");
        if let Some(next_message) = self.next_message.as_ref() {
            warn!(?next_message, "next_message should not exist");
        }

        let msg = UserInputMessage {
            content: input,
            user_input_message_context: Some(UserInputMessageContext {
                shell_state: Some(build_shell_state()),
                env_state: Some(build_env_state()),
                tool_results: None,
                tools: if self.tools.is_empty() {
                    None
                } else {
                    Some(self.tools.clone())
                },
                ..Default::default()
            }),
            user_intent: None,
        };
        self.next_message = Some(msg);
    }

    /// This should be called sometime after [Self::as_sendable_conversation_state], and before the
    /// next user message is set.
    pub fn push_assistant_message(&mut self, message: AssistantResponseMessage) {
        debug_assert!(self.next_message.is_none(), "next_message should not exist");
        if let Some(next_message) = self.next_message.as_ref() {
            warn!(?next_message, "next_message should not exist");
        }
        self.history.push_back(ChatMessage::AssistantResponseMessage(message));
    }

    /// Returns the conversation id.
    pub fn conversation_id(&self) -> &str {
        self.conversation_id.as_ref()
    }

    /// Returns the message id associated with the last assistant message, if present.
    ///
    /// This is equivalent to `utterance_id` in the Q API.
    pub fn message_id(&self) -> Option<&str> {
        self.history.iter().last().and_then(|m| match &m {
            ChatMessage::AssistantResponseMessage(m) => m.message_id.as_deref(),
            ChatMessage::UserInputMessage(_) => None,
        })
    }

    /// Updates the history so that, when non-empty, the following invariants are in place:
    /// 1. The history length is <= MAX_CONVERSATION_STATE_HISTORY_LEN if the next user message does
    ///    not contain tool results. Oldest messages are dropped.
    /// 2. The first message is from the user, and does not contain tool results. Oldest messages
    ///    are dropped.
    /// 3. The last message is from the assistant. The last message is dropped if it is from the
    ///    user.
    pub fn fix_history(&mut self) {
        if self.history.is_empty() {
            return;
        }

        // Invariant (3).
        if let Some(ChatMessage::UserInputMessage(msg)) = self.history.iter().last() {
            debug!(?msg, "last message in history is from the user, dropping");
            self.history.pop_back();
        }

        // Check if the next message contains tool results - if it does, then return early.
        // Required in the case that the entire history consists of tool results; every message is
        // therefore required to avoid validation errors in the backend.
        match self.next_message.as_ref() {
            Some(UserInputMessage {
                user_input_message_context: Some(ctx),
                ..
            }) if ctx.tool_results.as_ref().is_none_or(|r| r.is_empty()) => {
                debug!(
                    curr_history_len = self.history.len(),
                    max_history_len = MAX_CONVERSATION_STATE_HISTORY_LEN,
                    "next user message does not contain tool results, removing messages if required"
                );
            },
            _ => {
                debug!("next user message contains tool results, not modifying the history");
                return;
            },
        }

        // Invariant (1).
        while self.history.len() > MAX_CONVERSATION_STATE_HISTORY_LEN {
            self.history.pop_front();
        }

        // Invariant (2).
        match self
            .history
            .iter()
            .enumerate()
            .find(|(_, m)| -> bool {
                match m {
                    ChatMessage::UserInputMessage(m) => {
                        matches!(
                            m.user_input_message_context.as_ref(),
                            Some(ctx) if ctx.tool_results.as_ref().is_none_or(|v| v.is_empty())
                        )
                    },
                    ChatMessage::AssistantResponseMessage(_) => false,
                }
            })
            .map(|v| v.0)
        {
            Some(i) => {
                trace!("removing the first {i} elements in the history");
                self.history.drain(..i);
            },
            None => {
                trace!("no valid starting user message found in the history, clearing");
                self.history.clear();
            },
        }
    }

    pub fn add_tool_results(&mut self, tool_results: Vec<ToolResult>) {
        debug_assert!(self.next_message.is_none());
        let user_input_message_context = UserInputMessageContext {
            shell_state: None,
            env_state: Some(build_env_state()),
            tool_results: Some(tool_results),
            tools: if self.tools.is_empty() {
                None
            } else {
                Some(self.tools.clone())
            },
            ..Default::default()
        };
        let msg = UserInputMessage {
            content: String::new(),
            user_input_message_context: Some(user_input_message_context),
            user_intent: None,
        };
        self.next_message = Some(msg);
    }

    pub fn abandon_tool_use(&mut self, tools_to_be_abandoned: Vec<(String, super::tools::Tool)>, deny_input: String) {
        debug_assert!(self.next_message.is_none());
        let tool_results = tools_to_be_abandoned
            .into_iter()
            .map(|(tool_use_id, _)| ToolResult {
                tool_use_id,
                content: vec![ToolResultContentBlock::Text(
                    "Tool use was cancelled by the user".to_string(),
                )],
                status: fig_api_client::model::ToolResultStatus::Error,
            })
            .collect::<Vec<_>>();
        let user_input_message_context = UserInputMessageContext {
            shell_state: None,
            env_state: Some(build_env_state()),
            tool_results: Some(tool_results),
            tools: if self.tools.is_empty() {
                None
            } else {
                Some(self.tools.clone())
            },
            ..Default::default()
        };
        let msg = UserInputMessage {
            content: deny_input,
            user_input_message_context: Some(user_input_message_context),
            user_intent: None,
        };
        self.next_message = Some(msg);
    }

    /// Returns a [FigConversationState] capable of being sent by
    /// [fig_api_client::StreamingClient] while preparing the current conversation state to be sent
    /// in the next message.
    pub fn as_sendable_conversation_state(&mut self) -> FigConversationState {
        debug_assert!(self.next_message.is_some());
        self.fix_history();

        // The current state we want to send
        let curr_state = self.clone();

        // Updating `self` so that the current next_message is moved to history.
        let mut last_message = self.next_message.take().unwrap();
        if let Some(ctx) = &mut last_message.user_input_message_context {
            // Don't include the tool spec in all user messages in the history.
            ctx.tools.take();
        }
        self.history.push_back(ChatMessage::UserInputMessage(last_message));

        FigConversationState {
            conversation_id: Some(curr_state.conversation_id),
            user_input_message: curr_state.next_message.expect("no user input message available"),
            history: Some(curr_state.history.into()),
        }
    }
}

impl From<InvokeOutput> for ToolResultContentBlock {
    fn from(value: InvokeOutput) -> Self {
        match value.output {
            crate::cli::chat::tools::OutputKind::Text(text) => Self::Text(text),
            crate::cli::chat::tools::OutputKind::Json(value) => Self::Json(serde_value_to_document(value)),
        }
    }
}

impl From<InputSchema> for ToolInputSchema {
    fn from(value: InputSchema) -> Self {
        Self {
            json: Some(serde_value_to_document(value.0)),
        }
    }
}

fn build_env_state() -> EnvState {
    let mut env_state = EnvState {
        operating_system: Some(env::consts::OS.into()),
        ..Default::default()
    };

    match env::current_dir() {
        Ok(current_dir) => {
            env_state.current_working_directory =
                Some(truncate_safe(&current_dir.to_string_lossy(), MAX_CURRENT_WORKING_DIRECTORY_LEN).into());
        },
        Err(err) => {
            error!(?err, "Attempted to fetch the CWD but it did not exist.");
        },
    }

    env_state
}

fn build_shell_state() -> ShellState {
    // Try to grab the shell from the parent process via the `Shell::current_shell`,
    // then try the `SHELL` env, finally just report bash
    let shell_name = Shell::current_shell()
        .or_else(|| {
            let shell_name = env::var("SHELL").ok()?;
            Shell::try_find_shell(shell_name)
        })
        .unwrap_or(Shell::Bash)
        .to_string();

    ShellState {
        shell_name,
        shell_history: None,
    }
}

fn truncate_safe(s: &str, max_bytes: usize) -> &str {
    if s.len() <= max_bytes {
        return s;
    }

    let mut byte_count = 0;
    let mut char_indices = s.char_indices();

    for (byte_idx, _) in &mut char_indices {
        if byte_count + (byte_idx - byte_count) > max_bytes {
            break;
        }
        byte_count = byte_idx;
    }

    &s[..byte_count]
}

#[cfg(test)]
mod tests {
    use fig_api_client::model::{
        AssistantResponseMessage,
        ToolResultStatus,
    };

    use super::*;
    use crate::cli::chat::load_tools;

    #[test]
    fn test_truncate_safe() {
        assert_eq!(truncate_safe("Hello World", 5), "Hello");
        assert_eq!(truncate_safe("Hello ", 5), "Hello");
        assert_eq!(truncate_safe("Hello World", 11), "Hello World");
        assert_eq!(truncate_safe("Hello World", 15), "Hello World");
    }

    #[test]
    fn test_env_state() {
        let env_state = build_env_state();
        assert!(env_state.current_working_directory.is_some());
        assert!(env_state.operating_system.as_ref().is_some_and(|os| !os.is_empty()));
        println!("{env_state:?}");
    }

    #[tokio::test]
    async fn test_conversation_state_history_handling() {
        let mut conversation_state = ConversationState::new(load_tools().unwrap());

        // First, build a large conversation history. We need to ensure that the order is always
        // User -> Assistant -> User -> Assistant ...and so on.
        conversation_state.append_new_user_message("start".to_string()).await;
        for i in 0..=100 {
            let s = conversation_state.as_sendable_conversation_state();
            assert!(
                s.history
                    .as_ref()
                    .is_none_or(|h| h.first().is_none_or(|m| matches!(m, ChatMessage::UserInputMessage(_)))),
                "First message in the history must be from the user"
            );
            assert!(
                s.history.as_ref().is_none_or(|h| h
                    .last()
                    .is_none_or(|m| matches!(m, ChatMessage::AssistantResponseMessage(_)))),
                "Last message in the history must be from the assistant"
            );
            conversation_state.push_assistant_message(AssistantResponseMessage {
                message_id: None,
                content: i.to_string(),
                tool_uses: None,
            });
            conversation_state.append_new_user_message(i.to_string()).await;
        }

        let s = conversation_state.as_sendable_conversation_state();
        assert_eq!(
            s.history.as_ref().unwrap().len(),
            MAX_CONVERSATION_STATE_HISTORY_LEN,
            "history should be capped at {}",
            MAX_CONVERSATION_STATE_HISTORY_LEN
        );
        let first_msg = s.history.as_ref().unwrap().first().unwrap();
        match first_msg {
            ChatMessage::UserInputMessage(_) => {},
            other @ ChatMessage::AssistantResponseMessage(_) => {
                panic!("First message should be from the user, instead found {:?}", other)
            },
        }
        let last_msg = s.history.as_ref().unwrap().iter().last().unwrap();
        match last_msg {
            ChatMessage::AssistantResponseMessage(assistant_response_message) => {
                assert_eq!(assistant_response_message.content, "100");
            },
            other @ ChatMessage::UserInputMessage(_) => {
                panic!("Last message should be from the assistant, instead found {:?}", other)
            },
        }
    }

    #[tokio::test]
    async fn test_conversation_state_history_handling_with_tool_results() {
        let mut conversation_state = ConversationState::new(load_tools().unwrap());

        // Build a long conversation history of tool use results.
        conversation_state.append_new_user_message("start".to_string()).await;
        for i in 0..=(MAX_CONVERSATION_STATE_HISTORY_LEN + 100) {
            let _ = conversation_state.as_sendable_conversation_state();
            conversation_state.push_assistant_message(AssistantResponseMessage {
                message_id: None,
                content: i.to_string(),
                tool_uses: None,
            });
            conversation_state.add_tool_results(vec![ToolResult {
                tool_use_id: "tool_id".to_string(),
                content: vec![],
                status: ToolResultStatus::Success,
            }]);
        }

        let s = conversation_state.as_sendable_conversation_state();
        let actual_history_len = s.history.as_ref().unwrap().len();
        assert!(
            actual_history_len > MAX_CONVERSATION_STATE_HISTORY_LEN,
            "history should extend past the max limit of {}, instead found length {}",
            MAX_CONVERSATION_STATE_HISTORY_LEN,
            actual_history_len
        );
    }
}
