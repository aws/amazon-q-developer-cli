//! Strongly-typed slash command enum for ACP extension method execution.
//!
//! This enum is shared with TypeScript via typeshare, providing compile-time
//! type safety across the Rust/TypeScript boundary.
//!
//! Types live here (agent crate) for typeshare generation.
//! Execution logic lives in chat-cli crate where ApiClient is available.

use serde::{
    Deserialize,
    Serialize,
};
use typeshare::typeshare;

/// Slash command enum - each variant represents a command with its arguments.
///
/// Executed via `_kiro.dev/commands/execute` extension method, NOT as prompts.
/// This is distinct from ACP "slash commands" which are prompt-based workflows.
#[typeshare]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "command", content = "args", rename_all = "camelCase")]
pub enum TuiCommand {
    /// List available models or switch to a specific model
    Model(ModelArgs),
    /// Show context/token usage for the current conversation
    Context(ContextArgs),
    /// Compact the conversation history
    Compact(CompactArgs),
}

/// Arguments for /model command
#[typeshare]
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelArgs {
    /// Model ID to switch to. If None, lists available models.
    /// Accepts either `modelName` or `value` (for generic selection UI)
    #[serde(alias = "value", skip_serializing_if = "Option::is_none")]
    pub model_name: Option<String>,
}

/// Arguments for /context command
#[typeshare]
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContextArgs {
    /// Show a detailed breakdown
    #[serde(default)]
    pub verbose: bool,
}

/// Arguments for /compact command
#[typeshare]
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompactArgs {
    /// Target token count after compaction
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_tokens: Option<u32>,
}

impl TuiCommand {
    /// Command name with leading slash
    pub fn name(&self) -> &'static str {
        match self {
            TuiCommand::Model(_) => "/model",
            TuiCommand::Context(_) => "/context",
            TuiCommand::Compact(_) => "/compact",
        }
    }

    /// Human-readable description
    pub fn description(&self) -> &'static str {
        match self {
            TuiCommand::Model(_) => "Select or list available models",
            TuiCommand::Context(_) => "Show context/token usage",
            TuiCommand::Compact(_) => "Compact conversation history",
        }
    }

    /// Metadata for TUI (options method, input type, etc.)
    pub fn meta(&self) -> Option<serde_json::Map<String, serde_json::Value>> {
        match self {
            TuiCommand::Model(_) => {
                let mut meta = serde_json::Map::new();
                meta.insert("optionsMethod".into(), "_kiro.dev/commands/model/options".into());
                meta.insert("inputType".into(), "selection".into());
                meta.insert("hint".into(), "↑↓ to choose model".into());
                Some(meta)
            },
            TuiCommand::Context(_) => {
                let mut meta = serde_json::Map::new();
                meta.insert("inputType".into(), "panel".into());
                Some(meta)
            },
            TuiCommand::Compact(_) => None,
        }
    }

    /// All available commands with default args (for advertising to TUI)
    pub fn all_commands() -> Vec<TuiCommand> {
        vec![
            TuiCommand::Model(ModelArgs::default()),
            TuiCommand::Context(ContextArgs::default()),
            TuiCommand::Compact(CompactArgs::default()),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_serialize_model_with_args() {
        let cmd = TuiCommand::Model(ModelArgs {
            model_name: Some("claude-sonnet".into()),
        });
        let json = serde_json::to_string(&cmd).unwrap();
        assert!(json.contains(r#""command":"model""#));
        assert!(json.contains(r#""modelName":"claude-sonnet""#));
    }

    #[test]
    fn test_serialize_context() {
        let cmd = TuiCommand::Context(ContextArgs::default());
        let json = serde_json::to_string(&cmd).unwrap();
        assert!(json.contains(r#""command":"context""#));
    }

    #[test]
    fn test_deserialize_model() {
        let json = r#"{"command":"model","args":{"modelName":"sonnet"}}"#;
        let cmd: TuiCommand = serde_json::from_str(json).unwrap();
        assert!(matches!(cmd, TuiCommand::Model(ModelArgs { model_name: Some(n) }) if n == "sonnet"));
    }

    #[test]
    fn test_deserialize_model_with_value_alias() {
        // TUI sends generic { value: "..." } for selection commands
        let json = r#"{"command":"model","args":{"value":"claude-sonnet-4"}}"#;
        let cmd: TuiCommand = serde_json::from_str(json).unwrap();
        assert!(matches!(cmd, TuiCommand::Model(ModelArgs { model_name: Some(n) }) if n == "claude-sonnet-4"));
    }

    #[test]
    fn test_deserialize_context_no_args() {
        // With content="args", empty args object is required
        let json = r#"{"command":"context","args":{}}"#;
        let cmd: TuiCommand = serde_json::from_str(json).unwrap();
        assert!(matches!(cmd, TuiCommand::Context(_)));
    }
}
