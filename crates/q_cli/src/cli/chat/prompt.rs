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

const COMMANDS: &[&str] = &[
    "/clear",
    "/help",
    "/acceptall",
    "/quit",
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
];

pub fn generate_prompt(current_profile: Option<&str>) -> String {
    if let Some(profile_name) = &current_profile {
        if *profile_name != "default" {
            // Format with profile name for non-default profiles
            return format!("[{}] > ", profile_name);
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
        // Check if the prompt contains a context profile indicator
        if let Some(profile_end) = prompt.find("] ") {
            // Split the prompt into context part and the rest
            let context_part = &prompt[..=profile_end];
            let rest = &prompt[(profile_end + 1)..];

            // Color the context part cyan and the rest magenta
            Cow::Owned(format!("{}{}", context_part.cyan(), rest.magenta()))
        } else {
            // Default prompt with magenta color
            Cow::Owned(prompt.magenta().to_string())
        }
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

    #[test]
    fn test_generate_prompt() {
        assert_eq!(generate_prompt(None), "> ");
        assert_eq!(generate_prompt(Some("default")), "> ");
        assert!(generate_prompt(Some("test-profile")).contains("test-profile"));
    }
}
