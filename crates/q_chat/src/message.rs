use std::env;

use fig_api_client::model::{
    AssistantResponseMessage,
    EnvState,
    ShellState,
    ToolResult,
    ToolResultContentBlock,
    ToolResultStatus,
    ToolUse,
    UserInputMessage,
    UserInputMessageContext,
};
use fig_util::Shell;
use serde::{
    Deserialize,
    Serialize,
};
use tracing::error;

use super::consts::MAX_CURRENT_WORKING_DIRECTORY_LEN;
use super::tools::{
    InvokeOutput,
    OutputKind,
    document_to_serde_value,
    serde_value_to_document,
};
use super::util::truncate_safe;

const USER_ENTRY_START_HEADER: &str = "--- USER MESSAGE BEGIN ---\n";
const USER_ENTRY_END_HEADER: &str = "--- USER MESSAGE END ---\n\n";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserMessage {
    pub additional_context: String,
    pub env_context: UserEnvContext,
    pub content: UserMessageContent,
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

impl UserMessage {
    /// Creates a new [UserMessage::Prompt], automatically detecting and adding the user's
    /// environment [UserEnvContext].
    pub fn new_prompt(prompt: String) -> Self {
        Self {
            additional_context: String::new(),
            env_context: UserEnvContext::generate_new(),
            content: UserMessageContent::Prompt { prompt },
        }
    }

    pub fn new_cancelled_tool_uses<'a>(prompt: Option<String>, tool_use_ids: impl Iterator<Item = &'a str>) -> Self {
        Self {
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
        }
    }

    /// Converts this message into a [UserInputMessage] to be stored in the history of
    /// [fig_api_client::model::ConversationState].
    pub fn into_history_entry(self) -> UserInputMessage {
        UserInputMessage {
            content: self.prompt().unwrap_or_default().to_string(),
            user_input_message_context: Some(UserInputMessageContext {
                shell_state: self.env_context.shell_state,
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
        }
    }

    /// Converts this message into a [UserInputMessage] to be sent as
    /// [FigConversationState::user_input_message].
    pub fn into_user_input_message(self) -> UserInputMessage {
        let formatted_prompt = match self.prompt() {
            Some(prompt) if !prompt.is_empty() => {
                format!("{}{}{}", USER_ENTRY_START_HEADER, prompt, USER_ENTRY_END_HEADER)
            },
            _ => String::new(),
        };
        UserInputMessage {
            content: format!("{} {}", self.additional_context, formatted_prompt)
                .trim()
                .to_string(),
            user_input_message_context: Some(UserInputMessageContext {
                shell_state: self.env_context.shell_state,
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
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserEnvContext {
    shell_state: Option<ShellState>,
    env_state: Option<EnvState>,
}

impl UserEnvContext {
    pub fn generate_new() -> Self {
        Self {
            shell_state: Some(build_shell_state()),
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssistantToolUse {
    /// The ID for the tool request.
    pub id: String,
    /// The name for the tool.
    pub name: String,
    /// The input to pass to the tool.
    pub args: serde_json::Value,
}

impl From<AssistantToolUse> for ToolUse {
    fn from(value: AssistantToolUse) -> Self {
        Self {
            tool_use_id: value.id,
            name: value.name,
            input: serde_value_to_document(value.args),
        }
    }
}

impl From<ToolUse> for AssistantToolUse {
    fn from(value: ToolUse) -> Self {
        Self {
            id: value.tool_use_id,
            name: value.name,
            args: document_to_serde_value(value.input),
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
