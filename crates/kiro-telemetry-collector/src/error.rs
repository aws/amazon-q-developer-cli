//! Error taxonomy for the kiro-telemetry-collector crate.
//!
//! Variants enumerate every failure mode the lifecycle, binary resolution,
//! state-dir, config rendering, and health probe paths can produce. Callers
//! pattern-match on variants for diagnostic surfaces (e.g. `kiro telemetry
//! status`); the `Display` impls are user-facing and should stay actionable.

use std::path::PathBuf;

/// Errors produced by the collector lifecycle.
#[derive(Debug, thiserror::Error)]
pub enum CollectorError {
    /// All four binary-resolution layers (env override, PATH walk, bundled
    /// candidates) were exhausted without finding `otelcol-contrib`.
    #[error(
        "otelcol-contrib not found. Install via 'brew install opentelemetry-collector-contrib' \
         (macOS) or set KIRO_TELEMETRY_COLLECTOR_BIN=/path/to/otelcol-contrib. The bundled \
         installer (PR G) will provide this automatically in a future release. \
         Searched: {searched:?}"
    )]
    BinaryNotFound { searched: Vec<PathBuf> },

    /// `mkdir -p` failed for the state-dir tree.
    #[error("failed to create state dir at {path}: {source}")]
    StateDirCreate {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    /// fs4 try_lock_exclusive returned WouldBlock and our retry loop was
    /// exhausted; another kiro-cli invocation is currently inside
    /// `ensure_running`/`status`/`stop`.
    #[error("collector state lock at {path} is held by another process")]
    LockHeld { path: PathBuf },

    /// Non-WouldBlock io error opening or locking the lock file.
    #[error("failed to acquire collector state lock at {path}: {source}")]
    LockAcquire {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    /// Couldn't write rendered YAML or fingerprint file.
    #[error("failed to write {path}: {source}")]
    ConfigWrite {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    /// `std::process::Command::spawn` failed (binary not executable, ENOENT
    /// after our resolution check, etc.).
    #[error("failed to spawn collector binary {binary}: {source}")]
    SpawnFailed {
        binary: PathBuf,
        #[source]
        source: std::io::Error,
    },

    /// Collector spawned but didn't respond on :13133 within budget.
    #[error("collector health endpoint {endpoint} did not respond within {elapsed_ms}ms")]
    HealthProbeTimeout { endpoint: String, elapsed_ms: u64 },

    /// Collector responded but with non-2xx; surfaced from `health::probe`.
    #[error("collector health endpoint returned status {status}")]
    HealthEndpointBadStatus { status: u16 },

    /// Catch-all for unexpected io.
    #[error(transparent)]
    Io(#[from] std::io::Error),

    /// Catch-all for the health probe.
    #[error(transparent)]
    Reqwest(#[from] reqwest::Error),

    /// PID-record (de)serialization.
    #[error(transparent)]
    Json(#[from] serde_json::Error),
}
