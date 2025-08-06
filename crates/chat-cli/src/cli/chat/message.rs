use std::collections::HashMap;
use std::env;

use serde::{
    Deserialize,
    Serialize,
};
use tracing::{
    error,
    warn,
};

use super::consts::{
    MAX_CURRENT_WORKING_DIRECTORY_LEN,
    MAX_USER_MESSAGE_SIZE,
};
use super::tools::{
    InvokeOutput,
    OutputKind,
    ToolOrigin,
};
use super::util::{
    document_to_serde_value,
    serde_value_to_document,
    truncate_safe,
    truncate_safe_in_place,
};
use crate::api_client::model::{
    AssistantResponseMessage,
    EnvState,
    ImageBlock,
    Tool,
    ToolResult,
    ToolResultContentBlock,
    ToolResultStatus,
    ToolUse,
    UserInputMessage,
    UserInputMessageContext,
};

const USER_ENTRY_START_HEADER: &str = "--- USER MESSAGE BEGIN ---\n";
const USER_ENTRY_END_HEADER: &str = "--- USER MESSAGE END ---\n\n";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserMessage {
    pub additional_context: String,
    pub env_context: UserEnvContext,
    pub content: UserMessageContent,
    pub images: Option<Vec<ImageBlock>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum UserMessageContent {
    Prompt {
        /// The original prompt as input by the user.
        prompt: String,
    },
    CancelledToolUses {
        /// The original prompt as input by the user, if any.
        prompt: Option<String>,
        tool_use_results: Vec<ToolUseResult>,
    },
    ToolUseResults {
        tool_use_results: Vec<ToolUseResult>,
    },
}

impl UserMessageContent {
    pub const TRUNCATED_SUFFIX: &str = "...content truncated due to length";

    fn truncate_safe(&mut self, max_bytes: usize) {
        match self {
            UserMessageContent::Prompt { prompt } => {
                truncate_safe_in_place(prompt, max_bytes, Self::TRUNCATED_SUFFIX);
            },
            UserMessageContent::CancelledToolUses {
                prompt,
                tool_use_results,
            } => {
                if let Some(prompt) = prompt {
                    truncate_safe_in_place(prompt, max_bytes / 2, Self::TRUNCATED_SUFFIX);
                    truncate_safe_tool_use_results(
                        tool_use_results.as_mut_slice(),
                        max_bytes / 2,
                        Self::TRUNCATED_SUFFIX,
                    );
                } else {
                    truncate_safe_tool_use_results(tool_use_results.as_mut_slice(), max_bytes, Self::TRUNCATED_SUFFIX);
                }
            },
            UserMessageContent::ToolUseResults { tool_use_results } => {
                truncate_safe_tool_use_results(tool_use_results.as_mut_slice(), max_bytes, Self::TRUNCATED_SUFFIX);
            },
        }
    }
}

impl UserMessage {
    /// Creates a new [UserMessage::Prompt], automatically detecting and adding the user's
    /// environment [UserEnvContext].
    pub fn new_prompt(prompt: String) -> Self {
        Self {
            images: None,
            additional_context: String::new(),
            env_context: UserEnvContext::generate_new(),
            content: UserMessageContent::Prompt { prompt },
        }
    }

    pub fn new_cancelled_tool_uses<'a>(prompt: Option<String>, tool_use_ids: impl Iterator<Item = &'a str>) -> Self {
        Self {
            images: None,
            additional_context: String::new(),
            env_context: UserEnvContext::generate_new(),
            content: UserMessageContent::CancelledToolUses {
                prompt,
                tool_use_results: tool_use_ids
                    .map(|id| ToolUseResult {
                        tool_use_id: id.to_string(),
                        content: vec![ToolUseResultBlock::Text(
                            "Tool use was cancelled by the user".to_string(),
                        )],
                        status: ToolResultStatus::Error,
                    })
                    .collect(),
            },
        }
    }

    pub fn new_tool_use_results(results: Vec<ToolUseResult>) -> Self {
        Self {
            additional_context: String::new(),
            env_context: UserEnvContext::generate_new(),
            content: UserMessageContent::ToolUseResults {
                tool_use_results: results,
            },
            images: None,
        }
    }

    pub fn new_tool_use_results_with_images(results: Vec<ToolUseResult>, images: Vec<ImageBlock>) -> Self {
        Self {
            additional_context: String::new(),
            env_context: UserEnvContext::generate_new(),
            content: UserMessageContent::ToolUseResults {
                tool_use_results: results,
            },
            images: Some(images),
        }
    }

    /// Converts this message into a [UserInputMessage] to be stored in the history of
    /// [api_client::model::ConversationState].
    pub fn into_history_entry(self) -> UserInputMessage {
        let content = self.content_with_context();
        UserInputMessage {
            images: self.images.clone(),
            content,
            user_input_message_context: Some(UserInputMessageContext {
                env_state: self.env_context.env_state,
                tool_results: match self.content {
                    UserMessageContent::CancelledToolUses { tool_use_results, .. }
                    | UserMessageContent::ToolUseResults { tool_use_results } => {
                        Some(tool_use_results.into_iter().map(Into::into).collect())
                    },
                    UserMessageContent::Prompt { .. } => None,
                },
                tools: None,
                ..Default::default()
            }),
            user_intent: None,
            model_id: None,
        }
    }

    /// Converts this message into a [UserInputMessage] to be sent as
    /// [FigConversationState::user_input_message].
    pub fn into_user_input_message(
        self,
        model_id: Option<String>,
        tools: &HashMap<ToolOrigin, Vec<Tool>>,
    ) -> UserInputMessage {
        let content = self.content_with_context();
        UserInputMessage {
            images: self.images,
            content,
            user_input_message_context: Some(UserInputMessageContext {
                env_state: self.env_context.env_state,
                tool_results: match self.content {
                    UserMessageContent::CancelledToolUses { tool_use_results, .. }
                    | UserMessageContent::ToolUseResults { tool_use_results } => {
                        Some(tool_use_results.into_iter().map(Into::into).collect())
                    },
                    UserMessageContent::Prompt { .. } => None,
                },
                tools: if tools.is_empty() {
                    None
                } else {
                    Some(tools.values().flatten().cloned().collect::<Vec<_>>())
                },
                ..Default::default()
            }),
            user_intent: None,
            model_id,
        }
    }

    pub fn has_tool_use_results(&self) -> bool {
        match self.content() {
            UserMessageContent::CancelledToolUses { .. } | UserMessageContent::ToolUseResults { .. } => true,
            UserMessageContent::Prompt { .. } => false,
        }
    }

    pub fn tool_use_results(&self) -> Option<&[ToolUseResult]> {
        match self.content() {
            UserMessageContent::Prompt { .. } => None,
            UserMessageContent::CancelledToolUses { tool_use_results, .. } => Some(tool_use_results.as_slice()),
            UserMessageContent::ToolUseResults { tool_use_results } => Some(tool_use_results.as_slice()),
        }
    }

    pub fn additional_context(&self) -> &str {
        &self.additional_context
    }

    pub fn content(&self) -> &UserMessageContent {
        &self.content
    }

    pub fn prompt(&self) -> Option<&str> {
        match self.content() {
            UserMessageContent::Prompt { prompt } => Some(prompt.as_str()),
            UserMessageContent::CancelledToolUses { prompt, .. } => prompt.as_ref().map(|s| s.as_str()),
            UserMessageContent::ToolUseResults { .. } => None,
        }
    }

    /// Truncates the content contained in this user message to a maximum length of `max_bytes`.
    pub fn truncate_safe(&mut self, max_bytes: usize) {
        self.content.truncate_safe(max_bytes);
    }

    pub fn replace_content_with_tool_use_results(&mut self) {
        if let Some(tool_results) = self.tool_use_results() {
            let tool_content: Vec<String> = tool_results
                .iter()
                .flat_map(|tr| {
                    tr.content.iter().map(|c| match c {
                        ToolUseResultBlock::Json(document) => serde_json::to_string(&document)
                            .map_err(|err| error!(?err, "failed to serialize tool result"))
                            .unwrap_or_default(),
                        ToolUseResultBlock::Text(s) => s.clone(),
                    })
                })
                .collect::<_>();
            let mut tool_content = tool_content.join(" ");
            if tool_content.is_empty() {
                // To avoid validation errors with empty content, we need to make sure
                // something is set.
                tool_content.push_str("<tool result redacted>");
            }
            let prompt = truncate_safe(&tool_content, MAX_USER_MESSAGE_SIZE).to_string();
            self.content = UserMessageContent::Prompt { prompt };
        }
    }

    /// Returns a formatted [String] containing [Self::additional_context] and [Self::prompt].
    fn content_with_context(&self) -> String {
        match (self.additional_context.is_empty(), self.prompt()) {
            // Only add special delimiters if we have both a prompt and additional context
            (false, Some(prompt)) => format!(
                "{} {}{}{}",
                self.additional_context, USER_ENTRY_START_HEADER, prompt, USER_ENTRY_END_HEADER
            ),
            (true, Some(prompt)) => prompt.to_string(),
            _ => self.additional_context.clone(),
        }
        .trim()
        .to_string()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolUseResult {
    /// The ID for the tool request.
    pub tool_use_id: String,
    /// Content of the tool result.
    pub content: Vec<ToolUseResultBlock>,
    /// Status of the tool result.
    pub status: ToolResultStatus,
}

impl From<ToolResult> for ToolUseResult {
    fn from(value: ToolResult) -> Self {
        Self {
            tool_use_id: value.tool_use_id,
            content: value.content.into_iter().map(Into::into).collect(),
            status: value.status,
        }
    }
}

impl From<ToolUseResult> for ToolResult {
    fn from(value: ToolUseResult) -> Self {
        Self {
            tool_use_id: value.tool_use_id,
            content: value.content.into_iter().map(Into::into).collect(),
            status: value.status,
        }
    }
}

fn truncate_safe_tool_use_results(tool_use_results: &mut [ToolUseResult], max_bytes: usize, truncated_suffix: &str) {
    let max_bytes = max_bytes / tool_use_results.len();
    for result in tool_use_results {
        for content in &mut result.content {
            match content {
                ToolUseResultBlock::Json(value) => match serde_json::to_string(value) {
                    Ok(mut value_str) => {
                        if value_str.len() > max_bytes {
                            truncate_safe_in_place(&mut value_str, max_bytes, truncated_suffix);
                            *content = ToolUseResultBlock::Text(value_str);
                            return;
                        }
                    },
                    Err(err) => {
                        warn!(?err, "Unable to truncate JSON");
                    },
                },
                ToolUseResultBlock::Text(t) => {
                    truncate_safe_in_place(t, max_bytes, truncated_suffix);
                },
            }
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ToolUseResultBlock {
    Json(serde_json::Value),
    Text(String),
}

impl From<ToolUseResultBlock> for ToolResultContentBlock {
    fn from(value: ToolUseResultBlock) -> Self {
        match value {
            ToolUseResultBlock::Json(v) => Self::Json(serde_value_to_document(v)),
            ToolUseResultBlock::Text(s) => Self::Text(s),
        }
    }
}

impl From<ToolResultContentBlock> for ToolUseResultBlock {
    fn from(value: ToolResultContentBlock) -> Self {
        match value {
            ToolResultContentBlock::Json(v) => Self::Json(document_to_serde_value(v)),
            ToolResultContentBlock::Text(s) => Self::Text(s),
        }
    }
}

impl From<InvokeOutput> for ToolUseResultBlock {
    fn from(value: InvokeOutput) -> Self {
        match value.output {
            OutputKind::Text(text) => Self::Text(text),
            OutputKind::Json(value) => Self::Json(value),
            OutputKind::Images(_) => Self::Text("See images data supplied".to_string()),
            OutputKind::Mixed { text, .. } => ToolUseResultBlock::Text(text),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserEnvContext {
    env_state: Option<EnvState>,
}

impl UserEnvContext {
    pub fn generate_new() -> Self {
        Self {
            env_state: Some(build_env_state()),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AssistantMessage {
    /// Normal response containing no tool uses.
    Response {
        message_id: Option<String>,
        content: String,
    },
    /// An assistant message containing tool uses.
    ToolUse {
        message_id: Option<String>,
        content: String,
        tool_uses: Vec<AssistantToolUse>,
    },
}

impl AssistantMessage {
    pub fn new_response(message_id: Option<String>, content: String) -> Self {
        Self::Response { message_id, content }
    }

    pub fn new_tool_use(message_id: Option<String>, content: String, tool_uses: Vec<AssistantToolUse>) -> Self {
        Self::ToolUse {
            message_id,
            content,
            tool_uses,
        }
    }

    pub fn message_id(&self) -> Option<&str> {
        match self {
            AssistantMessage::Response { message_id, .. } => message_id.as_ref().map(|s| s.as_str()),
            AssistantMessage::ToolUse { message_id, .. } => message_id.as_ref().map(|s| s.as_str()),
        }
    }

    pub fn content(&self) -> &str {
        match self {
            AssistantMessage::Response { content, .. } => content.as_str(),
            AssistantMessage::ToolUse { content, .. } => content.as_str(),
        }
    }

    pub fn tool_uses(&self) -> Option<&[AssistantToolUse]> {
        match self {
            AssistantMessage::ToolUse { tool_uses, .. } => Some(tool_uses.as_slice()),
            AssistantMessage::Response { .. } => None,
        }
    }
}

impl From<AssistantMessage> for AssistantResponseMessage {
    fn from(value: AssistantMessage) -> Self {
        let (message_id, content, tool_uses) = match value {
            AssistantMessage::Response { message_id, content } => (message_id, content, None),
            AssistantMessage::ToolUse {
                message_id,
                content,
                tool_uses,
            } => (
                message_id,
                content,
                Some(tool_uses.into_iter().map(Into::into).collect()),
            ),
        };
        Self {
            message_id,
            content,
            tool_uses,
        }
    }
}

#[derive(Default, Debug, Clone, Serialize, Deserialize)]
pub struct AssistantToolUse {
    /// The ID for the tool request.
    pub id: String,
    /// The name for the tool as exposed to the model
    pub name: String,
    /// Original name of the tool
    pub orig_name: String,
    /// The input to pass to the tool as exposed to the model
    pub args: serde_json::Value,
    /// Original input passed to the tool
    pub orig_args: serde_json::Value,
}

impl From<AssistantToolUse> for ToolUse {
    fn from(value: AssistantToolUse) -> Self {
        Self {
            tool_use_id: value.id,
            name: value.name,
            input: serde_value_to_document(value.args).into(),
        }
    }
}

impl From<ToolUse> for AssistantToolUse {
    fn from(value: ToolUse) -> Self {
        Self {
            id: value.tool_use_id,
            name: value.name,
            args: document_to_serde_value(value.input.into()),
            ..Default::default()
        }
    }
}

pub fn build_env_state() -> EnvState {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_env_state() {
        let env_state = build_env_state();
        assert!(env_state.current_working_directory.is_some());
        assert!(env_state.operating_system.as_ref().is_some_and(|os| !os.is_empty()));
        println!("{env_state:?}");
    }
}
