use std::borrow::Cow;
use std::cell::RefCell;
use std::path::PathBuf;
use std::sync::{
    Arc,
    Mutex,
};

use eyre::Result;
use rustyline::completion::{
    Completer,
    FilenameCompleter,
    extract_word,
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
use winnow::stream::AsChar;

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
use crate::cli::experiment::experiment_manager::{
    ExperimentManager,
    ExperimentName,
};
use crate::database::settings::Setting;
use crate::os::Os;
use crate::util::directories::chat_cli_bash_history_path;

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
    "/experiment",
    "/agent",
    "/agent help",
    "/agent list",
    "/agent create",
    "/agent delete",
    "/agent rename",
    "/agent set",
    "/agent schema",
    "/agent generate",
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
    "/compact",
    "/compact help",
    "/usage",
    "/changelog",
    "/save",
    "/load",
    "/paste",
    "/subscribe",
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
fn complete_command(commands: Vec<&'static str>, word: &str, start: usize) -> (usize, Vec<String>) {
    (
        start,
        commands
            .iter()
            .filter(|p| p.starts_with(word))
            .map(|s| (*s).to_owned())
            .collect(),
    )
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
            PromptQueryResult::Search(list) => list.into_iter().map(|n| format!("@{n}")).collect::<Vec<_>>(),
            PromptQueryResult::List(_) => {
                return Err(ReadlineError::Io(std::io::Error::other(eyre::eyre!(
                    "Wrong query response type received",
                ))));
            },
        };

        Ok(matches)
    }
}

pub struct ChatCompleter {
    path_completer: PathCompleter,
    prompt_completer: PromptCompleter,
    available_commands: Vec<&'static str>,
}

impl ChatCompleter {
    fn new(
        sender: PromptQuerySender,
        receiver: PromptQueryResponseReceiver,
        available_commands: Vec<&'static str>,
    ) -> Self {
        Self {
            path_completer: PathCompleter::new(),
            prompt_completer: PromptCompleter::new(sender, receiver),
            available_commands,
        }
    }
}

impl Completer for ChatCompleter {
    type Candidate = String;

    fn complete(
        &self,
        line: &str,
        pos: usize,
        _ctx: &Context<'_>,
    ) -> Result<(usize, Vec<Self::Candidate>), ReadlineError> {
        let (start, word) = extract_word(line, pos, None, |c| c.is_space());

        // Handle command completion
        if word.starts_with('/') {
            return Ok(complete_command(self.available_commands.clone(), word, start));
        }

        if line.starts_with('@') {
            let search_word = line.strip_prefix('@').unwrap_or("");
            if let Ok(completions) = self.prompt_completer.complete_prompt(search_word) {
                if !completions.is_empty() {
                    return Ok((0, completions));
                }
            }
        }

        // Handle file path completion as fallback
        if let Ok((pos, completions)) = self.path_completer.complete_path(line, pos, _ctx) {
            if !completions.is_empty() {
                return Ok((pos, completions));
            }
        }

        // Default: no completions
        Ok((start, Vec::new()))
    }
}

/// Custom hinter that provides shadowtext suggestions
pub struct ChatHinter {
    /// Whether history-based hints are enabled
    history_hints_enabled: bool,
    history_path: PathBuf,
    available_commands: Vec<&'static str>,
}

impl ChatHinter {
    /// Creates a new ChatHinter instance
    pub fn new(history_hints_enabled: bool, history_path: PathBuf, available_commands: Vec<&'static str>) -> Self {
        Self {
            history_hints_enabled,
            history_path,
            available_commands,
        }
    }

    pub fn get_history_path(&self) -> PathBuf {
        self.history_path.clone()
    }

    /// Finds the best hint for the current input using rustyline's history
    fn find_hint(&self, line: &str, ctx: &Context<'_>) -> Option<String> {
        // If line is empty, no hint
        if line.is_empty() {
            return None;
        }

        // If line starts with a slash, try to find a command hint
        if line.starts_with('/') {
            return self
                .available_commands
                .iter()
                .find(|cmd| cmd.starts_with(line))
                .map(|cmd| cmd[line.len()..].to_string());
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

    fn highlight<'l>(&self, line: &'l str, _pos: usize) -> Cow<'l, str> {
        Cow::Borrowed(line)
    }

    fn highlight_char(&self, _line: &str, _pos: usize, _kind: CmdKind) -> bool {
        false
    }

    fn highlight_prompt<'b, 's: 'b, 'p: 'b>(&'s self, prompt: &'p str, _default: bool) -> Cow<'b, str> {
        use crate::theme::StyledText;

        // Parse the plain text prompt to extract profile and warning information
        // and apply colors using crossterm's ANSI escape codes
        if let Some(components) = parse_prompt_components(prompt) {
            let mut result = String::new();

            // Add notifier part if present (info blue)
            if let Some(notifier) = components.delegate_notifier {
                result.push_str(&StyledText::info(&format!("[{notifier}]\n")));
            }

            // Add profile part if present (profile indicator cyan)
            if let Some(profile) = components.profile {
                result.push_str(&StyledText::profile(&format!("[{profile}] ")));
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

    let history_path = chat_cli_bash_history_path(os)?;

    // Generate available commands based on enabled experiments
    let available_commands = get_available_commands(os);

    let h = ChatHelper {
        completer: ChatCompleter::new(sender, receiver, available_commands.clone()),
        hinter: ChatHinter::new(history_hints_enabled, history_path, available_commands),
        validator: MultiLineValidator,
    };

    let mut rl = Editor::with_config(config)?;
    rl.set_helper(Some(h));

    // Load history from ~/.kiro-cli/cli_history
    if let Err(e) = rl.load_history(&rl.helper().unwrap().get_history_path()) {
        if !matches!(e, ReadlineError::Io(ref io_err) if io_err.kind() == std::io::ErrorKind::NotFound) {
            eprintln!("Warning: Failed to load history: {e}");
        }
    }

    // Add custom keybinding for Ctrl+D to open delegate command (configurable)
    if ExperimentManager::is_enabled(os, ExperimentName::Delegate) {
        if let Some(key) = os.database.settings.get_string(Setting::DelegateModeKey) {
            if key.len() == 1 {
                rl.bind_sequence(
                    KeyEvent(KeyCode::Char(key.chars().next().unwrap()), Modifiers::CTRL),
                    EventHandler::Simple(Cmd::Insert(1, "/delegate ".to_string())),
                );
            }
        };
    }

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

    Ok(rl)
}

#[cfg(test)]
mod tests {
    use rustyline::highlight::Highlighter;
    use rustyline::history::DefaultHistory;

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
        let completer = ChatCompleter::new(prompt_request_sender, prompt_response_receiver, available_commands);
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
        let completer = ChatCompleter::new(prompt_request_sender, prompt_response_receiver, available_commands);
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
        let helper = ChatHelper {
            completer: ChatCompleter::new(
                prompt_request_sender,
                prompt_response_receiver,
                available_commands.clone(),
            ),
            hinter: ChatHinter::new(true, PathBuf::new(), available_commands),
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
        let helper = ChatHelper {
            completer: ChatCompleter::new(
                prompt_request_sender,
                prompt_response_receiver,
                available_commands.clone(),
            ),
            hinter: ChatHinter::new(true, PathBuf::new(), available_commands),
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
        let helper = ChatHelper {
            completer: ChatCompleter::new(
                prompt_request_sender,
                prompt_response_receiver,
                available_commands.clone(),
            ),
            hinter: ChatHinter::new(true, PathBuf::new(), available_commands),
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
        let helper = ChatHelper {
            completer: ChatCompleter::new(
                prompt_request_sender,
                prompt_response_receiver,
                available_commands.clone(),
            ),
            hinter: ChatHinter::new(true, PathBuf::new(), available_commands),
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
        let helper = ChatHelper {
            completer: ChatCompleter::new(
                prompt_request_sender,
                prompt_response_receiver,
                available_commands.clone(),
            ),
            hinter: ChatHinter::new(true, PathBuf::new(), available_commands),
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
        let helper = ChatHelper {
            completer: ChatCompleter::new(
                prompt_request_sender,
                prompt_response_receiver,
                available_commands.clone(),
            ),
            hinter: ChatHinter::new(true, PathBuf::new(), available_commands),
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
        let helper = ChatHelper {
            completer: ChatCompleter::new(
                prompt_request_sender,
                prompt_response_receiver,
                available_commands.clone(),
            ),
            hinter: ChatHinter::new(true, PathBuf::new(), available_commands),
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
        let helper = ChatHelper {
            completer: ChatCompleter::new(
                prompt_request_sender,
                prompt_response_receiver,
                available_commands.clone(),
            ),
            hinter: ChatHinter::new(true, PathBuf::new(), available_commands),
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
        let hinter = ChatHinter::new(true, PathBuf::new(), available_commands);

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
        let hinter = ChatHinter::new(false, PathBuf::new(), available_commands);

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
        let mut test_editor = rl(&mock_os, sender, receiver, paste_state).unwrap();

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
}
