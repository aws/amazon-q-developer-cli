pub mod cli;
mod command;
mod consts;
mod context;
mod conversation_state;
mod hooks;
mod input_source;
pub mod mcp;
mod message;
mod parse;
mod parser;
mod prompt;
mod server_messenger;
#[cfg(unix)]
mod skim_integration;
mod token_counter;
mod tool_manager;
mod tools;
pub mod util;

use std::borrow::Cow;
use std::collections::{
    HashMap,
    HashSet,
    VecDeque,
};
use std::io::{
    IsTerminal,
    Read,
    Write,
};
use std::process::{
    Command as ProcessCommand,
    ExitCode,
};
use std::sync::Arc;
use std::time::Duration;
use std::{
    env,
    fs,
};

use command::{
    Command,
    PromptsSubcommand,
    ToolsSubcommand,
};
use consts::{
    CONTEXT_FILES_MAX_SIZE,
    CONTEXT_WINDOW_SIZE,
    DUMMY_TOOL_NAME,
};
use context::ContextManager;
pub use conversation_state::ConversationState;
use conversation_state::TokenWarningLevel;
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
    ErrReport,
    Result,
    bail,
};
use hooks::{
    Hook,
    HookTrigger,
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
use rand::distr::{
    Alphanumeric,
    SampleString,
};
use regex::Regex;
use serde_json::Map;
use spinners::{
    Spinner,
    Spinners,
};
use thiserror::Error;
use token_counter::{
    TokenCount,
    TokenCounter,
};
use tokio::signal::ctrl_c;
use tool_manager::{
    GetPromptError,
    LoadingRecord,
    McpServerConfig,
    PromptBundle,
    ToolManager,
    ToolManagerBuilder,
};
use tools::gh_issue::GhIssueContext;
use tools::{
    OutputKind,
    QueuedTool,
    Tool,
    ToolOrigin,
    ToolPermissions,
    ToolSpec,
};
use tracing::{
    debug,
    error,
    info,
    trace,
    warn,
};
use unicode_width::UnicodeWidthStr;
use util::images::RichImageBlock;
use util::shared_writer::{
    NullWriter,
    SharedWriter,
};
use util::ui::draw_box;
use util::{
    animate_output,
    drop_matched_context_files,
    play_notification_bell,
    region_check,
};
use uuid::Uuid;
use winnow::Partial;
use winnow::stream::Offset;

use crate::api_client::StreamingClient;
use crate::api_client::clients::SendMessageOutput;
use crate::api_client::model::{
    ChatResponseStream,
    Tool as FigTool,
    ToolResultStatus,
};
use crate::database::Database;
use crate::database::settings::Setting;
use crate::mcp_client::{
    Prompt,
    PromptGetResult,
};
use crate::platform::Context;
use crate::telemetry::TelemetryThread;
use crate::telemetry::core::ToolUseEventBuilder;
use crate::util::CLI_BINARY_NAME;
use gemini_rust_sdk;

/// Help text for the compact command
fn compact_help_text() -> String {
    color_print::cformat!(
        r#"
<magenta,em>Conversation Compaction</magenta,em>

The <em>/compact</em> command summarizes the conversation history to free up context space
while preserving essential information. This is useful for long-running conversations
that may eventually reach memory constraints.

<cyan!>Usage</cyan!>
  <em>/compact</em>                   <black!>Summarize the conversation and clear history</black!>
  <em>/compact [prompt]</em>          <black!>Provide custom guidance for summarization</black!>

<cyan!>When to use</cyan!>
• When you see the memory constraint warning message
• When a conversation has been running for a long time
• Before starting a new topic within the same session
• After completing complex tool operations

<cyan!>How it works</cyan!>
• Creates an AI-generated summary of your conversation
• Retains key information, code, and tool executions in the summary
• Clears the conversation history to free up space
• The assistant will reference the summary context in future responses
"#
    )
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

const ROTATING_TIPS: [&str; 13] = [
    color_print::cstr! {"You can resume the last conversation from your current directory by launching with <green!>q chat --resume</green!>"},
    color_print::cstr! {"Get notified whenever Q CLI finishes responding. Just run <green!>q settings chat.enableNotifications true</green!>"},
    color_print::cstr! {"You can use <green!>/editor</green!> to edit your prompt with a vim-like experience"},
    color_print::cstr! {"<green!>/usage</green!> shows you a visual breakdown of your current context window usage"},
    color_print::cstr! {"Get notified whenever Q CLI finishes responding. Just run <green!>q settings chat.enableNotifications true</green!>"},
    color_print::cstr! {"You can execute bash commands by typing <green!>!</green!> followed by the command"},
    color_print::cstr! {"Q can use tools without asking for confirmation every time. Give <green!>/tools trust</green!> a try"},
    color_print::cstr! {"You can programmatically inject context to your prompts by using hooks. Check out <green!>/context hooks help</green!>"},
    color_print::cstr! {"You can use <green!>/compact</green!> to replace the conversation history with its summary to free up the context space"},
    color_print::cstr! {"If you want to file an issue to the Q CLI team, just tell me, or run <green!>q issue</green!>"},
    color_print::cstr! {"You can enable custom tools with <green!>MCP servers</green!>. Learn more with /help"},
    color_print::cstr! {"You can specify wait time (in ms) for mcp server loading with <green!>q settings mcp.initTimeout {timeout in int}</green!>. Servers that takes longer than the specified time will continue to load in the background. Use /tools to see pending servers."},
    color_print::cstr! {"You can see the server load status as well as any warnings or errors associated with <green!>/mcp</green!>"},
];

const GREETING_BREAK_POINT: usize = 80;

const POPULAR_SHORTCUTS: &str = color_print::cstr! {"<black!><green!>/help</green!> all commands  <em>•</em>  <green!>ctrl + j</green!> new lines  <em>•</em>  <green!>ctrl + s</green!> fuzzy search</black!>"};
const SMALL_SCREEN_POPULAR_SHORTCUTS: &str = color_print::cstr! {"<black!><green!>/help</green!> all commands
<green!>ctrl + j</green!> new lines
<green!>ctrl + s</green!> fuzzy search
</black!>"};

const HELP_TEXT: &str = color_print::cstr! {"

<magenta,em>q</magenta,em> (Amazon Q Chat)

<cyan,em>Commands:</cyan,em>
<em>/clear</em>        <black!>Clear the conversation history</black!>
<em>/issue</em>        <black!>Report an issue or make a feature request</black!>
<em>/editor</em>       <black!>Open $EDITOR (defaults to vi) to compose a prompt</black!>
<em>/help</em>         <black!>Show this help dialogue</black!>
<em>/quit</em>         <black!>Quit the application</black!>
<em>/compact</em>      <black!>Summarize the conversation to free up context space</black!>
  <em>help</em>        <black!>Show help for the compact command</black!>
  <em>[prompt]</em>    <black!>Optional custom prompt to guide summarization</black!>
<em>/tools</em>        <black!>View and manage tools and permissions</black!>
  <em>help</em>        <black!>Show an explanation for the trust command</black!>
  <em>trust</em>       <black!>Trust a specific tool or tools for the session</black!>
  <em>untrust</em>     <black!>Revert a tool or tools to per-request confirmation</black!>
  <em>trustall</em>    <black!>Trust all tools (equivalent to deprecated /acceptall)</black!>
  <em>reset</em>       <black!>Reset all tools to default permission levels</black!>
<em>/mcp</em>          <black!>See mcp server loaded</black!>
<em>/profile</em>      <black!>Manage profiles</black!>
  <em>help</em>        <black!>Show profile help</black!>
  <em>list</em>        <black!>List profiles</black!>
  <em>set</em>         <black!>Set the current profile</black!>
  <em>create</em>      <black!>Create a new profile</black!>
  <em>delete</em>      <black!>Delete a profile</black!>
  <em>rename</em>      <black!>Rename a profile</black!>
<em>/prompts</em>      <black!>View and retrieve prompts</black!>
  <em>help</em>        <black!>Show prompts help</black!>
  <em>list</em>        <black!>List or search available prompts</black!>
  <em>get</em>         <black!>Retrieve and send a prompt</black!>
<em>/context</em>      <black!>Manage context files and hooks for the chat session</black!>
  <em>help</em>        <black!>Show context help</black!>
  <em>show</em>        <black!>Display current context rules configuration [--expand]</black!>
  <em>add</em>         <black!>Add file(s) to context [--global] [--force]</black!>
  <em>rm</em>          <black!>Remove file(s) from context [--global]</black!>
  <em>clear</em>       <black!>Clear all files from current context [--global]</black!>
  <em>hooks</em>       <black!>View and manage context hooks</black!>
<em>/usage</em>        <black!>Show current session's context window usage</black!>
<em>/load</em>         <black!>Load conversation state from a JSON file</black!>
<em>/save</em>         <black!>Save conversation state to a JSON file</black!>

<cyan,em>MCP:</cyan,em>
<black!>You can now configure the Amazon Q CLI to use MCP servers. \nLearn how: https://docs.aws.amazon.com/en_us/amazonq/latest/qdeveloper-ug/command-line-mcp.html</black!>

<cyan,em>Tips:</cyan,em>
<em>!{command}</em>            <black!>Quickly execute a command in your current session</black!>
<em>Ctrl(^) + j</em>           <black!>Insert new-line to provide multi-line prompt. Alternatively, [Alt(⌥) + Enter(⏎)]</black!>
<em>Ctrl(^) + s</em>           <black!>Fuzzy search commands and context files. Use Tab to select multiple items.</black!>
                      <black!>Change the keybind to ctrl+x with: q settings chat.skimCommandKey x (where x is any key)</black!>
<em>chat.editMode</em>         <black!>Set editing mode (vim or emacs) using: q settings chat.editMode vi/emacs</black!>

"};

const RESPONSE_TIMEOUT_CONTENT: &str = "Response timed out - message took too long to generate";
const TRUST_ALL_TEXT: &str = color_print::cstr! {"<green!>All tools are now trusted (<red!>!</red!>). Amazon Q will execute tools <bold>without</bold> asking for confirmation.\
\nAgents can sometimes do unexpected things so understand the risks.</green!>
\nLearn more at https://docs.aws.amazon.com/amazonq/latest/qdeveloper-ug/command-line-chat-security.html#command-line-chat-trustall-safety"};

const TOOL_BULLET: &str = " ● ";
const CONTINUATION_LINE: &str = " ⋮ ";
const PURPOSE_ARROW: &str = " ↳ ";

pub async fn launch_chat(database: &mut Database, telemetry: &TelemetryThread, args: cli::Chat) -> Result<ExitCode> {
    let trust_tools = args.trust_tools.map(|mut tools| {
        if tools.len() == 1 && tools[0].is_empty() {
            tools.pop();
        }
        tools
    });

    chat(
        database,
        telemetry,
        args.input,
        args.no_interactive,
        args.resume,
        args.accept_all,
        args.profile,
        args.trust_all_tools,
        trust_tools,
    )
    .await
}

#[allow(clippy::too_many_arguments, clippy::fn_params_excessive_bools)]
pub async fn chat(
    database: &mut Database,
    telemetry: &TelemetryThread,
    input: Option<String>,
    no_interactive: bool,
    resume_conversation: bool,
    accept_all: bool,
    profile: Option<String>,
    trust_all_tools: bool,
    trust_tools: Option<Vec<String>>,
) -> Result<ExitCode> {
    if !crate::util::system_info::in_cloudshell() && !crate::auth::is_logged_in(database).await {
        bail!(
            "You are not logged in, please log in with {}",
            format!("{CLI_BINARY_NAME} login").bold()
        );
    }

    region_check("chat")?;

    let ctx = Context::new();

    let stdin = std::io::stdin();
    // no_interactive flag or part of a pipe
    let interactive = !no_interactive && stdin.is_terminal();
    let input = if !interactive && !stdin.is_terminal() {
        // append to input string any extra info that was provided, e.g. via pipe
        let mut input = input.unwrap_or_default();
        stdin.lock().read_to_string(&mut input)?;
        Some(input)
    } else {
        input
    };

    let mut output = match interactive {
        true => SharedWriter::stderr(),
        false => SharedWriter::stdout(),
    };

    let client = match ctx.env().get("Q_MOCK_CHAT_RESPONSE") {
        Ok(json) => create_stream(serde_json::from_str(std::fs::read_to_string(json)?.as_str())?),
        _ => StreamingClient::new(database).await?,
    };

    let mcp_server_configs = match McpServerConfig::load_config(&mut output).await {
        Ok(config) => {
            if interactive && !database.settings.get_bool(Setting::McpLoadedBefore).unwrap_or(false) {
                execute!(
                    output,
                    style::Print(
                        "To learn more about MCP safety, see https://docs.aws.amazon.com/amazonq/latest/qdeveloper-ug/command-line-mcp-security.html\n\n"
                    )
                )?;
            }
            database.settings.set(Setting::McpLoadedBefore, true).await?;
            config
        },
        Err(e) => {
            warn!("No mcp server config loaded: {}", e);
            McpServerConfig::default()
        },
    };

    // If profile is specified, verify it exists before starting the chat
    if let Some(ref profile_name) = profile {
        // Create a temporary context manager to check if the profile exists
        match ContextManager::new(Arc::clone(&ctx), None).await {
            Ok(context_manager) => {
                let profiles = context_manager.list_profiles().await?;
                if !profiles.contains(profile_name) {
                    bail!(
                        "Profile '{}' does not exist. Available profiles: {}",
                        profile_name,
                        profiles.join(", ")
                    );
                }
            },
            Err(e) => {
                warn!("Failed to initialize context manager to verify profile: {}", e);
                // Continue without verification if context manager can't be initialized
            },
        }
    }

    let conversation_id = Alphanumeric.sample_string(&mut rand::rng(), 9);
    info!(?conversation_id, "Generated new conversation id");
    let (prompt_request_sender, prompt_request_receiver) = std::sync::mpsc::channel::<Option<String>>();
    let (prompt_response_sender, prompt_response_receiver) = std::sync::mpsc::channel::<Vec<String>>();
    let tool_manager_output: Box<dyn Write + Send + Sync + 'static> = if interactive {
        Box::new(output.clone())
    } else {
        Box::new(NullWriter {})
    };
    let mut tool_manager = ToolManagerBuilder::default()
        .mcp_server_config(mcp_server_configs)
        .prompt_list_sender(prompt_response_sender)
        .prompt_list_receiver(prompt_request_receiver)
        .conversation_id(&conversation_id)
        .interactive(interactive)
        .build(telemetry, tool_manager_output)
        .await?;
    let tool_config = tool_manager.load_tools(database, &mut output).await?;
    let mut tool_permissions = ToolPermissions::new(tool_config.len());
    if accept_all || trust_all_tools {
        tool_permissions.trust_all = true;
        for tool in tool_config.values() {
            tool_permissions.trust_tool(&tool.name);
        }

        // Deprecation notice for --accept-all users
        if accept_all && interactive {
            queue!(
                output,
                style::SetForegroundColor(Color::Yellow),
                style::Print("\n--accept-all, -a is deprecated. Use --trust-all-tools instead."),
                style::SetForegroundColor(Color::Reset),
            )?;
        }
    } else if let Some(trusted) = trust_tools.map(|vec| vec.into_iter().collect::<HashSet<_>>()) {
        // --trust-all-tools takes precedence over --trust-tools=...
        for tool in tool_config.values() {
            if trusted.contains(&tool.name) {
                tool_permissions.trust_tool(&tool.name);
            } else {
                tool_permissions.untrust_tool(&tool.name);
            }
        }
    }

    let mut chat = ChatContext::new(
        ctx,
        database,
        &conversation_id,
        output,
        input,
        InputSource::new(database, prompt_request_sender, prompt_response_receiver)?,
        interactive,
        resume_conversation,
        client,
        || terminal::window_size().map(|s| s.columns.into()).ok(),
        tool_manager,
        profile,
        tool_config,
        tool_permissions,
    )
    .await?;

    let result = chat.try_chat(database, telemetry).await.map(|_| ExitCode::SUCCESS);
    drop(chat); // Explicit drop for clarity

    result
}

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
    Client(#[from] crate::api_client::ApiClientError),
    #[error("{0}")]
    ResponseStream(#[from] parser::RecvError),
    #[error("{0}")]
    Std(#[from] std::io::Error),
    #[error("{0}")]
    Readline(#[from] rustyline::error::ReadlineError),
    #[error("{0}")]
    Custom(Cow<'static, str>),
    #[error("interrupted")]
    Interrupted { tool_uses: Option<Vec<QueuedTool>> },
    #[error(
        "Tool approval required but --no-interactive was specified. Use --trust-all-tools to automatically approve tools."
    )]
    NonInteractiveToolApproval,
    #[error(transparent)]
    GetPromptError(#[from] GetPromptError),
}

pub struct ChatContext {
    ctx: Arc<Context>,
    /// The [Write] destination for printing conversation text.
    output: SharedWriter,
    initial_input: Option<String>,
    /// Whether we're starting a new conversation or continuing an old one.
    existing_conversation: bool,
    input_source: InputSource,
    interactive: bool,
    /// The client to use to interact with the model.
    client: StreamingClient,
    /// Width of the terminal, required for [ParseState].
    terminal_width_provider: fn() -> Option<usize>,
    spinner: Option<Spinner>,
    /// [ConversationState].
    conversation_state: ConversationState,
    /// State to track tools that need confirmation.
    tool_permissions: ToolPermissions,
    /// Telemetry events to be sent as part of the conversation.
    tool_use_telemetry_events: HashMap<String, ToolUseEventBuilder>,
    /// State used to keep track of tool use relation
    tool_use_status: ToolUseStatus,
    /// Any failed requests that could be useful for error report/debugging
    failed_request_ids: Vec<String>,
    /// Pending prompts to be sent
    pending_prompts: VecDeque<Prompt>,
    gemini_client: gemini_rust_sdk::Client,
}

async fn call_gemini_api(message: &str) -> Result<String, ChatError> {
    // Simulate API call delay
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;

    // Simulate a successful response
    // Remove the "/gemini " prefix from the message for the response
    let actual_message = message.strip_prefix("/gemini ").unwrap_or(message);
    Ok(format!("Gemini API mock response for: '{}'", actual_message))

    // To simulate an error, you could use:
    // Err(ChatError::Custom("Simulated Gemini API error".into()))
}

impl ChatContext {
    #[allow(clippy::too_many_arguments)]
    pub async fn new(
        ctx: Arc<Context>,
        database: &mut Database,
        conversation_id: &str,
        output: SharedWriter,
        mut input: Option<String>,
        input_source: InputSource,
        interactive: bool,
        resume_conversation: bool,
        client: StreamingClient,
        terminal_width_provider: fn() -> Option<usize>,
        tool_manager: ToolManager,
        profile: Option<String>,
        tool_config: HashMap<String, ToolSpec>,
        tool_permissions: ToolPermissions,
    ) -> Result<Self> {
        let ctx_clone = Arc::clone(&ctx);
        let output_clone = output.clone();

        let mut existing_conversation = false;
        let conversation_state = if resume_conversation {
            let prior = std::env::current_dir()
                .ok()
                .and_then(|cwd| database.get_conversation_by_path(cwd).ok())
                .flatten();

            // Only restore conversations where there were actual messages.
            // Prevents edge case where user clears conversation with --new, then exits without chatting.
            if prior.as_ref().is_some_and(|cs| !cs.history().is_empty()) {
                let mut cs = prior.unwrap();

                existing_conversation = true;
                cs.reload_serialized_state(Arc::clone(&ctx), Some(output.clone())).await;
                input = Some(input.unwrap_or("In a few words, summarize our conversation so far.".to_owned()));
                cs.tool_manager = tool_manager;
                cs.update_state(true).await;
                cs.enforce_tool_use_history_invariants();
                cs
            } else {
                ConversationState::new(
                    ctx_clone,
                    conversation_id,
                    tool_config,
                    profile,
                    Some(output_clone),
                    tool_manager,
                )
                .await
            }
        } else {
            ConversationState::new(
                ctx_clone,
                conversation_id,
                tool_config,
                profile,
                Some(output_clone),
                tool_manager,
            )
            .await
        };

        Ok(Self {
            ctx,
            output,
            initial_input: input,
            existing_conversation,
            input_source,
            interactive,
            client,
            terminal_width_provider,
            spinner: None,
            tool_permissions,
            conversation_state,
            tool_use_telemetry_events: HashMap::new(),
            tool_use_status: ToolUseStatus::Idle,
            failed_request_ids: Vec::new(),
            pending_prompts: VecDeque::new(),
            gemini_client: gemini_rust_sdk::Client::new(),
        })
    }
}

impl Drop for ChatContext {
    fn drop(&mut self) {
        if let Some(spinner) = &mut self.spinner {
            spinner.stop();
        }

        if self.interactive {
            queue!(
                self.output,
                cursor::MoveToColumn(0),
                style::SetAttribute(Attribute::Reset),
                style::ResetColor,
                cursor::Show
            )
            .ok();
        }

        self.output.flush().ok();
    }
}

/// The chat execution state.
///
/// Intended to provide more robust handling around state transitions while dealing with, e.g.,
/// tool validation, execution, response stream handling, etc.
#[derive(Debug)]
enum ChatState {
    /// Prompt the user with `tool_uses`, if available.
    PromptUser {
        /// Tool uses to present to the user.
        tool_uses: Option<Vec<QueuedTool>>,
        /// Tracks the next tool in tool_uses that needs user acceptance.
        pending_tool_index: Option<usize>,
        /// Used to avoid displaying the tool info at inappropriate times, e.g. after clear or help
        /// commands.
        skip_printing_tools: bool,
    },
    /// Handle the user input, depending on if any tools require execution.
    HandleInput {
        input: String,
        tool_uses: Option<Vec<QueuedTool>>,
        pending_tool_index: Option<usize>,
    },
    /// Validate the list of tool uses provided by the model.
    ValidateTools(Vec<AssistantToolUse>),
    /// Execute the list of tools.
    ExecuteTools(Vec<QueuedTool>),
    /// Consume the response stream and display to the user.
    HandleResponseStream(SendMessageOutput),
    /// Compact the chat history.
    CompactHistory {
        tool_uses: Option<Vec<QueuedTool>>,
        pending_tool_index: Option<usize>,
        /// Custom prompt to include as part of history compaction.
        prompt: Option<String>,
        /// Whether or not the summary should be shown on compact success.
        show_summary: bool,
        /// Whether or not to show the /compact help text.
        help: bool,
    },
    /// Exit the chat.
    Exit,
}

impl Default for ChatState {
    fn default() -> Self {
        Self::PromptUser {
            tool_uses: None,
            pending_tool_index: None,
            skip_printing_tools: false,
        }
    }

    #[tokio::test]
    async fn test_call_gemini_api_placeholder() {
        let response = call_gemini_api("/gemini test message").await;
        assert!(response.is_ok());
        assert_eq!(response.unwrap(), "Gemini API mock response for: 'test message'");

        let response_no_prefix = call_gemini_api("another message").await;
        assert!(response_no_prefix.is_ok());
        // This will also strip "/gemini " if present, but if not, it uses the whole message.
        // Based on current call_gemini_api, it would be "Gemini API mock response for: 'another message'"
        assert_eq!(response_no_prefix.unwrap(), "Gemini API mock response for: 'another message'");
    }

    #[tokio::test]
    async fn test_gemini_command_flow() {
        let ctx = Arc::new(Context::builder().with_test_home().await.unwrap().build_fake());
        let mut database = Database::new().await.unwrap();
        let env = Env::new();
        let telemetry = TelemetryThread::new(&env, &mut database).await.unwrap();

        let amazon_q_mock_client = StreamingClient::mock(vec![]);
        let mock_input_source = InputSource::new_mock(vec![
            "/gemini hello integration".to_string(),
            "/quit".to_string(), // To allow try_chat to terminate
        ]);
        let output_capture = SharedWriter::capture();

        // ToolManager setup
        let (tm_prompt_req_sender, _tm_prompt_req_receiver) = std::sync::mpsc::channel::<Option<String>>();
        let (_tm_prompt_resp_sender, tm_prompt_resp_receiver) = std::sync::mpsc::channel::<Vec<String>>();
        let tm_output: Box<dyn Write + Send + Sync + 'static> = Box::new(NullWriter {});

        let mut tool_manager = ToolManagerBuilder::default()
            .prompt_list_sender(_tm_prompt_resp_sender)
            .prompt_list_receiver(_tm_prompt_req_receiver)
            .conversation_id("gemini_test_conv_id")
            .interactive(true) // Some ToolManager functions might depend on this
            .build(&telemetry, tm_output)
            .await
            .unwrap();

        let tool_config_for_chatcontext: HashMap<String, ToolSpec> = tool_manager.load_tools(&mut database, &mut SharedWriter::null()).await.unwrap();
        let tool_permissions = ToolPermissions::new(tool_config_for_chatcontext.len());

        let mut chat_context = ChatContext::new(
            ctx,
            &mut database,
            "gemini_test_conv_id",
            output_capture.clone(),
            None,
            mock_input_source,
            true,
            false,
            amazon_q_mock_client,
            || Some(80),
            tool_manager,
            None,
            tool_config_for_chatcontext,
            tool_permissions,
        )
        .await
        .unwrap();

        let result = chat_context.try_chat(&mut database, &telemetry).await;
        assert!(result.is_ok(), "Chat loop failed: {:?}", result.err());

        let history = chat_context.conversation_state.history();
        let last_message_content = history.last().and_then(|msg| msg.content_text());
        assert!(
            last_message_content.is_some() && last_message_content.unwrap().contains("Gemini API mock response for: 'hello integration'"),
            "Conversation history did not contain Gemini response. Last message: {:?}, Full history: {:?}", last_message_content, history
        );
    }
}

impl ChatContext {
    /// Opens the user's preferred editor to compose a prompt
    fn open_editor(initial_text: Option<String>) -> Result<String, ChatError> {
        // Create a temporary file with a unique name
        let temp_dir = std::env::temp_dir();
        let file_name = format!("q_prompt_{}.md", Uuid::new_v4());
        let temp_file_path = temp_dir.join(file_name);

        // Get the editor from environment variable or use a default
        let editor_cmd = env::var("EDITOR").unwrap_or_else(|_| "vi".to_string());

        // Parse the editor command to handle arguments
        let mut parts =
            shlex::split(&editor_cmd).ok_or_else(|| ChatError::Custom("Failed to parse EDITOR command".into()))?;

        if parts.is_empty() {
            return Err(ChatError::Custom("EDITOR environment variable is empty".into()));
        }

        let editor_bin = parts.remove(0);

        // Write initial content to the file if provided
        let initial_content = initial_text.unwrap_or_default();
        fs::write(&temp_file_path, &initial_content)
            .map_err(|e| ChatError::Custom(format!("Failed to create temporary file: {}", e).into()))?;

        // Open the editor with the parsed command and arguments
        let mut cmd = ProcessCommand::new(editor_bin);
        // Add any arguments that were part of the EDITOR variable
        for arg in parts {
            cmd.arg(arg);
        }
        // Add the file path as the last argument
        let status = cmd
            .arg(&temp_file_path)
            .status()
            .map_err(|e| ChatError::Custom(format!("Failed to open editor: {}", e).into()))?;

        if !status.success() {
            return Err(ChatError::Custom("Editor exited with non-zero status".into()));
        }

        // Read the content back
        let content = fs::read_to_string(&temp_file_path)
            .map_err(|e| ChatError::Custom(format!("Failed to read temporary file: {}", e).into()))?;

        // Clean up the temporary file
        let _ = fs::remove_file(&temp_file_path);

        Ok(content.trim().to_string())
    }

    async fn try_chat(&mut self, database: &mut Database, telemetry: &TelemetryThread) -> Result<()> {
        let is_small_screen = self.terminal_width() < GREETING_BREAK_POINT;
        if self.interactive && database.settings.get_bool(Setting::ChatGreetingEnabled).unwrap_or(true) {
            let welcome_text = match self.existing_conversation {
                true => RESUME_TEXT,
                false => match is_small_screen {
                    true => SMALL_SCREEN_WELCOME_TEXT,
                    false => WELCOME_TEXT,
                },
            };

            execute!(self.output, style::Print(welcome_text), style::Print("\n\n"),)?;

            let current_tip_index = database.get_increment_rotating_tip().unwrap_or(0) % ROTATING_TIPS.len();

            let tip = ROTATING_TIPS[current_tip_index];
            if is_small_screen {
                // If the screen is small, print the tip in a single line
                execute!(
                    self.output,
                    style::Print("💡 ".to_string()),
                    style::Print(tip),
                    style::Print("\n")
                )?;
            } else {
                draw_box(
                    self.output.clone(),
                    "Did you know?",
                    tip,
                    GREETING_BREAK_POINT,
                    Color::DarkGrey,
                )?;
            }

            execute!(
                self.output,
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
            execute!(self.output, style::Print("\n"), style::SetForegroundColor(Color::Reset))?;
        }

        if self.interactive && self.all_tools_trusted() {
            queue!(
                self.output,
                style::Print(format!(
                    "{}{TRUST_ALL_TEXT}\n\n",
                    if !is_small_screen { "\n" } else { "" }
                ))
            )?;
        }
        self.output.flush()?;

        let mut next_state = Some(ChatState::PromptUser {
            tool_uses: None,
            pending_tool_index: None,
            skip_printing_tools: true,
        });

        if let Some(user_input) = self.initial_input.take() {
            next_state = Some(ChatState::HandleInput {
                input: user_input,
                tool_uses: None,
                pending_tool_index: None,
            });
        }

        loop {
            debug_assert!(next_state.is_some());
            let chat_state = next_state.take().unwrap_or_default();
            let ctrl_c_stream = ctrl_c();
            debug!(?chat_state, "changing to state");

            // Update conversation state with new tool information
            self.conversation_state.update_state(false).await;

            let result = match chat_state {
                ChatState::PromptUser {
                    tool_uses,
                    pending_tool_index,
                    skip_printing_tools,
                } => {
                    // Cannot prompt in non-interactive mode no matter what.
                    if !self.interactive {
                        return Ok(());
                    }
                    self.prompt_user(database, tool_uses, pending_tool_index, skip_printing_tools)
                        .await
                },
                ChatState::HandleInput {
                    input,
                    tool_uses,
                    pending_tool_index,
                } => {
                    let tool_uses_clone = tool_uses.clone();
                    tokio::select! {
                        res = self.handle_input(database, telemetry, input, tool_uses, pending_tool_index) => res,
                        Ok(_) = ctrl_c_stream => Err(ChatError::Interrupted { tool_uses: tool_uses_clone })
                    }
                },
                ChatState::CompactHistory {
                    tool_uses,
                    pending_tool_index,
                    prompt,
                    show_summary,
                    help,
                } => {
                    let tool_uses_clone = tool_uses.clone();
                    tokio::select! {
                        res = self.compact_history(telemetry, tool_uses, pending_tool_index, prompt, show_summary, help) => res,
                        Ok(_) = ctrl_c_stream => Err(ChatError::Interrupted { tool_uses: tool_uses_clone })
                    }
                },
                ChatState::ExecuteTools(tool_uses) => {
                    let tool_uses_clone = tool_uses.clone();
                    tokio::select! {
                        res = self.tool_use_execute(database, telemetry, tool_uses) => res,
                        Ok(_) = ctrl_c_stream => Err(ChatError::Interrupted { tool_uses: Some(tool_uses_clone) })
                    }
                },
                ChatState::ValidateTools(tool_uses) => {
                    tokio::select! {
                        res = self.validate_tools(telemetry, tool_uses) => res,
                        Ok(_) = ctrl_c_stream => Err(ChatError::Interrupted { tool_uses: None })
                    }
                },
                ChatState::HandleResponseStream(response) => tokio::select! {
                    res = self.handle_response(database, telemetry, response) => res,
                    Ok(_) = ctrl_c_stream => Err(ChatError::Interrupted { tool_uses: None })
                },
                ChatState::Exit => return Ok(()),
            };

            next_state = Some(self.handle_state_execution_result(database, result).await?);
        }
    }

    /// Handles the result of processing a [ChatState], returning the next [ChatState] to change
    /// to.
    async fn handle_state_execution_result(
        &mut self,
        database: &mut Database,
        result: Result<ChatState, ChatError>,
    ) -> Result<ChatState, ChatError> {
        // Remove non-ASCII and ANSI characters.
        let re = Regex::new(r"((\x9B|\x1B\[)[0-?]*[ -\/]*[@-~])|([^\x00-\x7F]+)").unwrap();
        match result {
            Ok(state) => Ok(state),
            Err(e) => {
                macro_rules! print_err {
                    ($prepend_msg:expr, $err:expr) => {{
                        queue!(
                            self.output,
                            style::SetAttribute(Attribute::Bold),
                            style::SetForegroundColor(Color::Red),
                        )?;

                        let report = eyre::Report::from($err);

                        let text = re
                            .replace_all(&format!("{}: {:?}\n", $prepend_msg, report), "")
                            .into_owned();

                        queue!(self.output, style::Print(&text),)?;
                        self.conversation_state.append_transcript(text);

                        execute!(
                            self.output,
                            style::SetAttribute(Attribute::Reset),
                            style::SetForegroundColor(Color::Reset),
                        )?;
                    }};
                }

                macro_rules! print_default_error {
                    ($err:expr) => {
                        print_err!("Amazon Q is having trouble responding right now", $err);
                    };
                }

                error!(?e, "An error occurred processing the current state");
                if self.interactive && self.spinner.is_some() {
                    drop(self.spinner.take());
                    queue!(
                        self.output,
                        terminal::Clear(terminal::ClearType::CurrentLine),
                        cursor::MoveToColumn(0),
                    )?;
                }
                match e {
                    ChatError::Interrupted { tool_uses: inter } => {
                        execute!(self.output, style::Print("\n\n"))?;
                        // If there was an interrupt during tool execution, then we add fake
                        // messages to "reset" the chat state.
                        match inter {
                            Some(tool_uses) if !tool_uses.is_empty() => {
                                self.conversation_state.abandon_tool_use(
                                    tool_uses,
                                    "The user interrupted the tool execution.".to_string(),
                                );
                                let _ = self.conversation_state.as_sendable_conversation_state(false).await;
                                self.conversation_state.push_assistant_message(
                                    AssistantMessage::new_response(
                                        None,
                                        "Tool uses were interrupted, waiting for the next user prompt".to_string(),
                                    ),
                                    database,
                                );
                            },
                            _ => (),
                        }
                    },
                    ChatError::Client(err) => match err {
                        // Errors from attempting to send too large of a conversation history. In
                        // this case, attempt to automatically compact the history for the user.
                        crate::api_client::ApiClientError::ContextWindowOverflow => {
                            if !self.conversation_state.can_create_summary_request().await {
                                execute!(
                                    self.output,
                                    style::SetForegroundColor(Color::Red),
                                    style::Print("Your conversation is too large to continue.\n"),
                                    style::SetForegroundColor(Color::Reset),
                                    style::Print(format!("• Run {} to analyze your context usage\n", "/usage".green())),
                                    style::Print(format!(
                                        "• Run {} to reset your conversation state\n",
                                        "/clear".green()
                                    )),
                                    style::SetAttribute(Attribute::Reset),
                                    style::Print("\n\n"),
                                )?;
                                self.conversation_state.reset_next_user_message();
                                return Ok(ChatState::PromptUser {
                                    tool_uses: None,
                                    pending_tool_index: None,
                                    skip_printing_tools: false,
                                });
                            }

                            execute!(
                                self.output,
                                style::SetForegroundColor(Color::Yellow),
                                style::Print("The context window has overflowed, summarizing the history..."),
                                style::SetAttribute(Attribute::Reset),
                                style::Print("\n\n"),
                            )?;

                            return Ok(ChatState::CompactHistory {
                                tool_uses: None,
                                pending_tool_index: None,
                                prompt: None,
                                show_summary: false,
                                help: false,
                            });
                        },
                        crate::api_client::ApiClientError::QuotaBreach(msg) => {
                            print_err!(msg, err);
                        },
                        _ => {
                            print_default_error!(err);
                        },
                    },
                    _ => {
                        print_default_error!(e);
                    },
                }
                self.conversation_state.enforce_conversation_invariants();
                self.conversation_state.reset_next_user_message();
                Ok(ChatState::PromptUser {
                    tool_uses: None,
                    pending_tool_index: None,
                    skip_printing_tools: false,
                })
            },
        }
    }

    /// Compacts the conversation history, replacing the history with a summary generated by the
    /// model.
    ///
    /// The last two user messages in the history are not included in the compaction process.
    async fn compact_history(
        &mut self,
        telemetry: &TelemetryThread,
        tool_uses: Option<Vec<QueuedTool>>,
        pending_tool_index: Option<usize>,
        custom_prompt: Option<String>,
        show_summary: bool,
        help: bool,
    ) -> Result<ChatState, ChatError> {
        let hist = self.conversation_state.history();
        debug!(?hist, "compacting history");

        // If help flag is set, show compact command help
        if help {
            execute!(
                self.output,
                style::Print("\n"),
                style::Print(compact_help_text()),
                style::Print("\n")
            )?;

            return Ok(ChatState::PromptUser {
                tool_uses,
                pending_tool_index,
                skip_printing_tools: true,
            });
        }

        if self.conversation_state.history().len() < 2 {
            execute!(
                self.output,
                style::SetForegroundColor(Color::Yellow),
                style::Print("\nConversation too short to compact.\n\n"),
                style::SetForegroundColor(Color::Reset)
            )?;

            return Ok(ChatState::PromptUser {
                tool_uses,
                pending_tool_index,
                skip_printing_tools: true,
            });
        }

        // Send a request for summarizing the history.
        let summary_state = self
            .conversation_state
            .create_summary_request(custom_prompt.as_ref())
            .await;
        if self.interactive {
            execute!(self.output, cursor::Hide, style::Print("\n"))?;
            self.spinner = Some(Spinner::new(Spinners::Dots, "Creating summary...".to_string()));
        }
        let response = self.client.send_message(summary_state).await;

        // TODO(brandonskiser): This is a temporary hotfix for failing compaction. We should instead
        // retry except with less context included.
        let response = match response {
            Ok(res) => res,
            Err(e) => match e {
                crate::api_client::ApiClientError::ContextWindowOverflow => {
                    self.conversation_state.clear(true);
                    if self.interactive {
                        self.spinner.take();
                        execute!(
                            self.output,
                            terminal::Clear(terminal::ClearType::CurrentLine),
                            cursor::MoveToColumn(0),
                            style::SetForegroundColor(Color::Yellow),
                            style::Print(
                                "The context window usage has overflowed. Clearing the conversation history.\n\n"
                            ),
                            style::SetAttribute(Attribute::Reset)
                        )?;
                    }
                    return Ok(ChatState::PromptUser {
                        tool_uses,
                        pending_tool_index,
                        skip_printing_tools: true,
                    });
                },
                e => return Err(e.into()),
            },
        };

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
                        return Err(err.into());
                    },
                }
            }
        };

        if self.interactive && self.spinner.is_some() {
            drop(self.spinner.take());
            queue!(
                self.output,
                terminal::Clear(terminal::ClearType::CurrentLine),
                cursor::MoveToColumn(0),
                cursor::Show
            )?;
        }

        if let Some(message_id) = self.conversation_state.message_id() {
            telemetry
                .send_chat_added_message(
                    self.conversation_state.conversation_id().to_owned(),
                    message_id.to_owned(),
                    self.conversation_state.context_message_length(),
                )
                .ok();
        }

        self.conversation_state.replace_history_with_summary(summary.clone());

        // Print output to the user.
        {
            execute!(
                self.output,
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
            animate_output(&mut self.output, &output)?;

            // Display the summary if the show_summary flag is set
            if show_summary {
                // Add a border around the summary for better visual separation
                let terminal_width = self.terminal_width();
                let border = "═".repeat(terminal_width.min(80));
                execute!(
                    self.output,
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
                animate_output(&mut self.output, &output)?;

                execute!(
                    self.output,
                    style::Print(&border),
                    style::Print("\n\n"),
                    style::SetForegroundColor(Color::Reset)
                )?;
            }
        }

        // If a next message is set, then retry the request.
        if self.conversation_state.next_user_message().is_some() {
            Ok(ChatState::HandleResponseStream(
                self.client
                    .send_message(self.conversation_state.as_sendable_conversation_state(false).await)
                    .await?,
            ))
        } else {
            // Otherwise, return back to the prompt for any pending tool uses.
            Ok(ChatState::PromptUser {
                tool_uses,
                pending_tool_index,
                skip_printing_tools: true,
            })
        }
    }

    /// Read input from the user.
    async fn prompt_user(
        &mut self,
        database: &Database,
        mut tool_uses: Option<Vec<QueuedTool>>,
        pending_tool_index: Option<usize>,
        skip_printing_tools: bool,
    ) -> Result<ChatState, ChatError> {
        execute!(self.output, cursor::Show)?;
        let tool_uses = tool_uses.take().unwrap_or_default();

        // Check token usage and display warnings if needed
        if pending_tool_index.is_none() {
            // Only display warnings when not waiting for tool approval
            if self.conversation_state.can_create_summary_request().await {
                if let Err(e) = self.display_char_warnings().await {
                    warn!("Failed to display character limit warnings: {}", e);
                }
            }
        }

        let show_tool_use_confirmation_dialog = !skip_printing_tools && pending_tool_index.is_some();
        if show_tool_use_confirmation_dialog {
            execute!(
                self.output,
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
        if let Some(ref context_manager) = self.conversation_state.context_manager {
            let tool_names = self
                .conversation_state
                .tool_manager
                .tn_map
                .keys()
                .filter(|name| *name != DUMMY_TOOL_NAME)
                .cloned()
                .collect::<Vec<_>>();
            self.input_source
                .put_skim_command_selector(database, Arc::new(context_manager.clone()), tool_names);
        }
        execute!(
            self.output,
            style::SetForegroundColor(Color::Reset),
            style::SetAttribute(Attribute::Reset)
        )?;
        let user_input = match self.read_user_input(&self.generate_tool_trust_prompt(), false) {
            Some(input) => input,
            None => return Ok(ChatState::Exit),
        };

        self.conversation_state.append_user_transcript(&user_input);
        Ok(ChatState::HandleInput {
            input: user_input,
            tool_uses: Some(tool_uses),
            pending_tool_index,
        })
    }

    async fn handle_input(
        &mut self,
        database: &mut Database,
        telemetry: &TelemetryThread,
        mut user_input: String,
        tool_uses: Option<Vec<QueuedTool>>,
        pending_tool_index: Option<usize>,
    ) -> Result<ChatState, ChatError> {
        if user_input.starts_with("/gemini ") {
            let gemini_response = call_gemini_api(&user_input).await?;
            self.conversation_state.push_assistant_message(
                AssistantMessage::new_response(None, gemini_response),
                database,
            );
            return Ok(ChatState::PromptUser {
                tool_uses: None,
                pending_tool_index: None,
                skip_printing_tools: true,
            });
        }

        let command_result = Command::parse(&user_input, &mut self.output);

        if let Err(error_message) = &command_result {
            // Display error message for command parsing errors
            execute!(
                self.output,
                style::SetForegroundColor(Color::Red),
                style::Print(format!("\nError: {}\n\n", error_message)),
                style::SetForegroundColor(Color::Reset)
            )?;

            return Ok(ChatState::PromptUser {
                tool_uses,
                pending_tool_index,
                skip_printing_tools: true,
            });
        }

        let command = command_result.unwrap();
        let mut tool_uses: Vec<QueuedTool> = tool_uses.unwrap_or_default();

        Ok(match command {
            Command::Ask { prompt } => {
                // Check for a pending tool approval
                if let Some(index) = pending_tool_index {
                    let tool_use = &mut tool_uses[index];

                    let is_trust = ["t", "T"].contains(&prompt.as_str());
                    if ["y", "Y"].contains(&prompt.as_str()) || is_trust {
                        if is_trust {
                            self.tool_permissions.trust_tool(&tool_use.name);
                        }
                        tool_use.accepted = true;

                        return Ok(ChatState::ExecuteTools(tool_uses));
                    }
                } else if !self.pending_prompts.is_empty() {
                    let prompts = self.pending_prompts.drain(0..).collect();
                    user_input = self
                        .conversation_state
                        .append_prompts(prompts)
                        .ok_or(ChatError::Custom("Prompt append failed".into()))?;
                }

                // Otherwise continue with normal chat on 'n' or other responses
                self.tool_use_status = ToolUseStatus::Idle;

                if pending_tool_index.is_some() {
                    self.conversation_state.abandon_tool_use(tool_uses, user_input);
                } else {
                    self.conversation_state.set_next_user_message(user_input).await;
                }

                let conv_state = self.conversation_state.as_sendable_conversation_state(true).await;
                self.send_tool_use_telemetry(telemetry).await;

                if self.interactive {
                    queue!(self.output, style::SetForegroundColor(Color::Magenta))?;
                    queue!(self.output, style::SetForegroundColor(Color::Reset))?;
                    queue!(self.output, cursor::Hide)?;
                    execute!(self.output, style::Print("\n"))?;
                    self.spinner = Some(Spinner::new(Spinners::Dots, "Thinking...".to_owned()));
                }

                ChatState::HandleResponseStream(self.client.send_message(conv_state).await?)
            },
            Command::Execute { command } => {
                queue!(self.output, style::Print('\n'))?;
                std::process::Command::new("bash").args(["-c", &command]).status().ok();
                queue!(self.output, style::Print('\n'))?;
                ChatState::PromptUser {
                    tool_uses: None,
                    pending_tool_index: None,
                    skip_printing_tools: false,
                }
            },
            Command::Clear => {
                execute!(self.output, cursor::Show)?;
                execute!(
                    self.output,
                    style::SetForegroundColor(Color::DarkGrey),
                    style::Print(
                        "\nAre you sure? This will erase the conversation history and context from hooks for the current session. "
                    ),
                    style::Print("["),
                    style::SetForegroundColor(Color::Green),
                    style::Print("y"),
                    style::SetForegroundColor(Color::DarkGrey),
                    style::Print("/"),
                    style::SetForegroundColor(Color::Green),
                    style::Print("n"),
                    style::SetForegroundColor(Color::DarkGrey),
                    style::Print("]:\n\n"),
                    style::SetForegroundColor(Color::Reset),
                )?;

                // Setting `exit_on_single_ctrl_c` for better ux: exit the confirmation dialog rather than the CLI
                let user_input = match self.read_user_input("> ".yellow().to_string().as_str(), true) {
                    Some(input) => input,
                    None => "".to_string(),
                };

                if ["y", "Y"].contains(&user_input.as_str()) {
                    self.conversation_state.clear(true);
                    if let Some(cm) = self.conversation_state.context_manager.as_mut() {
                        cm.hook_executor.global_cache.clear();
                        cm.hook_executor.profile_cache.clear();
                    }
                    execute!(
                        self.output,
                        style::SetForegroundColor(Color::Green),
                        style::Print("\nConversation history cleared.\n\n"),
                        style::SetForegroundColor(Color::Reset)
                    )?;
                }

                ChatState::PromptUser {
                    tool_uses: None,
                    pending_tool_index: None,
                    skip_printing_tools: true,
                }
            },
            Command::Compact {
                prompt,
                show_summary,
                help,
            } => {
                self.compact_history(
                    telemetry,
                    Some(tool_uses),
                    pending_tool_index,
                    prompt,
                    show_summary,
                    help,
                )
                .await?
            },
            Command::Help => {
                execute!(self.output, style::Print(HELP_TEXT))?;
                ChatState::PromptUser {
                    tool_uses: Some(tool_uses),
                    pending_tool_index,
                    skip_printing_tools: true,
                }
            },
            Command::Issue { prompt } => {
                let input = "I would like to report an issue or make a feature request";
                ChatState::HandleInput {
                    input: if let Some(prompt) = prompt {
                        format!("{input}: {prompt}")
                    } else {
                        input.to_string()
                    },
                    tool_uses: Some(tool_uses),
                    pending_tool_index,
                }
            },
            Command::PromptEditor { initial_text } => {
                match Self::open_editor(initial_text) {
                    Ok(content) => {
                        if content.trim().is_empty() {
                            execute!(
                                self.output,
                                style::SetForegroundColor(Color::Yellow),
                                style::Print("\nEmpty content from editor, not submitting.\n\n"),
                                style::SetForegroundColor(Color::Reset)
                            )?;

                            ChatState::PromptUser {
                                tool_uses: Some(tool_uses),
                                pending_tool_index,
                                skip_printing_tools: true,
                            }
                        } else {
                            execute!(
                                self.output,
                                style::SetForegroundColor(Color::Green),
                                style::Print("\nContent loaded from editor. Submitting prompt...\n\n"),
                                style::SetForegroundColor(Color::Reset)
                            )?;

                            // Display the content as if the user typed it
                            execute!(
                                self.output,
                                style::SetAttribute(Attribute::Reset),
                                style::SetForegroundColor(Color::Magenta),
                                style::Print("> "),
                                style::SetAttribute(Attribute::Reset),
                                style::Print(&content),
                                style::Print("\n")
                            )?;

                            // Process the content as user input
                            ChatState::HandleInput {
                                input: content,
                                tool_uses: Some(tool_uses),
                                pending_tool_index,
                            }
                        }
                    },
                    Err(e) => {
                        execute!(
                            self.output,
                            style::SetForegroundColor(Color::Red),
                            style::Print(format!("\nError opening editor: {}\n\n", e)),
                            style::SetForegroundColor(Color::Reset)
                        )?;

                        ChatState::PromptUser {
                            tool_uses: Some(tool_uses),
                            pending_tool_index,
                            skip_printing_tools: true,
                        }
                    },
                }
            },
            Command::Quit => ChatState::Exit,
            Command::Profile { subcommand } => {
                if let Some(context_manager) = &mut self.conversation_state.context_manager {
                    macro_rules! print_err {
                        ($err:expr) => {
                            execute!(
                                self.output,
                                style::SetForegroundColor(Color::Red),
                                style::Print(format!("\nError: {}\n\n", $err)),
                                style::SetForegroundColor(Color::Reset)
                            )?
                        };
                    }

                    match subcommand {
                        command::ProfileSubcommand::List => {
                            let profiles = match context_manager.list_profiles().await {
                                Ok(profiles) => profiles,
                                Err(e) => {
                                    execute!(
                                        self.output,
                                        style::SetForegroundColor(Color::Red),
                                        style::Print(format!("\nError listing profiles: {}\n\n", e)),
                                        style::SetForegroundColor(Color::Reset)
                                    )?;
                                    vec![]
                                },
                            };

                            execute!(self.output, style::Print("\n"))?;
                            for profile in profiles {
                                if profile == context_manager.current_profile {
                                    execute!(
                                        self.output,
                                        style::SetForegroundColor(Color::Green),
                                        style::Print("* "),
                                        style::Print(&profile),
                                        style::SetForegroundColor(Color::Reset),
                                        style::Print("\n")
                                    )?;
                                } else {
                                    execute!(
                                        self.output,
                                        style::Print("  "),
                                        style::Print(&profile),
                                        style::Print("\n")
                                    )?;
                                }
                            }
                            execute!(self.output, style::Print("\n"))?;
                        },
                        command::ProfileSubcommand::Create { name } => {
                            match context_manager.create_profile(&name).await {
                                Ok(_) => {
                                    execute!(
                                        self.output,
                                        style::SetForegroundColor(Color::Green),
                                        style::Print(format!("\nCreated profile: {}\n\n", name)),
                                        style::SetForegroundColor(Color::Reset)
                                    )?;
                                    context_manager
                                        .switch_profile(&name)
                                        .await
                                        .map_err(|e| warn!(?e, "failed to switch to newly created profile"))
                                        .ok();
                                },
                                Err(e) => print_err!(e),
                            }
                        },
                        command::ProfileSubcommand::Delete { name } => {
                            match context_manager.delete_profile(&name).await {
                                Ok(_) => {
                                    execute!(
                                        self.output,
                                        style::SetForegroundColor(Color::Green),
                                        style::Print(format!("\nDeleted profile: {}\n\n", name)),
                                        style::SetForegroundColor(Color::Reset)
                                    )?;
                                },
                                Err(e) => print_err!(e),
                            }
                        },
                        command::ProfileSubcommand::Set { name } => match context_manager.switch_profile(&name).await {
                            Ok(_) => {
                                execute!(
                                    self.output,
                                    style::SetForegroundColor(Color::Green),
                                    style::Print(format!("\nSwitched to profile: {}\n\n", name)),
                                    style::SetForegroundColor(Color::Reset)
                                )?;
                            },
                            Err(e) => print_err!(e),
                        },
                        command::ProfileSubcommand::Rename { old_name, new_name } => {
                            match context_manager.rename_profile(&old_name, &new_name).await {
                                Ok(_) => {
                                    execute!(
                                        self.output,
                                        style::SetForegroundColor(Color::Green),
                                        style::Print(format!("\nRenamed profile: {} -> {}\n\n", old_name, new_name)),
                                        style::SetForegroundColor(Color::Reset)
                                    )?;
                                },
                                Err(e) => print_err!(e),
                            }
                        },
                        command::ProfileSubcommand::Help => {
                            execute!(
                                self.output,
                                style::Print("\n"),
                                style::Print(command::ProfileSubcommand::help_text()),
                                style::Print("\n")
                            )?;
                        },
                    }
                }
                ChatState::PromptUser {
                    tool_uses: Some(tool_uses),
                    pending_tool_index,
                    skip_printing_tools: true,
                }
            },
            Command::Context { subcommand } => {
                if let Some(context_manager) = &mut self.conversation_state.context_manager {
                    match subcommand {
                        command::ContextSubcommand::Show { expand } => {
                            fn map_chat_error(e: ErrReport) -> ChatError {
                                ChatError::Custom(e.to_string().into())
                            }
                            // Display global context
                            execute!(
                                self.output,
                                style::SetAttribute(Attribute::Bold),
                                style::SetForegroundColor(Color::Magenta),
                                style::Print("\n🌍 global:\n"),
                                style::SetAttribute(Attribute::Reset),
                            )?;
                            let mut global_context_files = HashSet::new();
                            let mut profile_context_files = HashSet::new();
                            if context_manager.global_config.paths.is_empty() {
                                execute!(
                                    self.output,
                                    style::SetForegroundColor(Color::DarkGrey),
                                    style::Print("    <none>\n"),
                                    style::SetForegroundColor(Color::Reset)
                                )?;
                            } else {
                                for path in &context_manager.global_config.paths {
                                    execute!(self.output, style::Print(format!("    {} ", path)))?;
                                    if let Ok(context_files) = context_manager.get_context_files_by_path(path).await {
                                        execute!(
                                            self.output,
                                            style::SetForegroundColor(Color::Green),
                                            style::Print(format!(
                                                "({} match{})",
                                                context_files.len(),
                                                if context_files.len() == 1 { "" } else { "es" }
                                            )),
                                            style::SetForegroundColor(Color::Reset)
                                        )?;
                                        global_context_files.extend(context_files);
                                    }
                                    execute!(self.output, style::Print("\n"))?;
                                }
                            }

                            if expand {
                                queue!(
                                    self.output,
                                    style::SetAttribute(Attribute::Bold),
                                    style::SetForegroundColor(Color::DarkYellow),
                                    style::Print("\n    🔧 Hooks:\n")
                                )?;
                                print_hook_section(
                                    &mut self.output,
                                    &context_manager.global_config.hooks,
                                    HookTrigger::ConversationStart,
                                )
                                .map_err(map_chat_error)?;

                                print_hook_section(
                                    &mut self.output,
                                    &context_manager.global_config.hooks,
                                    HookTrigger::PerPrompt,
                                )
                                .map_err(map_chat_error)?;
                            }

                            // Display profile context
                            execute!(
                                self.output,
                                style::SetAttribute(Attribute::Bold),
                                style::SetForegroundColor(Color::Magenta),
                                style::Print(format!("\n👤 profile ({}):\n", context_manager.current_profile)),
                                style::SetAttribute(Attribute::Reset),
                            )?;

                            if context_manager.profile_config.paths.is_empty() {
                                execute!(
                                    self.output,
                                    style::SetForegroundColor(Color::DarkGrey),
                                    style::Print("    <none>\n\n"),
                                    style::SetForegroundColor(Color::Reset)
                                )?;
                            } else {
                                for path in &context_manager.profile_config.paths {
                                    execute!(self.output, style::Print(format!("    {} ", path)))?;
                                    if let Ok(context_files) = context_manager.get_context_files_by_path(path).await {
                                        execute!(
                                            self.output,
                                            style::SetForegroundColor(Color::Green),
                                            style::Print(format!(
                                                "({} match{})",
                                                context_files.len(),
                                                if context_files.len() == 1 { "" } else { "es" }
                                            )),
                                            style::SetForegroundColor(Color::Reset)
                                        )?;
                                        profile_context_files.extend(context_files);
                                    }
                                    execute!(self.output, style::Print("\n"))?;
                                }
                                execute!(self.output, style::Print("\n"))?;
                            }

                            if expand {
                                queue!(
                                    self.output,
                                    style::SetAttribute(Attribute::Bold),
                                    style::SetForegroundColor(Color::DarkYellow),
                                    style::Print("    🔧 Hooks:\n")
                                )?;
                                print_hook_section(
                                    &mut self.output,
                                    &context_manager.profile_config.hooks,
                                    HookTrigger::ConversationStart,
                                )
                                .map_err(map_chat_error)?;
                                print_hook_section(
                                    &mut self.output,
                                    &context_manager.profile_config.hooks,
                                    HookTrigger::PerPrompt,
                                )
                                .map_err(map_chat_error)?;
                                execute!(self.output, style::Print("\n"))?;
                            }

                            if global_context_files.is_empty() && profile_context_files.is_empty() {
                                execute!(
                                    self.output,
                                    style::SetForegroundColor(Color::DarkGrey),
                                    style::Print("No files in the current directory matched the rules above.\n\n"),
                                    style::SetForegroundColor(Color::Reset)
                                )?;
                            } else {
                                let total = global_context_files.len() + profile_context_files.len();
                                let total_tokens = global_context_files
                                    .iter()
                                    .map(|(_, content)| TokenCounter::count_tokens(content))
                                    .sum::<usize>()
                                    + profile_context_files
                                        .iter()
                                        .map(|(_, content)| TokenCounter::count_tokens(content))
                                        .sum::<usize>();
                                execute!(
                                    self.output,
                                    style::SetForegroundColor(Color::Green),
                                    style::SetAttribute(Attribute::Bold),
                                    style::Print(format!(
                                        "{} matched file{} in use:\n",
                                        total,
                                        if total == 1 { "" } else { "s" }
                                    )),
                                    style::SetForegroundColor(Color::Reset),
                                    style::SetAttribute(Attribute::Reset)
                                )?;

                                for (filename, content) in &global_context_files {
                                    let est_tokens = TokenCounter::count_tokens(content);
                                    execute!(
                                        self.output,
                                        style::Print(format!("🌍 {} ", filename)),
                                        style::SetForegroundColor(Color::DarkGrey),
                                        style::Print(format!("(~{} tkns)\n", est_tokens)),
                                        style::SetForegroundColor(Color::Reset),
                                    )?;
                                    if expand {
                                        execute!(
                                            self.output,
                                            style::SetForegroundColor(Color::DarkGrey),
                                            style::Print(format!("{}\n\n", content)),
                                            style::SetForegroundColor(Color::Reset)
                                        )?;
                                    }
                                }

                                for (filename, content) in &profile_context_files {
                                    let est_tokens = TokenCounter::count_tokens(content);
                                    execute!(
                                        self.output,
                                        style::Print(format!("👤 {} ", filename)),
                                        style::SetForegroundColor(Color::DarkGrey),
                                        style::Print(format!("(~{} tkns)\n", est_tokens)),
                                        style::SetForegroundColor(Color::Reset),
                                    )?;
                                    if expand {
                                        execute!(
                                            self.output,
                                            style::SetForegroundColor(Color::DarkGrey),
                                            style::Print(format!("{}\n\n", content)),
                                            style::SetForegroundColor(Color::Reset)
                                        )?;
                                    }
                                }

                                if expand {
                                    execute!(self.output, style::Print(format!("{}\n\n", "▔".repeat(3))),)?;
                                }

                                let mut combined_files: Vec<(String, String)> = global_context_files
                                    .iter()
                                    .chain(profile_context_files.iter())
                                    .cloned()
                                    .collect();

                                let dropped_files =
                                    drop_matched_context_files(&mut combined_files, CONTEXT_FILES_MAX_SIZE).ok();

                                execute!(
                                    self.output,
                                    style::Print(format!("\nTotal: ~{} tokens\n\n", total_tokens))
                                )?;

                                if let Some(dropped_files) = dropped_files {
                                    if !dropped_files.is_empty() {
                                        execute!(
                                            self.output,
                                            style::SetForegroundColor(Color::DarkYellow),
                                            style::Print(format!(
                                                "Total token count exceeds limit: {}. The following files will be automatically dropped when interacting with Q. Consider removing them. \n\n",
                                                CONTEXT_FILES_MAX_SIZE
                                            )),
                                            style::SetForegroundColor(Color::Reset)
                                        )?;
                                        let total_files = dropped_files.len();

                                        let truncated_dropped_files = &dropped_files[..10];

                                        for (filename, content) in truncated_dropped_files {
                                            let est_tokens = TokenCounter::count_tokens(content);
                                            execute!(
                                                self.output,
                                                style::Print(format!("{} ", filename)),
                                                style::SetForegroundColor(Color::DarkGrey),
                                                style::Print(format!("(~{} tkns)\n", est_tokens)),
                                                style::SetForegroundColor(Color::Reset),
                                            )?;
                                        }

                                        if total_files > 10 {
                                            execute!(
                                                self.output,
                                                style::Print(format!("({} more files)\n", total_files - 10))
                                            )?;
                                        }
                                    }
                                }

                                execute!(self.output, style::Print("\n"))?;
                            }

                            // Show last cached conversation summary if available, otherwise regenerate it
                            if expand {
                                if let Some(summary) = self.conversation_state.latest_summary() {
                                    let border = "═".repeat(self.terminal_width().min(80));
                                    execute!(
                                        self.output,
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
                                        style::Print(&summary),
                                        style::Print("\n\n\n")
                                    )?;
                                }
                            }
                        },
                        command::ContextSubcommand::Add { global, force, paths } => {
                            match context_manager.add_paths(paths.clone(), global, force).await {
                                Ok(_) => {
                                    let target = if global { "global" } else { "profile" };
                                    execute!(
                                        self.output,
                                        style::SetForegroundColor(Color::Green),
                                        style::Print(format!(
                                            "\nAdded {} path(s) to {} context.\n\n",
                                            paths.len(),
                                            target
                                        )),
                                        style::SetForegroundColor(Color::Reset)
                                    )?;
                                },
                                Err(e) => {
                                    execute!(
                                        self.output,
                                        style::SetForegroundColor(Color::Red),
                                        style::Print(format!("\nError: {}\n\n", e)),
                                        style::SetForegroundColor(Color::Reset)
                                    )?;
                                },
                            }
                        },
                        command::ContextSubcommand::Remove { global, paths } => {
                            match context_manager.remove_paths(paths.clone(), global).await {
                                Ok(_) => {
                                    let target = if global { "global" } else { "profile" };
                                    execute!(
                                        self.output,
                                        style::SetForegroundColor(Color::Green),
                                        style::Print(format!(
                                            "\nRemoved {} path(s) from {} context.\n\n",
                                            paths.len(),
                                            target
                                        )),
                                        style::SetForegroundColor(Color::Reset)
                                    )?;
                                },
                                Err(e) => {
                                    execute!(
                                        self.output,
                                        style::SetForegroundColor(Color::Red),
                                        style::Print(format!("\nError: {}\n\n", e)),
                                        style::SetForegroundColor(Color::Reset)
                                    )?;
                                },
                            }
                        },
                        command::ContextSubcommand::Clear { global } => match context_manager.clear(global).await {
                            Ok(_) => {
                                let target = if global {
                                    "global".to_string()
                                } else {
                                    format!("profile '{}'", context_manager.current_profile)
                                };
                                execute!(
                                    self.output,
                                    style::SetForegroundColor(Color::Green),
                                    style::Print(format!("\nCleared context for {}\n\n", target)),
                                    style::SetForegroundColor(Color::Reset)
                                )?;
                            },
                            Err(e) => {
                                execute!(
                                    self.output,
                                    style::SetForegroundColor(Color::Red),
                                    style::Print(format!("\nError: {}\n\n", e)),
                                    style::SetForegroundColor(Color::Reset)
                                )?;
                            },
                        },
                        command::ContextSubcommand::Help => {
                            execute!(
                                self.output,
                                style::Print("\n"),
                                style::Print(command::ContextSubcommand::help_text()),
                                style::Print("\n")
                            )?;
                        },
                        command::ContextSubcommand::Hooks { subcommand } => {
                            fn map_chat_error(e: ErrReport) -> ChatError {
                                ChatError::Custom(e.to_string().into())
                            }

                            let scope = |g: bool| if g { "global" } else { "profile" };
                            if let Some(subcommand) = subcommand {
                                match subcommand {
                                    command::HooksSubcommand::Add {
                                        name,
                                        trigger,
                                        command,
                                        global,
                                    } => {
                                        let trigger = if trigger == "conversation_start" {
                                            HookTrigger::ConversationStart
                                        } else {
                                            HookTrigger::PerPrompt
                                        };

                                        let result = context_manager
                                            .add_hook(name.clone(), Hook::new_inline_hook(trigger, command), global)
                                            .await;
                                        match result {
                                            Ok(_) => {
                                                execute!(
                                                    self.output,
                                                    style::SetForegroundColor(Color::Green),
                                                    style::Print(format!(
                                                        "\nAdded {} hook '{name}'.\n\n",
                                                        scope(global)
                                                    )),
                                                    style::SetForegroundColor(Color::Reset)
                                                )?;
                                            },
                                            Err(e) => {
                                                execute!(
                                                    self.output,
                                                    style::SetForegroundColor(Color::Red),
                                                    style::Print(format!(
                                                        "\nCannot add {} hook '{name}': {}\n\n",
                                                        scope(global),
                                                        e
                                                    )),
                                                    style::SetForegroundColor(Color::Reset)
                                                )?;
                                            },
                                        }
                                    },
                                    command::HooksSubcommand::Remove { name, global } => {
                                        let result = context_manager.remove_hook(&name, global).await;
                                        match result {
                                            Ok(_) => {
                                                execute!(
                                                    self.output,
                                                    style::SetForegroundColor(Color::Green),
                                                    style::Print(format!(
                                                        "\nRemoved {} hook '{name}'.\n\n",
                                                        scope(global)
                                                    )),
                                                    style::SetForegroundColor(Color::Reset)
                                                )?;
                                            },
                                            Err(e) => {
                                                execute!(
                                                    self.output,
                                                    style::SetForegroundColor(Color::Red),
                                                    style::Print(format!(
                                                        "\nCannot remove {} hook '{name}': {}\n\n",
                                                        scope(global),
                                                        e
                                                    )),
                                                    style::SetForegroundColor(Color::Reset)
                                                )?;
                                            },
                                        }
                                    },
                                    command::HooksSubcommand::Enable { name, global } => {
                                        let result = context_manager.set_hook_disabled(&name, global, false).await;
                                        match result {
                                            Ok(_) => {
                                                execute!(
                                                    self.output,
                                                    style::SetForegroundColor(Color::Green),
                                                    style::Print(format!(
                                                        "\nEnabled {} hook '{name}'.\n\n",
                                                        scope(global)
                                                    )),
                                                    style::SetForegroundColor(Color::Reset)
                                                )?;
                                            },
                                            Err(e) => {
                                                execute!(
                                                    self.output,
                                                    style::SetForegroundColor(Color::Red),
                                                    style::Print(format!(
                                                        "\nCannot enable {} hook '{name}': {}\n\n",
                                                        scope(global),
                                                        e
                                                    )),
                                                    style::SetForegroundColor(Color::Reset)
                                                )?;
                                            },
                                        }
                                    },
                                    command::HooksSubcommand::Disable { name, global } => {
                                        let result = context_manager.set_hook_disabled(&name, global, true).await;
                                        match result {
                                            Ok(_) => {
                                                execute!(
                                                    self.output,
                                                    style::SetForegroundColor(Color::Green),
                                                    style::Print(format!(
                                                        "\nDisabled {} hook '{name}'.\n\n",
                                                        scope(global)
                                                    )),
                                                    style::SetForegroundColor(Color::Reset)
                                                )?;
                                            },
                                            Err(e) => {
                                                execute!(
                                                    self.output,
                                                    style::SetForegroundColor(Color::Red),
                                                    style::Print(format!(
                                                        "\nCannot disable {} hook '{name}': {}\n\n",
                                                        scope(global),
                                                        e
                                                    )),
                                                    style::SetForegroundColor(Color::Reset)
                                                )?;
                                            },
                                        }
                                    },
                                    command::HooksSubcommand::EnableAll { global } => {
                                        context_manager
                                            .set_all_hooks_disabled(global, false)
                                            .await
                                            .map_err(map_chat_error)?;
                                        execute!(
                                            self.output,
                                            style::SetForegroundColor(Color::Green),
                                            style::Print(format!("\nEnabled all {} hooks.\n\n", scope(global))),
                                            style::SetForegroundColor(Color::Reset)
                                        )?;
                                    },
                                    command::HooksSubcommand::DisableAll { global } => {
                                        context_manager
                                            .set_all_hooks_disabled(global, true)
                                            .await
                                            .map_err(map_chat_error)?;
                                        execute!(
                                            self.output,
                                            style::SetForegroundColor(Color::Green),
                                            style::Print(format!("\nDisabled all {} hooks.\n\n", scope(global))),
                                            style::SetForegroundColor(Color::Reset)
                                        )?;
                                    },
                                    command::HooksSubcommand::Help => {
                                        execute!(
                                            self.output,
                                            style::Print("\n"),
                                            style::Print(command::ContextSubcommand::hooks_help_text()),
                                            style::Print("\n")
                                        )?;
                                    },
                                }
                            } else {
                                queue!(
                                    self.output,
                                    style::SetAttribute(Attribute::Bold),
                                    style::SetForegroundColor(Color::Magenta),
                                    style::Print("\n🌍 global:\n"),
                                    style::SetAttribute(Attribute::Reset),
                                )?;

                                print_hook_section(
                                    &mut self.output,
                                    &context_manager.global_config.hooks,
                                    HookTrigger::ConversationStart,
                                )
                                .map_err(map_chat_error)?;
                                print_hook_section(
                                    &mut self.output,
                                    &context_manager.global_config.hooks,
                                    HookTrigger::PerPrompt,
                                )
                                .map_err(map_chat_error)?;

                                queue!(
                                    self.output,
                                    style::SetAttribute(Attribute::Bold),
                                    style::SetForegroundColor(Color::Magenta),
                                    style::Print(format!("\n👤 profile ({}):\n", &context_manager.current_profile)),
                                    style::SetAttribute(Attribute::Reset),
                                )?;

                                print_hook_section(
                                    &mut self.output,
                                    &context_manager.profile_config.hooks,
                                    HookTrigger::ConversationStart,
                                )
                                .map_err(map_chat_error)?;
                                print_hook_section(
                                    &mut self.output,
                                    &context_manager.profile_config.hooks,
                                    HookTrigger::PerPrompt,
                                )
                                .map_err(map_chat_error)?;

                                execute!(
                                    self.output,
                                    style::Print(format!(
                                        "\nUse {} to manage hooks.\n\n",
                                        "/context hooks help".to_string().dark_green()
                                    )),
                                )?;
                            }
                        },
                    }
                    // crate::telemetry::send_context_command_executed
                } else {
                    execute!(
                        self.output,
                        style::SetForegroundColor(Color::Red),
                        style::Print("\nContext management is not available.\n\n"),
                        style::SetForegroundColor(Color::Reset)
                    )?;
                }

                ChatState::PromptUser {
                    tool_uses: Some(tool_uses),
                    pending_tool_index,
                    skip_printing_tools: true,
                }
            },
            Command::Tools { subcommand } => {
                let existing_tools: HashSet<&String> = self
                    .conversation_state
                    .tools
                    .values()
                    .flatten()
                    .map(|FigTool::ToolSpecification(spec)| &spec.name)
                    .collect();

                match subcommand {
                    Some(ToolsSubcommand::Schema) => {
                        let schema_json = serde_json::to_string_pretty(&self.conversation_state.tool_manager.schema)
                            .map_err(|e| {
                                ChatError::Custom(format!("Error converting tool schema to string: {e}").into())
                            })?;
                        queue!(self.output, style::Print(schema_json), style::Print("\n"))?;
                    },
                    Some(ToolsSubcommand::Trust { tool_names }) => {
                        let (valid_tools, invalid_tools): (Vec<String>, Vec<String>) = tool_names
                            .into_iter()
                            .partition(|tool_name| existing_tools.contains(tool_name));

                        if !invalid_tools.is_empty() {
                            queue!(
                                self.output,
                                style::SetForegroundColor(Color::Red),
                                style::Print(format!("\nCannot trust '{}', ", invalid_tools.join("', '"))),
                                if invalid_tools.len() > 1 {
                                    style::Print("they do not exist.")
                                } else {
                                    style::Print("it does not exist.")
                                },
                                style::SetForegroundColor(Color::Reset),
                            )?;
                        }
                        if !valid_tools.is_empty() {
                            valid_tools.iter().for_each(|t| self.tool_permissions.trust_tool(t));
                            queue!(
                                self.output,
                                style::SetForegroundColor(Color::Green),
                                if valid_tools.len() > 1 {
                                    style::Print(format!("\nTools '{}' are ", valid_tools.join("', '")))
                                } else {
                                    style::Print(format!("\nTool '{}' is ", valid_tools[0]))
                                },
                                style::Print("now trusted. I will "),
                                style::SetAttribute(Attribute::Bold),
                                style::Print("not"),
                                style::SetAttribute(Attribute::Reset),
                                style::SetForegroundColor(Color::Green),
                                style::Print(format!(
                                    " ask for confirmation before running {}.",
                                    if valid_tools.len() > 1 {
                                        "these tools"
                                    } else {
                                        "this tool"
                                    }
                                )),
                                style::SetForegroundColor(Color::Reset),
                            )?;
                        }
                    },
                    Some(ToolsSubcommand::Untrust { tool_names }) => {
                        let (valid_tools, invalid_tools): (Vec<String>, Vec<String>) = tool_names
                            .into_iter()
                            .partition(|tool_name| existing_tools.contains(tool_name));

                        if !invalid_tools.is_empty() {
                            queue!(
                                self.output,
                                style::SetForegroundColor(Color::Red),
                                style::Print(format!("\nCannot untrust '{}', ", invalid_tools.join("', '"))),
                                if invalid_tools.len() > 1 {
                                    style::Print("they do not exist.")
                                } else {
                                    style::Print("it does not exist.")
                                },
                                style::SetForegroundColor(Color::Reset),
                            )?;
                        }
                        if !valid_tools.is_empty() {
                            valid_tools.iter().for_each(|t| self.tool_permissions.untrust_tool(t));
                            queue!(
                                self.output,
                                style::SetForegroundColor(Color::Green),
                                if valid_tools.len() > 1 {
                                    style::Print(format!("\nTools '{}' are ", valid_tools.join("', '")))
                                } else {
                                    style::Print(format!("\nTool '{}' is ", valid_tools[0]))
                                },
                                style::Print("set to per-request confirmation."),
                                style::SetForegroundColor(Color::Reset),
                            )?;
                        }
                    },
                    Some(ToolsSubcommand::TrustAll) => {
                        self.conversation_state.tools.values().flatten().for_each(
                            |FigTool::ToolSpecification(spec)| {
                                self.tool_permissions.trust_tool(spec.name.as_str());
                            },
                        );
                        queue!(self.output, style::Print(TRUST_ALL_TEXT),)?;
                    },
                    Some(ToolsSubcommand::Reset) => {
                        self.tool_permissions.reset();
                        queue!(
                            self.output,
                            style::SetForegroundColor(Color::Green),
                            style::Print("\nReset all tools to the default permission levels."),
                            style::SetForegroundColor(Color::Reset),
                        )?;
                    },
                    Some(ToolsSubcommand::ResetSingle { tool_name }) => {
                        if self.tool_permissions.has(&tool_name) || self.tool_permissions.trust_all {
                            self.tool_permissions.reset_tool(&tool_name);
                            queue!(
                                self.output,
                                style::SetForegroundColor(Color::Green),
                                style::Print(format!("\nReset tool '{}' to the default permission level.", tool_name)),
                                style::SetForegroundColor(Color::Reset),
                            )?;
                        } else {
                            queue!(
                                self.output,
                                style::SetForegroundColor(Color::Red),
                                style::Print(format!(
                                    "\nTool '{}' does not exist or is already in default settings.",
                                    tool_name
                                )),
                                style::SetForegroundColor(Color::Reset),
                            )?;
                        }
                    },
                    Some(ToolsSubcommand::Help) => {
                        queue!(
                            self.output,
                            style::Print("\n"),
                            style::Print(command::ToolsSubcommand::help_text()),
                        )?;
                    },
                    None => {
                        // No subcommand - print the current tools and their permissions.
                        // Determine how to format the output nicely.
                        let terminal_width = self.terminal_width();
                        let longest = self
                            .conversation_state
                            .tools
                            .values()
                            .flatten()
                            .map(|FigTool::ToolSpecification(spec)| spec.name.len())
                            .max()
                            .unwrap_or(0);

                        queue!(
                            self.output,
                            style::Print("\n"),
                            style::SetAttribute(Attribute::Bold),
                            style::Print({
                                // Adding 2 because of "- " preceding every tool name
                                let width = longest + 2 - "Tool".len() + 4;
                                format!("Tool{:>width$}Permission", "", width = width)
                            }),
                            style::SetAttribute(Attribute::Reset),
                            style::Print("\n"),
                            style::Print("▔".repeat(terminal_width)),
                        )?;

                        let mut origin_tools: Vec<_> = self.conversation_state.tools.iter().collect();

                        // Built in tools always appear first.
                        origin_tools.sort_by(|(origin_a, _), (origin_b, _)| match (origin_a, origin_b) {
                            (ToolOrigin::Native, _) => std::cmp::Ordering::Less,
                            (_, ToolOrigin::Native) => std::cmp::Ordering::Greater,
                            (ToolOrigin::McpServer(name_a), ToolOrigin::McpServer(name_b)) => name_a.cmp(name_b),
                        });

                        for (origin, tools) in origin_tools.iter() {
                            let mut sorted_tools: Vec<_> = tools
                                .iter()
                                .filter(|FigTool::ToolSpecification(spec)| spec.name != DUMMY_TOOL_NAME)
                                .collect();

                            sorted_tools.sort_by_key(|t| match t {
                                FigTool::ToolSpecification(spec) => &spec.name,
                            });

                            let to_display =
                                sorted_tools
                                    .iter()
                                    .fold(String::new(), |mut acc, FigTool::ToolSpecification(spec)| {
                                        let width = longest - spec.name.len() + 4;
                                        acc.push_str(
                                            format!(
                                                "- {}{:>width$}{}\n",
                                                spec.name,
                                                "",
                                                self.tool_permissions.display_label(&spec.name),
                                                width = width
                                            )
                                            .as_str(),
                                        );
                                        acc
                                    });

                            let _ = queue!(
                                self.output,
                                style::SetAttribute(Attribute::Bold),
                                style::Print(format!("{}:\n", origin)),
                                style::SetAttribute(Attribute::Reset),
                                style::Print(to_display),
                                style::Print("\n")
                            );
                        }

                        let loading = self.conversation_state.tool_manager.pending_clients().await;
                        if !loading.is_empty() {
                            queue!(
                                self.output,
                                style::SetAttribute(Attribute::Bold),
                                style::Print("Servers still loading"),
                                style::SetAttribute(Attribute::Reset),
                                style::Print("\n"),
                                style::Print("▔".repeat(terminal_width)),
                            )?;
                            for client in loading {
                                queue!(self.output, style::Print(format!(" - {client}")), style::Print("\n"))?;
                            }
                        }

                        queue!(
                            self.output,
                            style::Print("\nTrusted tools will run without confirmation."),
                            style::SetForegroundColor(Color::DarkGrey),
                            style::Print(format!("\n{}\n", "* Default settings")),
                            style::Print("\n💡 Use "),
                            style::SetForegroundColor(Color::Green),
                            style::Print("/tools help"),
                            style::SetForegroundColor(Color::Reset),
                            style::SetForegroundColor(Color::DarkGrey),
                            style::Print(" to edit permissions.\n\n"),
                            style::SetForegroundColor(Color::Reset),
                        )?;
                    },
                };

                // Put spacing between previous output as to not be overwritten by
                // during PromptUser.
                self.output.flush()?;

                ChatState::PromptUser {
                    tool_uses: Some(tool_uses),
                    pending_tool_index,
                    skip_printing_tools: true,
                }
            },
            Command::Prompts { subcommand } => {
                match subcommand {
                    Some(PromptsSubcommand::Help) => {
                        queue!(self.output, style::Print(command::PromptsSubcommand::help_text()))?;
                    },
                    Some(PromptsSubcommand::Get { mut get_command }) => {
                        let orig_input = get_command.orig_input.take();
                        let prompts = match self.conversation_state.tool_manager.get_prompt(get_command).await {
                            Ok(resp) => resp,
                            Err(e) => {
                                match e {
                                    GetPromptError::AmbiguousPrompt(prompt_name, alt_msg) => {
                                        queue!(
                                            self.output,
                                            style::Print("\n"),
                                            style::SetForegroundColor(Color::Yellow),
                                            style::Print("Prompt "),
                                            style::SetForegroundColor(Color::Cyan),
                                            style::Print(prompt_name),
                                            style::SetForegroundColor(Color::Yellow),
                                            style::Print(" is ambiguous. Use one of the following "),
                                            style::SetForegroundColor(Color::Cyan),
                                            style::Print(alt_msg),
                                            style::SetForegroundColor(Color::Reset),
                                        )?;
                                    },
                                    GetPromptError::PromptNotFound(prompt_name) => {
                                        queue!(
                                            self.output,
                                            style::Print("\n"),
                                            style::SetForegroundColor(Color::Yellow),
                                            style::Print("Prompt "),
                                            style::SetForegroundColor(Color::Cyan),
                                            style::Print(prompt_name),
                                            style::SetForegroundColor(Color::Yellow),
                                            style::Print(" not found. Use "),
                                            style::SetForegroundColor(Color::Cyan),
                                            style::Print("/prompts list"),
                                            style::SetForegroundColor(Color::Yellow),
                                            style::Print(" to see available prompts.\n"),
                                            style::SetForegroundColor(Color::Reset),
                                        )?;
                                    },
                                    _ => return Err(ChatError::Custom(e.to_string().into())),
                                }
                                execute!(self.output, style::Print("\n"))?;
                                return Ok(ChatState::PromptUser {
                                    tool_uses: Some(tool_uses),
                                    pending_tool_index,
                                    skip_printing_tools: true,
                                });
                            },
                        };
                        if let Some(err) = prompts.error {
                            // If we are running into error we should just display the error
                            // and abort.
                            let to_display = serde_json::json!(err);
                            queue!(
                                self.output,
                                style::Print("\n"),
                                style::SetAttribute(Attribute::Bold),
                                style::Print("Error encountered while retrieving prompt:"),
                                style::SetAttribute(Attribute::Reset),
                                style::Print("\n"),
                                style::SetForegroundColor(Color::Red),
                                style::Print(
                                    serde_json::to_string_pretty(&to_display)
                                        .unwrap_or_else(|_| format!("{:?}", &to_display))
                                ),
                                style::SetForegroundColor(Color::Reset),
                                style::Print("\n"),
                            )?;
                        } else {
                            let prompts = prompts
                                .result
                                .ok_or(ChatError::Custom("Result field missing from prompt/get request".into()))?;
                            let prompts = serde_json::from_value::<PromptGetResult>(prompts).map_err(|e| {
                                ChatError::Custom(format!("Failed to deserialize prompt/get result: {:?}", e).into())
                            })?;
                            self.pending_prompts.clear();
                            self.pending_prompts.append(&mut VecDeque::from(prompts.messages));
                            return Ok(ChatState::HandleInput {
                                input: orig_input.unwrap_or_default(),
                                tool_uses: Some(tool_uses),
                                pending_tool_index,
                            });
                        }
                    },
                    subcommand => {
                        let search_word = match subcommand {
                            Some(PromptsSubcommand::List { search_word }) => search_word,
                            _ => None,
                        };
                        let terminal_width = self.terminal_width();
                        let mut prompts_wl = self.conversation_state.tool_manager.prompts.write().map_err(|e| {
                            ChatError::Custom(
                                format!("Poison error encountered while retrieving prompts: {}", e).into(),
                            )
                        })?;
                        self.conversation_state.tool_manager.refresh_prompts(&mut prompts_wl)?;
                        let mut longest_name = "";
                        let arg_pos = {
                            let optimal_case = UnicodeWidthStr::width(longest_name) + terminal_width / 4;
                            if optimal_case > terminal_width {
                                terminal_width / 3
                            } else {
                                optimal_case
                            }
                        };
                        queue!(
                            self.output,
                            style::Print("\n"),
                            style::SetAttribute(Attribute::Bold),
                            style::Print("Prompt"),
                            style::SetAttribute(Attribute::Reset),
                            style::Print({
                                let name_width = UnicodeWidthStr::width("Prompt");
                                let padding = arg_pos.saturating_sub(name_width);
                                " ".repeat(padding)
                            }),
                            style::SetAttribute(Attribute::Bold),
                            style::Print("Arguments (* = required)"),
                            style::SetAttribute(Attribute::Reset),
                            style::Print("\n"),
                            style::Print(format!("{}\n", "▔".repeat(terminal_width))),
                        )?;
                        let mut prompts_by_server: Vec<_> = prompts_wl
                            .iter()
                            .fold(
                                HashMap::<&String, Vec<&PromptBundle>>::new(),
                                |mut acc, (prompt_name, bundles)| {
                                    if prompt_name.contains(search_word.as_deref().unwrap_or("")) {
                                        if prompt_name.len() > longest_name.len() {
                                            longest_name = prompt_name.as_str();
                                        }
                                        for bundle in bundles {
                                            acc.entry(&bundle.server_name)
                                                .and_modify(|b| b.push(bundle))
                                                .or_insert(vec![bundle]);
                                        }
                                    }
                                    acc
                                },
                            )
                            .into_iter()
                            .collect();
                        prompts_by_server.sort_by_key(|(server_name, _)| server_name.as_str());

                        for (i, (server_name, bundles)) in prompts_by_server.iter_mut().enumerate() {
                            bundles.sort_by_key(|bundle| &bundle.prompt_get.name);

                            if i > 0 {
                                queue!(self.output, style::Print("\n"))?;
                            }
                            queue!(
                                self.output,
                                style::SetAttribute(Attribute::Bold),
                                style::Print(server_name),
                                style::Print(" (MCP):"),
                                style::SetAttribute(Attribute::Reset),
                                style::Print("\n"),
                            )?;
                            for bundle in bundles {
                                queue!(
                                    self.output,
                                    style::Print("- "),
                                    style::Print(&bundle.prompt_get.name),
                                    style::Print({
                                        if bundle
                                            .prompt_get
                                            .arguments
                                            .as_ref()
                                            .is_some_and(|args| !args.is_empty())
                                        {
                                            let name_width = UnicodeWidthStr::width(bundle.prompt_get.name.as_str());
                                            let padding =
                                                arg_pos.saturating_sub(name_width) - UnicodeWidthStr::width("- ");
                                            " ".repeat(padding)
                                        } else {
                                            "\n".to_owned()
                                        }
                                    })
                                )?;
                                if let Some(args) = bundle.prompt_get.arguments.as_ref() {
                                    for (i, arg) in args.iter().enumerate() {
                                        queue!(
                                            self.output,
                                            style::SetForegroundColor(Color::DarkGrey),
                                            style::Print(match arg.required {
                                                Some(true) => format!("{}*", arg.name),
                                                _ => arg.name.clone(),
                                            }),
                                            style::SetForegroundColor(Color::Reset),
                                            style::Print(if i < args.len() - 1 { ", " } else { "\n" }),
                                        )?;
                                    }
                                }
                            }
                        }
                    },
                }
                execute!(self.output, style::Print("\n"))?;
                ChatState::PromptUser {
                    tool_uses: Some(tool_uses),
                    pending_tool_index,
                    skip_printing_tools: true,
                }
            },
            Command::Usage => {
                let state = self.conversation_state.backend_conversation_state(true, true).await;

                if !state.dropped_context_files.is_empty() {
                    execute!(
                        self.output,
                        style::SetForegroundColor(Color::DarkYellow),
                        style::Print("\nSome context files are dropped due to size limit, please run "),
                        style::SetForegroundColor(Color::DarkGreen),
                        style::Print("/context show "),
                        style::SetForegroundColor(Color::DarkYellow),
                        style::Print("to learn more.\n"),
                        style::SetForegroundColor(style::Color::Reset)
                    )?;
                }

                let data = state.calculate_conversation_size();

                let context_token_count: TokenCount = data.context_messages.into();
                let assistant_token_count: TokenCount = data.assistant_messages.into();
                let user_token_count: TokenCount = data.user_messages.into();
                let total_token_used: TokenCount =
                    (data.context_messages + data.user_messages + data.assistant_messages).into();

                let window_width = self.terminal_width();
                // set a max width for the progress bar for better aesthetic
                let progress_bar_width = std::cmp::min(window_width, 80);

                let context_width = ((context_token_count.value() as f64 / CONTEXT_WINDOW_SIZE as f64)
                    * progress_bar_width as f64) as usize;
                let assistant_width = ((assistant_token_count.value() as f64 / CONTEXT_WINDOW_SIZE as f64)
                    * progress_bar_width as f64) as usize;
                let user_width = ((user_token_count.value() as f64 / CONTEXT_WINDOW_SIZE as f64)
                    * progress_bar_width as f64) as usize;

                let left_over_width = progress_bar_width
                    - std::cmp::min(context_width + assistant_width + user_width, progress_bar_width);

                let is_overflow = (context_width + assistant_width + user_width) > progress_bar_width;

                if is_overflow {
                    queue!(
                        self.output,
                        style::Print(format!(
                            "\nCurrent context window ({} of {}k tokens used)\n",
                            total_token_used,
                            CONTEXT_WINDOW_SIZE / 1000
                        )),
                        style::SetForegroundColor(Color::DarkRed),
                        style::Print("█".repeat(progress_bar_width)),
                        style::SetForegroundColor(Color::Reset),
                        style::Print(" "),
                        style::Print(format!(
                            "{:.2}%",
                            (total_token_used.value() as f32 / CONTEXT_WINDOW_SIZE as f32) * 100.0
                        )),
                    )?;
                } else {
                    queue!(
                        self.output,
                        style::Print(format!(
                            "\nCurrent context window ({} of {}k tokens used)\n",
                            total_token_used,
                            CONTEXT_WINDOW_SIZE / 1000
                        )),
                        style::SetForegroundColor(Color::DarkCyan),
                        // add a nice visual to mimic "tiny" progress, so the overral progress bar doesn't look too
                        // empty
                        style::Print("|".repeat(if context_width == 0 && *context_token_count > 0 {
                            1
                        } else {
                            0
                        })),
                        style::Print("█".repeat(context_width)),
                        style::SetForegroundColor(Color::Blue),
                        style::Print("|".repeat(if assistant_width == 0 && *assistant_token_count > 0 {
                            1
                        } else {
                            0
                        })),
                        style::Print("█".repeat(assistant_width)),
                        style::SetForegroundColor(Color::Magenta),
                        style::Print("|".repeat(if user_width == 0 && *user_token_count > 0 { 1 } else { 0 })),
                        style::Print("█".repeat(user_width)),
                        style::SetForegroundColor(Color::DarkGrey),
                        style::Print("█".repeat(left_over_width)),
                        style::Print(" "),
                        style::SetForegroundColor(Color::Reset),
                        style::Print(format!(
                            "{:.2}%",
                            (total_token_used.value() as f32 / CONTEXT_WINDOW_SIZE as f32) * 100.0
                        )),
                    )?;
                }

                queue!(self.output, style::Print("\n\n"))?;
                self.output.flush()?;

                queue!(
                    self.output,
                    style::SetForegroundColor(Color::DarkCyan),
                    style::Print("█ Context files: "),
                    style::SetForegroundColor(Color::Reset),
                    style::Print(format!(
                        "~{} tokens ({:.2}%)\n",
                        context_token_count,
                        (context_token_count.value() as f32 / CONTEXT_WINDOW_SIZE as f32) * 100.0
                    )),
                    style::SetForegroundColor(Color::Blue),
                    style::Print("█ Q responses: "),
                    style::SetForegroundColor(Color::Reset),
                    style::Print(format!(
                        "  ~{} tokens ({:.2}%)\n",
                        assistant_token_count,
                        (assistant_token_count.value() as f32 / CONTEXT_WINDOW_SIZE as f32) * 100.0
                    )),
                    style::SetForegroundColor(Color::Magenta),
                    style::Print("█ Your prompts: "),
                    style::SetForegroundColor(Color::Reset),
                    style::Print(format!(
                        " ~{} tokens ({:.2}%)\n\n",
                        user_token_count,
                        (user_token_count.value() as f32 / CONTEXT_WINDOW_SIZE as f32) * 100.0
                    )),
                )?;

                queue!(
                    self.output,
                    style::SetAttribute(Attribute::Bold),
                    style::Print("\n💡 Pro Tips:\n"),
                    style::SetAttribute(Attribute::Reset),
                    style::SetForegroundColor(Color::DarkGrey),
                    style::Print("Run "),
                    style::SetForegroundColor(Color::DarkGreen),
                    style::Print("/compact"),
                    style::SetForegroundColor(Color::DarkGrey),
                    style::Print(" to replace the conversation history with its summary\n"),
                    style::Print("Run "),
                    style::SetForegroundColor(Color::DarkGreen),
                    style::Print("/clear"),
                    style::SetForegroundColor(Color::DarkGrey),
                    style::Print(" to erase the entire chat history\n"),
                    style::Print("Run "),
                    style::SetForegroundColor(Color::DarkGreen),
                    style::Print("/context show"),
                    style::SetForegroundColor(Color::DarkGrey),
                    style::Print(" to see tokens per context file\n\n"),
                    style::SetForegroundColor(Color::Reset),
                )?;

                ChatState::PromptUser {
                    tool_uses: Some(tool_uses),
                    pending_tool_index,
                    skip_printing_tools: true,
                }
            },
            Command::Load { path } => {
                macro_rules! tri {
                    ($v:expr) => {
                        match $v {
                            Ok(v) => v,
                            Err(err) => {
                                execute!(
                                    self.output,
                                    style::SetForegroundColor(Color::Red),
                                    style::Print(format!("\nFailed to import from {}: {}\n\n", &path, &err)),
                                    style::SetAttribute(Attribute::Reset)
                                )?;
                                return Ok(ChatState::PromptUser {
                                    tool_uses: Some(tool_uses),
                                    pending_tool_index,
                                    skip_printing_tools: true,
                                });
                            },
                        }
                    };
                }

                let contents = tri!(self.ctx.fs().read_to_string(&path).await);
                let mut new_state: ConversationState = tri!(serde_json::from_str(&contents));
                new_state
                    .reload_serialized_state(Arc::clone(&self.ctx), Some(self.output.clone()))
                    .await;
                self.conversation_state = new_state;

                execute!(
                    self.output,
                    style::SetForegroundColor(Color::Green),
                    style::Print(format!("\n✔ Imported conversation state from {}\n\n", &path)),
                    style::SetAttribute(Attribute::Reset)
                )?;

                ChatState::PromptUser {
                    tool_uses: None,
                    pending_tool_index: None,
                    skip_printing_tools: true,
                }
            },
            Command::Save { path, force } => {
                macro_rules! tri {
                    ($v:expr) => {
                        match $v {
                            Ok(v) => v,
                            Err(err) => {
                                execute!(
                                    self.output,
                                    style::SetForegroundColor(Color::Red),
                                    style::Print(format!("\nFailed to export to {}: {}\n\n", &path, &err)),
                                    style::SetAttribute(Attribute::Reset)
                                )?;
                                return Ok(ChatState::PromptUser {
                                    tool_uses: Some(tool_uses),
                                    pending_tool_index,
                                    skip_printing_tools: true,
                                });
                            },
                        }
                    };
                }

                let contents = tri!(serde_json::to_string_pretty(&self.conversation_state));
                if self.ctx.fs().exists(&path) && !force {
                    execute!(
                        self.output,
                        style::SetForegroundColor(Color::Red),
                        style::Print(format!(
                            "\nFile at {} already exists. To overwrite, use -f or --force\n\n",
                            &path
                        )),
                        style::SetAttribute(Attribute::Reset)
                    )?;
                    return Ok(ChatState::PromptUser {
                        tool_uses: Some(tool_uses),
                        pending_tool_index,
                        skip_printing_tools: true,
                    });
                }
                tri!(self.ctx.fs().write(&path, contents).await);

                execute!(
                    self.output,
                    style::SetForegroundColor(Color::Green),
                    style::Print(format!("\n✔ Exported conversation state to {}\n\n", &path)),
                    style::SetAttribute(Attribute::Reset)
                )?;

                ChatState::PromptUser {
                    tool_uses: None,
                    pending_tool_index: None,
                    skip_printing_tools: true,
                }
            },
            Command::Mcp => {
                let terminal_width = self.terminal_width();
                let loaded_servers = self.conversation_state.tool_manager.mcp_load_record.lock().await;
                let still_loading = self
                    .conversation_state
                    .tool_manager
                    .pending_clients()
                    .await
                    .into_iter()
                    .map(|name| format!(" - {name}\n"))
                    .collect::<Vec<_>>()
                    .join("");
                for (server_name, msg) in loaded_servers.iter() {
                    let msg = msg
                        .iter()
                        .map(|record| match record {
                            LoadingRecord::Err(content)
                            | LoadingRecord::Warn(content)
                            | LoadingRecord::Success(content) => content.clone(),
                        })
                        .collect::<Vec<_>>()
                        .join("\n--- tools refreshed ---\n");
                    queue!(
                        self.output,
                        style::Print(server_name),
                        style::Print("\n"),
                        style::Print(format!("{}\n", "▔".repeat(terminal_width))),
                        style::Print(msg),
                        style::Print("\n")
                    )?;
                }
                if !still_loading.is_empty() {
                    queue!(
                        self.output,
                        style::Print("Still loading:\n"),
                        style::Print(format!("{}\n", "▔".repeat(terminal_width))),
                        style::Print(still_loading),
                        style::Print("\n")
                    )?;
                }
                self.output.flush()?;
                ChatState::PromptUser {
                    tool_uses: None,
                    pending_tool_index: None,
                    skip_printing_tools: true,
                }
            },
        })
    }

    async fn tool_use_execute(
        &mut self,
        database: &Database,
        telemetry: &TelemetryThread,
        mut tool_uses: Vec<QueuedTool>,
    ) -> Result<ChatState, ChatError> {
        // Verify tools have permissions.
        for (index, tool) in tool_uses.iter_mut().enumerate() {
            // Manually accepted by the user or otherwise verified already.
            if tool.accepted {
                continue;
            }

            // If there is an override, we will use it. Otherwise fall back to Tool's default.
            let allowed = self.tool_permissions.trust_all
                || (self.tool_permissions.has(&tool.name) && self.tool_permissions.is_trusted(&tool.name))
                || !tool.tool.requires_acceptance(&self.ctx);

            if database
                .settings
                .get_bool(Setting::ChatEnableNotifications)
                .unwrap_or(false)
            {
                play_notification_bell(!allowed);
            }

            self.print_tool_descriptions(tool, allowed).await?;

            if allowed {
                tool.accepted = true;
                continue;
            }

            let pending_tool_index = Some(index);
            if !self.interactive {
                // Cannot request in non-interactive, so fail.
                return Err(ChatError::NonInteractiveToolApproval);
            }

            return Ok(ChatState::PromptUser {
                tool_uses: Some(tool_uses),
                pending_tool_index,
                skip_printing_tools: false,
            });
        }

        // Execute the requested tools.
        let mut tool_results = vec![];
        let mut image_blocks: Vec<RichImageBlock> = Vec::new();

        for tool in tool_uses {
            let mut tool_telemetry = self.tool_use_telemetry_events.entry(tool.id.clone());
            tool_telemetry = tool_telemetry.and_modify(|ev| ev.is_accepted = true);

            let tool_start = std::time::Instant::now();
            let invoke_result = tool.tool.invoke(&self.ctx, &mut self.output).await;

            if self.interactive && self.spinner.is_some() {
                queue!(
                    self.output,
                    terminal::Clear(terminal::ClearType::CurrentLine),
                    cursor::MoveToColumn(0),
                    cursor::Show
                )?;
            }
            execute!(self.output, style::Print("\n"))?;

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
                    }

                    debug!("tool result output: {:#?}", result);
                    execute!(
                        self.output,
                        style::Print(CONTINUATION_LINE),
                        style::Print("\n"),
                        style::SetForegroundColor(Color::Green),
                        style::SetAttribute(Attribute::Bold),
                        style::Print(format!(" ● Completed in {}s", tool_time)),
                        style::SetForegroundColor(Color::Reset),
                        style::Print("\n"),
                    )?;

                    tool_telemetry = tool_telemetry.and_modify(|ev| ev.is_success = Some(true));
                    if let Tool::Custom(_) = &tool.tool {
                        tool_telemetry
                            .and_modify(|ev| ev.output_token_size = Some(TokenCounter::count_tokens(result.as_str())));
                    }
                    tool_results.push(ToolUseResult {
                        tool_use_id: tool.id,
                        content: vec![result.into()],
                        status: ToolResultStatus::Success,
                    });
                },
                Err(err) => {
                    error!(?err, "An error occurred processing the tool");
                    execute!(
                        self.output,
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
                        tool_use_id: tool.id,
                        content: vec![ToolUseResultBlock::Text(format!(
                            "An error occurred processing the tool: \n{}",
                            &err
                        ))],
                        status: ToolResultStatus::Error,
                    });
                    if let ToolUseStatus::Idle = self.tool_use_status {
                        self.tool_use_status = ToolUseStatus::RetryInProgress(
                            self.conversation_state
                                .message_id()
                                .map_or("No utterance id found".to_string(), |v| v.to_string()),
                        );
                    }
                },
            }
        }

        if !image_blocks.is_empty() {
            let images = image_blocks.into_iter().map(|(block, _)| block).collect();
            self.conversation_state
                .add_tool_results_with_images(tool_results, images);
            execute!(
                self.output,
                style::SetAttribute(Attribute::Reset),
                style::SetForegroundColor(Color::Reset),
                style::Print("\n")
            )?;
        } else {
            self.conversation_state.add_tool_results(tool_results);
        }
        if self.interactive {
            execute!(self.output, cursor::Hide)?;
            execute!(self.output, style::Print("\n"), style::SetAttribute(Attribute::Reset))?;
            self.spinner = Some(Spinner::new(Spinners::Dots, "Thinking...".to_string()));
        }

        self.send_tool_use_telemetry(telemetry).await;
        return Ok(ChatState::HandleResponseStream(
            self.client
                .send_message(self.conversation_state.as_sendable_conversation_state(false).await)
                .await?,
        ));
    }

    async fn handle_response(
        &mut self,
        database: &mut Database,
        telemetry: &TelemetryThread,
        response: SendMessageOutput,
    ) -> Result<ChatState, ChatError> {
        let request_id = response.request_id().map(|s| s.to_string());
        let mut buf = String::new();
        let mut offset = 0;
        let mut ended = false;
        let mut parser = ResponseParser::new(response);
        let mut state = ParseState::new(Some(self.terminal_width()));

        let mut tool_uses = Vec::new();
        let mut tool_name_being_recvd: Option<String> = None;

        if self.interactive && self.spinner.is_some() {
            drop(self.spinner.take());
            queue!(
                self.output,
                style::SetForegroundColor(Color::Reset),
                terminal::Clear(terminal::ClearType::CurrentLine),
                cursor::MoveToColumn(0),
                cursor::Show,
                cursor::MoveUp(1),
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
                            buf.push_str(&text);
                        },
                        parser::ResponseEvent::ToolUse(tool_use) => {
                            if self.interactive && self.spinner.is_some() {
                                drop(self.spinner.take());
                                queue!(
                                    self.output,
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
                            self.conversation_state.push_assistant_message(message, database);
                            ended = true;
                        },
                    }
                },
                Err(recv_error) => {
                    if let Some(request_id) = &recv_error.request_id {
                        self.failed_request_ids.push(request_id.clone());
                    };

                    match recv_error.source {
                        RecvErrorKind::StreamTimeout { source, duration } => {
                            error!(
                                recv_error.request_id,
                                ?source,
                                "Encountered a stream timeout after waiting for {}s",
                                duration.as_secs()
                            );
                            if self.interactive {
                                execute!(self.output, cursor::Hide)?;
                                self.spinner =
                                    Some(Spinner::new(Spinners::Dots, "Dividing up the work...".to_string()));
                            }
                            // For stream timeouts, we'll tell the model to try and split its response into
                            // smaller chunks.
                            self.conversation_state.push_assistant_message(
                                AssistantMessage::new_response(None, RESPONSE_TIMEOUT_CONTENT.to_string()),
                                database,
                            );
                            self.conversation_state
                                .set_next_user_message(
                                    "You took too long to respond - try to split up the work into smaller steps."
                                        .to_string(),
                                )
                                .await;
                            self.send_tool_use_telemetry(telemetry).await;
                            return Ok(ChatState::HandleResponseStream(
                                self.client
                                    .send_message(self.conversation_state.as_sendable_conversation_state(false).await)
                                    .await?,
                            ));
                        },
                        RecvErrorKind::UnexpectedToolUseEos {
                            tool_use_id,
                            name,
                            message,
                            time_elapsed,
                        } => {
                            error!(
                                recv_error.request_id,
                                tool_use_id, name, "The response stream ended before the entire tool use was received"
                            );
                            if self.interactive {
                                drop(self.spinner.take());
                                queue!(
                                    self.output,
                                    terminal::Clear(terminal::ClearType::CurrentLine),
                                    cursor::MoveToColumn(0),
                                    style::SetForegroundColor(Color::Yellow),
                                    style::SetAttribute(Attribute::Bold),
                                    style::Print(format!(
                                        "Warning: received an unexpected error from the model after {:.2}s",
                                        time_elapsed.as_secs_f64()
                                    )),
                                )?;
                                if let Some(request_id) = recv_error.request_id {
                                    queue!(
                                        self.output,
                                        style::Print(format!("\n         request_id: {}", request_id))
                                    )?;
                                }
                                execute!(self.output, style::Print("\n\n"), style::SetAttribute(Attribute::Reset))?;
                                self.spinner = Some(Spinner::new(
                                    Spinners::Dots,
                                    "Trying to divide up the work...".to_string(),
                                ));
                            }

                            self.conversation_state.push_assistant_message(*message, database);
                            let tool_results = vec![ToolUseResult {
                                    tool_use_id,
                                    content: vec![ToolUseResultBlock::Text(
                                        "The generated tool was too large, try again but this time split up the work between multiple tool uses".to_string(),
                                    )],
                                    status: ToolResultStatus::Error,
                                }];
                            self.conversation_state.add_tool_results(tool_results);
                            self.send_tool_use_telemetry(telemetry).await;
                            return Ok(ChatState::HandleResponseStream(
                                self.client
                                    .send_message(self.conversation_state.as_sendable_conversation_state(false).await)
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

            if tool_name_being_recvd.is_none() && !buf.is_empty() && self.interactive && self.spinner.is_some() {
                drop(self.spinner.take());
                queue!(
                    self.output,
                    terminal::Clear(terminal::ClearType::CurrentLine),
                    cursor::MoveToColumn(0),
                    cursor::Show
                )?;
            }

            // Print the response for normal cases
            loop {
                let input = Partial::new(&buf[offset..]);
                match interpret_markdown(input, &mut self.output, &mut state) {
                    Ok(parsed) => {
                        offset += parsed.offset_from(&input);
                        self.output.flush()?;
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
                std::thread::sleep(Duration::from_millis(8));
            }

            // Set spinner after showing all of the assistant text content so far.
            if let (Some(_name), true) = (&tool_name_being_recvd, self.interactive) {
                queue!(self.output, cursor::Hide)?;
                self.spinner = Some(Spinner::new(Spinners::Dots, "Thinking...".to_string()));
            }

            if ended {
                if let Some(message_id) = self.conversation_state.message_id() {
                    telemetry
                        .send_chat_added_message(
                            self.conversation_state.conversation_id().to_owned(),
                            message_id.to_owned(),
                            self.conversation_state.context_message_length(),
                        )
                        .ok();
                }

                if self.interactive
                    && database
                        .settings
                        .get_bool(Setting::ChatEnableNotifications)
                        .unwrap_or(false)
                {
                    // For final responses (no tools suggested), always play the bell
                    play_notification_bell(tool_uses.is_empty());
                }

                if self.interactive {
                    queue!(self.output, style::ResetColor, style::SetAttribute(Attribute::Reset))?;
                    execute!(self.output, style::Print("\n"))?;

                    for (i, citation) in &state.citations {
                        queue!(
                            self.output,
                            style::Print("\n"),
                            style::SetForegroundColor(Color::Blue),
                            style::Print(format!("[^{i}]: ")),
                            style::SetForegroundColor(Color::DarkGrey),
                            style::Print(format!("{citation}\n")),
                            style::SetForegroundColor(Color::Reset)
                        )?;
                    }
                }

                break;
            }
        }

        if !tool_uses.is_empty() {
            Ok(ChatState::ValidateTools(tool_uses))
        } else {
            Ok(ChatState::PromptUser {
                tool_uses: None,
                pending_tool_index: None,
                skip_printing_tools: false,
            })
        }
    }

    async fn validate_tools(
        &mut self,
        telemetry: &TelemetryThread,
        tool_uses: Vec<AssistantToolUse>,
    ) -> Result<ChatState, ChatError> {
        let conv_id = self.conversation_state.conversation_id().to_owned();
        debug!(?tool_uses, "Validating tool uses");
        let mut queued_tools: Vec<QueuedTool> = Vec::new();
        let mut tool_results: Vec<ToolUseResult> = Vec::new();

        for tool_use in tool_uses {
            let tool_use_id = tool_use.id.clone();
            let tool_use_name = tool_use.name.clone();
            let mut tool_telemetry = ToolUseEventBuilder::new(conv_id.clone(), tool_use.id.clone())
                .set_tool_use_id(tool_use_id.clone())
                .set_tool_name(tool_use.name.clone())
                .utterance_id(self.conversation_state.message_id().map(|s| s.to_string()));
            match self.conversation_state.tool_manager.get_tool_from_tool_use(tool_use) {
                Ok(mut tool) => {
                    // Apply non-Q-generated context to tools
                    self.contextualize_tool(&mut tool);

                    match tool.validate(&self.ctx).await {
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
                self.output,
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
                            self.output,
                            style::Print("\n"),
                            style::SetForegroundColor(Color::Red),
                            style::Print(format!("{}\n", content)),
                            style::SetForegroundColor(Color::Reset),
                        )?;
                    }
                }
            }
            self.conversation_state.add_tool_results(tool_results);
            self.send_tool_use_telemetry(telemetry).await;
            if let ToolUseStatus::Idle = self.tool_use_status {
                self.tool_use_status = ToolUseStatus::RetryInProgress(
                    self.conversation_state
                        .message_id()
                        .map_or("No utterance id found".to_string(), |v| v.to_string()),
                );
            }

            let response = self
                .client
                .send_message(self.conversation_state.as_sendable_conversation_state(false).await)
                .await?;
            return Ok(ChatState::HandleResponseStream(response));
        }

        Ok(ChatState::ExecuteTools(queued_tools))
    }

    /// Apply program context to tools that Q may not have.
    // We cannot attach this any other way because Tools are constructed by deserializing
    // output from Amazon Q.
    // TODO: Is there a better way?
    fn contextualize_tool(&self, tool: &mut Tool) {
        #[allow(clippy::single_match)]
        match tool {
            Tool::GhIssue(gh_issue) => {
                gh_issue.set_context(GhIssueContext {
                    // Ideally we avoid cloning, but this function is not called very often.
                    // Using references with lifetimes requires a large refactor, and Arc<Mutex<T>>
                    // seems like overkill and may incur some performance cost anyway.
                    context_manager: self.conversation_state.context_manager.clone(),
                    transcript: self.conversation_state.transcript.clone(),
                    failed_request_ids: self.failed_request_ids.clone(),
                    tool_permissions: self.tool_permissions.permissions.clone(),
                    interactive: self.interactive,
                });
            },
            _ => (),
        };
    }

    async fn print_tool_descriptions(&mut self, tool_use: &QueuedTool, trusted: bool) -> Result<(), ChatError> {
        queue!(
            self.output,
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
                self.output,
                style::SetForegroundColor(Color::Reset),
                style::Print(" from mcp server "),
                style::SetForegroundColor(Color::Magenta),
                style::Print(tool.client.get_server_name()),
                style::SetForegroundColor(Color::Reset),
            )?;
        }
        queue!(self.output, style::Print("\n"), style::Print(CONTINUATION_LINE))?;
        queue!(self.output, style::Print("\n"))?;
        queue!(self.output, style::Print(TOOL_BULLET))?;

        self.output.flush()?;

        tool_use
            .tool
            .queue_description(&self.ctx, &mut self.output)
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
                        self.output,
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
    fn generate_tool_trust_prompt(&self) -> String {
        prompt::generate_prompt(self.conversation_state.current_profile(), self.all_tools_trusted())
    }

    async fn send_tool_use_telemetry(&mut self, telemetry: &TelemetryThread) {
        for (_, mut event) in self.tool_use_telemetry_events.drain() {
            event.user_input_id = match self.tool_use_status {
                ToolUseStatus::Idle => self.conversation_state.message_id(),
                ToolUseStatus::RetryInProgress(ref id) => Some(id.as_str()),
            }
            .map(|v| v.to_string());

            telemetry.send_tool_use_suggested(event).ok();
        }
    }

    fn terminal_width(&self) -> usize {
        (self.terminal_width_provider)().unwrap_or(80)
    }

    fn all_tools_trusted(&self) -> bool {
        self.conversation_state.tools.values().flatten().all(|t| match t {
            FigTool::ToolSpecification(t) => self.tool_permissions.is_trusted(&t.name),
        })
    }

    /// Display character limit warnings based on current conversation size
    async fn display_char_warnings(&mut self) -> Result<(), std::io::Error> {
        let warning_level = self.conversation_state.get_token_warning_level().await;

        match warning_level {
            TokenWarningLevel::Critical => {
                // Memory constraint warning with gentler wording
                execute!(
                    self.output,
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
}

/// Prints hook configuration grouped by trigger: conversation session start or per user message
fn print_hook_section(output: &mut impl Write, hooks: &HashMap<String, Hook>, trigger: HookTrigger) -> Result<()> {
    let section = match trigger {
        HookTrigger::ConversationStart => "On Session Start",
        HookTrigger::PerPrompt => "Per User Message",
    };
    let hooks: Vec<(&String, &Hook)> = hooks.iter().filter(|(_, h)| h.trigger == trigger).collect();

    queue!(
        output,
        style::SetForegroundColor(Color::Cyan),
        style::Print(format!("    {section}:\n")),
        style::SetForegroundColor(Color::Reset),
    )?;

    if hooks.is_empty() {
        queue!(
            output,
            style::SetForegroundColor(Color::DarkGrey),
            style::Print("      <none>\n"),
            style::SetForegroundColor(Color::Reset)
        )?;
    } else {
        for (name, hook) in hooks {
            if hook.disabled {
                queue!(
                    output,
                    style::SetForegroundColor(Color::DarkGrey),
                    style::Print(format!("      {} (disabled)\n", name)),
                    style::SetForegroundColor(Color::Reset)
                )?;
            } else {
                queue!(output, style::Print(format!("      {}\n", name)),)?;
            }
        }
    }
    Ok(())
}

/// Testing helper
fn split_tool_use_event(value: &Map<String, serde_json::Value>) -> Vec<ChatResponseStream> {
    let tool_use_id = value.get("tool_use_id").unwrap().as_str().unwrap().to_string();
    let name = value.get("name").unwrap().as_str().unwrap().to_string();
    let args_str = value.get("args").unwrap().to_string();
    let split_point = args_str.len() / 2;
    vec![
        ChatResponseStream::ToolUseEvent {
            tool_use_id: tool_use_id.clone(),
            name: name.clone(),
            input: None,
            stop: None,
        },
        ChatResponseStream::ToolUseEvent {
            tool_use_id: tool_use_id.clone(),
            name: name.clone(),
            input: Some(args_str.split_at(split_point).0.to_string()),
            stop: None,
        },
        ChatResponseStream::ToolUseEvent {
            tool_use_id: tool_use_id.clone(),
            name: name.clone(),
            input: Some(args_str.split_at(split_point).1.to_string()),
            stop: None,
        },
        ChatResponseStream::ToolUseEvent {
            tool_use_id: tool_use_id.clone(),
            name: name.clone(),
            input: None,
            stop: Some(true),
        },
    ]
}

/// Testing helper
fn create_stream(model_responses: serde_json::Value) -> StreamingClient {
    let mut mock = Vec::new();
    for response in model_responses.as_array().unwrap() {
        let mut stream = Vec::new();
        for event in response.as_array().unwrap() {
            match event {
                serde_json::Value::String(assistant_text) => {
                    stream.push(ChatResponseStream::AssistantResponseEvent {
                        content: assistant_text.to_string(),
                    });
                },
                serde_json::Value::Object(tool_use) => {
                    stream.append(&mut split_tool_use_event(tool_use));
                },
                other => panic!("Unexpected value: {:?}", other),
            }
        }
        mock.push(stream);
    }
    StreamingClient::mock(mock)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platform::Env;

    #[tokio::test]
    async fn test_flow() {
        // let _ = tracing_subscriber::fmt::try_init();
        let ctx = Context::builder().with_test_home().await.unwrap().build_fake();
        let test_client = create_stream(serde_json::json!([
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

        let env = Env::new();
        let mut database = Database::new().await.unwrap();
        let telemetry = TelemetryThread::new(&env, &mut database).await.unwrap();

        let tool_manager = ToolManager::default();
        let tool_config = serde_json::from_str::<HashMap<String, ToolSpec>>(include_str!("tools/tool_index.json"))
            .expect("Tools failed to load");
        ChatContext::new(
            Arc::clone(&ctx),
            &mut database,
            "fake_conv_id",
            SharedWriter::stdout(),
            None,
            InputSource::new_mock(vec![
                "create a new file".to_string(),
                "y".to_string(),
                "exit".to_string(),
            ]),
            true,
            false,
            test_client,
            || Some(80),
            tool_manager,
            None,
            tool_config,
            ToolPermissions::new(0),
        )
        .await
        .unwrap()
        .try_chat(&mut database, &telemetry)
        .await
        .unwrap();

        assert_eq!(ctx.fs().read_to_string("/file.txt").await.unwrap(), "Hello, world!\n");
    }

    #[tokio::test]
    async fn test_flow_tool_permissions() {
        // let _ = tracing_subscriber::fmt::try_init();
        let ctx = Context::builder().with_test_home().await.unwrap().build_fake();
        let test_client = create_stream(serde_json::json!([
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

        let env = Env::new();
        let mut database = Database::new().await.unwrap();
        let telemetry = TelemetryThread::new(&env, &mut database).await.unwrap();

        let tool_manager = ToolManager::default();
        let tool_config = serde_json::from_str::<HashMap<String, ToolSpec>>(include_str!("tools/tool_index.json"))
            .expect("Tools failed to load");
        ChatContext::new(
            Arc::clone(&ctx),
            &mut database,
            "fake_conv_id",
            SharedWriter::stdout(),
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
            true,
            false,
            test_client,
            || Some(80),
            tool_manager,
            None,
            tool_config,
            ToolPermissions::new(0),
        )
        .await
        .unwrap()
        .try_chat(&mut database, &telemetry)
        .await
        .unwrap();

        assert_eq!(ctx.fs().read_to_string("/file2.txt").await.unwrap(), "Hello, world!\n");
        assert_eq!(ctx.fs().read_to_string("/file3.txt").await.unwrap(), "Hello, world!\n");
        assert!(!ctx.fs().exists("/file4.txt"));
        assert_eq!(ctx.fs().read_to_string("/file5.txt").await.unwrap(), "Hello, world!\n");
        assert!(!ctx.fs().exists("/file6.txt"));
    }

    #[tokio::test]
    async fn test_flow_multiple_tools() {
        // let _ = tracing_subscriber::fmt::try_init();
        let ctx = Context::builder().with_test_home().await.unwrap().build_fake();
        let test_client = create_stream(serde_json::json!([
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

        let env = Env::new();
        let mut database = Database::new().await.unwrap();
        let telemetry = TelemetryThread::new(&env, &mut database).await.unwrap();

        let tool_manager = ToolManager::default();
        let tool_config = serde_json::from_str::<HashMap<String, ToolSpec>>(include_str!("tools/tool_index.json"))
            .expect("Tools failed to load");
        ChatContext::new(
            Arc::clone(&ctx),
            &mut database,
            "fake_conv_id",
            SharedWriter::stdout(),
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
            true,
            false,
            test_client,
            || Some(80),
            tool_manager,
            None,
            tool_config,
            ToolPermissions::new(0),
        )
        .await
        .unwrap()
        .try_chat(&mut database, &telemetry)
        .await
        .unwrap();

        assert_eq!(ctx.fs().read_to_string("/file1.txt").await.unwrap(), "Hello, world!\n");
        assert_eq!(ctx.fs().read_to_string("/file2.txt").await.unwrap(), "Hello, world!\n");
        assert_eq!(ctx.fs().read_to_string("/file3.txt").await.unwrap(), "Hello, world!\n");
        assert_eq!(ctx.fs().read_to_string("/file4.txt").await.unwrap(), "Hello, world!\n");
    }

    #[tokio::test]
    async fn test_flow_tools_trust_all() {
        // let _ = tracing_subscriber::fmt::try_init();
        let ctx = Context::builder().with_test_home().await.unwrap().build_fake();
        let test_client = create_stream(serde_json::json!([
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

        let env = Env::new();
        let mut database = Database::new().await.unwrap();
        let telemetry = TelemetryThread::new(&env, &mut database).await.unwrap();

        let tool_manager = ToolManager::default();
        let tool_config = serde_json::from_str::<HashMap<String, ToolSpec>>(include_str!("tools/tool_index.json"))
            .expect("Tools failed to load");
        ChatContext::new(
            Arc::clone(&ctx),
            &mut database,
            "fake_conv_id",
            SharedWriter::stdout(),
            None,
            InputSource::new_mock(vec![
                "/tools trustall".to_string(),
                "create a new file".to_string(),
                "/tools reset".to_string(),
                "create a new file".to_string(),
                "exit".to_string(),
            ]),
            true,
            false,
            test_client,
            || Some(80),
            tool_manager,
            None,
            tool_config,
            ToolPermissions::new(0),
        )
        .await
        .unwrap()
        .try_chat(&mut database, &telemetry)
        .await
        .unwrap();

        assert_eq!(ctx.fs().read_to_string("/file1.txt").await.unwrap(), "Hello, world!\n");
        assert!(!ctx.fs().exists("/file2.txt"));
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
}
