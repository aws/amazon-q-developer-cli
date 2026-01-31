pub mod changelog;
pub mod checkpoint;
pub mod clear;
pub mod code;
pub mod compact;
pub mod context;
pub mod editor;
pub mod experiment;
pub mod help;
pub mod hooks;
pub mod knowledge;
pub mod logdump;
pub mod mcp;
pub mod model;
pub mod paste;
pub mod persist;
pub mod profile;
pub mod prompts;
pub mod reply;

pub mod tangent;
pub mod todos;
pub mod tools;
pub mod usage;

use changelog::ChangelogArgs;
use clap::Parser;
use clear::ClearArgs;
use code::CodeSubcommand;
use compact::CompactArgs;
use editor::EditorArgs;
use experiment::ExperimentArgs;
use help::HelpArgs;
use hooks::HooksArgs;
use knowledge::KnowledgeSubcommand;
use logdump::LogdumpArgs;
use mcp::McpArgs;
use model::ModelArgs;
use paste::PasteArgs;
use persist::ChatSubcommand;
use profile::AgentSubcommand;
use prompts::PromptsArgs;
use reply::ReplyArgs;
use tangent::TangentArgs;
use todos::TodoSubcommand;
use tools::ToolsArgs;

use crate::cli::chat::cli::checkpoint::CheckpointSubcommand;
use crate::cli::chat::cli::context::ContextArgs;
use crate::cli::chat::cli::usage::UsageArgs;
use crate::cli::chat::{
    ChatError,
    ChatSession,
    ChatState,
};
use crate::cli::issue;
use crate::constants::ui_text;
use crate::os::Os;

/// Use any of these commands to manage your Kiro session. All commands start with '/'.
#[derive(Debug, PartialEq, Parser)]
#[command(
    disable_help_flag = true,
    disable_help_subcommand = true,
    color = clap::ColorChoice::Always, term_width = 0, after_long_help = &ui_text::extra_help(), override_usage = "/<COMMAND>",
    styles = clap::builder::Styles::styled()
        .header(clap::builder::styling::Style::new().bold())
        .usage(clap::builder::styling::Style::new().bold())
        .literal(clap::builder::styling::Style::new()),
)]
pub enum SlashCommand {
    /// Quit the application
    #[command(aliases = ["q", "exit"])]
    Quit,
    /// Clear the conversation history
    Clear(ClearArgs),
    /// Manage agents
    #[command(subcommand)]
    Agent(AgentSubcommand),
    /// Manage saved conversations
    #[command(subcommand)]
    Chat(ChatSubcommand),
    /// Manage context files and view context window usage
    Context(ContextArgs),
    /// (Beta) Manage knowledge base for persistent context storage. Requires "q settings
    /// chat.enableKnowledge true"
    #[command(subcommand, hide = true)]
    Knowledge(KnowledgeSubcommand),
    /// Code intelligence with LSP integration - https://kiro.dev/docs/cli/code-intelligence/
    #[command(subcommand)]
    Code(CodeSubcommand),
    /// Open $EDITOR (defaults to vi) to compose a prompt
    #[command(name = "editor")]
    PromptEditor(EditorArgs),
    /// Open $EDITOR with the most recent assistant message quoted for reply
    Reply(ReplyArgs),
    /// Summarize the conversation to free up context space
    Compact(CompactArgs),
    /// View tools and permissions
    Tools(ToolsArgs),
    /// Create a new Github issue or make a feature request
    Issue(issue::IssueArgs),
    /// Create a zip file with logs for support investigation
    Logdump(LogdumpArgs),
    /// View changelog for Kiro CLI
    #[command(name = "changelog")]
    Changelog(ChangelogArgs),
    /// View and retrieve prompts
    Prompts(PromptsArgs),
    /// View context hooks
    Hooks(HooksArgs),
    /// Show billing and credits information
    Usage(UsageArgs),
    /// See mcp server loaded
    Mcp(McpArgs),
    /// Select a model for the current conversation session
    Model(ModelArgs),
    /// Toggle experimental features
    Experiment(ExperimentArgs),

    /// (Beta) Toggle tangent mode for isolated conversations. Requires "q settings
    /// chat.enableTangentMode true"
    #[command(hide = true)]
    Tangent(TangentArgs),
    /// Switch to Plan agent for breaking down ideas into implementation plans.
    /// Use Shift+Tab to switch back to your previous agent.
    Plan {
        /// Optional prompt to send to the Plan agent
        prompt: Vec<String>,
    },
    /// [DEPRECATED] Use "/chat save" instead
    #[command(hide = true)]
    Save {
        /// Path where the chat session will be saved
        path: Option<String>,
        #[arg(short, long)]
        /// Force overwrite if file already exists
        force: bool,
    },
    /// [DEPRECATED] Use "/chat load" instead
    #[command(hide = true)]
    Load {
        /// Path to the chat session file to load
        path: Option<String>,
    },
    // #[command(flatten)]
    // Root(RootSubcommand),
    #[command(
        about = "(Beta) Manage workspace checkpoints (init, list, restore, expand, diff, clean)\nExperimental features may be changed or removed at any time",
        hide = true,
        subcommand
    )]
    Checkpoint(CheckpointSubcommand),
    /// View, manage, and resume to-do lists
    #[command(subcommand)]
    Todos(TodoSubcommand),
    /// Paste an image from clipboard
    Paste(PasteArgs),
    /// Get help about Kiro CLI features and commands
    Help(HelpArgs),
}

impl SlashCommand {
    pub async fn execute(self, os: &mut Os, session: &mut ChatSession) -> Result<ChatState, ChatError> {
        match self {
            Self::Quit => Ok(ChatState::Exit),
            Self::Clear(args) => args.execute(session).await,
            Self::Agent(subcommand) => subcommand.execute(os, session).await,
            Self::Chat(subcommand) => subcommand.execute(os, session).await,
            Self::Context(args) => args.execute(os, session).await,
            Self::Knowledge(subcommand) => subcommand.execute(os, session).await,
            Self::Code(subcommand) => subcommand.execute(os, session).await,
            Self::PromptEditor(args) => args.execute(session).await,
            Self::Reply(args) => args.execute(session).await,
            Self::Compact(args) => args.execute(os, session).await,
            Self::Tools(args) => args.execute(session, os).await,
            Self::Issue(args) => {
                if let Err(err) = args.execute(os).await {
                    return Err(ChatError::Custom(err.to_string().into()));
                }

                Ok(ChatState::PromptUser {
                    skip_printing_tools: true,
                })
            },
            Self::Logdump(args) => args.execute(session).await,
            Self::Changelog(args) => args.execute(session).await,
            Self::Prompts(args) => args.execute(os, session).await,
            Self::Hooks(args) => args.execute(session).await,
            Self::Usage(args) => args.execute(os, session).await,
            Self::Mcp(args) => args.execute(os, session).await,
            Self::Model(args) => args.execute(os, session).await,
            Self::Experiment(args) => args.execute(os, session).await,

            Self::Tangent(args) => args.execute(os, session).await,
            Self::Plan { prompt } => {
                use crossterm::{
                    execute,
                    style,
                };

                use crate::constants::PLANNER_AGENT_NAME;
                use crate::theme::StyledText;

                let swap_state = session.input_source.agent_swap_state();
                let current_agent = swap_state.get_current_agent();

                // If already in planner, handle prompt if provided
                if current_agent == PLANNER_AGENT_NAME {
                    if !prompt.is_empty() {
                        let prompt_text = prompt.join(" ");
                        // Add to transcript and return as HandleInput to process immediately
                        session.conversation.append_user_transcript(&prompt_text);
                        return Ok(ChatState::HandleInput { input: prompt_text });
                    } else {
                        execute!(
                            session.stderr,
                            StyledText::warning_fg(),
                            style::Print("Already in Plan agent. Use "),
                            StyledText::current_item_fg(),
                            style::Print("Shift+Tab"),
                            StyledText::warning_fg(),
                            style::Print(" to return to previous agent.\n"),
                            StyledText::reset()
                        )?;
                        return Ok(ChatState::PromptUser {
                            skip_printing_tools: false,
                        });
                    }
                }

                let prompt_option = if prompt.is_empty() {
                    None
                } else {
                    Some(prompt.join(" "))
                };
                swap_state.trigger_swap(PLANNER_AGENT_NAME, prompt_option);

                Ok(ChatState::PromptUser {
                    skip_printing_tools: false,
                })
            },
            Self::Save { .. } => {
                use crossterm::{
                    execute,
                    style,
                };

                use crate::theme::StyledText;

                execute!(
                    session.stderr,
                    StyledText::warning_fg(),
                    style::Print("\n⚠ The /save command is deprecated. Use /chat save instead.\n"),
                    StyledText::reset_attributes()
                )?;

                Ok(ChatState::PromptUser {
                    skip_printing_tools: true,
                })
            },
            Self::Load { .. } => {
                use crossterm::{
                    execute,
                    style,
                };

                use crate::theme::StyledText;

                execute!(
                    session.stderr,
                    StyledText::warning_fg(),
                    style::Print("\n⚠ The /load command is deprecated. Use /chat load instead.\n"),
                    StyledText::reset_attributes()
                )?;

                Ok(ChatState::PromptUser {
                    skip_printing_tools: true,
                })
            },
            // Self::Root(subcommand) => {
            //     if let Err(err) = subcommand.execute(os, database, telemetry).await {
            //         return Err(ChatError::Custom(err.to_string().into()));
            //     }
            //
            //     Ok(ChatState::PromptUser {
            //         skip_printing_tools: true,
            //     })
            // },
            Self::Checkpoint(subcommand) => subcommand.execute(os, session).await,
            Self::Todos(subcommand) => subcommand.execute(os, session).await,
            Self::Paste(args) => args.execute(os, session).await,
            Self::Help(args) => args.execute(os, session).await,
        }
    }

    pub fn command_name(&self) -> &'static str {
        match self {
            Self::Quit => "quit",
            Self::Clear(_) => "clear",
            Self::Agent(_) => "agent",
            Self::Chat(_) => "chat",
            Self::Context(_) => "context",
            Self::Knowledge(_) => "knowledge",
            Self::Code(_) => "code",
            Self::PromptEditor(_) => "editor",
            Self::Reply(_) => "reply",
            Self::Compact(_) => "compact",
            Self::Tools(_) => "tools",
            Self::Issue(_) => "issue",
            Self::Logdump(_) => "logdump",
            Self::Changelog(_) => "changelog",
            Self::Prompts(_) => "prompts",
            Self::Hooks(_) => "hooks",
            Self::Usage(_) => "usage",
            Self::Mcp(_) => "mcp",
            Self::Model(_) => "model",
            Self::Experiment(_) => "experiment",

            Self::Tangent(_) => "tangent",
            Self::Plan { .. } => "plan",
            Self::Save { .. } => "save",
            Self::Load { .. } => "load",
            Self::Checkpoint(_) => "checkpoint",
            Self::Todos(_) => "todos",
            Self::Paste(_) => "paste",
            Self::Help { .. } => "help",
        }
    }

    pub fn subcommand_name(&self) -> Option<&'static str> {
        match self {
            SlashCommand::Agent(sub) => Some(sub.name()),
            SlashCommand::Chat(sub) => Some(sub.name()),
            SlashCommand::Context(args) => args.subcommand_name(),
            SlashCommand::Knowledge(sub) => Some(sub.name()),
            SlashCommand::Code(sub) => Some(sub.name()),
            SlashCommand::Tools(arg) => arg.subcommand_name(),
            SlashCommand::Prompts(arg) => arg.subcommand_name(),
            _ => None,
        }
    }
}
