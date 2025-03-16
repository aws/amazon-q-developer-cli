use std::borrow::Cow;

use crossterm::style::Stylize;
use eyre::Result;
use rustyline::completion::{
    Completer,
    extract_word,
};
use rustyline::error::ReadlineError;
use rustyline::highlight::{
    CmdKind,
    Highlighter,
};
use rustyline::history::DefaultHistory;
use rustyline::{
    Completer,
    CompletionType,
    Config,
    Context,
    EditMode,
    Editor,
    Helper,
    Hinter,
    Validator,
};
use winnow::stream::AsChar;

use super::conversation_state::ConversationState;

const COMMANDS: &[&str] = &[
    "/clear",
    "/help",
    "/acceptall",
    "/quit",
    "/context",
    "/context show",
    "/context show --expand",
    "/context add",
    "/context add --global",
    "/context rm",
    "/context rm --global",
    "/context profile",
    "/context profile --create",
    "/context profile --delete",
    "/context switch",
    "/context switch --create",
    "/context clear",
    "/context clear --global",
];

/// Generate a prompt string based on the active context profile
///
/// # Arguments
/// * `conversation_state` - The current conversation state containing the context manager
///
/// # Returns
/// A string to use as the prompt, with the profile indicator if a context profile is active
pub fn generate_prompt(conversation_state: &ConversationState) -> String {
    if let Some(context_manager) = &conversation_state.context_manager {
        if context_manager.current_profile != "default" {
            // Format with profile name for non-default profiles
            let profile = context_manager.current_profile.clone();
            return format!("[{}] > ", profile);
        }
    }

    // Default prompt
    "> ".to_string()
}

pub struct ChatCompleter {}

impl ChatCompleter {
    fn new() -> Self {
        Self {}
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
        Ok((
            start,
            if word.starts_with('/') {
                COMMANDS
                    .iter()
                    .filter(|p| p.starts_with(word))
                    .map(|s| (*s).to_owned())
                    .collect()
            } else {
                Vec::new()
            },
        ))
    }
}

#[derive(Helper, Completer, Hinter, Validator)]
pub struct ChatHelper {
    #[rustyline(Completer)]
    completer: ChatCompleter,
    #[rustyline(Validator)]
    validator: (),
    #[rustyline(Hinter)]
    hinter: (),
}

impl Highlighter for ChatHelper {
    fn highlight_prompt<'b, 's: 'b, 'p: 'b>(&'s self, prompt: &'p str, _default: bool) -> Cow<'b, str> {
        // Always color the entire prompt magenta for consistency
        Cow::Owned(prompt.magenta().to_string())
    }

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
        validator: (),
    };
    let mut rl = Editor::with_config(config)?;
    rl.set_helper(Some(h));
    Ok(rl)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::chat::context::ContextManager;

    #[test]
    fn test_generate_prompt_default_profile() {
        let conversation_state = ConversationState::new(Default::default(), None);
        assert_eq!(generate_prompt(&conversation_state), "> ");
    }

    #[test]
    fn test_generate_prompt_custom_profile() {
        let mut conversation_state = ConversationState::new(Default::default(), None);

        // Create a context manager with a custom profile
        if let Ok(mut context_manager) = ContextManager::new() {
            if let Ok(_) = context_manager.create_profile("test-profile") {
                if let Ok(_) = context_manager.switch_profile("test-profile", false) {
                    conversation_state.context_manager = Some(context_manager);

                    // The prompt should include the profile name
                    let prompt = generate_prompt(&conversation_state);
                    assert!(prompt.contains("test-profile"));
                }
            }
        }
    }

    #[test]
    fn test_generate_prompt_no_context_manager() {
        let mut conversation_state = ConversationState::new(Default::default(), None);
        conversation_state.context_manager = None;

        // Should fall back to default prompt
        assert_eq!(generate_prompt(&conversation_state), "> ");
    }
}
