pub mod changelog;
pub mod clear;
pub mod compact;
pub mod context;
pub mod editor;
pub mod experiment;
pub mod hooks;
pub mod knowledge;
pub mod mcp;
pub mod model;
pub mod persist;
pub mod profile;
pub mod prompts;
pub mod subscribe;
pub mod tangent;
pub mod todos;
pub mod tools;
pub mod usage;

use changelog::ChangelogArgs;
use clap::Parser;
use clear::ClearArgs;
use compact::CompactArgs;
use context::ContextSubcommand;
use editor::EditorArgs;
use experiment::ExperimentArgs;
use hooks::HooksArgs;
use knowledge::KnowledgeSubcommand;
use mcp::McpArgs;
use model::ModelArgs;
use persist::PersistSubcommand;
use profile::AgentSubcommand;
use prompts::PromptsArgs;
use tangent::TangentArgs;
use todos::TodoSubcommand;
use tools::ToolsArgs;

use crate::cli::chat::cli::subscribe::SubscribeArgs;
use crate::cli::chat::cli::usage::UsageArgs;
use crate::cli::chat::consts::AGENT_MIGRATION_DOC_URL;
use crate::cli::chat::{
    ChatError,
    ChatSession,
    ChatState,
    EXTRA_HELP,
};
use crate::cli::issue;
use crate::os::Os;

/// q (Amazon Q Chat)
#[derive(Debug, PartialEq, Parser)]
#[command(color = clap::ColorChoice::Always, term_width = 0, after_long_help = EXTRA_HELP)]
pub enum SlashCommand {
    /// Quit the application
    #[command(aliases = ["q", "exit"])]
    Quit,
    /// Clear the conversation history
    Clear(ClearArgs),
    /// Manage agents
    #[command(subcommand)]
    Agent(AgentSubcommand),
    #[command(hide = true)]
    Profile,
    /// Manage context files for the chat session
    #[command(subcommand)]
    Context(ContextSubcommand),
    /// (Beta) Manage knowledge base for persistent context storage. Requires "q settings
    /// chat.enableKnowledge true"
    #[command(subcommand, hide = true)]
    Knowledge(KnowledgeSubcommand),
    /// Open $EDITOR (defaults to vi) to compose a prompt
    #[command(name = "editor")]
    PromptEditor(EditorArgs),
    /// Summarize the conversation to free up context space
    Compact(CompactArgs),
    /// View tools and permissions
    Tools(ToolsArgs),
    /// Create a new Github issue or make a feature request
    Issue(issue::IssueArgs),
    /// View changelog for Amazon Q CLI
    #[command(name = "changelog")]
    Changelog(ChangelogArgs),
    /// View and retrieve prompts
    Prompts(PromptsArgs),
    /// View context hooks
    Hooks(HooksArgs),
    /// Show current session's context window usage
    Usage(UsageArgs),
    /// See mcp server loaded
    Mcp(McpArgs),
    /// Select a model for the current conversation session
    Model(ModelArgs),
    /// Toggle experimental features
    Experiment(ExperimentArgs),
    /// Upgrade to a Q Developer Pro subscription for increased query limits
    Subscribe(SubscribeArgs),
    /// (Beta) Toggle tangent mode for isolated conversations. Requires "q settings
    /// chat.enableTangentMode true"
    #[command(hide = true)]
    Tangent(TangentArgs),
    #[command(flatten)]
    Persist(PersistSubcommand),
    // #[command(flatten)]
    // Root(RootSubcommand),
    /// View, manage, and resume to-do lists
    #[command(subcommand)]
    Todos(TodoSubcommand),
}

impl SlashCommand {
    pub async fn execute(self, os: &mut Os, session: &mut ChatSession) -> Result<ChatState, ChatError> {
        match self {
            Self::Quit => Ok(ChatState::Exit),
            Self::Clear(args) => args.execute(session).await,
            Self::Agent(subcommand) => subcommand.execute(os, session).await,
            Self::Profile => {
                use crossterm::{
                    execute,
                    style,
                };
                execute!(
                    session.stderr,
                    style::SetForegroundColor(style::Color::Yellow),
                    style::Print("This command has been deprecated. Use"),
                    style::SetForegroundColor(style::Color::Cyan),
                    style::Print(" /agent "),
                    style::SetForegroundColor(style::Color::Yellow),
                    style::Print("instead.\nSee "),
                    style::Print(AGENT_MIGRATION_DOC_URL),
                    style::Print(" for more detail"),
                    style::Print("\n"),
                    style::ResetColor,
                )?;

                Ok(ChatState::PromptUser {
                    skip_printing_tools: true,
                })
            },
            Self::Context(args) => args.execute(os, session).await,
            Self::Knowledge(subcommand) => subcommand.execute(os, session).await,
            Self::PromptEditor(args) => args.execute(session).await,
            Self::Compact(args) => args.execute(os, session).await,
            Self::Tools(args) => args.execute(session).await,
            Self::Issue(args) => {
                if let Err(err) = args.execute(os).await {
                    return Err(ChatError::Custom(err.to_string().into()));
                }

                Ok(ChatState::PromptUser {
                    skip_printing_tools: true,
                })
            },
            Self::Changelog(args) => args.execute(session).await,
            Self::Prompts(args) => args.execute(session).await,
            Self::Hooks(args) => args.execute(session).await,
            Self::Usage(args) => args.execute(os, session).await,
            Self::Mcp(args) => args.execute(session).await,
            Self::Model(args) => args.execute(os, session).await,
            Self::Experiment(args) => args.execute(os, session).await,
            Self::Subscribe(args) => args.execute(os, session).await,
            Self::Tangent(args) => args.execute(os, session).await,
            Self::Persist(subcommand) => subcommand.execute(os, session).await,
            // Self::Root(subcommand) => {
            //     if let Err(err) = subcommand.execute(os, database, telemetry).await {
            //         return Err(ChatError::Custom(err.to_string().into()));
            //     }
            //
            //     Ok(ChatState::PromptUser {
            //         skip_printing_tools: true,
            //     })
            // },
            Self::Todos(subcommand) => subcommand.execute(os, session).await,
        }
    }

    pub fn command_name(&self) -> &'static str {
        match self {
            Self::Quit => "quit",
            Self::Clear(_) => "clear",
            Self::Agent(_) => "agent",
            Self::Profile => "profile",
            Self::Context(_) => "context",
            Self::Knowledge(_) => "knowledge",
            Self::PromptEditor(_) => "editor",
            Self::Compact(_) => "compact",
            Self::Tools(_) => "tools",
            Self::Issue(_) => "issue",
            Self::Changelog(_) => "changelog",
            Self::Prompts(_) => "prompts",
            Self::Hooks(_) => "hooks",
            Self::Usage(_) => "usage",
            Self::Mcp(_) => "mcp",
            Self::Model(_) => "model",
            Self::Experiment(_) => "experiment",
            Self::Subscribe(_) => "subscribe",
            Self::Tangent(_) => "tangent",
            Self::Persist(sub) => match sub {
                PersistSubcommand::Save { .. } => "save",
                PersistSubcommand::Load { .. } => "load",
            },
            Self::Todos(_) => "todos",
        }
    }

    pub fn subcommand_name(&self) -> Option<&'static str> {
        match self {
            SlashCommand::Agent(sub) => Some(sub.name()),
            SlashCommand::Context(sub) => Some(sub.name()),
            SlashCommand::Knowledge(sub) => Some(sub.name()),
            SlashCommand::Tools(arg) => arg.subcommand_name(),
            SlashCommand::Prompts(arg) => arg.subcommand_name(),
            _ => None,
        }
    }
}
