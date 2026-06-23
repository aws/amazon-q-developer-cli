//! End-to-end coverage for the KAS version heartbeat.
//!
//! Spawns the real `chat_cli` binary running the hidden `chat _ get-kas-token`
//! callback (the path the embedded KAS server invokes for auth) and asserts it
//! records a `extracted_kas_versions` heartbeat row for the running KAS bundle.
//!
//! The heartbeat is auth-independent: even when no user is logged in (so token
//! resolution fails), the running KAS version must be recorded so the version's
//! on-disk bundle is protected from garbage collection. All sandboxing is done
//! via child-process environment on the `Command`; the test process never
//! mutates its own environment.

use std::time::{
    SystemTime,
    UNIX_EPOCH,
};

use assert_cmd::Command;
use chat_cli::util::consts::env_var::{
    KAS_BUNDLE_PATH,
    KIRO_TEST_DB_PATH,
};
use rusqlite::Connection;

#[test]
fn get_kas_token_records_version_heartbeat() {
    let home = tempfile::tempdir().expect("create temp home");
    let db_path = home.path().join("data.sqlite3");
    // `kas_version()` derives the hash from the bundle bytes; any non-empty file
    // works as a stand-in bundle for the heartbeat (no extraction happens here).
    let bundle_path = home.path().join("kas-bundle.tar.gz");
    std::fs::write(&bundle_path, b"stub-kas-bundle-bytes").expect("write stub bundle");

    let before_ms = now_ms();

    // Not logged in, so token resolution fails (non-zero exit) - but the
    // heartbeat must still be recorded.
    Command::cargo_bin("chat_cli")
        .expect("locate chat_cli binary")
        .args(["chat", "_", "get-kas-token"])
        .env("HOME", home.path())
        .env(KIRO_TEST_DB_PATH, &db_path)
        .env(KAS_BUNDLE_PATH, &bundle_path)
        .assert()
        .failure();

    let after_ms = now_ms();

    let conn = Connection::open(&db_path).expect("open sqlite db");
    let rows: Vec<(String, i64)> = conn
        .prepare("SELECT version, last_used_at FROM extracted_kas_versions")
        .expect("extracted_kas_versions table should exist")
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
        .expect("query extracted_kas_versions")
        .collect::<Result<_, _>>()
        .expect("collect extracted_kas_versions rows");

    assert_eq!(rows.len(), 1, "expected exactly one heartbeat row, got {rows:?}");
    let (version, last_used_at) = &rows[0];
    assert!(
        version.starts_with(env!("CARGO_PKG_VERSION")),
        "version {version:?} should be prefixed with the CLI version {:?}",
        env!("CARGO_PKG_VERSION")
    );
    assert!(
        *last_used_at >= before_ms && *last_used_at <= after_ms,
        "last_used_at {last_used_at} should be a recent epoch-millis timestamp in [{before_ms}, {after_ms}]"
    );
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock before epoch")
        .as_millis() as i64
}
