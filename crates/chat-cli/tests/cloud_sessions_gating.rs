//! Cloud-session gating coverage for the headless surfaces (batch 1).
//!
//! Exercises the real `chat_cli` binary the way a released user and a test
//! user would run it, asserting the dark-ship guarantee on the headless
//! cloud-session surfaces:
//!
//!   - `--list-sessions`: cloud rows + environment/status columns appear ONLY when the
//!     remote-sandbox feature is force-enabled (`KIRO_TEST_MODE=1`); a released-shape run is
//!     byte-format identical to prod (no `local`/`cloud` tags, no status column).
//!   - `--delete-session`: short-id prefix resolution against the merged local+cloud listing,
//!     ambiguity handling, and cloud-row delete verification are gated the same way.
//!   - `--cloud` / `--repo` flag gating: on a released build shape both flags are rejected as
//!     unknown arguments (exit 2), indistinguishable from a typo.
//!
//! Most tests mock KAS at the listing seam via `KIRO_TEST_MOCK_KAS_SESSIONS`
//! (the same JSON the live `session/list` merge consumes): no network, no BFF.
//! `list_sessions_released_shape_has_no_cloud_ux` does not set the mock, so it
//! attempts (and quickly fails) a real KAS spawn — still no network. The
//! interactive cloud-session flows (boot, provider gate, repo picker,
//! disconnect, quit) are covered by the TUI e2e suite in
//! `packages/tui/e2e_tests/cloud/` and the knight-rider smoke script.
//!
//! Release-profile only tests: `Rollout::init` force-enables every feature in
//! debug builds, so the released (feature-off) shape is only observable on a
//! `--release` binary. Those tests carry `#[cfg_attr(debug_assertions, ignore)]`
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
    /// remote-sandbox feature exactly the way nightly testing does.
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
}

/// Mock KAS listing rows in the exact `SessionInfoEntry` wire shape the
/// listing merge consumes (camelCase). A local-store row, a cloud row, and an
/// unknown/not-yet-shipped remote kind (`remote-control`) that the fail-closed
/// released gate must also hide.
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

/// Released shape (no KIRO_TEST_MODE): the listing must carry ZERO cloud UX —
/// no environment tags, no status column, no cloud rows — even when a stray
/// endpoint env var is present. This is the dark-ship guarantee for the one
/// headless surface a released user can hit.
#[test]
#[cfg_attr(
    debug_assertions,
    ignore = "release-profile only: debug builds force-enable all rollout features"
)]
fn list_sessions_released_shape_has_no_cloud_ux() {
    if cfg!(debug_assertions) {
        // Guard against `--include-ignored` on a debug binary, where the
        // released shape is unobservable.
        return;
    }
    let th = TestHome::new();
    th.write_v2_session("11111111-2222-4333-8444-555555555555", "plain local session");

    let assert = th
        .cmd(false)
        // A stray endpoint alone must NOT unlock anything.
        .env("KIRO_REMOTE_SESSIONS_ENDPOINT", "https://app.kiro.dev")
        .args(["chat", "--list-sessions"])
        .assert()
        .success();

    let out = readable_output(&assert);
    assert!(out.contains("plain local session"), "local row must list: {out}");
    // The cloud additions are the ` | local | idle` / ` | cloud | <status>`
    // tag pairs appended per row. None may appear on the released shape.
    for tag in ["| cloud |", "| local |", "cloud-sandbox"] {
        assert!(!out.contains(tag), "released listing leaked cloud UX ({tag}): {out}");
    }
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

/// Cloud rows injected through the test seam still obey the rollout gate.
/// Local KAS rows remain part of the normal merged listing.
#[test]
#[cfg_attr(
    debug_assertions,
    ignore = "release-profile only: debug builds force-enable all rollout features"
)]
fn mock_kas_cloud_rows_are_hidden_on_released_shape() {
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
        !out.contains("banana-service (cloud)"),
        "released listing must filter cloud KAS rows: {out}"
    );
    // The fail-closed gate must also hide an unknown/future non-local kind. An
    // exclude-list gate (`!= Some("cloud-sandbox")`) would let this row through
    // — this assertion is what distinguishes fail-closed from fail-open.
    assert!(
        !out.contains("remote-control session"),
        "released listing must filter unknown/future non-local kinds (fail-closed): {out}"
    );
    // Positive guard: an over-broad filter that dropped *all* KAS rows would
    // also satisfy the assertions above. The local KAS row must survive.
    assert!(
        out.contains("KAS local session"),
        "released listing must keep local KAS rows: {out}"
    );
}

/// The JSON listing obeys the same gate: on the released shape no cloud row
/// survives the filter and no per-row `executionTarget`/`status` members are
/// emitted (they are skip-if-None and only KAS cloud rows carry them here).
#[test]
#[cfg_attr(
    debug_assertions,
    ignore = "release-profile only: debug builds force-enable all rollout features"
)]
fn list_sessions_json_released_shape_has_no_cloud_members() {
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
    for s in sessions {
        assert!(
            s["executionTarget"].is_null() && s["status"].is_null(),
            "released JSON listing leaked cloud members: {s}"
        );
        assert_ne!(s["title"], "banana-service (cloud)", "cloud row leaked: {listing}");
        assert_ne!(
            s["title"], "remote-control session",
            "unknown/future non-local kind leaked into released JSON: {listing}"
        );
    }
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

/// On the released shape the dark-shipped flags are rejected exactly like a
/// typo: clap `UnknownArgument`, exit code 2, no cloud bring-up of any kind —
/// even with a stray endpoint env var present. This is the end-to-end proof
/// of `ChatArgs::remote_sandbox_gate_error` on a real release binary.
#[test]
#[cfg_attr(
    debug_assertions,
    ignore = "release-profile only: debug builds force-enable all rollout features"
)]
fn cloud_and_repo_flags_released_shape_rejected_as_unknown_arguments() {
    if cfg!(debug_assertions) {
        return;
    }
    let th = TestHome::new();
    let cases: [(&[&str], &str); 2] = [
        (&["chat", "--cloud", "--no-interactive", "say hi"], "--cloud"),
        (
            &[
                "chat",
                "--repo",
                "kiro-team/banana-service",
                "--no-interactive",
                "say hi",
            ],
            "--repo",
        ),
    ];
    for (args, flag) in cases {
        let assert = th
            .cmd(false)
            .env("KIRO_REMOTE_SESSIONS_ENDPOINT", "https://app.kiro.dev")
            .args(args)
            .assert()
            .failure()
            .code(2);

        let out = readable_output(&assert);
        assert!(
            out.contains(&format!("unexpected argument '{flag}' found")),
            "released {flag} must be rejected as unknown: {out}"
        );
        for leak in [
            "Cloud session created",
            "Creating cloud session",
            "cloud-sandbox",
            "repositories found",
        ] {
            assert!(!out.contains(leak), "released {flag} leaked cloud UX ({leak}): {out}");
        }
    }
}
