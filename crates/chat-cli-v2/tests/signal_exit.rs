//! Integration tests: verify the ACP subprocess exits cleanly on signals and pipe close.
//!
//! These tests cover the "dead spiral" scenarios from PRs #1611, #1761, #1804:
//! - SIGHUP: terminal tab/window closed (Cmd+W)
//! - SIGTERM: graceful kill (kill <pid>)
//! - Stdin pipe close: parent process died (SIGKILL / hard death)
//!
//! The full process tree is: Rust launcher → bun TUI → ACP backend (chat_cli_v2 acp).
//! These tests exercise the ACP backend layer directly. The Rust→bun layer is tested
//! by scripts/test-orphan-fix.sh (requires a built binary + bun).
//!
//! Taskei: https://taskei.amazon.dev/tasks/P420105490

#[cfg(unix)]
mod common;

#[cfg(unix)]
mod tests {
    use std::time::{
        Duration,
        Instant,
    };

    use nix::sys::signal::{
        Signal,
        kill,
    };
    use nix::unistd::Pid;
    use ntest::timeout;
    use serial_test::serial;

    use crate::common::AcpTestHarnessBuilder;

    /// Helper: send a signal and assert exit within the deadline.
    async fn assert_exits_on_signal(harness: &mut crate::common::AcpTestHarness, signal: Signal, deadline_secs: u64) {
        let pid = harness.child.id().expect("child should have a pid") as i32;

        kill(Pid::from_raw(pid), signal).expect("failed to send signal");

        let start = Instant::now();
        let status = tokio::time::timeout(Duration::from_secs(deadline_secs), harness.child.wait())
            .await
            .expect("process did not exit within deadline")
            .expect("failed to wait on child");

        let elapsed = start.elapsed();
        eprintln!(
            "[{:?}] Process exited in {:?} with status: {:?}",
            signal, elapsed, status
        );
        assert!(
            elapsed < Duration::from_secs(deadline_secs),
            "exit took too long: {:?}",
            elapsed
        );
    }

    // ─── SIGHUP (terminal close / Cmd+W) ───────────────────────────────────────

    /// SIGHUP without active session.
    #[tokio::test]
    #[timeout(10000)]
    #[serial]
    async fn sighup_exits_cleanly() {
        let (mut harness, _client) = AcpTestHarnessBuilder::new("sighup_exits_cleanly")
            .with_trust_all(true)
            .build()
            .await;

        assert_exits_on_signal(&mut harness, Signal::SIGHUP, 5).await;
    }

    /// SIGHUP with an active session — ensures session cleanup doesn't hang.
    #[tokio::test]
    #[timeout(15000)]
    #[serial]
    async fn sighup_exits_cleanly_with_active_session() {
        let (mut harness, _client, _session_id, _cwd) = AcpTestHarnessBuilder::new("sighup_exits_session")
            .with_trust_all(true)
            .build_with_session()
            .await;

        assert_exits_on_signal(&mut harness, Signal::SIGHUP, 5).await;
    }

    // ─── SIGTERM (graceful kill) ────────────────────────────────────────────────

    /// SIGTERM without active session.
    #[tokio::test]
    #[timeout(10000)]
    #[serial]
    async fn sigterm_exits_cleanly() {
        let (mut harness, _client) = AcpTestHarnessBuilder::new("sigterm_exits_cleanly")
            .with_trust_all(true)
            .build()
            .await;

        assert_exits_on_signal(&mut harness, Signal::SIGTERM, 5).await;
    }

    /// SIGTERM with an active session.
    #[tokio::test]
    #[timeout(15000)]
    #[serial]
    async fn sigterm_exits_cleanly_with_active_session() {
        let (mut harness, _client, _session_id, _cwd) = AcpTestHarnessBuilder::new("sigterm_exits_session")
            .with_trust_all(true)
            .build_with_session()
            .await;

        assert_exits_on_signal(&mut harness, Signal::SIGTERM, 5).await;
    }

    // ─── Stdin pipe close (parent death / SIGKILL) ──────────────────────────────

    /// Dropping the ACP client closes the stdin pipe to the subprocess,
    /// simulating the parent process dying (SIGKILL scenario).
    /// The ACP subprocess detects stdin EOF and shuts down gracefully.
    #[tokio::test]
    #[timeout(10000)]
    #[serial]
    async fn stdin_close_exits_cleanly() {
        let (mut harness, client) = AcpTestHarnessBuilder::new("stdin_close_exits")
            .with_trust_all(true)
            .build()
            .await;

        // Drop the ACP client — this closes the stdin pipe to the subprocess
        drop(client);

        let start = Instant::now();
        let status = tokio::time::timeout(Duration::from_secs(5), harness.child.wait())
            .await
            .expect("process did not exit within 5 seconds after stdin close")
            .expect("failed to wait on child");

        let elapsed = start.elapsed();
        eprintln!(
            "[stdin close] Process exited in {:?} with status: {:?}",
            elapsed, status
        );
        assert!(elapsed < Duration::from_secs(5), "exit took too long: {:?}", elapsed);
    }

    /// Stdin close with an active session.
    #[tokio::test]
    #[timeout(15000)]
    #[serial]
    async fn stdin_close_exits_cleanly_with_active_session() {
        let (mut harness, client, _session_id, _cwd) = AcpTestHarnessBuilder::new("stdin_close_exits_session")
            .with_trust_all(true)
            .build_with_session()
            .await;

        // Drop the ACP client — this closes the stdin pipe to the subprocess
        drop(client);

        let start = Instant::now();
        let status = tokio::time::timeout(Duration::from_secs(5), harness.child.wait())
            .await
            .expect("process did not exit within 5 seconds after stdin close")
            .expect("failed to wait on child");

        let elapsed = start.elapsed();
        eprintln!(
            "[stdin close + session] Process exited in {:?} with status: {:?}",
            elapsed, status
        );
        assert!(elapsed < Duration::from_secs(5), "exit took too long: {:?}", elapsed);
    }
}
