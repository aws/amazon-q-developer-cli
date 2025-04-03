use std::borrow::Cow;

use crossterm::style::Stylize;
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

const COMMANDS: &[&str] = &[
    "/clear",
    "/help",
    "/editor",
    "/issue",
    // "/acceptall", /// Functional, but deprecated in favor of /tools trustall
    "/quit",
    "/tools",
    "/tools trust",
    "/tools untrust",
    "/tools trustall",
    "/tools reset",
    "/profile",
    "/profile help",
    "/profile list",
    "/profile add",
    "/profile create",
    "/profile delete",
    "/profile rename",
    "/profile set",
    "/context help",
    "/context show",
    "/context show --expand",
    "/context add",
    "/context add --global",
    "/context rm",
    "/context rm --global",
    "/context clear",
    "/context clear --global",
    "/compact",
    "/compact help",
    "/compact --summary",
];

pub fn generate_prompt(current_profile: Option<&str>, warning: bool) -> String {
    let warning_symbol = if warning { "!".red().to_string() } else { "".to_string() };
    let profile_part = current_profile
        .filter(|&p| p != "default")
        .map(|p| format!("[{p}] ").cyan().to_string())
        .unwrap_or_default();

    format!("{profile_part}{warning_symbol}{}", "> ".magenta())
}

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
        ctx: &Context<'_>,
    ) -> Result<(usize, Vec<String>), ReadlineError> {
        // Use the filename completer to get path completions
        match self.filename_completer.complete(line, pos, ctx) {
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

pub struct ChatCompleter {
    path_completer: PathCompleter,
}

impl ChatCompleter {
    fn new() -> Self {
        Self {
            path_completer: PathCompleter::new(),
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
            return Ok(complete_command(word, start));
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

/// Custom validator for multi-line input
pub struct MultiLineValidator;

impl Validator for MultiLineValidator {
    fn validate(&self, ctx: &mut ValidationContext<'_>) -> rustyline::Result<ValidationResult> {
        let input = ctx.input();

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
    hinter: (),
    validator: MultiLineValidator,
}

impl Validator for ChatHelper {
    fn validate(&self, ctx: &mut ValidationContext<'_>) -> rustyline::Result<ValidationResult> {
        self.validator.validate(ctx)
    }
}

impl Highlighter for ChatHelper {
    fn highlight_hint<'h>(&self, hint: &'h str) -> Cow<'h, str> {
        Cow::Owned(format!("\x1b[1m{hint}\x1b[m"))
    }

    fn highlight<'l>(&self, line: &'l str, _pos: usize) -> Cow<'l, str> {
        Cow::Borrowed(line)
    }

    fn highlight_char(&self, _line: &str, _pos: usize, _kind: CmdKind) -> bool {
        false
    }
}

pub fn rl() -> Result<Editor<ChatHelper, DefaultHistory>> {
    let edit_mode = match fig_settings::settings::get_string_opt("chat.editMode").as_deref() {
        Some("vi" | "vim") => EditMode::Vi,
        _ => EditMode::Emacs,
    };
    let config = Config::builder()
        .history_ignore_space(true)
        .completion_type(CompletionType::List)
        .edit_mode(edit_mode)
        .build();
    let h = ChatHelper {
        completer: ChatCompleter::new(),
        hinter: (),
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

    Ok(rl)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_prompt() {
        // Test default prompt (no profile)
        assert_eq!(generate_prompt(None, false), "> ".magenta().to_string());
        // Test default prompt with warning
        assert_eq!(generate_prompt(None, true), format!("{}{}", "!".red(), "> ".magenta()));
        // Test default profile (should be same as no profile)
        assert_eq!(generate_prompt(Some("default"), false), "> ".magenta().to_string());
        // Test custom profile
        assert_eq!(
            generate_prompt(Some("test-profile"), false),
            format!("{}{}", "[test-profile] ".cyan(), "> ".magenta())
        );
        // Test another custom profile with warning
        assert_eq!(
            generate_prompt(Some("dev"), true),
            format!("{}{}{}", "[dev] ".cyan(), "!".red(), "> ".magenta())
        );
    }

    #[test]
    fn test_chat_completer_command_completion() {
        let completer = ChatCompleter::new();
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

    #[test]
    fn test_chat_completer_no_completion() {
        let completer = ChatCompleter::new();
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
}
