//! Orchestration: `ensure_running`, `status`, `stop`.
//!
//! All three public functions take the per-user advisory file lock first so
//! concurrent `kiro-cli` invocations serialize through this module rather than
//! racing on the PID/config files.
//!
//! `ensure_running` is the only spawn path. The child collector is detached
//! (orphaned via `setsid` on unix) so it lives across kiro-cli invocations;
//! we never `wait` on it. `stop` signals SIGTERM (then SIGKILL) gated on
//! start-identity to avoid killing a recycled PID.

use std::path::{
    Path,
    PathBuf,
};
use std::process::Stdio;
use std::time::{
    Duration,
    Instant,
};

use serde::Serialize;
use tracing::{
    debug,
    info,
    warn,
};

use crate::binary::resolve_collector_binary;
use crate::config::{
    ConfigInputs,
    fingerprint,
    render,
    write_artifacts,
};
use crate::error::CollectorError;
use crate::health::{
    probe,
    probe_with_client,
};
use crate::state::{
    PidRecord,
    StateDir,
    is_pid_alive,
    process_start_identity,
};

/// Default health endpoint. Re-exported from `lib.rs`.
pub const DEFAULT_HEALTH_ENDPOINT: &str = "http://127.0.0.1:13133/health";

/// OTLP endpoint the kiro-cli SDK targets and the spawned collector binds.
pub const DEFAULT_LOOPBACK_OTLP_ENDPOINT: &str = "http://127.0.0.1:14318";

/// How long `ensure_running` waits for the freshly spawned collector to
/// announce itself healthy on `:13133/health`.
const HEALTH_BUDGET: Duration = Duration::from_secs(15);

/// How long `ensure_running` waits when verifying that an *already-running*
/// collector is still healthy (much shorter — the process is already up).
const REUSE_HEALTH_BUDGET: Duration = Duration::from_millis(800);

/// SIGTERM-to-SIGKILL grace window in `stop`.
const STOP_GRACE: Duration = Duration::from_secs(5);

/// Result of [`ensure_running`].
#[derive(Debug, Clone, Serialize)]
pub struct CollectorHandle {
    pub pid: u32,
    pub otlp_endpoint: String,
    pub health_endpoint: String,
    pub fingerprint: String,
}

/// Output of [`status`].
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum CollectorStatus {
    Running {
        pid: u32,
        fingerprint: String,
        healthy: bool,
        upstream_endpoint: String,
    },
    NotRunning,
    Stale {
        reason: String,
    },
}

/// Per-user singleton: ensure a healthy `otelcol-contrib` is running with the
/// requested upstream endpoint, reusing an existing one when its config
/// fingerprint matches.
///
/// Idempotent — safe to call from every kiro-cli invocation. Returns once the
/// collector's `:13133/health` endpoint reports healthy. The detached
/// collector lives across kiro-cli invocations.
pub async fn ensure_running(state: &StateDir, upstream_endpoint: &str) -> Result<CollectorHandle, CollectorError> {
    let upstream = upstream_endpoint.to_string();
    let state_owned = state.clone();
    // Heavy lifting (lock acquisition + spawn) happens in a blocking task so
    // we don't hold the runtime hostage on the synchronous file/process APIs.
    let prepared = tokio::task::spawn_blocking(move || prepare_spawn(&state_owned, &upstream))
        .await
        .map_err(io_join_err)??;

    match prepared {
        PrepareOutcome::Reuse(handle) => {
            // Even on reuse, do a quick health check — if the collector is
            // mid-restart we want the caller to wait on it.
            let client = reqwest::Client::new();
            probe_with_client(&client, &handle.health_endpoint, REUSE_HEALTH_BUDGET).await?;
            Ok(handle)
        },
        PrepareOutcome::Spawned(handle) => {
            probe(&handle.health_endpoint, HEALTH_BUDGET).await?;
            Ok(handle)
        },
    }
}

/// Diagnostic read-only view used by `kiro telemetry status`.
pub async fn status(state: &StateDir) -> Result<CollectorStatus, CollectorError> {
    let state_owned = state.clone();
    let observation = tokio::task::spawn_blocking(move || observe_state(&state_owned))
        .await
        .map_err(io_join_err)??;

    match observation {
        Observation::NotRunning => Ok(CollectorStatus::NotRunning),
        Observation::Stale { reason } => Ok(CollectorStatus::Stale { reason }),
        Observation::Candidate { record, fp } => {
            let client = reqwest::Client::new();
            let healthy = probe_with_client(&client, DEFAULT_HEALTH_ENDPOINT, REUSE_HEALTH_BUDGET)
                .await
                .is_ok();
            Ok(CollectorStatus::Running {
                pid: record.pid,
                fingerprint: fp,
                healthy,
                upstream_endpoint: record.upstream_endpoint,
            })
        },
    }
}

/// Cooperative shutdown: SIGTERM → wait → SIGKILL fallback → cleanup state.
/// Verifies start-identity before signaling so we never kill a recycled PID.
/// No-op if the collector is already not running.
pub async fn stop(state: &StateDir) -> Result<(), CollectorError> {
    let state_owned = state.clone();
    tokio::task::spawn_blocking(move || stop_blocking(&state_owned))
        .await
        .map_err(io_join_err)?
}

// ---------------------------------------------------------------------------
// Blocking implementation (runs on `spawn_blocking`).
// ---------------------------------------------------------------------------

enum PrepareOutcome {
    Reuse(CollectorHandle),
    Spawned(CollectorHandle),
}

fn prepare_spawn(state: &StateDir, upstream_endpoint: &str) -> Result<PrepareOutcome, CollectorError> {
    let _lock = state.acquire_lock()?;
    state.ensure_dirs()?;

    let binary = resolve_collector_binary()?;
    let queue_dir = state.queue_dir();
    let crashes_dir = state.crashes_dir();
    let inputs = ConfigInputs {
        upstream_endpoint,
        queue_dir: &queue_dir,
        crashes_dir: &crashes_dir,
        binary_path: &binary,
    };
    let rendered = render(&inputs);
    let new_fp = fingerprint(&rendered, &binary);

    // Reuse path?
    if let Some(existing) = state.read_pid_record()? {
        let fp_matches = read_existing_fingerprint(state).is_some_and(|s| s == new_fp);
        let alive = is_pid_alive(existing.pid);
        let identity_matches = existing.start_identity == 0
            || process_start_identity(existing.pid)
                .map(|i| i == existing.start_identity)
                .unwrap_or(false);
        if alive && identity_matches && fp_matches && existing.upstream_endpoint == upstream_endpoint {
            debug!(pid = existing.pid, "reusing existing collector");
            return Ok(PrepareOutcome::Reuse(CollectorHandle {
                pid: existing.pid,
                otlp_endpoint: DEFAULT_LOOPBACK_OTLP_ENDPOINT.to_string(),
                health_endpoint: DEFAULT_HEALTH_ENDPOINT.to_string(),
                fingerprint: new_fp,
            }));
        }
        warn!(
            pid = existing.pid,
            alive, identity_matches, fp_matches, "existing collector record is stale; cleaning up before respawn"
        );
        // Try to be a good citizen and SIGTERM the stale process so we don't
        // strand a collector with the wrong config bound to :14318.
        if alive && identity_matches {
            let _ = signal_terminate(existing.pid);
        }
        state.cleanup_stale()?;
    }

    // Persist config + fingerprint, spawn, persist PID record.
    write_artifacts(&state.config_path(), &state.fingerprint_path(), &rendered, &new_fp)?;
    let pid = spawn_collector(&binary, &state.config_path(), state)?;
    let record = PidRecord::new(pid, upstream_endpoint.to_string(), binary.clone());
    state.write_pid_record(&record)?;
    info!(pid, "spawned otelcol-contrib");

    Ok(PrepareOutcome::Spawned(CollectorHandle {
        pid,
        otlp_endpoint: DEFAULT_LOOPBACK_OTLP_ENDPOINT.to_string(),
        health_endpoint: DEFAULT_HEALTH_ENDPOINT.to_string(),
        fingerprint: new_fp,
    }))
}

enum Observation {
    NotRunning,
    Stale { reason: String },
    Candidate { record: PidRecord, fp: String },
}

fn observe_state(state: &StateDir) -> Result<Observation, CollectorError> {
    let _lock = state.acquire_lock()?;
    let Some(record) = state.read_pid_record()? else {
        return Ok(Observation::NotRunning);
    };
    if !is_pid_alive(record.pid) {
        return Ok(Observation::Stale {
            reason: format!("pid {} is not alive", record.pid),
        });
    }
    if record.start_identity != 0
        && process_start_identity(record.pid)
            .map(|i| i != record.start_identity)
            .unwrap_or(true)
    {
        return Ok(Observation::Stale {
            reason: format!("pid {} start-identity does not match recorded value", record.pid),
        });
    }
    let fp = read_existing_fingerprint(state).unwrap_or_default();
    Ok(Observation::Candidate { record, fp })
}

fn stop_blocking(state: &StateDir) -> Result<(), CollectorError> {
    let _lock = state.acquire_lock()?;
    let Some(record) = state.read_pid_record()? else {
        debug!("stop: no pid record, nothing to do");
        return Ok(());
    };

    if !is_pid_alive(record.pid) {
        debug!(pid = record.pid, "stop: pid not alive; cleaning state only");
        state.cleanup_stale()?;
        return Ok(());
    }

    // Gate on start-identity so we never SIGTERM a recycled PID.
    if record.start_identity != 0 {
        match process_start_identity(record.pid) {
            Ok(now) if now != record.start_identity => {
                warn!(
                    pid = record.pid,
                    "stop: start-identity mismatch (PID recycled); refusing to signal, cleaning state"
                );
                state.cleanup_stale()?;
                return Ok(());
            },
            Ok(_) => {},
            Err(e) => {
                warn!(error = %e, pid = record.pid, "stop: could not read start-identity; proceeding cautiously");
            },
        }
    }

    let _ = signal_terminate(record.pid);

    let deadline = Instant::now() + STOP_GRACE;
    while Instant::now() < deadline {
        if !is_pid_alive(record.pid) {
            break;
        }
        std::thread::sleep(Duration::from_millis(100));
    }

    if is_pid_alive(record.pid) {
        warn!(pid = record.pid, "SIGTERM grace expired; sending SIGKILL");
        let _ = signal_kill(record.pid);
    }

    state.cleanup_stale()?;
    Ok(())
}

fn read_existing_fingerprint(state: &StateDir) -> Option<String> {
    std::fs::read_to_string(state.fingerprint_path())
        .ok()
        .map(|s| s.trim().to_string())
}

// ---------------------------------------------------------------------------
// Spawn + signal helpers.
// ---------------------------------------------------------------------------

fn spawn_collector(bin: &Path, config_path: &Path, state: &StateDir) -> Result<u32, CollectorError> {
    use std::process::Command;
    // Truncate stdout/stderr logs each spawn so we don't grow forever.
    let stdout = std::fs::File::create(state.stdout_log_path()).map_err(|source| CollectorError::ConfigWrite {
        path: state.stdout_log_path(),
        source,
    })?;
    let stderr = std::fs::File::create(state.stderr_log_path()).map_err(|source| CollectorError::ConfigWrite {
        path: state.stderr_log_path(),
        source,
    })?;

    let mut cmd = Command::new(bin);
    cmd.arg(format!("--config={}", config_path.display()))
        .stdin(Stdio::null())
        .stdout(Stdio::from(stdout))
        .stderr(Stdio::from(stderr));

    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        // SAFETY: setsid is async-signal-safe and has no preconditions on the
        // parent state. We use this to detach from the kiro-cli session so
        // closing our terminal doesn't propagate SIGHUP to the collector.
        unsafe {
            cmd.pre_exec(|| {
                if libc::setsid() == -1 {
                    return Err(std::io::Error::last_os_error());
                }
                Ok(())
            });
        }
    }

    let child = cmd.spawn().map_err(|source| CollectorError::SpawnFailed {
        binary: PathBuf::from(bin),
        source,
    })?;
    let pid = child.id();
    // Intentionally leak the Child handle — the collector outlives this kiro-cli
    // invocation. Boxing + leaking avoids the `mem::forget`-with-Drop-fields
    // clippy lint while having the same effect: the OS still owns the
    // process, and we'll track it via the PID file from now on.
    Box::leak(Box::new(child));
    Ok(pid)
}

#[cfg(unix)]
fn signal_terminate(pid: u32) -> std::io::Result<()> {
    use nix::sys::signal::{
        Signal,
        kill,
    };
    use nix::unistd::Pid;
    kill(Pid::from_raw(pid as i32), Signal::SIGTERM).map_err(|e| std::io::Error::other(e.to_string()))
}

#[cfg(unix)]
fn signal_kill(pid: u32) -> std::io::Result<()> {
    use nix::sys::signal::{
        Signal,
        kill,
    };
    use nix::unistd::Pid;
    kill(Pid::from_raw(pid as i32), Signal::SIGKILL).map_err(|e| std::io::Error::other(e.to_string()))
}

#[cfg(not(unix))]
fn signal_terminate(_pid: u32) -> std::io::Result<()> {
    // Windows MVP: not implemented. Returning Ok(()) lets the cleanup path
    // continue; callers will see the PID is still alive and skip cleanup.
    Ok(())
}

#[cfg(not(unix))]
fn signal_kill(_pid: u32) -> std::io::Result<()> {
    Ok(())
}

fn io_join_err(e: tokio::task::JoinError) -> CollectorError {
    CollectorError::Io(std::io::Error::other(format!("blocking task join failed: {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn status_reports_not_running_for_empty_state() {
        let td = tempfile::tempdir().expect("tempdir");
        let s = StateDir::with_root(td.path().to_path_buf());
        let st = status(&s).await.expect("status ok");
        assert!(matches!(st, CollectorStatus::NotRunning), "got: {st:?}");
    }

    #[tokio::test]
    async fn stop_is_no_op_when_not_running() {
        let td = tempfile::tempdir().expect("tempdir");
        let s = StateDir::with_root(td.path().to_path_buf());
        stop(&s).await.expect("stop ok");
    }

    #[tokio::test]
    async fn status_reports_stale_when_pid_dead() {
        let td = tempfile::tempdir().expect("tempdir");
        let s = StateDir::with_root(td.path().to_path_buf());
        s.ensure_dirs().expect("ensure_dirs");
        let bogus = PidRecord {
            pid: 2_147_483_646,
            start_identity: 0,
            spawned_at_unix_ms: 0,
            upstream_endpoint: "https://nope".into(),
            binary_path: PathBuf::from("/usr/bin/true"),
        };
        s.write_pid_record(&bogus).expect("write");
        let st = status(&s).await.expect("status ok");
        assert!(matches!(st, CollectorStatus::Stale { .. }), "got: {st:?}");
    }
}
