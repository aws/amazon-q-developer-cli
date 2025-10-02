use serde::{
    Deserialize,
    Serialize,
};

use crate::cli::agent::hook::Hook;

const DEFAULT_TIMEOUT_MS: u64 = 30_000;
const DEFAULT_MAX_OUTPUT_SIZE: usize = 1024 * 10;
const DEFAULT_CACHE_TTL_SECONDS: u64 = 0;

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum LegacyHookType {
    // Execute an inline shell command
    Inline,
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum LegacyHookTrigger {
    ConversationStart,
    PerPrompt,
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
pub struct LegacyHook {
    pub trigger: LegacyHookTrigger,

    pub r#type: LegacyHookType,

    #[serde(default = "LegacyHook::default_disabled")]
    pub disabled: bool,

    /// Max time the hook can run before it throws a timeout error
    #[serde(default = "LegacyHook::default_timeout_ms")]
    pub timeout_ms: u64,

    /// Max output size of the hook before it is truncated
    #[serde(default = "LegacyHook::default_max_output_size")]
    pub max_output_size: usize,

    /// How long the hook output is cached before it will be executed again
    #[serde(default = "LegacyHook::default_cache_ttl_seconds")]
    pub cache_ttl_seconds: u64,

    // Type-specific fields
    /// The bash command to execute
    pub command: Option<String>, // For inline hooks

    // Internal data
    #[serde(skip)]
    pub name: String,
    #[serde(skip)]
    pub is_global: bool,
}

impl LegacyHook {
    fn default_disabled() -> bool {
        false
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

impl From<LegacyHook> for Option<Hook> {
    fn from(value: LegacyHook) -> Self {
        Some(Hook {
            command: value.command?,
            timeout_ms: value.timeout_ms,
            max_output_size: value.max_output_size,
            cache_ttl_seconds: value.cache_ttl_seconds,
            matcher: None,
            only_when_turn_complete: false,
            source: Default::default(),
        })
    }
}
