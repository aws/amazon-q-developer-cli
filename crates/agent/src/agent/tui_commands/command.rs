//! Strongly-typed slash command enum for ACP extension method execution.
//!
//! This enum is shared with TypeScript via typeshare, providing compile-time
//! type safety across the Rust/TypeScript boundary.
//!
//! Types live here (agent crate) for typeshare generation.
//! Execution logic lives in the host crate where ApiClient is available.

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
    /// Voice input mode
    Voice(VoiceArgs),
    /// View configured hooks
    Hooks(HooksArgs),
    /// Switch to the guide agent for help with Kiro CLI
    Guide(GuideArgs),
    /// Rewind to a previous turn (clones history into a new session)
    Rewind(RewindArgs),
    /// Show request stats for debugging slow turns
    Stats(StatsArgs),
    /// Set thinking effort for this session
    Effort(EffortArgs),
    /// Set a goal with validation criteria for iterative completion
    Goal(GoalArgs),
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

/// Arguments for /voice command
#[typeshare]
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VoiceArgs {
    /// Enable continuous voice mode (auto-record after each response)
    #[serde(default)]
    pub continuous: bool,
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

/// Arguments for /rewind command
#[typeshare]
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RewindArgs {
    /// Log entry index of the selected `Prompt` entry. If None, shows the picker.
    /// Accepts either `turnIndex` or `value` (for generic selection UI).
    #[serde(alias = "value", skip_serializing_if = "Option::is_none")]
    pub turn_index: Option<String>,
}

/// Arguments for /stats command
#[typeshare]
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StatsArgs {
    /// Subcommand: "save <filename>" to export to file
    #[serde(default, alias = "value", skip_serializing_if = "Option::is_none")]
    pub subcommand: Option<String>,
    /// Show only the last N requests (default: all)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last: Option<u32>,
}

/// Arguments for /effort command
#[typeshare]
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EffortArgs {
    /// Effort level to set. If None, shows available levels.
    /// Accepts either `level` or `value` (for generic selection UI)
    #[serde(alias = "value", skip_serializing_if = "Option::is_none")]
    pub level: Option<String>,
}

/// Arguments for /goal command
#[typeshare]
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GoalArgs {
    #[serde(alias = "value", default, skip_serializing_if = "Option::is_none")]
    pub subcommand: Option<String>,
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
            TuiCommand::Voice(_) => "/voice",
            TuiCommand::Hooks(_) => "/hooks",
            TuiCommand::Guide(_) => "/guide",
            TuiCommand::Rewind(_) => "/rewind",
            TuiCommand::Stats(_) => "/stats",
            TuiCommand::Effort(_) => "/effort",
            TuiCommand::Goal(_) => "/goal",
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
            TuiCommand::Voice(_) => "Voice input mode for hands-free interaction",
            TuiCommand::Hooks(_) => "View configured hooks",
            TuiCommand::Guide(_) => "Get help with Kiro CLI features from the guide agent",
            TuiCommand::Rewind(_) => "Rewind conversation to a previous turn (forks into a new session)",
            TuiCommand::Stats(_) => "Show request IDs and timings for debugging slow turns",
            TuiCommand::Effort(_) => "Set thinking effort for this session",
            TuiCommand::Goal(_) => "Set a goal with validation criteria for iterative completion",
        }
    }

    /// Usage example
    pub fn usage(&self) -> &'static str {
        match self {
            TuiCommand::Help(_) => "/help",
            TuiCommand::Model(_) => "/model [model-name|set-current-as-default]",
            TuiCommand::Agent(_) => "/agent [agent-name|create <name>|edit [name]|swap <name>]",
            TuiCommand::Context(_) => "/context [show|add [--force] <path>...|remove <path>...|clear]",
            TuiCommand::Compact(_) => "/compact",
            TuiCommand::Clear(_) => "/clear",
            TuiCommand::Quit(_) => "/quit",
            TuiCommand::Usage(_) => "/usage",
            TuiCommand::PasteImage(_) => "/paste",
            TuiCommand::Mcp(_) => "/mcp [list|add <name>|remove <name>]",
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
            TuiCommand::Voice(_) => "/voice [start|stop|status]",
            TuiCommand::Hooks(_) => "/hooks",
            TuiCommand::Guide(_) => "/guide [question]",
            TuiCommand::Rewind(_) => "/rewind",
            TuiCommand::Stats(_) => "/stats [N|save <filename>]",
            TuiCommand::Effort(_) => "/effort [level|set-current-as-default]",
            TuiCommand::Goal(_) => "/goal [description --validate criteria --agent name --max N] | clear",
        }
    }

    /// Subcommand names, if any
    pub fn subcommands(&self) -> Vec<&'static str> {
        match self {
            TuiCommand::Agent(_) => vec!["create", "edit", "swap"],
            TuiCommand::Model(_) => vec!["set-current-as-default"],
            TuiCommand::Effort(_) => vec!["set-current-as-default"],
            TuiCommand::Context(_) => vec!["show", "add", "remove", "clear"],
            TuiCommand::Knowledge(_) => vec!["show", "add", "remove", "update", "clear", "cancel"],
            TuiCommand::Tools(_) => vec!["trust-all", "trust", "untrust", "reset"],
            TuiCommand::Chat(_) => vec!["save", "load", "new"],
            TuiCommand::Code(_) => vec!["status", "init", "logs", "overview", "summary"],
            TuiCommand::Voice(_) => vec!["start", "stop", "status"],
            TuiCommand::Mcp(_) => vec!["list", "add", "remove"],
            TuiCommand::Goal(_) => vec!["clear"],
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
            TuiCommand::Goal(_) => vec![],
            _ => vec![],
        }
    }

    /// Human-readable descriptions shown next to each subcommand in the TUI dropdown.
    pub fn subcommand_descriptions(&self) -> Vec<(&'static str, &'static str)> {
        match self {
            TuiCommand::Agent(_) => vec![
                ("create", "Create a new agent"),
                ("edit", "Edit an agent config in $EDITOR"),
                ("swap", "Switch to a different agent"),
            ],
            TuiCommand::Model(_) => vec![("set-current-as-default", "Save the active model as the default")],
            TuiCommand::Effort(_) => {
                vec![("set-current-as-default", "Save the active effort level as the default")]
            },
            TuiCommand::Context(_) => vec![
                ("show", "Show context files and usage"),
                ("add", "Add files to context"),
                ("remove", "Remove files from context"),
                ("clear", "Remove all files from context"),
            ],
            TuiCommand::Knowledge(_) => vec![
                ("show", "Show knowledge bases"),
                ("add", "Index a file or directory"),
                ("remove", "Remove a knowledge base"),
                ("update", "Re-index a knowledge base"),
                ("clear", "Remove all knowledge bases"),
                ("cancel", "Cancel background indexing"),
            ],
            TuiCommand::Tools(_) => vec![
                ("trust-all", "Trust all tools for this session"),
                ("trust", "Trust a tool"),
                ("untrust", "Require approval for a tool"),
                ("reset", "Reset tool permissions to defaults"),
            ],
            TuiCommand::Chat(_) => vec![
                ("save", "Save the conversation to a file"),
                ("load", "Load a conversation from a file"),
                ("new", "Start a fresh session"),
            ],
            TuiCommand::Code(_) => vec![
                ("status", "Show code intelligence status"),
                ("init", "Initialize code intelligence for the workspace"),
                ("logs", "Show code intelligence logs"),
                ("overview", "Generate a codebase overview"),
                ("summary", "Summarize the workspace"),
            ],
            TuiCommand::Voice(_) => vec![
                ("start", "Start voice input"),
                ("stop", "Stop voice input"),
                ("status", "Show voice input status"),
            ],
            TuiCommand::Mcp(_) => vec![
                ("list", "List MCP servers"),
                ("add", "Add an MCP server"),
                ("remove", "Remove an MCP server"),
            ],
            TuiCommand::Goal(_) => vec![("clear", "Clear the active goal")],
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
            TuiCommand::Voice(_) => None,
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
            TuiCommand::Rewind(_) => {
                let mut meta = serde_json::Map::new();
                meta.insert("inputType".into(), "panel".into());
                Some(meta)
            },
            TuiCommand::Stats(_) => {
                let mut meta = serde_json::Map::new();
                meta.insert("inputType".into(), "panel".into());
                meta.insert("hidden".into(), true.into());
                Some(meta)
            },
            TuiCommand::Effort(_) => {
                let mut meta = serde_json::Map::new();
                meta.insert("inputType".into(), "selection".into());
                meta.insert("hint".into(), "".into());
                meta.insert("searchable".into(), false.into());
                Some(meta)
            },
            TuiCommand::Goal(_) => {
                let mut meta = serde_json::Map::new();
                meta.insert("inputType".into(), "panel".into());
                Some(meta)
            },
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

            let descriptions = self.subcommand_descriptions();
            if !descriptions.is_empty() {
                let desc_map: serde_json::Map<String, serde_json::Value> = descriptions
                    .into_iter()
                    .map(|(name, desc)| (name.to_string(), serde_json::Value::String(desc.to_string())))
                    .collect();
                meta.insert("subcommandDescriptions".into(), serde_json::Value::Object(desc_map));
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
            TuiCommand::Voice(VoiceArgs::default()),
            TuiCommand::Hooks(HooksArgs::default()),
            TuiCommand::Guide(GuideArgs::default()),
            TuiCommand::Rewind(RewindArgs::default()),
            TuiCommand::Stats(StatsArgs::default()),
            TuiCommand::Effort(EffortArgs::default()),
            TuiCommand::Goal(GoalArgs::default()),
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
            "voice" => Some(Self::Voice(VoiceArgs {
                continuous: args == "--continuous" || args == "-c",
            })),
            "guide" => Some(Self::Guide(GuideArgs {
                question: (!args.is_empty()).then(|| args.to_string()),
            })),
            "rewind" => Some(Self::Rewind(RewindArgs {
                turn_index: (!args.is_empty()).then(|| args.to_string()),
            })),
            "stats" => {
                if args.is_empty() {
                    Some(Self::Stats(StatsArgs::default()))
                } else if let Ok(n) = args.parse::<u32>() {
                    Some(Self::Stats(StatsArgs {
                        last: Some(n),
                        ..Default::default()
                    }))
                } else {
                    Some(Self::Stats(StatsArgs {
                        subcommand: Some(args.to_string()),
                        ..Default::default()
                    }))
                }
            },
            "effort" => Some(Self::Effort(EffortArgs {
                level: (!args.is_empty()).then(|| args.to_string()),
            })),
            "goal" => Some(Self::Goal(GoalArgs {
                subcommand: (!args.is_empty()).then(|| args.to_string()),
            })),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_subcommand_has_a_description() {
        for cmd in TuiCommand::all_commands() {
            let descriptions: Vec<&str> = cmd
                .subcommand_descriptions()
                .into_iter()
                .map(|(name, _)| name)
                .collect();
            for sub in cmd.subcommands() {
                assert!(
                    descriptions.contains(&sub),
                    "{} {sub} is missing a subcommand_descriptions entry",
                    cmd.name()
                );
            }
        }
    }

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
    fn test_model_subcommands_listed() {
        let cmd = TuiCommand::Model(ModelArgs::default());
        let subs = cmd.subcommands();
        assert!(subs.contains(&"set-current-as-default"));
    }

    #[test]
    fn test_parse_effort_set_current_as_default() {
        let cmd = TuiCommand::parse("effort", "set-current-as-default").unwrap();
        match cmd {
            TuiCommand::Effort(args) => {
                assert_eq!(args.level, Some("set-current-as-default".to_string()));
            },
            _ => panic!("expected Effort"),
        }
    }

    #[test]
    fn test_effort_subcommands_listed() {
        let cmd = TuiCommand::Effort(EffortArgs::default());
        let subs = cmd.subcommands();
        assert!(subs.contains(&"set-current-as-default"));
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

    #[test]
    fn test_subcommands_match_usage_string() {
        // Every subcommand listed in subcommands() should appear in the usage() string.
        // This catches drift between the two lists.
        for cmd in TuiCommand::all_commands() {
            let subs = cmd.subcommands();
            let usage = cmd.usage();
            for sub in &subs {
                assert!(
                    usage.contains(sub),
                    "{}: subcommand '{}' is in subcommands() but not in usage() \"{}\"",
                    cmd.name(),
                    sub,
                    usage
                );
            }
        }
    }

    // === Serde roundtrip for ALL variants ===

    #[test]
    fn test_serde_roundtrip_all_variants() {
        let commands = vec![
            TuiCommand::Help(HelpArgs::default()),
            TuiCommand::Model(ModelArgs {
                model_name: Some("test".into()),
            }),
            TuiCommand::Agent(AgentArgs {
                agent_name: Some("a".into()),
            }),
            TuiCommand::Context(ContextArgs {
                verbose: true,
                subcommand: Some("show".into()),
            }),
            TuiCommand::Compact(CompactArgs {
                target_tokens: Some(1000),
            }),
            TuiCommand::Clear(ClearArgs::default()),
            TuiCommand::Quit(QuitArgs::default()),
            TuiCommand::Usage(UsageArgs::default()),
            TuiCommand::PasteImage(PasteImageArgs::default()),
            TuiCommand::Mcp(McpArgs {
                subcommand: Some("list".into()),
            }),
            TuiCommand::Tools(ToolsArgs {
                subcommand: Some("trust-all".into()),
            }),
            TuiCommand::Plan(PlanArgs {
                prompt: Some("build it".into()),
            }),
            TuiCommand::Feedback(FeedbackArgs {
                feedback_type: Some("issue".into()),
            }),
            TuiCommand::Chat(ChatArgs {
                subcommand: Some("save path".into()),
            }),
            TuiCommand::Knowledge(KnowledgeArgs {
                subcommand: Some("add x y".into()),
            }),
            TuiCommand::Prompts(PromptsArgs {
                prompt_name: Some("p".into()),
            }),
            TuiCommand::Reply(ReplyArgs::default()),
            TuiCommand::Code(CodeArgs {
                subcommand: Some("status".into()),
            }),
            TuiCommand::Voice(VoiceArgs { continuous: true }),
            TuiCommand::Hooks(HooksArgs::default()),
            TuiCommand::Guide(GuideArgs {
                question: Some("how?".into()),
            }),
            TuiCommand::Rewind(RewindArgs {
                turn_index: Some("3".into()),
            }),
            TuiCommand::Stats(StatsArgs {
                subcommand: Some("save f.json".into()),
                last: Some(5),
            }),
            TuiCommand::Effort(EffortArgs {
                level: Some("high".into()),
            }),
        ];
        for cmd in commands {
            let json = serde_json::to_string(&cmd).unwrap();
            let parsed: TuiCommand = serde_json::from_str(&json).unwrap();
            assert_eq!(cmd.name(), parsed.name(), "roundtrip failed for {}", cmd.name());
        }
    }

    #[test]
    fn test_serde_paste_image_rename() {
        // PasteImage uses #[serde(rename = "paste")]
        let cmd = TuiCommand::PasteImage(PasteImageArgs::default());
        let json = serde_json::to_string(&cmd).unwrap();
        assert!(json.contains(r#""command":"paste""#));
        let parsed: TuiCommand = serde_json::from_str(&json).unwrap();
        assert!(matches!(parsed, TuiCommand::PasteImage(_)));
    }

    #[test]
    fn test_serde_skip_serializing_none_fields() {
        let cmd = TuiCommand::Model(ModelArgs { model_name: None });
        let json = serde_json::to_string(&cmd).unwrap();
        assert!(!json.contains("modelName"));
    }

    // === Parse edge cases ===

    #[test]
    fn test_parse_unknown_command_returns_none() {
        assert!(TuiCommand::parse("nonexistent", "").is_none());
        assert!(TuiCommand::parse("", "").is_none());
        assert!(TuiCommand::parse("HELP", "").is_none()); // case sensitive
    }

    #[test]
    fn test_parse_compact_with_number() {
        let cmd = TuiCommand::parse("compact", "5000").unwrap();
        match cmd {
            TuiCommand::Compact(args) => assert_eq!(args.target_tokens, Some(5000)),
            _ => panic!("expected Compact"),
        }
    }

    #[test]
    fn test_parse_compact_with_invalid_number() {
        let cmd = TuiCommand::parse("compact", "abc").unwrap();
        match cmd {
            TuiCommand::Compact(args) => assert_eq!(args.target_tokens, None),
            _ => panic!("expected Compact"),
        }
    }

    #[test]
    fn test_parse_compact_empty() {
        let cmd = TuiCommand::parse("compact", "").unwrap();
        match cmd {
            TuiCommand::Compact(args) => assert_eq!(args.target_tokens, None),
            _ => panic!("expected Compact"),
        }
    }

    #[test]
    fn test_parse_voice_continuous_flag() {
        let cmd = TuiCommand::parse("voice", "--continuous").unwrap();
        match cmd {
            TuiCommand::Voice(args) => assert!(args.continuous),
            _ => panic!("expected Voice"),
        }
    }

    #[test]
    fn test_parse_voice_short_flag() {
        let cmd = TuiCommand::parse("voice", "-c").unwrap();
        match cmd {
            TuiCommand::Voice(args) => assert!(args.continuous),
            _ => panic!("expected Voice"),
        }
    }

    #[test]
    fn test_parse_voice_no_flag() {
        let cmd = TuiCommand::parse("voice", "").unwrap();
        match cmd {
            TuiCommand::Voice(args) => assert!(!args.continuous),
            _ => panic!("expected Voice"),
        }
    }

    #[test]
    fn test_parse_voice_other_arg() {
        let cmd = TuiCommand::parse("voice", "start").unwrap();
        match cmd {
            TuiCommand::Voice(args) => assert!(!args.continuous),
            _ => panic!("expected Voice"),
        }
    }

    #[test]
    fn test_parse_stats_empty() {
        let cmd = TuiCommand::parse("stats", "").unwrap();
        match cmd {
            TuiCommand::Stats(args) => {
                assert!(args.last.is_none());
                assert!(args.subcommand.is_none());
            },
            _ => panic!("expected Stats"),
        }
    }

    #[test]
    fn test_parse_stats_with_number() {
        let cmd = TuiCommand::parse("stats", "10").unwrap();
        match cmd {
            TuiCommand::Stats(args) => {
                assert_eq!(args.last, Some(10));
                assert!(args.subcommand.is_none());
            },
            _ => panic!("expected Stats"),
        }
    }

    #[test]
    fn test_parse_stats_with_subcommand() {
        let cmd = TuiCommand::parse("stats", "save output.json").unwrap();
        match cmd {
            TuiCommand::Stats(args) => {
                assert!(args.last.is_none());
                assert_eq!(args.subcommand, Some("save output.json".to_string()));
            },
            _ => panic!("expected Stats"),
        }
    }

    #[test]
    fn test_parse_effort_with_level() {
        let cmd = TuiCommand::parse("effort", "high").unwrap();
        match cmd {
            TuiCommand::Effort(args) => assert_eq!(args.level, Some("high".to_string())),
            _ => panic!("expected Effort"),
        }
    }

    #[test]
    fn test_parse_effort_empty() {
        let cmd = TuiCommand::parse("effort", "").unwrap();
        match cmd {
            TuiCommand::Effort(args) => assert!(args.level.is_none()),
            _ => panic!("expected Effort"),
        }
    }

    #[test]
    fn test_parse_guide_with_question() {
        let cmd = TuiCommand::parse("guide", "how do I use tools?").unwrap();
        match cmd {
            TuiCommand::Guide(args) => assert_eq!(args.question, Some("how do I use tools?".to_string())),
            _ => panic!("expected Guide"),
        }
    }

    #[test]
    fn test_parse_rewind_with_index() {
        let cmd = TuiCommand::parse("rewind", "5").unwrap();
        match cmd {
            TuiCommand::Rewind(args) => assert_eq!(args.turn_index, Some("5".to_string())),
            _ => panic!("expected Rewind"),
        }
    }

    #[test]
    fn test_parse_all_no_arg_commands() {
        for (name, expected_name) in [
            ("help", "/help"),
            ("clear", "/clear"),
            ("quit", "/quit"),
            ("usage", "/usage"),
            ("paste", "/paste"),
            ("reply", "/reply"),
            ("hooks", "/hooks"),
        ] {
            let cmd = TuiCommand::parse(name, "").unwrap();
            assert_eq!(cmd.name(), expected_name, "parse({name}) name mismatch");
        }
    }

    #[test]
    fn test_parse_all_optional_arg_commands() {
        for (name, arg, expected_name) in [
            ("model", "x", "/model"),
            ("agent", "x", "/agent"),
            ("mcp", "list", "/mcp"),
            ("tools", "trust x", "/tools"),
            ("plan", "do stuff", "/plan"),
            ("feedback", "issue", "/feedback"),
            ("knowledge", "show", "/knowledge"),
            ("prompts", "my-prompt", "/prompts"),
            ("chat", "new", "/chat"),
            ("code", "status", "/code"),
            ("guide", "q", "/guide"),
            ("rewind", "2", "/rewind"),
            ("effort", "low", "/effort"),
        ] {
            let cmd = TuiCommand::parse(name, arg).unwrap();
            assert_eq!(cmd.name(), expected_name, "parse({name}, {arg}) name mismatch");
        }
    }

    // === Deserialization error paths ===

    #[test]
    fn test_deserialize_invalid_command_name() {
        let json = r#"{"command":"invalid","args":{}}"#;
        let result = serde_json::from_str::<TuiCommand>(json);
        assert!(result.is_err());
    }

    #[test]
    fn test_deserialize_missing_args() {
        let json = r#"{"command":"model"}"#;
        let result = serde_json::from_str::<TuiCommand>(json);
        assert!(result.is_err());
    }

    #[test]
    fn test_deserialize_malformed_json() {
        let result = serde_json::from_str::<TuiCommand>("not json");
        assert!(result.is_err());
    }

    // === name(), description(), usage() coverage for all variants ===

    #[test]
    fn test_all_commands_have_slash_prefix_name() {
        for cmd in TuiCommand::all_commands() {
            assert!(cmd.name().starts_with('/'), "{} name should start with /", cmd.name());
        }
    }

    #[test]
    fn test_all_commands_have_nonempty_description() {
        for cmd in TuiCommand::all_commands() {
            assert!(!cmd.description().is_empty(), "{} has empty description", cmd.name());
        }
    }

    #[test]
    fn test_all_commands_have_nonempty_usage() {
        for cmd in TuiCommand::all_commands() {
            assert!(!cmd.usage().is_empty(), "{} has empty usage", cmd.name());
            assert!(cmd.usage().starts_with('/'), "{} usage should start with /", cmd.name());
        }
    }

    // === Meta coverage for variants not yet tested ===

    #[test]
    fn test_meta_none_variants() {
        // These should return None from meta (before subcommand injection)
        let none_variants = vec![
            TuiCommand::Clear(ClearArgs::default()),
            TuiCommand::Compact(CompactArgs::default()),
            TuiCommand::PasteImage(PasteImageArgs::default()),
            TuiCommand::Plan(PlanArgs::default()),
            TuiCommand::Reply(ReplyArgs::default()),
            TuiCommand::Voice(VoiceArgs::default()),
            TuiCommand::Guide(GuideArgs::default()),
        ];
        for cmd in none_variants {
            // Voice has subcommands so meta won't be None
            if !cmd.subcommands().is_empty() {
                assert!(
                    cmd.meta().is_some(),
                    "{} has subcommands so meta should be Some",
                    cmd.name()
                );
            } else {
                assert!(cmd.meta().is_none(), "{} should have no meta", cmd.name());
            }
        }
    }

    #[test]
    fn test_meta_panel_variants() {
        let panel_variants = vec![
            TuiCommand::Help(HelpArgs::default()),
            TuiCommand::Usage(UsageArgs::default()),
            TuiCommand::Mcp(McpArgs::default()),
            TuiCommand::Tools(ToolsArgs::default()),
            TuiCommand::Knowledge(KnowledgeArgs::default()),
            TuiCommand::Code(CodeArgs::default()),
            TuiCommand::Hooks(HooksArgs::default()),
            TuiCommand::Rewind(RewindArgs::default()),
            TuiCommand::Stats(StatsArgs::default()),
            TuiCommand::Context(ContextArgs::default()),
        ];
        for cmd in panel_variants {
            let meta = cmd.meta().unwrap_or_else(|| panic!("{} should have meta", cmd.name()));
            assert_eq!(
                meta.get("inputType").unwrap().as_str().unwrap(),
                "panel",
                "{} should have inputType=panel",
                cmd.name()
            );
        }
    }

    #[test]
    fn test_meta_selection_variants() {
        let selection_variants = vec![
            TuiCommand::Model(ModelArgs::default()),
            TuiCommand::Agent(AgentArgs::default()),
            TuiCommand::Feedback(FeedbackArgs::default()),
            TuiCommand::Prompts(PromptsArgs::default()),
            TuiCommand::Chat(ChatArgs::default()),
            TuiCommand::Effort(EffortArgs::default()),
        ];
        for cmd in selection_variants {
            let meta = cmd.meta().unwrap_or_else(|| panic!("{} should have meta", cmd.name()));
            assert_eq!(
                meta.get("inputType").unwrap().as_str().unwrap(),
                "selection",
                "{} should have inputType=selection",
                cmd.name()
            );
        }
    }

    #[test]
    fn test_meta_quit_local() {
        let cmd = TuiCommand::Quit(QuitArgs::default());
        let meta = cmd.meta().expect("quit should have meta");
        assert_eq!(meta.get("local").unwrap(), &serde_json::Value::Bool(true));
    }

    #[test]
    fn test_meta_stats_hidden() {
        let cmd = TuiCommand::Stats(StatsArgs::default());
        let meta = cmd.meta().expect("stats should have meta");
        assert_eq!(meta.get("hidden").unwrap(), &serde_json::Value::Bool(true));
    }

    #[test]
    fn test_meta_feedback_not_searchable() {
        let cmd = TuiCommand::Feedback(FeedbackArgs::default());
        let meta = cmd.meta().expect("feedback should have meta");
        assert_eq!(meta.get("searchable").unwrap(), &serde_json::Value::Bool(false));
    }

    #[test]
    fn test_meta_model_options_method() {
        let cmd = TuiCommand::Model(ModelArgs::default());
        let meta = cmd.meta().expect("model should have meta");
        assert_eq!(
            meta.get("optionsMethod").unwrap().as_str().unwrap(),
            "_kiro.dev/commands/model/options"
        );
    }

    #[test]
    fn test_meta_agent_options_method() {
        let cmd = TuiCommand::Agent(AgentArgs::default());
        let meta = cmd.meta().expect("agent should have meta");
        assert_eq!(
            meta.get("optionsMethod").unwrap().as_str().unwrap(),
            "_kiro.dev/commands/agent/options"
        );
    }

    #[test]
    fn test_meta_prompts_options_method() {
        let cmd = TuiCommand::Prompts(PromptsArgs::default());
        let meta = cmd.meta().expect("prompts should have meta");
        assert_eq!(
            meta.get("optionsMethod").unwrap().as_str().unwrap(),
            "_kiro.dev/commands/prompts/options"
        );
    }

    // === Subcommands and hints for remaining variants ===

    #[test]
    fn test_subcommands_knowledge() {
        let cmd = TuiCommand::Knowledge(KnowledgeArgs::default());
        let subs = cmd.subcommands();
        assert_eq!(subs, vec!["show", "add", "remove", "update", "clear", "cancel"]);
    }

    #[test]
    fn test_subcommands_tools() {
        let cmd = TuiCommand::Tools(ToolsArgs::default());
        let subs = cmd.subcommands();
        assert_eq!(subs, vec!["trust-all", "trust", "untrust", "reset"]);
    }

    #[test]
    fn test_subcommands_code() {
        let cmd = TuiCommand::Code(CodeArgs::default());
        let subs = cmd.subcommands();
        assert_eq!(subs, vec!["status", "init", "logs", "overview", "summary"]);
    }

    #[test]
    fn test_subcommands_voice() {
        let cmd = TuiCommand::Voice(VoiceArgs::default());
        let subs = cmd.subcommands();
        assert_eq!(subs, vec!["start", "stop", "status"]);
    }

    #[test]
    fn test_subcommands_mcp() {
        let cmd = TuiCommand::Mcp(McpArgs::default());
        let subs = cmd.subcommands();
        assert_eq!(subs, vec!["list", "add", "remove"]);
    }

    #[test]
    fn test_subcommand_hints_knowledge() {
        let cmd = TuiCommand::Knowledge(KnowledgeArgs::default());
        let hints = cmd.subcommand_hints();
        assert!(hints.iter().any(|(n, _)| *n == "add"));
        assert!(hints.iter().any(|(n, _)| *n == "remove"));
        assert!(hints.iter().any(|(n, _)| *n == "update"));
    }

    #[test]
    fn test_subcommand_hints_chat() {
        let cmd = TuiCommand::Chat(ChatArgs::default());
        let hints = cmd.subcommand_hints();
        assert!(hints.iter().any(|(n, _)| *n == "save"));
        assert!(hints.iter().any(|(n, _)| *n == "load"));
        assert!(hints.iter().any(|(n, _)| *n == "new"));
    }

    #[test]
    fn test_subcommand_hints_mcp() {
        let cmd = TuiCommand::Mcp(McpArgs::default());
        let hints = cmd.subcommand_hints();
        assert!(hints.iter().any(|(n, _)| *n == "add"));
        assert!(hints.iter().any(|(n, _)| *n == "remove"));
        assert!(!hints.iter().any(|(n, _)| *n == "list"));
    }

    #[test]
    fn test_subcommand_hints_empty_for_no_subcommand_variants() {
        let cmd = TuiCommand::Help(HelpArgs::default());
        assert!(cmd.subcommand_hints().is_empty());
        let cmd = TuiCommand::Clear(ClearArgs::default());
        assert!(cmd.subcommand_hints().is_empty());
    }

    // === all_commands() coverage ===

    #[test]
    fn test_all_commands_sorted_by_name() {
        let all = TuiCommand::all_commands();
        let names: Vec<&str> = all.iter().map(|c| c.name()).collect();
        let mut sorted = names.clone();
        sorted.sort();
        assert_eq!(names, sorted, "all_commands() should be sorted by name");
    }

    #[test]
    fn test_all_commands_count() {
        let all = TuiCommand::all_commands();
        // Should have exactly 25 commands
        assert_eq!(all.len(), 25);
    }

    // === Deserialize with value alias for remaining variants ===

    #[test]
    fn test_deserialize_agent_with_value_alias() {
        let json = r#"{"command":"agent","args":{"value":"my-agent"}}"#;
        let cmd: TuiCommand = serde_json::from_str(json).unwrap();
        assert!(matches!(cmd, TuiCommand::Agent(AgentArgs { agent_name: Some(n) }) if n == "my-agent"));
    }

    #[test]
    fn test_deserialize_tools_with_value_alias() {
        let json = r#"{"command":"tools","args":{"value":"trust-all"}}"#;
        let cmd: TuiCommand = serde_json::from_str(json).unwrap();
        match cmd {
            TuiCommand::Tools(args) => assert_eq!(args.subcommand, Some("trust-all".to_string())),
            _ => panic!("expected Tools"),
        }
    }

    #[test]
    fn test_deserialize_mcp_with_value_alias() {
        let json = r#"{"command":"mcp","args":{"value":"list"}}"#;
        let cmd: TuiCommand = serde_json::from_str(json).unwrap();
        match cmd {
            TuiCommand::Mcp(args) => assert_eq!(args.subcommand, Some("list".to_string())),
            _ => panic!("expected Mcp"),
        }
    }

    #[test]
    fn test_deserialize_knowledge_with_value_alias() {
        let json = r#"{"command":"knowledge","args":{"value":"show"}}"#;
        let cmd: TuiCommand = serde_json::from_str(json).unwrap();
        match cmd {
            TuiCommand::Knowledge(args) => assert_eq!(args.subcommand, Some("show".to_string())),
            _ => panic!("expected Knowledge"),
        }
    }

    #[test]
    fn test_deserialize_prompts_with_value_alias() {
        let json = r#"{"command":"prompts","args":{"value":"my-prompt"}}"#;
        let cmd: TuiCommand = serde_json::from_str(json).unwrap();
        match cmd {
            TuiCommand::Prompts(args) => assert_eq!(args.prompt_name, Some("my-prompt".to_string())),
            _ => panic!("expected Prompts"),
        }
    }

    #[test]
    fn test_deserialize_feedback_with_value_alias() {
        let json = r#"{"command":"feedback","args":{"value":"feature"}}"#;
        let cmd: TuiCommand = serde_json::from_str(json).unwrap();
        match cmd {
            TuiCommand::Feedback(args) => assert_eq!(args.feedback_type, Some("feature".to_string())),
            _ => panic!("expected Feedback"),
        }
    }

    #[test]
    fn test_deserialize_rewind_with_value_alias() {
        let json = r#"{"command":"rewind","args":{"value":"7"}}"#;
        let cmd: TuiCommand = serde_json::from_str(json).unwrap();
        match cmd {
            TuiCommand::Rewind(args) => assert_eq!(args.turn_index, Some("7".to_string())),
            _ => panic!("expected Rewind"),
        }
    }

    #[test]
    fn test_deserialize_effort_with_value_alias() {
        let json = r#"{"command":"effort","args":{"value":"low"}}"#;
        let cmd: TuiCommand = serde_json::from_str(json).unwrap();
        match cmd {
            TuiCommand::Effort(args) => assert_eq!(args.level, Some("low".to_string())),
            _ => panic!("expected Effort"),
        }
    }

    #[test]
    fn test_deserialize_stats_with_value_alias() {
        let json = r#"{"command":"stats","args":{"value":"save x.json"}}"#;
        let cmd: TuiCommand = serde_json::from_str(json).unwrap();
        match cmd {
            TuiCommand::Stats(args) => assert_eq!(args.subcommand, Some("save x.json".to_string())),
            _ => panic!("expected Stats"),
        }
    }

    // === Voice subcommands in meta ===

    #[test]
    fn test_voice_meta_has_subcommands() {
        let cmd = TuiCommand::Voice(VoiceArgs::default());
        let meta = cmd.meta().expect("voice has subcommands so meta should exist");
        let subs = meta.get("subcommands").expect("should have subcommands");
        let arr = subs.as_array().unwrap();
        let values: Vec<&str> = arr.iter().map(|v| v.as_str().unwrap()).collect();
        assert_eq!(values, vec!["start", "stop", "status"]);
    }

    // === Chat meta local flag ===

    #[test]
    fn test_chat_meta_local() {
        let cmd = TuiCommand::Chat(ChatArgs::default());
        let meta = cmd.meta().expect("chat should have meta");
        assert_eq!(meta.get("local").unwrap(), &serde_json::Value::Bool(true));
    }

    // === Effort meta searchable ===

    #[test]
    fn test_effort_meta_not_searchable() {
        let cmd = TuiCommand::Effort(EffortArgs::default());
        let meta = cmd.meta().expect("effort should have meta");
        assert_eq!(meta.get("searchable").unwrap(), &serde_json::Value::Bool(false));
    }
}
