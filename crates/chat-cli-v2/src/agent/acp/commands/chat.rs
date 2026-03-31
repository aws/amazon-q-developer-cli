//! /chat command — session listing and loading

use agent::tui_commands::{
    ChatArgs,
    CommandOption,
    CommandResult,
};

use super::CommandContext;
use crate::agent::acp::schema::SessionInfoEntry;
use crate::agent::acp::session_manager::SessionManagerHandle;

const TITLE_NOT_AVAILABLE: &str = "<title not available>";

pub async fn execute(args: &ChatArgs, ctx: &CommandContext<'_>) -> CommandResult {
    let Some(ref subcommand) = args.subcommand else {
        return CommandResult::success("");
    };

    let parts: Vec<&str> = subcommand.splitn(2, ' ').collect();
    match parts[0] {
        "save" => {
            let Some(rest) = parts.get(1).filter(|s| !s.is_empty()) else {
                return CommandResult::error("Usage: /chat save [--force] <path>");
            };
            let words: Vec<&str> = rest.split_whitespace().collect();
            let force = words.iter().any(|w| *w == "--force" || *w == "-f");
            let path_str = words.iter().find(|w| **w != "--force" && **w != "-f");
            let Some(path_str) = path_str else {
                return CommandResult::error("Usage: /chat save [--force] <path>");
            };
            save_session(path_str, force, ctx).await
        },
        "load" => {
            let Some(path_str) = parts.get(1).filter(|s| !s.is_empty()) else {
                return CommandResult::error("Usage: /chat load <path>");
            };
            load_session(path_str, ctx).await
        },
        _ => CommandResult::error(format!(
            "Unknown subcommand: {}. Use: save <path>, load <path>",
            parts[0]
        )),
    }
}

async fn save_session(path_str: &str, force: bool, ctx: &CommandContext<'_>) -> CommandResult {
    let expanded = match crate::util::paths::expand_path(ctx.os, path_str) {
        Ok(p) => p,
        Err(e) => return CommandResult::error(format!("Failed to expand path: {e}")),
    };

    if expanded.exists() && !force {
        return CommandResult::error(format!(
            "File already exists: {}. Use --force to overwrite.",
            expanded.display()
        ));
    }

    let sessions_dir = match crate::util::paths::sessions_dir() {
        Ok(d) => d,
        Err(e) => return CommandResult::error(format!("Failed to find sessions directory: {e}")),
    };

    // Read session metadata
    let meta_path = sessions_dir.join(format!("{}.json", ctx.session_id));
    let metadata = match std::fs::read_to_string(&meta_path) {
        Ok(c) => c,
        Err(e) => return CommandResult::error(format!("Failed to read session metadata: {e}")),
    };

    // Read session log
    let log_path = sessions_dir.join(format!("{}.jsonl", ctx.session_id));
    let log = std::fs::read_to_string(&log_path).unwrap_or_default();

    // Bundle into a single export file
    let export = serde_json::json!({
        "format": "kiro-session-export-v1",
        "metadata": serde_json::from_str::<serde_json::Value>(&metadata).unwrap_or_default(),
        "log_entries": log.lines().filter(|l| !l.is_empty()).map(|l| {
            serde_json::from_str::<serde_json::Value>(l).unwrap_or_default()
        }).collect::<Vec<_>>(),
    });

    let content = match serde_json::to_string_pretty(&export) {
        Ok(c) => c,
        Err(e) => return CommandResult::error(format!("Failed to serialize session: {e}")),
    };

    if let Err(e) = std::fs::write(&expanded, &content) {
        return CommandResult::error(format!("Failed to write to {}: {e}", expanded.display()));
    }

    CommandResult::success(format!("Saved session to {}", expanded.display()))
}

async fn load_session(path_str: &str, ctx: &CommandContext<'_>) -> CommandResult {
    let expanded = match crate::util::paths::expand_path(ctx.os, path_str) {
        Ok(p) => p,
        Err(e) => return CommandResult::error(format!("Failed to expand path: {e}")),
    };

    // Try original path, then with .json suffix
    let content = match std::fs::read_to_string(&expanded) {
        Ok(c) => c,
        Err(_) if !path_str.ends_with(".json") => {
            let json_path = expanded.with_extension("json");
            match std::fs::read_to_string(&json_path) {
                Ok(c) => c,
                Err(e) => return CommandResult::error(format!("Failed to read {}: {e}", expanded.display())),
            }
        },
        Err(e) => return CommandResult::error(format!("Failed to read {}: {e}", expanded.display())),
    };

    let export: serde_json::Value = match serde_json::from_str(&content) {
        Ok(v) => v,
        Err(e) => return CommandResult::error(format!("Failed to parse session file: {e}")),
    };

    // Validate format
    if export.get("format").and_then(|v| v.as_str()) != Some("kiro-session-export-v1") {
        return CommandResult::error("Invalid session file format. Expected kiro-session-export-v1.");
    }

    let sessions_dir = match crate::util::paths::sessions_dir() {
        Ok(d) => d,
        Err(e) => return CommandResult::error(format!("Failed to find sessions directory: {e}")),
    };

    // Generate a new session ID for the import
    let new_session_id = uuid::Uuid::new_v4().to_string();

    // Write metadata with new session ID
    let mut metadata = export.get("metadata").cloned().unwrap_or_default();
    if let Some(obj) = metadata.as_object_mut() {
        obj.insert(
            "session_id".to_string(),
            serde_json::Value::String(new_session_id.clone()),
        );
    }
    let meta_path = sessions_dir.join(format!("{new_session_id}.json"));
    if let Err(e) = std::fs::write(&meta_path, serde_json::to_string_pretty(&metadata).unwrap_or_default()) {
        return CommandResult::error(format!("Failed to write session metadata: {e}"));
    }

    // Write log entries
    let log_path = sessions_dir.join(format!("{new_session_id}.jsonl"));
    let log_entries = export
        .get("log_entries")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    let log_content: String = log_entries
        .iter()
        .map(|e| serde_json::to_string(e).unwrap_or_default())
        .collect::<Vec<_>>()
        .join("\n");
    let log_content = if log_content.is_empty() {
        log_content
    } else {
        format!("{log_content}\n")
    };
    if let Err(e) = std::fs::write(&log_path, &log_content) {
        // Clean up metadata file on failure
        let _ = std::fs::remove_file(&meta_path);
        return CommandResult::error(format!("Failed to write session log: {e}"));
    }

    let mut result = CommandResult::success(format!("Loaded session from {}", expanded.display()));
    result.data = Some(serde_json::json!({ "sessionId": new_session_id }));
    result
}

/// List sessions with title backfill. Shared by both the `_kiro.dev/session/list`
/// extension handler and the `getCommandOptions` path.
pub async fn list_sessions(
    session_manager: &SessionManagerHandle,
    cwd: Option<std::path::PathBuf>,
) -> Result<Vec<SessionInfoEntry>, sacp::Error> {
    let sessions = session_manager.list_sessions(cwd).await?;
    let sessions_dir = crate::util::paths::sessions_dir().ok();
    Ok(sessions
        .into_iter()
        .map(|s| {
            let title = s.title.or_else(|| {
                sessions_dir
                    .as_ref()
                    .and_then(|d| crate::agent::session::title_from_first_log_entry(d, &s.session_id))
            });
            SessionInfoEntry {
                session_id: s.session_id,
                cwd: s.cwd,
                title,
                updated_at: Some(s.updated_at.to_rfc3339()),
            }
        })
        .collect())
}

impl From<SessionInfoEntry> for CommandOption {
    fn from(s: SessionInfoEntry) -> Self {
        let label = format!(
            "{} ({})",
            s.title.as_deref().unwrap_or(TITLE_NOT_AVAILABLE),
            agent::util::truncate_safe(&s.session_id, 8),
        );
        let description = s.updated_at.map(|t| {
            crate::util::format_relative_time(&t.parse::<chrono::DateTime<chrono::Utc>>().unwrap_or_default())
        });
        CommandOption {
            value: s.session_id,
            label,
            description,
            group: None,
            hint: None,
        }
    }
}
