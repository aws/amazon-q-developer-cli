use agent::tui_commands::TuiCommand;
use sacp::schema::ContentBlock;

/// Result of parsing a slash command from a prompt.
pub enum SlashRoute {
    /// Known action command (/model, /help, etc.)
    Action(TuiCommand),
    /// Prompt command (/prompt_name args)
    Prompt {
        name: String,
        args: Vec<String>,
        /// Original trimmed text (including leading '/'), used to fall back to
        /// a plain user message when `name` matches no known prompt.
        original: String,
    },
}

/// Parse a slash command from ACP prompt content blocks.
pub fn parse(prompt: &[ContentBlock]) -> Option<SlashRoute> {
    let text = prompt.iter().find_map(|block| {
        if let ContentBlock::Text(t) = block {
            let trimmed = t.text.trim();
            trimmed.starts_with('/').then_some(trimmed)
        } else {
            None
        }
    })?;

    let without_slash = &text[1..];
    let (name, args_str) = match without_slash.split_once(char::is_whitespace) {
        Some((n, a)) => (n, a.trim()),
        None => (without_slash, ""),
    };

    if let Some(cmd) = TuiCommand::parse(name, args_str) {
        return Some(SlashRoute::Action(cmd));
    }

    Some(SlashRoute::Prompt {
        name: name.to_string(),
        args: parse_prompt_args(args_str),
        original: text.to_string(),
    })
}

/// Parse prompt arguments with quote-aware splitting.
fn parse_prompt_args(input: &str) -> Vec<String> {
    let mut args = Vec::new();
    let mut chars = input.chars().peekable();

    while chars.peek().is_some() {
        // Skip whitespace
        while chars.peek().is_some_and(|c| c.is_whitespace()) {
            chars.next();
        }

        if chars.peek().is_none() {
            break;
        }

        if chars.peek() == Some(&'"') {
            chars.next(); // consume opening quote
            let mut arg = String::new();
            while let Some(&c) = chars.peek() {
                if c == '"' {
                    chars.next(); // consume closing quote
                    break;
                }
                arg.push(c);
                chars.next();
            }
            args.push(arg);
        } else {
            let mut arg = String::new();
            while let Some(&c) = chars.peek() {
                if c.is_whitespace() {
                    break;
                }
                arg.push(c);
                chars.next();
            }
            args.push(arg);
        }
    }

    args
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_prompt_args_simple() {
        assert_eq!(parse_prompt_args(""), Vec::<String>::new());
        assert_eq!(parse_prompt_args("a b c"), vec!["a", "b", "c"]);
        assert_eq!(parse_prompt_args("  a  b  "), vec!["a", "b"]);
    }

    #[test]
    fn test_parse_prompt_args_quoted() {
        assert_eq!(parse_prompt_args(r#"src/main.rs "error handling""#), vec![
            "src/main.rs",
            "error handling"
        ]);
        assert_eq!(parse_prompt_args(r#""hello world" test"#), vec!["hello world", "test"]);
        assert_eq!(parse_prompt_args(r#""a" "b" "c""#), vec!["a", "b", "c"]);
    }

    #[test]
    fn test_parse_prompt_args_unclosed_quote() {
        // Unclosed quote consumes to end of input
        assert_eq!(parse_prompt_args(r#""unclosed"#), vec!["unclosed"]);
    }
}
