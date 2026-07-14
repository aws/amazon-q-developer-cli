//! File I/O for the universal-config upgrade: enriches a V2-style config with derived V3 fields and
//! rewrites it in place after backing the original up to `<name>.json.bak` (numbered on collision).
//! The result is a config both engines load — V2 (Rust) reads its trust fields, KAS reads
//! `permissions`.

use std::ffi::OsString;
use std::path::{
    Path,
    PathBuf,
};

use serde::{
    Deserialize,
    Serialize,
};
use serde_json::Value;
use tracing::{
    debug,
    warn,
};
use typeshare::typeshare;

use super::migrate::{
    AgentClassification,
    upgrade_agent_config,
};
use super::permissions::MigrationWarning;

/// Suffix used for the safety-net backup of the pre-upgrade source file.
pub const BACKUP_SUFFIX: &str = ".bak";

/// Terminal status of a single-file upgrade attempt.
#[typeshare]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum UpgradeStatus {
    Upgraded,
    SkippedInSync,
    SkippedV3Only,
    SkippedNotAgent,
    SkippedManaged,
    Error,
}

/// Per-agent upgrade outcome.
#[typeshare]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentUpgradeOutcome {
    /// Agent name (filename without extension).
    pub name: String,
    pub source_path: String,
    /// Backup written before the in-place rewrite, if any.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub backup_path: Option<String>,
    /// `None` when the file isn't an agent config (unreadable, invalid JSON, or non-agent).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub classification: Option<AgentClassification>,
    pub status: UpgradeStatus,
    pub warnings: Vec<MigrationWarning>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Append `suffix` to the full filename (extension included): `foo.json` -> `foo.json.bak`.
fn append_suffix(path: &Path, suffix: &str) -> PathBuf {
    let mut s: OsString = path.as_os_str().to_owned();
    s.push(suffix);
    PathBuf::from(s)
}

/// Next available backup path (GNU `cp --backup=numbered`): `foo.json.bak`, then `.bak.1`,
/// `.bak.2`, … Always returns a path that doesn't already exist; hard error past 10000.
pub fn backup_path(source_path: &Path) -> Result<PathBuf, String> {
    let base = append_suffix(source_path, BACKUP_SUFFIX);
    if !base.exists() {
        return Ok(base);
    }
    for i in 1..10000 {
        let candidate = append_suffix(source_path, &format!("{BACKUP_SUFFIX}.{i}"));
        if !candidate.exists() {
            return Ok(candidate);
        }
    }
    // Bail out hard rather than silently overwrite a backup.
    Err(format!(
        "Refusing to upgrade {}: too many existing backups",
        source_path.display()
    ))
}

/// Write `bytes` to `dest` atomically: write a temp file in the same directory, then rename over
/// `dest`. A crash mid-write leaves `dest` either untouched or fully replaced, never half-written.
/// The pre-existing `dest`'s permissions are carried over — the fresh temp file is created 0600, so
/// without this an upgrade would silently tighten a checked-in 0644 config to owner-only.
fn write_atomic(dest: &Path, bytes: &[u8]) -> Result<(), String> {
    let dir = dest.parent().unwrap_or_else(|| Path::new("."));
    let original_perms = std::fs::metadata(dest).map(|m| m.permissions()).ok();
    let mut tmp = tempfile::NamedTempFile::new_in(dir).map_err(|e| e.to_string())?;
    std::io::Write::write_all(&mut tmp, bytes).map_err(|e| e.to_string())?;
    tmp.persist(dest).map_err(|e| e.to_string())?;
    if let Some(perms) = original_perms {
        std::fs::set_permissions(dest, perms).map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Agent display name from its path: filename without `.json`/`.md`.
pub fn agent_name_from_path(file_path: &Path) -> String {
    let base = file_path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or_else(|| file_path.to_str().unwrap_or_default());
    for ext in [".json", ".md"] {
        if let Some(stripped) = base.strip_suffix(ext) {
            return stripped.to_string();
        }
    }
    base.to_string()
}

/// Lenient parse of an agent JSON file. Returns `None` on any failure or a non-object root.
pub fn parse_agent_file(file_path: &Path) -> Option<Value> {
    let raw = std::fs::read_to_string(file_path).ok()?;
    match serde_json::from_str::<Value>(&raw) {
        Ok(value) if value.is_object() => Some(value),
        Ok(_) => None,
        Err(err) => {
            debug!("[agent-upgrade] failed to parse {}: {err}", file_path.display());
            None
        },
    }
}

/// List candidate agent files (absolute `.json` paths), sorted by filename. Backup files (`.bak*`)
/// don't end in `.json`, so the extension filter excludes them. Returns `[]` for missing/unreadable
/// directories.
pub fn list_agent_files(dir: &Path) -> Vec<PathBuf> {
    if !dir.exists() {
        return Vec::new();
    }
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(err) => {
            warn!("[agent-upgrade] Failed to read dir {}: {err}", dir.display());
            return Vec::new();
        },
    };
    let mut names: Vec<String> = entries
        .filter_map(|e| e.ok())
        .filter_map(|e| e.file_name().into_string().ok())
        .filter(|name| name.ends_with(".json"))
        .collect();
    names.sort();
    names.into_iter().map(|name| dir.join(name)).collect()
}

/// True if a parsed object looks like an agent config (has a meaningful field).
pub fn looks_like_agent(config: &Value) -> bool {
    let Some(obj) = config.as_object() else {
        return false;
    };
    [
        "prompt",
        "tools",
        "allowedTools",
        "permissions",
        "toolsSettings",
        "hooks",
    ]
    .iter()
    .any(|f| obj.contains_key(*f))
}

/// Heuristic: an AIM-managed agent (marker phrase in `description`) must never be rewritten.
/// Centralized so the scan and the writer share one exclusion.
pub fn is_externally_managed(config: &Value) -> bool {
    config
        .get("description")
        .and_then(Value::as_str)
        .is_some_and(|desc| desc.contains("managed by AIM"))
}

/// Upgrade a single agent file in place: back up to `<source>.bak` (numbered on collision), then
/// write the enriched config. Non-actionable classifications leave the file untouched.
pub fn upgrade_agent_file(source_path: &Path) -> AgentUpgradeOutcome {
    let name = agent_name_from_path(source_path);
    let source_path_str = source_path.to_string_lossy().into_owned();

    let raw = match std::fs::read_to_string(source_path) {
        Ok(raw) => raw,
        Err(err) => {
            return AgentUpgradeOutcome {
                name,
                source_path: source_path_str,
                backup_path: None,
                classification: None,
                status: UpgradeStatus::Error,
                warnings: Vec::new(),
                error: Some(format!("Failed to read: {err}")),
            };
        },
    };

    let config: Value = match serde_json::from_str(&raw) {
        Ok(config) => config,
        Err(err) => {
            return AgentUpgradeOutcome {
                name,
                source_path: source_path_str,
                backup_path: None,
                classification: None,
                status: UpgradeStatus::Error,
                warnings: Vec::new(),
                error: Some(format!("Invalid JSON: {err}")),
            };
        },
    };

    if !looks_like_agent(&config) {
        return AgentUpgradeOutcome {
            name,
            source_path: source_path_str,
            backup_path: None,
            classification: None,
            status: UpgradeStatus::SkippedNotAgent,
            warnings: Vec::new(),
            error: None,
        };
    }

    let result = upgrade_agent_config(&config);
    let classification = Some(result.classification);

    // Defense-in-depth: never rewrite a managed agent even if handed its path directly (the scan
    // excludes these too).
    if is_externally_managed(&config) {
        return AgentUpgradeOutcome {
            name,
            source_path: source_path_str,
            backup_path: None,
            classification,
            status: UpgradeStatus::SkippedManaged,
            warnings: Vec::new(),
            error: None,
        };
    }

    // Non-actionable classifications stay untouched.
    if result.classification == AgentClassification::V3Only {
        return AgentUpgradeOutcome {
            name,
            source_path: source_path_str,
            backup_path: None,
            classification,
            status: UpgradeStatus::SkippedV3Only,
            warnings: Vec::new(),
            error: None,
        };
    }
    if result.classification == AgentClassification::UniversalInSync {
        return AgentUpgradeOutcome {
            name,
            source_path: source_path_str,
            backup_path: None,
            classification,
            status: UpgradeStatus::SkippedInSync,
            warnings: Vec::new(),
            error: None,
        };
    }

    let backup = match backup_path(source_path).and_then(|backup| {
        std::fs::copy(source_path, &backup)
            .map(|_| backup)
            .map_err(|err| err.to_string())
    }) {
        Ok(backup) => backup,
        Err(err) => {
            return AgentUpgradeOutcome {
                name,
                source_path: source_path_str,
                backup_path: None,
                classification,
                status: UpgradeStatus::Error,
                warnings: result.warnings,
                error: Some(format!("Failed to back up: {err}")),
            };
        },
    };
    let backup_str = backup.to_string_lossy().into_owned();

    let mut serialized = serde_json::to_string_pretty(&result.config).expect("config serializes");
    serialized.push('\n');
    if let Err(err) = write_atomic(source_path, serialized.as_bytes()) {
        return AgentUpgradeOutcome {
            name,
            source_path: source_path_str,
            backup_path: Some(backup_str),
            classification,
            status: UpgradeStatus::Error,
            warnings: result.warnings,
            error: Some(format!("Failed to write: {err}")),
        };
    }

    AgentUpgradeOutcome {
        name,
        source_path: source_path_str,
        backup_path: Some(backup_str),
        classification,
        status: UpgradeStatus::Upgraded,
        warnings: result.warnings,
        error: None,
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use tempfile::TempDir;

    use super::*;

    /// Write `files` into a fresh temp dir, upgrade each candidate, and return status-by-filename.
    fn run(files: &[(&str, Value)]) -> (TempDir, HashMap<String, UpgradeStatus>) {
        let dir = tempfile::tempdir().expect("tempdir");
        for (name, content) in files {
            let serialized = serde_json::to_string_pretty(content).expect("serializes");
            std::fs::write(dir.path().join(name), serialized).expect("write");
        }
        let statuses = list_agent_files(dir.path())
            .into_iter()
            .map(|p| {
                let outcome = upgrade_agent_file(&p);
                let file = Path::new(&outcome.source_path)
                    .file_name()
                    .and_then(|s| s.to_str())
                    .unwrap()
                    .to_string();
                (file, outcome.status)
            })
            .collect();
        (dir, statuses)
    }

    fn read_json(dir: &TempDir, name: &str) -> Value {
        let raw = std::fs::read_to_string(dir.path().join(name)).expect("read");
        serde_json::from_str(&raw).expect("parse")
    }

    fn exists(dir: &TempDir, name: &str) -> bool {
        dir.path().join(name).exists()
    }

    #[test]
    fn v2_only_upgraded_in_place_original_backed_up() {
        let (dir, status) = run(&[("a.json", serde_json::json!({ "tools": ["fs_read"] }))]);
        assert_eq!(status.get("a.json"), Some(&UpgradeStatus::Upgraded));
        assert_eq!(read_json(&dir, "a.json"), serde_json::json!({ "tools": ["read"] }));
        assert_eq!(
            read_json(&dir, "a.json.bak"),
            serde_json::json!({ "tools": ["fs_read"] })
        );
    }

    #[test]
    fn universal_in_sync_writes_nothing() {
        let (dir, status) = run(&[(
            "u.json",
            serde_json::json!({
                "tools": ["shell"],
                "toolsSettings": { "execute_bash": { "allowedCommands": ["git status"] } },
                "permissions": {
                    "rules": [ { "capability": "shell", "match": ["git status"], "effect": "allow" } ]
                }
            }),
        )]);
        assert_eq!(status.get("u.json"), Some(&UpgradeStatus::SkippedInSync));
        assert!(!exists(&dir, "u.json.bak"));
    }

    #[test]
    fn universal_out_of_sync_rederives_tools_preserves_permissions() {
        let (dir, status) = run(&[(
            "o.json",
            serde_json::json!({ "tools": ["fs_read", "fs_write"], "permissions": { "rules": [] } }),
        )]);
        assert_eq!(status.get("o.json"), Some(&UpgradeStatus::Upgraded));
        assert_eq!(
            read_json(&dir, "o.json"),
            serde_json::json!({ "tools": ["read", "write"], "permissions": { "rules": [] } })
        );
        assert_eq!(
            read_json(&dir, "o.json.bak"),
            serde_json::json!({ "tools": ["fs_read", "fs_write"], "permissions": { "rules": [] } })
        );
    }

    #[test]
    fn v3_only_left_untouched() {
        let (dir, status) = run(&[(
            "k.json",
            serde_json::json!({ "tools": ["read"], "permissions": { "rules": [] } }),
        )]);
        assert_eq!(status.get("k.json"), Some(&UpgradeStatus::SkippedV3Only));
        assert!(!exists(&dir, "k.json.bak"));
    }

    #[test]
    fn object_form_hooks_agent_upgraded_to_array_in_place() {
        let (dir, status) = run(&[(
            "h.json",
            serde_json::json!({ "hooks": { "agentSpawn": [ { "command": "git status" } ] } }),
        )]);
        assert_eq!(status.get("h.json"), Some(&UpgradeStatus::Upgraded));
        assert_eq!(
            read_json(&dir, "h.json"),
            serde_json::json!({
                "hooks": [
                    { "name": "agentSpawn-0", "trigger": "agentSpawn",
                      "action": { "type": "command", "command": "git status" }, "timeout": 10 }
                ]
            })
        );
    }

    #[test]
    fn non_agent_json_is_skipped() {
        let (dir, status) = run(&[("mcp.json", serde_json::json!({ "mcpServers": {} }))]);
        assert_eq!(status.get("mcp.json"), Some(&UpgradeStatus::SkippedNotAgent));
        assert!(!exists(&dir, "mcp.json.bak"));
    }

    #[test]
    fn externally_managed_agent_never_rewritten() {
        let (dir, status) = run(&[(
            "managed.json",
            serde_json::json!({ "description": "A workspace agent managed by AIM", "tools": ["fs_read"] }),
        )]);
        assert_eq!(status.get("managed.json"), Some(&UpgradeStatus::SkippedManaged));
        assert_eq!(
            read_json(&dir, "managed.json"),
            serde_json::json!({ "description": "A workspace agent managed by AIM", "tools": ["fs_read"] })
        );
        assert!(!exists(&dir, "managed.json.bak"));
    }

    #[test]
    fn numbered_suffix_on_bak_collision() {
        let (dir, status) = run(&[
            ("a.json", serde_json::json!({ "tools": ["fs_read"] })),
            ("a.json.bak", serde_json::json!({ "tools": ["fs_write"] })),
        ]);
        assert_eq!(status.get("a.json"), Some(&UpgradeStatus::Upgraded));
        assert_eq!(read_json(&dir, "a.json"), serde_json::json!({ "tools": ["read"] }));
        // Pre-existing `.bak` is untouched; the original lands in `.bak.1`.
        assert_eq!(
            read_json(&dir, "a.json.bak"),
            serde_json::json!({ "tools": ["fs_write"] })
        );
        assert_eq!(
            read_json(&dir, "a.json.bak.1"),
            serde_json::json!({ "tools": ["fs_read"] })
        );
    }

    #[test]
    fn non_disruptive_preserves_v2_fields_and_dedupes_alias_spellings() {
        let (dir, status) = run(&[(
            "v.json",
            serde_json::json!({
                "name": "v",
                "tools": ["fs_read", "grep", "glob", "execute_bash"],
                "allowedTools": ["fs_read"],
                "toolsSettings": { "execute_bash": { "allowedCommands": ["git status"] } }
            }),
        )]);
        assert_eq!(status.get("v.json"), Some(&UpgradeStatus::Upgraded));
        assert_eq!(
            read_json(&dir, "v.json"),
            serde_json::json!({
                "name": "v",
                "tools": ["glob", "grep", "read", "shell"],
                "allowedTools": ["fs_read"],
                "toolsSettings": { "execute_bash": { "allowedCommands": ["git status"] } },
                "permissions": {
                    "rules": [
                        { "capability": "shell", "match": ["git status"], "effect": "allow" },
                        { "capability": "fs_read", "effect": "allow" }
                    ]
                }
            })
        );
        assert_eq!(
            read_json(&dir, "v.json.bak"),
            serde_json::json!({
                "name": "v",
                "tools": ["fs_read", "grep", "glob", "execute_bash"],
                "allowedTools": ["fs_read"],
                "toolsSettings": { "execute_bash": { "allowedCommands": ["git status"] } }
            })
        );
    }

    #[test]
    fn read_error_reports_error_status() {
        let dir = tempfile::tempdir().expect("tempdir");
        let missing = dir.path().join("nope.json");
        let outcome = upgrade_agent_file(&missing);
        assert_eq!(outcome.status, UpgradeStatus::Error);
        assert_eq!(outcome.classification, None);
        assert!(outcome.error.expect("error").starts_with("Failed to read:"));
    }

    #[test]
    fn invalid_json_reports_error_status() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("bad.json");
        std::fs::write(&path, "{ not json").expect("write");
        let outcome = upgrade_agent_file(&path);
        assert_eq!(outcome.status, UpgradeStatus::Error);
        assert_eq!(outcome.classification, None);
        assert!(outcome.error.expect("error").starts_with("Invalid JSON:"));
    }

    #[cfg(unix)]
    #[test]
    fn upgrade_preserves_original_file_mode() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("a.json");
        std::fs::write(
            &path,
            serde_json::to_string(&serde_json::json!({ "tools": ["fs_read"] })).unwrap(),
        )
        .expect("write");
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).expect("chmod");

        assert_eq!(upgrade_agent_file(&path).status, UpgradeStatus::Upgraded);
        let mode = std::fs::metadata(&path).expect("metadata").permissions().mode() & 0o777;
        assert_eq!(mode, 0o644, "atomic write must not tighten the config's mode");
    }

    #[test]
    fn second_upgrade_is_idempotent_no_new_backup() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("a.json");
        std::fs::write(
            &path,
            serde_json::to_string_pretty(&serde_json::json!({ "tools": ["fs_read"] })).unwrap(),
        )
        .expect("write");

        let first = upgrade_agent_file(&path);
        assert_eq!(first.status, UpgradeStatus::Upgraded);
        assert!(exists(&dir, "a.json.bak"));

        // Re-running the settled universal config is a no-op: skipped-in-sync, no fresh `.bak.1`.
        let second = upgrade_agent_file(&path);
        assert_eq!(second.status, UpgradeStatus::SkippedInSync);
        assert!(!exists(&dir, "a.json.bak.1"));
    }
}
