use std::borrow::Cow;
use std::cell::RefCell;
use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::sync::{
    Arc,
    Mutex,
};

use eyre::Result;
use rustyline::completion::{
    Completer,
    FilenameCompleter,
};
use rustyline::error::ReadlineError;
use rustyline::highlight::{
    CmdKind,
    Highlighter,
};
use rustyline::hint::Hinter as RustylineHinter;
use rustyline::history::{
    FileHistory,
    SearchDirection,
};
use rustyline::validate::{
    ValidationContext,
    ValidationResult,
    Validator,
};
use rustyline::{
    Cmd,
    Completer,
    CompletionType,
    Config,
    Context,
    EditMode,
    Editor,
    EventHandler,
    Helper,
    Hinter,
    KeyCode,
    KeyEvent,
    Modifiers,
};

pub use super::prompt_parser::generate_prompt;
use super::prompt_parser::parse_prompt_components;
use super::tool_manager::{
    PromptQuery,
    PromptQueryResult,
};
use super::util::clipboard::{
    ClipboardError,
    paste_image_from_clipboard,
};
use crate::cli::chat::file_reference::REFERENCE_PREFIX;
use crate::cli::experiment::experiment_manager::{
    ExperimentManager,
    ExperimentName,
};
use crate::database::settings::Setting;
use crate::os::Os;
use crate::theme::colors::BRAND_PURPLE;

/// Shared state for clipboard paste operations triggered by Ctrl+V
#[derive(Clone, Debug)]
pub struct PasteState {
    inner: Arc<Mutex<PasteStateInner>>,
}

#[derive(Debug)]
struct PasteStateInner {
    paths: Vec<PathBuf>,
}

impl PasteState {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(PasteStateInner { paths: Vec::new() })),
        }
    }

    pub fn add(&self, path: PathBuf) -> usize {
        let mut inner = self.inner.lock().unwrap();
        inner.paths.push(path);
        inner.paths.len()
    }

    pub fn take_all(&self) -> Vec<PathBuf> {
        let mut inner = self.inner.lock().unwrap();
        std::mem::take(&mut inner.paths)
    }

    pub fn reset_count(&self) {
        let mut inner = self.inner.lock().unwrap();
        inner.paths.clear();
    }
}

pub const COMMANDS: &[&str] = &[
    "/clear",
    "/help",
    "/editor",
    "/reply",
    "/issue",
    "/quit",
    "/tools",
    "/tools trust",
    "/tools untrust",
    "/tools trust-all",
    "/tools reset",
    "/mcp",
    "/model",
    "/model set-current-as-default",
    "/experiment",
    "/agent",
    "/agent help",
    "/agent list",
    "/agent create",
    "/agent delete",
    "/agent rename",
    "/agent set",
    "/agent swap",
    "/agent schema",
    "/agent generate",
    "/chat",
    "/chat resume",
    "/chat save",
    "/chat load",
    "/chat save-via-script",
    "/chat load-via-script",
    "/prompts",
    "/context",
    "/context help",
    "/context show",
    "/context show --expand",
    "/context add",
    "/context rm",
    "/context clear",
    "/hooks",
    "/hooks help",
    "/hooks add",
    "/hooks rm",
    "/hooks enable",
    "/hooks disable",
    "/hooks enable-all",
    "/hooks disable-all",
    "/compact (optional: custom instruction for compaction)",
    "/compact --hide-summary",
    "/compact --help",
    "/usage",
    "/changelog",
    "/paste",
    "/save",
    "/load",
    "/paste",
    "/code",
    "/code status",
    "/code init",
    "/code logs",
    "/code overview",
    "/code summary",
];

/// Generate dynamic command list including experiment-based commands when enabled
pub fn get_available_commands(os: &Os) -> Vec<&'static str> {
    let mut commands = COMMANDS.to_vec();
    commands.extend(ExperimentManager::get_commands(os));
    commands.sort();
    commands
}

pub type PromptQuerySender = tokio::sync::broadcast::Sender<PromptQuery>;
pub type PromptQueryResponseReceiver = tokio::sync::broadcast::Receiver<PromptQueryResult>;

/// Complete commands that start with a slash
fn complete_command(commands: Vec<&'static str>, word: &str) -> Vec<String> {
    commands
        .iter()
        .filter(|p| p.starts_with(word))
        .map(|s| (*s).to_owned())
        .collect()
}

/// A wrapper around FilenameCompleter that provides enhanced path detection
/// and completion capabilities for the chat interface.
pub struct PathCompleter {
    /// The underlying filename completer from rustyline
    filename_completer: FilenameCompleter,
}

impl PathCompleter {
    /// Creates a new PathCompleter instance
    pub fn new() -> Self {
        Self {
            filename_completer: FilenameCompleter::new(),
        }
    }

    /// Attempts to complete a file path at the given position in the line
    pub fn complete_path(
        &self,
        line: &str,
        pos: usize,
        os: &Context<'_>,
    ) -> Result<(usize, Vec<String>), ReadlineError> {
        // Use the filename completer to get path completions
        match self.filename_completer.complete(line, pos, os) {
            Ok((pos, completions)) => {
                // Convert the filename completer's pairs to strings
                let file_completions: Vec<String> = completions.iter().map(|pair| pair.replacement.clone()).collect();

                // Return the completions if we have any
                Ok((pos, file_completions))
            },
            Err(err) => Err(err),
        }
    }
}

pub struct PromptCompleter {
    sender: PromptQuerySender,
    receiver: RefCell<PromptQueryResponseReceiver>,
}

impl PromptCompleter {
    fn new(sender: PromptQuerySender, receiver: PromptQueryResponseReceiver) -> Self {
        PromptCompleter {
            sender,
            receiver: RefCell::new(receiver),
        }
    }

    fn complete_prompt(&self, word: &str) -> Result<Vec<String>, ReadlineError> {
        let sender = &self.sender;
        let receiver = self.receiver.borrow_mut();
        let query = PromptQuery::Search(if !word.is_empty() { Some(word.to_string()) } else { None });

        sender
            .send(query)
            .map_err(|e| ReadlineError::Io(std::io::Error::other(e.to_string())))?;
        // We only want stuff from the current tail end onward
        let mut new_receiver = receiver.resubscribe();

        // Here we poll on the receiver for [max_attempts] number of times.
        // The reason for this is because we are trying to receive something managed by an async
        // channel from a sync context.
        // If we ever switch back to a single threaded runtime for whatever reason, this function
        // will not panic but nothing will be fetched because the thread that is doing
        // try_recv is also the thread that is supposed to be doing the sending.
        let mut attempts = 0;
        let max_attempts = 5;
        let query_res = loop {
            match new_receiver.try_recv() {
                Ok(result) => break result,
                Err(_e) if attempts < max_attempts - 1 => {
                    attempts += 1;
                    std::thread::sleep(std::time::Duration::from_millis(100));
                },
                Err(e) => {
                    return Err(ReadlineError::Io(std::io::Error::other(eyre::eyre!(
                        "Failed to receive prompt info from complete prompt after {} attempts: {:?}",
                        max_attempts,
                        e
                    ))));
                },
            }
        };
        let matches = match query_res {
            PromptQueryResult::Search(list) => list
                .into_iter()
                .map(|n| format!("{REFERENCE_PREFIX}{n}"))
                .collect::<Vec<_>>(),
            PromptQueryResult::List(_) => {
                return Err(ReadlineError::Io(std::io::Error::other(eyre::eyre!(
                    "Wrong query response type received",
                ))));
            },
            PromptQueryResult::Models(_) => {
                return Err(ReadlineError::Io(std::io::Error::other(eyre::eyre!(
                    "Wrong query response type received",
                ))));
            },
        };

        Ok(matches)
    }
}

pub struct ModelCompleter {
    sender: PromptQuerySender,
    receiver: RefCell<PromptQueryResponseReceiver>,
}

impl ModelCompleter {
    fn new(sender: PromptQuerySender, receiver: PromptQueryResponseReceiver) -> Self {
        ModelCompleter {
            sender,
            receiver: RefCell::new(receiver),
        }
    }

    fn complete_model(&self, prefix: &str) -> Result<Vec<String>, ReadlineError> {
        let sender = &self.sender;
        let receiver = self.receiver.borrow_mut();
        let query = PromptQuery::Models(if !prefix.is_empty() {
            Some(prefix.to_string())
        } else {
            None
        });

        sender
            .send(query)
            .map_err(|e| ReadlineError::Io(std::io::Error::other(e.to_string())))?;

        let mut new_receiver = receiver.resubscribe();

        let mut attempts = 0;
        let max_attempts = 5;
        let query_res = loop {
            match new_receiver.try_recv() {
                Ok(result) => break result,
                Err(_e) if attempts < max_attempts - 1 => {
                    attempts += 1;
                    std::thread::sleep(std::time::Duration::from_millis(100));
                },
                Err(e) => {
                    return Err(ReadlineError::Io(std::io::Error::other(eyre::eyre!(
                        "Failed to receive model info after {} attempts: {:?}",
                        max_attempts,
                        e
                    ))));
                },
            }
        };

        let matches = match query_res {
            PromptQueryResult::Models(list) => list.into_iter().map(|n| format!("/model {n}")).collect::<Vec<_>>(),
            _ => {
                return Err(ReadlineError::Io(std::io::Error::other(eyre::eyre!(
                    "Wrong query response type received",
                ))));
            },
        };

        Ok(matches)
    }
}

// Agent commands that support agent name completion
const AGENT_COMMANDS_WITH_NAME_COMPLETION: &[&str] = &["/agent swap ", "/agent delete "];

pub struct ChatCompleter {
    path_completer: PathCompleter,
    prompt_completer: PromptCompleter,
    model_completer: ModelCompleter,
    available_commands: Vec<&'static str>,
    agent_names: Vec<String>,
}

impl ChatCompleter {
    fn new(
        sender: PromptQuerySender,
        receiver: PromptQueryResponseReceiver,
        available_commands: Vec<&'static str>,
        agent_names: Vec<String>,
    ) -> Self {
        Self {
            path_completer: PathCompleter::new(),
            prompt_completer: PromptCompleter::new(sender.clone(), receiver.resubscribe()),
            model_completer: ModelCompleter::new(sender, receiver),
            available_commands,
            agent_names,
        }
    }

    /// Helper method to check if line matches an agent command pattern and return agent name part
    fn try_complete_agent_name(&self, line: &str, pos: usize) -> Option<(usize, Vec<String>)> {
        for prefix in AGENT_COMMANDS_WITH_NAME_COMPLETION {
            if line.starts_with(prefix) {
                let prefix_len = prefix.len();
                let agent_part = &line[prefix_len..pos];
                let mut candidates: Vec<String> = self
                    .agent_names
                    .iter()
                    .filter(|name| name.starts_with(agent_part))
                    .cloned()
                    .collect();
                candidates.sort();

                if !candidates.is_empty() {
                    return Some((prefix_len, candidates));
                }
            }
        }
        None
    }
}

impl Completer for ChatCompleter {
    type Candidate = String;

    fn complete(
        &self,
        line: &str,
        pos: usize,
        ctx: &Context<'_>,
    ) -> Result<(usize, Vec<Self::Candidate>), ReadlineError> {
        // Handle command completion - check if line starts with / for multi-word commands
        if line.starts_with('/') {
            let cmd_part = &line[..pos];

            // Handle /model <partial> - use dynamic model completion
            if let Some(model_prefix) = cmd_part.strip_prefix("/model ") {
                // Don't intercept subcommands like "set-current-as-default"
                if !model_prefix.starts_with("set-")
                    && let Ok(completions) = self.model_completer.complete_model(model_prefix)
                {
                    return Ok((0, completions));
                }
            }

            let candidates = complete_command(self.available_commands.clone(), cmd_part);
            if !candidates.is_empty() {
                return Ok((0, candidates));
            }
        }

        // Handle agent name completion for agent commands
        if let Some((start, candidates)) = self.try_complete_agent_name(line, pos) {
            return Ok((start, candidates));
        }

        // Handle @reference completion: prompts only at start, files anywhere
        if let Some(at_pos) = line.rfind(REFERENCE_PREFIX) {
            let search_word = &line[at_pos + 1..];
            let mut all_completions = Vec::new();

            let is_quoted = search_word.starts_with('"');
            let actual_search = if is_quoted { &search_word[1..] } else { search_word };

            let path_pos = actual_search.len();
            if let Ok((_, path_completions)) = self.path_completer.complete_path(actual_search, path_pos, ctx) {
                all_completions.extend(path_completions.into_iter().map(|p| {
                    if is_quoted || p.contains(' ') {
                        let unescaped = p.replace("\\ ", " ").replace("\\(", "(").replace("\\)", ")");
                        format!("{REFERENCE_PREFIX}\"{}\"", unescaped)
                    } else {
                        format!("{REFERENCE_PREFIX}{}", p)
                    }
                }));
            }

            if at_pos == 0
                && let Ok(prompt_completions) = self.prompt_completer.complete_prompt(search_word)
            {
                all_completions.extend(prompt_completions);
            }

            if !all_completions.is_empty() {
                all_completions.sort();
                all_completions.dedup();
                return Ok((at_pos, all_completions));
            }
        }

        // Handle file path completion as fallback
        if let Ok((pos, completions)) = self.path_completer.complete_path(line, pos, ctx)
            && !completions.is_empty()
        {
            return Ok((pos, completions));
        }

        // Default: no completions
        Ok((pos, Vec::new()))
    }
}

/// Custom hinter that provides shadowtext suggestions
pub struct ChatHinter {
    /// Whether history-based hints are enabled
    history_hints_enabled: bool,
    /// Whether prompt hints are enabled
    prompt_hints_enabled: bool,
    history_path: PathBuf,
    available_commands: Vec<&'static str>,
    /// Track if first hint has been shown
    first_hint_shown: std::sync::atomic::AtomicBool,
    agent_names: Vec<String>,
    model_query_sender: PromptQuerySender,
    model_query_receiver: RefCell<PromptQueryResponseReceiver>,
}

const INITIAL_PROMPT_HINTS: &[(&str, u32)] = &[
    ("How can I help?", 1),
    ("What should we work on?", 1),
    ("Ask me anything!", 1),
    ("What would you like to do?", 1),
    ("Ready when you are!", 1),
    ("Curious what I can do? Just ask!", 1),
    ("Not sure where to start? Ask me about my features", 1),
    ("Want to know what commands I have? Just ask", 1),
    ("Need help with features or setup? Use /help", 3),
    ("Use @file or @dir to include file contents inline", 1),
];

impl ChatHinter {
    /// Creates a new ChatHinter instance
    pub fn new(
        history_hints_enabled: bool,
        prompt_hints_enabled: bool,
        history_path: PathBuf,
        available_commands: Vec<&'static str>,
        agent_names: Vec<String>,
        sender: PromptQuerySender,
        receiver: PromptQueryResponseReceiver,
    ) -> Self {
        Self {
            history_hints_enabled,
            prompt_hints_enabled,
            history_path,
            available_commands,
            first_hint_shown: std::sync::atomic::AtomicBool::new(false),
            agent_names,
            model_query_sender: sender,
            model_query_receiver: RefCell::new(receiver),
        }
    }

    pub fn get_history_path(&self) -> PathBuf {
        self.history_path.clone()
    }

    /// Helper method to check if line matches an agent command pattern and return hint
    fn try_hint_agent_name(&self, line: &str) -> Option<String> {
        for prefix in AGENT_COMMANDS_WITH_NAME_COMPLETION {
            if let Some(agent_part) = line.strip_prefix(prefix) {
                let mut matching_agents: Vec<&String> = self
                    .agent_names
                    .iter()
                    .filter(|name| name.starts_with(agent_part))
                    .collect();
                matching_agents.sort();
                return matching_agents.first().and_then(|name| {
                    let remainder = &name[agent_part.len()..];
                    if remainder.is_empty() {
                        None
                    } else {
                        Some(remainder.to_string())
                    }
                });
            }
        }
        None
    }

    /// Finds the best hint for the current input using rustyline's history
    fn find_hint(&self, line: &str, ctx: &Context<'_>) -> Option<String> {
        // If line is empty, show hint (if enabled)
        if line.is_empty() && self.prompt_hints_enabled {
            // Only show hint on first prompt
            if !self.first_hint_shown.swap(true, Ordering::Relaxed) {
                use rand::Rng;
                // Build weighted list by duplicating hints based on weight
                let mut weighted_hints = Vec::new();
                for (hint, weight) in INITIAL_PROMPT_HINTS {
                    for _ in 0..*weight {
                        weighted_hints.push(*hint);
                    }
                }
                let idx = rand::rng().random_range(0..weighted_hints.len());
                return Some(weighted_hints[idx].to_string());
            }
        }

        // If line starts with /model, try to get model hint
        if let Some(model_prefix) = line.strip_prefix("/model ") {
            // Don't hint for subcommands
            if model_prefix.starts_with("set-") {
                return None;
            }

            // Query for models matching the prefix
            let query = PromptQuery::Models(if !model_prefix.is_empty() {
                Some(model_prefix.to_string())
            } else {
                None
            });

            if self.model_query_sender.send(query).is_ok() {
                let mut receiver = self.model_query_receiver.borrow_mut().resubscribe();

                // Quick poll for hint (don't wait too long or it'll feel laggy)
                for _ in 0..2 {
                    if let Ok(PromptQueryResult::Models(models)) = receiver.try_recv() {
                        if let Some(first_model) = models.first() {
                            // Return the remainder after what user has typed
                            if first_model.len() > model_prefix.len() {
                                return Some(first_model[model_prefix.len()..].to_string());
                            }
                        }
                        break;
                    }
                    std::thread::sleep(std::time::Duration::from_millis(50));
                }
            }
        }

        // If line starts with a slash, try to find a command hint
        if line.starts_with('/') {
            // Check if we're completing an agent name for agent commands
            if let Some(hint) = self.try_hint_agent_name(line) {
                return Some(hint);
            }

            // Otherwise, provide command completion hint
            let cmd_hint = self
                .available_commands
                .iter()
                .find(|cmd| cmd.starts_with(line))
                .map(|cmd| cmd[line.len()..].to_string());
            return cmd_hint;
        }

        // Try to find a hint from rustyline's history if history hints are enabled
        if self.history_hints_enabled {
            let history = ctx.history();
            let history_len = history.len();
            if history_len == 0 {
                return None;
            }

            if let Ok(Some(search_result)) = history.starts_with(line, history_len - 1, SearchDirection::Reverse) {
                let entry = search_result.entry.to_string();
                if entry.len() > line.len() {
                    return Some(entry[line.len()..].to_string());
                }
            }
        }

        None
    }
}

impl RustylineHinter for ChatHinter {
    type Hint = String;

    fn hint(&self, line: &str, pos: usize, ctx: &Context<'_>) -> Option<Self::Hint> {
        // Only provide hints when cursor is at the end of the line
        if pos < line.len() {
            return None;
        }

        self.find_hint(line, ctx)
    }
}

/// Custom validator for multi-line input
pub struct MultiLineValidator;

impl Validator for MultiLineValidator {
    fn validate(&self, os: &mut ValidationContext<'_>) -> rustyline::Result<ValidationResult> {
        let input = os.input();

        // Check for code block markers
        if input.contains("```") {
            // Count the number of ``` occurrences
            let triple_backtick_count = input.matches("```").count();

            // If we have an odd number of ```, we're in an incomplete code block
            if triple_backtick_count % 2 == 1 {
                return Ok(ValidationResult::Incomplete);
            }
        }

        // Check for backslash continuation
        if input.ends_with('\\') {
            return Ok(ValidationResult::Incomplete);
        }

        Ok(ValidationResult::Valid(None))
    }
}

#[derive(Helper, Completer, Hinter)]
pub struct ChatHelper {
    #[rustyline(Completer)]
    completer: ChatCompleter,
    #[rustyline(Hinter)]
    hinter: ChatHinter,
    validator: MultiLineValidator,
}

impl ChatHelper {
    pub fn get_history_path(&self) -> PathBuf {
        self.hinter.get_history_path()
    }
}

impl Validator for ChatHelper {
    fn validate(&self, os: &mut ValidationContext<'_>) -> rustyline::Result<ValidationResult> {
        self.validator.validate(os)
    }
}

impl Highlighter for ChatHelper {
    fn highlight_hint<'h>(&self, hint: &'h str) -> Cow<'h, str> {
        Cow::Owned(format!("\x1b[38;5;240m{hint}\x1b[m"))
    }

    /// Highlight @references with brand purple color
    fn highlight<'l>(&self, line: &'l str, _pos: usize) -> Cow<'l, str> {
        // Skip highlighting if no @ references present
        if !line.contains(REFERENCE_PREFIX) {
            return Cow::Borrowed(line);
        }

        const RESET: &str = "\x1b[0m";
        let brand_escape = format!("\x1b[38;5;{}m", BRAND_PURPLE);

        let mut result = String::with_capacity(line.len() + 32);
        let mut chars = line.char_indices().peekable();
        let mut prev_char: Option<char> = None;

        while let Some((_i, c)) = chars.next() {
            // Check for @ at word boundary (start of line or after whitespace)
            if c == REFERENCE_PREFIX && (prev_char.is_none() || prev_char.is_some_and(|p| p.is_whitespace())) {
                result.push_str(&brand_escape);
                result.push(REFERENCE_PREFIX);

                // Check for quoted path: @"path with spaces"
                if chars.peek().map(|(_, c)| *c) == Some('"') {
                    result.push('"');
                    chars.next(); // consume opening quote
                    // Consume until closing quote
                    while let Some(&(_, next_c)) = chars.peek() {
                        result.push(next_c);
                        chars.next();
                        if next_c == '"' {
                            break;
                        }
                    }
                } else {
                    // Regular path: consume until whitespace
                    while let Some(&(_, next_c)) = chars.peek() {
                        if next_c.is_whitespace() {
                            break;
                        }
                        result.push(next_c);
                        chars.next();
                    }
                }

                result.push_str(RESET);
            } else {
                result.push(c);
            }
            prev_char = Some(c);
        }

        Cow::Owned(result)
    }

    fn highlight_char(&self, line: &str, _pos: usize, _kind: CmdKind) -> bool {
        // Tell rustyline to refresh highlighting when line contains @
        line.contains(REFERENCE_PREFIX)
    }

    fn highlight_prompt<'b, 's: 'b, 'p: 'b>(&'s self, prompt: &'p str, _default: bool) -> Cow<'b, str> {
        use crate::theme::StyledText;

        // Parse the plain text prompt to extract components
        if let Some(components) = parse_prompt_components(prompt) {
            let mut result = String::new();

            // Add delegate notifier if present (colored as warning)
            if let Some(notifier) = components.delegate_notifier {
                result.push_str(&StyledText::warning(&notifier));
                result.push('\n');
            }

            // Add profile part if present
            if let Some(profile) = components.profile {
                if profile == "plan" {
                    // Special styling for planner agent: [plan] in purple (brand color)
                    result.push_str(&StyledText::brand("[plan] "));
                } else if profile == "help" {
                    // Special styling for help agent: [help] in purple (brand color)
                    result.push_str(&StyledText::brand("[help] "));
                } else {
                    // Default styling for other agents: [agent] in cyan (profile indicator)
                    result.push_str(&StyledText::profile(&format!("[{profile}] ")));
                }
            }

            // Add percentage part if present (colored by usage level)
            if let Some(percentage) = components.usage_percentage {
                let colored_percentage = if percentage < 50.0 {
                    StyledText::usage_low(&format!("{}% ", percentage as u32))
                } else if percentage < 90.0 {
                    StyledText::usage_medium(&format!("{}% ", percentage as u32))
                } else {
                    StyledText::usage_high(&format!("{}% ", percentage as u32))
                };
                result.push_str(&colored_percentage);
            }

            // Add tangent indicator if present (tangent yellow)
            if components.tangent_mode {
                result.push_str(&StyledText::tangent("↯ "));
            }

            // Add code intelligence indicator if present (vibrant blue)
            if components.code_intelligence {
                result.push_str(&StyledText::code_intelligence("λ "));
            }

            // Add warning symbol if present (error red)
            if components.warning {
                result.push_str(&StyledText::error("!"));
            }

            // Add the prompt symbol (prompt magenta)
            result.push_str(&StyledText::prompt("> "));

            Cow::Owned(result)
        } else {
            // If we can't parse the prompt, return it as-is
            Cow::Borrowed(prompt)
        }
    }
}

/// Handler for pasting images from clipboard via Ctrl+V
///
/// This stores the pasted image path in shared state and inserts a marker.
/// The marker causes readline to return, and the chat loop handles the paste automatically.
struct PasteImageHandler {
    paste_state: PasteState,
}

impl PasteImageHandler {
    fn new(paste_state: PasteState) -> Self {
        Self { paste_state }
    }
}

impl rustyline::ConditionalEventHandler for PasteImageHandler {
    fn handle(
        &self,
        _evt: &rustyline::Event,
        _n: rustyline::RepeatCount,
        _positive: bool,
        _ctx: &rustyline::EventContext<'_>,
    ) -> Option<Cmd> {
        match paste_image_from_clipboard() {
            Ok(path) => {
                // Store the full path in shared state and get the count
                let count = self.paste_state.add(path);

                // Insert [Image #N] marker so user sees what they're pasting
                // User presses Enter to submit
                Some(Cmd::Insert(1, format!("[Image #{count}]")))
            },
            Err(ClipboardError::NoImage) => {
                // Silent fail - no image to paste
                Some(Cmd::Noop)
            },
            Err(_) => {
                // Could log error, but don't interrupt user
                Some(Cmd::Noop)
            },
        }
    }
}

pub fn rl(
    os: &Os,
    sender: PromptQuerySender,
    receiver: PromptQueryResponseReceiver,
    paste_state: PasteState,
    agents: &crate::cli::agent::Agents,
    agent_swap_state: &super::agent_swap::AgentSwapState,
) -> Result<Editor<ChatHelper, FileHistory>> {
    let edit_mode = match os.database.settings.get_string(Setting::ChatEditMode).as_deref() {
        Some("vi" | "vim") => EditMode::Vi,
        _ => EditMode::Emacs,
    };
    let config = Config::builder()
        .history_ignore_space(true)
        .completion_type(CompletionType::List)
        .edit_mode(edit_mode)
        .build();

    let history_hints_enabled = os
        .database
        .settings
        .get_bool(Setting::ChatEnableHistoryHints)
        .unwrap_or(false);

    let prompt_hints_enabled = os
        .database
        .settings
        .get_bool(Setting::ChatEnablePromptHints)
        .unwrap_or(true);

    let history_path = os.path_resolver().global().cli_bash_history()?;

    // Generate available commands based on enabled experiments
    let available_commands = get_available_commands(os);

    // Extract agent names for completion/hints
    let agent_names: Vec<String> = agents.agents.keys().cloned().collect();

    let h = ChatHelper {
        completer: ChatCompleter::new(
            sender.clone(),
            receiver.resubscribe(),
            available_commands.clone(),
            agent_names.clone(),
        ),
        hinter: ChatHinter::new(
            history_hints_enabled,
            prompt_hints_enabled,
            history_path,
            available_commands,
            agent_names,
            sender,
            receiver,
        ),
        validator: MultiLineValidator,
    };

    let mut rl = Editor::with_config(config)?;
    rl.set_helper(Some(h));

    // Load history from CLI bash history file
    if let Err(e) = rl.load_history(&rl.helper().unwrap().get_history_path())
        && !matches!(e, ReadlineError::Io(ref io_err) if io_err.kind() == std::io::ErrorKind::NotFound)
    {
        eprintln!("Warning: Failed to load history: {e}");
    }

    // Add custom keybinding for Ctrl+D to open delegate command (configurable)
    if ExperimentManager::is_enabled(os, ExperimentName::Delegate)
        && let Some(key) = os.database.settings.get_string(Setting::DelegateModeKey)
        && key.len() == 1
    {
        rl.bind_sequence(
            KeyEvent(KeyCode::Char(key.chars().next().unwrap()), Modifiers::CTRL),
            EventHandler::Simple(Cmd::Insert(1, "/delegate ".to_string())),
        );
    };

    // Add custom keybinding for Alt+Enter to insert a newline
    rl.bind_sequence(
        KeyEvent(KeyCode::Enter, Modifiers::ALT),
        EventHandler::Simple(Cmd::Insert(1, "\n".to_string())),
    );

    // Add custom keybinding for Ctrl+j to insert a newline
    rl.bind_sequence(
        KeyEvent(KeyCode::Char('j'), Modifiers::CTRL),
        EventHandler::Simple(Cmd::Insert(1, "\n".to_string())),
    );

    // Add custom keybinding for autocompletion hint acceptance (configurable)
    let autocompletion_key_char = match os.database.settings.get_string(Setting::AutocompletionKey) {
        Some(key) if key.len() == 1 => key.chars().next().unwrap_or('g'),
        _ => 'g', // Default to 'g' if setting is missing or invalid
    };
    rl.bind_sequence(
        KeyEvent(KeyCode::Char(autocompletion_key_char), Modifiers::CTRL),
        EventHandler::Simple(Cmd::CompleteHint),
    );

    // Add custom keybinding for Ctrl+t to toggle tangent mode (configurable)
    let tangent_key_char = match os.database.settings.get_string(Setting::TangentModeKey) {
        Some(key) if key.len() == 1 => key.chars().next().unwrap_or('t'),
        _ => 't', // Default to 't' if setting is missing or invalid
    };
    rl.bind_sequence(
        KeyEvent(KeyCode::Char(tangent_key_char), Modifiers::CTRL),
        EventHandler::Simple(Cmd::Insert(1, "/tangent".to_string())),
    );

    // Add custom keybinding for Ctrl+V to paste images from clipboard
    rl.bind_sequence(
        KeyEvent(KeyCode::Char('v'), Modifiers::CTRL),
        EventHandler::Conditional(Box::new(PasteImageHandler::new(paste_state))),
    );

    // Setup agent keybinds
    super::agent_keybinds::bind_agent_shortcuts(&mut rl, agents, agent_swap_state)?;

    Ok(rl)
}

#[cfg(test)]
mod tests {
    use rustyline::highlight::Highlighter;
    use rustyline::history::{
        DefaultHistory,
        History,
    };

    use super::*;
    use crate::cli::experiment::experiment_manager::ExperimentName;
    use crate::theme::StyledText;

    #[tokio::test]
    async fn test_chat_completer_command_completion() {
        let (prompt_request_sender, _) = tokio::sync::broadcast::channel::<PromptQuery>(5);
        let (_, prompt_response_receiver) = tokio::sync::broadcast::channel::<PromptQueryResult>(5);

        // Create a mock Os for testing
        let mock_os = crate::os::Os::new().await.unwrap();
        let available_commands = get_available_commands(&mock_os);
        let completer = ChatCompleter::new(
            prompt_request_sender,
            prompt_response_receiver,
            available_commands,
            Vec::new(),
        );
        let line = "/h";
        let pos = 2; // Position at the end of "/h"

        // Create a mock context with empty history
        let empty_history = DefaultHistory::new();
        let ctx = Context::new(&empty_history);

        // Get completions
        let (start, completions) = completer.complete(line, pos, &ctx).unwrap();

        // Verify start position
        assert_eq!(start, 0);

        // Verify completions contain expected commands
        assert!(completions.contains(&"/help".to_string()));
    }

    #[tokio::test]
    async fn test_chat_completer_no_completion() {
        let (prompt_request_sender, _) = tokio::sync::broadcast::channel::<PromptQuery>(5);
        let (_, prompt_response_receiver) = tokio::sync::broadcast::channel::<PromptQueryResult>(5);

        // Create a mock Os for testing
        let mock_os = crate::os::Os::new().await.unwrap();
        let available_commands = get_available_commands(&mock_os);
        let completer = ChatCompleter::new(
            prompt_request_sender,
            prompt_response_receiver,
            available_commands,
            Vec::new(),
        );
        let line = "Hello, how are you?";
        let pos = line.len();

        // Create a mock context with empty history
        let empty_history = DefaultHistory::new();
        let ctx = Context::new(&empty_history);

        // Get completions
        let (_, completions) = completer.complete(line, pos, &ctx).unwrap();

        // Verify no completions are returned for regular text
        assert!(completions.is_empty());
    }

    #[tokio::test]
    async fn test_highlight_prompt_basic() {
        let (prompt_request_sender, _) = tokio::sync::broadcast::channel::<PromptQuery>(5);
        let (_, prompt_response_receiver) = tokio::sync::broadcast::channel::<PromptQueryResult>(5);

        // Create a mock Os for testing
        let mock_os = crate::os::Os::new().await.unwrap();
        let available_commands = get_available_commands(&mock_os);
        let (hinter_sender, _) = tokio::sync::broadcast::channel::<PromptQuery>(5);
        let (_, hinter_receiver) = tokio::sync::broadcast::channel::<PromptQueryResult>(5);
        let helper = ChatHelper {
            completer: ChatCompleter::new(
                prompt_request_sender,
                prompt_response_receiver,
                available_commands.clone(),
                Vec::new(),
            ),
            hinter: ChatHinter::new(
                true,
                true,
                PathBuf::new(),
                available_commands,
                Vec::new(),
                hinter_sender,
                hinter_receiver,
            ),
            validator: MultiLineValidator,
        };

        // Test basic prompt highlighting
        let highlighted = helper.highlight_prompt("> ", true);

        assert_eq!(highlighted, StyledText::prompt("> "));
    }

    #[tokio::test]
    async fn test_highlight_prompt_with_warning() {
        let (prompt_request_sender, _) = tokio::sync::broadcast::channel::<PromptQuery>(5);
        let (_, prompt_response_receiver) = tokio::sync::broadcast::channel::<PromptQueryResult>(5);

        // Create a mock Os for testing
        let mock_os = crate::os::Os::new().await.unwrap();
        let available_commands = get_available_commands(&mock_os);
        let (hinter_sender, _) = tokio::sync::broadcast::channel::<PromptQuery>(5);
        let (_, hinter_receiver) = tokio::sync::broadcast::channel::<PromptQueryResult>(5);
        let helper = ChatHelper {
            completer: ChatCompleter::new(
                prompt_request_sender,
                prompt_response_receiver,
                available_commands.clone(),
                Vec::new(),
            ),
            hinter: ChatHinter::new(
                true,
                true,
                PathBuf::new(),
                available_commands,
                Vec::new(),
                hinter_sender,
                hinter_receiver,
            ),
            validator: MultiLineValidator,
        };

        // Test warning prompt highlighting
        let highlighted = helper.highlight_prompt("!> ", true);

        assert_eq!(
            highlighted,
            format!("{}{}", StyledText::error("!"), StyledText::prompt("> "))
        );
    }

    #[tokio::test]
    async fn test_highlight_prompt_with_profile() {
        let (prompt_request_sender, _) = tokio::sync::broadcast::channel::<PromptQuery>(5);
        let (_, prompt_response_receiver) = tokio::sync::broadcast::channel::<PromptQueryResult>(5);

        // Create a mock Os for testing
        let mock_os = crate::os::Os::new().await.unwrap();
        let available_commands = get_available_commands(&mock_os);
        let (hinter_sender, _) = tokio::sync::broadcast::channel::<PromptQuery>(5);
        let (_, hinter_receiver) = tokio::sync::broadcast::channel::<PromptQueryResult>(5);
        let helper = ChatHelper {
            completer: ChatCompleter::new(
                prompt_request_sender,
                prompt_response_receiver,
                available_commands.clone(),
                Vec::new(),
            ),
            hinter: ChatHinter::new(
                true,
                true,
                PathBuf::new(),
                available_commands,
                Vec::new(),
                hinter_sender,
                hinter_receiver,
            ),
            validator: MultiLineValidator,
        };

        // Test profile prompt highlighting
        let highlighted = helper.highlight_prompt("[test-profile] > ", true);

        assert_eq!(
            highlighted,
            format!("{}{}", StyledText::profile("[test-profile] "), StyledText::prompt("> "))
        );
    }

    #[tokio::test]
    async fn test_highlight_prompt_with_profile_and_warning() {
        let (prompt_request_sender, _) = tokio::sync::broadcast::channel::<PromptQuery>(5);
        let (_, prompt_response_receiver) = tokio::sync::broadcast::channel::<PromptQueryResult>(5);

        // Create a mock Os for testing
        let mock_os = crate::os::Os::new().await.unwrap();
        let available_commands = get_available_commands(&mock_os);
        let (hinter_sender, _) = tokio::sync::broadcast::channel::<PromptQuery>(5);
        let (_, hinter_receiver) = tokio::sync::broadcast::channel::<PromptQueryResult>(5);
        let helper = ChatHelper {
            completer: ChatCompleter::new(
                prompt_request_sender,
                prompt_response_receiver,
                available_commands.clone(),
                Vec::new(),
            ),
            hinter: ChatHinter::new(
                true,
                true,
                PathBuf::new(),
                available_commands,
                Vec::new(),
                hinter_sender,
                hinter_receiver,
            ),
            validator: MultiLineValidator,
        };

        // Test profile + warning prompt highlighting
        let highlighted = helper.highlight_prompt("[dev] !> ", true);
        // Should have cyan profile + red warning + cyan bold prompt
        assert_eq!(
            highlighted,
            format!(
                "{}{}{}",
                StyledText::profile("[dev] "),
                StyledText::error("!"),
                StyledText::prompt("> ")
            )
        );
    }

    #[tokio::test]
    async fn test_highlight_prompt_invalid_format() {
        let (prompt_request_sender, _) = tokio::sync::broadcast::channel::<PromptQuery>(5);
        let (_, prompt_response_receiver) = tokio::sync::broadcast::channel::<PromptQueryResult>(5);

        // Create a mock Os for testing
        let mock_os = crate::os::Os::new().await.unwrap();
        let available_commands = get_available_commands(&mock_os);
        let (hinter_sender, _) = tokio::sync::broadcast::channel::<PromptQuery>(5);
        let (_, hinter_receiver) = tokio::sync::broadcast::channel::<PromptQueryResult>(5);
        let helper = ChatHelper {
            completer: ChatCompleter::new(
                prompt_request_sender,
                prompt_response_receiver,
                available_commands.clone(),
                Vec::new(),
            ),
            hinter: ChatHinter::new(
                true,
                true,
                PathBuf::new(),
                available_commands,
                Vec::new(),
                hinter_sender,
                hinter_receiver,
            ),
            validator: MultiLineValidator,
        };

        // Test invalid prompt format (should return as-is)
        let invalid_prompt = "invalid prompt format";
        let highlighted = helper.highlight_prompt(invalid_prompt, true);
        assert_eq!(highlighted, invalid_prompt);
    }

    #[tokio::test]
    async fn test_highlight_prompt_tangent_mode() {
        let (prompt_request_sender, _) = tokio::sync::broadcast::channel::<PromptQuery>(1);
        let (_, prompt_response_receiver) = tokio::sync::broadcast::channel::<PromptQueryResult>(1);

        // Create a mock Os for testing
        let mock_os = crate::os::Os::new().await.unwrap();
        let available_commands = get_available_commands(&mock_os);
        let (hinter_sender, _) = tokio::sync::broadcast::channel::<PromptQuery>(5);
        let (_, hinter_receiver) = tokio::sync::broadcast::channel::<PromptQueryResult>(5);
        let helper = ChatHelper {
            completer: ChatCompleter::new(
                prompt_request_sender,
                prompt_response_receiver,
                available_commands.clone(),
                Vec::new(),
            ),
            hinter: ChatHinter::new(
                true,
                true,
                PathBuf::new(),
                available_commands,
                Vec::new(),
                hinter_sender,
                hinter_receiver,
            ),
            validator: MultiLineValidator,
        };

        // Test tangent mode prompt highlighting - ↯ yellow, > magenta
        let highlighted = helper.highlight_prompt("↯ > ", true);
        assert_eq!(
            highlighted,
            format!("{}{}", StyledText::tangent("↯ "), StyledText::prompt("> "))
        );
    }

    #[tokio::test]
    async fn test_highlight_prompt_tangent_mode_with_warning() {
        let (prompt_request_sender, _) = tokio::sync::broadcast::channel::<PromptQuery>(1);
        let (_, prompt_response_receiver) = tokio::sync::broadcast::channel::<PromptQueryResult>(1);

        // Create a mock Os for testing
        let mock_os = crate::os::Os::new().await.unwrap();
        let available_commands = get_available_commands(&mock_os);
        let (hinter_sender, _) = tokio::sync::broadcast::channel::<PromptQuery>(5);
        let (_, hinter_receiver) = tokio::sync::broadcast::channel::<PromptQueryResult>(5);
        let helper = ChatHelper {
            completer: ChatCompleter::new(
                prompt_request_sender,
                prompt_response_receiver,
                available_commands.clone(),
                Vec::new(),
            ),
            hinter: ChatHinter::new(
                true,
                true,
                PathBuf::new(),
                available_commands,
                Vec::new(),
                hinter_sender,
                hinter_receiver,
            ),
            validator: MultiLineValidator,
        };

        // Test tangent mode with warning - ↯ yellow, ! red, > magenta
        let highlighted = helper.highlight_prompt("↯ !> ", true);
        assert_eq!(
            highlighted,
            format!(
                "{}{}{}",
                StyledText::tangent("↯ "),
                StyledText::error("!"),
                StyledText::prompt("> ")
            )
        );
    }

    #[tokio::test]
    async fn test_highlight_prompt_profile_with_tangent_mode() {
        let (prompt_request_sender, _) = tokio::sync::broadcast::channel::<PromptQuery>(1);
        let (_, prompt_response_receiver) = tokio::sync::broadcast::channel::<PromptQueryResult>(1);

        // Create a mock Os for testing
        let mock_os = crate::os::Os::new().await.unwrap();
        let available_commands = get_available_commands(&mock_os);
        let (hinter_sender, _) = tokio::sync::broadcast::channel::<PromptQuery>(5);
        let (_, hinter_receiver) = tokio::sync::broadcast::channel::<PromptQueryResult>(5);
        let helper = ChatHelper {
            completer: ChatCompleter::new(
                prompt_request_sender,
                prompt_response_receiver,
                available_commands.clone(),
                Vec::new(),
            ),
            hinter: ChatHinter::new(
                true,
                true,
                PathBuf::new(),
                available_commands,
                Vec::new(),
                hinter_sender,
                hinter_receiver,
            ),
            validator: MultiLineValidator,
        };

        // Test profile with tangent mode - [dev] cyan, ↯ yellow, > magenta
        let highlighted = helper.highlight_prompt("[dev] ↯ > ", true);
        assert_eq!(
            highlighted,
            format!(
                "{}{}{}",
                StyledText::profile("[dev] "),
                StyledText::tangent("↯ "),
                StyledText::prompt("> ")
            )
        );
    }

    #[tokio::test]
    async fn test_chat_hinter_command_hint() {
        // Create a mock Os for testing
        let mock_os = crate::os::Os::new().await.unwrap();
        let available_commands = get_available_commands(&mock_os);
        let (prompt_request_sender, _) = tokio::sync::broadcast::channel::<PromptQuery>(5);
        let (_, prompt_response_receiver) = tokio::sync::broadcast::channel::<PromptQueryResult>(5);
        let hinter = ChatHinter::new(
            true,
            true,
            PathBuf::new(),
            available_commands,
            Vec::new(),
            prompt_request_sender,
            prompt_response_receiver,
        );

        // Test hint for a command
        let line = "/he";
        let pos = line.len();
        let empty_history = DefaultHistory::new();
        let ctx = Context::new(&empty_history);

        let hint = hinter.hint(line, pos, &ctx);
        assert_eq!(hint, Some("lp".to_string()));

        // Test hint when cursor is not at the end
        let hint = hinter.hint(line, 1, &ctx);
        assert_eq!(hint, None);

        // Test hint for a non-existent command
        let line = "/xyz";
        let pos = line.len();
        let hint = hinter.hint(line, pos, &ctx);
        assert_eq!(hint, None);

        // Test hint for a multi-line command
        let line = "/abcd\nefg";
        let pos = line.len();
        let hint = hinter.hint(line, pos, &ctx);
        assert_eq!(hint, None);
    }

    #[tokio::test]
    async fn test_chat_hinter_history_hint_disabled() {
        // Create a mock Os for testing
        let mock_os = crate::os::Os::new().await.unwrap();
        let available_commands = get_available_commands(&mock_os);
        let (prompt_request_sender, _) = tokio::sync::broadcast::channel::<PromptQuery>(5);
        let (_, prompt_response_receiver) = tokio::sync::broadcast::channel::<PromptQueryResult>(5);
        let hinter = ChatHinter::new(
            false,
            true,
            PathBuf::new(),
            available_commands,
            Vec::new(),
            prompt_request_sender,
            prompt_response_receiver,
        );

        // Test hint from history - should be None since history hints are disabled
        let line = "How";
        let pos = line.len();
        let empty_history = DefaultHistory::new();
        let ctx = Context::new(&empty_history);

        let hint = hinter.hint(line, pos, &ctx);
        assert_eq!(hint, None);
    }

    #[tokio::test]
    // If you get a unit test failure for key override, please consider using a new key binding instead.
    // The list of reserved keybindings here are the standard in UNIX world so please don't take them
    async fn test_no_emacs_keybindings_overridden() {
        let (sender, _) = tokio::sync::broadcast::channel::<PromptQuery>(1);
        let (_, receiver) = tokio::sync::broadcast::channel::<PromptQueryResult>(1);

        // Create a mock Os for testing
        let mock_os = crate::os::Os::new().await.unwrap();
        let paste_state = PasteState::new();
        let agents = crate::cli::agent::Agents::default();
        let agent_swap_state = crate::cli::chat::agent_swap::AgentSwapState::new();
        let mut test_editor = rl(&mock_os, sender, receiver, paste_state, &agents, &agent_swap_state).unwrap();

        // Reserved Emacs keybindings that should not be overridden
        let reserved_keys = ['a', 'e', 'f', 'b', 'k'];

        for &key in &reserved_keys {
            let key_event = KeyEvent(KeyCode::Char(key), Modifiers::CTRL);

            // Try to bind and get the previous handler
            let previous_handler = test_editor.bind_sequence(key_event, EventHandler::Simple(Cmd::Noop));

            // If there was a previous handler, it means the key was already bound
            // (which could be our custom binding overriding Emacs)
            if previous_handler.is_some() {
                panic!("Ctrl+{key} appears to be overridden (found existing binding)");
            }
        }
    }

    #[tokio::test]
    async fn test_experiment_based_command_completion() {
        // Test that experimental commands are included when experiments are enabled
        let mock_os = crate::os::Os::new().await.unwrap();
        let available_commands = get_available_commands(&mock_os);

        // Check if experimental commands are included based on experiment status
        let knowledge_enabled = ExperimentManager::is_enabled(&mock_os, ExperimentName::Knowledge);
        let checkpoint_enabled = ExperimentManager::is_enabled(&mock_os, ExperimentName::Checkpoint);
        let todolist_enabled = ExperimentManager::is_enabled(&mock_os, ExperimentName::TodoList);
        let tangent_enabled = ExperimentManager::is_enabled(&mock_os, ExperimentName::TangentMode);

        if knowledge_enabled {
            assert!(available_commands.contains(&"/knowledge"));
            assert!(available_commands.contains(&"/knowledge help"));
        } else {
            assert!(!available_commands.contains(&"/knowledge"));
        }

        if checkpoint_enabled {
            assert!(available_commands.contains(&"/checkpoint"));
            assert!(available_commands.contains(&"/checkpoint help"));
        } else {
            assert!(!available_commands.contains(&"/checkpoint"));
        }

        if todolist_enabled {
            assert!(available_commands.contains(&"/todos"));
            assert!(available_commands.contains(&"/todos help"));
        } else {
            assert!(!available_commands.contains(&"/todos"));
        }

        if tangent_enabled {
            assert!(available_commands.contains(&"/tangent"));
            assert!(available_commands.contains(&"/tangent tail"));
        } else {
            assert!(!available_commands.contains(&"/tangent"));
        }

        // Base commands should always be available
        assert!(available_commands.contains(&"/help"));
        assert!(available_commands.contains(&"/clear"));
        assert!(available_commands.contains(&"/quit"));
    }

    #[test]
    fn test_history_search_is_case_insensitive() {
        let mut history = DefaultHistory::new();
        history.add("Hello World").unwrap();

        // Lowercase search should find mixed-case entry
        let result = history.search("hello", 0, SearchDirection::Reverse).unwrap();
        assert!(result.is_some(), "Should find 'Hello World' when searching for 'hello'");
        assert_eq!(result.unwrap().entry, "Hello World");
    }

    #[tokio::test]
    async fn test_chat_completer_agent_swap_completion() {
        let (prompt_request_sender, _) = tokio::sync::broadcast::channel::<PromptQuery>(5);
        let (_, prompt_response_receiver) = tokio::sync::broadcast::channel::<PromptQueryResult>(5);

        // Create a mock Os for testing
        let mock_os = crate::os::Os::new().await.unwrap();
        let available_commands = get_available_commands(&mock_os);
        let completer = ChatCompleter::new(
            prompt_request_sender,
            prompt_response_receiver,
            available_commands,
            Vec::new(),
        );

        // Create a mock context with empty history
        let empty_history = DefaultHistory::new();
        let ctx = Context::new(&empty_history);

        // Test completion for "/agent s" - should include "/agent swap"
        let line = "/agent s";
        let pos = line.len();
        let (start, completions) = completer.complete(line, pos, &ctx).unwrap();

        // Verify start position
        assert_eq!(start, 0);

        // Verify completions contain "/agent swap"
        assert!(
            completions.contains(&"/agent swap".to_string()),
            "Expected '/agent swap' in completions, got: {:?}",
            completions
        );
    }

    #[tokio::test]
    async fn test_chat_hinter_agent_swap_hint() {
        // Create a mock Os for testing
        let mock_os = crate::os::Os::new().await.unwrap();
        let available_commands = get_available_commands(&mock_os);
        let (prompt_request_sender, _) = tokio::sync::broadcast::channel::<PromptQuery>(5);
        let (_, prompt_response_receiver) = tokio::sync::broadcast::channel::<PromptQueryResult>(5);
        let hinter = ChatHinter::new(
            true,
            true,
            PathBuf::new(),
            available_commands,
            Vec::new(),
            prompt_request_sender,
            prompt_response_receiver,
        );

        // Create a mock context with empty history
        let empty_history = DefaultHistory::new();
        let ctx = Context::new(&empty_history);

        // Test hint for "/agent s" - the hinter provides the first alphabetically sorted match
        // Since commands are sorted, "/agent schema" comes before "/agent swap"
        // So we expect "chema" as the hint, not "wap"
        let line = "/agent s";
        let pos = line.len();
        let hint = hinter.hint(line, pos, &ctx);

        // Should get "chema" as the hint (completes to "/agent schema", the first alphabetical match)
        assert_eq!(
            hint,
            Some("chema".to_string()),
            "Expected 'chema' as hint for '/agent s', got: {:?}",
            hint
        );

        // Test hint for "/agent sw" - should now provide "ap" as ghost text
        let line = "/agent sw";
        let pos = line.len();
        let hint = hinter.hint(line, pos, &ctx);
        assert_eq!(
            hint,
            Some("ap".to_string()),
            "Expected 'ap' as hint for '/agent sw', got: {:?}",
            hint
        );

        // Test that hint is not provided when cursor is not at the end
        let hint = hinter.hint(line, 5, &ctx);
        assert_eq!(hint, None);
    }

    #[tokio::test]
    async fn test_agent_swap_agent_name_completion() {
        let (prompt_request_sender, _) = tokio::sync::broadcast::channel::<PromptQuery>(5);
        let (_, prompt_response_receiver) = tokio::sync::broadcast::channel::<PromptQueryResult>(5);

        // Create a mock Os for testing
        let mock_os = crate::os::Os::new().await.unwrap();
        let available_commands = get_available_commands(&mock_os);

        // Create mock agent names
        let agent_names = vec![
            "kiro-dev".to_string(),
            "kiro-research-agent".to_string(),
            "python-agent".to_string(),
        ];

        let completer = ChatCompleter::new(
            prompt_request_sender,
            prompt_response_receiver,
            available_commands,
            agent_names,
        );

        // Create a mock context with empty history
        let empty_history = DefaultHistory::new();
        let ctx = Context::new(&empty_history);

        // Test completion for "/agent swap k" - should return agents starting with "k"
        let line = "/agent swap k";
        let pos = line.len();
        let (start, completions) = completer.complete(line, pos, &ctx).unwrap();

        // Verify start position is where agent name begins (after "/agent swap ")
        assert_eq!(start, 12);

        // Verify completions contain the two "kiro-" agents
        assert_eq!(completions.len(), 2);
        assert!(completions.contains(&"kiro-dev".to_string()));
        assert!(completions.contains(&"kiro-research-agent".to_string()));
        assert!(!completions.contains(&"python-agent".to_string()));

        // Test completion for "/agent swap p" - should return "python-agent"
        let line = "/agent swap p";
        let pos = line.len();
        let (start, completions) = completer.complete(line, pos, &ctx).unwrap();

        assert_eq!(start, 12);
        assert_eq!(completions.len(), 1);
        assert!(completions.contains(&"python-agent".to_string()));
    }

    #[tokio::test]
    async fn test_agent_swap_agent_name_hint() {
        // Create a mock Os for testing
        let mock_os = crate::os::Os::new().await.unwrap();
        let available_commands = get_available_commands(&mock_os);

        // Create mock agent names
        let agent_names = vec![
            "kiro-dev".to_string(),
            "kiro-research-agent".to_string(),
            "python-agent".to_string(),
        ];

        let (prompt_request_sender, _) = tokio::sync::broadcast::channel::<PromptQuery>(5);
        let (_, prompt_response_receiver) = tokio::sync::broadcast::channel::<PromptQueryResult>(5);
        let hinter = ChatHinter::new(
            true,
            true,
            PathBuf::new(),
            available_commands,
            agent_names,
            prompt_request_sender,
            prompt_response_receiver,
        );

        // Create a mock context with empty history
        let empty_history = DefaultHistory::new();
        let ctx = Context::new(&empty_history);

        // Test hint for "/agent swap k" - should show "iro-dev" (first alphabetically)
        let line = "/agent swap k";
        let pos = line.len();
        let hint = hinter.hint(line, pos, &ctx);
        assert_eq!(hint, Some("iro-dev".to_string()));

        // Test hint for "/agent swap p" - should show "ython-agent"
        let line = "/agent swap p";
        let pos = line.len();
        let hint = hinter.hint(line, pos, &ctx);
        assert_eq!(hint, Some("ython-agent".to_string()));
    }

    #[tokio::test]
    async fn test_agent_delete_agent_name_completion() {
        let (prompt_request_sender, _) = tokio::sync::broadcast::channel::<PromptQuery>(5);
        let (_, prompt_response_receiver) = tokio::sync::broadcast::channel::<PromptQueryResult>(5);

        // Create a mock Os for testing
        let mock_os = crate::os::Os::new().await.unwrap();
        let available_commands = get_available_commands(&mock_os);

        // Create mock agent names
        let agent_names = vec![
            "kiro-dev".to_string(),
            "kiro-research-agent".to_string(),
            "python-agent".to_string(),
        ];

        let completer = ChatCompleter::new(
            prompt_request_sender,
            prompt_response_receiver,
            available_commands,
            agent_names,
        );

        // Create a mock context with empty history
        let empty_history = DefaultHistory::new();
        let ctx = Context::new(&empty_history);

        // Test completion for "/agent delete k" - should return agents starting with "k"
        let line = "/agent delete k";
        let pos = line.len();
        let (start, completions) = completer.complete(line, pos, &ctx).unwrap();

        // Verify start position is where agent name begins (after "/agent delete ")
        assert_eq!(start, 14);

        // Verify completions contain the two "kiro-" agents
        assert_eq!(completions.len(), 2);
        assert!(completions.contains(&"kiro-dev".to_string()));
        assert!(completions.contains(&"kiro-research-agent".to_string()));
        assert!(!completions.contains(&"python-agent".to_string()));

        // Test completion for "/agent delete p" - should return "python-agent"
        let line = "/agent delete p";
        let pos = line.len();
        let (start, completions) = completer.complete(line, pos, &ctx).unwrap();

        assert_eq!(start, 14);
        assert_eq!(completions.len(), 1);
        assert!(completions.contains(&"python-agent".to_string()));
    }

    #[tokio::test]
    async fn test_agent_delete_agent_name_hint() {
        // Create a mock Os for testing
        let mock_os = crate::os::Os::new().await.unwrap();
        let available_commands = get_available_commands(&mock_os);

        // Create mock agent names
        let agent_names = vec![
            "kiro-dev".to_string(),
            "kiro-research-agent".to_string(),
            "python-agent".to_string(),
        ];

        let (prompt_request_sender, _) = tokio::sync::broadcast::channel::<PromptQuery>(5);
        let (_, prompt_response_receiver) = tokio::sync::broadcast::channel::<PromptQueryResult>(5);
        let hinter = ChatHinter::new(
            true,
            true,
            PathBuf::new(),
            available_commands,
            agent_names,
            prompt_request_sender,
            prompt_response_receiver,
        );

        // Create a mock context with empty history
        let empty_history = DefaultHistory::new();
        let ctx = Context::new(&empty_history);

        // Test hint for "/agent delete k" - should show "iro-dev" (first alphabetically)
        let line = "/agent delete k";
        let pos = line.len();
        let hint = hinter.hint(line, pos, &ctx);
        assert_eq!(hint, Some("iro-dev".to_string()));

        // Test hint for "/agent delete p" - should show "ython-agent"
        let line = "/agent delete p";
        let pos = line.len();
        let hint = hinter.hint(line, pos, &ctx);
        assert_eq!(hint, Some("ython-agent".to_string()));

        // Test hint for "/agent delete kiro-r" - should show "esearch-agent"
        let line = "/agent delete kiro-r";
        let pos = line.len();
        let hint = hinter.hint(line, pos, &ctx);
        assert_eq!(hint, Some("esearch-agent".to_string()));

        // Test that no hint is provided for a complete match
        let line = "/agent delete kiro-dev";
        let pos = line.len();
        let hint = hinter.hint(line, pos, &ctx);
        assert_eq!(hint, None);
    }
}
