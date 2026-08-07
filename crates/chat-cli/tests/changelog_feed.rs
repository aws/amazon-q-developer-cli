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
    KIRO_ROLLOUT_FORCE_INTERNAL,
    KIRO_VERSION_OVERRIDE,
};
use predicates::prelude::*;
use predicates::str::contains;

/// `chat _ refresh-feed` does a blocking fetch and snapshots the processed
/// feed to the file the TUI reads; non-nightly channels report updated=false
/// and write nothing.
#[test]
fn refresh_feed_subcommand_snapshots_fresh_feed() {
    let data_dir = tempfile::tempdir().unwrap();
    let mut server = mockito::Server::new();
    let url = format!("{}/feed.json", server.url());
    let feed_file = data_dir.path().join("feed.json");

    let refresh_cmd = |version: &str| {
        let mut cmd = Command::cargo_bin("chat_cli").unwrap();
        cmd.args(["chat", "_", "refresh-feed"])
            .env(KIRO_VERSION_OVERRIDE, version)
            .env(KIRO_FEED_URL, &url)
            .env("KIRO_DATA_DIR", data_dir.path())
            .env("KIRO_TEST_DB_PATH", data_dir.path().join("test.sqlite3"))
            .env("HOME", data_dir.path())
            .timeout(std::time::Duration::from_secs(30));
        cmd
    };

    // Kill switch: no fetch, updated=false, no snapshot written. The switch
    // returns before any channel or rollout check, so this is profile- and
    // gate-independent.
    let never = server.mock("GET", "/feed.json").expect(0).create();
    refresh_cmd("9.9.9")
        .env(KIRO_NO_REMOTE_CHANGELOG, "1")
        .assert()
        .success()
        .stdout(contains(r#""updated":false"#));
    never.assert();
    assert!(!feed_file.exists());

    // Nightly: fetch succeeds, snapshot written with the fixture content.
    let ok = server
        .mock("GET", "/feed.json")
        .with_status(200)
        .with_body(FIXTURE_FEED)
        .create();
    refresh_cmd("9.9.9-nightly.1")
        .assert()
        .success()
        .stdout(contains(r#""updated":true"#));
    ok.assert();
    let snapshot = std::fs::read_to_string(&feed_file).unwrap();
    assert!(snapshot.contains("Remote fixture entry"));

    // Unchanged content on a re-fetch reports updated=false (no re-render).
    let ok = server
        .mock("GET", "/feed.json")
        .with_status(200)
        .with_body(FIXTURE_FEED)
        .create();
    refresh_cmd("9.9.9-nightly.1")
        .assert()
        .success()
        .stdout(contains(r#""updated":false"#));
    ok.assert();

    // Embedded floor applies to the snapshot: with a bundled fixture newer
    // than the fetched feed, the snapshot carries the bundled content, never
    // the raw fetch body.
    let bundled = data_dir.path().join("bundled.json");
    std::fs::write(
        &bundled,
        r#"{ "entries": [ { "type": "release", "date": "2026-06-01", "version": "5.0.0",
            "changes": [{ "type": "added", "description": "Bundled floor entry" }] } ] }"#,
    )
    .unwrap();
    let ok = server
        .mock("GET", "/feed.json")
        .with_status(200)
        .with_body(FIXTURE_FEED)
        .create();
    refresh_cmd("9.9.9-nightly.1")
        .env(KIRO_BUNDLED_FEED_FILE, &bundled)
        .assert()
        .success()
        .stdout(contains(r#""updated":true"#));
    ok.assert();
    let snapshot = std::fs::read_to_string(&feed_file).unwrap();
    assert!(snapshot.contains("Bundled floor entry"));
    assert!(!snapshot.contains("Remote fixture entry"));

    // Nightly with the server failing: updated=false, snapshot untouched.
    let err = server.mock("GET", "/feed.json").with_status(500).create();
    refresh_cmd("9.9.9-nightly.1")
        .assert()
        .success()
        .stdout(contains(r#""updated":false"#));
    err.assert();
    assert!(
        std::fs::read_to_string(&feed_file)
            .unwrap()
            .contains("Bundled floor entry")
    );
}

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
        // Scrub inherited host env (tests may run inside a Kiro session) so
        // the sandbox is deterministic: the client-id and telemetry vars
        // change whether a bucketing id gets persisted, and the last two
        // would force gates on and mask a dark binary.
        .env_remove("KIRO_TELEMETRY_CLIENT_ID")
        .env_remove("Q_TELEMETRY_CLIENT_ID")
        .env_remove("KIRO_DISABLE_TELEMETRY")
        .env_remove("Q_DISABLE_TELEMETRY")
        .env_remove("KIRO_TEST_MODE")
        .env_remove(KIRO_ROLLOUT_FORCE_INTERNAL)
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

/// rc and feature builds ship to gamma only, so they read the gamma feed the
/// same way nightly does (the URL is redirected to a fixture here).
#[test]
fn rc_and_feature_channels_fetch() {
    for version in ["1.5.0-rc.1", "1.5.0-fix-foo.1"] {
        let data_dir = tempfile::tempdir().unwrap();
        let mut server = mockito::Server::new();
        let mock = server
            .mock("GET", "/feed.json")
            .with_status(200)
            .with_body(FIXTURE_FEED)
            .create();

        changelog_cmd(version, &format!("{}/feed.json", server.url()), data_dir.path())
            .assert()
            .success()
            .stdout(contains("Remote fixture entry"));
        mock.assert();
    }
}

/// GA pin: an EXTERNAL stable user (no internal sign-in, no force-internal
/// override) fetches the feed, because rollout.json ships remote_changelog
/// at segment all / 100%. Release-profile only so the assertion goes through
/// the real rollout config rather than the debug enable-all shortcut. The
/// per-user dark lever remains the KIRO_NO_REMOTE_CHANGELOG kill switch;
/// fleet-wide dark is treatment_percent 0 in a follow-up release.
#[test]
#[cfg_attr(debug_assertions, ignore = "real rollout gate only exists in release builds")]
fn stable_fetches_at_ga_without_internal_signin() {
    let data_dir = tempfile::tempdir().unwrap();
    let mut server = mockito::Server::new();
    let mock = server
        .mock("GET", "/feed.json")
        .with_status(200)
        .with_body(FIXTURE_FEED)
        .create();

    changelog_cmd("1.5.0", &format!("{}/feed.json", server.url()), data_dir.path())
        .assert()
        .success()
        .stdout(contains("Remote fixture entry"));
    mock.assert();
}

/// A telemetry-opted-out stable user also fetches at GA. Telemetry-disabled
/// runs never persist a client id, and fully ramped features must not
/// require one — otherwise opting out of telemetry would silently opt users
/// out of GA features too. Release-profile only, same reason as above.
#[test]
#[cfg_attr(debug_assertions, ignore = "real rollout gate only exists in release builds")]
fn stable_fetches_at_ga_with_telemetry_disabled() {
    let data_dir = tempfile::tempdir().unwrap();
    let mut server = mockito::Server::new();
    let mock = server
        .mock("GET", "/feed.json")
        .with_status(200)
        .with_body(FIXTURE_FEED)
        .create();

    changelog_cmd("1.5.0", &format!("{}/feed.json", server.url()), data_dir.path())
        .env("KIRO_DISABLE_TELEMETRY", "1")
        .assert()
        .success()
        .stdout(contains("Remote fixture entry"));
    mock.assert();
}

/// A gated stable build that cannot reach the feed still renders the embedded
/// copy. This matters more on stable than nightly: stable binaries are built
/// from release branches, whose bundled feed carries real entries, so the
/// worst case is last-release content rather than an empty changelog.
/// Release-profile only, same reason as stable_fetches_when_rollout_enabled.
#[test]
#[cfg_attr(debug_assertions, ignore = "real rollout gate only exists in release builds")]
fn stable_falls_back_to_bundled_then_cache() {
    let data_dir = tempfile::tempdir().unwrap();
    let mut server = mockito::Server::new();
    let url = format!("{}/feed.json", server.url());

    let bundled = data_dir.path().join("bundled.json");
    std::fs::write(
        &bundled,
        r#"{ "entries": [ { "type": "release", "date": "2026-05-01", "version": "0.9.0",
            "changes": [{ "type": "added", "description": "Embedded stable entry" }] } ] }"#,
    )
    .unwrap();

    let stable_cmd = || {
        let mut cmd = changelog_cmd("1.5.0", &url, data_dir.path());
        cmd.env(KIRO_ROLLOUT_FORCE_INTERNAL, "1")
            .env(KIRO_BUNDLED_FEED_FILE, &bundled);
        cmd
    };

    // Fetch fails with no cache yet -> embedded copy is served.
    let err = server.mock("GET", "/feed.json").with_status(500).create();
    stable_cmd()
        .assert()
        .success()
        .stdout(contains("Embedded stable entry").and(contains("Remote fixture entry").not()));
    err.assert();

    // Seed the cache from a successful fetch.
    let ok = server
        .mock("GET", "/feed.json")
        .with_status(200)
        .with_body(FIXTURE_FEED)
        .create();
    stable_cmd().assert().success().stdout(contains("Remote fixture entry"));
    ok.assert();

    // Fetch fails again -> the cache is preferred over the embedded copy
    // (cache 1.0.0 outranks bundled 0.9.0 under the embedded floor).
    let err = server.mock("GET", "/feed.json").with_status(500).create();
    stable_cmd().assert().success().stdout(contains("Remote fixture entry"));
    err.assert();
}

/// Local dev builds stay hermetic: no fetch even with a feed URL configured.
#[test]
fn dev_builds_never_fetch() {
    let data_dir = tempfile::tempdir().unwrap();
    let mut server = mockito::Server::new();
    let mock = server.mock("GET", "/feed.json").expect(0).create();

    changelog_cmd("99.99.99-dev", &format!("{}/feed.json", server.url()), data_dir.path())
        .assert()
        .success()
        .stdout(contains("Remote fixture entry").not());
    mock.assert();
}

/// The kill switch stops the fetch before any channel or gate check.
#[test]
fn stable_does_not_fetch_when_kill_switch_set() {
    let data_dir = tempfile::tempdir().unwrap();
    let mut server = mockito::Server::new();
    let mock = server.mock("GET", "/feed.json").expect(0).create();

    changelog_cmd("1.5.0", &format!("{}/feed.json", server.url()), data_dir.path())
        .env(KIRO_NO_REMOTE_CHANGELOG, "1")
        .assert()
        .success()
        .stdout(contains("Remote fixture entry").not());
    mock.assert();
}

/// Stable builds fetch and render once the remote_changelog rollout gate is
/// on. Release-profile only: debug builds short-circuit Rollout::init to
/// enable-all before KIRO_ROLLOUT_FORCE_INTERNAL is read, so only a
/// --release binary exercises the real gate (the env var satisfies the
/// `segment: internal` requirement; rollout.json's channel and percentage
/// still decide).
#[test]
#[cfg_attr(debug_assertions, ignore = "real rollout gate only exists in release builds")]
fn stable_fetches_when_rollout_enabled() {
    let data_dir = tempfile::tempdir().unwrap();
    let mut server = mockito::Server::new();
    let mock = server
        .mock("GET", "/feed.json")
        .with_status(200)
        .with_body(FIXTURE_FEED)
        .create();

    changelog_cmd("1.5.0", &format!("{}/feed.json", server.url()), data_dir.path())
        .env(KIRO_ROLLOUT_FORCE_INTERNAL, "1")
        .assert()
        .success()
        .stdout(contains("Remote fixture entry"));
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

    // Bundled-only baseline: server errors and there is no cache yet. The
    // bundle is placeholder-only; its hidden 0.0.0 entry must never print.
    let error_mock = server.mock("GET", "/feed.json").with_status(500).create();
    changelog_cmd("1.5.0-nightly.1", &url, data_dir.path())
        .assert()
        .success()
        .stdout(
            contains("No changelog information available")
                .and(contains("Remote fixture entry").not())
                .and(contains("0.0.0").not()),
        );
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

/// A newer rolling feed must not erase the last cache this client can render.
#[test]
fn newer_only_remote_feed_preserves_compatible_cache() {
    let data_dir = tempfile::tempdir().unwrap();
    let mut server = mockito::Server::new();
    let url = format!("{}/feed.json", server.url());
    let cmd = || {
        let mut cmd = changelog_cmd("2.17.0", &url, data_dir.path());
        cmd.env(KIRO_ROLLOUT_FORCE_INTERNAL, "1");
        cmd
    };

    let compatible = r#"{ "entries": [
        { "type": "release", "date": "2026-01-17", "version": "2.17.0",
          "changes": [{ "type": "added", "description": "Client-compatible cached entry" }] },
        { "type": "release", "date": "2026-01-16", "version": "2.16.0", "changes": [] }
    ] }"#;
    let seed = server
        .mock("GET", "/feed.json")
        .with_status(200)
        .with_body(compatible)
        .create();
    cmd()
        .assert()
        .success()
        .stdout(contains("Client-compatible cached entry"));
    seed.assert();

    let newer_only = r#"{ "entries": [
        { "type": "release", "date": "2026-01-19", "version": "2.19.0", "changes": [] },
        { "type": "release", "date": "2026-01-18", "version": "2.18.0", "changes": [] }
    ] }"#;
    let latest = server
        .mock("GET", "/feed.json")
        .with_status(200)
        .with_body(newer_only)
        .create();
    cmd()
        .assert()
        .success()
        .stdout(contains("Client-compatible cached entry"));
    latest.assert();

    let offline = server.mock("GET", "/feed.json").with_status(500).create();
    cmd()
        .assert()
        .success()
        .stdout(contains("Client-compatible cached entry"));
    offline.assert();
}

/// Version capping to zero visible entries stays safe on fetch and cache fallback.
#[test]
fn binary_older_than_all_remote_entries_degrades_to_empty() {
    let data_dir = tempfile::tempdir().unwrap();
    let mut server = mockito::Server::new();
    let url = format!("{}/feed.json", server.url());

    let empty_output = || {
        contains("No changelog information available")
            .and(contains("Remote fixture entry").not())
            .and(contains("Future entry").not())
            .and(contains("0.0.0").not())
    };

    // Both successful fetches must reapply the current binary's cap.
    let ok = server
        .mock("GET", "/feed.json")
        .with_status(200)
        .with_body(FIXTURE_FEED)
        .expect(2)
        .create();
    changelog_cmd("0.5.0-nightly.1", &url, data_dir.path())
        .assert()
        .success()
        .stdout(empty_output());
    changelog_cmd("0.5.0-nightly.1", &url, data_dir.path())
        .assert()
        .success()
        .stdout(empty_output());
    ok.assert();

    // A capped-to-empty cache safely falls back to the placeholder-only bundle.
    let err = server.mock("GET", "/feed.json").with_status(500).create();
    changelog_cmd("0.5.0-nightly.1", &url, data_dir.path())
        .assert()
        .success()
        .stdout(empty_output());
    err.assert();
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
    // The blocking remote path applies the bundled floor immediately while
    // still caching the older response for later comparisons.
    floor_cmd("9.9.9-nightly.1")
        .assert()
        .success()
        .stdout(contains("Bundled floor entry").and(contains("Old cached entry").not()));
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
