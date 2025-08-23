pub mod cli;
mod consts;
pub mod context;
mod conversation;
mod error_formatter;
mod input_source;
mod message;
mod parse;
use std::path::MAIN_SEPARATOR;
mod line_tracker;
mod parser;
mod prompt;
mod prompt_parser;
mod server_messenger;
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
};
use cli::compact::CompactStrategy;
use cli::model::{
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
use crate::cli::agent::Agents;
use crate::cli::chat::cli::SlashCommand;
use crate::cli::chat::cli::model::find_model;
use crate::cli::chat::cli::prompts::{
    GetPromptError,
    PromptsSubcommand,
};
use crate::cli::chat::util::sanitize_unicode_tags;
use crate::database::settings::Setting;
use crate::mcp_client::Prompt;
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
use crate::util::MCP_SERVER_TOOL_DELIMITER;

const LIMIT_REACHED_TEXT: &str = color_print::cstr! { "You've used all your free requests for this month. You have two options:
1. Upgrade to a paid subscription for increased limits. See our Pricing page for what's included> <blue!>https://aws.amazon.com/q/developer/pricing/</blue!>
2. Wait until next month when your limit automatically resets." };

pub const EXTRA_HELP: &str = color_print::cstr! {"
<cyan,em>MCP:</cyan,em>
<black!>You can now configure the Amazon Q CLI to use MCP servers. \nLearn how: https://docs.aws.amazon.com/amazonq/latest/qdeveloper-ug/qdev-mcp.html</black!>

<cyan,em>Tips:</cyan,em>
<em>!{command}</em>          <black!>Quickly execute a command in your current session</black!>
<em>Ctrl(^) + j</em>         <black!>Insert new-line to provide multi-line prompt</black!>
                    <black!>Alternatively, [Alt(⌥) + Enter(⏎)]</black!>
<em>Ctrl(^) + s</em>         <black!>Fuzzy search commands and context files</black!>
                    <black!>Use Tab to select multiple items</black!>
                    <black!>Change the keybind using: q settings chat.skimCommandKey x</black!>
<em>Ctrl(^) + t</em>         <black!>Toggle tangent mode for isolated conversations</black!>
                    <black!>Change the keybind using: q settings chat.tangentModeKey x</black!>
<em>chat.editMode</em>       <black!>The prompt editing mode (vim or emacs)</black!>
                    <black!>Change using: q settings chat.skimCommandKey x</black!>
"};

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
}

impl ChatArgs {
    pub async fn execute(mut self, os: &mut Os) -> Result<ExitCode> {
        let mut input = self.input;

        if self.no_interactive && input.is_none() {
            if !std::io::stdin().is_terminal() {
                let mut buffer = String::new();
                match std::io::stdin().read_to_string(&mut buffer) {
                    Ok(_) => {
                        if !buffer.trim().is_empty() {
                            input = Some(buffer.trim().to_string());
                        }
                    },
                    Err(e) => {
                        eprintln!("Error reading from stdin: {}", e);
                    },
                }
            }

            if input.is_none() {
                bail!("Input must be supplied when running in non-interactive mode");
            }
        }

        let stdout = std::io::stdout();
        let mut stderr = std::io::stderr();

        let args: Vec<String> = std::env::args().collect();
        if args
            .iter()
            .any(|arg| arg == "--profile" || arg.starts_with("--profile="))
        {
            execute!(
                stderr,
                style::SetForegroundColor(Color::Yellow),
                style::Print("WARNING: "),
                style::SetForegroundColor(Color::Reset),
                style::Print("--profile is deprecated, use "),
                style::SetForegroundColor(Color::Green),
                style::Print("--agent"),
                style::SetForegroundColor(Color::Reset),
                style::Print(" instead\n")
            )?;
        }

        let conversation_id = uuid::Uuid::new_v4().to_string();
        info!(?conversation_id, "Generated new conversation id");

        // Check MCP status once at the beginning of the session
        let mcp_enabled = match os.client.is_mcp_enabled().await {
            Ok(enabled) => enabled,
            Err(err) => {
                tracing::warn!(?err, "Failed to check MCP configuration, defaulting to enabled");
                true
            },
        };

        let agents = {
            let skip_migration = self.no_interactive;
            let (mut agents, md) =
                Agents::load(os, self.agent.as_deref(), skip_migration, &mut stderr, mcp_enabled).await;
            agents.trust_all_tools = self.trust_all_tools;

            os.telemetry
                .send_agent_config_init(&os.database, conversation_id.clone(), AgentConfigInitArgs {
                    agents_loaded_count: md.load_count as i64,
                    agents_loaded_failed_count: md.load_failed_count as i64,
                    legacy_profile_migration_executed: md.migration_performed,
                    legacy_profile_migrated_count: md.migrated_count as i64,
                    launched_agent: md.launched_agent,
                })
                .await
                .map_err(|err| error!(?err, "failed to send agent config init telemetry"))
                .ok();

            // Only show MCP safety message if MCP is enabled and has servers
            if mcp_enabled
                && agents
                    .get_active()
                    .is_some_and(|a| !a.mcp_servers.mcp_servers.is_empty())
            {
                if !self.no_interactive && !os.database.settings.get_bool(Setting::McpLoadedBefore).unwrap_or(false) {
                    execute!(
                        stderr,
                        style::Print(
                            "To learn more about MCP safety, see https://docs.aws.amazon.com/amazonq/latest/qdeveloper-ug/command-line-mcp-security.html\n\n"
                        )
                    )?;
                }
                os.database.settings.set(Setting::McpLoadedBefore, true).await?;
            }

            if let Some(trust_tools) = self.trust_tools.take() {
                for tool in &trust_tools {
                    if !tool.starts_with("@") && !NATIVE_TOOLS.contains(&tool.as_str()) {
                        let _ = queue!(
                            stderr,
                            style::SetForegroundColor(Color::Yellow),
                            style::Print("WARNING: "),
                            style::SetForegroundColor(Color::Reset),
                            style::Print("--trust-tools arg for custom tool "),
                            style::SetForegroundColor(Color::Cyan),
                            style::Print(tool),
                            style::SetForegroundColor(Color::Reset),
                            style::Print(" needs to be prepended with "),
                            style::SetForegroundColor(Color::Green),
                            style::Print("@{MCPSERVERNAME}/"),
                            style::SetForegroundColor(Color::Reset),
                            style::Print("\n"),
                        );
                    }
                }

                let _ = stderr.flush();

                if let Some(a) = agents.get_active_mut() {
                    a.allowed_tools.extend(trust_tools);
                }
            }

            agents
        };

        // If modelId is specified, verify it exists before starting the chat
        // Otherwise, CLI will use a default model when starting chat
        let (models, default_model_opt) = get_available_models(os).await?;
        let model_id: Option<String> = if let Some(requested) = self.model.as_ref() {
            if let Some(m) = find_model(&models, requested) {
                Some(m.model_id.clone())
            } else {
                let available = models
                    .iter()
                    .map(|m| m.model_name.as_deref().unwrap_or(&m.model_id))
                    .collect::<Vec<_>>()
                    .join(", ");
                bail!("Model '{}' does not exist. Available models: {}", requested, available);
            }
        } else if let Some(saved) = os.database.settings.get_string(Setting::ChatDefaultModel) {
            find_model(&models, &saved)
                .map(|m| m.model_id.clone())
                .or(Some(default_model_opt.model_id.clone()))
        } else {
            Some(default_model_opt.model_id.clone())
        };

        let (prompt_request_sender, prompt_request_receiver) = tokio::sync::broadcast::channel::<PromptQuery>(5);
        let (prompt_response_sender, prompt_response_receiver) =
            tokio::sync::broadcast::channel::<PromptQueryResult>(5);
        let mut tool_manager = ToolManagerBuilder::default()
            .prompt_query_result_sender(prompt_response_sender)
            .prompt_query_receiver(prompt_request_receiver)
            .prompt_query_sender(prompt_request_sender.clone())
            .prompt_query_result_receiver(prompt_response_receiver.resubscribe())
            .conversation_id(&conversation_id)
            .agent(agents.get_active().cloned().unwrap_or_default())
            .build(os, Box::new(std::io::stderr()), !self.no_interactive)
            .await?;
        let tool_config = tool_manager.load_tools(os, &mut stderr).await?;

        ChatSession::new(
            os,
            stdout,
            stderr,
            &conversation_id,
            agents,
            input,
            InputSource::new(os, prompt_request_sender, prompt_response_receiver)?,
            self.resume,
            || terminal::window_size().map(|s| s.columns.into()).ok(),
            tool_manager,
            model_id,
            tool_config,
            !self.no_interactive,
            mcp_enabled,
        )
        .await?
        .spawn(os)
        .await
        .map(|_| ExitCode::SUCCESS)
    }
}

const WELCOME_TEXT: &str = color_print::cstr! {"<cyan!>
    ⢠⣶⣶⣦⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⢀⣤⣶⣿⣿⣿⣶⣦⡀⠀
 ⠀⠀⠀⣾⡿⢻⣿⡆⠀⠀⠀⢀⣄⡄⢀⣠⣤⣤⡀⢀⣠⣤⣤⡀⠀⠀⢀⣠⣤⣤⣤⣄⠀⠀⢀⣤⣤⣤⣤⣤⣤⡀⠀⠀⣀⣤⣤⣤⣀⠀⠀⠀⢠⣤⡀⣀⣤⣤⣄⡀⠀⠀⠀⠀⠀⠀⢠⣿⣿⠋⠀⠀⠀⠙⣿⣿⡆
 ⠀⠀⣼⣿⠇⠀⣿⣿⡄⠀⠀⢸⣿⣿⠛⠉⠻⣿⣿⠛⠉⠛⣿⣿⠀⠀⠘⠛⠉⠉⠻⣿⣧⠀⠈⠛⠛⠛⣻⣿⡿⠀⢀⣾⣿⠛⠉⠻⣿⣷⡀⠀⢸⣿⡟⠛⠉⢻⣿⣷⠀⠀⠀⠀⠀⠀⣼⣿⡏⠀⠀⠀⠀⠀⢸⣿⣿
 ⠀⢰⣿⣿⣤⣤⣼⣿⣷⠀⠀⢸⣿⣿⠀⠀⠀⣿⣿⠀⠀⠀⣿⣿⠀⠀⢀⣴⣶⣶⣶⣿⣿⠀⠀⠀⣠⣾⡿⠋⠀⠀⢸⣿⣿⠀⠀⠀⣿⣿⡇⠀⢸⣿⡇⠀⠀⢸⣿⣿⠀⠀⠀⠀⠀⠀⢹⣿⣇⠀⠀⠀⠀⠀⢸⣿⡿
 ⢀⣿⣿⠋⠉⠉⠉⢻⣿⣇⠀⢸⣿⣿⠀⠀⠀⣿⣿⠀⠀⠀⣿⣿⠀⠀⣿⣿⡀⠀⣠⣿⣿⠀⢀⣴⣿⣋⣀⣀⣀⡀⠘⣿⣿⣄⣀⣠⣿⣿⠃⠀⢸⣿⡇⠀⠀⢸⣿⣿⠀⠀⠀⠀⠀⠀⠈⢿⣿⣦⣀⣀⣀⣴⣿⡿⠃
 ⠚⠛⠋⠀⠀⠀⠀⠘⠛⠛⠀⠘⠛⠛⠀⠀⠀⠛⠛⠀⠀⠀⠛⠛⠀⠀⠙⠻⠿⠟⠋⠛⠛⠀⠘⠛⠛⠛⠛⠛⠛⠃⠀⠈⠛⠿⠿⠿⠛⠁⠀⠀⠘⠛⠃⠀⠀⠘⠛⠛⠀⠀⠀⠀⠀⠀⠀⠀⠙⠛⠿⢿⣿⣿⣋⠀⠀
 ⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠈⠛⠿⢿⡧</cyan!>"};

const SMALL_SCREEN_WELCOME_TEXT: &str = color_print::cstr! {"<em>Welcome to <cyan!>Amazon Q</cyan!>!</em>"};
const RESUME_TEXT: &str = color_print::cstr! {"<em>Picking up where we left off...</em>"};

// Only show the model-related tip for now to make users aware of this feature.
const ROTATING_TIPS: [&str; 17] = [
    color_print::cstr! {"You can resume the last conversation from your current directory by launching with
    <green!>q chat --resume</green!>"},
    color_print::cstr! {"Get notified whenever Q CLI finishes responding.
    Just run <green!>q settings chat.enableNotifications true</green!>"},
    color_print::cstr! {"You can use
    <green!>/editor</green!> to edit your prompt with a vim-like experience"},
    color_print::cstr! {"<green!>/usage</green!> shows you a visual breakdown of your current context window usage"},
    color_print::cstr! {"Get notified whenever Q CLI finishes responding. Just run <green!>q settings
    chat.enableNotifications true</green!>"},
    color_print::cstr! {"You can execute bash commands by typing
    <green!>!</green!> followed by the command"},
    color_print::cstr! {"Q can use tools without asking for
    confirmation every time. Give <green!>/tools trust</green!> a try"},
    color_print::cstr! {"You can
    programmatically inject context to your prompts by using hooks. Check out <green!>/context hooks
    help</green!>"},
    color_print::cstr! {"You can use <green!>/compact</green!> to replace the conversation
    history with its summary to free up the context space"},
    color_print::cstr! {"If you want to file an issue
    to the Q CLI team, just tell me, or run <green!>q issue</green!>"},
    color_print::cstr! {"You can enable
    custom tools with <green!>MCP servers</green!>. Learn more with /help"},
    color_print::cstr! {"You can
    specify wait time (in ms) for mcp server loading with <green!>q settings mcp.initTimeout {timeout in
    int}</green!>. Servers that takes longer than the specified time will continue to load in the background. Use
    /tools to see pending servers."},
    color_print::cstr! {"You can see the server load status as well as any
    warnings or errors associated with <green!>/mcp</green!>"},
    color_print::cstr! {"Use <green!>/model</green!> to select the model to use for this conversation"},
    color_print::cstr! {"Set a default model by running <green!>q settings chat.defaultModel MODEL</green!>. Run <green!>/model</green!> to learn more."},
    color_print::cstr! {"Run <green!>/prompts</green!> to learn how to build & run repeatable workflows"},
    color_print::cstr! {"Use <green!>/tangent</green!> or <green!>ctrl + t</green!> (customizable) to start isolated conversations ( ↯ ) that don't affect your main chat history"},
];

const GREETING_BREAK_POINT: usize = 80;

const POPULAR_SHORTCUTS: &str = color_print::cstr! {"<black!><green!>/help</green!> all commands  <em>•</em>  <green!>ctrl + j</green!> new lines  <em>•</em>  <green!>ctrl + s</green!> fuzzy search</black!>"};
const SMALL_SCREEN_POPULAR_SHORTCUTS: &str = color_print::cstr! {"<black!><green!>/help</green!> all commands
<green!>ctrl + j</green!> new lines
<green!>ctrl + s</green!> fuzzy search
</black!>"};

const RESPONSE_TIMEOUT_CONTENT: &str = "Response timed out - message took too long to generate";
const TRUST_ALL_TEXT: &str = color_print::cstr! {"<green!>All tools are now trusted (<red!>!</red!>). Amazon Q will execute tools <bold>without</bold> asking for confirmation.\
\nAgents can sometimes do unexpected things so understand the risks.</green!>
\nLearn more at https://docs.aws.amazon.com/amazonq/latest/qdeveloper-ug/command-line-chat-security.html#command-line-chat-trustall-safety"};

const TOOL_BULLET: &str = " ● ";
const CONTINUATION_LINE: &str = " ⋮ ";
const PURPOSE_ARROW: &str = " ↳ ";
const SUCCESS_TICK: &str = " ✓ ";
const ERROR_EXCLAMATION: &str = " ❗ ";

/// Enum used to denote the origin of a tool use event
enum ToolUseStatus {
    /// Variant denotes that the tool use event associated with chat context is a direct result of
    /// a user request
    Idle,
    /// Variant denotes that the tool use event associated with the chat context is a result of a
    /// retry for one or more previously attempted tool use. The tuple is the utterance id
    /// associated with the original user request that necessitated the tool use
    RetryInProgress(String),
}

#[derive(Debug, Error)]
pub enum ChatError {
    #[error("{0}")]
    Client(Box<crate::api_client::ApiClientError>),
    #[error("{0}")]
    Auth(#[from] AuthError),
    #[error("{0}")]
    SendMessage(Box<parser::SendMessageError>),
    #[error("{0}")]
    ResponseStream(Box<parser::RecvError>),
    #[error("{0}")]
    Std(#[from] std::io::Error),
    #[error("{0}")]
    Readline(#[from] rustyline::error::ReadlineError),
    #[error("{0}")]
    Custom(Cow<'static, str>),
    #[error("interrupted")]
    Interrupted { tool_uses: Option<Vec<QueuedTool>> },
    #[error(transparent)]
    GetPromptError(#[from] GetPromptError),
    #[error(
        "Tool approval required but --no-interactive was specified. Use --trust-all-tools to automatically approve tools."
    )]
    NonInteractiveToolApproval,
    #[error("The conversation history is too large to compact")]
    CompactHistoryFailure,
    #[error("Failed to swap to agent: {0}")]
    AgentSwapError(eyre::Report),
}

impl ChatError {
    fn status_code(&self) -> Option<u16> {
        match self {
            ChatError::Client(e) => e.status_code(),
            ChatError::Auth(_) => None,
            ChatError::SendMessage(e) => e.status_code(),
            ChatError::ResponseStream(_) => None,
            ChatError::Std(_) => None,
            ChatError::Readline(_) => None,
            ChatError::Custom(_) => None,
            ChatError::Interrupted { .. } => None,
            ChatError::GetPromptError(_) => None,
            ChatError::NonInteractiveToolApproval => None,
            ChatError::CompactHistoryFailure => None,
            ChatError::AgentSwapError(_) => None,
        }
    }
}

impl ReasonCode for ChatError {
    fn reason_code(&self) -> String {
        match self {
            ChatError::Client(e) => e.reason_code(),
            ChatError::SendMessage(e) => e.reason_code(),
            ChatError::ResponseStream(e) => e.reason_code(),
            ChatError::Std(_) => "StdIoError".to_string(),
            ChatError::Readline(_) => "ReadlineError".to_string(),
            ChatError::Custom(_) => "GenericError".to_string(),
            ChatError::Interrupted { .. } => "Interrupted".to_string(),
            ChatError::GetPromptError(_) => "GetPromptError".to_string(),
            ChatError::Auth(_) => "AuthError".to_string(),
            ChatError::NonInteractiveToolApproval => "NonInteractiveToolApproval".to_string(),
            ChatError::CompactHistoryFailure => "CompactHistoryFailure".to_string(),
            ChatError::AgentSwapError(_) => "AgentSwapError".to_string(),
        }
    }
}

impl From<ApiClientError> for ChatError {
    fn from(value: ApiClientError) -> Self {
        Self::Client(Box::new(value))
    }
}

impl From<parser::SendMessageError> for ChatError {
    fn from(value: parser::SendMessageError) -> Self {
        Self::SendMessage(Box::new(value))
    }
}

impl From<parser::RecvError> for ChatError {
    fn from(value: parser::RecvError) -> Self {
        Self::ResponseStream(Box::new(value))
    }
}



impl Drop for ChatSession {
    fn drop(&mut self) {
        if let Some(spinner) = &mut self.spinner {
            spinner.stop();
        }

        execute!(
            self.stderr,
            cursor::MoveToColumn(0),
            style::SetAttribute(Attribute::Reset),
            style::ResetColor,
            cursor::Show
        )
        .ok();
    }
}

/// The chat execution state.
///
/// Intended to provide more robust handling around state transitions while dealing with, e.g.,
/// tool validation, execution, response stream handling, etc.
#[allow(clippy::large_enum_variant)]
#[derive(Debug)]
pub enum ChatState {
    /// Prompt the user with `tool_uses`, if available.
    PromptUser {
        /// Used to avoid displaying the tool info at inappropriate times, e.g. after clear or help
        /// commands.
        skip_printing_tools: bool,
    },
    /// Handle the user input, depending on if any tools require execution.
    HandleInput { input: String },
    /// Validate the list of tool uses provided by the model.
    ValidateTools { tool_uses: Vec<AssistantToolUse> },
    /// Execute the list of tools.
    ExecuteTools,
    /// Consume the response stream and display to the user.
    HandleResponseStream(crate::api_client::model::ConversationState),
    /// Compact the chat history.
    CompactHistory {
        /// Custom prompt to include as part of history compaction.
        prompt: Option<String>,
        /// Whether or not the summary should be shown on compact success.
        show_summary: bool,
        /// Parameters for how to perform the compaction request.
        strategy: CompactStrategy,
    },
    /// Retry the current request if we encounter a model overloaded error.
    RetryModelOverload,
    /// Exit the chat.
    Exit,
}

impl Default for ChatState {
    fn default() -> Self {
        Self::PromptUser {
            skip_printing_tools: false,
        }
    }
}

/// Replaces amzn_codewhisperer_client::types::SubscriptionStatus with a more descriptive type.
/// See response expectations in [`get_subscription_status`] for reasoning.
#[derive(Debug, Clone, PartialEq, Eq)]
enum ActualSubscriptionStatus {
    Active,   // User has paid for this month
    Expiring, // User has paid for this month but cancelled
    None,     // User has not paid for this month
}

// NOTE: The subscription API behaves in a non-intuitive way. We expect the following responses:
//
// 1. SubscriptionStatus::Active:
//    - The user *has* a subscription, but it is set to *not auto-renew* (i.e., cancelled).
//    - We return ActualSubscriptionStatus::Expiring to indicate they are eligible to re-subscribe
//
// 2. SubscriptionStatus::Inactive:
//    - The user has no subscription at all (no Pro access).
//    - We return ActualSubscriptionStatus::None to indicate they are eligible to subscribe.
//
// 3. ConflictException (as an error):
//    - The user already has an active subscription *with auto-renewal enabled*.
//    - We return ActualSubscriptionStatus::Active since they don’t need to subscribe again.
//
// Also, it is currently not possible to subscribe or re-subscribe via console, only IDE/CLI.
async fn get_subscription_status(os: &mut Os) -> Result<ActualSubscriptionStatus> {
    if is_idc_user(&os.database).await? {
        return Ok(ActualSubscriptionStatus::Active);
    }

    match os.client.create_subscription_token().await {
        Ok(response) => match response.status() {
            SubscriptionStatus::Active => Ok(ActualSubscriptionStatus::Expiring),
            SubscriptionStatus::Inactive => Ok(ActualSubscriptionStatus::None),
            _ => Ok(ActualSubscriptionStatus::None),
        },
        Err(ApiClientError::CreateSubscriptionToken(e)) => {
            let sdk_error_code = e.as_service_error().and_then(|err| err.meta().code());

            if sdk_error_code.is_some_and(|c| c.contains("ConflictException")) {
                Ok(ActualSubscriptionStatus::Active)
            } else {
                Err(e.into())
            }
        },
        Err(e) => Err(e.into()),
    }
}

async fn get_subscription_status_with_spinner(
    os: &mut Os,
    output: &mut impl Write,
) -> Result<ActualSubscriptionStatus> {
    return with_spinner(output, "Checking subscription status...", || async {
        get_subscription_status(os).await
    })
    .await;
}

async fn with_spinner<T, E, F, Fut>(output: &mut impl std::io::Write, spinner_text: &str, f: F) -> Result<T, E>
where
    F: FnOnce() -> Fut,
    Fut: std::future::Future<Output = Result<T, E>>,
{
    queue!(output, cursor::Hide,).ok();
    let spinner = Some(Spinner::new(Spinners::Dots, spinner_text.to_owned()));

    let result = f().await;

    if let Some(mut s) = spinner {
        s.stop();
        let _ = queue!(
            output,
            terminal::Clear(terminal::ClearType::CurrentLine),
            cursor::MoveToColumn(0),
        );
    }

    result
}

/// Checks if an input may be referencing a file and should not be handled as a typical slash
/// command. If true, then return [Option::Some<ChatState>], otherwise [Option::None].
fn does_input_reference_file(input: &str) -> Option<ChatState> {
    let after_slash = input.strip_prefix("/")?;

    if let Some(first) = shlex::split(after_slash).unwrap_or_default().first() {
        let looks_like_path =
            first.contains(MAIN_SEPARATOR) || first.contains('/') || first.contains('\\') || first.contains('.');

        if looks_like_path {
            return Some(ChatState::HandleInput {
                input: after_slash.to_string(),
            });
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use crate::cli::agent::Agent;

    async fn get_test_agents(os: &Os) -> Agents {
        const AGENT_PATH: &str = "/persona/TestAgent.json";
        let mut agents = Agents::default();
        let agent = Agent {
            path: Some(PathBuf::from(AGENT_PATH)),
            ..Default::default()
        };
        if let Ok(false) = os.fs.try_exists(AGENT_PATH).await {
            let content = agent.to_str_pretty().expect("Failed to serialize test agent to file");
            let agent_path = PathBuf::from(AGENT_PATH);
            os.fs
                .create_dir_all(
                    agent_path
                        .parent()
                        .expect("Failed to obtain parent path for agent config"),
                )
                .await
                .expect("Failed to create test agent dir");
            os.fs
                .write(agent_path, &content)
                .await
                .expect("Failed to write test agent to file");
        }
        agents.agents.insert("TestAgent".to_string(), agent);
        agents.switch("TestAgent").expect("Failed to switch agent");
        agents
    }

    #[tokio::test]
    async fn test_flow() {
        let mut os = Os::new().await.unwrap();
        os.client.set_mock_output(serde_json::json!([
            [
                "Sure, I'll create a file for you",
                {
                    "tool_use_id": "1",
                    "name": "fs_write",
                    "args": {
                        "command": "create",
                        "file_text": "Hello, world!",
                        "path": "/file.txt",
                    }
                }
            ],
            [
                "Hope that looks good to you!",
            ],
        ]));

        let agents = get_test_agents(&os).await;
        let tool_manager = ToolManager::default();
        let tool_config = serde_json::from_str::<HashMap<String, ToolSpec>>(include_str!("tools/tool_index.json"))
            .expect("Tools failed to load");
        ChatSession::new(
            &mut os,
            std::io::stdout(),
            std::io::stderr(),
            "fake_conv_id",
            agents,
            None,
            InputSource::new_mock(vec![
                "create a new file".to_string(),
                "y".to_string(),
                "exit".to_string(),
            ]),
            false,
            || Some(80),
            tool_manager,
            None,
            tool_config,
            true,
            false,
        )
        .await
        .unwrap()
        .spawn(&mut os)
        .await
        .unwrap();

        assert_eq!(os.fs.read_to_string("/file.txt").await.unwrap(), "Hello, world!\n");
    }

    #[tokio::test]
    async fn test_flow_tool_permissions() {
        let mut os = Os::new().await.unwrap();
        os.client.set_mock_output(serde_json::json!([
            [
                "Ok",
                {
                    "tool_use_id": "1",
                    "name": "fs_write",
                    "args": {
                        "command": "create",
                        "file_text": "Hello, world!",
                        "path": "/file1.txt",
                    }
                }
            ],
            [
                "Done",
            ],
            [
                "Ok",
                {
                    "tool_use_id": "1",
                    "name": "fs_write",
                    "args": {
                        "command": "create",
                        "file_text": "Hello, world!",
                        "path": "/file2.txt",
                    }
                }
            ],
            [
                "Done",
            ],
            [
                "Ok",
                {
                    "tool_use_id": "1",
                    "name": "fs_write",
                    "args": {
                        "command": "create",
                        "file_text": "Hello, world!",
                        "path": "/file3.txt",
                    }
                }
            ],
            [
                "Done",
            ],
            [
                "Ok",
                {
                    "tool_use_id": "1",
                    "name": "fs_write",
                    "args": {
                        "command": "create",
                        "file_text": "Hello, world!",
                        "path": "/file4.txt",
                    }
                }
            ],
            [
                "Ok, I won't make it.",
            ],
            [
                "Ok",
                {
                    "tool_use_id": "1",
                    "name": "fs_write",
                    "args": {
                        "command": "create",
                        "file_text": "Hello, world!",
                        "path": "/file5.txt",
                    }
                }
            ],
            [
                "Done",
            ],
            [
                "Ok",
                {
                    "tool_use_id": "1",
                    "name": "fs_write",
                    "args": {
                        "command": "create",
                        "file_text": "Hello, world!",
                        "path": "/file6.txt",
                    }
                }
            ],
            [
                "Ok, I won't make it.",
            ],
        ]));

        let agents = get_test_agents(&os).await;
        let tool_manager = ToolManager::default();
        let tool_config = serde_json::from_str::<HashMap<String, ToolSpec>>(include_str!("tools/tool_index.json"))
            .expect("Tools failed to load");
        ChatSession::new(
            &mut os,
            std::io::stdout(),
            std::io::stderr(),
            "fake_conv_id",
            agents,
            None,
            InputSource::new_mock(vec![
                "/tools".to_string(),
                "/tools help".to_string(),
                "create a new file".to_string(),
                "y".to_string(),
                "create a new file".to_string(),
                "t".to_string(),
                "create a new file".to_string(), // should make without prompting due to 't'
                "/tools untrust fs_write".to_string(),
                "create a file".to_string(), // prompt again due to untrust
                "n".to_string(),             // cancel
                "/tools trust fs_write".to_string(),
                "create a file".to_string(), // again without prompting due to '/tools trust'
                "/tools reset".to_string(),
                "create a file".to_string(), // prompt again due to reset
                "n".to_string(),             // cancel
                "exit".to_string(),
            ]),
            false,
            || Some(80),
            tool_manager,
            None,
            tool_config,
            true,
            false,
        )
        .await
        .unwrap()
        .spawn(&mut os)
        .await
        .unwrap();

        assert_eq!(os.fs.read_to_string("/file2.txt").await.unwrap(), "Hello, world!\n");
        assert_eq!(os.fs.read_to_string("/file3.txt").await.unwrap(), "Hello, world!\n");
        assert!(!os.fs.exists("/file4.txt"));
        assert_eq!(os.fs.read_to_string("/file5.txt").await.unwrap(), "Hello, world!\n");
        // TODO: fix this with agent change (dingfeli)
        // assert!(!ctx.fs.exists("/file6.txt"));
    }

    #[tokio::test]
    async fn test_flow_multiple_tools() {
        // let _ = tracing_subscriber::fmt::try_init();
        let mut os = Os::new().await.unwrap();
        os.client.set_mock_output(serde_json::json!([
            [
                "Sure, I'll create a file for you",
                {
                    "tool_use_id": "1",
                    "name": "fs_write",
                    "args": {
                        "command": "create",
                        "file_text": "Hello, world!",
                        "path": "/file1.txt",
                    }
                },
                {
                    "tool_use_id": "2",
                    "name": "fs_write",
                    "args": {
                        "command": "create",
                        "file_text": "Hello, world!",
                        "path": "/file2.txt",
                    }
                }
            ],
            [
                "Done",
            ],
            [
                "Sure, I'll create a file for you",
                {
                    "tool_use_id": "1",
                    "name": "fs_write",
                    "args": {
                        "command": "create",
                        "file_text": "Hello, world!",
                        "path": "/file3.txt",
                    }
                },
                {
                    "tool_use_id": "2",
                    "name": "fs_write",
                    "args": {
                        "command": "create",
                        "file_text": "Hello, world!",
                        "path": "/file4.txt",
                    }
                }
            ],
            [
                "Done",
            ],
        ]));

        let agents = get_test_agents(&os).await;
        let tool_manager = ToolManager::default();
        let tool_config = serde_json::from_str::<HashMap<String, ToolSpec>>(include_str!("tools/tool_index.json"))
            .expect("Tools failed to load");
        ChatSession::new(
            &mut os,
            std::io::stdout(),
            std::io::stderr(),
            "fake_conv_id",
            agents,
            None,
            InputSource::new_mock(vec![
                "create 2 new files parallel".to_string(),
                "t".to_string(),
                "/tools reset".to_string(),
                "create 2 new files parallel".to_string(),
                "y".to_string(),
                "y".to_string(),
                "exit".to_string(),
            ]),
            false,
            || Some(80),
            tool_manager,
            None,
            tool_config,
            true,
            false,
        )
        .await
        .unwrap()
        .spawn(&mut os)
        .await
        .unwrap();

        assert_eq!(os.fs.read_to_string("/file1.txt").await.unwrap(), "Hello, world!\n");
        assert_eq!(os.fs.read_to_string("/file2.txt").await.unwrap(), "Hello, world!\n");
        assert_eq!(os.fs.read_to_string("/file3.txt").await.unwrap(), "Hello, world!\n");
        assert_eq!(os.fs.read_to_string("/file4.txt").await.unwrap(), "Hello, world!\n");
    }

    #[tokio::test]
    async fn test_flow_tools_trust_all() {
        // let _ = tracing_subscriber::fmt::try_init();
        let mut os = Os::new().await.unwrap();
        os.client.set_mock_output(serde_json::json!([
            [
                "Sure, I'll create a file for you",
                {
                    "tool_use_id": "1",
                    "name": "fs_write",
                    "args": {
                        "command": "create",
                        "file_text": "Hello, world!",
                        "path": "/file1.txt",
                    }
                }
            ],
            [
                "Done",
            ],
            [
                "Sure, I'll create a file for you",
                {
                    "tool_use_id": "1",
                    "name": "fs_write",
                    "args": {
                        "command": "create",
                        "file_text": "Hello, world!",
                        "path": "/file3.txt",
                    }
                }
            ],
            [
                "Ok I won't.",
            ],
        ]));

        let agents = get_test_agents(&os).await;
        let tool_manager = ToolManager::default();
        let tool_config = serde_json::from_str::<HashMap<String, ToolSpec>>(include_str!("tools/tool_index.json"))
            .expect("Tools failed to load");
        ChatSession::new(
            &mut os,
            std::io::stdout(),
            std::io::stderr(),
            "fake_conv_id",
            agents,
            None,
            InputSource::new_mock(vec![
                "/tools trust-all".to_string(),
                "create a new file".to_string(),
                "/tools reset".to_string(),
                "create a new file".to_string(),
                "exit".to_string(),
            ]),
            false,
            || Some(80),
            tool_manager,
            None,
            tool_config,
            true,
            false,
        )
        .await
        .unwrap()
        .spawn(&mut os)
        .await
        .unwrap();

        assert_eq!(os.fs.read_to_string("/file1.txt").await.unwrap(), "Hello, world!\n");
        assert!(!os.fs.exists("/file2.txt"));
    }

    #[test]
    fn test_editor_content_processing() {
        // Since we no longer have template replacement, this test is simplified
        let cases = vec![
            ("My content", "My content"),
            ("My content with newline\n", "My content with newline"),
            ("", ""),
        ];

        for (input, expected) in cases {
            let processed = input.trim().to_string();
            assert_eq!(processed, expected.trim().to_string(), "Failed for input: {}", input);
        }
    }

    #[tokio::test]
    #[cfg(unix)]
    async fn test_subscribe_flow() {
        let mut os = Os::new().await.unwrap();
        os.client.set_mock_output(serde_json::Value::Array(vec![]));
        let agents = get_test_agents(&os).await;

        let tool_manager = ToolManager::default();
        let tool_config = serde_json::from_str::<HashMap<String, ToolSpec>>(include_str!("tools/tool_index.json"))
            .expect("Tools failed to load");
        ChatSession::new(
            &mut os,
            std::io::stdout(),
            std::io::stderr(),
            "fake_conv_id",
            agents,
            None,
            InputSource::new_mock(vec!["/subscribe".to_string(), "y".to_string(), "/quit".to_string()]),
            false,
            || Some(80),
            tool_manager,
            None,
            tool_config,
            true,
            false,
        )
        .await
        .unwrap()
        .spawn(&mut os)
        .await
        .unwrap();
    }

    #[test]
    fn test_does_input_reference_file() {
        let tests = &[
            (
                r"/Users/user/Desktop/Screenshot\ 2025-06-30\ at\ 2.13.34 PM.png read this image for me",
                true,
            ),
            ("/path/to/file.json", true),
            ("/save output.json", false),
            ("~/does/not/start/with/slash", false),
        ];
        for (input, expected) in tests {
            let actual = does_input_reference_file(input).is_some();
            assert_eq!(actual, *expected, "expected {} for input {}", expected, input);
        }
    }
}
