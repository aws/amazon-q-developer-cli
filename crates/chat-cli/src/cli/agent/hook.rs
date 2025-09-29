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
}

impl Display for HookTrigger {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            HookTrigger::AgentSpawn => write!(f, "agentSpawn"),
            HookTrigger::UserPromptSubmit => write!(f, "userPromptSubmit"),
            HookTrigger::PreToolUse => write!(f, "preToolUse"),
            HookTrigger::PostToolUse => write!(f, "postToolUse"),
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

    /// If true, PostToolUse hooks are deferred until turn completion
    #[serde(default = "Hook::default_only_when_turn_complete")]
    pub only_when_turn_complete: bool,

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
            only_when_turn_complete: Self::default_only_when_turn_complete(),
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

    fn default_only_when_turn_complete() -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hook_only_when_turn_complete_default() {
        let hook = Hook::new("echo test".to_string(), Source::Agent);
        assert_eq!(hook.only_when_turn_complete, false);
    }

    #[test]
    fn test_hook_serde_with_only_when_turn_complete() {
        let json = r#"{
            "command": "cargo fmt",
            "only_when_turn_complete": true
        }"#;

        let hook: Hook = serde_json::from_str(json).unwrap();
        assert_eq!(hook.command, "cargo fmt");
        assert_eq!(hook.only_when_turn_complete, true);
    }

    #[test]
    fn test_hook_serde_without_only_when_turn_complete() {
        let json = r#"{
            "command": "cargo fmt"
        }"#;

        let hook: Hook = serde_json::from_str(json).unwrap();
        assert_eq!(hook.command, "cargo fmt");
        assert_eq!(hook.only_when_turn_complete, false); // Should default to false
    }
}
