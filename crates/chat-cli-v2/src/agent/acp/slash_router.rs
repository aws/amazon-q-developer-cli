use std::collections::HashMap;

use agent::tui_commands::TuiCommand;
use sacp::schema::ContentBlock;

/// Result of parsing a slash command from a prompt.
pub enum SlashRoute {
    /// Known action command (/model, /help, etc.)
    Action(TuiCommand),
    /// MCP prompt command (/prompt_name args)
    Prompt {
        name: String,
        args: HashMap<String, String>,
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
    })
}

fn parse_prompt_args(args: &str) -> HashMap<String, String> {
    if args.is_empty() {
        return HashMap::new();
    }
    args.split_whitespace()
        .enumerate()
        .map(|(i, v)| (format!("arg{i}"), v.to_string()))
        .collect()
}

/// Extract text content from resolved MCP prompt messages.
pub fn extract_prompt_text(messages: &[serde_json::Value]) -> String {
    messages
        .iter()
        .filter_map(|m| m.get("content")?.get("text")?.as_str())
        .collect::<Vec<_>>()
        .join("\n")
}
