use std::borrow::Cow;

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
use rustyline::history::DefaultHistory;
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
use crate::database::settings::Setting;
use crate::os::Os;

pub const COMMANDS: &[&str] = &[
    "/clear",
    "/help",
    "/editor",
    "/issue",
    "/quit",
    "/tools",
    "/tools trust",
    "/tools untrust",
    "/tools trust-all",
    "/tools reset",
    "/mcp",
    "/model",
    "/agent",
    "/agent help",
    "/agent list",
    "/agent create",
    "/agent delete",
    "/agent rename",
    "/agent set",
    "/agent schema",
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
    "/save",
    "/load",
    "/subscribe",
];

/// Complete commands that start with a slash
fn complete_command(word: &str, start: usize) -> (usize, Vec<String>) {
    (
        start,
        COMMANDS
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
    sender: std::sync::mpsc::Sender<Option<String>>,
    receiver: std::sync::mpsc::Receiver<Vec<String>>,
}

impl PromptCompleter {
    fn new(sender: std::sync::mpsc::Sender<Option<String>>, receiver: std::sync::mpsc::Receiver<Vec<String>>) -> Self {
        PromptCompleter { sender, receiver }
    }

    fn complete_prompt(&self, word: &str) -> Result<Vec<String>, ReadlineError> {
        let sender = &self.sender;
        let receiver = &self.receiver;
        sender
            .send(if !word.is_empty() { Some(word.to_string()) } else { None })
            .map_err(|e| ReadlineError::Io(std::io::Error::other(e.to_string())))?;
        let prompt_info = receiver
            .recv()
            .map_err(|e| ReadlineError::Io(std::io::Error::other(e.to_string())))?
            .iter()
            .map(|n| format!("@{n}"))
            .collect::<Vec<_>>();

        Ok(prompt_info)
    }
}

pub struct ChatCompleter {
    path_completer: PathCompleter,
    prompt_completer: PromptCompleter,
}

impl ChatCompleter {
    fn new(sender: std::sync::mpsc::Sender<Option<String>>, receiver: std::sync::mpsc::Receiver<Vec<String>>) -> Self {
        Self {
            path_completer: PathCompleter::new(),
            prompt_completer: PromptCompleter::new(sender, receiver),
        }
    }
}

impl Completer for ChatCompleter {
    type Candidate = String;

    fn complete(
        &self,
        line: &str,
        pos: usize,
        _os: &Context<'_>,
    ) -> Result<(usize, Vec<Self::Candidate>), ReadlineError> {
        let (start, word) = extract_word(line, pos, None, |c| c.is_space());

        // Handle command completion
        if word.starts_with('/') {
            return Ok(complete_command(word, start));
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
        if let Ok((pos, completions)) = self.path_completer.complete_path(line, pos, _os) {
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
    /// Command history for providing suggestions based on past commands
    history: Vec<String>,
    /// Whether history-based hints are enabled
    history_hints_enabled: bool,
}

impl ChatHinter {
    /// Creates a new ChatHinter instance
    pub fn new(history_hints_enabled: bool) -> Self {
        Self {
            history: Vec::new(),
            history_hints_enabled,
        }
    }

    /// Updates the history with a new command
    pub fn update_history(&mut self, command: &str) {
        let command = command.trim();
        if !command.is_empty() && !command.contains('\n') && !command.contains('\r') {
            self.history.push(command.to_string());
        }
    }

    /// Finds the best hint for the current input
    fn find_hint(&self, line: &str) -> Option<String> {
        // If line is empty, no hint
        if line.is_empty() {
            return None;
        }

        // If line starts with a slash, try to find a command hint
        if line.starts_with('/') {
            return COMMANDS
                .iter()
                .find(|cmd| cmd.starts_with(line))
                .map(|cmd| cmd[line.len()..].to_string());
        }

        // Try to find a hint from history if history hints are enabled
        if self.history_hints_enabled {
            return self.history
                .iter()
                .rev() // Start from most recent
                .find(|cmd| cmd.starts_with(line) && cmd.len() > line.len())
                .map(|cmd| cmd[line.len()..].to_string());
        }

        None
    }
}

impl RustylineHinter for ChatHinter {
    type Hint = String;

    fn hint(&self, line: &str, pos: usize, _ctx: &Context<'_>) -> Option<Self::Hint> {
        // Only provide hints when cursor is at the end of the line
        if pos < line.len() {
            return None;
        }

        self.find_hint(line)
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
    /// Updates the history of the ChatHinter with a new command
    pub fn update_hinter_history(&mut self, command: &str) {
        self.hinter.update_history(command);
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
        use crossterm::style::Stylize;

        // Parse the plain text prompt to extract profile and warning information
        // and apply colors using crossterm's ANSI escape codes
        if let Some(components) = parse_prompt_components(prompt) {
            let mut result = String::new();

            // Add profile part if present
            if let Some(profile) = components.profile {
                result.push_str(&format!("[{}] ", profile).cyan().to_string());
            }

            // Add warning symbol if present
            if components.warning {
                result.push_str(&"!".red().to_string());
            }

            // Add the prompt symbol
            result.push_str(&"> ".magenta().to_string());

            Cow::Owned(result)
        } else {
            // If we can't parse the prompt, return it as-is
            Cow::Borrowed(prompt)
        }
    }
}

pub fn rl(
    os: &Os,
    sender: std::sync::mpsc::Sender<Option<String>>,
    receiver: std::sync::mpsc::Receiver<Vec<String>>,
) -> Result<Editor<ChatHelper, DefaultHistory>> {
    let edit_mode = match os.database.settings.get_string(Setting::ChatEditMode).as_deref() {
        Some("vi" | "vim") => EditMode::Vi,
        _ => EditMode::Emacs,
    };
    let config = Config::builder()
        .history_ignore_space(true)
        .completion_type(CompletionType::List)
        .edit_mode(edit_mode)
        .build();

    // Default to disabled if setting doesn't exist
    let history_hints_enabled = os
        .database
        .settings
        .get_bool(Setting::ChatEnableHistoryHints)
        .unwrap_or(false);
    let h = ChatHelper {
        completer: ChatCompleter::new(sender, receiver),
        hinter: ChatHinter::new(history_hints_enabled),
        validator: MultiLineValidator,
    };

    let mut rl = Editor::with_config(config)?;
    rl.set_helper(Some(h));

    // Add custom keybinding for Alt+Enter to insert a newline
    rl.bind_sequence(
        KeyEvent(KeyCode::Enter, Modifiers::ALT),
        EventHandler::Simple(Cmd::Insert(1, "\n".to_string())),
    );

    // Add custom keybinding for Ctrl+J to insert a newline
    rl.bind_sequence(
        KeyEvent(KeyCode::Char('j'), Modifiers::CTRL),
        EventHandler::Simple(Cmd::Insert(1, "\n".to_string())),
    );

    // Add custom keybinding for Ctrl+F to accept hint (like fish shell)
    rl.bind_sequence(
        KeyEvent(KeyCode::Char('f'), Modifiers::CTRL),
        EventHandler::Simple(Cmd::CompleteHint),
    );

    Ok(rl)
}

#[cfg(test)]
mod tests {
    use crossterm::style::Stylize;
    use rustyline::highlight::Highlighter;

    use super::*;

    #[test]
    fn test_chat_completer_command_completion() {
        let (prompt_request_sender, _) = std::sync::mpsc::channel::<Option<String>>();
        let (_, prompt_response_receiver) = std::sync::mpsc::channel::<Vec<String>>();
        let completer = ChatCompleter::new(prompt_request_sender, prompt_response_receiver);
        let line = "/h";
        let pos = 2; // Position at the end of "/h"

        // Create a mock context with empty history
        let empty_history = DefaultHistory::new();
        let os = Context::new(&empty_history);

        // Get completions
        let (start, completions) = completer.complete(line, pos, &os).unwrap();

        // Verify start position
        assert_eq!(start, 0);

        // Verify completions contain expected commands
        assert!(completions.contains(&"/help".to_string()));
    }

    #[test]
    fn test_chat_completer_no_completion() {
        let (prompt_request_sender, _) = std::sync::mpsc::channel::<Option<String>>();
        let (_, prompt_response_receiver) = std::sync::mpsc::channel::<Vec<String>>();
        let completer = ChatCompleter::new(prompt_request_sender, prompt_response_receiver);
        let line = "Hello, how are you?";
        let pos = line.len();

        // Create a mock context with empty history
        let empty_history = DefaultHistory::new();
        let os = Context::new(&empty_history);

        // Get completions
        let (_, completions) = completer.complete(line, pos, &os).unwrap();

        // Verify no completions are returned for regular text
        assert!(completions.is_empty());
    }

    #[test]
    fn test_highlight_prompt_basic() {
        let (prompt_request_sender, _) = std::sync::mpsc::channel::<Option<String>>();
        let (_, prompt_response_receiver) = std::sync::mpsc::channel::<Vec<String>>();
        let helper = ChatHelper {
            completer: ChatCompleter::new(prompt_request_sender, prompt_response_receiver),
            hinter: ChatHinter::new(true),
            validator: MultiLineValidator,
        };

        // Test basic prompt highlighting
        let highlighted = helper.highlight_prompt("> ", true);

        assert_eq!(highlighted, "> ".magenta().to_string());
    }

    #[test]
    fn test_highlight_prompt_with_warning() {
        let (prompt_request_sender, _) = std::sync::mpsc::channel::<Option<String>>();
        let (_, prompt_response_receiver) = std::sync::mpsc::channel::<Vec<String>>();
        let helper = ChatHelper {
            completer: ChatCompleter::new(prompt_request_sender, prompt_response_receiver),
            hinter: ChatHinter::new(true),
            validator: MultiLineValidator,
        };

        // Test warning prompt highlighting
        let highlighted = helper.highlight_prompt("!> ", true);

        assert_eq!(highlighted, format!("{}{}", "!".red(), "> ".magenta()));
    }

    #[test]
    fn test_highlight_prompt_with_profile() {
        let (prompt_request_sender, _) = std::sync::mpsc::channel::<Option<String>>();
        let (_, prompt_response_receiver) = std::sync::mpsc::channel::<Vec<String>>();
        let helper = ChatHelper {
            completer: ChatCompleter::new(prompt_request_sender, prompt_response_receiver),
            hinter: ChatHinter::new(true),
            validator: MultiLineValidator,
        };

        // Test profile prompt highlighting
        let highlighted = helper.highlight_prompt("[test-profile] > ", true);

        assert_eq!(highlighted, format!("{}{}", "[test-profile] ".cyan(), "> ".magenta()));
    }

    #[test]
    fn test_highlight_prompt_with_profile_and_warning() {
        let (prompt_request_sender, _) = std::sync::mpsc::channel::<Option<String>>();
        let (_, prompt_response_receiver) = std::sync::mpsc::channel::<Vec<String>>();
        let helper = ChatHelper {
            completer: ChatCompleter::new(prompt_request_sender, prompt_response_receiver),
            hinter: ChatHinter::new(true),
            validator: MultiLineValidator,
        };

        // Test profile + warning prompt highlighting
        let highlighted = helper.highlight_prompt("[dev] !> ", true);
        // Should have cyan profile + red warning + cyan bold prompt
        assert_eq!(
            highlighted,
            format!("{}{}{}", "[dev] ".cyan(), "!".red(), "> ".magenta())
        );
    }

    #[test]
    fn test_highlight_prompt_invalid_format() {
        let (prompt_request_sender, _) = std::sync::mpsc::channel::<Option<String>>();
        let (_, prompt_response_receiver) = std::sync::mpsc::channel::<Vec<String>>();
        let helper = ChatHelper {
            completer: ChatCompleter::new(prompt_request_sender, prompt_response_receiver),
            hinter: ChatHinter::new(true),
            validator: MultiLineValidator,
        };

        // Test invalid prompt format (should return as-is)
        let invalid_prompt = "invalid prompt format";
        let highlighted = helper.highlight_prompt(invalid_prompt, true);
        assert_eq!(highlighted, invalid_prompt);
    }

    #[test]
    fn test_chat_hinter_command_hint() {
        let hinter = ChatHinter::new(true);

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

    #[test]
    fn test_chat_hinter_history_hint_disabled() {
        let mut hinter = ChatHinter::new(false);

        // Add some history
        hinter.update_history("Hello, world!");
        hinter.update_history("How are you?");

        // Test hint from history - should be None since history hints are disabled
        let line = "How";
        let pos = line.len();
        let empty_history = DefaultHistory::new();
        let ctx = Context::new(&empty_history);

        let hint = hinter.hint(line, pos, &ctx);
        assert_eq!(hint, None);
    }
}
