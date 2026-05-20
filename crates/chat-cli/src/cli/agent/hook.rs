use std::fmt::Display;

use schemars::JsonSchema;
use serde::{
    Deserialize,
    Serialize,
};

const DEFAULT_TIMEOUT_MS: u64 = 30_000;
const DEFAULT_MAX_OUTPUT_SIZE: usize = 1024 * 10;
const DEFAULT_CACHE_TTL_SECONDS: u64 = 0;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Eq, PartialEq, JsonSchema, Hash)]
#[serde(rename_all = "camelCase")]
pub enum HookTrigger {
    /// Triggered during agent spawn
    AgentSpawn,
    /// Triggered per user message submission
    UserPromptSubmit,
    /// Triggered before tool execution
    PreToolUse,
    /// Triggered after tool execution
    PostToolUse,
    /// Triggered when the assistant finishes responding
    Stop,
}

impl Display for HookTrigger {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            HookTrigger::AgentSpawn => write!(f, "agentSpawn"),
            HookTrigger::UserPromptSubmit => write!(f, "userPromptSubmit"),
            HookTrigger::PreToolUse => write!(f, "preToolUse"),
            HookTrigger::PostToolUse => write!(f, "postToolUse"),
            HookTrigger::Stop => write!(f, "stop"),
        }
    }
}

#[derive(Debug, Clone, Deserialize, Eq, PartialEq, Hash)]
pub enum Source {
    Agent,
    Session,
}

impl Default for Source {
    fn default() -> Self {
        Self::Agent
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq, JsonSchema, Hash)]
pub struct Hook {
    /// The command to run when the hook is triggered
    pub command: String,

    /// Max time the hook can run before it throws a timeout error
    #[serde(default = "Hook::default_timeout_ms")]
    pub timeout_ms: u64,

    /// Max output size of the hook before it is truncated
    #[serde(default = "Hook::default_max_output_size")]
    pub max_output_size: usize,

    /// How long the hook output is cached before it will be executed again
    #[serde(default = "Hook::default_cache_ttl_seconds")]
    pub cache_ttl_seconds: u64,

    /// Optional glob matcher for hook
    /// Currently used for matching tool name of PreToolUse and PostToolUse hook
    #[serde(skip_serializing_if = "Option::is_none")]
    pub matcher: Option<String>,

    #[schemars(skip)]
    #[serde(default, skip_serializing)]
    pub source: Source,
}

impl Hook {
    pub fn new(command: String, source: Source) -> Self {
        Self {
            command,
            timeout_ms: Self::default_timeout_ms(),
            max_output_size: Self::default_max_output_size(),
            cache_ttl_seconds: Self::default_cache_ttl_seconds(),
            matcher: None,
            source,
        }
    }

    fn default_timeout_ms() -> u64 {
        DEFAULT_TIMEOUT_MS
    }

    fn default_max_output_size() -> usize {
        DEFAULT_MAX_OUTPUT_SIZE
    }

    fn default_cache_ttl_seconds() -> u64 {
        DEFAULT_CACHE_TTL_SECONDS
    }
}

/// Decision returned by a PreToolUse hook
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum HookDecision {
    /// Allow tool execution (default behavior)
    #[default]
    Allow,
    /// Prompt user for confirmation before executing
    Ask,
    /// Block tool execution
    Block,
}

/// Response from a hook that can include a decision and message
#[derive(Debug, Clone, Default)]
pub struct HookResponse {
    pub decision: HookDecision,
    pub message: Option<String>,
}

impl HookResponse {
    /// Try to parse a JSON response from hook stdout
    /// Returns None if the output is not valid JSON or doesn't contain a decision field
    pub fn from_stdout(stdout: &str) -> Option<Self> {
        let trimmed = stdout.trim();
        if trimmed.is_empty() || !trimmed.starts_with('{') {
            return None;
        }

        let json: serde_json::Value = serde_json::from_str(trimmed).ok()?;
        let decision_str = json.get("decision")?.as_str()?;

        let decision = match decision_str.to_lowercase().as_str() {
            "allow" => HookDecision::Allow,
            "ask" => HookDecision::Ask,
            "block" => HookDecision::Block,
            _ => return None,
        };

        let message = json.get("message").and_then(|m| m.as_str()).map(String::from);

        Some(Self { decision, message })
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hook_response_from_stdout_ask() {
        let stdout = r#"{"decision": "ask", "message": "⚠️ This command uses sudo. Allow?"}"#;
        let response = HookResponse::from_stdout(stdout).unwrap();
        assert_eq!(response.decision, HookDecision::Ask);
        assert_eq!(response.message, Some("⚠️ This command uses sudo. Allow?".to_string()));
    }

    #[test]
    fn test_hook_response_from_stdout_block() {
        let stdout = r#"{"decision": "block", "message": "Blocked by policy"}"#;
        let response = HookResponse::from_stdout(stdout).unwrap();
        assert_eq!(response.decision, HookDecision::Block);
        assert_eq!(response.message, Some("Blocked by policy".to_string()));
    }

    #[test]
    fn test_hook_response_from_stdout_allow() {
        let stdout = r#"{"decision": "allow"}"#;
        let response = HookResponse::from_stdout(stdout).unwrap();
        assert_eq!(response.decision, HookDecision::Allow);
        assert_eq!(response.message, None);
    }

    #[test]
    fn test_hook_response_from_stdout_case_insensitive() {
        let stdout = r#"{"decision": "ASK", "message": "Confirm?"}"#;
        let response = HookResponse::from_stdout(stdout).unwrap();
        assert_eq!(response.decision, HookDecision::Ask);
    }

    #[test]
    fn test_hook_response_from_stdout_empty() {
        assert!(HookResponse::from_stdout("").is_none());
    }

    #[test]
    fn test_hook_response_from_stdout_not_json() {
        assert!(HookResponse::from_stdout("not json").is_none());
    }

    #[test]
    fn test_hook_response_from_stdout_no_decision() {
        let stdout = r#"{"message": "some message"}"#;
        assert!(HookResponse::from_stdout(stdout).is_none());
    }

    #[test]
    fn test_hook_response_from_stdout_invalid_decision() {
        let stdout = r#"{"decision": "invalid"}"#;
        assert!(HookResponse::from_stdout(stdout).is_none());
    }

    #[test]
    fn test_hook_response_from_stdout_with_whitespace() {
        let stdout = "  \n{\"decision\": \"ask\", \"message\": \"test\"}\n  ";
        let response = HookResponse::from_stdout(stdout).unwrap();
        assert_eq!(response.decision, HookDecision::Ask);
    }
}
