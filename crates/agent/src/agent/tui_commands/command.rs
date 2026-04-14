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
    /// Submit feedback, request features, or report issues
    Feedback(FeedbackArgs),
    /// Load a previous chat session
    Chat(ChatArgs),
    /// Manage knowledge base
    Knowledge(KnowledgeArgs),
    /// List and execute available prompts
    Prompts(PromptsArgs),
    /// Open editor pre-filled with the last assistant message to compose a reply
    Reply(ReplyArgs),
    /// Code intelligence workspace management
    Code(CodeArgs),
    /// View configured hooks
    Hooks(HooksArgs),
    /// Switch to the guide agent for help with Kiro CLI
    Guide(GuideArgs),
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
pub struct McpArgs {
    #[serde(alias = "value", skip_serializing_if = "Option::is_none")]
    pub subcommand: Option<String>,
}

/// Arguments for /tools command
#[typeshare]
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolsArgs {
    /// Subcommand: trust-all, trust, untrust, reset
    #[serde(alias = "value", skip_serializing_if = "Option::is_none")]
    pub subcommand: Option<String>,
}

/// Arguments for /plan command
#[typeshare]
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanArgs {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompt: Option<String>,
}

/// Arguments for /feedback command
#[typeshare]
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FeedbackArgs {
    /// Feedback type: general, feature, issue. If None, shows the selection panel.
    #[serde(alias = "value", skip_serializing_if = "Option::is_none")]
    pub feedback_type: Option<String>,
}

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
/// Arguments for /chat command
#[typeshare]
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatArgs {
    /// Subcommand: save <path>, load <path>, new [prompt], list, delete <id>
    #[serde(alias = "value", skip_serializing_if = "Option::is_none")]
    pub subcommand: Option<String>,
}

/// Arguments for /reply command
#[typeshare]
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReplyArgs {}

/// Arguments for /code command
#[typeshare]
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CodeArgs {
    /// Subcommand: status, init, logs, overview, summary
    #[serde(alias = "value", skip_serializing_if = "Option::is_none")]
    pub subcommand: Option<String>,
}

/// Arguments for /hooks command
#[typeshare]
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HooksArgs {}

/// Arguments for /guide command
#[typeshare]
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GuideArgs {
    /// Optional question to ask the guide agent
    #[serde(default)]
    pub question: Option<String>,
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
            TuiCommand::Feedback(_) => "/feedback",
            TuiCommand::Knowledge(_) => "/knowledge",
            TuiCommand::Prompts(_) => "/prompts",
            TuiCommand::Chat(_) => "/chat",
            TuiCommand::Reply(_) => "/reply",
            TuiCommand::Code(_) => "/code",
            TuiCommand::Hooks(_) => "/hooks",
            TuiCommand::Guide(_) => "/guide",
        }
    }

    /// Human-readable description
    pub fn description(&self) -> &'static str {
        match self {
            TuiCommand::Help(_) => "Show available commands",
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
            TuiCommand::Feedback(_) => "Submit feedback, request features, or report issues",
            TuiCommand::Knowledge(_) => "Manage knowledge base",
            TuiCommand::Prompts(_) => "Select or list available prompts",
            TuiCommand::Chat(_) => "Load a previous session or start a new one",
            TuiCommand::Reply(_) => "Open editor pre-filled with the last assistant message to compose a reply",
            TuiCommand::Code(_) => "Code intelligence workspace management",
            TuiCommand::Hooks(_) => "View configured hooks",
            TuiCommand::Guide(_) => "Get help with Kiro CLI features from the guide agent",
        }
    }

    /// Usage example
    pub fn usage(&self) -> &'static str {
        match self {
            TuiCommand::Help(_) => "/help",
            TuiCommand::Model(_) => "/model [model-name]",
            TuiCommand::Agent(_) => "/agent [agent-name|create <name>|edit [name]|swap <name>]",
            TuiCommand::Context(_) => "/context [add [--force] <path>...|remove <path>...|clear]",
            TuiCommand::Compact(_) => "/compact",
            TuiCommand::Clear(_) => "/clear",
            TuiCommand::Quit(_) => "/quit",
            TuiCommand::Usage(_) => "/usage",
            TuiCommand::PasteImage(_) => "/paste",
            TuiCommand::Mcp(_) => "/mcp",
            TuiCommand::Tools(_) => "/tools [trust-all|trust <name>|untrust <name>|reset]",
            TuiCommand::Plan(_) => "/plan [prompt]",
            TuiCommand::Feedback(_) => "/feedback",
            TuiCommand::Knowledge(_) => {
                "/knowledge [show|add <name> <path>|remove <name|path>|update <path>|clear|cancel]"
            },
            TuiCommand::Prompts(_) => "/prompts [prompt-name]",
            TuiCommand::Chat(_) => "/chat [save [--force] <path>|load <path>|new [prompt]]",
            TuiCommand::Reply(_) => "/reply",
            TuiCommand::Code(_) => "/code [status|init|logs|overview|summary]",
            TuiCommand::Hooks(_) => "/hooks",
            TuiCommand::Guide(_) => "/guide [question]",
        }
    }

    /// Subcommand names, if any
    pub fn subcommands(&self) -> Vec<&'static str> {
        match self {
            TuiCommand::Agent(_) => vec!["create", "edit", "swap"],
            TuiCommand::Context(_) => vec!["add", "remove", "clear"],
            TuiCommand::Knowledge(_) => vec!["show", "add", "remove", "update", "clear", "cancel"],
            TuiCommand::Tools(_) => vec!["trust-all", "trust", "untrust", "reset"],
            TuiCommand::Chat(_) => vec!["save", "load", "new"],
            TuiCommand::Code(_) => vec!["status", "init", "logs", "overview", "summary"],
            TuiCommand::Mcp(_) => vec!["list", "add", "remove"],
            _ => vec![],
        }
    }

    /// Argument hints for subcommands that require additional input.
    /// Returns (subcommand_name, hint_text) pairs. Subcommands not listed here
    /// execute immediately when selected.
    pub fn subcommand_hints(&self) -> Vec<(&'static str, &'static str)> {
        match self {
            TuiCommand::Agent(_) => vec![("create", "<name>"), ("edit", "[name]"), ("swap", "<name>")],
            TuiCommand::Context(_) => vec![("add", "[--force] <path>..."), ("remove", "<path>...")],
            TuiCommand::Knowledge(_) => vec![
                ("add", "<name> <path>"),
                ("remove", "<name|path>"),
                ("update", "<path>"),
            ],
            TuiCommand::Tools(_) => vec![("trust", "<name>"), ("untrust", "<name>")],
            TuiCommand::Chat(_) => vec![("save", "[--force] <path>"), ("load", "<path>"), ("new", "[prompt]")],
            TuiCommand::Mcp(_) => vec![("add", "<server-name>"), ("remove", "<server-name>")],
            _ => vec![],
        }
    }

    /// Metadata for TUI (options method, input type, etc.)
    pub fn meta(&self) -> Option<serde_json::Map<String, serde_json::Value>> {
        let mut meta = match self {
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
                meta.insert("hint".into(), "trust-all, trust <name>, untrust <name>, reset".into());
                Some(meta)
            },
            TuiCommand::Plan(_) => None,
            TuiCommand::Feedback(_) => {
                let mut meta = serde_json::Map::new();
                meta.insert("inputType".into(), "selection".into());
                meta.insert("searchable".into(), false.into());
                meta.insert("hint".into(), "".into());
                Some(meta)
            },
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
            TuiCommand::Chat(_) => {
                let mut meta = serde_json::Map::new();
                meta.insert("inputType".into(), "selection".into());
                meta.insert("local".into(), true.into());
                meta.insert("hint".into(), "save <path>, load <path>, new [prompt]".into());
                Some(meta)
            },
            TuiCommand::Reply(_) => None,
            TuiCommand::Code(_) => {
                let mut meta = serde_json::Map::new();
                meta.insert("inputType".into(), "panel".into());
                Some(meta)
            },
            TuiCommand::Hooks(_) => {
                let mut meta = serde_json::Map::new();
                meta.insert("inputType".into(), "panel".into());
                Some(meta)
            },
            TuiCommand::Guide(_) => None,
        };

        // Attach subcommands to meta so the TUI can offer a sub-command dropdown
        let subs = self.subcommands();
        if !subs.is_empty() {
            let arr: Vec<serde_json::Value> = subs
                .into_iter()
                .map(|s| serde_json::Value::String(s.to_string()))
                .collect();
            let meta = meta.get_or_insert_with(serde_json::Map::new);
            meta.insert("subcommands".into(), serde_json::Value::Array(arr));

            // Include arg hints so the TUI knows which sub-commands need more input
            let hints = self.subcommand_hints();
            if !hints.is_empty() {
                let hints_map: serde_json::Map<String, serde_json::Value> = hints
                    .into_iter()
                    .map(|(name, hint)| (name.to_string(), serde_json::Value::String(hint.to_string())))
                    .collect();
                meta.insert("subcommandHints".into(), serde_json::Value::Object(hints_map));
            }
        }

        meta
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
            TuiCommand::Feedback(FeedbackArgs::default()),
            TuiCommand::Knowledge(KnowledgeArgs::default()),
            TuiCommand::Prompts(PromptsArgs::default()),
            TuiCommand::Chat(ChatArgs::default()),
            TuiCommand::Reply(ReplyArgs::default()),
            TuiCommand::Code(CodeArgs::default()),
            TuiCommand::Hooks(HooksArgs::default()),
            TuiCommand::Guide(GuideArgs::default()),
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
            "mcp" => Some(Self::Mcp(McpArgs {
                subcommand: (!args.is_empty()).then(|| args.to_string()),
            })),
            "tools" => Some(Self::Tools(ToolsArgs {
                subcommand: (!args.is_empty()).then(|| args.to_string()),
            })),
            "plan" => Some(Self::Plan(PlanArgs {
                prompt: (!args.is_empty()).then(|| args.to_string()),
            })),
            "feedback" => Some(Self::Feedback(FeedbackArgs {
                feedback_type: (!args.is_empty()).then(|| args.to_string()),
            })),
            "paste" => Some(Self::PasteImage(PasteImageArgs::default())),
            "knowledge" => Some(Self::Knowledge(KnowledgeArgs {
                subcommand: (!args.is_empty()).then(|| args.to_string()),
            })),
            "prompts" => Some(Self::Prompts(PromptsArgs {
                prompt_name: (!args.is_empty()).then(|| args.to_string()),
            })),
            "chat" => Some(Self::Chat(ChatArgs {
                subcommand: (!args.is_empty()).then(|| args.to_string()),
            })),
            "reply" => Some(Self::Reply(ReplyArgs::default())),
            "code" => Some(Self::Code(CodeArgs {
                subcommand: (!args.is_empty()).then(|| args.to_string()),
            })),
            "hooks" => Some(Self::Hooks(HooksArgs::default())),
            "guide" => Some(Self::Guide(GuideArgs {
                question: (!args.is_empty()).then(|| args.to_string()),
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

    #[test]
    fn test_parse_agent_no_args() {
        let cmd = TuiCommand::parse("agent", "").unwrap();
        assert!(matches!(cmd, TuiCommand::Agent(AgentArgs { agent_name: None })));
    }

    #[test]
    fn test_parse_agent_switch() {
        let cmd = TuiCommand::parse("agent", "my-agent").unwrap();
        match cmd {
            TuiCommand::Agent(args) => {
                assert_eq!(args.agent_name, Some("my-agent".to_string()));
            },
            _ => panic!("expected Agent"),
        }
    }

    #[test]
    fn test_parse_agent_create_subcommand() {
        let cmd = TuiCommand::parse("agent", "create myagent").unwrap();
        match cmd {
            TuiCommand::Agent(args) => {
                assert_eq!(args.agent_name, Some("create myagent".to_string()));
            },
            _ => panic!("expected Agent"),
        }
    }

    #[test]
    fn test_parse_agent_edit_subcommand() {
        let cmd = TuiCommand::parse("agent", "edit myagent").unwrap();
        match cmd {
            TuiCommand::Agent(args) => {
                assert_eq!(args.agent_name, Some("edit myagent".to_string()));
            },
            _ => panic!("expected Agent"),
        }
    }

    #[test]
    fn test_parse_agent_edit_no_name() {
        let cmd = TuiCommand::parse("agent", "edit").unwrap();
        match cmd {
            TuiCommand::Agent(args) => {
                assert_eq!(args.agent_name, Some("edit".to_string()));
            },
            _ => panic!("expected Agent"),
        }
    }

    #[test]
    fn test_agent_subcommands_listed() {
        let cmd = TuiCommand::Agent(AgentArgs::default());
        let subs = cmd.subcommands();
        assert!(subs.contains(&"create"));
        assert!(subs.contains(&"edit"));
    }

    #[test]
    fn test_parse_chat_no_args() {
        let cmd = TuiCommand::parse("chat", "").unwrap();
        match cmd {
            TuiCommand::Chat(args) => assert!(args.subcommand.is_none()),
            _ => panic!("expected Chat"),
        }
    }

    #[test]
    fn test_parse_chat_new() {
        let cmd = TuiCommand::parse("chat", "new").unwrap();
        match cmd {
            TuiCommand::Chat(args) => assert_eq!(args.subcommand, Some("new".to_string())),
            _ => panic!("expected Chat"),
        }
    }

    #[test]
    fn test_parse_chat_new_with_prompt() {
        let cmd = TuiCommand::parse("chat", "new hello world").unwrap();
        match cmd {
            TuiCommand::Chat(args) => assert_eq!(args.subcommand, Some("new hello world".to_string())),
            _ => panic!("expected Chat"),
        }
    }

    #[test]
    fn test_deserialize_chat_with_value_alias() {
        let json = r#"{"command":"chat","args":{"value":"new"}}"#;
        let cmd: TuiCommand = serde_json::from_str(json).unwrap();
        match cmd {
            TuiCommand::Chat(args) => assert_eq!(args.subcommand, Some("new".to_string())),
            _ => panic!("expected Chat"),
        }
    }

    #[test]
    fn test_deserialize_chat_empty_args() {
        let json = r#"{"command":"chat","args":{}}"#;
        let cmd: TuiCommand = serde_json::from_str(json).unwrap();
        match cmd {
            TuiCommand::Chat(args) => assert!(args.subcommand.is_none()),
            _ => panic!("expected Chat"),
        }
    }

    #[test]
    fn test_parse_hooks() {
        let cmd = TuiCommand::parse("hooks", "").unwrap();
        assert!(matches!(cmd, TuiCommand::Hooks(_)));
    }

    #[test]
    fn test_parse_hooks_ignores_args() {
        let cmd = TuiCommand::parse("hooks", "some extra args").unwrap();
        assert!(matches!(cmd, TuiCommand::Hooks(_)));
    }

    #[test]
    fn test_serialize_hooks() {
        let cmd = TuiCommand::Hooks(HooksArgs::default());
        let json = serde_json::to_string(&cmd).unwrap();
        assert!(json.contains(r#""command":"hooks""#));
    }

    #[test]
    fn test_deserialize_hooks() {
        let json = r#"{"command":"hooks","args":{}}"#;
        let cmd: TuiCommand = serde_json::from_str(json).unwrap();
        assert!(matches!(cmd, TuiCommand::Hooks(_)));
    }

    #[test]
    fn test_hooks_metadata() {
        let cmd = TuiCommand::Hooks(HooksArgs::default());
        assert_eq!(cmd.name(), "/hooks");
        assert_eq!(cmd.description(), "View configured hooks");
        assert_eq!(cmd.usage(), "/hooks");
        assert!(cmd.subcommands().is_empty());
        let meta = cmd.meta().expect("hooks should have meta");
        assert_eq!(meta.get("inputType").unwrap(), "panel");
    }

    #[test]
    fn test_hooks_in_all_commands() {
        let all = TuiCommand::all_commands();
        assert!(
            all.iter().any(|c| matches!(c, TuiCommand::Hooks(_))),
            "Hooks should be in all_commands()"
        );
    }

    #[test]
    fn test_parse_model_no_args() {
        let cmd = TuiCommand::parse("model", "").unwrap();
        assert!(matches!(cmd, TuiCommand::Model(ModelArgs { model_name: None })));
    }

    #[test]
    fn test_parse_model_switch() {
        let cmd = TuiCommand::parse("model", "claude-sonnet-4").unwrap();
        match cmd {
            TuiCommand::Model(args) => {
                assert_eq!(args.model_name, Some("claude-sonnet-4".to_string()));
            },
            _ => panic!("expected Model"),
        }
    }

    #[test]
    fn test_parse_model_set_current_as_default() {
        let cmd = TuiCommand::parse("model", "set-current-as-default").unwrap();
        match cmd {
            TuiCommand::Model(args) => {
                assert_eq!(args.model_name, Some("set-current-as-default".to_string()));
            },
            _ => panic!("expected Model"),
        }
    }

    #[test]
    fn test_model_subcommands_empty() {
        let cmd = TuiCommand::Model(ModelArgs::default());
        let subs = cmd.subcommands();
        assert!(subs.is_empty(), "model should have no subcommands");
    }

    #[test]
    fn test_meta_includes_subcommands_for_agent() {
        let cmd = TuiCommand::Agent(AgentArgs::default());
        let meta = cmd.meta().expect("agent should have meta");
        let subs = meta.get("subcommands").expect("agent meta should have subcommands");
        let arr = subs.as_array().expect("subcommands should be an array");
        let values: Vec<&str> = arr.iter().map(|v| v.as_str().unwrap()).collect();
        assert!(values.contains(&"create"));
        assert!(values.contains(&"edit"));
        assert!(values.contains(&"swap"));
    }

    #[test]
    fn test_meta_includes_subcommands_for_context() {
        let cmd = TuiCommand::Context(ContextArgs::default());
        let meta = cmd.meta().expect("context should have meta");
        let subs = meta.get("subcommands").expect("context meta should have subcommands");
        let arr = subs.as_array().expect("subcommands should be an array");
        let values: Vec<&str> = arr.iter().map(|v| v.as_str().unwrap()).collect();
        assert!(values.contains(&"add"));
        assert!(values.contains(&"remove"));
        assert!(values.contains(&"clear"));
    }

    #[test]
    fn test_meta_includes_subcommands_for_chat() {
        let cmd = TuiCommand::Chat(ChatArgs::default());
        let meta = cmd.meta().expect("chat should have meta");
        let subs = meta.get("subcommands").expect("chat meta should have subcommands");
        let arr = subs.as_array().expect("subcommands should be an array");
        let values: Vec<&str> = arr.iter().map(|v| v.as_str().unwrap()).collect();
        assert!(values.contains(&"save"));
        assert!(values.contains(&"load"));
        assert!(values.contains(&"new"));
    }

    #[test]
    fn test_meta_excludes_subcommands_for_commands_without_them() {
        let cmd = TuiCommand::Hooks(HooksArgs::default());
        let meta = cmd.meta().expect("hooks should have meta");
        assert!(
            meta.get("subcommands").is_none(),
            "hooks should not have subcommands in meta"
        );

        let cmd = TuiCommand::Help(HelpArgs::default());
        let meta = cmd.meta().expect("help should have meta");
        assert!(
            meta.get("subcommands").is_none(),
            "help should not have subcommands in meta"
        );
    }

    #[test]
    fn test_meta_no_subcommands_for_commands_with_no_meta() {
        // Commands that return None from meta() and have no subcommands should stay None
        let cmd = TuiCommand::Clear(ClearArgs::default());
        assert!(cmd.subcommands().is_empty());
        assert!(cmd.meta().is_none(), "clear should have no meta");

        let cmd = TuiCommand::Compact(CompactArgs::default());
        assert!(cmd.subcommands().is_empty());
        assert!(cmd.meta().is_none(), "compact should have no meta");
    }

    #[test]
    fn test_meta_subcommands_match_subcommands_method() {
        // For every command, meta subcommands should exactly match subcommands()
        for cmd in TuiCommand::all_commands() {
            let subs = cmd.subcommands();
            let meta = cmd.meta();
            if subs.is_empty() {
                // Should not have subcommands key in meta
                if let Some(ref m) = meta {
                    assert!(
                        m.get("subcommands").is_none(),
                        "{} has empty subcommands() but meta contains subcommands key",
                        cmd.name()
                    );
                }
            } else {
                // Should have subcommands key in meta matching exactly
                let m = meta.unwrap_or_else(|| panic!("{} has subcommands but no meta", cmd.name()));
                let arr = m
                    .get("subcommands")
                    .unwrap_or_else(|| panic!("{} meta missing subcommands key", cmd.name()))
                    .as_array()
                    .unwrap();
                let meta_subs: Vec<&str> = arr.iter().map(|v| v.as_str().unwrap()).collect();
                assert_eq!(
                    meta_subs,
                    subs,
                    "{} meta subcommands don't match subcommands()",
                    cmd.name()
                );
            }
        }
    }

    #[test]
    fn test_subcommand_hints_for_agent() {
        let cmd = TuiCommand::Agent(AgentArgs::default());
        let hints = cmd.subcommand_hints();
        assert!(hints.contains(&("create", "<name>")));
        assert!(hints.contains(&("edit", "[name]")));
        assert!(hints.contains(&("swap", "<name>")));
    }

    #[test]
    fn test_subcommand_hints_for_context() {
        let cmd = TuiCommand::Context(ContextArgs::default());
        let hints = cmd.subcommand_hints();
        // "add" and "remove" need args, "clear" does not
        assert!(hints.iter().any(|(name, _)| *name == "add"));
        assert!(hints.iter().any(|(name, _)| *name == "remove"));
        assert!(
            !hints.iter().any(|(name, _)| *name == "clear"),
            "clear should not have a hint"
        );
    }

    #[test]
    fn test_subcommand_hints_for_tools() {
        let cmd = TuiCommand::Tools(ToolsArgs::default());
        let hints = cmd.subcommand_hints();
        // "trust" and "untrust" need args, "trust-all" and "reset" do not
        assert!(hints.iter().any(|(name, _)| *name == "trust"));
        assert!(hints.iter().any(|(name, _)| *name == "untrust"));
        assert!(
            !hints.iter().any(|(name, _)| *name == "trust-all"),
            "trust-all should not have a hint"
        );
        assert!(
            !hints.iter().any(|(name, _)| *name == "reset"),
            "reset should not have a hint"
        );
    }

    #[test]
    fn test_meta_includes_subcommand_hints() {
        let cmd = TuiCommand::Agent(AgentArgs::default());
        let meta = cmd.meta().expect("agent should have meta");
        let hints = meta
            .get("subcommandHints")
            .expect("agent meta should have subcommandHints");
        let obj = hints.as_object().expect("subcommandHints should be an object");
        assert_eq!(obj.get("create").unwrap().as_str().unwrap(), "<name>");
        assert_eq!(obj.get("swap").unwrap().as_str().unwrap(), "<name>");
    }

    #[test]
    fn test_meta_no_subcommand_hints_when_none_needed() {
        // /code has subcommands but none need args
        let cmd = TuiCommand::Code(CodeArgs::default());
        let meta = cmd.meta().expect("code should have meta");
        assert!(meta.get("subcommands").is_some(), "code should have subcommands");
        assert!(
            meta.get("subcommandHints").is_none(),
            "code should not have subcommandHints since no sub-commands need args"
        );
    }

    #[test]
    fn test_subcommand_hints_only_for_subcommands_that_exist() {
        // Every hint key should be a valid subcommand
        for cmd in TuiCommand::all_commands() {
            let subs = cmd.subcommands();
            let hints = cmd.subcommand_hints();
            for (hint_name, _) in &hints {
                assert!(
                    subs.contains(hint_name),
                    "{}: subcommand_hints contains '{}' which is not in subcommands()",
                    cmd.name(),
                    hint_name
                );
            }
        }
    }
}
