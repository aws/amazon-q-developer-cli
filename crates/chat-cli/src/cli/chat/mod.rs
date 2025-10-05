pub mod cli;
pub(crate) mod consts;
pub mod context;
pub(crate) mod conversation;
pub(crate) mod input_source;
pub(crate) mod message;
pub(crate) mod parse;
mod chat_session;

pub use chat_session::{
    ChatSession, ChatError, ChatState,
    ActualSubscriptionStatus, get_subscription_status_with_spinner, with_spinner,
    trust_all_text, CONTINUATION_LINE, PURPOSE_ARROW, ERROR_EXCLAMATION,
    TOOL_BULLET, SUCCESS_TICK,
};
use std::path::MAIN_SEPARATOR;
pub mod checkpoint;
pub(crate) mod line_tracker;
pub(crate) mod parser;
pub(crate) mod prompt;
pub(crate) mod prompt_parser;
pub mod server_messenger;
use crate::cli::chat::checkpoint::CHECKPOINT_MESSAGE_MAX_LENGTH;
use crate::constants::ui_text::{
    LIMIT_REACHED_TEXT,
    POPULAR_SHORTCUTS,
    RESUME_TEXT,
    SMALL_SCREEN_POPULAR_SHORTCUTS,
    SMALL_SCREEN_WELCOME,
    WELCOME_TEXT,
};
#[cfg(unix)]
mod skim_integration;
mod token_counter;
pub mod tool_manager;
pub mod tools;
pub mod util;
use std::borrow::Cow;
use std::collections::{
    HashMap,
    VecDeque,
};
use std::io::{
    IsTerminal,
    Read,
    Write,
};
use std::process::ExitCode;
use std::sync::Arc;
use std::time::{
    Duration,
    Instant,
};

use amzn_codewhisperer_client::types::SubscriptionStatus;
use clap::{
    Args,
    CommandFactory,
    Parser,
    ValueEnum,
};
use cli::compact::CompactStrategy;
use cli::hooks::ToolContext;
use cli::model::{
    find_model,
    get_available_models,
    select_model,
};
pub use conversation::ConversationState;
use conversation::TokenWarningLevel;
use crossterm::style::{
    Attribute,
    Color,
    Stylize,
};
use crossterm::{
    cursor,
    execute,
    queue,
    style,
    terminal,
};
use eyre::{
    Report,
    Result,
    bail,
    eyre,
};
use input_source::InputSource;
use message::{
    AssistantMessage,
    AssistantToolUse,
    ToolUseResult,
    ToolUseResultBlock,
};
use parse::{
    ParseState,
    interpret_markdown,
};
use parser::{
    RecvErrorKind,
    RequestMetadata,
    SendMessageStream,
};
use regex::Regex;
use rmcp::model::PromptMessage;
use spinners::{
    Spinner,
    Spinners,
};
use thiserror::Error;
use time::OffsetDateTime;
use token_counter::TokenCounter;
use tokio::signal::ctrl_c;
use tokio::sync::{
    Mutex,
    broadcast,
};
use tool_manager::{
    PromptQuery,
    PromptQueryResult,
    ToolManager,
    ToolManagerBuilder,
};
use tools::delegate::status_all_agents;
use tools::gh_issue::GhIssueContext;
use tools::{
    NATIVE_TOOLS,
    OutputKind,
    QueuedTool,
    Tool,
    ToolSpec,
};
use tracing::{
    debug,
    error,
    info,
    trace,
    warn,
};
use util::images::RichImageBlock;
use util::ui::draw_box;
use util::{
    animate_output,
    play_notification_bell,
};
use winnow::Partial;
use winnow::stream::Offset;

use super::agent::{
    Agent,
    DEFAULT_AGENT_NAME,
    PermissionEvalResult,
};
use crate::api_client::model::ToolResultStatus;
use crate::api_client::{
    self,
    ApiClientError,
};
use crate::auth::AuthError;
use crate::auth::builder_id::is_idc_user;
use crate::cli::TodoListState;
use crate::cli::agent::Agents;
use crate::cli::chat::checkpoint::{
    CheckpointManager,
    truncate_message,
};
use crate::cli::chat::cli::SlashCommand;
use crate::cli::chat::cli::editor::open_editor;
use crate::cli::chat::cli::prompts::{
    GetPromptError,
    PromptsSubcommand,
};
use crate::cli::chat::message::UserMessage;
use crate::cli::chat::util::sanitize_unicode_tags;
use crate::cli::experiment::experiment_manager::{
    ExperimentManager,
    ExperimentName,
};
use crate::constants::{
    error_messages,
    tips,
    ui_text,
};
use crate::database::settings::Setting;
use crate::os::Os;
use crate::telemetry::core::{
    AgentConfigInitArgs,
    ChatAddedMessageParams,
    ChatConversationType,
    MessageMetaTag,
    RecordUserTurnCompletionArgs,
    ToolUseEventBuilder,
};
use crate::telemetry::{
    ReasonCode,
    TelemetryResult,
    get_error_reason,
};
use crate::util::directories::get_shadow_repo_dir;
use crate::util::{
    MCP_SERVER_TOOL_DELIMITER,
    directories,
    ui,
};

#[derive(Copy, Clone, Debug, PartialEq, Eq, ValueEnum)]
pub enum WrapMode {
    /// Always wrap at terminal width
    Always,
    /// Never wrap (raw output)
    Never,
    /// Auto-detect based on output target (default)
    Auto,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Args)]
pub struct ChatArgs {
    /// Resumes the previous conversation from this directory.
    #[arg(short, long)]
    pub resume: bool,
    /// Context profile to use
    #[arg(long = "agent", alias = "profile")]
    pub agent: Option<String>,
    /// Current model to use
    #[arg(long = "model")]
    pub model: Option<String>,
    /// Allows the model to use any tool to run commands without asking for confirmation.
    #[arg(short = 'a', long)]
    pub trust_all_tools: bool,
    /// Trust only this set of tools. Example: trust some tools:
    /// '--trust-tools=fs_read,fs_write', trust no tools: '--trust-tools='
    #[arg(long, value_delimiter = ',', value_name = "TOOL_NAMES")]
    pub trust_tools: Option<Vec<String>>,
    /// Whether the command should run without expecting user input
    #[arg(long, alias = "non-interactive")]
    pub no_interactive: bool,
    /// The first question to ask
    pub input: Option<String>,
    /// Control line wrapping behavior (default: auto-detect)
    #[arg(short = 'w', long, value_enum)]
    pub wrap: Option<WrapMode>,
}

impl ChatArgs {
    pub async fn execute(mut self, os: &mut Os) -> Result<ExitCode> {
        let mut input = self.input;

        let mut stderr = std::io::stderr();
        execute!(
            stderr,
            style::SetForegroundColor(Color::Red),
            style::Print("HELLO WORLD: "),
            style::SetForegroundColor(Color::Reset),
            style::Print("nothing is here yet\n")
        )?;

        // TODO: This is where we plug in new entry point

        Ok(ExitCode::SUCCESS)
    }
}
