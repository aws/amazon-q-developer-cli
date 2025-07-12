pub mod cli;
mod consts;
pub mod context;
mod conversation;
mod error_formatter;
mod history_utils;
mod input_source;
mod message;
mod parse;
use std::path::MAIN_SEPARATOR;
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
use std::time::Duration;

use amzn_codewhisperer_client::types::SubscriptionStatus;
use clap::{
    Args,
    CommandFactory,
    Parser,
};
use cli::compact::CompactStrategy;
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
    ResponseParser,
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
use tool_manager::{
    ToolManager,
    ToolManagerBuilder,
};
use tools::gh_issue::GhIssueContext;
use tools::{
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

use super::agent::PermissionEvalResult;
use crate::api_client::ApiClientError;
use crate::api_client::model::ToolResultStatus;
use crate::api_client::send_message_output::SendMessageOutput;
use crate::auth::AuthError;
use crate::auth::builder_id::is_idc_user;
use crate::cli::agent::Agents;
use crate::cli::chat::cli::SlashCommand;
use crate::cli::chat::cli::model::{
    MODEL_OPTIONS,
    default_model_id,
};
use crate::cli::chat::cli::prompts::{
    GetPromptError,
    PromptsSubcommand,
};
use crate::database::settings::Setting;
use crate::mcp_client::Prompt;
use crate::os::Os;
use crate::telemetry::core::ToolUseEventBuilder;
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
<black!>You can now configure the Amazon Q CLI to use MCP servers. \nLearn how: https://docs.aws.amazon.com/en_us/amazonq/latest/qdeveloper-ug/command-line-mcp.html</black!>

<cyan,em>Tips:</cyan,em>
<em>!{command}</em>          <black!>Quickly execute a command in your current session</black!>
<em>Ctrl(^) + j</em>         <black!>Insert new-line to provide multi-line prompt</black!>
                    <black!>Alternatively, [Alt(⌥) + Enter(⏎)]</black!>
<em>Ctrl(^) + s</em>         <black!>Fuzzy search commands and context files</black!>
                    <black!>Use Tab to select multiple items</black!>
                    <black!>Change the keybind using: q settings chat.skimCommandKey x</black!>
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
    /// Run migration of legacy profiles to agents if applicable
    #[arg(long)]
    pub migrate: bool,
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

        let agents = {
            let skip_migration = self.no_interactive || !self.migrate;
            let mut agents = Agents::load(os, self.agent.as_deref(), skip_migration, &mut stderr).await;
            agents.trust_all_tools = self.trust_all_tools;

            if agents
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
                if let Some(a) = agents.get_active_mut() {
                    a.allowed_tools.extend(trust_tools);
                }
            }

            agents
        };

        // If modelId is specified, verify it exists before starting the chat
        let model_id: Option<String> = if let Some(model_name) = self.model {
            let model_name_lower = model_name.to_lowercase();
            match MODEL_OPTIONS.iter().find(|opt| opt.name == model_name_lower) {
                Some(opt) => Some((opt.model_id).to_string()),
                None => {
                    let available_names: Vec<&str> = MODEL_OPTIONS.iter().map(|opt| opt.name).collect();
                    bail!(
                        "Model '{}' does not exist. Available models: {}",
                        model_name,
                        available_names.join(", ")
                    );
                },
            }
        } else {
            None
        };

        let conversation_id = uuid::Uuid::new_v4().to_string();
        info!(?conversation_id, "Generated new conversation id");
        let (prompt_request_sender, prompt_request_receiver) = std::sync::mpsc::channel::<Option<String>>();
        let (prompt_response_sender, prompt_response_receiver) = std::sync::mpsc::channel::<Vec<String>>();
        let mut tool_manager = ToolManagerBuilder::default()
            .prompt_list_sender(prompt_response_sender)
            .prompt_list_receiver(prompt_request_receiver)
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
const ROTATING_TIPS: [&str; 16] = [
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
}

impl ChatError {
    fn status_code(&self) -> Option<u16> {
        match self {
            ChatError::Client(e) => e.status_code(),
            ChatError::Auth(_) => None,
            ChatError::ResponseStream(_) => None,
            ChatError::Std(_) => None,
            ChatError::Readline(_) => None,
            ChatError::Custom(_) => None,
            ChatError::Interrupted { .. } => None,
            ChatError::GetPromptError(_) => None,
            ChatError::NonInteractiveToolApproval => None,
            ChatError::CompactHistoryFailure => None,
        }
    }
}

impl ReasonCode for ChatError {
    fn reason_code(&self) -> String {
        match self {
            ChatError::Client(e) => e.reason_code(),
            ChatError::ResponseStream(e) => e.reason_code(),
            ChatError::Std(_) => "StdIoError".to_string(),
            ChatError::Readline(_) => "ReadlineError".to_string(),
            ChatError::Custom(_) => "GenericError".to_string(),
            ChatError::Interrupted { .. } => "Interrupted".to_string(),
            ChatError::GetPromptError(_) => "GetPromptError".to_string(),
            ChatError::Auth(_) => "AuthError".to_string(),
            ChatError::NonInteractiveToolApproval => "NonInteractiveToolApproval".to_string(),
            ChatError::CompactHistoryFailure => "CompactHistoryFailure".to_string(),
        }
    }
}

impl From<ApiClientError> for ChatError {
    fn from(value: ApiClientError) -> Self {
        Self::Client(Box::new(value))
    }
}

impl From<parser::RecvError> for ChatError {
    fn from(value: parser::RecvError) -> Self {
        Self::ResponseStream(Box::new(value))
    }
}

pub struct ChatSession {
    /// For output read by humans and machine
    pub stdout: std::io::Stdout,
    /// For display output, only read by humans
    pub stderr: std::io::Stderr,
    initial_input: Option<String>,
    /// Whether we're starting a new conversation or continuing an old one.
    existing_conversation: bool,
    input_source: InputSource,
    /// Width of the terminal, required for [ParseState].
    terminal_width_provider: fn() -> Option<usize>,
    spinner: Option<Spinner>,
    /// [ConversationState].
    conversation: ConversationState,
    tool_uses: Vec<QueuedTool>,
    pending_tool_index: Option<usize>,
    /// Telemetry events to be sent as part of the conversation.
    tool_use_telemetry_events: HashMap<String, ToolUseEventBuilder>,
    /// State used to keep track of tool use relation
    tool_use_status: ToolUseStatus,
    /// Any failed requests that could be useful for error report/debugging
    failed_request_ids: Vec<String>,
    /// Pending prompts to be sent
    pending_prompts: VecDeque<Prompt>,
    interactive: bool,
    inner: Option<ChatState>,
}

impl ChatSession {
    #[allow(clippy::too_many_arguments)]
    pub async fn new(
        os: &mut Os,
        stdout: std::io::Stdout,
        mut stderr: std::io::Stderr,
        conversation_id: &str,
        mut agents: Agents,
        mut input: Option<String>,
        input_source: InputSource,
        resume_conversation: bool,
        terminal_width_provider: fn() -> Option<usize>,
        tool_manager: ToolManager,
        model_id: Option<String>,
        tool_config: HashMap<String, ToolSpec>,
        interactive: bool,
    ) -> Result<Self> {
        let valid_model_id = match model_id {
            Some(id) => id,
            None => {
                let from_settings = os
                    .database
                    .settings
                    .get_string(Setting::ChatDefaultModel)
                    .and_then(|model_name| {
                        MODEL_OPTIONS
                            .iter()
                            .find(|opt| opt.name == model_name)
                            .map(|opt| opt.model_id.to_owned())
                    });

                match from_settings {
                    Some(id) => id,
                    None => default_model_id(os).await.to_owned(),
                }
            },
        };

        // Reload prior conversation
        let mut existing_conversation = false;
        let previous_conversation = std::env::current_dir()
            .ok()
            .and_then(|cwd| os.database.get_conversation_by_path(cwd).ok())
            .flatten();

        // Only restore conversations where there were actual messages.
        // Prevents edge case where user clears conversation then exits without chatting.
        let conversation = match resume_conversation
            && previous_conversation
                .as_ref()
                .is_some_and(|cs| !cs.history().is_empty())
        {
            true => {
                let mut cs = previous_conversation.unwrap();
                existing_conversation = true;
                input = Some(input.unwrap_or("In a few words, summarize our conversation so far.".to_owned()));
                cs.tool_manager = tool_manager;
                if let Some(profile) = cs.current_profile() {
                    if agents.switch(profile).is_err() {
                        execute!(
                            stderr,
                            style::SetForegroundColor(Color::Red),
                            style::Print("Error"),
                            style::ResetColor,
                            style::Print(format!(
                                ": cannot resume conversation with {profile} because it no longer exists. Using default.\n"
                            ))
                        )?;
                        let _ = agents.switch("default");
                    }
                }
                cs.agents = agents;
                cs.update_state(true).await;
                cs.enforce_tool_use_history_invariants();
                cs
            },
            false => {
                ConversationState::new(conversation_id, agents, tool_config, tool_manager, Some(valid_model_id)).await
            },
        };

        Ok(Self {
            stdout,
            stderr,
            initial_input: input,
            existing_conversation,
            input_source,
            terminal_width_provider,
            spinner: None,
            conversation,
            tool_uses: vec![],
            pending_tool_index: None,
            tool_use_telemetry_events: HashMap::new(),
            tool_use_status: ToolUseStatus::Idle,
            failed_request_ids: Vec::new(),
            pending_prompts: VecDeque::new(),
            interactive,
            inner: Some(ChatState::default()),
        })
    }

    pub async fn next(&mut self, os: &mut Os) -> Result<(), ChatError> {
        // Update conversation state with new tool information
        self.conversation.update_state(false).await;

        let ctrl_c_stream = ctrl_c();
        let result = match self.inner.take().expect("state must always be Some") {
            ChatState::PromptUser { skip_printing_tools } => {
                match (self.interactive, self.tool_uses.is_empty()) {
                    (false, true) => {
                        self.inner = Some(ChatState::Exit);
                        return Ok(());
                    },
                    (false, false) => {
                        return Err(ChatError::NonInteractiveToolApproval);
                    },
                    _ => (),
                };

                self.prompt_user(os, skip_printing_tools).await
            },
            ChatState::HandleInput { input } => {
                tokio::select! {
                    res = self.handle_input(os, input) => res,
                    Ok(_) = ctrl_c_stream => Err(ChatError::Interrupted { tool_uses: Some(self.tool_uses.clone()) })
                }
            },
            ChatState::CompactHistory {
                prompt,
                show_summary,
                strategy,
            } => {
                tokio::select! {
                    res = self.compact_history(os, prompt, show_summary, strategy) => res,
                    Ok(_) = ctrl_c_stream => Err(ChatError::Interrupted { tool_uses: Some(self.tool_uses.clone()) })
                }
            },
            ChatState::ExecuteTools => {
                let tool_uses_clone = self.tool_uses.clone();
                tokio::select! {
                    res = self.tool_use_execute(os) => res,
                    Ok(_) = ctrl_c_stream => Err(ChatError::Interrupted { tool_uses: Some(tool_uses_clone) })
                }
            },
            ChatState::ValidateTools(tool_uses) => {
                tokio::select! {
                    res = self.validate_tools(os, tool_uses) => res,
                    Ok(_) = ctrl_c_stream => Err(ChatError::Interrupted { tool_uses: None })
                }
            },
            ChatState::HandleResponseStream(response) => tokio::select! {
                res = self.handle_response(os, response) => res,
                Ok(_) = ctrl_c_stream => {
                    self.send_chat_telemetry(os, None, TelemetryResult::Cancelled, None, None, None).await;
                    Err(ChatError::Interrupted { tool_uses: None })
                }
            },
            ChatState::Exit => return Ok(()),
        };

        let err = match result {
            Ok(state) => {
                self.inner = Some(state);
                return Ok(());
            },
            Err(err) => err,
        };

        // We encountered an error. Handle it.
        error!(?err, "An error occurred processing the current state");
        let (reason, reason_desc) = get_error_reason(&err);
        self.send_error_telemetry(os, reason, Some(reason_desc), err.status_code())
            .await;

        if self.spinner.is_some() {
            drop(self.spinner.take());
            queue!(
                self.stderr,
                terminal::Clear(terminal::ClearType::CurrentLine),
                cursor::MoveToColumn(0),
            )?;
        }

        let (context, report, display_err_message) = match err {
            ChatError::Interrupted { tool_uses: ref inter } => {
                execute!(self.stderr, style::Print("\n\n"))?;

                // If there was an interrupt during tool execution, then we add fake
                // messages to "reset" the chat state.
                match inter {
                    Some(tool_uses) if !tool_uses.is_empty() => {
                        self.conversation
                            .abandon_tool_use(tool_uses, "The user interrupted the tool execution.".to_string());
                        let _ = self
                            .conversation
                            .as_sendable_conversation_state(os, &mut self.stderr, false)
                            .await?;
                        self.conversation.push_assistant_message(
                            os,
                            AssistantMessage::new_response(
                                None,
                                "Tool uses were interrupted, waiting for the next user prompt".to_string(),
                            ),
                        );
                    },
                    _ => (),
                }

                ("Tool use was interrupted", Report::from(err), false)
            },
            ChatError::CompactHistoryFailure => {
                // This error is not retryable - the user must take manual intervention to manage
                // their context.
                execute!(
                    self.stderr,
                    style::SetForegroundColor(Color::Red),
                    style::Print("Your conversation is too large to continue.\n"),
                    style::SetForegroundColor(Color::Reset),
                    style::Print(format!(
                        "• Run {} to compact your conversation. See {} for compaction options\n",
                        "/compact".green(),
                        "/compact --help".green()
                    )),
                    style::Print(format!("• Run {} to analyze your context usage\n", "/usage".green())),
                    style::Print(format!("• Run {} to reset your conversation state\n", "/clear".green())),
                    style::SetAttribute(Attribute::Reset),
                    style::Print("\n\n"),
                )?;
                ("Unable to compact the conversation history", eyre!(err), true)
            },
            ChatError::Client(err) => match *err {
                // Errors from attempting to send too large of a conversation history. In
                // this case, attempt to automatically compact the history for the user.
                ApiClientError::ContextWindowOverflow { .. } => {
                    if os
                        .database
                        .settings
                        .get_bool(Setting::ChatDisableAutoCompaction)
                        .unwrap_or(false)
                    {
                        execute!(
                            self.stderr,
                            style::SetForegroundColor(Color::Red),
                            style::Print("The conversation history has overflowed.\n"),
                            style::SetForegroundColor(Color::Reset),
                            style::Print(format!("• Run {} to compact your conversation\n", "/compact".green())),
                            style::SetAttribute(Attribute::Reset),
                            style::Print("\n\n"),
                        )?;
                        ("The conversation history has overflowed", eyre!(err), false)
                    } else {
                        self.inner = Some(ChatState::CompactHistory {
                            prompt: None,
                            show_summary: false,
                            strategy: CompactStrategy {
                                truncate_large_messages: self.conversation.history().len() <= 2,
                                ..Default::default()
                            },
                        });

                        execute!(
                            self.stdout,
                            style::SetForegroundColor(Color::Yellow),
                            style::Print("The context window has overflowed, summarizing the history..."),
                            style::SetAttribute(Attribute::Reset),
                            style::Print("\n\n"),
                        )?;

                        return Ok(());
                    }
                },
                ApiClientError::QuotaBreach {
                    message: _,
                    status_code: _,
                } => {
                    let err = "Request quota exceeded. Please wait a moment and try again.".to_string();
                    self.conversation.append_transcript(err.clone());
                    execute!(
                        self.stderr,
                        style::SetAttribute(Attribute::Bold),
                        style::SetForegroundColor(Color::Red),
                        style::Print(" ⚠️  Amazon Q rate limit reached:\n"),
                        style::Print(format!("    {}\n\n", err.clone())),
                        style::SetAttribute(Attribute::Reset),
                        style::SetForegroundColor(Color::Reset),
                    )?;
                    ("Amazon Q is having trouble responding right now", eyre!(err), false)
                },
                ApiClientError::ModelOverloadedError { request_id, .. } => {
                    let model_instruction = if self.interactive {
                        "Please use '/model' to select a different model and try again."
                    } else {
                        "Please relaunch with '--model <model_id>' to use a different model."
                    };

                    let err = format!(
                        "The model you've selected is temporarily unavailable. {}{}\n\n",
                        model_instruction,
                        match request_id {
                            Some(id) => format!("\n    Request ID: {}", id),
                            None => "".to_owned(),
                        }
                    );
                    self.conversation.append_transcript(err.clone());
                    execute!(
                        self.stderr,
                        style::SetAttribute(Attribute::Bold),
                        style::SetForegroundColor(Color::Red),
                        style::Print("Amazon Q is having trouble responding right now:\n"),
                        style::Print(format!("    {}\n", err.clone())),
                        style::SetAttribute(Attribute::Reset),
                        style::SetForegroundColor(Color::Reset),
                    )?;
                    ("Amazon Q is having trouble responding right now", eyre!(err), false)
                },
                ApiClientError::MonthlyLimitReached { .. } => {
                    let subscription_status = get_subscription_status(os).await;
                    if subscription_status.is_err() {
                        execute!(
                            self.stderr,
                            style::SetForegroundColor(Color::Red),
                            style::Print(format!(
                                "Unable to verify subscription status: {}\n\n",
                                subscription_status.as_ref().err().unwrap()
                            )),
                            style::SetForegroundColor(Color::Reset),
                        )?;
                    }

                    execute!(
                        self.stderr,
                        style::SetForegroundColor(Color::Yellow),
                        style::Print("Monthly request limit reached"),
                        style::SetForegroundColor(Color::Reset),
                    )?;

                    let limits_text = format!(
                        "The limits reset on {:02}/01.",
                        OffsetDateTime::now_utc().month().next() as u8
                    );

                    if subscription_status.is_err()
                        || subscription_status.is_ok_and(|s| s == ActualSubscriptionStatus::None)
                    {
                        execute!(
                            self.stderr,
                            style::Print(format!("\n\n{LIMIT_REACHED_TEXT} {limits_text}")),
                            style::SetForegroundColor(Color::DarkGrey),
                            style::Print("\n\nUse "),
                            style::SetForegroundColor(Color::Green),
                            style::Print("/subscribe"),
                            style::SetForegroundColor(Color::DarkGrey),
                            style::Print(" to upgrade your subscription.\n\n"),
                            style::SetForegroundColor(Color::Reset),
                        )?;
                    } else {
                        execute!(
                            self.stderr,
                            style::SetForegroundColor(Color::Yellow),
                            style::Print(format!(" - {limits_text}\n\n")),
                            style::SetForegroundColor(Color::Reset),
                        )?;
                    }

                    self.inner = Some(ChatState::PromptUser {
                        skip_printing_tools: false,
                    });

                    return Ok(());
                },
                _ => (
                    "Amazon Q is having trouble responding right now",
                    Report::from(err),
                    true,
                ),
            },
            _ => (
                "Amazon Q is having trouble responding right now",
                Report::from(err),
                true,
            ),
        };

        if display_err_message {
            // Remove non-ASCII and ANSI characters.
            let re = Regex::new(r"((\x9B|\x1B\[)[0-?]*[ -\/]*[@-~])|([^\x00-\x7F]+)").unwrap();

            queue!(
                self.stderr,
                style::SetAttribute(Attribute::Bold),
                style::SetForegroundColor(Color::Red),
            )?;

            let text = re.replace_all(&format!("{}: {:?}\n", context, report), "").into_owned();

            queue!(self.stderr, style::Print(&text),)?;
            self.conversation.append_transcript(text);

            execute!(
                self.stderr,
                style::SetAttribute(Attribute::Reset),
                style::SetForegroundColor(Color::Reset),
            )?;
        }

        self.conversation.enforce_conversation_invariants();
        self.conversation.reset_next_user_message();
        self.pending_tool_index = None;

        self.inner = Some(ChatState::PromptUser {
            skip_printing_tools: false,
        });

        Ok(())
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
    ValidateTools(Vec<AssistantToolUse>),
    /// Execute the list of tools.
    ExecuteTools,
    /// Consume the response stream and display to the user.
    HandleResponseStream(SendMessageOutput),
    /// Compact the chat history.
    CompactHistory {
        /// Custom prompt to include as part of history compaction.
        prompt: Option<String>,
        /// Whether or not the summary should be shown on compact success.
        show_summary: bool,
        /// Parameters for how to perform the compaction request.
        strategy: CompactStrategy,
    },
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

impl ChatSession {
    async fn spawn(&mut self, os: &mut Os) -> Result<()> {
        let is_small_screen = self.terminal_width() < GREETING_BREAK_POINT;
        if os
            .database
            .settings
            .get_bool(Setting::ChatGreetingEnabled)
            .unwrap_or(true)
        {
            let welcome_text = match self.existing_conversation {
                true => RESUME_TEXT,
                false => match is_small_screen {
                    true => SMALL_SCREEN_WELCOME_TEXT,
                    false => WELCOME_TEXT,
                },
            };

            execute!(self.stderr, style::Print(welcome_text), style::Print("\n\n"),)?;

            let tip = ROTATING_TIPS[usize::try_from(rand::random::<u32>()).unwrap_or(0) % ROTATING_TIPS.len()];
            if is_small_screen {
                // If the screen is small, print the tip in a single line
                execute!(
                    self.stderr,
                    style::Print("💡 ".to_string()),
                    style::Print(tip),
                    style::Print("\n")
                )?;
            } else {
                draw_box(
                    &mut self.stderr,
                    "Did you know?",
                    tip,
                    GREETING_BREAK_POINT,
                    Color::DarkGrey,
                )?;
            }

            execute!(
                self.stderr,
                style::Print("\n"),
                style::Print(match is_small_screen {
                    true => SMALL_SCREEN_POPULAR_SHORTCUTS,
                    false => POPULAR_SHORTCUTS,
                }),
                style::Print("\n"),
                style::Print(
                    "━"
                        .repeat(if is_small_screen { 0 } else { GREETING_BREAK_POINT })
                        .dark_grey()
                )
            )?;
            execute!(self.stderr, style::Print("\n"), style::SetForegroundColor(Color::Reset))?;
        }

        if self.all_tools_trusted() {
            queue!(
                self.stderr,
                style::Print(format!(
                    "{}{TRUST_ALL_TEXT}\n\n",
                    if !is_small_screen { "\n" } else { "" }
                ))
            )?;
        }
        self.stderr.flush()?;

        if let Some(ref id) = self.conversation.model {
            if let Some(model_option) = MODEL_OPTIONS.iter().find(|option| option.model_id == *id) {
                execute!(
                    self.stderr,
                    style::SetForegroundColor(Color::Cyan),
                    style::Print(format!("🤖 You are chatting with {}\n", model_option.name)),
                    style::SetForegroundColor(Color::Reset),
                    style::Print("\n")
                )?;
            }
        }

        if let Some(user_input) = self.initial_input.take() {
            self.inner = Some(ChatState::HandleInput { input: user_input });
        }

        while !matches!(self.inner, Some(ChatState::Exit)) {
            self.next(os).await?;
        }

        Ok(())
    }

    /// Compacts the conversation history using the strategy specified by [CompactStrategy],
    /// replacing the history with a summary generated by the model.
    ///
    /// If the compact request itself fails, it will be retried depending on [CompactStrategy]
    ///
    /// If [CompactStrategy::messages_to_exclude] is greater than 0, and
    /// [CompactStrategy::truncate_large_messages] is true, then compaction will not be retried and
    /// will fail with [ChatError::CompactHistoryFailure].
    async fn compact_history(
        &mut self,
        os: &Os,
        custom_prompt: Option<String>,
        show_summary: bool,
        strategy: CompactStrategy,
    ) -> Result<ChatState, ChatError> {
        let hist = self.conversation.history();
        debug!(?strategy, ?hist, "compacting history");

        if self.conversation.history().is_empty() {
            execute!(
                self.stderr,
                style::SetForegroundColor(Color::Yellow),
                style::Print("\nConversation too short to compact.\n\n"),
                style::SetForegroundColor(Color::Reset)
            )?;

            return Ok(ChatState::PromptUser {
                skip_printing_tools: true,
            });
        }

        if strategy.truncate_large_messages {
            info!("truncating large messages");
            execute!(
                self.stderr,
                terminal::Clear(terminal::ClearType::CurrentLine),
                cursor::MoveToColumn(0),
                style::SetForegroundColor(Color::Yellow),
                style::Print("Truncating large messages..."),
                style::SetAttribute(Attribute::Reset),
                style::Print("\n\n"),
            )?;
        }

        // Send a request for summarizing the history.
        let summary_state = self
            .conversation
            .create_summary_request(os, custom_prompt.as_ref(), strategy)
            .await?;

        execute!(self.stderr, cursor::Hide, style::Print("\n"))?;

        if self.interactive {
            self.spinner = Some(Spinner::new(Spinners::Dots, "Creating summary...".to_string()));
        }

        let response = os.client.send_message(summary_state).await;

        let response = match response {
            Ok(res) => res,
            Err(err) => {
                if self.interactive {
                    self.spinner.take();
                    execute!(
                        self.stderr,
                        terminal::Clear(terminal::ClearType::CurrentLine),
                        cursor::MoveToColumn(0),
                        style::SetAttribute(Attribute::Reset)
                    )?;
                }

                let (reason, reason_desc) = get_error_reason(&err);
                self.send_chat_telemetry(
                    os,
                    None,
                    TelemetryResult::Failed,
                    Some(reason),
                    Some(reason_desc),
                    err.status_code(),
                )
                .await;
                let history_len = self.conversation.history().len();
                match err {
                    ApiClientError::ContextWindowOverflow { .. } => {
                        error!(?strategy, "failed to send compaction request");
                        // If there's only two messages in the history, we have no choice but to
                        // truncate it. We use two messages since it's almost guaranteed to contain:
                        // 1. A small user prompt
                        // 2. A large user tool use result
                        if history_len <= 2 && !strategy.truncate_large_messages {
                            return Ok(ChatState::CompactHistory {
                                prompt: custom_prompt,
                                show_summary,
                                strategy: CompactStrategy {
                                    truncate_large_messages: true,
                                    max_message_length: 25_000,
                                    messages_to_exclude: 0,
                                },
                            });
                        }

                        // Otherwise, we will first exclude the most recent message, and only then
                        // truncate. If both of these have already been set, then return an error.
                        if history_len > 2 && strategy.messages_to_exclude < 1 {
                            return Ok(ChatState::CompactHistory {
                                prompt: custom_prompt,
                                show_summary,
                                strategy: CompactStrategy {
                                    messages_to_exclude: 1,
                                    ..strategy
                                },
                            });
                        } else if !strategy.truncate_large_messages {
                            return Ok(ChatState::CompactHistory {
                                prompt: custom_prompt,
                                show_summary,
                                strategy: CompactStrategy {
                                    truncate_large_messages: true,
                                    max_message_length: 25_000,
                                    ..strategy
                                },
                            });
                        } else {
                            return Err(ChatError::CompactHistoryFailure);
                        }
                    },
                    err => return Err(err.into()),
                }
            },
        };

        let request_id = response.request_id().map(|s| s.to_string());
        let summary = {
            let mut parser = ResponseParser::new(response);
            loop {
                match parser.recv().await {
                    Ok(parser::ResponseEvent::EndStream { message }) => {
                        break message.content().to_string();
                    },
                    Ok(_) => (),
                    Err(err) => {
                        if let Some(request_id) = &err.request_id {
                            self.failed_request_ids.push(request_id.clone());
                        };
                        let (reason, reason_desc) = get_error_reason(&err);
                        self.send_chat_telemetry(
                            os,
                            err.request_id.clone(),
                            TelemetryResult::Failed,
                            Some(reason),
                            Some(reason_desc),
                            err.status_code(),
                        )
                        .await;
                        return Err(err.into());
                    },
                }
            }
        };

        if self.spinner.is_some() {
            drop(self.spinner.take());
            queue!(
                self.stderr,
                terminal::Clear(terminal::ClearType::CurrentLine),
                cursor::MoveToColumn(0),
                cursor::Show
            )?;
        }

        self.send_chat_telemetry(os, request_id, TelemetryResult::Succeeded, None, None, None)
            .await;

        self.conversation
            .replace_history_with_summary(summary.clone(), strategy);

        // Print output to the user.
        {
            execute!(
                self.stderr,
                style::SetForegroundColor(Color::Green),
                style::Print("✔ Conversation history has been compacted successfully!\n\n"),
                style::SetForegroundColor(Color::DarkGrey)
            )?;

            let mut output = Vec::new();
            if let Some(custom_prompt) = &custom_prompt {
                execute!(
                    output,
                    style::Print(format!("• Custom prompt applied: {}\n", custom_prompt))
                )?;
            }
            animate_output(&mut self.stderr, &output)?;

            // Display the summary if the show_summary flag is set
            if show_summary {
                // Add a border around the summary for better visual separation
                let terminal_width = self.terminal_width();
                let border = "═".repeat(terminal_width.min(80));
                execute!(
                    self.stderr,
                    style::Print("\n"),
                    style::SetForegroundColor(Color::Cyan),
                    style::Print(&border),
                    style::Print("\n"),
                    style::SetAttribute(Attribute::Bold),
                    style::Print("                       CONVERSATION SUMMARY"),
                    style::Print("\n"),
                    style::Print(&border),
                    style::SetAttribute(Attribute::Reset),
                    style::Print("\n\n"),
                )?;

                execute!(
                    output,
                    style::Print(&summary),
                    style::Print("\n\n"),
                    style::SetForegroundColor(Color::Cyan),
                    style::Print("The conversation history has been replaced with this summary.\n"),
                    style::Print("It contains all important details from previous interactions.\n"),
                )?;
                animate_output(&mut self.stderr, &output)?;

                execute!(
                    self.stderr,
                    style::Print(&border),
                    style::Print("\n\n"),
                    style::SetForegroundColor(Color::Reset)
                )?;
            }
        }

        // If a next message is set, then retry the request.
        if self.conversation.next_user_message().is_some() {
            Ok(ChatState::HandleResponseStream(
                os.client
                    .send_message(
                        self.conversation
                            .as_sendable_conversation_state(os, &mut self.stderr, false)
                            .await?,
                    )
                    .await?,
            ))
        } else {
            // Otherwise, return back to the prompt for any pending tool uses.
            Ok(ChatState::PromptUser {
                skip_printing_tools: true,
            })
        }
    }

    /// Read input from the user.
    async fn prompt_user(&mut self, os: &Os, skip_printing_tools: bool) -> Result<ChatState, ChatError> {
        execute!(self.stderr, cursor::Show)?;

        // Check token usage and display warnings if needed
        if self.pending_tool_index.is_none() {
            // Only display warnings when not waiting for tool approval
            if let Err(err) = self.display_char_warnings(os).await {
                warn!("Failed to display character limit warnings: {}", err);
            }
        }

        let show_tool_use_confirmation_dialog = !skip_printing_tools && self.pending_tool_index.is_some();
        if show_tool_use_confirmation_dialog {
            execute!(
                self.stderr,
                style::SetForegroundColor(Color::DarkGrey),
                style::Print("\nAllow this action? Use '"),
                style::SetForegroundColor(Color::Green),
                style::Print("t"),
                style::SetForegroundColor(Color::DarkGrey),
                style::Print("' to trust (always allow) this tool for the session. ["),
                style::SetForegroundColor(Color::Green),
                style::Print("y"),
                style::SetForegroundColor(Color::DarkGrey),
                style::Print("/"),
                style::SetForegroundColor(Color::Green),
                style::Print("n"),
                style::SetForegroundColor(Color::DarkGrey),
                style::Print("/"),
                style::SetForegroundColor(Color::Green),
                style::Print("t"),
                style::SetForegroundColor(Color::DarkGrey),
                style::Print("]:\n\n"),
                style::SetForegroundColor(Color::Reset),
            )?;
        }

        // Do this here so that the skim integration sees an updated view of the context *during the current
        // q session*. (e.g., if I add files to context, that won't show up for skim for the current
        // q session unless we do this in prompt_user... unless you can find a better way)
        #[cfg(unix)]
        if let Some(ref context_manager) = self.conversation.context_manager {
            use std::sync::Arc;

            use crate::cli::chat::consts::DUMMY_TOOL_NAME;

            let tool_names = self
                .conversation
                .tool_manager
                .tn_map
                .keys()
                .filter(|name| *name != DUMMY_TOOL_NAME)
                .cloned()
                .collect::<Vec<_>>();
            self.input_source
                .put_skim_command_selector(os, Arc::new(context_manager.clone()), tool_names);
        }

        execute!(
            self.stderr,
            style::SetForegroundColor(Color::Reset),
            style::SetAttribute(Attribute::Reset)
        )?;
        let prompt = self.generate_tool_trust_prompt();
        let user_input = match self.read_user_input(&prompt, false) {
            Some(input) => input,
            None => return Ok(ChatState::Exit),
        };

        self.conversation.append_user_transcript(&user_input);
        Ok(ChatState::HandleInput { input: user_input })
    }

    async fn handle_input(&mut self, os: &mut Os, mut user_input: String) -> Result<ChatState, ChatError> {
        queue!(self.stderr, style::Print('\n'))?;

        let input = user_input.trim();

        // handle image path
        if let Some(chat_state) = does_input_reference_file(input) {
            return Ok(chat_state);
        }
        if let Some(mut args) = input.strip_prefix("/").and_then(shlex::split) {
            // Required for printing errors correctly.
            let orig_args = args.clone();

            // We set the binary name as a dummy name "slash_command" which we
            // replace anytime we error out and print a usage statement.
            args.insert(0, "slash_command".to_owned());

            match SlashCommand::try_parse_from(args) {
                Ok(command) => {
                    match command.execute(os, self).await {
                        Ok(chat_state)
                            if matches!(chat_state, ChatState::Exit)
                                || matches!(chat_state, ChatState::HandleInput { input: _ })
                                // TODO(bskiser): this is just a hotfix for handling state changes
                                // from manually running /compact, without impacting behavior of
                                // other slash commands.
                                || matches!(chat_state, ChatState::CompactHistory { .. }) =>
                        {
                            return Ok(chat_state);
                        },
                        Err(err) => {
                            queue!(
                                self.stderr,
                                style::SetForegroundColor(Color::Red),
                                style::Print(format!("\nFailed to execute command: {}\n", err)),
                                style::SetForegroundColor(Color::Reset)
                            )?;
                        },
                        _ => {},
                    }

                    writeln!(self.stderr)?;
                },
                Err(err) => {
                    // Replace the dummy name with a slash. Also have to check for an ansi sequence
                    // for invalid slash commands (e.g. on a "/doesntexist" input).
                    let ansi_output = err
                        .render()
                        .ansi()
                        .to_string()
                        .replace("slash_command ", "/")
                        .replace("slash_command\u{1b}[0m ", "/");

                    writeln!(self.stderr, "{}", ansi_output)?;

                    // Print the subcommand help, if available. Required since by default we won't
                    // show what the actual arguments are, requiring an unnecessary --help call.
                    if let clap::error::ErrorKind::InvalidValue
                    | clap::error::ErrorKind::UnknownArgument
                    | clap::error::ErrorKind::InvalidSubcommand
                    | clap::error::ErrorKind::MissingRequiredArgument = err.kind()
                    {
                        let mut cmd = SlashCommand::command();
                        for arg in &orig_args {
                            match cmd.find_subcommand(arg) {
                                Some(subcmd) => cmd = subcmd.clone(),
                                None => break,
                            }
                        }
                        let help = cmd.help_template("{all-args}").render_help();
                        writeln!(self.stderr, "{}", help.ansi())?;
                    }
                },
            }

            Ok(ChatState::PromptUser {
                skip_printing_tools: false,
            })
        } else if let Some(command) = input.strip_prefix("@") {
            let input_parts =
                shlex::split(command).ok_or(ChatError::Custom("Error splitting prompt command".into()))?;

            let mut iter = input_parts.into_iter();
            let prompt_name = iter
                .next()
                .ok_or(ChatError::Custom("Prompt name needs to be specified".into()))?;

            let args: Vec<String> = iter.collect();
            let arguments = if args.is_empty() { None } else { Some(args) };

            let subcommand = PromptsSubcommand::Get {
                orig_input: Some(command.to_string()),
                name: prompt_name,
                arguments,
            };
            return subcommand.execute(self).await;
        } else if let Some(command) = input.strip_prefix("!") {
            // Use platform-appropriate shell
            let result = if cfg!(target_os = "windows") {
                std::process::Command::new("cmd").args(["/C", command]).status()
            } else {
                std::process::Command::new("bash").args(["-c", command]).status()
            };

            // Handle the result and provide appropriate feedback
            match result {
                Ok(status) => {
                    if !status.success() {
                        queue!(
                            self.stderr,
                            style::SetForegroundColor(Color::Yellow),
                            style::Print(format!("Self exited with status: {}\n", status)),
                            style::SetForegroundColor(Color::Reset)
                        )?;
                    }
                },
                Err(e) => {
                    queue!(
                        self.stderr,
                        style::SetForegroundColor(Color::Red),
                        style::Print(format!("\nFailed to execute command: {}\n", e)),
                        style::SetForegroundColor(Color::Reset)
                    )?;
                },
            }

            Ok(ChatState::PromptUser {
                skip_printing_tools: false,
            })
        } else {
            // Check for a pending tool approval
            if let Some(index) = self.pending_tool_index {
                let is_trust = ["t", "T"].contains(&input);
                let tool_use = &mut self.tool_uses[index];
                if ["y", "Y"].contains(&input) || is_trust {
                    if is_trust {
                        let formatted_tool_name = self
                            .conversation
                            .tool_manager
                            .tn_map
                            .get(&tool_use.name)
                            .map(|info| {
                                format!(
                                    "@{}{MCP_SERVER_TOOL_DELIMITER}{}",
                                    info.server_name, info.host_tool_name
                                )
                            })
                            .clone()
                            .unwrap_or(tool_use.name.clone());
                        self.conversation.agents.trust_tools(vec![formatted_tool_name]);
                    }
                    tool_use.accepted = true;

                    return Ok(ChatState::ExecuteTools);
                }
            } else if !self.pending_prompts.is_empty() {
                let prompts = self.pending_prompts.drain(0..).collect();
                user_input = self
                    .conversation
                    .append_prompts(prompts)
                    .ok_or(ChatError::Custom("Prompt append failed".into()))?;
            }

            // Otherwise continue with normal chat on 'n' or other responses
            self.tool_use_status = ToolUseStatus::Idle;

            if self.pending_tool_index.is_some() {
                // If the user just enters "n", replace the message we send to the model with
                // something more substantial.
                // TODO: Update this flow to something that does *not* require two requests just to
                // get a meaningful response from the user - this is a short term solution before
                // we decide on a better flow.
                let user_input = if ["n", "N"].contains(&user_input.trim()) {
                    "I deny this tool request. Ask a follow up question clarifying the expected action".to_string()
                } else {
                    user_input
                };
                self.conversation.abandon_tool_use(&self.tool_uses, user_input);
            } else {
                self.conversation.set_next_user_message(user_input).await;
            }

            let conv_state = self
                .conversation
                .as_sendable_conversation_state(os, &mut self.stderr, true)
                .await?;
            self.send_tool_use_telemetry(os).await;

            queue!(self.stderr, style::SetForegroundColor(Color::Magenta))?;
            queue!(self.stderr, style::SetForegroundColor(Color::Reset))?;
            queue!(self.stderr, cursor::Hide)?;

            if self.interactive {
                self.spinner = Some(Spinner::new(Spinners::Dots, "Thinking...".to_owned()));
            }

            Ok(ChatState::HandleResponseStream(
                os.client.send_message(conv_state).await?,
            ))
        }
    }

    async fn tool_use_execute(&mut self, os: &mut Os) -> Result<ChatState, ChatError> {
        // Verify tools have permissions.
        for i in 0..self.tool_uses.len() {
            let tool = &mut self.tool_uses[i];

            // Manually accepted by the user or otherwise verified already.
            if tool.accepted {
                continue;
            }

            let mut denied = false;
            let allowed =
                self.conversation
                    .agents
                    .get_active()
                    .is_some_and(|a| match tool.tool.requires_acceptance(a) {
                        PermissionEvalResult::Allow => true,
                        PermissionEvalResult::Ask => false,
                        PermissionEvalResult::Deny => {
                            denied = true;
                            false
                        },
                    })
                    || self.conversation.agents.trust_all_tools;

            if denied {
                return Ok(ChatState::HandleInput {
                    input: format!(
                        "Tool use with {} was rejected because the arguments supplied were forbidden",
                        tool.name
                    ),
                });
            }

            if os
                .database
                .settings
                .get_bool(Setting::ChatEnableNotifications)
                .unwrap_or(false)
            {
                play_notification_bell(!allowed);
            }

            // TODO: Control flow is hacky here because of borrow rules
            let _ = tool;
            self.print_tool_description(os, i, allowed).await?;
            let tool = &mut self.tool_uses[i];

            if allowed {
                tool.accepted = true;
                continue;
            }

            self.pending_tool_index = Some(i);

            return Ok(ChatState::PromptUser {
                skip_printing_tools: false,
            });
        }

        // Execute the requested tools.
        let mut tool_results = vec![];
        let mut image_blocks: Vec<RichImageBlock> = Vec::new();

        for tool in &self.tool_uses {
            let mut tool_telemetry = self.tool_use_telemetry_events.entry(tool.id.clone());
            tool_telemetry = tool_telemetry.and_modify(|ev| ev.is_accepted = true);

            let tool_start = std::time::Instant::now();
            let invoke_result = tool.tool.invoke(os, &mut self.stdout).await;

            if self.spinner.is_some() {
                queue!(
                    self.stderr,
                    terminal::Clear(terminal::ClearType::CurrentLine),
                    cursor::MoveToColumn(0),
                    cursor::Show
                )?;
            }
            execute!(self.stdout, style::Print("\n"))?;

            let tool_time = std::time::Instant::now().duration_since(tool_start);
            if let Tool::Custom(ct) = &tool.tool {
                tool_telemetry = tool_telemetry.and_modify(|ev| {
                    ev.custom_tool_call_latency = Some(tool_time.as_secs() as usize);
                    ev.input_token_size = Some(ct.get_input_token_size());
                    ev.is_custom_tool = true;
                });
            }
            let tool_time = format!("{}.{}", tool_time.as_secs(), tool_time.subsec_millis());
            match invoke_result {
                Ok(result) => {
                    match result.output {
                        OutputKind::Text(ref text) => {
                            debug!("Output is Text: {}", text);
                        },
                        OutputKind::Json(ref json) => {
                            debug!("Output is JSON: {}", json);
                        },
                        OutputKind::Images(ref image) => {
                            image_blocks.extend(image.clone());
                        },
                        OutputKind::Mixed { ref text, ref images } => {
                            debug!("Output is Mixed: text = {:?}, images = {}", text, images.len());
                            image_blocks.extend(images.clone());
                        },
                    }

                    debug!("tool result output: {:#?}", result);
                    execute!(
                        self.stdout,
                        style::Print(CONTINUATION_LINE),
                        style::Print("\n"),
                        style::SetForegroundColor(Color::Green),
                        style::SetAttribute(Attribute::Bold),
                        style::Print(format!(" ● Completed in {}s", tool_time)),
                        style::SetForegroundColor(Color::Reset),
                        style::Print("\n\n"),
                    )?;

                    tool_telemetry = tool_telemetry.and_modify(|ev| ev.is_success = Some(true));
                    if let Tool::Custom(_) = &tool.tool {
                        tool_telemetry
                            .and_modify(|ev| ev.output_token_size = Some(TokenCounter::count_tokens(result.as_str())));
                    }
                    tool_results.push(ToolUseResult {
                        tool_use_id: tool.id.clone(),
                        content: vec![result.into()],
                        status: ToolResultStatus::Success,
                    });
                },
                Err(err) => {
                    error!(?err, "An error occurred processing the tool");
                    execute!(
                        self.stderr,
                        style::Print(CONTINUATION_LINE),
                        style::Print("\n"),
                        style::SetAttribute(Attribute::Bold),
                        style::SetForegroundColor(Color::Red),
                        style::Print(format!(" ● Execution failed after {}s:\n", tool_time)),
                        style::SetAttribute(Attribute::Reset),
                        style::SetForegroundColor(Color::Red),
                        style::Print(&err),
                        style::SetAttribute(Attribute::Reset),
                        style::Print("\n\n"),
                    )?;

                    tool_telemetry.and_modify(|ev| ev.is_success = Some(false));
                    tool_results.push(ToolUseResult {
                        tool_use_id: tool.id.clone(),
                        content: vec![ToolUseResultBlock::Text(format!(
                            "An error occurred processing the tool: \n{}",
                            &err
                        ))],
                        status: ToolResultStatus::Error,
                    });
                    if let ToolUseStatus::Idle = self.tool_use_status {
                        self.tool_use_status = ToolUseStatus::RetryInProgress(
                            self.conversation
                                .message_id()
                                .map_or("No utterance id found".to_string(), |v| v.to_string()),
                        );
                    }
                },
            }
        }

        if !image_blocks.is_empty() {
            let images = image_blocks.into_iter().map(|(block, _)| block).collect();
            self.conversation.add_tool_results_with_images(tool_results, images);
            execute!(
                self.stderr,
                style::SetAttribute(Attribute::Reset),
                style::SetForegroundColor(Color::Reset),
                style::Print("\n")
            )?;
        } else {
            self.conversation.add_tool_results(tool_results);
        }

        execute!(self.stderr, cursor::Hide)?;
        execute!(self.stderr, style::Print("\n"), style::SetAttribute(Attribute::Reset))?;
        if self.interactive {
            self.spinner = Some(Spinner::new(Spinners::Dots, "Thinking...".to_string()));
        }

        self.send_tool_use_telemetry(os).await;
        return Ok(ChatState::HandleResponseStream(
            os.client
                .send_message(
                    self.conversation
                        .as_sendable_conversation_state(os, &mut self.stderr, false)
                        .await?,
                )
                .await?,
        ));
    }

    async fn handle_response(&mut self, os: &mut Os, response: SendMessageOutput) -> Result<ChatState, ChatError> {
        let request_id = response.request_id().map(|s| s.to_string());
        let mut buf = String::new();
        let mut offset = 0;
        let mut ended = false;
        let mut parser = ResponseParser::new(response);
        let mut state = ParseState::new(Some(self.terminal_width()));
        let mut response_prefix_printed = false;

        let mut tool_uses = Vec::new();
        let mut tool_name_being_recvd: Option<String> = None;

        if self.spinner.is_some() {
            drop(self.spinner.take());
            queue!(
                self.stderr,
                style::SetForegroundColor(Color::Reset),
                cursor::MoveToColumn(0),
                cursor::Show,
                terminal::Clear(terminal::ClearType::CurrentLine),
            )?;
        }

        loop {
            match parser.recv().await {
                Ok(msg_event) => {
                    trace!("Consumed: {:?}", msg_event);
                    match msg_event {
                        parser::ResponseEvent::ToolUseStart { name } => {
                            // We need to flush the buffer here, otherwise text will not be
                            // printed while we are receiving tool use events.
                            buf.push('\n');
                            tool_name_being_recvd = Some(name);
                        },
                        parser::ResponseEvent::AssistantText(text) => {
                            // Add Q response prefix before the first assistant text.
                            // This must be markdown - using a code tick, which is printed
                            // as green.
                            if !response_prefix_printed && !text.trim().is_empty() {
                                buf.push_str("`>` ");
                                response_prefix_printed = true;
                            }
                            buf.push_str(&text);
                        },
                        parser::ResponseEvent::ToolUse(tool_use) => {
                            if self.spinner.is_some() {
                                drop(self.spinner.take());
                                queue!(
                                    self.stderr,
                                    terminal::Clear(terminal::ClearType::CurrentLine),
                                    cursor::MoveToColumn(0),
                                    cursor::Show
                                )?;
                            }
                            tool_uses.push(tool_use);
                            tool_name_being_recvd = None;
                        },
                        parser::ResponseEvent::EndStream { message } => {
                            // This log is attempting to help debug instances where users encounter
                            // the response timeout message.
                            if message.content() == RESPONSE_TIMEOUT_CONTENT {
                                error!(?request_id, ?message, "Encountered an unexpected model response");
                            }
                            self.conversation.push_assistant_message(os, message);
                            ended = true;
                        },
                    }
                },
                Err(recv_error) => {
                    if let Some(request_id) = &recv_error.request_id {
                        self.failed_request_ids.push(request_id.clone());
                    };

                    let (reason, reason_desc) = get_error_reason(&recv_error);
                    self.send_chat_telemetry(
                        os,
                        recv_error.request_id.clone(),
                        TelemetryResult::Failed,
                        Some(reason),
                        Some(reason_desc),
                        recv_error.status_code(),
                    )
                    .await;

                    match recv_error.source {
                        RecvErrorKind::StreamTimeout { source, duration } => {
                            error!(
                                recv_error.request_id,
                                ?source,
                                "Encountered a stream timeout after waiting for {}s",
                                duration.as_secs()
                            );

                            execute!(self.stderr, cursor::Hide)?;
                            self.spinner = Some(Spinner::new(Spinners::Dots, "Dividing up the work...".to_string()));

                            // For stream timeouts, we'll tell the model to try and split its response into
                            // smaller chunks.
                            self.conversation.push_assistant_message(
                                os,
                                AssistantMessage::new_response(None, RESPONSE_TIMEOUT_CONTENT.to_string()),
                            );
                            self.conversation
                                .set_next_user_message(
                                    "You took too long to respond - try to split up the work into smaller steps."
                                        .to_string(),
                                )
                                .await;
                            self.send_tool_use_telemetry(os).await;
                            return Ok(ChatState::HandleResponseStream(
                                os.client
                                    .send_message(
                                        self.conversation
                                            .as_sendable_conversation_state(os, &mut self.stderr, false)
                                            .await?,
                                    )
                                    .await?,
                            ));
                        },
                        RecvErrorKind::UnexpectedToolUseEos {
                            tool_use_id,
                            name,
                            message,
                            ..
                        } => {
                            error!(
                                recv_error.request_id,
                                tool_use_id, name, "The response stream ended before the entire tool use was received"
                            );
                            self.conversation.push_assistant_message(os, *message);
                            let tool_results = vec![ToolUseResult {
                                    tool_use_id,
                                    content: vec![ToolUseResultBlock::Text(
                                        "The generated tool was too large, try again but this time split up the work between multiple tool uses".to_string(),
                                    )],
                                    status: ToolResultStatus::Error,
                                }];
                            self.conversation.add_tool_results(tool_results);
                            self.send_tool_use_telemetry(os).await;
                            return Ok(ChatState::HandleResponseStream(
                                os.client
                                    .send_message(
                                        self.conversation
                                            .as_sendable_conversation_state(os, &mut self.stderr, false)
                                            .await?,
                                    )
                                    .await?,
                            ));
                        },
                        _ => return Err(recv_error.into()),
                    }
                },
            }

            // Fix for the markdown parser copied over from q chat:
            // this is a hack since otherwise the parser might report Incomplete with useful data
            // still left in the buffer. I'm not sure how this is intended to be handled.
            if ended {
                buf.push('\n');
            }

            if tool_name_being_recvd.is_none() && !buf.is_empty() && self.spinner.is_some() {
                drop(self.spinner.take());
                queue!(
                    self.stderr,
                    terminal::Clear(terminal::ClearType::CurrentLine),
                    cursor::MoveToColumn(0),
                    cursor::Show
                )?;
            }

            // Print the response for normal cases
            loop {
                let input = Partial::new(&buf[offset..]);
                match interpret_markdown(input, &mut self.stdout, &mut state) {
                    Ok(parsed) => {
                        offset += parsed.offset_from(&input);
                        self.stdout.flush()?;
                        state.newline = state.set_newline;
                        state.set_newline = false;
                    },
                    Err(err) => match err.into_inner() {
                        Some(err) => return Err(ChatError::Custom(err.to_string().into())),
                        None => break, // Data was incomplete
                    },
                }

                // TODO: We should buffer output based on how much we have to parse, not as a constant
                // Do not remove unless you are nabochay :)
                tokio::time::sleep(Duration::from_millis(8)).await;
            }

            // Set spinner after showing all of the assistant text content so far.
            if tool_name_being_recvd.is_some() {
                queue!(self.stderr, cursor::Hide)?;
                if self.interactive {
                    self.spinner = Some(Spinner::new(Spinners::Dots, "Thinking...".to_string()));
                }
            }

            if ended {
                self.send_chat_telemetry(os, request_id, TelemetryResult::Succeeded, None, None, None)
                    .await;

                if os
                    .database
                    .settings
                    .get_bool(Setting::ChatEnableNotifications)
                    .unwrap_or(false)
                {
                    // For final responses (no tools suggested), always play the bell
                    play_notification_bell(tool_uses.is_empty());
                }

                queue!(self.stderr, style::ResetColor, style::SetAttribute(Attribute::Reset))?;
                execute!(self.stdout, style::Print("\n"))?;

                for (i, citation) in &state.citations {
                    queue!(
                        self.stdout,
                        style::Print("\n"),
                        style::SetForegroundColor(Color::Blue),
                        style::Print(format!("[^{i}]: ")),
                        style::SetForegroundColor(Color::DarkGrey),
                        style::Print(format!("{citation}\n")),
                        style::SetForegroundColor(Color::Reset)
                    )?;
                }

                break;
            }
        }

        if !tool_uses.is_empty() {
            Ok(ChatState::ValidateTools(tool_uses))
        } else {
            self.tool_uses.clear();
            self.pending_tool_index = None;

            Ok(ChatState::PromptUser {
                skip_printing_tools: false,
            })
        }
    }

    async fn validate_tools(&mut self, os: &Os, tool_uses: Vec<AssistantToolUse>) -> Result<ChatState, ChatError> {
        let conv_id = self.conversation.conversation_id().to_owned();
        debug!(?tool_uses, "Validating tool uses");
        let mut queued_tools: Vec<QueuedTool> = Vec::new();
        let mut tool_results: Vec<ToolUseResult> = Vec::new();

        for tool_use in tool_uses {
            let tool_use_id = tool_use.id.clone();
            let tool_use_name = tool_use.name.clone();
            let mut tool_telemetry =
                ToolUseEventBuilder::new(conv_id.clone(), tool_use.id.clone(), self.conversation.model.clone())
                    .set_tool_use_id(tool_use_id.clone())
                    .set_tool_name(tool_use.name.clone())
                    .utterance_id(self.conversation.message_id().map(|s| s.to_string()));
            match self.conversation.tool_manager.get_tool_from_tool_use(tool_use) {
                Ok(mut tool) => {
                    // Apply non-Q-generated context to tools
                    self.contextualize_tool(&mut tool);

                    match tool.validate(os).await {
                        Ok(()) => {
                            tool_telemetry.is_valid = Some(true);
                            queued_tools.push(QueuedTool {
                                id: tool_use_id.clone(),
                                name: tool_use_name,
                                tool,
                                accepted: false,
                            });
                        },
                        Err(err) => {
                            tool_telemetry.is_valid = Some(false);
                            tool_results.push(ToolUseResult {
                                tool_use_id: tool_use_id.clone(),
                                content: vec![ToolUseResultBlock::Text(format!(
                                    "Failed to validate tool parameters: {err}"
                                ))],
                                status: ToolResultStatus::Error,
                            });
                        },
                    };
                },
                Err(err) => {
                    tool_telemetry.is_valid = Some(false);
                    tool_results.push(err.into());
                },
            }
            self.tool_use_telemetry_events.insert(tool_use_id, tool_telemetry);
        }

        // If we have any validation errors, then return them immediately to the model.
        if !tool_results.is_empty() {
            debug!(?tool_results, "Error found in the model tools");
            queue!(
                self.stderr,
                style::SetAttribute(Attribute::Bold),
                style::Print("Tool validation failed: "),
                style::SetAttribute(Attribute::Reset),
            )?;
            for tool_result in &tool_results {
                for block in &tool_result.content {
                    let content: Option<Cow<'_, str>> = match block {
                        ToolUseResultBlock::Text(t) => Some(t.as_str().into()),
                        ToolUseResultBlock::Json(d) => serde_json::to_string(d)
                            .map_err(|err| error!(?err, "failed to serialize tool result content"))
                            .map(Into::into)
                            .ok(),
                    };
                    if let Some(content) = content {
                        queue!(
                            self.stderr,
                            style::Print("\n"),
                            style::SetForegroundColor(Color::Red),
                            style::Print(format!("{}\n", content)),
                            style::SetForegroundColor(Color::Reset),
                        )?;
                    }
                }
            }
            self.conversation.add_tool_results(tool_results);
            self.send_tool_use_telemetry(os).await;
            if let ToolUseStatus::Idle = self.tool_use_status {
                self.tool_use_status = ToolUseStatus::RetryInProgress(
                    self.conversation
                        .message_id()
                        .map_or("No utterance id found".to_string(), |v| v.to_string()),
                );
            }

            let response = os
                .client
                .send_message(
                    self.conversation
                        .as_sendable_conversation_state(os, &mut self.stderr, false)
                        .await?,
                )
                .await?;
            return Ok(ChatState::HandleResponseStream(response));
        }

        self.tool_uses = queued_tools;
        self.pending_tool_index = Some(0);
        Ok(ChatState::ExecuteTools)
    }

    /// Apply program context to tools that Q may not have.
    // We cannot attach this any other way because Tools are constructed by deserializing
    // output from Amazon Q.
    // TODO: Is there a better way?
    fn contextualize_tool(&self, tool: &mut Tool) {
        if let Tool::GhIssue(gh_issue) = tool {
            let allowed_tools = self
                .conversation
                .agents
                .get_active()
                .map(|a| a.allowed_tools.iter().cloned().collect::<Vec<_>>())
                .unwrap_or_default();
            gh_issue.set_context(GhIssueContext {
                // Ideally we avoid cloning, but this function is not called very often.
                // Using references with lifetimes requires a large refactor, and Arc<Mutex<T>>
                // seems like overkill and may incur some performance cost anyway.
                context_manager: self.conversation.context_manager.clone(),
                transcript: self.conversation.transcript.clone(),
                failed_request_ids: self.failed_request_ids.clone(),
                tool_permissions: allowed_tools,
            });
        }
    }

    async fn print_tool_description(&mut self, os: &Os, tool_index: usize, trusted: bool) -> Result<(), ChatError> {
        let tool_use = &self.tool_uses[tool_index];

        queue!(
            self.stdout,
            style::SetForegroundColor(Color::Magenta),
            style::Print(format!(
                "🛠️  Using tool: {}{}",
                tool_use.tool.display_name(),
                if trusted { " (trusted)".dark_green() } else { "".reset() }
            )),
            style::SetForegroundColor(Color::Reset)
        )?;
        if let Tool::Custom(ref tool) = tool_use.tool {
            queue!(
                self.stdout,
                style::SetForegroundColor(Color::Reset),
                style::Print(" from mcp server "),
                style::SetForegroundColor(Color::Magenta),
                style::Print(tool.client.get_server_name()),
                style::SetForegroundColor(Color::Reset),
            )?;
        }

        execute!(
            self.stdout,
            style::Print("\n"),
            style::Print(CONTINUATION_LINE),
            style::Print("\n"),
            style::Print(TOOL_BULLET)
        )?;

        tool_use
            .tool
            .queue_description(os, &mut self.stdout)
            .await
            .map_err(|e| ChatError::Custom(format!("failed to print tool, `{}`: {}", tool_use.name, e).into()))?;

        Ok(())
    }

    /// Helper function to read user input with a prompt and Ctrl+C handling
    fn read_user_input(&mut self, prompt: &str, exit_on_single_ctrl_c: bool) -> Option<String> {
        let mut ctrl_c = false;
        loop {
            match (self.input_source.read_line(Some(prompt)), ctrl_c) {
                (Ok(Some(line)), _) => {
                    if line.trim().is_empty() {
                        continue; // Reprompt if the input is empty
                    }
                    return Some(line);
                },
                (Ok(None), false) => {
                    if exit_on_single_ctrl_c {
                        return None;
                    }
                    execute!(
                        self.stderr,
                        style::Print(format!(
                            "\n(To exit the CLI, press Ctrl+C or Ctrl+D again or type {})\n\n",
                            "/quit".green()
                        ))
                    )
                    .unwrap_or_default();
                    ctrl_c = true;
                },
                (Ok(None), true) => return None, // Exit if Ctrl+C was pressed twice
                (Err(_), _) => return None,
            }
        }
    }

    /// Helper function to generate a prompt based on the current context
    fn generate_tool_trust_prompt(&mut self) -> String {
        let profile = self.conversation.current_profile().map(|s| s.to_string());
        let all_trusted = self.all_tools_trusted();
        prompt::generate_prompt(profile.as_deref(), all_trusted)
    }

    async fn send_tool_use_telemetry(&mut self, os: &Os) {
        for (_, mut event) in self.tool_use_telemetry_events.drain() {
            event.user_input_id = match self.tool_use_status {
                ToolUseStatus::Idle => self.conversation.message_id(),
                ToolUseStatus::RetryInProgress(ref id) => Some(id.as_str()),
            }
            .map(|v| v.to_string());

            os.telemetry.send_tool_use_suggested(event).ok();
        }
    }

    fn terminal_width(&self) -> usize {
        (self.terminal_width_provider)().unwrap_or(80)
    }

    fn all_tools_trusted(&self) -> bool {
        self.conversation.agents.trust_all_tools
    }

    /// Display character limit warnings based on current conversation size
    async fn display_char_warnings(&mut self, os: &Os) -> Result<(), ChatError> {
        let warning_level = self.conversation.get_token_warning_level(os).await?;

        match warning_level {
            TokenWarningLevel::Critical => {
                // Memory constraint warning with gentler wording
                execute!(
                    self.stderr,
                    style::SetForegroundColor(Color::Yellow),
                    style::SetAttribute(Attribute::Bold),
                    style::Print("\n⚠️ This conversation is getting lengthy.\n"),
                    style::SetAttribute(Attribute::Reset),
                    style::Print(
                        "To ensure continued smooth operation, please use /compact to summarize the conversation.\n\n"
                    ),
                    style::SetForegroundColor(Color::Reset)
                )?;
            },
            TokenWarningLevel::None => {
                // No warning needed
            },
        }

        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    async fn send_chat_telemetry(
        &self,
        os: &Os,
        request_id: Option<String>,
        result: TelemetryResult,
        reason: Option<String>,
        reason_desc: Option<String>,
        status_code: Option<u16>,
    ) {
        os.telemetry
            .send_chat_added_message(
                &os.database,
                self.conversation.conversation_id().to_owned(),
                self.conversation.message_id().map(|s| s.to_owned()),
                request_id,
                self.conversation.context_message_length(),
                result,
                reason,
                reason_desc,
                status_code,
                self.conversation.model.clone(),
            )
            .await
            .ok();
    }

    async fn send_error_telemetry(
        &self,
        os: &Os,
        reason: String,
        reason_desc: Option<String>,
        status_code: Option<u16>,
    ) {
        os.telemetry
            .send_response_error(
                &os.database,
                self.conversation.conversation_id().to_owned(),
                self.conversation.context_message_length(),
                TelemetryResult::Failed,
                Some(reason),
                reason_desc,
                status_code,
            )
            .await
            .ok();
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
            let content = serde_json::to_string_pretty(&agent).expect("Failed to serialize test agent to file");
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
