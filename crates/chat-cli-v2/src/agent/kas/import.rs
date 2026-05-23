//! Import side: extract a KAS session zip into the local sessions
//! directory, generating a fresh id and rebucketing under the caller's
//! workspace hash.

use std::fs;
use std::path::PathBuf;

use chrono::Utc;
use serde::{
    Deserialize,
    Serialize,
};
use uuid::Uuid;

use super::shared::SessionArchiveError;
use super::workspace_hash::compute_workspace_hash;
use super::zip_util::{
    decode_zip,
    safe_resolve,
};

/// Schema versions this implementation accepts in incoming archives.
///
/// KAS itself currently writes `"1.0.0"` (see
/// `kiro-agent/packages/kiro-agent/src/session/session-persistence.ts`).
/// Imports with any other value are rejected so we fail fast at import
/// time rather than producing a session that KAS later refuses to load.
const SUPPORTED_SCHEMA_VERSIONS: &[&str] = &["1.0.0"];

/// Schema version stamped onto imported sessions. Always the highest
/// supported version - the import flow has already validated the
/// source against [`SUPPORTED_SCHEMA_VERSIONS`].
const CURRENT_SCHEMA_VERSION: &str = "1.0.0";

/// Sibling of the per-workspace bucket dirs under `kas_sessions_root`.
/// In-progress imports are extracted under this dir and renamed into
/// their final bucket on success. Living outside any bucket prevents
/// KAS's session lister from enumerating in-flight or orphaned imports.
const IMPORT_STAGING_DIR: &str = ".import-staging";

/// Subset of `SessionMetadata` (defined in
/// `kiro-agent/packages/acp-type-covenant/session/schemas/index.ts`)
/// that the import path accepts and re-emits.
///
/// Allowlist-style: only fields named here survive deserialize.
/// Anything else in the source `session.json` is silently dropped by
/// serde, including `parentSessionId`, `parentExecutionId`,
/// `lastCheckpointId`, and any future linkage fields KAS adds. This
/// flips the safety default: the output cannot carry a stale
/// reference into the destination environment unless the import path
/// is explicitly updated to opt that field in.
///
/// Required fields are non-`Option`; KAS rejects sessions missing any
/// of these on load.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct SessionMetadata {
    schema_version: String,
    id: String,
    title: String,
    agent_mode: String,
    workspace_paths: Vec<String>,
    created_at: String,
    last_modified_at: String,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    data_model_version: Option<u64>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    repositories: Option<Vec<String>>,
}

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
    /// Root sessions directory, e.g. `~/.kiro/sessions`.
    pub kas_sessions_root: PathBuf,
    /// Path to the archive file to import.
    pub archive_path: PathBuf,
    /// Workspace paths to bucket the imported session under. The
    /// session.json's `workspacePaths` field is rewritten to these,
    /// and the directory lands at
    /// `{kas_sessions_root}/{compute_workspace_hash(workspace_paths)}/{new_id}/`.
    pub workspace_paths: Vec<String>,
}

/// Extract a KAS session zip into the local sessions directory.
///
/// Generates a fresh UUIDv4 for the imported session, rewrites
/// `session.json`'s `workspacePaths` to `opts.workspace_paths`, drops
/// every metadata field outside the import allowlist (so parent /
/// checkpoint linkage and any unknown future fields do not survive),
/// and places the extracted directory under
/// `{kas_sessions_root}/{compute_workspace_hash(workspace_paths)}/{new_id}/`.
///
/// Steps:
///   1. Read archive bytes.
///   2. Decode zip + zip-slip-guard every entry name.
///   3. Require + parse session.json (allowlist parse - unknown fields silently dropped).
///   4. Validate the parsed metadata against the typed schema.
///   5. Generate a fresh UUIDv4 for the new id; rewrite `workspacePaths`, `schemaVersion`, and
///      `lastModifiedAt`.
///   6. Stage all entries (with rewritten session.json) under
///      `{kas_sessions_root}/.import-staging/{uuid}/` so a process killed mid-import never leaves
///      an orphan that KAS's session lister enumerates.
///   7. Atomic rename into the final `{newHash}/{newId}/` location.
///
/// On failure, the staging directory is cleaned up. Returns the
/// absolute path of the imported session directory; the new session
/// id is `imported.file_name()`.
pub fn import_session(opts: ImportSessionOptions) -> Result<PathBuf, SessionArchiveError> {
    // 1. Read archive bytes.
    let archive_bytes = fs::read(&opts.archive_path)
        .map_err(|e| SessionArchiveError::io(format!("failed to read archive: {}", opts.archive_path.display()), e))?;

    // 2. Decode the archive. Only KAS-format zips are supported today.
    let entries = decode_zip(&archive_bytes)?;

    // 3. session.json must be present.
    let session_json_bytes = entries
        .get("session.json")
        .ok_or_else(|| SessionArchiveError::msg("archive does not contain session.json"))?;

    // 4. Parse + validate session.json.
    let mut metadata = parse_session_metadata(session_json_bytes)?;

    // 5. Rewrite metadata for the new session. Strip linkage fields that reference IDs from the source
    //    environment - they are invalid in the destination.
    let new_id = Uuid::new_v4().to_string();
    let new_hash = compute_workspace_hash(&opts.workspace_paths);
    metadata.id = new_id.clone();
    metadata.schema_version = CURRENT_SCHEMA_VERSION.to_string();
    metadata.workspace_paths = opts.workspace_paths.clone();
    metadata.last_modified_at = Utc::now().to_rfc3339();
    // Pretty-print to match KAS's own session.json convention
    // (`JSON.stringify(meta, null, 2)` in session-persistence.ts).
    let rewritten_bytes = serde_json::to_vec_pretty(&metadata)
        .map_err(|e| SessionArchiveError::io("serialize rewritten session.json", std::io::Error::other(e)))?;

    // 6. Stage extraction outside the workspace bucket.
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

    let result = (|| -> Result<PathBuf, SessionArchiveError> {
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

        // 7. Atomic rename into final location.
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
        Ok(final_dir)
    })();

    if result.is_err() {
        // Best-effort cleanup; ignore errors here so the original
        // failure surfaces.
        let _ = fs::remove_dir_all(&staging_dir);
    }
    result
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
        import_session(ImportSessionOptions {
            kas_sessions_root: dir.join("sessions"),
            archive_path,
            workspace_paths: vec!["/x".to_string()],
        })
    }
    // -- import_session: happy path --

    #[test]
    fn import_rewrites_metadata_and_extracts_files() {
        let dir = temp_base();
        let sessions_root = dir.path().join("sessions");
        let workspace = vec!["/foo".to_string(), "/bar".to_string()];
        let (zip_bytes, original_id) = build_default_kas_zip();
        let archive_path = write_archive(dir.path(), &zip_bytes);

        let imported = import_session(ImportSessionOptions {
            kas_sessions_root: sessions_root.clone(),
            archive_path,
            workspace_paths: workspace.clone(),
        })
        .unwrap();

        // Bucketed under the destination workspace's hash.
        assert!(imported.starts_with(sessions_root.join(compute_workspace_hash(&workspace))));

        let session = read_session_json(&imported);

        // Fresh UUID, distinct from source, matches the directory name.
        let new_id = session.get("id").and_then(Value::as_str).unwrap();
        assert_ne!(new_id, original_id);
        assert!(Uuid::parse_str(new_id).is_ok());
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

        // Unknown / future fields are dropped on import (allowlist
        // parse - serde silently discards anything not on the
        // `SessionMetadata` struct). The default fixture seeds an
        // `extra` field; it must not survive the round-trip.
        assert_eq!(session.get("extra"), None);

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

        let imported = import_session(ImportSessionOptions {
            kas_sessions_root: sessions_root,
            archive_path,
            workspace_paths: vec!["/x".to_string()],
        })
        .unwrap();

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

        let imported = import_session(ImportSessionOptions {
            kas_sessions_root: sessions_root,
            archive_path,
            workspace_paths: vec!["/x".to_string()],
        })
        .unwrap();

        // The extractor must not treat the marker as a regular file and
        // attempt to write `sub-executions/` (which would error on most
        // filesystems).
        assert!(imported.join("session.json").exists());
    }

    // -- import_session: error paths --

    #[test]
    fn import_errors_on_missing_archive_file() {
        let dir = temp_base();
        let err = import_session(ImportSessionOptions {
            kas_sessions_root: dir.path().join("sessions"),
            archive_path: dir.path().join("nope.zip"),
            workspace_paths: vec!["/x".to_string()],
        })
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
        let err = import_session(ImportSessionOptions {
            kas_sessions_root: dir.path().join("sessions"),
            archive_path,
            workspace_paths: vec!["/x".to_string()],
        })
        .unwrap_err();
        assert!(err.to_string().contains("zip file"), "unexpected message: {err}");
    }

    #[test]
    fn import_errors_when_session_json_missing() {
        let dir = temp_base();
        let mut contents = BTreeMap::new();
        contents.insert("messages.jsonl".to_string(), b"hi\n".to_vec());
        let archive_path = write_archive(dir.path(), &build_zip(&contents));
        let err = import_session(ImportSessionOptions {
            kas_sessions_root: dir.path().join("sessions"),
            archive_path,
            workspace_paths: vec!["/x".to_string()],
        })
        .unwrap_err();
        assert!(
            err.to_string().contains("does not contain session.json"),
            "unexpected message: {err}"
        );
    }

    #[test]
    fn import_errors_on_invalid_session_json() {
        let bad_id_metadata = serde_json::to_vec(&json!({
            "id": 5,
            "schemaVersion": "1.0.0",
            "title": "t",
            "agentMode": "vibe",
            "workspacePaths": ["/x"],
            "createdAt": "2025-01-01T00:00:00Z",
            "lastModifiedAt": "2025-01-01T00:00:00Z",
        }))
        .unwrap();
        let cases: &[(&str, &[u8])] = &[
            ("garbled", b"{ not json"),
            ("top_level_array", b"[1, 2, 3]"),
            ("field_type_mismatch", &bad_id_metadata),
        ];
        for (label, bytes) in cases {
            let dir = temp_base();
            let err = import_session_json_bytes(dir.path(), bytes).unwrap_err();
            assert!(
                err.to_string().contains("session.json is invalid"),
                "[{label}] expected 'session.json is invalid', got: {err}"
            );
        }
    }

    #[test]
    fn import_errors_when_session_json_missing_required_field() {
        // Drop one required field at a time and confirm parse fails
        // with a `missing field <field>` message. Covers every
        // non-Option field of `SessionMetadata`.
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
            match import_metadata(dir.path(), &metadata) {
                Err(err) => {
                    let msg = err.to_string();
                    if !(msg.contains("missing field") && msg.contains(field)) {
                        failures.push(format!(
                            "missing {field}: expected \"missing field\" + {field:?}, got {msg:?}"
                        ));
                    }
                },
                Ok(p) => failures.push(format!("missing {field}: expected error, got Ok({p:?})")),
            }
        }
        assert!(failures.is_empty(), "{failures:#?}");
    }

    #[test]
    fn import_errors_on_unsupported_schema_version() {
        let dir = temp_base();
        let mut metadata = default_metadata("abc");
        metadata
            .as_object_mut()
            .unwrap()
            .insert("schemaVersion".to_string(), json!("99.0.0"));
        let err = import_metadata(dir.path(), &metadata).unwrap_err();
        assert!(
            err.to_string().contains("unsupported schemaVersion"),
            "unexpected message: {err}"
        );
    }

    #[test]
    fn import_strips_session_id_coupled_fields() {
        // Fields that reference an id from the source environment
        // must not survive import. They are not on the import
        // allowlist, so the allowlist parse drops them along with
        // every other unknown field; this test pins that behavior to
        // each linkage field individually so a regression on any one
        // of them fails loudly with its name attached.
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
    fn import_drops_unknown_fields_outside_allowlist() {
        // The import path uses an allowlist parse: any field not
        // explicitly named on the `SessionMetadata` struct is dropped
        // by serde during deserialize. This is the primary defense
        // against KAS adding a new linkage / id-coupled field that
        // would otherwise silently round-trip into the destination.
        let dir = temp_base();
        let mut metadata = default_metadata("id");
        let map = metadata.as_object_mut().unwrap();
        map.insert("hypotheticalNewLinkageId".to_string(), json!("dangling-source-id"));
        map.insert("anotherFutureField".to_string(), json!({"nested": true}));
        map.insert("yetAnotherOne".to_string(), json!(["a", "b"]));

        let imported = import_metadata(dir.path(), &metadata).unwrap();
        let parsed: Value = serde_json::from_slice(&fs::read(imported.join("session.json")).unwrap()).unwrap();
        let obj = parsed.as_object().unwrap();
        assert!(!obj.contains_key("hypotheticalNewLinkageId"));
        assert!(!obj.contains_key("anotherFutureField"));
        assert!(!obj.contains_key("yetAnotherOne"));
        // The default `extra` field is also unknown.
        assert!(!obj.contains_key("extra"));
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

        let err = import_session(ImportSessionOptions {
            kas_sessions_root: dir.path().join("sessions"),
            archive_path,
            workspace_paths: vec!["/x".to_string()],
        })
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
        let _ = import_session(ImportSessionOptions {
            kas_sessions_root: sessions_root.clone(),
            archive_path,
            workspace_paths: workspace.clone(),
        });

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

        let imported = import_session(ImportSessionOptions {
            kas_sessions_root: dest_root,
            archive_path: out,
            workspace_paths: vec!["/dest/workspace".to_string()],
        })
        .unwrap();

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
        // The default fixture seeds an `extra` field; the import-side
        // allowlist parse drops it.
        assert_eq!(parsed.get("extra"), None);
        assert_eq!(parsed.get("schemaVersion").and_then(Value::as_str), Some("1.0.0"));
    }
}
