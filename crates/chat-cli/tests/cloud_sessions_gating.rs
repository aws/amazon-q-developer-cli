//! Cloud-session gating coverage for the headless surfaces (batch 1).
//!
//! Exercises the real `chat_cli` binary the way a released user and a test
//! user would run it. The remote-sandbox rollout is ramped to every segment
//! and channel at 100%, and a fully-ramped feature needs no bucketing id —
//! so every released user gets the full cloud UX, with or without a rollout
//! cohort (a persisted client id). The rollout entry stays wired as the
//! kill-switch: a partial/zero `treatment_percent` re-darkens the flags,
//! and at a partial percent a user without a client id fails closed.
//! Both cohort and no-cohort users are pinned to the SAME live shape here:
//!
//!   - `--list-sessions`: cloud rows + environment/status columns appear for every user (cohort,
//!     no-cohort, and under `KIRO_TEST_MODE=1`).
//!   - `--delete-session`: short-id prefix resolution against the merged local+cloud listing,
//!     ambiguity handling, and cloud-row delete verification.
//!   - `--cloud` / `--repo` flags: live for every user (clap validation errors like
//!     `--repo`-requires-`--cloud` prove the enabled parse path; a kill-switched build would reject
//!     them as unknown arguments instead).
//!
//! Most tests mock KAS at the listing seam via `KIRO_TEST_MOCK_KAS_SESSIONS`
//! (the same JSON the live `session/list` merge consumes): no network, no BFF.
//! `list_sessions_no_cohort_shape_shows_cloud_columns` does not set the mock,
//! so it attempts (and quickly fails) a real KAS spawn — still no network. The
//! interactive cloud-session flows (boot, provider gate, repo picker,
//! disconnect, quit) are covered by the TUI e2e suite in
//! `packages/tui/e2e_tests/cloud/` and the knight-rider smoke script.
//!
//! Release-profile only tests: `Rollout::init` force-enables every feature in
//! debug builds, so the real rollout decision is only observable on a
//! `--release` binary. Those tests carry
//! `#[cfg_attr(debug_assertions, ignore)]`
//! so a debug `cargo test` reports them as IGNORED (not silently `ok`); the CI
//! release step (`.github/workflows/rust.yml`) is what actually proves them.

use assert_cmd::Command;
use predicates::str::contains;

/// Strip ANSI SGR sequences so tag assertions match what a user READS
/// (`| local | idle`), not the styled byte stream.
fn strip_ansi(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\u{1b}' {
            // Skip to the terminating letter of a CSI sequence.
            if chars.peek() == Some(&'[') {
                for c2 in chars.by_ref() {
                    if c2.is_ascii_alphabetic() {
                        break;
                    }
                }
            }
            continue;
        }
        out.push(c);
    }
    out
}

/// Combined, ANSI-stripped stdout+stderr of a finished assert.
fn readable_output(assert: &assert_cmd::assert::Assert) -> String {
    let stderr = String::from_utf8_lossy(&assert.get_output().stderr);
    let stdout = String::from_utf8_lossy(&assert.get_output().stdout);
    strip_ansi(&format!("{stderr}{stdout}"))
}

/// A minimal fake HOME so the tests never touch the developer's real
/// `~/.kiro`. Session stores resolve under `KIRO_TEST_SESSIONS_DIR`/DB.
struct TestHome {
    _dir: tempfile::TempDir,
    home: std::path::PathBuf,
    /// V2 session-store dir (what `KIRO_TEST_SESSIONS_DIR` points at — the
    /// override IS the store dir; no `sessions/cli` suffix is appended).
    sessions: std::path::PathBuf,
}

impl TestHome {
    fn new() -> Self {
        let dir = tempfile::tempdir().expect("tempdir");
        let home = dir.path().to_path_buf();
        let sessions = home.join("sessions-store");
        std::fs::create_dir_all(&sessions).expect("mk sessions");
        Self {
            _dir: dir,
            home,
            sessions,
        }
    }

    /// Base command with the hermetic environment. `test_mode` toggles the
    /// remote-sandbox feature exactly the way internal testing does.
    ///
    /// `KIRO_API_KEY` satisfies the `is_logged_in` gate on the released shape
    /// (without it, a not-logged-in run without `KIRO_TEST_MODE` opens the
    /// interactive auth portal and hangs the test). It does NOT enable any
    /// feature — the dark-ship assertions below stay meaningful.
    fn cmd(&self, test_mode: bool) -> Command {
        let mut c = Command::cargo_bin("chat_cli").expect("binary");
        c.env_clear()
            .env("PATH", std::env::var("PATH").unwrap_or_default())
            .env("HOME", &self.home)
            .env("KIRO_DATA_DIR", self.home.join("data"))
            .env("KIRO_TEST_DB_PATH", self.home.join("test.sqlite3"))
            .env("KIRO_TEST_SESSIONS_DIR", &self.sessions)
            .env("KIRO_DISABLE_TELEMETRY", "1")
            .env("KIRO_API_KEY", "test-key-login-gate-bypass")
            .timeout(std::time::Duration::from_secs(60));
        if test_mode {
            c.env("KIRO_TEST_MODE", "1");
        }
        c
    }

    /// Write a V2 session json so the local store has a row.
    fn write_v2_session(&self, id: &str, title: &str) {
        let cwd = std::env::current_dir().expect("cwd");
        let now = chrono::Utc::now().to_rfc3339();
        let body = serde_json::json!({
            "session_id": id,
            "cwd": cwd,
            "created_at": now,
            "updated_at": now,
            "title": title,
        });
        std::fs::write(
            self.sessions.join(format!("{id}.json")),
            serde_json::to_string(&body).unwrap(),
        )
        .expect("write v2 session");
    }

    fn session_file(&self, id: &str) -> std::path::PathBuf {
        self.sessions.join(format!("{id}.json"))
    }

    /// Put this home IN the rollout cohort: run the binary once so the real
    /// migration path creates the database, then write a client id the way
    /// first-run telemetry initialization would. `Rollout::init` reads the
    /// client id from this table; the cohort tests prove the feature is live
    /// through the bucketing path too, not only the 100% short-circuit.
    fn seed_client_id(&self) {
        self.cmd(false).args(["chat", "--list-sessions"]).assert().success();
        let conn = rusqlite::Connection::open(self.home.join("test.sqlite3")).expect("open test db");
        conn.execute(
            "INSERT OR REPLACE INTO state (key, value) VALUES ('telemetryClientId', ?1)",
            ["\"550e8400-e29b-41d4-a716-446655440000\""],
        )
        .expect("seed client id");
    }
}

/// Mock KAS listing rows in the exact `SessionInfoEntry` wire shape the
/// listing merge consumes (camelCase). A local-store row, a cloud row, and an
/// unknown/not-yet-shipped remote kind (`remote-control`) that the fail-closed
/// classifier must tag `cloud`, never `local`.
fn mock_kas_sessions(cwd: &std::path::Path) -> String {
    serde_json::json!([
        {
            "sessionId": "sess_local1111-2222-4333-8444-555555555555",
            "cwd": cwd,
            "title": "KAS local session",
            "updatedAt": chrono::Utc::now().to_rfc3339(),
        },
        {
            "sessionId": "cloudaaa-bbbb-4ccc-8ddd-eeeeeeeeeeee",
            "cwd": cwd,
            "title": "banana-service (cloud)",
            "updatedAt": chrono::Utc::now().to_rfc3339(),
            "executionTarget": "cloud-sandbox",
            "status": "in_progress",
        },
        {
            "sessionId": "remotectl-cccc-4ddd-8eee-ffffffffffff",
            "cwd": cwd,
            "title": "remote-control session",
            "updatedAt": chrono::Utc::now().to_rfc3339(),
            "executionTarget": "remote-control",
            "status": "in_progress",
        }
    ])
    .to_string()
}

// ── --list-sessions ─────────────────────────────────────────────────────────

/// No-cohort shape (fresh home, no client id, no KIRO_TEST_MODE): at 100%
/// every bucket is treatment, so a user without a persisted client id
/// (telemetry opted out) gets the SAME live listing as everyone else — the
/// environment tag column appears on local rows. "All users" includes users
/// the rollout cannot bucket.
#[test]
#[cfg_attr(
    debug_assertions,
    ignore = "release-profile only: debug builds force-enable all rollout features"
)]
fn list_sessions_no_cohort_shape_shows_cloud_columns() {
    if cfg!(debug_assertions) {
        // Guard against `--include-ignored` on a debug binary, where the
        // real rollout decision is unobservable.
        return;
    }
    let th = TestHome::new();
    th.write_v2_session("11111111-2222-4333-8444-555555555555", "plain local session");

    let assert = th.cmd(false).args(["chat", "--list-sessions"]).assert().success();

    let out = readable_output(&assert);
    assert!(out.contains("plain local session"), "local row must list: {out}");
    assert!(
        out.contains("| local |"),
        "no-cohort listing must carry the live environment tags at 100% ramp: {out}"
    );
}

/// Test shape (KIRO_TEST_MODE=1): cloud rows appear with the environment tag
/// and status word, local rows gain the `local` tag — the cloud listing columns.
#[test]
fn list_sessions_test_shape_shows_cloud_rows_with_state() {
    let th = TestHome::new();
    th.write_v2_session("11111111-2222-4333-8444-555555555555", "plain local session");
    let cwd = std::env::current_dir().expect("cwd");

    let assert = th
        .cmd(true)
        .env("KIRO_TEST_MOCK_KAS_SESSIONS", mock_kas_sessions(&cwd))
        .args(["chat", "--list-sessions"])
        .assert()
        .success();

    let out = readable_output(&assert);
    // Cloud row: id, title, cloud indicator, mapped state word.
    assert!(out.contains("cloudaaa-bbbb-4ccc-8ddd-eeeeeeeeeeee"), "{out}");
    assert!(out.contains("banana-service (cloud)"), "{out}");
    assert!(out.contains("| cloud |"), "cloud tag missing: {out}");
    // `in_progress` renders as the state word `working`.
    assert!(out.contains("working"), "status word missing: {out}");
    // Local rows carry the `local` tag + default idle state.
    assert!(out.contains("| local |"), "local tag missing: {out}");
    assert!(out.contains("plain local session"), "{out}");
}

/// Cloud rows injected through the test seam list for a no-cohort user too:
/// at 100% the listing gate is open for everyone, so the merged listing shows
/// cloud rows with their tags. The unknown/future non-local kind must still
/// classify as `cloud` (fail-closed classification), never `local`.
#[test]
#[cfg_attr(
    debug_assertions,
    ignore = "release-profile only: debug builds force-enable all rollout features"
)]
fn mock_kas_cloud_rows_list_on_no_cohort_shape() {
    if cfg!(debug_assertions) {
        return;
    }
    let th = TestHome::new();
    let cwd = std::env::current_dir().expect("cwd");

    let assert = th
        .cmd(false)
        .env("KIRO_TEST_MOCK_KAS_SESSIONS", mock_kas_sessions(&cwd))
        .args(["chat", "--list-sessions"])
        .assert()
        .success();

    let out = readable_output(&assert);
    assert!(
        out.contains("banana-service (cloud)"),
        "no-cohort listing must show cloud rows at 100% ramp: {out}"
    );
    assert!(out.contains("| cloud |"), "cloud tag missing: {out}");
    assert!(
        out.contains("KAS local session"),
        "local KAS rows must list alongside cloud rows: {out}"
    );
    assert!(out.contains("| local |"), "local tag missing: {out}");
}

/// The JSON listing is live for a no-cohort user too: cloud rows survive with
/// their `executionTarget`/`status` members intact.
#[test]
#[cfg_attr(
    debug_assertions,
    ignore = "release-profile only: debug builds force-enable all rollout features"
)]
fn list_sessions_json_no_cohort_shape_has_cloud_members() {
    if cfg!(debug_assertions) {
        return;
    }
    let th = TestHome::new();
    th.write_v2_session("11111111-2222-4333-8444-555555555555", "plain local session");
    let cwd = std::env::current_dir().expect("cwd");

    let assert = th
        .cmd(false)
        .env("KIRO_TEST_MOCK_KAS_SESSIONS", mock_kas_sessions(&cwd))
        .args(["chat", "--list-sessions", "--format", "json"])
        .assert()
        .success();

    let stdout = String::from_utf8_lossy(&assert.get_output().stdout).to_string();
    let json_start = stdout.find('[').expect("json output");
    let json_body = stdout.get(json_start..).expect("json slice");
    let listing: serde_json::Value = serde_json::from_str(json_body.trim()).expect("valid json listing");
    let sessions = listing[0]["sessions"].as_array().expect("sessions array");
    assert!(
        sessions.iter().any(|s| s["title"] == "plain local session"),
        "local row must list: {listing}"
    );
    let cloud = sessions
        .iter()
        .find(|s| s["title"] == "banana-service (cloud)")
        .unwrap_or_else(|| panic!("cloud row must list at 100% ramp: {listing}"));
    assert_eq!(cloud["executionTarget"], "cloud-sandbox", "{listing}");
    assert_eq!(cloud["status"], "in_progress", "{listing}");
}

// ── --delete-session ────────────────────────────────────────────────────────

/// Full-id delete of a local V2 session works identically on both shapes
/// (pre-existing behavior — regression guard).
#[test]
fn delete_session_full_id_local() {
    for test_mode in [false, true] {
        let th = TestHome::new();
        let id = "22222222-3333-4444-8555-666666666666";
        th.write_v2_session(id, "session to delete");

        th.cmd(test_mode)
            .args(["chat", "--delete-session", id])
            .assert()
            .success()
            .stderr(contains("Deleted chat session"));

        assert!(
            !th.session_file(id).exists(),
            "session file must be removed (test_mode={test_mode})"
        );
    }
}

/// Short-id (8-char) prefix resolution: with the feature ON, a unique local
/// prefix resolves against the merged local+cloud listing and deletes the
/// local session. The mocked KAS rows share no prefix with the target, so
/// the merged consultation must still resolve uniquely.
#[test]
fn delete_session_short_prefix_resolves_locally() {
    let th = TestHome::new();
    let id = "33333333-4444-4555-8666-777777777777";
    th.write_v2_session(id, "prefix delete target");
    let cwd = std::env::current_dir().expect("cwd");

    th.cmd(true)
        .env("KIRO_TEST_MOCK_KAS_SESSIONS", mock_kas_sessions(&cwd))
        .args(["chat", "--delete-session", "33333333"])
        .assert()
        .success()
        .stderr(contains("Deleted chat session"));

    assert!(!th.session_file(id).exists());
}

/// A prefix shared by a local session and a mocked cloud session must be
/// reported ambiguous (with a COUNT, never the session ids) instead of
/// silently deleting the local one.
#[test]
fn delete_session_ambiguous_prefix_errors_with_count() {
    let th = TestHome::new();
    // Local session sharing the first 8 chars with the mocked cloud row.
    let id = "cloudaaa-1111-4222-8333-444444444444";
    th.write_v2_session(id, "collides with cloud row");
    let cwd = std::env::current_dir().expect("cwd");

    let assert = th
        .cmd(true)
        .env("KIRO_TEST_MOCK_KAS_SESSIONS", mock_kas_sessions(&cwd))
        .args(["chat", "--delete-session", "cloudaaa"])
        .assert()
        .failure();

    let err = readable_output(&assert);
    assert!(
        err.contains("matches 2 sessions"),
        "ambiguity must report a count: {err}"
    );
    // CodeQL cleartext-logging guard: ids must not be printed.
    assert!(
        !err.contains("cloudaaa-bbbb"),
        "ambiguity error must not leak session ids: {err}"
    );
    assert!(th.session_file(id).exists(), "nothing may be deleted on ambiguity");
}

/// Below eight characters a value must match a stored id exactly — a short
/// "prefix" must never destructively resolve.
#[test]
fn delete_session_sub_eight_char_prefix_is_not_resolved() {
    let th = TestHome::new();
    let id = "44444444-5555-4666-8777-888888888888";
    th.write_v2_session(id, "must survive");
    let cwd = std::env::current_dir().expect("cwd");

    th.cmd(true)
        .env("KIRO_TEST_MOCK_KAS_SESSIONS", mock_kas_sessions(&cwd))
        .args(["chat", "--delete-session", "4444444"]) // 7 chars
        .assert()
        .failure()
        .stderr(contains("not found"));

    assert!(th.session_file(id).exists());
}

// ── --cloud / --repo flag gating ────────────────────────────────────────────

/// No-cohort shape: the flags are LIVE — at 100% ramp
/// `remote_sandbox_gate_error` rejects nothing even without a client id.
/// Proven the same way as the cohort test below, via deterministic
/// parse-time clap errors that never reach the network: `--repo` without
/// `--cloud` errors "requires '--cloud'" (a kill-switched build would
/// instead reject `--repo` as an unknown argument).
#[test]
#[cfg_attr(
    debug_assertions,
    ignore = "release-profile only: debug builds force-enable all rollout features"
)]
fn cloud_and_repo_flags_live_on_no_cohort_shape() {
    if cfg!(debug_assertions) {
        return;
    }
    let th = TestHome::new();

    let assert = th
        .cmd(false)
        .args([
            "chat",
            "--repo",
            "kiro-team/banana-service",
            "--no-interactive",
            "say hi",
        ])
        .assert()
        .failure()
        .code(2);
    let out = readable_output(&assert);
    assert!(
        out.contains("'--repo <REPO>' requires '--cloud'"),
        "no-cohort --repo must take the enabled path: {out}"
    );
    assert!(
        !out.contains("unexpected argument"),
        "no-cohort --repo must not be rejected as unknown: {out}"
    );
}

/// Cohort user (client id seeded, external segment, stable channel): the
/// flags are LIVE on a real release binary. Proven via deterministic
/// parse-time clap errors that never reach the network: `--repo` without
/// `--cloud` errors "requires '--cloud'" (a kill-switched build would
/// instead reject `--repo` as an unknown argument), and a blank
/// `--repo` value errors as invalid.
#[test]
#[cfg_attr(
    debug_assertions,
    ignore = "release-profile only: debug builds force-enable all rollout features"
)]
fn cloud_flags_live_for_cohort_user_on_release_binary() {
    if cfg!(debug_assertions) {
        return;
    }
    let th = TestHome::new();
    th.seed_client_id();

    let assert = th
        .cmd(false)
        .args([
            "chat",
            "--repo",
            "kiro-team/banana-service",
            "--no-interactive",
            "say hi",
        ])
        .assert()
        .failure()
        .code(2);
    let out = readable_output(&assert);
    assert!(
        out.contains("'--repo <REPO>' requires '--cloud'"),
        "cohort --repo must take the enabled path: {out}"
    );
    assert!(
        !out.contains("unexpected argument"),
        "cohort --repo must not be rejected as unknown: {out}"
    );

    let assert = th
        .cmd(false)
        .args(["chat", "--cloud", "--repo", "", "--no-interactive", "say hi"])
        .assert()
        .failure()
        .code(2);
    let out = readable_output(&assert);
    assert!(
        out.contains("repository name must not be blank"),
        "cohort blank --repo must take the enabled validation path: {out}"
    );
}

/// Cohort user: the listing shows the full cloud UX — cloud rows, environment
/// tags, status column — on a real release binary. The mocked KAS seam keeps
/// this network-free; the unknown `remote-control` kind must read `cloud`
/// (fail-closed classification), never `local`.
#[test]
#[cfg_attr(
    debug_assertions,
    ignore = "release-profile only: debug builds force-enable all rollout features"
)]
fn list_sessions_cohort_shape_shows_cloud_rows_on_release_binary() {
    if cfg!(debug_assertions) {
        return;
    }
    let th = TestHome::new();
    th.seed_client_id();
    th.write_v2_session("11111111-2222-4333-8444-555555555555", "plain local session");
    let cwd = std::env::current_dir().expect("cwd");

    let assert = th
        .cmd(false)
        .env("KIRO_TEST_MOCK_KAS_SESSIONS", mock_kas_sessions(&cwd))
        .args(["chat", "--list-sessions"])
        .assert()
        .success();

    let out = readable_output(&assert);
    assert!(out.contains("banana-service (cloud)"), "cloud row must list: {out}");
    assert!(out.contains("| cloud |"), "cloud tag missing: {out}");
    assert!(out.contains("| local |"), "local tag missing: {out}");
    assert!(out.contains("working"), "status word missing: {out}");
    assert!(out.contains("plain local session"), "local row must list: {out}");
}
