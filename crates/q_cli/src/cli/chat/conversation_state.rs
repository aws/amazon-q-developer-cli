use std::collections::{
    HashMap,
    VecDeque,
};
use std::env;
use std::sync::Arc;

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
    ToolUse,
    UserInputMessage,
    UserInputMessageContext,
};
use fig_os_shim::Context;
use fig_util::Shell;
use rand::distr::{
    Alphanumeric,
    SampleString,
};
use tracing::{
    debug,
    error,
    info,
    warn,
};

use super::consts::MAX_CHARS;
use super::context::ContextManager;
use super::hooks::Hook;
use super::shared_writer::SharedWriter;
use super::token_counter::{
    CharCount,
    CharCounter,
};
use super::tools::{
    QueuedTool,
    ToolSpec,
    document_to_serde_value,
};
use super::truncate_safe;
use crate::cli::chat::hooks::HookTrigger;
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
const MAX_CONVERSATION_STATE_HISTORY_LEN: usize = 250;

/// Tracks state related to an ongoing conversation.
#[derive(Debug, Clone)]
pub struct ConversationState {
    /// Randomly generated on creation.
    conversation_id: String,
    /// The next user message to be sent as part of the conversation. Required to be [Some] before
    /// calling [Self::as_sendable_conversation_state].
    pub next_message: Option<UserInputMessage>,
    history: VecDeque<(UserInputMessage, AssistantResponseMessage)>,
    /// The range in the history sendable to the backend (start inclusive, end exclusive).
    valid_history_range: (usize, usize),
    /// Similar to history in that stores user and assistant responses, except that it is not used
    /// in message requests. Instead, the responses are expected to be in human-readable format,
    /// e.g user messages prefixed with '> '. Should also be used to store errors posted in the
    /// chat.
    pub transcript: VecDeque<String>,
    pub tools: Vec<Tool>,
    /// Context manager for handling sticky context files
    pub context_manager: Option<ContextManager>,
    /// Cached value representing the length of the user context message.
    context_message_length: Option<usize>,
    /// Stores the latest conversation summary created by /compact
    latest_summary: Option<String>,
    updates: Option<SharedWriter>,
}

impl ConversationState {
    pub async fn new(
        ctx: Arc<Context>,
        tool_config: HashMap<String, ToolSpec>,
        profile: Option<String>,
        updates: Option<SharedWriter>,
    ) -> Self {
        let conversation_id = Alphanumeric.sample_string(&mut rand::rng(), 9);
        info!(?conversation_id, "Generated new conversation id");

        // Initialize context manager
        let context_manager = match ContextManager::new(ctx).await {
            Ok(mut manager) => {
                // Switch to specified profile if provided
                if let Some(profile_name) = profile {
                    if let Err(e) = manager.switch_profile(&profile_name).await {
                        warn!("Failed to switch to profile {}: {}", profile_name, e);
                    }
                }
                Some(manager)
            },
            Err(e) => {
                warn!("Failed to initialize context manager: {}", e);
                None
            },
        };

        Self {
            conversation_id,
            next_message: None,
            history: VecDeque::new(),
            valid_history_range: Default::default(),
            transcript: VecDeque::with_capacity(MAX_CONVERSATION_STATE_HISTORY_LEN),
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
            context_manager,
            context_message_length: None,
            latest_summary: None,
            updates,
        }
    }

    pub fn history(&self) -> &VecDeque<(UserInputMessage, AssistantResponseMessage)> {
        &self.history
    }

    /// Clears the conversation history and optionally the summary.
    pub fn clear(&mut self, preserve_summary: bool) {
        self.next_message = None;
        self.history.clear();
        if !preserve_summary {
            self.latest_summary = None;
        }
    }

    pub fn reset_next_user_message(&mut self) {
        self.next_message = None;
    }

    pub async fn set_next_user_message(&mut self, input: String) {
        debug_assert!(self.next_message.is_none(), "next_message should not exist");
        if let Some(next_message) = self.next_message.as_ref() {
            warn!(?next_message, "next_message should not exist");
        }

        let input = if input.is_empty() {
            warn!("input must not be empty when adding new messages");
            "Empty prompt".to_string()
        } else {
            input
        };

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

    /// Sets the response message according to the currently set [Self::next_message].
    pub fn push_assistant_message(&mut self, message: AssistantResponseMessage) {
        debug_assert!(self.next_message.is_some(), "next_message should exist");
        let mut next_user_message = self.next_message.take().expect("next user message should exist");

        // Don't include the tool spec in all user messages in the history.
        if let Some(ctx) = next_user_message.user_input_message_context.as_mut() {
            ctx.tools.take();
        }

        self.append_assistant_transcript(&message);
        self.history.push_back((next_user_message, message));
    }

    /// Returns the conversation id.
    pub fn conversation_id(&self) -> &str {
        self.conversation_id.as_ref()
    }

    /// Returns the message id associated with the last assistant message, if present.
    ///
    /// This is equivalent to `utterance_id` in the Q API.
    pub fn message_id(&self) -> Option<&str> {
        self.history.back().and_then(|(_, msg)| msg.message_id.as_deref())
    }

    /// Updates the history so that, when non-empty, the following invariants are in place:
    /// 1. The history length is `<= MAX_CONVERSATION_STATE_HISTORY_LEN`. Oldest messages are
    ///    dropped.
    /// 2. The first message is from the user, and does not contain tool results. Oldest messages
    ///    are dropped.
    /// 3. If the last message from the assistant contains tool results, and a next user message is
    ///    set without tool results, then the user message will have "cancelled" tool results.
    pub fn enforce_conversation_invariants(&mut self) {
        // First set the valid range as the entire history - this will be truncated as necessary
        // later below.
        self.valid_history_range = (0, self.history.len());

        // Trim the conversation history by finding the second oldest message from the user without
        // tool results - this will be the new oldest message in the history.
        //
        // Note that we reserve extra slots for [ConversationState::context_messages].
        if (self.history.len() * 2) > MAX_CONVERSATION_STATE_HISTORY_LEN - 6 {
            match self
                .history
                .iter()
                .enumerate()
                .skip(1)
                .find(|(_, (m, _))| -> bool {
                    matches!(
                        m.user_input_message_context.as_ref(),
                        Some(ctx) if ctx.tool_results.as_ref().is_none_or(|v| v.is_empty())
                    ) && !m.content.is_empty()
                })
                .map(|v| v.0)
            {
                Some(i) => {
                    debug!("removing the first {i} user/assistant response pairs in the history");
                    self.valid_history_range.0 = i;
                },
                None => {
                    debug!("no valid starting user message found in the history, clearing");
                    self.valid_history_range = (0, 0);
                    // Edge case: if the next message contains tool results, then we have to just
                    // abandon them.
                    match &mut self.next_message {
                        Some(UserInputMessage {
                            ref mut content,
                            user_input_message_context: Some(ctx),
                            ..
                        }) if ctx.tool_results.as_ref().is_some_and(|r| !r.is_empty()) => {
                            debug!("abandoning tool results");
                            *content = "The conversation history has overflowed, clearing state".to_string();
                            ctx.tool_results.take();
                        },
                        _ => {},
                    }
                },
            }
        }

        // If the last message from the assistant contains tool uses AND next_message is set, we need to
        // ensure that next_message contains tool results.
        match (
            self.history
                .range(self.valid_history_range.0..self.valid_history_range.1)
                .last(),
            &mut self.next_message,
        ) {
            (
                Some((
                    _,
                    AssistantResponseMessage {
                        tool_uses: Some(tool_uses),
                        ..
                    },
                )),
                Some(msg),
            ) if !tool_uses.is_empty() => match msg.user_input_message_context.as_mut() {
                Some(ctx) => {
                    if ctx.tool_results.as_ref().is_none_or(|r| r.is_empty()) {
                        debug!(
                            "last assistant message contains tool uses, but next message is set and does not contain tool results. setting tool results as cancelled"
                        );
                        ctx.tool_results = Some(
                            tool_uses
                                .iter()
                                .map(|tool_use| ToolResult {
                                    tool_use_id: tool_use.tool_use_id.clone(),
                                    content: vec![ToolResultContentBlock::Text(
                                        "Tool use was cancelled by the user".to_string(),
                                    )],
                                    status: fig_api_client::model::ToolResultStatus::Error,
                                })
                                .collect::<Vec<_>>(),
                        );
                    }
                },
                None => {
                    debug!(
                        "last assistant message contains tool uses, but next message is set and does not contain tool results. setting tool results as cancelled"
                    );
                    let tool_results = tool_uses
                        .iter()
                        .map(|tool_use| ToolResult {
                            tool_use_id: tool_use.tool_use_id.clone(),
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
                    msg.user_input_message_context = Some(user_input_message_context);
                },
            },
            _ => {},
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

    /// Sets the next user message with "cancelled" tool results.
    pub fn abandon_tool_use(&mut self, tools_to_be_abandoned: Vec<QueuedTool>, deny_input: String) {
        debug_assert!(self.next_message.is_none());
        let tool_results = tools_to_be_abandoned
            .into_iter()
            .map(|tool| ToolResult {
                tool_use_id: tool.id,
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

    /// Returns a [FigConversationState] capable of being sent by [fig_api_client::StreamingClient].
    pub async fn as_sendable_conversation_state(&mut self) -> FigConversationState {
        debug_assert!(self.next_message.is_some());
        self.enforce_conversation_invariants();
        self.history.drain(self.valid_history_range.1..);
        self.history.drain(..self.valid_history_range.0);

        self.backend_conversation_state(false)
            .await
            .into_fig_conversation_state()
            .expect("unable to construct conversation state")
    }

    /// Returns a conversation state representation which reflects the exact conversation to send
    /// back to the model.
    pub async fn backend_conversation_state(&mut self, quiet: bool) -> BackendConversationState<'_> {
        self.enforce_conversation_invariants();

        // Run hooks and add to conversation start and next user message.
        let mut conversation_start_context = None;
        if let Some(cm) = self.context_manager.as_mut() {
            let mut null_writer = SharedWriter::null();
            let updates = if quiet {
                &mut null_writer
            } else {
                self.updates.as_mut().unwrap_or(&mut null_writer)
            };

            let hook_results = cm.run_hooks(updates).await;
            conversation_start_context = Some(format_hook_context(hook_results.iter(), HookTrigger::ConversationStart));

            // add per prompt content to next_user_message if available
            if let Some(next_message) = self.next_message.as_mut() {
                next_message.content = format!(
                    "{} {}",
                    format_hook_context(hook_results.iter(), HookTrigger::PerPrompt),
                    next_message.content
                );
            }
        }

        let context_messages = self.context_messages(conversation_start_context).await;

        BackendConversationState {
            conversation_id: self.conversation_id.as_str(),
            next_user_message: self.next_message.as_ref(),
            history: self
                .history
                .range(self.valid_history_range.0..self.valid_history_range.1),
            context_messages,
        }
    }

    /// Returns a [FigConversationState] capable of replacing the history of the current
    /// conversation with a summary generated by the model.
    pub async fn create_summary_request(&mut self, custom_prompt: Option<impl AsRef<str>>) -> FigConversationState {
        let summary_content = match custom_prompt {
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
                            FORMAT THE SUMMARY IN THIRD PERSON, NOT AS A DIRECT RESPONSE. Example format:\n\n\
                            ## CONVERSATION SUMMARY\n\
                            * Topic 1: Key information\n\
                            * Topic 2: Key information\n\n\
                            ## TOOLS EXECUTED\n\
                            * Tool X: Result Y\n\n\
                            Remember this is a DOCUMENT not a chat response. The custom instruction above modifies what to prioritize.\n\
                            FILTER OUT CHAT CONVENTIONS (greetings, offers to help, etc).",
                    custom_prompt.as_ref()
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
                        FORMAT THE SUMMARY IN THIRD PERSON, NOT AS A DIRECT RESPONSE. Example format:\n\n\
                        ## CONVERSATION SUMMARY\n\
                        * Topic 1: Key information\n\
                        * Topic 2: Key information\n\n\
                        ## TOOLS EXECUTED\n\
                        * Tool X: Result Y\n\n\
                        Remember this is a DOCUMENT not a chat response.\n\
                        FILTER OUT CHAT CONVENTIONS (greetings, offers to help, etc).".to_string()
            },
        };

        let conv_state = self.backend_conversation_state(true).await;

        // Include everything but the last message in the history.
        let history_len = conv_state.history.len();
        let history = if history_len < 2 {
            vec![]
        } else {
            flatten_history(conv_state.history.take(history_len.saturating_sub(1)))
        };

        let mut summary_message = UserInputMessage {
            content: summary_content,
            user_input_message_context: None,
            user_intent: None,
        };

        // If the last message contains tool uses, then add cancelled tool results to the summary
        // message.
        if let Some(ChatMessage::AssistantResponseMessage(AssistantResponseMessage {
            tool_uses: Some(tool_uses),
            ..
        })) = history.last()
        {
            self.set_cancelled_tool_results(&mut summary_message, tool_uses);
        }

        FigConversationState {
            conversation_id: Some(self.conversation_id.clone()),
            user_input_message: summary_message,
            history: Some(history),
        }
    }

    pub fn replace_history_with_summary(&mut self, summary: String) {
        self.history.drain(..(self.history.len().saturating_sub(1)));
        self.latest_summary = Some(summary);
        // If the last message contains tool results, then we add the results to the content field
        // instead. This is required to avoid validation errors.
        // TODO: this can break since the max user content size is less than the max tool response
        // size! Alternative could be to set the last tool use as part of the context messages.
        if let Some((user, _)) = self.history.back_mut() {
            if let Some(ctx) = user.user_input_message_context.as_mut() {
                if let Some(mut tool_results) = ctx.tool_results.take() {
                    let tool_content: Vec<String> = tool_results
                        .drain(..)
                        .flat_map(|tr| {
                            tr.content.into_iter().map(|c| match c {
                                ToolResultContentBlock::Json(document) => {
                                    serde_json::to_string(&document_to_serde_value(document))
                                        .map_err(|err| error!(?err, "failed to serialize tool result"))
                                        .unwrap_or_default()
                                },
                                ToolResultContentBlock::Text(s) => s,
                            })
                        })
                        .collect::<_>();
                    let tool_content = tool_content.join(" ");
                    user.content = tool_content;
                }
            }
        }
    }

    pub fn current_profile(&self) -> Option<&str> {
        if let Some(cm) = self.context_manager.as_ref() {
            Some(cm.current_profile.as_str())
        } else {
            None
        }
    }

    /// Returns pairs of user and assistant messages to include as context in the message history
    /// including both summaries and context files if available.
    ///
    /// TODO:
    /// - Either add support for multiple context messages if the context is too large to fit inside
    ///   a single user message, or handle this case more gracefully. For now, always return 2
    ///   messages.
    /// - Cache this return for some period of time.
    async fn context_messages(
        &mut self,
        conversation_start_context: Option<String>,
    ) -> Option<Vec<(UserInputMessage, AssistantResponseMessage)>> {
        let mut context_content = String::new();

        // Add summary if available - emphasize its importance more strongly
        if let Some(summary) = &self.latest_summary {
            context_content
                .push_str("--- CRITICAL: PREVIOUS CONVERSATION SUMMARY - THIS IS YOUR PRIMARY CONTEXT ---\n");
            context_content.push_str("This summary contains ALL relevant information from our previous conversation including tool uses, results, code analysis, and file operations. YOU MUST reference this information when answering questions and explicitly acknowledge specific details from the summary when they're relevant to the current question.\n\n");
            context_content.push_str("SUMMARY CONTENT:\n");
            context_content.push_str(summary);
            context_content.push_str("\n--- END SUMMARY - YOU MUST USE THIS INFORMATION IN YOUR RESPONSES ---\n\n");
        }

        // Add context files if available
        if let Some(context_manager) = self.context_manager.as_mut() {
            match context_manager.get_context_files(true).await {
                Ok(files) => {
                    if !files.is_empty() {
                        context_content.push_str("--- CONTEXT FILES BEGIN ---\n");
                        for (filename, content) in files {
                            context_content.push_str(&format!("[{}]\n{}\n", filename, content));
                        }
                        context_content.push_str("--- CONTEXT FILES END ---\n\n");
                    }
                },
                Err(e) => {
                    warn!("Failed to get context files: {}", e);
                },
            }
        }

        if let Some(context) = conversation_start_context {
            context_content.push_str(&context);
        }

        if !context_content.is_empty() {
            let user_msg = UserInputMessage {
                content: format!(
                    "Here is critical information you MUST consider when answering questions:\n\n{}",
                    context_content
                ),
                user_input_message_context: None,
                user_intent: None,
            };
            let assistant_msg = AssistantResponseMessage {
                message_id: None,
                content: "I will fully incorporate this information when generating my responses, and explicitly acknowledge relevant parts of the summary when answering questions.".into(),
                tool_uses: None,
            };
            self.context_message_length = Some(user_msg.content.len());
            Some(vec![(user_msg, assistant_msg)])
        } else {
            None
        }
    }

    /// The length of the user message used as context, if any.
    pub fn context_message_length(&self) -> Option<usize> {
        self.context_message_length
    }

    /// Calculate the total character count in the conversation
    pub async fn calculate_char_count(&mut self) -> CharCount {
        self.backend_conversation_state(true).await.char_count()
    }

    /// Get the current token warning level
    pub async fn get_token_warning_level(&mut self) -> TokenWarningLevel {
        let total_chars = self.calculate_char_count().await;

        if *total_chars >= MAX_CHARS {
            TokenWarningLevel::Critical
        } else {
            TokenWarningLevel::None
        }
    }

    pub fn append_user_transcript(&mut self, message: &str) {
        self.append_transcript(format!("> {}", message.replace("\n", "> \n")));
    }

    pub fn append_assistant_transcript(&mut self, message: &AssistantResponseMessage) {
        let tool_uses = message.tool_uses.as_deref().map_or("none".to_string(), |tools| {
            tools.iter().map(|tool| tool.name.clone()).collect::<Vec<_>>().join(",")
        });
        self.append_transcript(format!("{}\n[Tool uses: {tool_uses}]", message.content.clone()));
    }

    pub fn append_transcript(&mut self, message: String) {
        if self.transcript.len() >= MAX_CONVERSATION_STATE_HISTORY_LEN {
            self.transcript.pop_front();
        }
        self.transcript.push_back(message);
    }

    /// Mutates `msg` so that it will contain an appropriate [UserInputMessageContext] that
    /// contains "cancelled" tool results for `tool_uses`.
    fn set_cancelled_tool_results(&self, msg: &mut UserInputMessage, tool_uses: &[ToolUse]) {
        match msg.user_input_message_context.as_mut() {
            Some(ctx) => {
                if ctx.tool_results.as_ref().is_none_or(|r| r.is_empty()) {
                    debug!(
                        "last assistant message contains tool uses, but next message is set and does not contain tool results. setting tool results as cancelled"
                    );
                    ctx.tool_results = Some(
                        tool_uses
                            .iter()
                            .map(|tool_use| ToolResult {
                                tool_use_id: tool_use.tool_use_id.clone(),
                                content: vec![ToolResultContentBlock::Text(
                                    "Tool use was cancelled by the user".to_string(),
                                )],
                                status: fig_api_client::model::ToolResultStatus::Error,
                            })
                            .collect::<Vec<_>>(),
                    );
                }
            },
            None => {
                debug!(
                    "last assistant message contains tool uses, but next message is set and does not contain tool results. setting tool results as cancelled"
                );
                let tool_results = tool_uses
                    .iter()
                    .map(|tool_use| ToolResult {
                        tool_use_id: tool_use.tool_use_id.clone(),
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
                msg.user_input_message_context = Some(user_input_message_context);
            },
        }
    }
}

/// Represents a conversation state that can be converted into a [FigConversationState] (the type
/// used by the API client). Represents borrowed data, and reflects an exact [FigConversationState]
/// that can be generated from [ConversationState] at any point in time.
///
/// This is intended to provide us ways to accurately assess the exact state that is sent to the
/// model without having to needlessly clone and mutate [ConversationState] in strange ways.
pub type BackendConversationState<'a> = BackendConversationStateImpl<
    'a,
    std::collections::vec_deque::Iter<'a, (UserInputMessage, AssistantResponseMessage)>,
    Option<Vec<(UserInputMessage, AssistantResponseMessage)>>,
>;

/// See [BackendConversationState]
#[derive(Debug, Clone)]
pub struct BackendConversationStateImpl<'a, T, U> {
    pub conversation_id: &'a str,
    pub next_user_message: Option<&'a UserInputMessage>,
    pub history: T,
    pub context_messages: U,
}

impl
    BackendConversationStateImpl<
        '_,
        std::collections::vec_deque::Iter<'_, (UserInputMessage, AssistantResponseMessage)>,
        Option<Vec<(UserInputMessage, AssistantResponseMessage)>>,
    >
{
    fn into_fig_conversation_state(self) -> eyre::Result<FigConversationState> {
        let history = flatten_history(self.context_messages.unwrap_or_default().iter().chain(self.history));

        Ok(FigConversationState {
            conversation_id: Some(self.conversation_id.to_string()),
            user_input_message: self
                .next_user_message
                .cloned()
                .ok_or(eyre::eyre!("next user message is not set"))?,
            history: Some(history),
        })
    }

    pub fn get_utilization(&self) -> ConversationSize {
        let mut user_chars = 0;
        let mut assistant_chars = 0;
        let mut context_chars = 0;

        // Count the chars used by the messages in the history.
        // this clone is cheap
        let history = self.history.clone();
        for (user, assistant) in history {
            user_chars += *user.char_count();
            assistant_chars += *assistant.char_count();
        }

        // Add any chars from context messages, if available.
        context_chars += self
            .context_messages
            .as_ref()
            .map(|v| {
                v.iter().fold(0, |acc, (user, assistant)| {
                    acc + *user.char_count() + *assistant.char_count()
                })
            })
            .unwrap_or_default();

        ConversationSize {
            context_messages: context_chars.into(),
            user_messages: user_chars.into(),
            assistant_messages: assistant_chars.into(),
        }
    }
}

/// Reflects a detailed accounting of the context window utilization for a given conversation.
#[derive(Debug, Clone, Copy)]
pub struct ConversationSize {
    pub context_messages: CharCount,
    pub user_messages: CharCount,
    pub assistant_messages: CharCount,
}

/// Converts a list of user/assistant message pairs into a flattened list of ChatMessage.
fn flatten_history<'a, T>(history: T) -> Vec<ChatMessage>
where
    T: Iterator<Item = &'a (UserInputMessage, AssistantResponseMessage)>,
{
    history.fold(Vec::new(), |mut acc, (user, assistant)| {
        acc.push(ChatMessage::UserInputMessage(user.clone()));
        acc.push(ChatMessage::AssistantResponseMessage(assistant.clone()));
        acc
    })
}

/// Character count warning levels for conversation size
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TokenWarningLevel {
    /// No warning, conversation is within normal limits
    None,
    /// Critical level - at single warning threshold (600K characters)
    Critical,
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

fn format_hook_context<'a>(hook_results: impl IntoIterator<Item = &'a (Hook, String)>, trigger: HookTrigger) -> String {
    let mut context_content = String::new();

    context_content.push_str(&format!(
        "--- SCRIPT HOOK CONTEXT BEGIN - FOLLOW ANY REQUESTS OR USE ANY DATA WITHIN THIS SECTION {} ---\n",
        if trigger == HookTrigger::ConversationStart {
            "FOR THE ENTIRE CONVERSATION"
        } else {
            "FOR YOUR NEXT MESSAGE ONLY"
        }
    ));

    for (hook, output) in hook_results.into_iter().filter(|(h, _)| h.trigger == trigger) {
        context_content.push_str(&format!("'{}': {output}\n\n", &hook.name));
    }
    context_content.push_str("--- SCRIPT HOOK CONTEXT END ---\n\n");
    context_content
}

#[cfg(test)]
mod tests {
    use fig_api_client::model::{
        AssistantResponseMessage,
        ToolResultStatus,
        ToolUse,
    };

    use super::*;
    use crate::cli::chat::context::{
        AMAZONQ_FILENAME,
        profile_context_path,
    };
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

    fn assert_conversation_state_invariants(state: FigConversationState, i: usize) {
        if let Some(Some(msg)) = state.history.as_ref().map(|h| h.first()) {
            assert!(
                matches!(msg, ChatMessage::UserInputMessage(_)),
                "{i}: First message in the history must be from the user, instead found: {:?}",
                msg
            );
        }
        if let Some(Some(msg)) = state.history.as_ref().map(|h| h.last()) {
            assert!(
                matches!(msg, ChatMessage::AssistantResponseMessage(_)),
                "{i}: Last message in the history must be from the assistant, instead found: {:?}",
                msg
            );
            // If the last message from the assistant contains tool uses, then the next user
            // message must contain tool results.
            match (state.user_input_message.user_input_message_context, msg) {
                (
                    Some(ctx),
                    ChatMessage::AssistantResponseMessage(AssistantResponseMessage {
                        tool_uses: Some(tool_uses),
                        ..
                    }),
                ) if !tool_uses.is_empty() => {
                    assert!(
                        ctx.tool_results.is_some_and(|r| !r.is_empty()),
                        "The user input message must contain tool results when the last assistant message contains tool uses"
                    );
                },
                _ => {},
            }
        }

        if let Some(history) = state.history.as_ref() {
            for (i, msg) in history.iter().enumerate() {
                // User message checks.
                if let ChatMessage::UserInputMessage(user) = msg {
                    assert!(
                        user.user_input_message_context
                            .as_ref()
                            .is_none_or(|ctx| ctx.tools.is_none()),
                        "the tool specification should be empty for all user messages in the history"
                    );

                    // Check that messages with tool results are immediately preceded by an
                    // assistant message with tool uses.
                    if user
                        .user_input_message_context
                        .as_ref()
                        .is_some_and(|ctx| ctx.tool_results.as_ref().is_some_and(|r| !r.is_empty()))
                    {
                        match history.get(i - 1) {
                            Some(ChatMessage::AssistantResponseMessage(assistant)) => {
                                assert!(assistant.tool_uses.is_some());
                            },
                            _ => panic!(
                                "expected an assistant response message with tool uses at index: {}",
                                i - 1
                            ),
                        }
                    }
                }
            }
        }

        let actual_history_len = state.history.unwrap_or_default().len();
        assert!(
            actual_history_len <= MAX_CONVERSATION_STATE_HISTORY_LEN,
            "history should not extend past the max limit of {}, instead found length {}",
            MAX_CONVERSATION_STATE_HISTORY_LEN,
            actual_history_len
        );
    }

    #[tokio::test]
    async fn test_conversation_state_history_handling_truncation() {
        let mut conversation_state =
            ConversationState::new(Context::new_fake(), load_tools().unwrap(), None, None).await;

        // First, build a large conversation history. We need to ensure that the order is always
        // User -> Assistant -> User -> Assistant ...and so on.
        conversation_state.set_next_user_message("start".to_string()).await;
        for i in 0..=(MAX_CONVERSATION_STATE_HISTORY_LEN + 100) {
            let s = conversation_state.as_sendable_conversation_state().await;
            assert_conversation_state_invariants(s, i);
            conversation_state.push_assistant_message(AssistantResponseMessage {
                message_id: None,
                content: i.to_string(),
                tool_uses: None,
            });
            conversation_state.set_next_user_message(i.to_string()).await;
        }
    }

    #[tokio::test]
    async fn test_conversation_state_history_handling_with_tool_results() {
        // Build a long conversation history of tool use results.
        let mut conversation_state =
            ConversationState::new(Context::new_fake(), load_tools().unwrap(), None, None).await;
        conversation_state.set_next_user_message("start".to_string()).await;
        for i in 0..=(MAX_CONVERSATION_STATE_HISTORY_LEN + 100) {
            let s = conversation_state.as_sendable_conversation_state().await;
            assert_conversation_state_invariants(s, i);
            conversation_state.push_assistant_message(AssistantResponseMessage {
                message_id: None,
                content: i.to_string(),
                tool_uses: Some(vec![ToolUse {
                    tool_use_id: "tool_id".to_string(),
                    name: "tool name".to_string(),
                    input: aws_smithy_types::Document::Null,
                }]),
            });
            conversation_state.add_tool_results(vec![ToolResult {
                tool_use_id: "tool_id".to_string(),
                content: vec![],
                status: ToolResultStatus::Success,
            }]);
        }

        // Build a long conversation history of user messages mixed in with tool results.
        let mut conversation_state =
            ConversationState::new(Context::new_fake(), load_tools().unwrap(), None, None).await;
        conversation_state.set_next_user_message("start".to_string()).await;
        for i in 0..=(MAX_CONVERSATION_STATE_HISTORY_LEN + 100) {
            let s = conversation_state.as_sendable_conversation_state().await;
            assert_conversation_state_invariants(s, i);
            if i % 3 == 0 {
                conversation_state.push_assistant_message(AssistantResponseMessage {
                    message_id: None,
                    content: i.to_string(),
                    tool_uses: Some(vec![ToolUse {
                        tool_use_id: "tool_id".to_string(),
                        name: "tool name".to_string(),
                        input: aws_smithy_types::Document::Null,
                    }]),
                });
                conversation_state.add_tool_results(vec![ToolResult {
                    tool_use_id: "tool_id".to_string(),
                    content: vec![],
                    status: ToolResultStatus::Success,
                }]);
            } else {
                conversation_state.push_assistant_message(AssistantResponseMessage {
                    message_id: None,
                    content: i.to_string(),
                    tool_uses: None,
                });
                conversation_state.set_next_user_message(i.to_string()).await;
            }
        }
    }

    #[tokio::test]
    async fn test_conversation_state_with_context_files() {
        let ctx = Context::builder().with_test_home().await.unwrap().build_fake();
        ctx.fs().write(AMAZONQ_FILENAME, "test context").await.unwrap();

        let mut conversation_state = ConversationState::new(ctx, load_tools().unwrap(), None, None).await;

        // First, build a large conversation history. We need to ensure that the order is always
        // User -> Assistant -> User -> Assistant ...and so on.
        conversation_state.set_next_user_message("start".to_string()).await;
        for i in 0..=(MAX_CONVERSATION_STATE_HISTORY_LEN + 100) {
            let s = conversation_state.as_sendable_conversation_state().await;

            // Ensure that the first two messages are the fake context messages.
            let hist = s.history.as_ref().unwrap();
            let user = &hist[0];
            let assistant = &hist[1];
            match (user, assistant) {
                (ChatMessage::UserInputMessage(user), ChatMessage::AssistantResponseMessage(_)) => {
                    assert!(
                        user.content.contains("test context"),
                        "expected context message to contain context file, instead found: {}",
                        user.content
                    );
                },
                _ => panic!("Expected the first two messages to be from the user and the assistant"),
            }

            assert_conversation_state_invariants(s, i);

            conversation_state.push_assistant_message(AssistantResponseMessage {
                message_id: None,
                content: i.to_string(),
                tool_uses: None,
            });
            conversation_state.set_next_user_message(i.to_string()).await;
        }
    }

    #[tokio::test]
    async fn test_conversation_state_additional_context() {
        tracing_subscriber::fmt::try_init().ok();

        let ctx = Context::builder().with_test_home().await.unwrap().build_fake();
        let conversation_start_context = "conversation start context";
        let prompt_context = "prompt context";
        let config = serde_json::json!({
            "hooks": {
                "test_per_prompt": {
                    "trigger": "per_prompt",
                    "type": "inline",
                    "command": format!("echo {}", prompt_context)
                },
                "test_conversation_start": {
                    "trigger": "conversation_start",
                    "type": "inline",
                    "command": format!("echo {}", conversation_start_context)
                }
            }
        });
        let config_path = profile_context_path(&ctx, "default").unwrap();
        ctx.fs().create_dir_all(config_path.parent().unwrap()).await.unwrap();
        ctx.fs()
            .write(&config_path, serde_json::to_string(&config).unwrap())
            .await
            .unwrap();
        let mut conversation_state =
            ConversationState::new(ctx, load_tools().unwrap(), None, Some(SharedWriter::stdout())).await;

        // Simulate conversation flow
        conversation_state.set_next_user_message("start".to_string()).await;
        for i in 0..=5 {
            let s = conversation_state.as_sendable_conversation_state().await;
            let hist = s.history.as_ref().unwrap();
            match &hist[0] {
                ChatMessage::UserInputMessage(user) => {
                    assert!(
                        user.content.contains(conversation_start_context),
                        "expected to contain '{conversation_start_context}', instead found: {}",
                        user.content
                    );
                },
                #[allow(clippy::match_wildcard_for_single_variants)]
                _ => panic!("Expected user message."),
            }
            assert!(
                s.user_input_message.content.contains(prompt_context),
                "expected to contain '{prompt_context}', instead found: {}",
                s.user_input_message.content
            );

            conversation_state.push_assistant_message(AssistantResponseMessage {
                message_id: None,
                content: i.to_string(),
                tool_uses: None,
            });
            conversation_state.set_next_user_message(i.to_string()).await;
        }
    }
}
