//! Detection of session-archive file formats.
//!
//! The single API entry point used by every `/chat load` code path
//! (the V2 ACP `chat::load_session_impl`, the KAS `import_session`,
//! and any future cross-engine importer) to decide what kind of file
//! the user pointed at before dispatching to a format-specific
//! reader.
//!
//! Detection is byte-only: callers pass the file's contents and get
//! back a [`DetectedFormat`] (or an error message). No filesystem I/O
//! happens here.
//!
//! ## Format precedence
//!
//! Probing order matters: zip variants are distinguished by their
//! interior layout, and JSON variants by which top-level key is
//! present. The current order is:
//!
//!   1. Zip magic bytes -> peek inside. a. `session.json` with a recognized `schemaVersion` ->
//!      [`DetectedFormat::KasZip`]. b. `session_metadata.json` -> [`DetectedFormat::V2Zip`]. c.
//!      Neither -> error.
//!   2. JSON top-level field probing. a. `format == "kiro-session-export-v1"` ->
//!      [`DetectedFormat::KiroV1Json`]. b. `conversation_id` present ->
//!      [`DetectedFormat::LegacyV1`]. c. Otherwise -> error.

use std::io::{
    Cursor,
    Read,
};

use zip::read::ZipArchive;

use super::schema::SUPPORTED_SCHEMA_VERSIONS;

/// First four bytes of every PKZIP archive (the local file header
/// signature). Used to short-circuit JSON probing on binary inputs.
const ZIP_MAGIC_BYTES: [u8; 4] = [0x50, 0x4b, 0x03, 0x04];

/// On-disk format detected for a session archive file.
///
/// `KasZip` and `V2Zip` are both valid zip archives; they're
/// distinguished by which manifest file lives at the root.
/// `KiroV1Json` and `LegacyV1` are JSON objects distinguished by
/// which top-level key they carry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DetectedFormat {
    /// KAS-format zip: `session.json` (with a `schemaVersion` from
    /// [`SUPPORTED_SCHEMA_VERSIONS`]) plus any `*.jsonl` siblings.
    /// Produced by `/chat save` when the active engine is KAS.
    KasZip,
    /// V2-format zip: `session_metadata.json` (V2 `SessionData`
    /// shape) plus `conversation_log.jsonl`. Produced by V2's
    /// `save_as_zip` helper.
    V2Zip,
    /// V2 portable JSON: top-level
    /// `{"format": "kiro-session-export-v1", ...}`. Produced by V2's
    /// `/chat save` (default output).
    KiroV1Json,
    /// V1 (classic) `ConversationState` JSON. Distinguished by the
    /// top-level `conversation_id` field. Loading legacy V1 sessions
    /// is not yet implemented for KAS targets.
    LegacyV1,
}

/// Detect the format of `data`.
///
/// See module docs for the probing order. Returns a human-readable
/// error message when no probe matches; the message is not stable
/// and callers should not pattern-match on it.
pub fn detect(data: &[u8]) -> Result<DetectedFormat, String> {
    if data.starts_with(&ZIP_MAGIC_BYTES) {
        return detect_zip(data);
    }
    detect_json(data)
}

/// Decide between [`DetectedFormat::KasZip`] and
/// [`DetectedFormat::V2Zip`] by reading the zip directory and looking
/// for a recognized manifest file at the root.
fn detect_zip(data: &[u8]) -> Result<DetectedFormat, String> {
    let mut archive = ZipArchive::new(Cursor::new(data))
        .map_err(|_e| "File has zip magic bytes but is not a valid zip archive".to_string())?;

    if let Some(format) = probe_kas_zip(&mut archive)? {
        return Ok(format);
    }
    if archive.by_name("session_metadata.json").is_ok() {
        return Ok(DetectedFormat::V2Zip);
    }
    Err("Zip archive does not contain a recognized session manifest \
         (expected session.json or session_metadata.json)"
        .to_string())
}

/// Returns `Some(KasZip)` if the archive has a `session.json` at the
/// root with a `schemaVersion` listed in [`SUPPORTED_SCHEMA_VERSIONS`].
/// Returns `None` if the file is absent or its `schemaVersion` is
/// unrecognized (so the caller can fall through to V2 detection).
/// Returns `Err` only on I/O failure reading the entry.
fn probe_kas_zip(archive: &mut ZipArchive<Cursor<&[u8]>>) -> Result<Option<DetectedFormat>, String> {
    let mut entry = match archive.by_name("session.json") {
        Ok(e) => e,
        Err(_) => return Ok(None),
    };
    let mut buf = String::new();
    entry
        .read_to_string(&mut buf)
        .map_err(|e| format!("Failed to read session.json from zip: {e}"))?;
    drop(entry);

    let value: serde_json::Value = match serde_json::from_str(&buf) {
        Ok(v) => v,
        Err(_) => return Ok(None),
    };
    let version = value.get("schemaVersion").and_then(|v| v.as_str());
    if version.is_some_and(|v| SUPPORTED_SCHEMA_VERSIONS.contains(&v)) {
        Ok(Some(DetectedFormat::KasZip))
    } else {
        Ok(None)
    }
}

/// Probe a non-zip input as JSON. Returns the matching variant or
/// an error message if no top-level key matches a known shape.
fn detect_json(data: &[u8]) -> Result<DetectedFormat, String> {
    let text = std::str::from_utf8(data).map_err(|e| format!("File is not valid UTF-8 text or a zip archive: {e}"))?;
    let value: serde_json::Value = serde_json::from_str(text).map_err(|e| format!("Failed to parse JSON: {e}"))?;

    if value.get("format").and_then(|v| v.as_str()) == Some("kiro-session-export-v1") {
        return Ok(DetectedFormat::KiroV1Json);
    }
    if value.get("conversation_id").is_some() {
        return Ok(DetectedFormat::LegacyV1);
    }
    Err("Unrecognized session file format".to_string())
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::io::Write;

    use serde_json::json;
    use zip::ZipWriter;
    use zip::write::SimpleFileOptions;

    use super::*;

    /// Build a zip blob from a `name -> bytes` map. Helper for tests
    /// that need to construct synthetic archives.
    fn build_zip(entries: &BTreeMap<String, Vec<u8>>) -> Vec<u8> {
        let mut buf = Vec::new();
        {
            let mut writer = ZipWriter::new(Cursor::new(&mut buf));
            let opts = SimpleFileOptions::default();
            for (name, bytes) in entries {
                writer.start_file(name.as_str(), opts).unwrap();
                writer.write_all(bytes).unwrap();
            }
            writer.finish().unwrap();
        }
        buf
    }

    fn kas_session_json() -> Vec<u8> {
        serde_json::to_vec(&json!({
            "schemaVersion": "1.0.0",
            "id": "sess_abc",
            "title": "t",
            "agentMode": "vibe",
            "workspacePaths": ["/tmp"],
            "createdAt": "2024-01-01T00:00:00Z",
            "lastModifiedAt": "2024-01-01T00:00:00Z",
        }))
        .unwrap()
    }

    fn v2_session_metadata_json() -> Vec<u8> {
        serde_json::to_vec(&json!({
            "session_id": "abc-123",
            "cwd": "/tmp",
            "created_at": "2024-01-01T00:00:00Z",
            "updated_at": "2024-01-01T00:00:00Z",
            "session_state": "Unknown",
        }))
        .unwrap()
    }

    // -- happy path --

    #[test]
    fn detects_kas_zip_when_session_json_has_supported_schema_version() {
        let mut entries = BTreeMap::new();
        entries.insert("session.json".to_string(), kas_session_json());
        entries.insert("messages.jsonl".to_string(), b"{}\n".to_vec());
        let zip = build_zip(&entries);
        assert_eq!(detect(&zip).unwrap(), DetectedFormat::KasZip);
    }

    #[test]
    fn detects_v2_zip_when_session_metadata_json_present() {
        let mut entries = BTreeMap::new();
        entries.insert("session_metadata.json".to_string(), v2_session_metadata_json());
        entries.insert("conversation_log.jsonl".to_string(), b"{}\n".to_vec());
        let zip = build_zip(&entries);
        assert_eq!(detect(&zip).unwrap(), DetectedFormat::V2Zip);
    }

    /// A KAS-format archive that ALSO happens to contain a stray
    /// `session_metadata.json` (e.g. a user packaging extra files)
    /// must still be detected as KasZip - probe order favors KAS.
    #[test]
    fn prefers_kas_over_v2_when_both_manifests_present() {
        let mut entries = BTreeMap::new();
        entries.insert("session.json".to_string(), kas_session_json());
        entries.insert("session_metadata.json".to_string(), v2_session_metadata_json());
        let zip = build_zip(&entries);
        assert_eq!(detect(&zip).unwrap(), DetectedFormat::KasZip);
    }

    #[test]
    fn detects_kiro_v1_json() {
        let json = json!({
            "format": "kiro-session-export-v1",
            "metadata": { "session_id": "s1" },
            "log_entries": [],
        });
        let bytes = serde_json::to_vec(&json).unwrap();
        assert_eq!(detect(&bytes).unwrap(), DetectedFormat::KiroV1Json);
    }

    #[test]
    fn detects_legacy_v1_via_conversation_id() {
        let json = json!({ "conversation_id": "abc-123", "history": [] });
        let bytes = serde_json::to_vec(&json).unwrap();
        assert_eq!(detect(&bytes).unwrap(), DetectedFormat::LegacyV1);
    }

    #[test]
    fn errors_on_json_with_only_session_id_field() {
        let bytes = v2_session_metadata_json();
        let err = detect(&bytes).unwrap_err();
        assert!(
            err.contains("Unrecognized session file format"),
            "unexpected message: {err}"
        );
    }

    // -- error paths --

    #[test]
    fn errors_on_zip_with_unknown_manifest() {
        let mut entries = BTreeMap::new();
        entries.insert("readme.txt".to_string(), b"hello".to_vec());
        let zip = build_zip(&entries);
        let err = detect(&zip).unwrap_err();
        assert!(err.contains("recognized session manifest"), "unexpected message: {err}");
    }

    /// A zip whose `session.json` carries an unsupported schemaVersion
    /// must NOT be detected as KasZip. If the archive also has a V2
    /// manifest it falls through to V2; otherwise it's an error.
    #[test]
    fn falls_through_to_v2_when_session_json_has_unsupported_schema_version() {
        let mut entries = BTreeMap::new();
        entries.insert(
            "session.json".to_string(),
            serde_json::to_vec(&json!({"schemaVersion": "99.0.0"})).unwrap(),
        );
        entries.insert("session_metadata.json".to_string(), v2_session_metadata_json());
        let zip = build_zip(&entries);
        assert_eq!(detect(&zip).unwrap(), DetectedFormat::V2Zip);
    }

    #[test]
    fn errors_on_session_json_with_unsupported_schema_and_no_v2_fallback() {
        let mut entries = BTreeMap::new();
        entries.insert(
            "session.json".to_string(),
            serde_json::to_vec(&json!({"schemaVersion": "99.0.0"})).unwrap(),
        );
        let zip = build_zip(&entries);
        assert!(detect(&zip).is_err());
    }

    #[test]
    fn errors_on_zip_magic_with_corrupt_body() {
        let mut bytes = ZIP_MAGIC_BYTES.to_vec();
        bytes.extend_from_slice(b"not actually a zip");
        let err = detect(&bytes).unwrap_err();
        assert!(err.contains("not a valid zip archive"), "unexpected message: {err}");
    }

    #[test]
    fn errors_on_non_utf8_non_zip_input() {
        let bytes = vec![0xff, 0xfe, 0xfd, 0xfc, 0x00, 0x01];
        let err = detect(&bytes).unwrap_err();
        assert!(err.contains("UTF-8"), "unexpected message: {err}");
    }

    #[test]
    fn errors_on_invalid_json() {
        let err = detect(b"{ not valid json").unwrap_err();
        assert!(err.contains("parse JSON"), "unexpected message: {err}");
    }

    #[test]
    fn errors_on_json_object_without_recognized_keys() {
        let bytes = b"{\"random\": true}";
        let err = detect(bytes).unwrap_err();
        assert!(
            err.contains("Unrecognized session file format"),
            "unexpected message: {err}"
        );
    }

    /// Top-level array isn't a session shape we recognize. The detector
    /// must not panic on non-object JSON.
    #[test]
    fn errors_on_top_level_json_array() {
        let err = detect(b"[1, 2, 3]").unwrap_err();
        assert!(
            err.contains("Unrecognized session file format"),
            "unexpected message: {err}"
        );
    }
}
