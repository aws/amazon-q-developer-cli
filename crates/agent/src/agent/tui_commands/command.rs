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
    /// Show help with all available commands
    Help(HelpArgs),
    /// List available models or switch to a specific model
    Model(ModelArgs),
    /// List available agents or switch to a specific agent
    Agent(AgentArgs),
    /// Show context/token usage for the current conversation
    Context(ContextArgs),
    /// Compact the conversation history
    Compact(CompactArgs),
    /// Clear the conversation history
    Clear(ClearArgs),
    /// Quit the application
    Quit(QuitArgs),
    /// Show billing and usage information
    Usage(UsageArgs),
    /// Paste image from system clipboard (returns base64 PNG data)
    #[serde(rename = "paste")]
    PasteImage(PasteImageArgs),
    /// Show configured MCP servers
    Mcp(McpArgs),
    /// Show available tools
    Tools(ToolsArgs),
    /// Switch to Plan agent for breaking down ideas into implementation plans.
    Plan(PlanArgs),
    /// Report an issue (currently internal Amazon users only)
    Issue(IssueArgs),
    /// Manage knowledge base
    Knowledge(KnowledgeArgs),
    /// List and execute available prompts
    Prompts(PromptsArgs),
}

/// Arguments for /help command
#[typeshare]
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HelpArgs {}

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

/// Arguments for /agent command
#[typeshare]
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentArgs {
    /// Agent name to switch to. If None, lists available agents.
    /// Accepts either `agentName` or `value` (for generic selection UI)
    #[serde(alias = "value", skip_serializing_if = "Option::is_none")]
    pub agent_name: Option<String>,
}

/// Arguments for /context command
#[typeshare]
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContextArgs {
    /// Show a detailed breakdown
    #[serde(default)]
    pub verbose: bool,
    /// Subcommand: add, remove, show, clear
    #[serde(alias = "value", skip_serializing_if = "Option::is_none")]
    pub subcommand: Option<String>,
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

/// Arguments for /clear command
#[typeshare]
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClearArgs {}

/// Arguments for /quit command
#[typeshare]
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QuitArgs {}

/// Arguments for /usage command
#[typeshare]
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageArgs {}

/// Arguments for /paste command
#[typeshare]
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PasteImageArgs {}

/// Arguments for /mcp command
#[typeshare]
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpArgs {}

/// Arguments for /tools command
#[typeshare]
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolsArgs {}

/// Arguments for /plan command
#[typeshare]
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanArgs {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompt: Option<String>,
}

/// Arguments for /issue command
#[typeshare]
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IssueArgs {}

/// Arguments for /knowledge command
#[typeshare]
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeArgs {
    /// Subcommand: show, add, remove, update, clear, cancel
    #[serde(alias = "value", skip_serializing_if = "Option::is_none")]
    pub subcommand: Option<String>,
}

/// Arguments for /prompts command
#[typeshare]
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PromptsArgs {
    /// Prompt name to execute. If None, lists available prompts.
    /// Accepts either `promptName` or `value` (for generic selection UI)
    #[serde(alias = "value", skip_serializing_if = "Option::is_none")]
    pub prompt_name: Option<String>,
}

impl TuiCommand {
    /// Command name with leading slash
    pub fn name(&self) -> &'static str {
        match self {
            TuiCommand::Help(_) => "/help",
            TuiCommand::Model(_) => "/model",
            TuiCommand::Agent(_) => "/agent",
            TuiCommand::Context(_) => "/context",
            TuiCommand::Compact(_) => "/compact",
            TuiCommand::Clear(_) => "/clear",
            TuiCommand::Quit(_) => "/quit",
            TuiCommand::Usage(_) => "/usage",
            TuiCommand::PasteImage(_) => "/paste",
            TuiCommand::Mcp(_) => "/mcp",
            TuiCommand::Tools(_) => "/tools",
            TuiCommand::Plan(_) => "/plan",
            TuiCommand::Issue(_) => "/issue",
            TuiCommand::Knowledge(_) => "/knowledge",
            TuiCommand::Prompts(_) => "/prompts",
        }
    }

    /// Human-readable description
    pub fn description(&self) -> &'static str {
        match self {
            TuiCommand::Help(_) => "Show this help message",
            TuiCommand::Model(_) => "Select or list available models",
            TuiCommand::Agent(_) => "Select or list available agents",
            TuiCommand::Context(_) => "Manage context files or show token usage",
            TuiCommand::Compact(_) => "Compact conversation history",
            TuiCommand::Clear(_) => "Clear conversation history",
            TuiCommand::Quit(_) => "Quit the application",
            TuiCommand::Usage(_) => "Show billing and usage information",
            TuiCommand::PasteImage(_) => "Paste image from clipboard",
            TuiCommand::Mcp(_) => "Show configured MCP servers",
            TuiCommand::Tools(_) => "Show available tools",
            TuiCommand::Plan(_) => "Switch to Plan agent for breaking down ideas into implementation plans",
            TuiCommand::Issue(_) => "Report an issue",
            TuiCommand::Knowledge(_) => "Manage knowledge base (show, add, remove, update, clear, cancel)",
            TuiCommand::Prompts(_) => "Select or list available prompts",
        }
    }

    /// Usage example
    pub fn usage(&self) -> &'static str {
        match self {
            TuiCommand::Help(_) => "/help",
            TuiCommand::Model(_) => "/model [model-name]",
            TuiCommand::Agent(_) => "/agent [agent-name]",
            TuiCommand::Context(_) => "/context [add [--force] <path>...|remove <path>...|clear]",
            TuiCommand::Compact(_) => "/compact",
            TuiCommand::Clear(_) => "/clear",
            TuiCommand::Quit(_) => "/quit",
            TuiCommand::Usage(_) => "/usage",
            TuiCommand::PasteImage(_) => "/paste",
            TuiCommand::Mcp(_) => "/mcp",
            TuiCommand::Tools(_) => "/tools",
            TuiCommand::Plan(_) => "/plan [prompt]",
            TuiCommand::Issue(_) => "/issue",
            TuiCommand::Knowledge(_) => {
                "/knowledge [show|add <name> <path>|remove <name|path>|update <path>|clear|cancel]"
            },
            TuiCommand::Prompts(_) => "/prompts [prompt-name]",
        }
    }

    /// Metadata for TUI (options method, input type, etc.)
    pub fn meta(&self) -> Option<serde_json::Map<String, serde_json::Value>> {
        match self {
            TuiCommand::Help(_) => {
                let mut meta = serde_json::Map::new();
                meta.insert("inputType".into(), "panel".into());
                Some(meta)
            },
            TuiCommand::Model(_) => {
                let mut meta = serde_json::Map::new();
                meta.insert("optionsMethod".into(), "_kiro.dev/commands/model/options".into());
                meta.insert("inputType".into(), "selection".into());
                meta.insert("hint".into(), "".into());
                Some(meta)
            },
            TuiCommand::Agent(_) => {
                let mut meta = serde_json::Map::new();
                meta.insert("optionsMethod".into(), "_kiro.dev/commands/agent/options".into());
                meta.insert("inputType".into(), "selection".into());
                meta.insert("hint".into(), "".into());
                Some(meta)
            },
            TuiCommand::Context(_) => {
                let mut meta = serde_json::Map::new();
                meta.insert("inputType".into(), "panel".into());
                meta.insert("hint".into(), "add <path>, remove <path>, clear".into());
                Some(meta)
            },
            TuiCommand::Clear(_) => None,
            TuiCommand::Quit(_) => {
                let mut meta = serde_json::Map::new();
                meta.insert("local".into(), true.into());
                Some(meta)
            },
            TuiCommand::Compact(_) => None,
            TuiCommand::Usage(_) => {
                let mut meta = serde_json::Map::new();
                meta.insert("inputType".into(), "panel".into());
                Some(meta)
            },
            TuiCommand::PasteImage(_) => None,
            TuiCommand::Mcp(_) => {
                let mut meta = serde_json::Map::new();
                meta.insert("inputType".into(), "panel".into());
                Some(meta)
            },
            TuiCommand::Tools(_) => {
                let mut meta = serde_json::Map::new();
                meta.insert("inputType".into(), "panel".into());
                Some(meta)
            },
            TuiCommand::Plan(_) => None,
            TuiCommand::Issue(_) => None,
            TuiCommand::Knowledge(_) => {
                let mut meta = serde_json::Map::new();
                meta.insert("inputType".into(), "panel".into());
                Some(meta)
            },
            TuiCommand::Prompts(_) => {
                let mut meta = serde_json::Map::new();
                meta.insert("optionsMethod".into(), "_kiro.dev/commands/prompts/options".into());
                meta.insert("inputType".into(), "selection".into());
                meta.insert("hint".into(), "".into());
                Some(meta)
            },
        }
    }

    /// All available commands with default args (for advertising to TUI)
    pub fn all_commands() -> Vec<TuiCommand> {
        let mut commands = vec![
            TuiCommand::Help(HelpArgs::default()),
            TuiCommand::Model(ModelArgs::default()),
            TuiCommand::Agent(AgentArgs::default()),
            TuiCommand::Context(ContextArgs::default()),
            TuiCommand::Compact(CompactArgs::default()),
            TuiCommand::Clear(ClearArgs::default()),
            TuiCommand::Quit(QuitArgs::default()),
            TuiCommand::Usage(UsageArgs::default()),
            TuiCommand::Mcp(McpArgs::default()),
            TuiCommand::Tools(ToolsArgs::default()),
            TuiCommand::Plan(PlanArgs::default()),
            TuiCommand::PasteImage(PasteImageArgs::default()),
            TuiCommand::Issue(IssueArgs::default()),
            TuiCommand::Knowledge(KnowledgeArgs::default()),
            TuiCommand::Prompts(PromptsArgs::default()),
        ];
        commands.sort_by_key(|cmd| cmd.name());
        commands
    }

    /// Parse a command from name (without leading slash) and argument string.
    pub fn parse(name: &str, args: &str) -> Option<Self> {
        match name {
            "help" => Some(Self::Help(HelpArgs::default())),
            "model" => Some(Self::Model(ModelArgs {
                model_name: (!args.is_empty()).then(|| args.to_string()),
            })),
            "agent" => Some(Self::Agent(AgentArgs {
                agent_name: (!args.is_empty()).then(|| args.to_string()),
            })),
            "context" => Some(Self::Context(ContextArgs {
                subcommand: (!args.is_empty()).then(|| args.to_string()),
                ..Default::default()
            })),
            "compact" => Some(Self::Compact(CompactArgs {
                target_tokens: args.parse().ok(),
            })),
            "clear" => Some(Self::Clear(ClearArgs::default())),
            "quit" => Some(Self::Quit(QuitArgs::default())),
            "usage" => Some(Self::Usage(UsageArgs::default())),
            "mcp" => Some(Self::Mcp(McpArgs::default())),
            "tools" => Some(Self::Tools(ToolsArgs::default())),
            "plan" => Some(Self::Plan(PlanArgs {
                prompt: (!args.is_empty()).then(|| args.to_string()),
            })),
            "issue" => Some(Self::Issue(IssueArgs::default())),
            "paste" => Some(Self::PasteImage(PasteImageArgs::default())),
            "knowledge" => Some(Self::Knowledge(KnowledgeArgs {
                subcommand: (!args.is_empty()).then(|| args.to_string()),
            })),
            "prompts" => Some(Self::Prompts(PromptsArgs {
                prompt_name: (!args.is_empty()).then(|| args.to_string()),
            })),
            _ => None,
        }
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

    #[test]
    fn test_parse_context_add() {
        let cmd = TuiCommand::parse("context", "add foo.txt").unwrap();
        match cmd {
            TuiCommand::Context(args) => {
                assert_eq!(args.subcommand, Some("add foo.txt".to_string()));
            },
            _ => panic!("expected Context"),
        }
    }

    #[test]
    fn test_parse_context_remove() {
        let cmd = TuiCommand::parse("context", "remove *.md").unwrap();
        match cmd {
            TuiCommand::Context(args) => {
                assert_eq!(args.subcommand, Some("remove *.md".to_string()));
            },
            _ => panic!("expected Context"),
        }
    }

    #[test]
    fn test_parse_context_no_args() {
        let cmd = TuiCommand::parse("context", "").unwrap();
        match cmd {
            TuiCommand::Context(args) => {
                assert!(args.subcommand.is_none());
            },
            _ => panic!("expected Context"),
        }
    }

    #[test]
    fn test_deserialize_context_with_value_alias() {
        // TUI sends { value: "add foo.txt" } for subcommand
        let json = r#"{"command":"context","args":{"value":"add foo.txt"}}"#;
        let cmd: TuiCommand = serde_json::from_str(json).unwrap();
        match cmd {
            TuiCommand::Context(args) => {
                assert_eq!(args.subcommand, Some("add foo.txt".to_string()));
            },
            _ => panic!("expected Context"),
        }
    }

    #[test]
    fn test_deserialize_context_with_subcommand() {
        let json = r#"{"command":"context","args":{"subcommand":"remove bar.rs"}}"#;
        let cmd: TuiCommand = serde_json::from_str(json).unwrap();
        match cmd {
            TuiCommand::Context(args) => {
                assert_eq!(args.subcommand, Some("remove bar.rs".to_string()));
            },
            _ => panic!("expected Context"),
        }
    }
}
