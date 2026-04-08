//! Slash command execution - each command has its own module with execute fn

pub mod agent;
pub mod chat;
pub mod clear;
pub mod code;
pub mod compact;
pub mod context;
pub mod exit;
pub mod help;
pub mod hooks;
pub mod issue;
pub mod knowledge;
pub mod mcp;
pub mod model;
pub mod paste_image;
pub mod plan;
pub mod prompts;
pub mod reply;
pub mod tools;
pub mod usage;

use std::path::PathBuf;
use std::sync::Arc;

use ::agent::AgentHandle;
use ::agent::agent_config::LoadedAgentConfig;
use ::agent::tui_commands::{
    CommandResult,
    TuiCommand,
};

use crate::agent::acp::session_manager::{
    AgentInfo,
    SessionManagerHandle,
};
use crate::agent::rts::RtsState;
use crate::api_client::ApiClient;

/// Split a string into arguments, respecting quoted strings and backslash escapes.
///
/// Examples:
/// - `"/path/with spaces/file.md" other` → `["/path/with spaces/file.md", "other"]`
/// - `'/path/with spaces/file.md'` → `["/path/with spaces/file.md"]`
/// - `~/path/with\ spaces/file.md` → `["~/path/with spaces/file.md"]`
pub fn shell_split(input: &str) -> Vec<String> {
    let mut args = Vec::new();
    let mut chars = input.chars().peekable();

    while chars.peek().is_some() {
        skip_whitespace(&mut chars);
        if chars.peek().is_none() {
            break;
        }
        let arg = match chars.peek() {
            Some(&'"') => parse_quoted(&mut chars, '"'),
            Some(&'\'') => parse_quoted(&mut chars, '\''),
            _ => parse_unquoted(&mut chars),
        };
        args.push(arg);
    }
    args
}

/// Advance past any leading whitespace.
fn skip_whitespace(chars: &mut std::iter::Peekable<std::str::Chars<'_>>) {
    while chars.peek().is_some_and(|c| c.is_whitespace()) {
        chars.next();
    }
}

/// Parse a quoted argument. Consumes the opening and closing quote character.
fn parse_quoted(chars: &mut std::iter::Peekable<std::str::Chars<'_>>, quote: char) -> String {
    chars.next(); // consume opening quote
    let mut arg = String::new();
    while let Some(&c) = chars.peek() {
        if c == quote {
            chars.next(); // consume closing quote
            break;
        }
        arg.push(c);
        chars.next();
    }
    arg
}

/// Parse an unquoted argument, handling backslash escapes.
fn parse_unquoted(chars: &mut std::iter::Peekable<std::str::Chars<'_>>) -> String {
    let mut arg = String::new();
    while let Some(&c) = chars.peek() {
        match c {
            '\\' => {
                chars.next();
                if let Some(&next) = chars.peek() {
                    arg.push(next);
                    chars.next();
                }
            },
            c if c.is_whitespace() => break,
            _ => {
                arg.push(c);
                chars.next();
            },
        }
    }
    arg
}

/// Strip surrounding quotes (single or double) from a string.
/// e.g. `"/path/with spaces"` → `/path/with spaces`
pub fn strip_quotes(s: &str) -> &str {
    s.trim().trim_matches('"').trim_matches('\'')
}

/// Split input into a name (first word) and a trailing path, stripping quotes from the path.
/// Used by commands like `/knowledge add <name> <path>` where the path is the last argument
/// and may contain unquoted spaces.
pub fn split_name_and_path(input: &str) -> Option<(&str, &str)> {
    let parts: Vec<&str> = input.splitn(2, ' ').collect();
    if parts.len() < 2 || parts[0].is_empty() || parts[1].is_empty() {
        return None;
    }
    Some((parts[0], strip_quotes(parts[1])))
}

/// Context passed to command executors
pub struct CommandContext<'a> {
    pub api_client: &'a ApiClient,
    pub rts_state: &'a Arc<RtsState>,
    pub agent: &'a AgentHandle,
    pub session_tx: &'a SessionManagerHandle,
    pub available_agents: &'a [AgentInfo],
    pub agent_configs: &'a [LoadedAgentConfig],
    pub local_mcp_path: Option<&'a PathBuf>,
    pub global_mcp_path: Option<&'a PathBuf>,
    pub session_id: &'a str,
    pub current_agent_name: &'a str,
    pub os: &'a crate::os::Os,
    pub cwd: &'a std::path::Path,
    pub legacy_session_exporter: &'a Arc<dyn crate::agent::session::legacy_compat::LegacySessionExporter>,
    pub session_injected_mcp_servers: &'a [(String, ::agent::agent_config::definitions::McpServerConfig)],
}
/// Execute a slash command by dispatching to the appropriate module
pub async fn execute(command: TuiCommand, ctx: &CommandContext<'_>) -> CommandResult {
    match command {
        TuiCommand::Help(_args) => help::execute(ctx).await,
        TuiCommand::Model(ref args) => model::execute(args, ctx).await,
        TuiCommand::Agent(ref args) => agent::execute(args, ctx).await,
        TuiCommand::Context(ref args) => context::execute(args, ctx).await,
        TuiCommand::Compact(ref args) => compact::execute(args, ctx).await,
        TuiCommand::Clear(ref args) => clear::execute(args, ctx).await,
        TuiCommand::Quit(ref args) => exit::execute(args, ctx).await,
        TuiCommand::Usage(_args) => usage::execute(ctx).await,
        TuiCommand::PasteImage(_) => paste_image::execute().await,
        TuiCommand::Mcp(ref args) => mcp::execute(ctx, args).await,
        TuiCommand::Tools(ref args) => tools::execute(args, ctx).await,
        TuiCommand::Plan(ref args) => plan::execute(args.prompt.as_deref(), ctx).await,
        TuiCommand::Feedback(ref args) => {
            let is_amzn = matches!(
                crate::auth::builder_id::BuilderIdToken::load(&ctx.os.database, None).await,
                Ok(Some(token)) if token.is_amzn_user()
            );
            issue::execute(args.feedback_type.as_deref(), is_amzn).await
        },
        TuiCommand::Knowledge(ref args) => knowledge::execute(args, ctx).await,
        TuiCommand::Prompts(ref args) => prompts::execute(args).await,
        TuiCommand::Chat(ref args) => chat::execute(args, ctx).await,
        TuiCommand::Reply(_) => reply::execute(ctx).await,
        TuiCommand::Code(ref args) => code::execute(args, ctx).await,
        TuiCommand::Hooks(_) => hooks::execute(ctx).await,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_shell_split_simple() {
        assert_eq!(shell_split("a b c"), vec!["a", "b", "c"]);
        assert_eq!(shell_split("  a  b  "), vec!["a", "b"]);
        assert_eq!(shell_split(""), Vec::<String>::new());
    }

    #[test]
    fn test_shell_split_quoted_path_with_spaces() {
        assert_eq!(shell_split(r#""/Users/user/Documents/Obsidian Vault/file.md""#), vec![
            "/Users/user/Documents/Obsidian Vault/file.md"
        ]);
    }

    #[test]
    fn test_shell_split_mixed() {
        assert_eq!(shell_split(r#"--force "/path/with spaces/file.md""#), vec![
            "--force",
            "/path/with spaces/file.md"
        ]);
    }

    #[test]
    fn test_shell_split_unclosed_quote() {
        assert_eq!(shell_split(r#""unclosed"#), vec!["unclosed"]);
    }

    #[test]
    fn test_shell_split_multiple_quoted() {
        assert_eq!(shell_split(r#""first arg" "second arg""#), vec![
            "first arg",
            "second arg"
        ]);
    }

    #[test]
    fn test_shell_split_backslash_escaped_spaces() {
        assert_eq!(shell_split(r"hello\ world"), vec!["hello world"]);
        assert_eq!(shell_split(r"a\ b c"), vec!["a b", "c"]);
    }

    #[test]
    fn test_shell_split_single_quoted() {
        assert_eq!(shell_split("'/path/with spaces/file.md'"), vec![
            "/path/with spaces/file.md"
        ]);
        assert_eq!(shell_split("'/a b' \"/c d\""), vec!["/a b", "/c d"]);
    }

    // Tests for single-path commands that use strip_quotes / split_name_and_path
    // (e.g. /chat load, /knowledge add, /knowledge remove)

    #[test]
    fn test_strip_quotes_unquoted_with_spaces() {
        assert_eq!(
            strip_quotes("/path/with spaces/session.zip"),
            "/path/with spaces/session.zip"
        );
    }

    #[test]
    fn test_strip_quotes_double_quoted() {
        assert_eq!(
            strip_quotes(r#""/path/with spaces/file.md""#),
            "/path/with spaces/file.md"
        );
    }

    #[test]
    fn test_strip_quotes_single_quoted() {
        assert_eq!(strip_quotes("'/path/with spaces/file.md'"), "/path/with spaces/file.md");
    }

    #[test]
    fn test_split_name_and_path_unquoted_with_spaces() {
        let (name, path) = split_name_and_path("mydb /path/with spaces/data.db").unwrap();
        assert_eq!(name, "mydb");
        assert_eq!(path, "/path/with spaces/data.db");
    }

    #[test]
    fn test_split_name_and_path_quoted() {
        let (name, path) = split_name_and_path(r#"mydb "/path/with spaces/data.db""#).unwrap();
        assert_eq!(name, "mydb");
        assert_eq!(path, "/path/with spaces/data.db");
    }

    #[test]
    fn test_split_name_and_path_missing_path() {
        assert!(split_name_and_path("mydb").is_none());
        assert!(split_name_and_path("").is_none());
    }
}
