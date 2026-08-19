//! End-to-end KAS bundle extraction via the real `chat_cli` binary.
//!
//! Runs `kiro-cli chat --v3 --no-interactive`, the user-facing V3 entrypoint, to assert that KAS
//! assets are extracted on startup, and garbage collection runs correctly.

use std::io::Write;
use std::path::{
    Path,
    PathBuf,
};
use std::time::{
    Duration,
    SystemTime,
    UNIX_EPOCH,
};

use assert_cmd::Command;
use chat_cli::util::consts::env_var::{
    KAS_BUNDLE_PATH,
    KIRO_DATA_DIR,
    KIRO_KAS_NODE_PATH,
    KIRO_TEST_DB_PATH,
};
use predicates::str::contains;
use rusqlite::Connection;

const SERVER_REL: &str = "node_modules/@kiro/agent/dist/server/acp-server.js";
const SERVER_BODY: &[u8] = b"export const hi = 1;\n";

/// Build a gzip'd tar containing a single `acp-server.js` at the KAS layout path.
fn make_bundle() -> Vec<u8> {
    let mut tar_buf = Vec::new();
    {
        let mut builder = tar::Builder::new(&mut tar_buf);
        let mut header = tar::Header::new_gnu();
        header.set_size(SERVER_BODY.len() as u64);
        header.set_mode(0o644);
        header.set_cksum();
        builder
            .append_data(&mut header, SERVER_REL, SERVER_BODY)
            .expect("append server.js");
        builder.finish().expect("finish tar");
    }
    let mut gz = Vec::new();
    let mut enc = flate2::write::GzEncoder::new(&mut gz, flate2::Compression::fast());
    enc.write_all(&tar_buf).expect("gzip tar");
    enc.finish().expect("finish gzip");
    gz
}

/// Find the `{version}` directory under `kas_root` that holds an extracted
/// `acp-server.js`, skipping scratch (`.incoming*`) and lock entries.
fn find_versioned_dir(kas_root: &Path) -> Option<PathBuf> {
    for entry in std::fs::read_dir(kas_root).ok()?.flatten() {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.starts_with(".incoming") || name.ends_with(".lock") {
            continue;
        }
        if entry.path().join(SERVER_REL).exists() {
            return Some(entry.path());
        }
    }
    None
}

fn run_chat_v3(home: &Path, data_dir: &Path, db_path: &Path, bundle: &Path, bogus_node: &Path) {
    // `chat --v3 --no-interactive` runs the startup GC in the parent, then
    // spawns the v3 engine `acp` child, which extracts the bundle and then
    // spawns node.
    //
    // Note - The bogus node path makes that spawn fail *after* extraction, so the
    // command exits non-zero with the known "failed to spawn the v3 engine: node binary"
    // error.
    Command::cargo_bin("chat_cli")
        .expect("locate chat_cli binary")
        .args(["chat", "--v3", "--no-interactive", "hi"])
        .env("HOME", home)
        .env("KIRO_TEST_MODE", "1")
        .env(KIRO_DATA_DIR, data_dir)
        .env(KIRO_TEST_DB_PATH, db_path)
        .env(KAS_BUNDLE_PATH, bundle)
        .env(KIRO_KAS_NODE_PATH, bogus_node)
        .timeout(Duration::from_secs(60))
        .assert()
        .failure()
        .stderr(contains("failed to spawn the v3 engine: node binary"));
}

#[test]
fn chat_v3_extracts_bundle_into_versioned_dir() {
    let home = tempfile::tempdir().expect("temp home");
    let data_dir = home.path().join("data");
    let db_path = home.path().join("data.sqlite3");
    let bundle = home.path().join("kas-bundle.tar.gz");
    std::fs::write(&bundle, make_bundle()).expect("write bundle");
    let bogus_node_exe = home.path().join("nonexistent-node");

    run_chat_v3(home.path(), &data_dir, &db_path, &bundle, &bogus_node_exe);

    let kas_root = data_dir.join("kas");
    let version_dir = find_versioned_dir(&kas_root).expect("bundle extracted into a versioned dir");
    assert_eq!(
        std::fs::read(version_dir.join(SERVER_REL)).expect("read extracted server.js"),
        SERVER_BODY
    );
    assert!(
        version_dir
            .file_name()
            .unwrap()
            .to_string_lossy()
            .starts_with(env!("CARGO_PKG_VERSION")),
        "versioned dir {version_dir:?} should be prefixed with the CLI version",
    );

    // Idempotent: a second run takes the fast path and must not nuke + rebuild
    // the versioned dir, so a sentinel placed inside survives.
    let sentinel = version_dir.join("SENTINEL");
    std::fs::write(&sentinel, b"keep").expect("write sentinel");
    run_chat_v3(home.path(), &data_dir, &db_path, &bundle, &bogus_node_exe);
    assert!(sentinel.exists(), "second run must take the fast path (no re-extract)");
}

/// Happy-path garbage collection through the real binary: a stale extracted
/// version (old heartbeat + on-disk dir) is reaped on the next launch, while
/// the current version survives. GC runs synchronously in the parent for
/// non-interactive launches, so it completes before the command returns.
#[test]
fn chat_v3_gc_reaps_stale_version_on_startup() {
    let home = tempfile::tempdir().expect("temp home");
    let data_dir = home.path().join("data");
    let db_path = home.path().join("data.sqlite3");
    let bundle = home.path().join("kas-bundle.tar.gz");
    std::fs::write(&bundle, make_bundle()).expect("write bundle");
    let bogus_node_exe = home.path().join("nonexistent-node");

    // First run bootstraps the DB (creating the extracted_kas_versions table) and extracts
    // + heartbeats the current version.
    run_chat_v3(home.path(), &data_dir, &db_path, &bundle, &bogus_node_exe);
    let kas_root = data_dir.join("kas");
    let current_dir = find_versioned_dir(&kas_root).expect("current version extracted");

    // Seed a stale version: a heartbeat row ~20 days old plus a matching dir.
    let stale_version = "9.9.9-staleversionhash";
    let stale_dir = kas_root.join(stale_version);
    std::fs::create_dir_all(&stale_dir).expect("create stale version dir");
    std::fs::write(stale_dir.join("marker"), b"stale").expect("write stale marker");
    let twenty_days_ago = now_ms() - 20 * 24 * 60 * 60 * 1000;
    {
        let conn = Connection::open(&db_path).expect("open sqlite db");
        conn.execute(
            "INSERT INTO extracted_kas_versions (version, last_used_at) VALUES (?1, ?2)",
            rusqlite::params![stale_version, twenty_days_ago],
        )
        .expect("seed stale extracted_kas_versions row");
    }
    assert!(stale_dir.exists(), "stale dir should exist before GC");

    // Second run: startup GC reaps the stale version before the command returns.
    run_chat_v3(home.path(), &data_dir, &db_path, &bundle, &bogus_node_exe);

    assert!(!stale_dir.exists(), "stale version dir should be reaped by GC");
    assert!(current_dir.exists(), "current version dir must survive GC");

    let conn = Connection::open(&db_path).expect("open sqlite db");
    let stale_rows: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM extracted_kas_versions WHERE version = ?1",
            rusqlite::params![stale_version],
            |row| row.get(0),
        )
        .expect("count stale rows");
    assert_eq!(stale_rows, 0, "stale heartbeat row should be pruned by GC");
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock before epoch")
        .as_millis() as i64
}
