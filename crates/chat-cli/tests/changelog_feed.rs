//! End-to-end remote changelog feed behavior via the real `chat_cli` binary.
//!
//! Spawns `kiro-cli version --changelog=all` with KIRO_VERSION_OVERRIDE
//! selecting the channel, KIRO_DATA_DIR isolating the cache, and
//! KIRO_FEED_URL pointing at a local server hosting a fixture feed.

use assert_cmd::Command;
use chat_cli::util::consts::env_var::{
    KIRO_BUNDLED_FEED_FILE,
    KIRO_FEED_URL,
    KIRO_NO_REMOTE_CHANGELOG,
    KIRO_VERSION_OVERRIDE,
};
use predicates::prelude::*;
use predicates::str::contains;

const FIXTURE_FEED: &str = r#"{
    "entries": [
        {
            "type": "release",
            "date": "2026-02-01",
            "version": "2.0.0",
            "changes": [{ "type": "added", "description": "Future entry beyond the binary version" }]
        },
        {
            "type": "release",
            "date": "2026-01-01",
            "version": "1.0.0",
            "changes": [{ "type": "added", "description": "Remote fixture entry" }]
        }
    ]
}"#;

/// `version --changelog=all` against the given env; returns the command for
/// assertion chaining.
fn changelog_cmd(version: &str, feed_url: &str, data_dir: &std::path::Path) -> Command {
    let mut cmd = Command::cargo_bin("chat_cli").unwrap();
    cmd.args(["version", "--changelog=all"])
        .env(KIRO_VERSION_OVERRIDE, version)
        .env(KIRO_FEED_URL, feed_url)
        .env("KIRO_DATA_DIR", data_dir)
        .env("KIRO_TEST_DB_PATH", data_dir.join("test.sqlite3"))
        .env("HOME", data_dir)
        .timeout(std::time::Duration::from_secs(30));
    cmd
}

#[test]
fn nightly_fetches_remote_feed_and_caps_at_binary_version() {
    let data_dir = tempfile::tempdir().unwrap();
    let mut server = mockito::Server::new();
    let mock = server
        .mock("GET", "/feed.json")
        .with_status(200)
        .with_body(FIXTURE_FEED)
        .create();

    // 1.5.0-nightly.1: sees the 1.0.0 entry, but not 2.0.0 (version cap).
    changelog_cmd(
        "1.5.0-nightly.1",
        &format!("{}/feed.json", server.url()),
        data_dir.path(),
    )
    .assert()
    .success()
    .stdout(contains("Remote fixture entry").and(contains("Future entry").not()));
    mock.assert();
}

#[test]
fn non_nightly_channels_never_fetch() {
    let data_dir = tempfile::tempdir().unwrap();
    let mut server = mockito::Server::new();
    let mock = server.mock("GET", "/feed.json").expect(0).create();

    for version in ["1.5.0", "1.5.0-rc.1", "1.5.0-fix-foo.1"] {
        changelog_cmd(version, &format!("{}/feed.json", server.url()), data_dir.path())
            .assert()
            .success()
            .stdout(contains("Remote fixture entry").not());
    }
    // No request ever hit the server.
    mock.assert();
}

#[test]
fn kill_switch_disables_fetch_on_nightly() {
    let data_dir = tempfile::tempdir().unwrap();
    let mut server = mockito::Server::new();
    let mock = server.mock("GET", "/feed.json").expect(0).create();

    changelog_cmd(
        "1.5.0-nightly.1",
        &format!("{}/feed.json", server.url()),
        data_dir.path(),
    )
    .env(KIRO_NO_REMOTE_CHANGELOG, "1")
    .assert()
    .success()
    .stdout(contains("Remote fixture entry").not());
    mock.assert();
}

#[test]
fn redirects_are_not_followed() {
    let data_dir = tempfile::tempdir().unwrap();
    let mut server = mockito::Server::new();
    let redirect = server
        .mock("GET", "/feed.json")
        .with_status(302)
        .with_header("location", &format!("{}/elsewhere.json", server.url()))
        .create();
    let target = server.mock("GET", "/elsewhere.json").expect(0).create();

    changelog_cmd(
        "1.5.0-nightly.1",
        &format!("{}/feed.json", server.url()),
        data_dir.path(),
    )
    .assert()
    .success()
    .stdout(contains("Remote fixture entry").not());
    redirect.assert();
    target.assert();
}

#[test]
fn failed_fetch_falls_back_to_cache_then_bundled() {
    let data_dir = tempfile::tempdir().unwrap();
    let mut server = mockito::Server::new();
    let url = format!("{}/feed.json", server.url());

    // Bundled-only baseline: server errors and there is no cache yet.
    let error_mock = server.mock("GET", "/feed.json").with_status(500).create();
    changelog_cmd("1.5.0-nightly.1", &url, data_dir.path())
        .assert()
        .success()
        .stdout(contains("Remote fixture entry").not());
    error_mock.assert();

    // Seed the cache with a successful fetch.
    let ok_mock = server
        .mock("GET", "/feed.json")
        .with_status(200)
        .with_body(FIXTURE_FEED)
        .create();
    changelog_cmd("1.5.0-nightly.1", &url, data_dir.path())
        .assert()
        .success()
        .stdout(contains("Remote fixture entry"));
    ok_mock.assert();

    // Server errors again: the cache serves the previously fetched feed.
    let error_mock = server.mock("GET", "/feed.json").with_status(500).create();
    changelog_cmd("1.5.0-nightly.1", &url, data_dir.path())
        .assert()
        .success()
        .stdout(contains("Remote fixture entry"));
    error_mock.assert();
}

/// Bundled feed newer than the cache (post-upgrade) wins; a newer cache wins.
/// Uses KIRO_BUNDLED_FEED_FILE so both sides are explicit fixtures rather
/// than the compile-time feed.
#[test]
fn embedded_floor_prefers_newer_of_cache_and_bundled() {
    let data_dir = tempfile::tempdir().unwrap();
    let mut server = mockito::Server::new();
    let url = format!("{}/feed.json", server.url());

    let bundled = data_dir.path().join("bundled.json");
    std::fs::write(
        &bundled,
        r#"{ "entries": [ { "type": "release", "date": "2026-06-01", "version": "3.0.0",
            "changes": [{ "type": "added", "description": "Bundled floor entry" }] } ] }"#,
    )
    .unwrap();

    let floor_cmd = |version: &str| {
        let mut cmd = changelog_cmd(version, &url, data_dir.path());
        cmd.env(KIRO_BUNDLED_FEED_FILE, &bundled);
        cmd
    };

    // Cache an older feed (2.0.0), then take the server offline.
    let seed = r#"{ "entries": [ { "type": "release", "date": "2026-02-01", "version": "2.0.0",
        "changes": [{ "type": "added", "description": "Old cached entry" }] } ] }"#;
    let ok = server
        .mock("GET", "/feed.json")
        .with_status(200)
        .with_body(seed)
        .create();
    floor_cmd("9.9.9-nightly.1").assert().success();
    ok.assert();

    // Offline: cache 2.0.0 < bundled 3.0.0 -> bundled floor entry served.
    let err = server.mock("GET", "/feed.json").with_status(500).create();
    floor_cmd("9.9.9-nightly.1")
        .assert()
        .success()
        .stdout(contains("Bundled floor entry").and(contains("Old cached entry").not()));
    err.assert();

    // Now cache a newer feed (4.0.0); offline: cache >= bundled -> cache served.
    let newer = r#"{ "entries": [ { "type": "release", "date": "2026-07-01", "version": "4.0.0",
        "changes": [{ "type": "added", "description": "New cached entry" }] } ] }"#;
    let ok = server
        .mock("GET", "/feed.json")
        .with_status(200)
        .with_body(newer)
        .create();
    floor_cmd("9.9.9-nightly.1").assert().success();
    ok.assert();
    let err = server.mock("GET", "/feed.json").with_status(500).create();
    floor_cmd("9.9.9-nightly.1")
        .assert()
        .success()
        .stdout(contains("New cached entry").and(contains("Bundled floor entry").not()));
    err.assert();
}
