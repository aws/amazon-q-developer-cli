//! /chat command — session listing and loading

use std::io::{
    Cursor,
    Read,
    Write,
};
use std::path::Path;
use std::sync::Arc;

use agent::tui_commands::{
    ChatArgs,
    CommandOption,
    CommandResult,
};
use zip::ZipWriter;
use zip::read::ZipArchive;
use zip::write::SimpleFileOptions;

use super::CommandContext;
use crate::agent::acp::schema::SessionInfoEntry;
use crate::agent::acp::session_manager::SessionManagerHandle;
use crate::agent::session::v1_compat::V1SessionExporter;
use crate::agent::session::{
    SessionData,
    log_path,
    metadata_path,
};

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
    let mut expanded = match crate::util::paths::expand_path(ctx.os, path_str) {
        Ok(p) => p,
        Err(e) => return CommandResult::error(format!("Failed to expand path: {e}")),
    };
    if expanded.extension().is_none() {
        expanded.set_extension("zip");
    }

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

    match save_session_impl(&sessions_dir, ctx.session_id, &expanded) {
        Ok(()) => CommandResult::success(format!("Saved session to {}", expanded.display())),
        Err(e) => CommandResult::error(e),
    }
}

/// Save a session as a zip archive containing session_metadata.json and conversation_log.jsonl.
fn save_session_impl(sessions_dir: &Path, session_id: &str, output_path: &Path) -> Result<(), String> {
    let metadata = std::fs::read(metadata_path(sessions_dir, session_id))
        .map_err(|e| format!("Failed to read session metadata: {e}"))?;
    let log = std::fs::read(log_path(sessions_dir, session_id)).unwrap_or_default();

    let buf = Vec::new();
    let mut zip = ZipWriter::new(Cursor::new(buf));
    let options = SimpleFileOptions::default();

    (|| -> Result<(), Box<dyn std::error::Error>> {
        zip.start_file("session_metadata.json", options)?;
        zip.write_all(&metadata)?;
        zip.start_file("conversation_log.jsonl", options)?;
        zip.write_all(&log)?;
        Ok(())
    })()
    .map_err(|e| format!("Failed to create zip archive: {e}"))?;

    let cursor = zip
        .finish()
        .map_err(|e| format!("Failed to finalize zip archive: {e}"))?;
    std::fs::write(output_path, cursor.into_inner())
        .map_err(|e| format!("Failed to write to {}: {e}", output_path.display()))
}

async fn load_session(path_str: &str, ctx: &CommandContext<'_>) -> CommandResult {
    let expanded = match crate::util::paths::expand_path(ctx.os, path_str) {
        Ok(p) => p,
        Err(e) => return CommandResult::error(format!("Failed to expand path: {e}")),
    };

    let sessions_dir = match crate::util::paths::sessions_dir() {
        Ok(d) => d,
        Err(e) => return CommandResult::error(format!("Failed to find sessions directory: {e}")),
    };

    match load_session_impl(&expanded, &sessions_dir, ctx.cwd, ctx.v1_session_exporter) {
        Ok(session_id) => {
            let mut result = CommandResult::success(format!("Loaded session from {}", expanded.display()));
            result.data = Some(serde_json::json!({ "sessionId": session_id }));
            result
        },
        Err(e) => CommandResult::error(e),
    }
}

/// Load a session from a file path. Tries the path as-is, then with .zip/.json
/// extensions. Attempts zip, then V2 SessionData JSON, then V1 ConversationState.
fn load_session_impl(
    input_path: &Path,
    sessions_dir: &Path,
    cwd: &Path,
    v1_exporter: &Arc<dyn V1SessionExporter>,
) -> Result<String, String> {
    let data = read_with_fallback(input_path)?;
    let abs_path = std::fs::canonicalize(input_path)
        .unwrap_or_else(|_| input_path.to_path_buf())
        .to_string_lossy()
        .into_owned();

    // Try zip
    if let Ok(session_id) = load_from_zip(&data, sessions_dir, &abs_path) {
        return Ok(session_id);
    }

    // Try as text
    let content =
        std::str::from_utf8(&data).map_err(|_e| "File is not a valid zip archive or text file.".to_string())?;

    // Try V2 SessionData JSON
    if let Ok(session_data) = serde_json::from_str::<SessionData>(content) {
        let new_id = uuid::Uuid::new_v4().to_string();
        write_imported_session(sessions_dir, &new_id, &session_data, "", &abs_path)?;
        return Ok(new_id);
    }

    // Try V1 ConversationState
    let new_id = uuid::Uuid::new_v4().to_string();
    v1_exporter
        .try_export_from_json(content, &new_id, cwd, sessions_dir, Some(input_path))
        .map_err(|e| format!("Failed to import session: {e}"))?;
    Ok(new_id)
}

fn read_with_fallback(path: &Path) -> Result<Vec<u8>, String> {
    if let Ok(data) = std::fs::read(path) {
        return Ok(data);
    }
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
    if ext == "zip" || ext == "json" {
        return Err(format!("Failed to read {}", path.display()));
    }
    for suffix in &["zip", "json"] {
        if let Ok(data) = std::fs::read(path.with_extension(suffix)) {
            return Ok(data);
        }
    }
    Err(format!("Failed to read {}", path.display()))
}

fn load_from_zip(data: &[u8], sessions_dir: &Path, imported_from: &str) -> Result<String, String> {
    let mut archive = ZipArchive::new(Cursor::new(data)).map_err(|e| e.to_string())?;

    let mut metadata_str = String::new();
    archive
        .by_name("session_metadata.json")
        .map_err(|e| e.to_string())?
        .read_to_string(&mut metadata_str)
        .map_err(|e| e.to_string())?;

    let mut log = String::new();
    if let Ok(mut entry) = archive.by_name("conversation_log.jsonl") {
        entry.read_to_string(&mut log).map_err(|e| e.to_string())?;
    }

    let session_data: SessionData = serde_json::from_str(&metadata_str).map_err(|e| e.to_string())?;
    let new_id = uuid::Uuid::new_v4().to_string();
    write_imported_session(sessions_dir, &new_id, &session_data, &log, imported_from)
}

/// Write imported session files with a new session ID.
fn write_imported_session(
    sessions_dir: &Path,
    new_session_id: &str,
    original: &SessionData,
    log_content: &str,
    imported_from: &str,
) -> Result<String, String> {
    let mut session_data = original.clone();
    session_data.session_id = new_session_id.to_string();
    session_data.imported_from = Some(imported_from.to_string());
    let new_metadata = serde_json::to_string_pretty(&session_data).map_err(|e| e.to_string())?;

    let meta_path = metadata_path(sessions_dir, new_session_id);
    let log_path = log_path(sessions_dir, new_session_id);

    std::fs::write(&meta_path, &new_metadata).map_err(|e| e.to_string())?;
    std::fs::write(&log_path, log_content).map_err(|e| {
        let _ = std::fs::remove_file(&meta_path);
        e.to_string()
    })?;

    Ok(new_session_id.to_string())
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
                message_count: s.message_count,
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
