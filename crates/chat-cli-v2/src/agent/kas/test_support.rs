//! Test helpers shared by the [`super::import`] and [`super::export`]
//! test modules. Gated behind `#[cfg(test)]` so it adds nothing to the
//! release binary.

use std::collections::BTreeMap;
use std::fs;
use std::io::{
    Cursor,
    Write,
};
use std::path::{
    Path,
    PathBuf,
};

use serde_json::{
    Value,
    json,
};
use tempfile::TempDir;
use zip::ZipWriter;
use zip::write::SimpleFileOptions;

pub(super) fn temp_base() -> TempDir {
    TempDir::new().expect("create tempdir")
}

/// Build an in-memory zip from a name -> bytes map.
pub(super) fn build_zip(contents: &BTreeMap<String, Vec<u8>>) -> Vec<u8> {
    let mut out = Vec::new();
    {
        let mut writer = ZipWriter::new(Cursor::new(&mut out));
        let opts: SimpleFileOptions = SimpleFileOptions::default();
        for (name, bytes) in contents {
            writer.start_file(name.as_str(), opts).unwrap();
            writer.write_all(bytes).unwrap();
        }
        writer.finish().unwrap();
    }
    out
}

/// Write `bytes` to `dir/archive.zip` and return the path.
pub(super) fn write_archive(dir: &Path, bytes: &[u8]) -> PathBuf {
    let path = dir.join("archive.zip");
    fs::write(&path, bytes).unwrap();
    path
}

/// A minimal valid `session.json` payload. Includes every required
/// field so the typed deserializer accepts it; tests for missing-field
/// error paths build their own JSON instead of starting from this.
pub(super) fn default_metadata(id: &str) -> Value {
    json!({
        "id": id,
        "schemaVersion": "1.0.0",
        "title": "Original Title",
        "agentMode": "default",
        "workspacePaths": ["/old/path"],
        "createdAt": "2024-12-31T00:00:00.000Z",
        "lastModifiedAt": "2025-01-01T00:00:00.000Z",
        "extra": "preserved",
    })
}

/// File map representing a populated KAS session. Includes the file
/// types the export filter accepts (`session.json`, `*.jsonl`) plus
/// the file types it must exclude (`snapshots/` at top level and
/// nested under `sub-executions/`, plus a stray non-allowlisted file)
/// so the same fixture exercises both halves of the contract.
pub(super) fn default_session_files(session_id: &str) -> BTreeMap<String, Vec<u8>> {
    let mut files = BTreeMap::new();
    files.insert(
        "session.json".to_string(),
        serde_json::to_vec(&default_metadata(session_id)).unwrap(),
    );
    files.insert("messages.jsonl".to_string(), b"{\"role\":\"user\"}\n".to_vec());
    files.insert(
        "sub-executions/abc/session.json".to_string(),
        b"{\"id\":\"abc\"}".to_vec(),
    );
    files.insert(
        "sub-executions/abc/messages.jsonl".to_string(),
        b"{\"sub\":true}\n".to_vec(),
    );
    files.insert("snapshots/deadbeef".to_string(), b"snapshot".to_vec());
    files.insert(
        "sub-executions/abc/snapshots/cafe".to_string(),
        b"sub snapshot".to_vec(),
    );
    files.insert("session.json.lock".to_string(), b"".to_vec());
    files
}

/// Subset of [`default_session_files`] that the export filter is
/// expected to include. Tests use this to assert presence; use
/// `default_session_files \ this` to assert exclusion.
pub(super) fn included_session_files(session_id: &str) -> BTreeMap<String, Vec<u8>> {
    let mut files = BTreeMap::new();
    files.insert(
        "session.json".to_string(),
        serde_json::to_vec(&default_metadata(session_id)).unwrap(),
    );
    files.insert("messages.jsonl".to_string(), b"{\"role\":\"user\"}\n".to_vec());
    files.insert(
        "sub-executions/abc/session.json".to_string(),
        b"{\"id\":\"abc\"}".to_vec(),
    );
    files.insert(
        "sub-executions/abc/messages.jsonl".to_string(),
        b"{\"sub\":true}\n".to_vec(),
    );
    files
}

/// Write a session directory at `{root}/{hash}/{session_id}/` with the
/// given files. Returns the session directory path.
pub(super) fn write_kas_session(
    root: &Path,
    hash: &str,
    session_id: &str,
    files: &BTreeMap<String, Vec<u8>>,
) -> PathBuf {
    let dir = root.join(hash).join(session_id);
    fs::create_dir_all(&dir).unwrap();
    for (name, bytes) in files {
        let target = dir.join(name);
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(&target, bytes).unwrap();
    }
    dir
}

/// Read and JSON-parse `session.json` from an imported session dir.
pub(super) fn read_session_json(dir: &Path) -> Value {
    serde_json::from_slice(&fs::read(dir.join("session.json")).unwrap()).unwrap()
}

/// Build a default KAS-format zip with `session.json` +
/// `messages.jsonl`. Returns `(zip bytes, original session id)`.
pub(super) fn build_default_kas_zip() -> (Vec<u8>, String) {
    let original_id = "00000000-0000-0000-0000-000000000001".to_string();
    let mut contents = BTreeMap::new();
    contents.insert(
        "session.json".to_string(),
        serde_json::to_vec(&default_metadata(&original_id)).unwrap(),
    );
    contents.insert("messages.jsonl".to_string(), b"{\"role\":\"user\"}\n".to_vec());
    (build_zip(&contents), original_id)
}
