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
use super::history_utils::{truncate_command_for_display, should_display_history};
use crate::database::{Database, settings::Setting};
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
    /// Database reference for accessing settings
    database: Database,
}

impl ChatHinter {
    /// Creates a new ChatHinter instance
    pub fn new(database: Database) -> Self {
        Self { 
            history: Vec::new(),
            database,
        }
    }

    /// Updates the history with a new command
    pub fn update_history(&mut self, command: &str) {
        if !command.trim().is_empty() {
            self.history.push(command.to_string());
        }
    }

    /// Finds the best hint for the current input
    fn find_hint(&self, line: &str) -> Option<String> {
        // If line is empty, no hint
        if line.is_empty() {
            return None;
        }

        // Check if history should be displayed at all
        if !should_display_history(&self.database) {
            // Only provide command hints when history is disabled
            if line.starts_with('/') {
                return COMMANDS
                    .iter()
                    .find(|cmd| cmd.starts_with(line))
                    .map(|cmd| cmd[line.len()..].to_string());
            }
            return None;
        }

        // If line starts with a slash, try to find a command hint
        if line.starts_with('/') {
            return COMMANDS
                .iter()
                .find(|cmd| cmd.starts_with(line))
                .map(|cmd| cmd[line.len()..].to_string());
        }

        // Try to find a hint from history
        self.history
            .iter()
            .rev() // Start from most recent
            .find(|cmd| cmd.starts_with(line) && cmd.len() > line.len())
            .map(|cmd| {
                // Get the full command that matches
                let full_command = cmd;
                // Get the remaining part after the current input
                let remaining_part = &cmd[line.len()..];
                
                // Truncate the full command for display purposes
                let truncated_display = truncate_command_for_display(full_command, &self.database);
                
                // If the truncated version is different from the original, we need to adjust the hint
                if truncated_display.len() < full_command.len() && truncated_display.ends_with("...") {
                    // The command was truncated, so we need to calculate what part of the hint to show
                    let truncated_without_ellipsis = &truncated_display[..truncated_display.len() - 3];
                    
                    if line.len() >= truncated_without_ellipsis.len() {
                        // The current input is already longer than what we can display
                        return String::new();
                    }
                    
                    // Show the remaining part up to the truncation point, then add "..."
                    let remaining_in_truncated = &truncated_without_ellipsis[line.len()..];
                    if remaining_in_truncated.is_empty() {
                        "...".to_string()
                    } else {
                        format!("{}...", remaining_in_truncated)
                    }
                } else {
                    // Command wasn't truncated, show the normal remaining part
                    remaining_part.to_string()
                }
            })
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

        // Check for explicit multi-line markers
        if input.starts_with("```") && !input.ends_with("```") {
            return Ok(ValidationResult::Incomplete);
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
        // Don't truncate the actual input line - user should see their full input
        // Truncation only applies to hints, not to the current input
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
    let h = ChatHelper {
        completer: ChatCompleter::new(sender, receiver),
        hinter: ChatHinter::new(os.database.clone()),
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
    use crate::database::Database;
    
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

    #[tokio::test]
    async fn test_highlight_prompt_basic() {
        let db = Database::new().await.unwrap();
        let (prompt_request_sender, _) = std::sync::mpsc::channel::<Option<String>>();
        let (_, prompt_response_receiver) = std::sync::mpsc::channel::<Vec<String>>();
        let helper = ChatHelper {
            completer: ChatCompleter::new(prompt_request_sender, prompt_response_receiver),
            hinter: ChatHinter::new(db),
            validator: MultiLineValidator,
        };

        // Test basic prompt highlighting
        let highlighted = helper.highlight_prompt("> ", true);

        assert_eq!(highlighted, "> ".magenta().to_string());
    }

    #[tokio::test]
    async fn test_highlight_prompt_with_warning() {
        let db = Database::new().await.unwrap();
        let (prompt_request_sender, _) = std::sync::mpsc::channel::<Option<String>>();
        let (_, prompt_response_receiver) = std::sync::mpsc::channel::<Vec<String>>();
        let helper = ChatHelper {
            completer: ChatCompleter::new(prompt_request_sender, prompt_response_receiver),
            hinter: ChatHinter::new(db),
            validator: MultiLineValidator,
        };

        // Test warning prompt highlighting
        let highlighted = helper.highlight_prompt("!> ", true);

        assert_eq!(highlighted, format!("{}{}", "!".red(), "> ".magenta()));
    }

    #[tokio::test]
    async fn test_highlight_prompt_with_profile() {
        let db = Database::new().await.unwrap();
        let (prompt_request_sender, _) = std::sync::mpsc::channel::<Option<String>>();
        let (_, prompt_response_receiver) = std::sync::mpsc::channel::<Vec<String>>();
        let helper = ChatHelper {
            completer: ChatCompleter::new(prompt_request_sender, prompt_response_receiver),
            hinter: ChatHinter::new(db),
            validator: MultiLineValidator,
        };

        // Test profile prompt highlighting
        let highlighted = helper.highlight_prompt("[test-profile] > ", true);

        assert_eq!(highlighted, format!("{}{}", "[test-profile] ".cyan(), "> ".magenta()));
    }

    #[tokio::test]
    async fn test_highlight_prompt_with_profile_and_warning() {
        let db = Database::new().await.unwrap();
        let (prompt_request_sender, _) = std::sync::mpsc::channel::<Option<String>>();
        let (_, prompt_response_receiver) = std::sync::mpsc::channel::<Vec<String>>();
        let helper = ChatHelper {
            completer: ChatCompleter::new(prompt_request_sender, prompt_response_receiver),
            hinter: ChatHinter::new(db),
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

    #[tokio::test]
    async fn test_highlight_prompt_invalid_format() {
        let db = Database::new().await.unwrap();
        let (prompt_request_sender, _) = std::sync::mpsc::channel::<Option<String>>();
        let (_, prompt_response_receiver) = std::sync::mpsc::channel::<Vec<String>>();
        let helper = ChatHelper {
            completer: ChatCompleter::new(prompt_request_sender, prompt_response_receiver),
            hinter: ChatHinter::new(db),
            validator: MultiLineValidator,
        };

        // Test invalid prompt format (should return as-is)
        let invalid_prompt = "invalid prompt format";
        let highlighted = helper.highlight_prompt(invalid_prompt, true);
        assert_eq!(highlighted, invalid_prompt);
    }

    #[tokio::test]
    async fn test_chat_hinter_command_hint() {
        let db = Database::new().await.unwrap();
        let hinter = ChatHinter::new(db);

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
    }

    #[tokio::test]
    async fn test_chat_hinter_history_hint() {
        let db = Database::new().await.unwrap();
        let mut hinter = ChatHinter::new(db);

        // Add some history
        hinter.update_history("Hello, world!");
        hinter.update_history("How are you?");

        // Test hint from history
        let line = "How";
        let pos = line.len();
        let empty_history = DefaultHistory::new();
        let ctx = Context::new(&empty_history);

        let hint = hinter.hint(line, pos, &ctx);
        assert_eq!(hint, Some(" are you?".to_string()));
    }

    #[tokio::test]
    async fn test_chat_hinter_truncation_basic() {
        let mut db = Database::new().await.unwrap();
        // Set a small truncation limit for testing
        db.settings.set(Setting::ChatHistoryMaxLength, 20).await.unwrap();
        
        let mut hinter = ChatHinter::new(db);
        
        // Add a long command to history
        let long_command = "this is a very long command that exceeds the limit";
        hinter.update_history(long_command);
        
        // Test hint for partial match
        let line = "this";
        let pos = line.len();
        let empty_history = DefaultHistory::new();
        let ctx = Context::new(&empty_history);
        
        let hint = hinter.hint(line, pos, &ctx);
        // Should provide a truncated hint
        assert!(hint.is_some());
        let hint_text = hint.unwrap();
        // The hint should end with "..." indicating truncation
        assert!(hint_text.ends_with("..."));
    }

    #[tokio::test]
    async fn test_chat_hinter_truncation_no_truncation_needed() {
        let mut db = Database::new().await.unwrap();
        // Set a large truncation limit
        db.settings.set(Setting::ChatHistoryMaxLength, 100).await.unwrap();
        
        let mut hinter = ChatHinter::new(db);
        
        // Add a short command to history
        let short_command = "echo hello world";
        hinter.update_history(short_command);
        
        // Test hint for partial match
        let line = "echo";
        let pos = line.len();
        let empty_history = DefaultHistory::new();
        let ctx = Context::new(&empty_history);
        
        let hint = hinter.hint(line, pos, &ctx);
        // Should provide the full remaining part without truncation
        assert_eq!(hint, Some(" hello world".to_string()));
    }

    #[tokio::test]
    async fn test_chat_hinter_history_hidden() {
        let mut db = Database::new().await.unwrap();
        // Set history to hidden (0)
        db.settings.set(Setting::ChatHistoryMaxLength, 0).await.unwrap();
        
        let mut hinter = ChatHinter::new(db);
        
        // Add commands to history
        hinter.update_history("some command in history");
        
        // Test that history hints are not provided
        let line = "some";
        let pos = line.len();
        let empty_history = DefaultHistory::new();
        let ctx = Context::new(&empty_history);
        
        let hint = hinter.hint(line, pos, &ctx);
        // Should not provide history hints when history is hidden
        assert_eq!(hint, None);
        
        // But command hints should still work
        let line = "/he";
        let pos = line.len();
        let hint = hinter.hint(line, pos, &ctx);
        assert_eq!(hint, Some("lp".to_string()));
    }

    #[tokio::test]
    async fn test_chat_hinter_truncation_edge_cases() {
        let mut db = Database::new().await.unwrap();
        // Set a very small truncation limit
        db.settings.set(Setting::ChatHistoryMaxLength, 5).await.unwrap();
        
        let mut hinter = ChatHinter::new(db);
        
        // Add a command that will be heavily truncated
        let command = "very long command";
        hinter.update_history(command);
        
        // Test hint when input is already longer than truncation limit
        let line = "very long";
        let pos = line.len();
        let empty_history = DefaultHistory::new();
        let ctx = Context::new(&empty_history);
        
        let hint = hinter.hint(line, pos, &ctx);
        // Should handle gracefully when input exceeds truncation limit
        assert!(hint.is_some());
    }

    #[tokio::test]
    async fn test_chat_hinter_truncation_with_unicode() {
        let mut db = Database::new().await.unwrap();
        // Set truncation limit
        db.settings.set(Setting::ChatHistoryMaxLength, 15).await.unwrap();
        
        let mut hinter = ChatHinter::new(db);
        
        // Add a command with unicode characters
        let unicode_command = "héllo wörld 🌍 test command";
        hinter.update_history(unicode_command);
        
        // Test hint for partial match
        let line = "héllo";
        let pos = line.len();
        let empty_history = DefaultHistory::new();
        let ctx = Context::new(&empty_history);
        
        let hint = hinter.hint(line, pos, &ctx);
        // Should handle unicode characters properly in truncation
        assert!(hint.is_some());
        let hint_text = hint.unwrap();
        // Should end with "..." due to truncation
        assert!(hint_text.ends_with("..."));
    }

    #[tokio::test]
    async fn test_chat_hinter_multiple_history_entries() {
        let mut db = Database::new().await.unwrap();
        // Set truncation limit
        db.settings.set(Setting::ChatHistoryMaxLength, 25).await.unwrap();
        
        let mut hinter = ChatHinter::new(db);
        
        // Add multiple commands to history
        hinter.update_history("test short command");
        hinter.update_history("test this is a very long command that will be truncated");
        
        // Test hint - should match the most recent command
        let line = "test";
        let pos = line.len();
        let empty_history = DefaultHistory::new();
        let ctx = Context::new(&empty_history);
        
        let hint = hinter.hint(line, pos, &ctx);
        assert!(hint.is_some());
        let hint_text = hint.unwrap();
        // Should provide hint from the most recent (long) command with truncation
        assert!(hint_text.ends_with("..."));
    }

    #[tokio::test]
    async fn test_chat_hinter_negative_setting_fallback() {
        let mut db = Database::new().await.unwrap();
        // Set negative value (should fall back to default)
        db.settings.set(Setting::ChatHistoryMaxLength, -10).await.unwrap();
        
        let mut hinter = ChatHinter::new(db);
        
        // Add a command that would be truncated at default (60) but not at a very large limit
        let command = "this is a moderately long command that is longer than 60 characters for testing";
        hinter.update_history(command);
        
        // Test hint for partial match
        let line = "this";
        let pos = line.len();
        let empty_history = DefaultHistory::new();
        let ctx = Context::new(&empty_history);
        
        let hint = hinter.hint(line, pos, &ctx);
        assert!(hint.is_some());
        let hint_text = hint.unwrap();
        // Should be truncated because negative setting falls back to default (60)
        assert!(hint_text.ends_with("..."));
    }

    #[tokio::test]
    async fn test_highlighter_no_truncation() {
        let mut db = Database::new().await.unwrap();
        // Set a small truncation limit for testing
        db.settings.set(Setting::ChatHistoryMaxLength, 20).await.unwrap();
        
        let (prompt_request_sender, _) = std::sync::mpsc::channel::<Option<String>>();
        let (_, prompt_response_receiver) = std::sync::mpsc::channel::<Vec<String>>();
        let helper = ChatHelper {
            completer: ChatCompleter::new(prompt_request_sender, prompt_response_receiver),
            hinter: ChatHinter::new(db),
            validator: MultiLineValidator,
        };

        // Test that long command is NOT truncated in highlight (user input should be preserved)
        let long_command = "this is a very long command that exceeds the limit";
        let highlighted = helper.highlight(long_command, 0);
        
        // Should NOT be truncated - user should see their full input
        assert_eq!(highlighted, long_command);
        
        // Test short command (should also not be truncated)
        let short_command = "short cmd";
        let highlighted = helper.highlight(short_command, 0);
        assert_eq!(highlighted, short_command);
    }

    #[tokio::test]
    async fn test_highlighter_preserves_input_regardless_of_settings() {
        let mut db = Database::new().await.unwrap();
        // Set history to hidden (0)
        db.settings.set(Setting::ChatHistoryMaxLength, 0).await.unwrap();
        
        let (prompt_request_sender, _) = std::sync::mpsc::channel::<Option<String>>();
        let (_, prompt_response_receiver) = std::sync::mpsc::channel::<Vec<String>>();
        let helper = ChatHelper {
            completer: ChatCompleter::new(prompt_request_sender, prompt_response_receiver),
            hinter: ChatHinter::new(db),
            validator: MultiLineValidator,
        };

        // Test that user input is preserved even when history is hidden
        let command = "any command here";
        let highlighted = helper.highlight(command, 0);
        assert_eq!(highlighted, command); // Should preserve user input
    }

    #[tokio::test]
    async fn test_highlighter_preserves_word_boundaries() {
        let mut db = Database::new().await.unwrap();
        // Set a limit that will test word boundary logic
        db.settings.set(Setting::ChatHistoryMaxLength, 25).await.unwrap();
        
        let (prompt_request_sender, _) = std::sync::mpsc::channel::<Option<String>>();
        let (_, prompt_response_receiver) = std::sync::mpsc::channel::<Vec<String>>();
        let helper = ChatHelper {
            completer: ChatCompleter::new(prompt_request_sender, prompt_response_receiver),
            hinter: ChatHinter::new(db),
            validator: MultiLineValidator,
        };

        // Test that user input is preserved regardless of word boundaries
        let cmd = "short words here and more text";
        let highlighted = helper.highlight(cmd, 0);
        
        // Should preserve the full input
        assert_eq!(highlighted, cmd);
    }

    #[tokio::test]
    async fn test_highlighter_unicode_safety() {
        let mut db = Database::new().await.unwrap();
        // Set a small limit to test unicode boundary handling
        db.settings.set(Setting::ChatHistoryMaxLength, 15).await.unwrap();
        
        let (prompt_request_sender, _) = std::sync::mpsc::channel::<Option<String>>();
        let (_, prompt_response_receiver) = std::sync::mpsc::channel::<Vec<String>>();
        let helper = ChatHelper {
            completer: ChatCompleter::new(prompt_request_sender, prompt_response_receiver),
            hinter: ChatHinter::new(db),
            validator: MultiLineValidator,
        };

        // Test with unicode characters - should preserve full input
        let unicode_cmd = "héllo wörld 🌍 test command";
        let highlighted = helper.highlight(unicode_cmd, 0);
        
        // Should preserve the full unicode input
        assert_eq!(highlighted, unicode_cmd);
    }
}
