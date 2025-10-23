//! Everything in this module is here to preserve the old UI's look and feel when used on top of
//! the new event loop.
use std::borrow::Cow;
use std::path::PathBuf;

use crossterm::style::{
    Color,
    ResetColor,
    SetAttribute,
    SetForegroundColor,
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

/// Complete commands that start with a slash
fn complete_command(commands: &[String], word: &str, start: usize) -> (usize, Vec<String>) {
    (
        start,
        commands
            .iter()
            .filter(|p| p.starts_with(word))
            .map(|s| (*s).clone())
            .collect(),
    )
}

/// A wrapper around FilenameCompleter that provides enhanced path detection
/// and completion capabilities for the chat interface.
#[derive(Default)]
pub struct PathCompleter {
    /// The underlying filename completer from rustyline
    filename_completer: FilenameCompleter,
}

impl PathCompleter {
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

pub struct ChatCompleter {
    available_commands: Vec<String>,
    available_prompts: Vec<String>,
    path_completer: PathCompleter,
}

impl ChatCompleter {
    fn new(available_commands: Vec<String>, available_prompts: Vec<String>) -> Self {
        Self {
            available_commands,
            available_prompts,
            path_completer: Default::default(),
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
        let (start, word) = extract_word(line, pos, None, |c| c.is_whitespace());

        // Handle command completion
        if word.starts_with('/') {
            return Ok(complete_command(&self.available_commands, word, start));
        }

        if line.starts_with('@') {
            let search_word = line.strip_prefix('@').unwrap_or("");
            // Here we assume that the names given by the event loop is already namespaced
            // appropriately (i.e. not namespaced if the prompt name is unique and namespaced with
            // their respective server if it is)
            let completions = self
                .available_prompts
                .iter()
                .filter_map(|p| {
                    if p.contains(search_word) {
                        Some(format!("@{p}"))
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>();

            if !completions.is_empty() {
                return Ok((0, completions));
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
    available_commands: Vec<String>,
}

impl ChatHinter {
    /// Creates a new ChatHinter instance
    pub fn new(history_hints_enabled: bool, history_path: PathBuf, available_commands: Vec<String>) -> Self {
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
        // Parse the plain text prompt to extract profile and warning information
        // and apply colors using crossterm's ANSI escape codes
        if let Some(components) = parse_prompt_components(prompt) {
            let mut result = String::new();

            // Add notifier part if present (info blue)
            if let Some(notifier) = components.delegate_notifier {
                let notifier = format!("[{}]\n", notifier);
                result.push_str(&format!("\x1b[{}m{}\x1b[0m", color_to_ansi_code(Color::Blue), notifier));
            }

            // Add profile part if present (profile indicator cyan)
            if let Some(profile) = components.profile {
                let profile = &format!("[{}] ", profile);
                result.push_str(&format!("\x1b[{}m{}\x1b[0m", color_to_ansi_code(Color::Cyan), profile));
            }

            // Add percentage part if present (colored by usage level)
            if let Some(percentage) = components.usage_percentage {
                let text = format!("{}% ", percentage as u32);
                let colored_percentage = if percentage < 50.0 {
                    format!("\x1b[{}m{}\x1b[0m", color_to_ansi_code(Color::Green), text)
                } else if percentage < 90.0 {
                    format!("\x1b[{}m{}\x1b[0m", color_to_ansi_code(Color::Yellow), text)
                } else {
                    format!("\x1b[{}m{}\x1b[0m", color_to_ansi_code(Color::Red), text)
                };
                result.push_str(&colored_percentage);
            }

            // Add tangent indicator if present (tangent yellow)
            if components.tangent_mode {
                let text = format!("\x1b[{}m{}\x1b[0m", color_to_ansi_code(Color::Yellow), "↯ ");
                result.push_str(&text);
            }

            // Add warning symbol if present (error red)
            if components.warning {
                let text = format!("\x1b[{}m{}\x1b[0m", color_to_ansi_code(Color::Red), "!");
                result.push_str(&text);
            }

            // Add the prompt symbol (prompt magenta)
            let text = format!("\x1b[{}m{}\x1b[0m", color_to_ansi_code(Color::Magenta), "> ");
            result.push_str(&text);

            Cow::Owned(result)
        } else {
            // If we can't parse the prompt, return it as-is
            Cow::Borrowed(prompt)
        }
    }
}

fn color_to_ansi_code(color: Color) -> u8 {
    match color {
        Color::Black => 30,
        Color::DarkGrey => 90,
        Color::Red => 31,
        Color::DarkRed => 31,
        Color::Green => 32,
        Color::DarkGreen => 32,
        Color::Yellow => 33,
        Color::DarkYellow => 33,
        Color::Blue => 34,
        Color::DarkBlue => 34,
        Color::Magenta => 35,
        Color::DarkMagenta => 35,
        Color::Cyan => 36,
        Color::DarkCyan => 36,
        Color::White => 37,
        Color::Grey => 37,
        Color::Rgb { r, g, b } => {
            // For RGB colors, we'll use a simplified mapping to the closest basic color
            // This is a fallback - in practice, most terminals support RGB
            if r > 200 && g < 100 && b < 100 {
                31
            }
            // Red-ish
            else if r < 100 && g > 200 && b < 100 {
                32
            }
            // Green-ish
            else if r > 200 && g > 200 && b < 100 {
                33
            }
            // Yellow-ish
            else if r < 100 && g < 100 && b > 200 {
                34
            }
            // Blue-ish
            else if r > 200 && g < 100 && b > 200 {
                35
            }
            // Magenta-ish
            else if r < 100 && g > 200 && b > 200 {
                36
            }
            // Cyan-ish
            else if r > 150 && g > 150 && b > 150 {
                37
            }
            // White-ish
            else {
                30
            } // Black-ish
        },
        Color::AnsiValue(val) => {
            // Map ANSI 256 colors to basic 8 colors
            match val {
                0..=7 => 30 + val,
                8..=15 => 90 + (val - 8),
                _ => 37, // Default to white for other values
            }
        },
        Color::Reset => 37, // Default to white
    }
}

/// Components extracted from a prompt string
#[derive(Debug, PartialEq)]
pub struct PromptComponents {
    pub delegate_notifier: Option<String>,
    pub profile: Option<String>,
    pub warning: bool,
    pub tangent_mode: bool,
    pub usage_percentage: Option<f32>,
}

/// Parse prompt components from a plain text prompt
pub fn parse_prompt_components(prompt: &str) -> Option<PromptComponents> {
    // Expected format: "[agent] 6% !> " or "> " or "!> " or "[agent] ↯ > " or "6% ↯ > " etc.
    let mut delegate_notifier = None::<String>;
    let mut profile = None;
    let mut warning = false;
    let mut tangent_mode = false;
    let mut usage_percentage = None;
    let mut remaining = prompt.trim();

    // Check for delegate notifier first
    if let Some(start) = remaining.find('[') {
        if let Some(end) = remaining.find(']') {
            if start < end {
                let content = &remaining[start + 1..end];
                // Only set profile if it's not "BACKGROUND TASK READY" or if it doesn't end with newline
                if content == "BACKGROUND TASK READY" && remaining[end + 1..].starts_with('\n') {
                    delegate_notifier = Some(content.to_string());
                    remaining = remaining[end + 1..].trim_start();
                }
            }
        }
    }

    // Check for agent pattern [agent] first
    if let Some(start) = remaining.find('[') {
        if let Some(end) = remaining.find(']') {
            if start < end {
                let content = &remaining[start + 1..end];
                profile = Some(content.to_string());
                remaining = remaining[end + 1..].trim_start();
            }
        }
    }

    // Check for percentage pattern (e.g., "6% ")
    if let Some(percent_pos) = remaining.find('%') {
        let before_percent = &remaining[..percent_pos];
        if let Ok(percentage) = before_percent.trim().parse::<f32>() {
            usage_percentage = Some(percentage);
            if let Some(space_after_percent) = remaining[percent_pos..].find(' ') {
                remaining = remaining[percent_pos + space_after_percent + 1..].trim_start();
            }
        }
    }

    // Check for tangent mode ↯ first
    if let Some(after_tangent) = remaining.strip_prefix('↯') {
        tangent_mode = true;
        remaining = after_tangent.trim_start();
    }

    // Check for warning symbol ! (comes after tangent mode)
    if remaining.starts_with('!') {
        warning = true;
        remaining = remaining[1..].trim_start();
    }

    // Should end with "> " for both normal and tangent mode
    if remaining.trim_end() == ">" {
        Some(PromptComponents {
            delegate_notifier,
            profile,
            warning,
            tangent_mode,
            usage_percentage,
        })
    } else {
        None
    }
}

pub fn generate_prompt(
    current_profile: Option<&str>,
    warning: bool,
    tangent_mode: bool,
    usage_percentage: Option<f32>,
) -> String {
    // Generate plain text prompt that will be colored by highlight_prompt
    let warning_symbol = if warning { "!" } else { "" };
    let profile_part = current_profile.map(|p| format!("[{p}] ")).unwrap_or_default();

    let percentage_part = usage_percentage.map(|p| format!("{:.0}% ", p)).unwrap_or_default();

    if tangent_mode {
        format!("{profile_part}{percentage_part}↯ {warning_symbol}> ")
    } else {
        format!("{profile_part}{percentage_part}{warning_symbol}> ")
    }
}

#[allow(clippy::too_many_arguments)]
pub fn rl(
    history_hints_enabled: bool,
    edit_mode: EditMode,
    history_path: PathBuf,
    available_commands: Vec<String>,
    available_prompts: Vec<String>,
) -> eyre::Result<Editor<ChatHelper, FileHistory>> {
    let config = Config::builder()
        .history_ignore_space(true)
        .completion_type(CompletionType::List)
        .edit_mode(edit_mode)
        .build();

    let h = ChatHelper {
        completer: ChatCompleter::new(available_commands.clone(), available_prompts),
        hinter: ChatHinter::new(history_hints_enabled, history_path, available_commands),
        validator: MultiLineValidator,
    };

    let mut rl = Editor::with_config(config)?;
    rl.set_helper(Some(h));

    if let Err(e) = rl.load_history(&rl.helper().unwrap().get_history_path()) {
        if !matches!(e, ReadlineError::Io(ref io_err) if io_err.kind() == std::io::ErrorKind::NotFound) {
            eprintln!("Warning: Failed to load history: {}", e);
        }
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

    Ok(rl)
}

/// This trait is purely here to facilitate a smooth transition from the old event loop to a new
/// event loop. It is a way to achieve inversion of control to delegate the implementation of
/// themes to the consumer of this crate. Without this, we would be running into a circular
/// dependency.
pub trait ThemeSource: Send + Sync + 'static {
    fn error(&self, text: &str) -> String;
    fn info(&self, text: &str) -> String;
    fn emphasis(&self, text: &str) -> String;
    fn command(&self, text: &str) -> String;
    fn prompt(&self, text: &str) -> String;
    fn profile(&self, text: &str) -> String;
    fn tangent(&self, text: &str) -> String;
    fn usage_low(&self, text: &str) -> String;
    fn usage_medium(&self, text: &str) -> String;
    fn usage_high(&self, text: &str) -> String;
    fn brand(&self, text: &str) -> String;
    fn primary(&self, text: &str) -> String;
    fn secondary(&self, text: &str) -> String;
    fn success(&self, text: &str) -> String;
    fn error_fg(&self) -> SetForegroundColor;
    fn warning_fg(&self) -> SetForegroundColor;
    fn success_fg(&self) -> SetForegroundColor;
    fn info_fg(&self) -> SetForegroundColor;
    fn brand_fg(&self) -> SetForegroundColor;
    fn secondary_fg(&self) -> SetForegroundColor;
    fn emphasis_fg(&self) -> SetForegroundColor;
    fn reset(&self) -> ResetColor;
    fn reset_attributes(&self) -> SetAttribute;
}
