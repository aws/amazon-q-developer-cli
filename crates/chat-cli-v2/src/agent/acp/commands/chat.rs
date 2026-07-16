//! /chat command — session listing and loading

use std::io::{
    Cursor,
    Read,
    Write,
};
use std::path::Path;
use std::sync::Arc;

use agent::event_log::LogEntry;
use agent::tui_commands::{
    ChatArgs,
    CommandOption,
    CommandResult,
};
use serde::{
    Deserialize,
    Serialize,
};
use tracing::debug;
use zip::ZipWriter;
use zip::read::ZipArchive;
use zip::write::SimpleFileOptions;

use super::CommandContext;
use crate::agent::acp::schema::SessionInfoEntry;
use crate::agent::acp::session_manager::SessionManagerHandle;
use crate::agent::session::legacy_compat::LegacySessionExporter;
use crate::agent::session::{
    SessionData,
    log_path,
    metadata_path,
};

const TITLE_NOT_AVAILABLE: &str = "<title not available>";

/// Versioned export envelope, tagged by `"format"`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "format")]
pub enum ExportFormat {
    #[serde(rename = "kiro-session-export-v1")]
    KiroV1(Box<KiroV1>),
    #[serde(other)]
    Unknown,
}

/// V1 export payload: session metadata + log entries.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KiroV1 {
    pub metadata: SessionData,
    pub log_entries: Vec<LogEntry>,
}

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
            let args = super::shell_split(rest);
            let force = args.iter().any(|w| w == "--force" || w == "-f");
            let path_str = args.iter().find(|w| *w != "--force" && *w != "-f");
            let Some(path_str) = path_str else {
                return CommandResult::error("Usage: /chat save [--force] <path>");
            };
            save_session(path_str, force, ctx).await
        },
        "load" => {
            let Some(rest) = parts.get(1).filter(|s| !s.is_empty()) else {
                return CommandResult::error("Usage: /chat load <path>");
            };
            let path_str = super::strip_quotes(rest);
            if path_str.is_empty() {
                return CommandResult::error("Usage: /chat load <path>");
            }
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

    match save_session_impl(&sessions_dir, ctx.session_id, &expanded) {
        Ok(path) => CommandResult::success(format!("Saved session to {}", path.display())),
        Err(e) => CommandResult::error(e),
    }
}

/// Save a session as `kiro-session-export-v1` JSON.
/// Adds `.json` extension if none is present.
fn save_session_impl(sessions_dir: &Path, session_id: &str, output_path: &Path) -> Result<std::path::PathBuf, String> {
    let mut output_path = output_path.to_path_buf();
    if output_path.extension().is_none() {
        output_path.set_extension("json");
    }
    debug!(?sessions_dir, session_id, ?output_path, "saving session");
    let metadata_bytes = std::fs::read(metadata_path(sessions_dir, session_id))
        .map_err(|e| format!("Failed to read session metadata: {e}"))?;
    let metadata: SessionData =
        serde_json::from_slice(&metadata_bytes).map_err(|e| format!("Failed to parse session metadata: {e}"))?;

    let log_bytes = std::fs::read(log_path(sessions_dir, session_id)).unwrap_or_default();
    let log_entries: Vec<LogEntry> = String::from_utf8_lossy(&log_bytes)
        .lines()
        .filter(|l| !l.is_empty())
        .filter_map(|l| serde_json::from_str(l).ok())
        .collect();

    debug!(log_entry_count = log_entries.len(), "serializing session export");

    let export = ExportFormat::KiroV1(Box::new(KiroV1 { metadata, log_entries }));

    let content = serde_json::to_string_pretty(&export).map_err(|e| format!("Failed to serialize session: {e}"))?;
    std::fs::write(&output_path, &content).map_err(|e| format!("Failed to write to {}: {e}", output_path.display()))?;
    Ok(output_path)
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

    match load_session_impl(&expanded, &sessions_dir, ctx.cwd, ctx.legacy_session_exporter) {
        Ok(session_id) => {
            let mut result = CommandResult::success(format!("Loaded session from {}", expanded.display()));
            result.data = Some(serde_json::json!({ "sessionId": session_id }));
            result
        },
        Err(e) => CommandResult::error(e),
    }
}

/// Load a session from a file path. Tries the path as-is, then with .zip/.json
/// extensions. Funnels every recognized format through the unified
/// [`crate::agent::kas::import_session`] entry point.
fn load_session_impl(
    input_path: &Path,
    sessions_dir: &Path,
    cwd: &Path,
    v1_exporter: &Arc<dyn LegacySessionExporter>,
) -> Result<String, String> {
    debug!(?input_path, ?sessions_dir, "loading session");
    let (_data, resolved_path) = read_path_with_optional_extension(input_path)?;

    let kas_sessions_root = crate::agent::kas::default_kas_sessions_root().map_err(|e| e.to_string())?;
    let result = crate::agent::kas::import_session(
        crate::agent::kas::ImportSessionOptions {
            kas_sessions_root,
            v2_sessions_dir: sessions_dir.to_path_buf(),
            archive_path: resolved_path,
            workspace_paths: vec![cwd.to_string_lossy().into_owned()],
        },
        crate::agent::kas::ImportTarget::V2,
        v1_exporter,
    )
    .map_err(|e| e.to_string())?;
    Ok(result.session_id)
}

/// Try to read a file, falling back to `.zip` then `.json` extensions if the
/// path has no recognized extension. Returns the data and the canonicalized path.
fn read_path_with_optional_extension(path: &Path) -> Result<(Vec<u8>, std::path::PathBuf), String> {
    let (data, resolved) = read_path_raw(path)?;
    let canonical = std::fs::canonicalize(&resolved).unwrap_or(resolved);
    Ok((data, canonical))
}

fn read_path_raw(path: &Path) -> Result<(Vec<u8>, std::path::PathBuf), String> {
    if let Ok(data) = std::fs::read(path) {
        debug!(?path, "read file directly");
        return Ok((data, path.to_path_buf()));
    }
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
    if ext == "zip" || ext == "json" {
        return Err(format!("Failed to read {}", path.display()));
    }
    for suffix in &["zip", "json"] {
        let p = path.with_extension(suffix);
        if let Ok(data) = std::fs::read(&p) {
            debug!(?path, resolved = ?p, "read file with extension fallback");
            return Ok((data, p));
        }
    }
    Err(format!("Failed to read {}", path.display()))
}

fn entries_to_jsonl(entries: &[LogEntry]) -> String {
    if entries.is_empty() {
        return String::new();
    }
    let mut out: String = entries
        .iter()
        .map(|e| serde_json::to_string(e).unwrap_or_default())
        .collect::<Vec<_>>()
        .join("\n");
    out.push('\n');
    out
}

pub(crate) fn load_from_kiro(data: &[u8]) -> Result<(SessionData, String), String> {
    let export: ExportFormat = serde_json::from_slice(data).map_err(|e| format!("Failed to parse export file: {e}"))?;
    match export {
        ExportFormat::KiroV1(v1) => Ok((v1.metadata, entries_to_jsonl(&v1.log_entries))),
        ExportFormat::Unknown => Err("Unsupported export format".into()),
    }
}

pub(crate) fn load_from_zip(data: &[u8]) -> Result<(SessionData, String), String> {
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
    Ok((session_data, log))
}

/// Write imported session files with a new session ID.
pub(crate) fn write_imported_session(
    sessions_dir: &Path,
    new_session_id: &str,
    original: &SessionData,
    log_content: &str,
    imported_from: &str,
) -> Result<String, String> {
    debug!(
        new_session_id,
        imported_from,
        log_bytes = log_content.len(),
        "writing imported session"
    );
    let mut session_data = original.clone();
    session_data.session_id = new_session_id.to_string();
    session_data.imported_from = Some(imported_from.to_string());
    let new_metadata = serde_json::to_string_pretty(&session_data).map_err(|e| e.to_string())?;

    let meta_path = metadata_path(sessions_dir, new_session_id);
    let log_file_path = log_path(sessions_dir, new_session_id);

    std::fs::write(&meta_path, &new_metadata).map_err(|e| e.to_string())?;
    std::fs::write(&log_file_path, log_content).map_err(|e| {
        let _ = std::fs::remove_file(&meta_path);
        e.to_string()
    })?;

    Ok(new_session_id.to_string())
}

/// Unused outside of tests, kept here for future use e.g. `/chat save --format zip convo`
fn save_as_zip(metadata_json: &[u8], log_jsonl: &[u8]) -> Result<Vec<u8>, String> {
    let buf = Vec::new();
    let mut zip = ZipWriter::new(Cursor::new(buf));
    let options = SimpleFileOptions::default();

    (|| -> Result<(), Box<dyn std::error::Error>> {
        zip.start_file("session_metadata.json", options)?;
        zip.write_all(metadata_json)?;
        zip.start_file("conversation_log.jsonl", options)?;
        zip.write_all(log_jsonl)?;
        Ok(())
    })()
    .map_err(|e| format!("Failed to create zip archive: {e}"))?;

    let cursor = zip
        .finish()
        .map_err(|e| format!("Failed to finalize zip archive: {e}"))?;
    Ok(cursor.into_inner())
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
                message_count: Some(s.message_count),
                execution_target: None, // V2 sessions are always local
                status: None,           // V2 rows carry no activity snapshot
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

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use chrono::Utc;

    use super::*;
    use crate::agent::kas::file_detection::{
        DetectedFormat,
        detect as detect_session_file,
    };
    use crate::agent::session::legacy_compat::NoOpLegacySessionExporter;
    use crate::agent::session::{
        SessionCreatedReason,
        SessionState,
    };

    fn noop_exporter() -> Arc<dyn LegacySessionExporter> {
        Arc::new(NoOpLegacySessionExporter)
    }

    /// Base test metadata reused across all format tests.
    fn test_metadata() -> SessionData {
        SessionData {
            session_id: "test-session-id".into(),
            cwd: PathBuf::from("/tmp/test"),
            created_at: Utc::now(),
            updated_at: Utc::now(),
            title: Some("Test Session".into()),
            exported_from_v1: false,
            imported_from: None,
            parent_session_id: None,
            session_created_reason: SessionCreatedReason::Subagent,
            session_state: SessionState::Unknown,
        }
    }

    const KIRO_STR: &str = r#"{
        "format": "kiro-session-export-v1",
        "metadata": { "session_id": "s1", "cwd": "/tmp", "created_at": "2026-01-01T00:00:00Z", "updated_at": "2026-01-01T00:00:00Z", "session_state": "Unknown" },
        "log_entries": [{"version":"v1","kind":"Prompt","data":{"message_id":"m1","content":[{"kind":"text","data":"hi"}]}}]
    }"#;

    const LEGACY_STR: &str = r#"{
        "conversation_id": "abc-123",
        "history": []
    }"#;

    const V2_SESSION_DATA_STR: &str = r#"{
        "session_id": "test-session-id",
        "cwd": "/tmp/test",
        "created_at": "2026-01-01T00:00:00Z",
        "updated_at": "2026-01-01T00:00:00Z",
        "session_state": "Unknown"
    }"#;

    #[test]
    fn export_format_roundtrip() {
        let v1 = ExportFormat::KiroV1(Box::new(KiroV1 {
            metadata: test_metadata(),
            log_entries: vec![],
        }));
        let json = serde_json::to_string(&v1).unwrap();
        assert!(json.contains(r#""format":"kiro-session-export-v1""#));
        let parsed: ExportFormat = serde_json::from_str(&json).unwrap();
        assert!(matches!(parsed, ExportFormat::KiroV1(_)));
    }

    #[test]
    fn export_format_unknown() {
        let json = r#"{"format":"some-future-format","data":{}}"#;
        let parsed: ExportFormat = serde_json::from_str(json).unwrap();
        assert!(matches!(parsed, ExportFormat::Unknown));
    }

    #[test]
    fn test_detect_format() {
        let zip_data = save_as_zip(V2_SESSION_DATA_STR.as_bytes(), b"").unwrap();

        // Successful detection
        assert_eq!(
            detect_session_file(KIRO_STR.as_bytes()).unwrap(),
            DetectedFormat::KiroV1Json
        );
        assert_eq!(detect_session_file(&zip_data).unwrap(), DetectedFormat::V2Zip);
        assert_eq!(
            detect_session_file(LEGACY_STR.as_bytes()).unwrap(),
            DetectedFormat::LegacyV1
        );

        // Error cases
        assert!(detect_session_file(br#"{"random":true}"#).is_err());
        assert!(detect_session_file(b"not a zip").is_err());
        // Bare V2 SessionData JSON (only `session_id`) is not a
        // recognized format.
        assert!(detect_session_file(V2_SESSION_DATA_STR.as_bytes()).is_err());
    }

    fn make_jsonl(entries: &[serde_json::Value]) -> String {
        entries
            .iter()
            .map(|e| serde_json::to_string(e).unwrap())
            .collect::<Vec<_>>()
            .join("\n")
            + "\n"
    }

    #[test]
    fn save_and_load_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let sessions_dir = dir.path().join("sessions");
        std::fs::create_dir_all(&sessions_dir).unwrap();

        let metadata = test_metadata();
        let log_entry = serde_json::json!({"version":"v1","kind":"Prompt","data":{"message_id":"m1","content":[{"kind":"text","data":"hi"}]}});

        // Write source session files
        std::fs::write(
            metadata_path(&sessions_dir, &metadata.session_id),
            serde_json::to_string_pretty(&metadata).unwrap(),
        )
        .unwrap();
        std::fs::write(log_path(&sessions_dir, &metadata.session_id), make_jsonl(&[log_entry])).unwrap();

        // Save
        // Save with no extension — should default to .json
        let export_path = dir.path().join("export");
        let saved_path = save_session_impl(&sessions_dir, &metadata.session_id, &export_path).unwrap();
        assert_eq!(saved_path.extension().unwrap(), "json");

        // Verify saved file is Kiro format
        let saved = std::fs::read(&saved_path).unwrap();
        assert_eq!(detect_session_file(&saved).unwrap(), DetectedFormat::KiroV1Json);

        // Load into fresh sessions dir
        let import_sessions = dir.path().join("import_sessions");
        std::fs::create_dir_all(&import_sessions).unwrap();
        let new_id = load_session_impl(&saved_path, &import_sessions, Path::new("/tmp"), &noop_exporter()).unwrap();

        let imported_meta: SessionData =
            serde_json::from_str(&std::fs::read_to_string(metadata_path(&import_sessions, &new_id)).unwrap()).unwrap();
        assert_eq!(imported_meta.session_id, new_id);
        assert_eq!(imported_meta.title, metadata.title);
        assert!(imported_meta.imported_from.is_some());
        assert!(
            !std::fs::read_to_string(log_path(&import_sessions, &new_id))
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn load_zip_format() {
        let dir = tempfile::tempdir().unwrap();
        let sessions_dir = dir.path().join("sessions");
        std::fs::create_dir_all(&sessions_dir).unwrap();

        let zip_bytes = save_as_zip(serde_json::to_string_pretty(&test_metadata()).unwrap().as_bytes(), b"").unwrap();
        let zip_path = dir.path().join("session.zip");
        std::fs::write(&zip_path, &zip_bytes).unwrap();

        let new_id = load_session_impl(&zip_path, &sessions_dir, Path::new("/tmp"), &noop_exporter()).unwrap();
        let imported: SessionData =
            serde_json::from_str(&std::fs::read_to_string(metadata_path(&sessions_dir, &new_id)).unwrap()).unwrap();
        assert_eq!(imported.session_id, new_id);
        assert!(imported.imported_from.is_some());
    }
}
