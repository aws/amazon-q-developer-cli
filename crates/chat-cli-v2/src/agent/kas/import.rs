//! Import side: extract a KAS session zip into the local sessions
//! directory, generating a fresh id and rebucketing under the caller's
//! workspace hash.

use std::fs;
use std::path::{
    Path,
    PathBuf,
};
use std::sync::Arc;

use agent::event_log::LogEntry;
use chrono::Utc;
use uuid::Uuid;

use super::file_detection::DetectedFormat;
use super::persist::{
    WriteKasSessionError,
    WriteKasSessionOptions,
    write_kas_session_dir,
};
use super::schema::{
    CURRENT_SCHEMA_VERSION,
    SUPPORTED_SCHEMA_VERSIONS,
    SessionMetadata,
};
use super::session_id::generate_kas_session_id;
use super::shared::SessionArchiveError;
use super::v2_to_kas::{
    ConvertArgs as V2ConvertArgs,
    convert_v2_to_kas,
};
use super::workspace_hash::compute_workspace_hash;
use super::zip_util::{
    decode_zip,
    safe_resolve,
};
use crate::agent::acp::commands::chat::{
    ExportFormat,
    KiroV1,
};
use crate::agent::session::legacy_compat::LegacySessionExporter;
use crate::agent::session::{
    SessionData,
    SessionDb,
};

/// Sibling of the per-workspace bucket dirs under `kas_sessions_root`.
/// In-progress imports are extracted under this dir and renamed into
/// their final bucket on success. Living outside any bucket prevents
/// KAS's session lister from enumerating in-flight or orphaned imports.
const IMPORT_STAGING_DIR: &str = ".import-staging";

/// Fields KAS persists that reference an id from the SOURCE
/// environment. They have no meaning in the destination's id space
/// and must not survive import: a dangling `parentSessionId` would
/// link the imported session to a parent that doesn't exist
/// locally; a dangling `lastCheckpointId` would crash KAS's resolver
/// when it walks the snapshot tree on resume.
///
/// Anything else KAS persists round-trips through
/// [`SessionMetadata::extra`] verbatim, so user data and
/// non-id-coupled metadata (`effortLevel`, `semanticReviewEnabled`,
/// future fields, etc.) survive import.
const SOURCE_ID_COUPLED_FIELDS: &[&str] = &["parentSessionId", "parentExecutionId", "lastCheckpointId"];

/// Parse `bytes` as a [`SessionMetadata`], validating that the schema
/// version is in [`SUPPORTED_SCHEMA_VERSIONS`].
fn parse_session_metadata(bytes: &[u8]) -> Result<SessionMetadata, SessionArchiveError> {
    let metadata: SessionMetadata =
        serde_json::from_slice(bytes).map_err(|e| SessionArchiveError::msg(format!("session.json is invalid: {e}")))?;
    if !SUPPORTED_SCHEMA_VERSIONS.contains(&metadata.schema_version.as_str()) {
        return Err(SessionArchiveError::msg(format!(
            "session.json has unsupported schemaVersion {:?} (supported: {})",
            metadata.schema_version,
            SUPPORTED_SCHEMA_VERSIONS.join(", "),
        )));
    }
    Ok(metadata)
}

/// Options for [`import_session`].
pub struct ImportSessionOptions {
    /// Root sessions directory, e.g. `~/.kiro/sessions`. Used when
    /// the import target is [`ImportTarget::Kas`].
    pub kas_sessions_root: PathBuf,
    /// V2 sessions directory, e.g. `~/.kiro/sessions/cli`. Used
    /// when the import target is [`ImportTarget::V2`] and as the
    /// staging location for the V2 byproduct of a V1 -> KAS
    /// conversion.
    pub v2_sessions_dir: PathBuf,
    /// Path to the archive file to import.
    pub archive_path: PathBuf,
    /// Workspace paths to bucket the imported session under. For
    /// KAS targets, `session.json`'s `workspacePaths` field is
    /// rewritten to these and the directory lands at
    /// `{kas_sessions_root}/{compute_workspace_hash(workspace_paths)}/{new_id}/`.
    /// For V2 targets, the first entry is recorded as the session
    /// `cwd`.
    pub workspace_paths: Vec<String>,
}

/// Where the imported session is written.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImportTarget {
    /// Write the imported session to V2 storage as flat
    /// `{v2_sessions_dir}/{id}.{json,jsonl}` files.
    V2,
    /// Write the imported session to KAS storage as
    /// `{kas_sessions_root}/{wsHash}/{id}/{session.json,messages.jsonl}`.
    Kas,
}

/// Result of a successful [`import_session`] call.
#[derive(Debug)]
pub struct ImportResult {
    /// The new session id assigned by the importer. For KAS targets
    /// this is the directory name under the workspace bucket; for
    /// V2 targets this is the basename of the flat-file pair.
    pub session_id: String,
    /// The on-disk path of the imported session. KAS target: the
    /// session directory. V2 target: the metadata JSON file.
    pub path: PathBuf,
}

/// Imports the session given by [ImportSessionOptions::archive_path] to [ImportTarget], creating a
/// new session id for the imported session.
///
/// Returns the new session id and the on-disk path of the imported
/// session (a directory for KAS, a metadata JSON file for V2).
pub fn import_session(
    opts: ImportSessionOptions,
    target: ImportTarget,
    legacy_exporter: &Arc<dyn LegacySessionExporter>,
) -> Result<ImportResult, SessionArchiveError> {
    let archive_bytes = fs::read(&opts.archive_path)
        .map_err(|e| SessionArchiveError::io(format!("failed to read archive: {}", opts.archive_path.display()), e))?;

    let format = super::file_detection::detect(&archive_bytes).map_err(SessionArchiveError::msg)?;

    match (format, target) {
        (DetectedFormat::KasZip, ImportTarget::Kas) => import_kas_zip(opts, archive_bytes),
        (DetectedFormat::KasZip, ImportTarget::V2) => Err(SessionArchiveError::msg("Unsupported source format")),
        (DetectedFormat::KiroV1Json, ImportTarget::Kas) => import_v2_kiro_v1_json(opts, archive_bytes),
        (DetectedFormat::KiroV1Json, ImportTarget::V2) => import_v2_kiro_v1_json_to_v2(opts, archive_bytes),
        (DetectedFormat::V2Zip, ImportTarget::Kas) => import_v2_zip(opts, archive_bytes),
        (DetectedFormat::V2Zip, ImportTarget::V2) => import_v2_zip_to_v2(opts, archive_bytes),
        (DetectedFormat::LegacyV1, ImportTarget::Kas) => import_v1_to_kas(opts, archive_bytes, legacy_exporter),
        (DetectedFormat::LegacyV1, ImportTarget::V2) => import_v1_to_v2(opts, archive_bytes, legacy_exporter),
    }
}

/// Extract a KAS-format zip archive into the local sessions
/// directory.
///
/// Generates a fresh KAS session id (`cli_<uuid>`), rewrites
/// `session.json`'s `workspacePaths` to `opts.workspace_paths`, drops
/// every metadata field outside the import allowlist (so parent /
/// checkpoint linkage and any unknown future fields do not survive),
/// and places the extracted directory under
/// `{kas_sessions_root}/{compute_workspace_hash(workspace_paths)}/{new_id}/`.
///
/// Steps:
///   1. Decode zip + zip-slip-guard every entry name.
///   2. Require + parse session.json (allowlist parse - unknown fields silently dropped).
///   3. Validate the parsed metadata against the typed schema.
///   4. Generate a fresh session id; rewrite `workspacePaths`, `schemaVersion`, and
///      `lastModifiedAt`.
///   5. Stage all entries (with rewritten session.json) under
///      `{kas_sessions_root}/.import-staging/{uuid}/` so a process killed mid-import never leaves
///      an orphan that KAS's session lister enumerates.
///   6. Atomic rename into the final `{newHash}/{newId}/` location.
///
/// On failure, the staging directory is cleaned up.
fn import_kas_zip(opts: ImportSessionOptions, archive_bytes: Vec<u8>) -> Result<ImportResult, SessionArchiveError> {
    // 1. Decode the archive.
    let entries = decode_zip(&archive_bytes)?;

    // 2. session.json must be present.
    let session_json_bytes = entries
        .get("session.json")
        .ok_or_else(|| SessionArchiveError::msg("archive does not contain session.json"))?;

    // 3. Parse + validate session.json.
    let mut metadata = parse_session_metadata(session_json_bytes)?;

    // 4. Rewrite metadata for the new session. Strip the source-id-coupled fields enumerated in
    //    `SOURCE_ID_COUPLED_FIELDS`; the rest round-trips via `metadata.extra`.
    //
    // KAS-zip imports do not preserve the source archive's session id. The shared id generator
    // produces a `cli_<uuid>` form when no source id is supplied, distinguishing imports from
    // KAS-native `sess_<uuid>` sessions and from V2 conversions (which encode the V2 id in the
    // suffix).
    let new_id = generate_kas_session_id(None);
    let new_hash = compute_workspace_hash(&opts.workspace_paths);
    metadata.id = new_id.clone();
    metadata.schema_version = CURRENT_SCHEMA_VERSION.to_string();
    metadata.workspace_paths = opts.workspace_paths.clone();
    metadata.last_modified_at = Utc::now().to_rfc3339();
    for field in SOURCE_ID_COUPLED_FIELDS {
        metadata.extra.remove(*field);
    }
    // Pretty-print to match KAS's own session.json convention
    // (`JSON.stringify(meta, null, 2)` in session-persistence.ts).
    let rewritten_bytes = serde_json::to_vec_pretty(&metadata)
        .map_err(|e| SessionArchiveError::io("serialize rewritten session.json", std::io::Error::other(e)))?;

    // 5. Stage extraction outside the workspace bucket.
    //
    // KAS's session lister walks every subdirectory of a bucket and
    // tries to read `session.json` from each. Staging inside the
    // bucket means a process killed mid-import (OOM, SIGKILL, power
    // loss) leaves an orphan that KAS surfaces as a warning on every
    // subsequent listing. The dedicated `.import-staging/` sibling
    // directory is never enumerated by KAS.
    let bucket_dir = opts.kas_sessions_root.join(&new_hash);
    let staging_dir = opts
        .kas_sessions_root
        .join(IMPORT_STAGING_DIR)
        .join(Uuid::new_v4().to_string());

    let result = (|| -> Result<ImportResult, SessionArchiveError> {
        fs::create_dir_all(&staging_dir)
            .map_err(|e| SessionArchiveError::io(format!("create staging dir {}", staging_dir.display()), e))?;

        for (name, bytes) in &entries {
            // Skip directory markers (zip format allows entries that
            // are pure directories, indicated by trailing slash).
            if name.ends_with('/') {
                continue;
            }
            let target = safe_resolve(&staging_dir, name)?;
            if let Some(parent) = target.parent() {
                fs::create_dir_all(parent)
                    .map_err(|e| SessionArchiveError::io(format!("create dir {}", parent.display()), e))?;
            }
            let body: &[u8] = if name == "session.json" {
                &rewritten_bytes
            } else {
                bytes
            };
            fs::write(&target, body).map_err(|e| SessionArchiveError::io(format!("write {}", target.display()), e))?;
        }

        // 6. Atomic rename into final location.
        let final_dir = bucket_dir.join(&new_id);
        // The bucket dir might not exist yet for first-import-into-this
        // -workspace; rename requires the parent.
        fs::create_dir_all(&bucket_dir)
            .map_err(|e| SessionArchiveError::io(format!("create bucket dir {}", bucket_dir.display()), e))?;
        fs::rename(&staging_dir, &final_dir).map_err(|e| {
            SessionArchiveError::io(
                format!("rename {} -> {}", staging_dir.display(), final_dir.display()),
                e,
            )
        })?;
        Ok(ImportResult {
            session_id: new_id.clone(),
            path: final_dir,
        })
    })();

    if result.is_err() {
        // Best-effort cleanup; ignore errors here so the original
        // failure surfaces.
        let _ = fs::remove_dir_all(&staging_dir);
    }
    result
}

/// Convert a V2 portable export JSON (`kiro-session-export-v1`) into
/// a fresh KAS session.
///
/// Parses the top-level envelope into V2
/// `(SessionData, Vec<LogEntry>)` and funnels the result through
/// [`convert_v2_session_and_write`] which the V2-zip path also uses.
fn import_v2_kiro_v1_json(
    opts: ImportSessionOptions,
    archive_bytes: Vec<u8>,
) -> Result<ImportResult, SessionArchiveError> {
    let parsed: ExportFormat = serde_json::from_slice(&archive_bytes)
        .map_err(|e| SessionArchiveError::msg(format!("failed to parse V2 export JSON: {e}")))?;
    let payload: KiroV1 = match parsed {
        ExportFormat::KiroV1(boxed) => *boxed,
        ExportFormat::Unknown => {
            return Err(SessionArchiveError::msg(
                "V2 export JSON has unrecognized `format` field",
            ));
        },
    };
    convert_v2_session_and_write(payload.metadata, payload.log_entries, opts)
}

/// Convert a V2 zip archive (with `session_metadata.json` +
/// `conversation_log.jsonl` at the root) into a fresh KAS session.
///
/// Reads the two manifest files out of the zip, parses them into V2
/// `SessionData` and `Vec<LogEntry>`, and funnels the result through
/// [`convert_v2_session_and_write`] which the JSON path also uses.
fn import_v2_zip(opts: ImportSessionOptions, archive_bytes: Vec<u8>) -> Result<ImportResult, SessionArchiveError> {
    let entries = decode_zip(&archive_bytes)?;
    let metadata_bytes = entries
        .get("session_metadata.json")
        .ok_or_else(|| SessionArchiveError::msg("V2 zip does not contain session_metadata.json"))?;
    let session: SessionData = serde_json::from_slice(metadata_bytes)
        .map_err(|e| SessionArchiveError::msg(format!("failed to parse session_metadata.json: {e}")))?;

    // The log file is optional - V2's `save_as_zip` writes an empty
    // `conversation_log.jsonl` for sessions that have no entries yet.
    // Treat a missing entry the same as an empty one.
    let log_bytes = entries.get("conversation_log.jsonl").map_or(&[][..], Vec::as_slice);
    let log_entries = parse_v2_log_jsonl(log_bytes)?;

    convert_v2_session_and_write(session, log_entries, opts)
}

/// Parse a V2 `conversation_log.jsonl` blob (one [`LogEntry`] per
/// line; trailing newline tolerated). Empty lines are skipped to
/// match V2's tolerant reader.
fn parse_v2_log_jsonl(bytes: &[u8]) -> Result<Vec<LogEntry>, SessionArchiveError> {
    let text =
        std::str::from_utf8(bytes).map_err(|e| SessionArchiveError::msg(format!("conversation_log.jsonl: {e}")))?;
    let mut entries = Vec::new();
    for (idx, line) in text.lines().enumerate() {
        if line.is_empty() {
            continue;
        }
        let entry: LogEntry = serde_json::from_str(line).map_err(|e| {
            SessionArchiveError::msg(format!(
                "conversation_log.jsonl line {}: failed to parse log entry: {e}",
                idx + 1
            ))
        })?;
        entries.push(entry);
    }
    Ok(entries)
}

/// Run the V2 -> KAS converter on a parsed V2 session and persist
/// the result. Shared by every V2-source import path
/// ([`import_v2_kiro_v1_json`], [`import_v2_zip`]) so the conversion
/// + write logic lives in exactly one place.
///
/// The new id encodes the source V2 session id so the resume-id
/// idempotency probe can recognize prior conversions of the same V2
/// session and reuse the existing KAS directory.
fn convert_v2_session_and_write(
    session: SessionData,
    entries: Vec<LogEntry>,
    opts: ImportSessionOptions,
) -> Result<ImportResult, SessionArchiveError> {
    let output = convert_v2_to_kas(V2ConvertArgs {
        session,
        entries,
        workspace_paths: opts.workspace_paths.clone(),
    })
    .map_err(|e| SessionArchiveError::msg(format!("V2 -> KAS conversion failed: {e}")))?;

    let session_id = output.metadata.id.clone();
    let path = write_kas_session_dir(WriteKasSessionOptions {
        kas_sessions_root: opts.kas_sessions_root,
        workspace_paths: opts.workspace_paths,
        session_id: session_id.clone(),
        metadata: output.metadata,
        messages: output.messages,
    })
    .map_err(|e| match e {
        WriteKasSessionError::AlreadyExists(p) => {
            SessionArchiveError::msg(format!("target session directory already exists: {}", p.display()))
        },
        other => SessionArchiveError::msg(format!("failed to write KAS session: {other}")),
    })?;
    Ok(ImportResult { session_id, path })
}

/// Convert a legacy V1 `ConversationState` JSON into a fresh KAS
/// session.
///
/// Composes V1 -> V2 -> KAS through a tempdir: the V2 byproduct
/// uses the V1 conversation id as the V2 session id (so the
/// downstream V2 -> KAS converter encodes that id into the KAS id
/// as `cli_<v1_id>_<random>`), then it is read back and the V2 ->
/// KAS converter writes the final KAS session under the caller's
/// workspace bucket. The tempdir is dropped on return so the V2
/// byproduct never surfaces in the user's V2 listing.
fn import_v1_to_kas(
    opts: ImportSessionOptions,
    archive_bytes: Vec<u8>,
    legacy_exporter: &Arc<dyn LegacySessionExporter>,
) -> Result<ImportResult, SessionArchiveError> {
    let content = std::str::from_utf8(&archive_bytes)
        .map_err(|e| SessionArchiveError::msg(format!("V1 archive is not valid UTF-8: {e}")))?;

    let v1_id = extract_v1_conversation_id(content)?;

    let temp = tempfile::tempdir().map_err(|e| SessionArchiveError::io("create temp V2 dir for V1 import", e))?;

    let cwd_str = opts
        .workspace_paths
        .first()
        .ok_or_else(|| SessionArchiveError::msg("import_session: no workspace paths supplied"))?
        .clone();
    let cwd = Path::new(&cwd_str);

    legacy_exporter
        .try_export_from_json(content, &v1_id, cwd, temp.path(), Some(&opts.archive_path))
        .map_err(|e| SessionArchiveError::msg(format!("V1 -> V2 export failed: {e}")))?;

    let db = SessionDb::load_with_sessions_dir(temp.path(), &v1_id, None)
        .map_err(|e| SessionArchiveError::msg(format!("read V2 byproduct: {e}")))?;
    let session_data = db.session().clone();
    let entries = db
        .load_log_entries()
        .map_err(|e| SessionArchiveError::msg(format!("load V2 byproduct entries: {e}")))?;

    convert_v2_session_and_write(session_data, entries, opts)
}

/// Extract the `conversation_id` field from a V1 `ConversationState`
/// JSON. Used as the V2 byproduct id during V1 -> KAS import so the
/// resulting KAS id deterministically encodes the V1 source id.
fn extract_v1_conversation_id(content: &str) -> Result<String, SessionArchiveError> {
    let parsed: serde_json::Value =
        serde_json::from_str(content).map_err(|e| SessionArchiveError::msg(format!("V1 JSON parse: {e}")))?;
    let id = parsed
        .get("conversation_id")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| SessionArchiveError::msg("V1 JSON missing string `conversation_id` field"))?;
    Ok(id.to_string())
}

/// Convert a legacy V1 `ConversationState` JSON into a fresh V2
/// session under `opts.v2_sessions_dir`. Mints a new UUID for the
/// V2 session id; the V1 conversation id is recorded inside the
/// converted session metadata only.
fn import_v1_to_v2(
    opts: ImportSessionOptions,
    archive_bytes: Vec<u8>,
    legacy_exporter: &Arc<dyn LegacySessionExporter>,
) -> Result<ImportResult, SessionArchiveError> {
    let content = std::str::from_utf8(&archive_bytes)
        .map_err(|e| SessionArchiveError::msg(format!("V1 archive is not valid UTF-8: {e}")))?;
    let cwd_str = opts
        .workspace_paths
        .first()
        .ok_or_else(|| SessionArchiveError::msg("import_session: no workspace paths supplied"))?
        .clone();
    let cwd = Path::new(&cwd_str);
    let new_id = uuid::Uuid::new_v4().to_string();

    legacy_exporter
        .try_export_from_json(content, &new_id, cwd, &opts.v2_sessions_dir, Some(&opts.archive_path))
        .map_err(|e| SessionArchiveError::msg(format!("V1 -> V2 export failed: {e}")))?;

    Ok(ImportResult {
        path: crate::agent::session::metadata_path(&opts.v2_sessions_dir, &new_id),
        session_id: new_id,
    })
}

/// Convert a V2 portable export JSON into V2 flat-file storage,
/// minting a fresh V2 session id.
fn import_v2_kiro_v1_json_to_v2(
    opts: ImportSessionOptions,
    archive_bytes: Vec<u8>,
) -> Result<ImportResult, SessionArchiveError> {
    let (session_data, log_content) =
        crate::agent::acp::commands::chat::load_from_kiro(&archive_bytes).map_err(SessionArchiveError::msg)?;
    write_v2_imported(&opts, &session_data, &log_content)
}

/// Convert a V2 zip archive into V2 flat-file storage, minting a
/// fresh V2 session id.
fn import_v2_zip_to_v2(
    opts: ImportSessionOptions,
    archive_bytes: Vec<u8>,
) -> Result<ImportResult, SessionArchiveError> {
    let (session_data, log_content) =
        crate::agent::acp::commands::chat::load_from_zip(&archive_bytes).map_err(SessionArchiveError::msg)?;
    write_v2_imported(&opts, &session_data, &log_content)
}

fn write_v2_imported(
    opts: &ImportSessionOptions,
    session_data: &SessionData,
    log_content: &str,
) -> Result<ImportResult, SessionArchiveError> {
    let new_id = uuid::Uuid::new_v4().to_string();
    let imported_from = opts.archive_path.to_string_lossy().into_owned();
    let session_id = crate::agent::acp::commands::chat::write_imported_session(
        &opts.v2_sessions_dir,
        &new_id,
        session_data,
        log_content,
        &imported_from,
    )
    .map_err(SessionArchiveError::msg)?;
    Ok(ImportResult {
        path: crate::agent::session::metadata_path(&opts.v2_sessions_dir, &session_id),
        session_id,
    })
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::path::Path;

    use serde_json::{
        Value,
        json,
    };
    use uuid::Uuid;

    use super::super::test_support::{
        build_default_kas_zip,
        build_zip,
        default_metadata,
        default_session_files,
        included_session_files,
        read_session_json,
        temp_base,
        write_archive,
        write_kas_session,
    };
    use super::*;
    use crate::agent::kas::export::{
        ExportSessionOptions,
        export_session,
    };
    use crate::agent::session::legacy_compat::NoOpLegacySessionExporter;

    /// In-file tests do not exercise the V1 -> KAS path, so a no-op
    /// exporter satisfies the trait without pulling in chat-cli's
    /// SQLite-backed implementation.
    fn no_v1_exporter() -> Arc<dyn LegacySessionExporter> {
        Arc::new(NoOpLegacySessionExporter)
    }

    /// Build a zip containing a single `session.json` with the
    /// given metadata, write it to `dir`, and run import. The test's
    /// session bucket lands under `{dir}/sessions/`.
    fn import_metadata(dir: &Path, metadata: &Value) -> Result<PathBuf, SessionArchiveError> {
        import_session_json_bytes(dir, &serde_json::to_vec(metadata).unwrap())
    }

    /// Like [`import_metadata`] but takes raw `session.json` bytes so
    /// tests can exercise non-JSON / non-object payloads.
    fn import_session_json_bytes(dir: &Path, bytes: &[u8]) -> Result<PathBuf, SessionArchiveError> {
        let mut contents = BTreeMap::new();
        contents.insert("session.json".to_string(), bytes.to_vec());
        let archive_path = write_archive(dir, &build_zip(&contents));
        import_session(
            ImportSessionOptions {
                kas_sessions_root: dir.join("sessions"),
                v2_sessions_dir: PathBuf::from("/tmp/v2_unused_in_kas_target"),
                archive_path,
                workspace_paths: vec!["/x".to_string()],
            },
            ImportTarget::Kas,
            &no_v1_exporter(),
        )
        .map(|r| r.path)
    }
    // -- import_session: happy path --

    #[test]
    fn import_rewrites_metadata_and_extracts_files() {
        let dir = temp_base();
        let sessions_root = dir.path().join("sessions");
        let workspace = vec!["/foo".to_string(), "/bar".to_string()];
        let (zip_bytes, original_id) = build_default_kas_zip();
        let archive_path = write_archive(dir.path(), &zip_bytes);

        let imported = import_session(
            ImportSessionOptions {
                kas_sessions_root: sessions_root.clone(),
                v2_sessions_dir: PathBuf::from("/tmp/v2_unused_in_kas_target"),
                archive_path,
                workspace_paths: workspace.clone(),
            },
            ImportTarget::Kas,
            &no_v1_exporter(),
        )
        .unwrap()
        .path;

        // Bucketed under the destination workspace's hash.
        assert!(imported.starts_with(sessions_root.join(compute_workspace_hash(&workspace))));

        let session = read_session_json(&imported);

        // Fresh UUID with `cli_` prefix, distinct from source, matches the directory name.
        let new_id = session.get("id").and_then(Value::as_str).unwrap();
        assert_ne!(new_id, original_id);
        let uuid_part = new_id.strip_prefix("cli_").expect("imported id must have cli_ prefix");
        assert!(Uuid::parse_str(uuid_part).is_ok());
        assert_eq!(imported.file_name().and_then(|s| s.to_str()).unwrap(), new_id);

        // workspacePaths rewritten to the destination.
        let paths: Vec<&str> = session
            .get("workspacePaths")
            .and_then(Value::as_array)
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect();
        assert_eq!(paths, vec!["/foo", "/bar"]);

        // lastModifiedAt refreshed to a fresh, valid timestamp.
        let last = session.get("lastModifiedAt").and_then(Value::as_str).unwrap();
        assert_ne!(last, "2025-01-01T00:00:00.000Z");
        chrono::DateTime::parse_from_rfc3339(last).expect("valid timestamp");

        // schemaVersion stamped to current.
        assert_eq!(
            session.get("schemaVersion").and_then(Value::as_str),
            Some(CURRENT_SCHEMA_VERSION)
        );

        // Unknown / future fields round-trip via `serde(flatten)`
        // on `SessionMetadata::extra`. The default fixture seeds an
        // `extra` field; it survives import unchanged.
        assert_eq!(session.get("extra").and_then(Value::as_str), Some("preserved"));

        // Non-session.json files extracted byte-identical.
        assert_eq!(
            fs::read(imported.join("messages.jsonl")).unwrap(),
            b"{\"role\":\"user\"}\n"
        );
    }

    #[test]
    fn import_creates_nested_subdirectories() {
        let dir = temp_base();
        let sessions_root = dir.path().join("sessions");
        let mut contents = BTreeMap::new();
        contents.insert(
            "session.json".to_string(),
            serde_json::to_vec(&default_metadata("orig-id")).unwrap(),
        );
        contents.insert("sub-executions/abc/messages.jsonl".to_string(), b"sub data\n".to_vec());
        contents.insert("snapshots/deadbeef".to_string(), b"snapshot bytes".to_vec());
        let archive_path = write_archive(dir.path(), &build_zip(&contents));

        let imported = import_session(
            ImportSessionOptions {
                kas_sessions_root: sessions_root,
                v2_sessions_dir: PathBuf::from("/tmp/v2_unused_in_kas_target"),
                archive_path,
                workspace_paths: vec!["/x".to_string()],
            },
            ImportTarget::Kas,
            &no_v1_exporter(),
        )
        .unwrap()
        .path;

        assert!(imported.join("sub-executions/abc/messages.jsonl").exists());
        assert!(imported.join("snapshots/deadbeef").exists());
    }

    #[test]
    fn import_skips_directory_marker_entries() {
        let dir = temp_base();
        let sessions_root = dir.path().join("sessions");
        let mut contents = BTreeMap::new();
        contents.insert(
            "session.json".to_string(),
            serde_json::to_vec(&default_metadata("orig-id")).unwrap(),
        );
        // A pure directory entry. The zip crate may or may not emit
        // these on round-trip, but extractors that consume third-party
        // zips have to tolerate them.
        contents.insert("sub-executions/".to_string(), Vec::new());
        let archive_path = write_archive(dir.path(), &build_zip(&contents));

        let imported = import_session(
            ImportSessionOptions {
                kas_sessions_root: sessions_root,
                v2_sessions_dir: PathBuf::from("/tmp/v2_unused_in_kas_target"),
                archive_path,
                workspace_paths: vec!["/x".to_string()],
            },
            ImportTarget::Kas,
            &no_v1_exporter(),
        )
        .unwrap()
        .path;

        // The extractor must not treat the marker as a regular file and
        // attempt to write `sub-executions/` (which would error on most
        // filesystems).
        assert!(imported.join("session.json").exists());
    }

    // -- import_session: error paths --

    #[test]
    fn import_errors_on_missing_archive_file() {
        let dir = temp_base();
        let err = import_session(
            ImportSessionOptions {
                kas_sessions_root: dir.path().join("sessions"),
                v2_sessions_dir: PathBuf::from("/tmp/v2_unused_in_kas_target"),
                archive_path: dir.path().join("nope.zip"),
                workspace_paths: vec!["/x".to_string()],
            },
            ImportTarget::Kas,
            &no_v1_exporter(),
        )
        .unwrap_err();
        assert!(
            err.to_string().contains("failed to read archive"),
            "unexpected message: {err}"
        );
    }

    #[test]
    fn import_errors_on_non_zip_input() {
        let dir = temp_base();
        let archive_path = write_archive(dir.path(), b"not a zip file at all");
        let err = import_session(
            ImportSessionOptions {
                kas_sessions_root: dir.path().join("sessions"),
                v2_sessions_dir: PathBuf::from("/tmp/v2_unused_in_kas_target"),
                archive_path,
                workspace_paths: vec!["/x".to_string()],
            },
            ImportTarget::Kas,
            &no_v1_exporter(),
        )
        .unwrap_err();
        // Bytes don't carry zip magic and aren't valid JSON, so the
        // detector surfaces a JSON parse failure.
        assert!(
            err.to_string().contains("Failed to parse JSON"),
            "unexpected message: {err}"
        );
    }

    #[test]
    fn import_errors_when_session_json_missing() {
        // A zip whose root has neither `session.json` nor
        // `session_metadata.json` is unrecognized at the detector
        // level.
        let dir = temp_base();
        let mut contents = BTreeMap::new();
        contents.insert("messages.jsonl".to_string(), b"hi\n".to_vec());
        let archive_path = write_archive(dir.path(), &build_zip(&contents));
        let err = import_session(
            ImportSessionOptions {
                kas_sessions_root: dir.path().join("sessions"),
                v2_sessions_dir: PathBuf::from("/tmp/v2_unused_in_kas_target"),
                archive_path,
                workspace_paths: vec!["/x".to_string()],
            },
            ImportTarget::Kas,
            &no_v1_exporter(),
        )
        .unwrap_err();
        assert!(
            err.to_string().contains("recognized session manifest"),
            "unexpected message: {err}"
        );
    }

    #[test]
    fn import_errors_on_invalid_session_json() {
        // A zip whose `session.json` is malformed or missing
        // `schemaVersion` is unrecognized: the detector requires a
        // valid, supported `schemaVersion` to classify the archive
        // as KAS, and falls through to V2 detection (also failing
        // here because there's no `session_metadata.json`).
        let bad_id_metadata = serde_json::to_vec(&json!({
            "id": 5,
            "schemaVersion": "1.0.0",
            "title": "t",
            "agentMode": "default",
            "workspacePaths": ["/x"],
            "createdAt": "2025-01-01T00:00:00Z",
            "lastModifiedAt": "2025-01-01T00:00:00Z",
        }))
        .unwrap();
        // `garbled` and `top_level_array` produce a zip whose
        // session.json fails KAS schema probing and falls through
        // to V2 detection, which surfaces "recognized session
        // manifest"; `field_type_mismatch` carries a recognized
        // `schemaVersion` so the detector classifies it as KAS and
        // the typed parse in `import_kas_zip` rejects it.
        let cases: &[(&str, &[u8], &str)] = &[
            ("garbled", b"{ not json", "recognized session manifest"),
            ("top_level_array", b"[1, 2, 3]", "recognized session manifest"),
            ("field_type_mismatch", &bad_id_metadata, "session.json is invalid"),
        ];
        for (label, bytes, expected) in cases {
            let dir = temp_base();
            let err = import_session_json_bytes(dir.path(), bytes).unwrap_err();
            assert!(
                err.to_string().contains(expected),
                "[{label}] expected {expected:?}, got: {err}"
            );
        }
    }

    #[test]
    fn import_errors_when_session_json_missing_required_field() {
        // Removing a required field has two distinct error surfaces:
        // dropping `schemaVersion` makes detection fall through (no
        // recognized manifest), so the import errors with
        // "recognized session manifest". Dropping any other required
        // field leaves `schemaVersion: "1.0.0"` intact, so the
        // detector classifies the archive as KAS and the typed parse
        // in `import_kas_zip` produces "missing field <name>".
        let required_fields = [
            "id",
            "schemaVersion",
            "title",
            "agentMode",
            "workspacePaths",
            "createdAt",
            "lastModifiedAt",
        ];
        let mut failures = Vec::new();
        for field in required_fields {
            let dir = temp_base();
            let mut metadata = default_metadata("abc");
            metadata.as_object_mut().unwrap().remove(field);
            let result = import_metadata(dir.path(), &metadata);
            match (field, result) {
                ("schemaVersion", Err(err)) => {
                    let msg = err.to_string();
                    if !msg.contains("recognized session manifest") {
                        failures.push(format!(
                            "missing schemaVersion: expected \"recognized session manifest\", got {msg:?}"
                        ));
                    }
                },
                (field, Err(err)) => {
                    let msg = err.to_string();
                    if !(msg.contains("missing field") && msg.contains(field)) {
                        failures.push(format!(
                            "missing {field}: expected \"missing field\" + {field:?}, got {msg:?}"
                        ));
                    }
                },
                (field, Ok(p)) => failures.push(format!("missing {field}: expected error, got Ok({p:?})")),
            }
        }
        assert!(failures.is_empty(), "{failures:#?}");
    }

    #[test]
    fn import_errors_on_unsupported_schema_version() {
        // An unrecognized `schemaVersion` causes the detector to
        // reject the archive as KAS-format (and to fall through to
        // V2, which also fails because there's no
        // `session_metadata.json`). Surface is the detector's
        // "recognized session manifest" message rather than the
        // typed parser's "unsupported schemaVersion".
        let dir = temp_base();
        let mut metadata = default_metadata("abc");
        metadata
            .as_object_mut()
            .unwrap()
            .insert("schemaVersion".to_string(), json!("99.0.0"));
        let err = import_metadata(dir.path(), &metadata).unwrap_err();
        assert!(
            err.to_string().contains("recognized session manifest"),
            "unexpected message: {err}"
        );
    }

    #[test]
    fn import_strips_session_id_coupled_fields() {
        // Pins the strip behavior per linkage field so a regression on any one of them fails
        // loudly with its name attached. See `SOURCE_ID_COUPLED_FIELDS` for the rationale.
        let strip_fields: &[&str] = &["parentSessionId", "parentExecutionId", "lastCheckpointId"];
        let mut failures = Vec::new();
        for field in strip_fields {
            let dir = temp_base();
            let mut metadata = default_metadata("id");
            metadata
                .as_object_mut()
                .unwrap()
                .insert((*field).to_string(), json!("source-value"));
            match import_metadata(dir.path(), &metadata) {
                Ok(imported) => {
                    let parsed: Value =
                        serde_json::from_slice(&fs::read(imported.join("session.json")).unwrap()).unwrap();
                    if parsed.get(*field).is_some() {
                        failures.push(format!("{field}: not stripped on import (got {parsed:?})"));
                    }
                },
                Err(err) => failures.push(format!("{field}: import failed: {err}")),
            }
        }
        assert!(failures.is_empty(), "{failures:#?}");
    }

    #[test]
    fn import_preserves_unknown_fields() {
        // The import path uses `SessionMetadata`'s `serde(flatten)`
        // `extra` field, so unknown fields outside the explicit
        // schema (e.g. `effortLevel`, `semanticReviewEnabled`,
        // future user-data fields KAS adds) round-trip verbatim
        // through import. Id-coupled fields are scrubbed separately;
        // see `import_strips_session_id_coupled_fields`.
        let dir = temp_base();
        let mut metadata = default_metadata("id");
        let map = metadata.as_object_mut().unwrap();
        map.insert("hypotheticalFutureField".to_string(), json!("user-value"));
        map.insert("anotherFutureField".to_string(), json!({"nested": true}));
        map.insert("yetAnotherOne".to_string(), json!(["a", "b"]));

        let imported = import_metadata(dir.path(), &metadata).unwrap();
        let parsed: Value = serde_json::from_slice(&fs::read(imported.join("session.json")).unwrap()).unwrap();
        let obj = parsed.as_object().unwrap();
        assert_eq!(obj.get("hypotheticalFutureField"), Some(&json!("user-value")));
        assert_eq!(obj.get("anotherFutureField"), Some(&json!({"nested": true})));
        assert_eq!(obj.get("yetAnotherOne"), Some(&json!(["a", "b"])));
    }

    #[test]
    fn import_preserves_optional_known_fields() {
        // Optional fields that are not session-id-coupled should
        // round-trip verbatim.
        let dir = temp_base();
        let mut metadata = default_metadata("id");
        let map = metadata.as_object_mut().unwrap();
        map.insert("dataModelVersion".to_string(), json!(2));
        map.insert("repositories".to_string(), json!(["org/repo"]));

        let imported = import_metadata(dir.path(), &metadata).unwrap();
        let parsed: Value = serde_json::from_slice(&fs::read(imported.join("session.json")).unwrap()).unwrap();
        assert_eq!(parsed.get("dataModelVersion").and_then(Value::as_u64), Some(2));
        assert_eq!(
            parsed.get("repositories").and_then(Value::as_array).map(|a| a.len()),
            Some(1)
        );
    }

    #[test]
    fn import_errors_on_zip_slip_via_dotdot() {
        let dir = temp_base();
        let mut contents = BTreeMap::new();
        contents.insert(
            "session.json".to_string(),
            serde_json::to_vec(&default_metadata("id")).unwrap(),
        );
        contents.insert("../escape".to_string(), b"naughty".to_vec());
        let archive_path = write_archive(dir.path(), &build_zip(&contents));

        let err = import_session(
            ImportSessionOptions {
                kas_sessions_root: dir.path().join("sessions"),
                v2_sessions_dir: PathBuf::from("/tmp/v2_unused_in_kas_target"),
                archive_path,
                workspace_paths: vec!["/x".to_string()],
            },
            ImportTarget::Kas,
            &no_v1_exporter(),
        )
        .unwrap_err();
        assert!(
            err.to_string().contains("escapes destination"),
            "unexpected message: {err}"
        );
    }

    #[test]
    fn import_cleans_up_staging_dir_on_failure() {
        let dir = temp_base();
        let sessions_root = dir.path().join("sessions");
        let mut contents = BTreeMap::new();
        contents.insert(
            "session.json".to_string(),
            serde_json::to_vec(&default_metadata("id")).unwrap(),
        );
        contents.insert("../escape".to_string(), b"x".to_vec());
        let archive_path = write_archive(dir.path(), &build_zip(&contents));

        let workspace = vec!["/x".to_string()];
        let _ = import_session(
            ImportSessionOptions {
                kas_sessions_root: sessions_root.clone(),
                v2_sessions_dir: PathBuf::from("/tmp/v2_unused_in_kas_target"),
                archive_path,
                workspace_paths: workspace.clone(),
            },
            ImportTarget::Kas,
            &no_v1_exporter(),
        );

        // After failure:
        // 1. The bucket dir, if it exists, must contain no orphaned staging entries (no `.import-*` and no
        //    in-progress session dirs).
        // 2. The dedicated `.import-staging/` sibling must contain no leftover uuid-named subdirectories.
        let bucket_dir = sessions_root.join(compute_workspace_hash(&workspace));
        if bucket_dir.exists() {
            for entry in fs::read_dir(&bucket_dir).unwrap() {
                let name = entry.unwrap().file_name();
                let name = name.to_string_lossy();
                assert!(!name.starts_with(".import-"), "leftover staging dir in bucket: {name}",);
            }
        }
        let staging_root = sessions_root.join(IMPORT_STAGING_DIR);
        if staging_root.exists() {
            let leftover: Vec<_> = fs::read_dir(&staging_root).unwrap().collect();
            assert!(
                leftover.is_empty(),
                "leftover staging entries: {} found",
                leftover.len(),
            );
        }
    }

    // -- export -> import roundtrip --

    #[test]
    fn export_then_import_roundtrips_files_verbatim() {
        let dir = temp_base();
        let src_root = dir.path().join("sessions-src");
        let dest_root = dir.path().join("sessions-dest");
        let session_id = "round-trip";
        let workspace = vec!["/source/workspace".to_string()];
        write_kas_session(
            &src_root,
            &compute_workspace_hash(&workspace),
            session_id,
            &default_session_files(session_id),
        );

        let out = dir.path().join("round-trip.zip");
        export_session(ExportSessionOptions {
            kas_sessions_root: src_root,
            session_id: session_id.to_string(),
            workspace_paths: workspace,
            out_path: out.clone(),
            force: false,
        })
        .unwrap();

        let imported = import_session(
            ImportSessionOptions {
                kas_sessions_root: dest_root,
                v2_sessions_dir: PathBuf::from("/tmp/v2_unused_in_kas_target"),
                archive_path: out,
                workspace_paths: vec!["/dest/workspace".to_string()],
            },
            ImportTarget::Kas,
            &no_v1_exporter(),
        )
        .unwrap()
        .path;

        // Every allowlisted file roundtrips byte-identical except
        // session.json (id and lastModifiedAt are rewritten).
        for (name, original_bytes) in &included_session_files(session_id) {
            if name == "session.json" {
                continue;
            }
            let actual = fs::read(imported.join(name)).unwrap();
            assert_eq!(&actual, original_bytes, "mismatch for {name}");
        }
        // Excluded files are absent from the imported session.
        for name in [
            "snapshots/deadbeef",
            "sub-executions/abc/snapshots/cafe",
            "session.json.lock",
        ] {
            assert!(!imported.join(name).exists(), "expected {name} to be excluded");
        }
        let parsed: Value = serde_json::from_slice(&fs::read(imported.join("session.json")).unwrap()).unwrap();
        // The default fixture seeds an `extra` field; it survives
        // import via `serde(flatten)` on `SessionMetadata::extra`.
        assert_eq!(parsed.get("extra").and_then(Value::as_str), Some("preserved"));
        assert_eq!(parsed.get("schemaVersion").and_then(Value::as_str), Some("1.0.0"));
    }

    // -- V2 source imports (kiro-session-export-v1 JSON, V2 zip) --

    /// V2 `SessionData` JSON for the given source id. Used as the
    /// `metadata` field of a `kiro-session-export-v1` payload, and as
    /// the `session_metadata.json` file inside a V2 zip.
    fn v2_source_metadata(session_id: &str) -> Value {
        json!({
            "session_id": session_id,
            "cwd": "/tmp/source",
            "created_at": "2024-01-01T00:00:00Z",
            "updated_at": "2024-01-01T00:05:00Z",
            "title": "Source V2 session",
            "exported_from_v1": false,
            "imported_from": null,
            "parent_session_id": null,
            "session_created_reason": "subagent",
            "session_state": "Unknown",
        })
    }

    /// Single `LogEntry::Prompt` with the given user message id and
    /// text. Used in fixtures that only need to assert the converter
    /// fired and produced a recognizable user payload.
    fn v2_prompt_log_entry(message_id: &str, text: &str) -> Value {
        json!({
            "version": "v1",
            "kind": "Prompt",
            "data": {
                "message_id": message_id,
                "content": [{"kind": "text", "data": text}],
            },
        })
    }

    /// Encode log entries as a `conversation_log.jsonl` blob (one
    /// entry per line, trailing newline).
    fn v2_log_jsonl(entries: &[Value]) -> Vec<u8> {
        let mut out = String::new();
        for entry in entries {
            out.push_str(&serde_json::to_string(entry).unwrap());
            out.push('\n');
        }
        out.into_bytes()
    }

    /// Build a minimal `kiro-session-export-v1` JSON payload with one
    /// user prompt.
    fn build_v2_kiro_v1_json(session_id: &str) -> Vec<u8> {
        serde_json::to_vec(&json!({
            "format": "kiro-session-export-v1",
            "metadata": v2_source_metadata(session_id),
            "log_entries": [v2_prompt_log_entry("u1", "hello from v2")],
        }))
        .unwrap()
    }

    /// Build a minimal V2 zip archive (`session_metadata.json` +
    /// `conversation_log.jsonl`) with one user prompt.
    fn build_v2_zip(session_id: &str) -> Vec<u8> {
        let mut contents = BTreeMap::new();
        contents.insert(
            "session_metadata.json".to_string(),
            serde_json::to_vec(&v2_source_metadata(session_id)).unwrap(),
        );
        contents.insert(
            "conversation_log.jsonl".to_string(),
            v2_log_jsonl(&[v2_prompt_log_entry("u1", "hello from v2")]),
        );
        build_zip(&contents)
    }

    /// Assertions shared by every V2-source import test. Confirms the
    /// imported session lives in the destination workspace bucket,
    /// the new id encodes the source V2 id (so the resume-id
    /// idempotency probe can find it on a subsequent conversion),
    /// `workspacePaths` is rewritten to the destination, and
    /// `messages.jsonl` carries the converted user prompt.
    fn assert_v2_import_lands_in_bucket(imported: &Path, sessions_root: &Path, workspace: &[String], v2_id: &str) {
        assert!(
            imported.starts_with(sessions_root.join(compute_workspace_hash(workspace))),
            "imported session not in workspace bucket: {}",
            imported.display()
        );

        let session = read_session_json(imported);

        let new_id = session.get("id").and_then(Value::as_str).unwrap();
        let expected_prefix = format!("cli_{v2_id}_");
        assert!(
            new_id.starts_with(&expected_prefix),
            "imported id {new_id} does not start with {expected_prefix}"
        );
        assert_eq!(imported.file_name().and_then(|s| s.to_str()).unwrap(), new_id);

        let paths: Vec<&str> = session
            .get("workspacePaths")
            .and_then(Value::as_array)
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect();
        let expected_paths: Vec<&str> = workspace.iter().map(String::as_str).collect();
        assert_eq!(paths, expected_paths);

        let messages_jsonl = fs::read_to_string(imported.join("messages.jsonl")).unwrap();
        assert!(
            !messages_jsonl.is_empty(),
            "messages.jsonl should contain converted prompt"
        );
        let first_line = messages_jsonl.lines().next().unwrap();
        let first: Value = serde_json::from_str(first_line).unwrap();
        assert_eq!(first["payload"]["type"].as_str(), Some("user"));
        assert_eq!(first["payload"]["content"].as_str(), Some("hello from v2"));
    }

    /// `import_session` accepts a `kiro-session-export-v1` JSON file
    /// and produces a KAS session in the destination bucket.
    #[test]
    fn import_v2_kiro_v1_json_writes_kas_session_in_bucket() {
        let dir = temp_base();
        let sessions_root = dir.path().join("sessions");
        let workspace = vec!["/dest/workspace".to_string()];
        let v2_id = "11111111-2222-3333-4444-555555555555";
        let archive_path = dir.path().join("export.json");
        fs::write(&archive_path, build_v2_kiro_v1_json(v2_id)).unwrap();

        let imported = import_session(
            ImportSessionOptions {
                kas_sessions_root: sessions_root.clone(),
                v2_sessions_dir: PathBuf::from("/tmp/v2_unused_in_kas_target"),
                archive_path,
                workspace_paths: workspace.clone(),
            },
            ImportTarget::Kas,
            &no_v1_exporter(),
        )
        .expect("V2 KiroV1 JSON import should succeed")
        .path;

        assert_v2_import_lands_in_bucket(&imported, &sessions_root, &workspace, v2_id);
    }

    /// `import_session` accepts a V2 zip archive (with
    /// `session_metadata.json` + `conversation_log.jsonl` at the
    /// root) and produces a KAS session indistinguishable from the
    /// JSON path's output.
    #[test]
    fn import_v2_zip_writes_kas_session_in_bucket() {
        let dir = temp_base();
        let sessions_root = dir.path().join("sessions");
        let workspace = vec!["/dest/workspace".to_string()];
        let v2_id = "22222222-3333-4444-5555-666666666666";
        let archive_path = write_archive(dir.path(), &build_v2_zip(v2_id));

        let imported = import_session(
            ImportSessionOptions {
                kas_sessions_root: sessions_root.clone(),
                v2_sessions_dir: PathBuf::from("/tmp/v2_unused_in_kas_target"),
                archive_path,
                workspace_paths: workspace.clone(),
            },
            ImportTarget::Kas,
            &no_v1_exporter(),
        )
        .expect("V2 zip import should succeed")
        .path;

        assert_v2_import_lands_in_bucket(&imported, &sessions_root, &workspace, v2_id);
    }
}
