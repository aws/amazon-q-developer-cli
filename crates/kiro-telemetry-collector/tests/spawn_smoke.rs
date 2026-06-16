//! End-to-end spawn test against a real `otelcol-contrib` binary if available,
//! skipped with an `eprintln!` if not. Exercised in the workflow's e2e phase
//! after `brew install opentelemetry-collector-contrib`.

use std::time::Duration;

use kiro_telemetry_collector::{
    CollectorStatus,
    StateDir,
    ensure_running,
    status,
    stop,
};

/// Returns true when a collector binary is reachable via env override or PATH.
fn collector_binary_available() -> bool {
    if std::env::var_os("KIRO_TELEMETRY_COLLECTOR_BIN").is_some() {
        return true;
    }
    let Some(path) = std::env::var_os("PATH") else {
        return false;
    };
    for dir in std::env::split_paths(&path) {
        if dir.join("otelcol-contrib").is_file() {
            return true;
        }
    }
    false
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn ensure_running_spawns_then_reuses() {
    if !collector_binary_available() {
        eprintln!(
            "skipping: otelcol-contrib not available (set KIRO_TELEMETRY_COLLECTOR_BIN or `brew install opentelemetry-collector-contrib`)"
        );
        return;
    }

    let td = tempfile::tempdir().expect("tempdir");
    let state = StateDir::with_root(td.path().to_path_buf());
    let upstream = "http://127.0.0.1:9999/v1/metrics".to_string();

    // First call spawns.
    let h1 = ensure_running(&state, &upstream).await.expect("first ensure_running");
    assert!(h1.pid > 0);
    assert_eq!(h1.otlp_endpoint, "http://127.0.0.1:14318");

    // Second call reuses.
    let h2 = ensure_running(&state, &upstream).await.expect("second ensure_running");
    assert_eq!(h1.pid, h2.pid, "second call should reuse the same PID");
    assert_eq!(h1.fingerprint, h2.fingerprint);

    // Status reports running.
    let st = status(&state).await.expect("status");
    match st {
        CollectorStatus::Running { pid, healthy, .. } => {
            assert_eq!(pid, h1.pid);
            assert!(healthy, "collector should be healthy");
        },
        other => panic!("expected Running, got: {other:?}"),
    }

    // Stop kills it.
    stop(&state).await.expect("stop");

    // Give the OS a moment to reap the process.
    tokio::time::sleep(Duration::from_millis(200)).await;

    let st2 = status(&state).await.expect("status after stop");
    assert!(matches!(st2, CollectorStatus::NotRunning), "got: {st2:?}");
}
